use alloy_consensus::{Block, EthereumTxEnvelope, Transaction, TxEip4844, TxEip4844Variant};
use alloy_primitives::{Address, B256, address, hex};
use alloy_signer_local::PrivateKeySigner;
use k256::ecdsa::SigningKey;
use rayon::prelude::*;
use reth_ethereum::TransactionSigned;
use reth_evm::{
    ConfigureEvm, NextBlockEnvAttributes, RecoveredTx,
    execute::{BlockBuilder, BlockBuilderOutcome, ExecutorTx},
};
use reth_node_ethereum::EthEvmConfig;
use reth_primitives_traits::{Recovered, SealedHeader, SignedTransaction};
use reth_provider::StateProvider;
use reth_revm::{State, database::StateProviderDatabase};
use tracing::{info, warn};

use crate::{
    actors::{Actor, ActorPool},
    transaction::{DEFAULT_GAS_LIMIT, TransactionOperations},
};

use crate::metrics::{self, time_async_section};
use crate::time_block_section;

type StateDB<'a> = State<StateProviderDatabase<&'a Box<dyn StateProvider>>>;

pub struct SandboxBlockBuilder<'a> {
    state_provider: &'a Box<dyn StateProvider>,
    parent_header: SealedHeader,
    parent_timestamp: u64,
    gas_limit: u64,
    state_db: StateDB<'a>,
    evm_config: EthEvmConfig,
}

impl<'a> SandboxBlockBuilder<'a> {
    pub fn new(
        state_provider: &'a Box<dyn StateProvider>,
        parent_header: SealedHeader,
        parent_timestamp: u64,
        gas_limit: u64,
        evm_config: EthEvmConfig,
    ) -> Self {
        let state = StateProviderDatabase::new(state_provider);

        let mut state_db: State<StateProviderDatabase<&Box<dyn StateProvider>>> = State::builder()
            .with_database(state)
            .with_bundle_update()
            .build();

        Self {
            state_provider,
            parent_header,
            parent_timestamp,
            gas_limit,
            state_db,
            evm_config,
        }
    }

    pub async fn build_next_block(
        &mut self,
        block_number: u64,
        actor_pool: &mut ActorPool,
    ) -> eyre::Result<Block<EthereumTxEnvelope<TxEip4844>>> {
        let mut builder = self
            .evm_config
            .builder_for_next_block(
                &mut self.state_db,
                &self.parent_header,
                NextBlockEnvAttributes {
                    timestamp: self.parent_timestamp + 1,
                    suggested_fee_recipient: Address::ZERO,
                    prev_randao: B256::ZERO,
                    gas_limit: self.gas_limit,
                    parent_beacon_block_root: None,
                    withdrawals: None,
                },
            )
            .map_err(|err| {
                warn!(target: "sandbox", %err, "failed to create a builder");
                err
            })?;
        {
            let _t = time_block_section!(block_number, "apply_pre_execution_changes");
            builder.apply_pre_execution_changes().map_err(|err| {
                warn!(target: "sandbox", %err, "failed to apply pre-execution changes");
                err
            })?;
        }

        let mut cumulative_gas_used = 0;
        let mut tx_count = 0;

        //Work out how many transactions we can fit in the block
        let max_transactions = self.gas_limit / DEFAULT_GAS_LIMIT;
        info!("Max transactions for gas limit: {}", max_transactions);

        let (g_wallet, g_nonce) = actor_pool.genesis_actor_info();

        let new_actors = {
            let _t = time_block_section!(block_number, "generate_new_actors");
            (0..max_transactions)
                .into_par_iter()
                .map(|_| Actor::new())
                .collect::<Vec<Actor>>()
        };

        let new_addresses = new_actors
            .iter()
            .map(|actor| actor.address())
            .collect::<Vec<Address>>();

        {
            let _t = time_block_section!(block_number, "add_new_actors_to_pool");
            for actor in new_actors {
                actor_pool.add_actor_instance(actor);
            }
        }

        let start_nonce = actor_pool.genesis_actor_nonce();

        // generate transactions
        let txs = {
            let _t = time_block_section!(block_number, "generate_transactions");

            (0..max_transactions)
                .into_par_iter()
                .map(|i| {
                    TransactionOperations::transfer_tx(
                        2600,
                        &g_wallet,
                        new_addresses[i as usize],
                        start_nonce + i as u64,
                    )
                })
                .collect::<Vec<Recovered<EthereumTxEnvelope<TxEip4844>>>>()
        };

        for tx in txs {
            let gas_used = {
                let _t = time_block_section!(block_number, "execute_transaction");
                let gas_used = builder.execute_transaction(tx).map_err(|err| {
                    warn!(target: "sandbox", %err, "failed to execute transaction");
                    err
                })?;
                gas_used
            };

            actor_pool.increment_genesis_actor_nonce();
            tx_count += 1;
            cumulative_gas_used += gas_used;
        }

        let BlockBuilderOutcome {
            execution_result,
            block,
            ..
        } = {
            let _t = time_block_section!(block_number, "finish_building_block");
            let outcome = builder.finish(&self.state_provider).map_err(|err| {
                warn!(target: "sandbox", %err, "failed to finish building block");
                err
            })?;
            outcome
        };

        self.parent_header = block.sealed_header().clone();
        self.parent_timestamp = block.sealed_header().timestamp;

        let block = block.into_block();

        info!("Block built with {} transactions", tx_count);

        Ok(block)
    }
}
