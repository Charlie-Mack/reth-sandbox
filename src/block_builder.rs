use std::sync::Arc;

use alloy_consensus::{BlockHeader, EthereumTxEnvelope, Transaction, TxEip4844};
use alloy_primitives::{Address, B256};
use alloy_rlp::Encodable;

use reth_chain_state::ExecutedBlock;
use reth_chainspec::ChainSpec;
use reth_db::DatabaseEnv;
use reth_ethereum::EthPrimitives;
use reth_evm::{
    ConfigureEvm, NextBlockEnvAttributes,
    execute::{BlockBuilder, BlockBuilderOutcome},
};
use reth_node_api::NodeTypesWithDBAdapter;
use reth_node_ethereum::{EthEvmConfig, EthereumNode};
use reth_primitives_traits::{Recovered, SealedHeader};
use reth_provider::{ExecutionOutcome, ProviderFactory, StateProvider};
use reth_revm::{State, database::StateProviderDatabase};
use tokio::sync::mpsc::Receiver;
use tracing::{info, warn};

use crate::block_writer::BlockFileHeader;
use crate::block_writer::BlockFileWriter;

type PF = ProviderFactory<NodeTypesWithDBAdapter<EthereumNode, Arc<DatabaseEnv>>>;

pub struct SandboxBlockBuilder {
    provider_factory: PF,
    parent_header: SealedHeader,
    parent_timestamp: u64,
    gas_limit: u64,
    evm_config: EthEvmConfig,
    receiver: Receiver<Recovered<EthereumTxEnvelope<TxEip4844>>>,
    block_writer: BlockFileWriter,
}

impl SandboxBlockBuilder {
    pub fn new(
        provider_factory: PF,
        chain: Arc<ChainSpec>,
        receiver: Receiver<Recovered<EthereumTxEnvelope<TxEip4844>>>,
    ) -> Self {
        let output_path = std::env::current_dir().unwrap().join("blocks.bin");

        let mut block_writer =
            BlockFileWriter::new(&output_path, BlockFileHeader::new(false, 0, 100)).unwrap();

        let evm_config = EthEvmConfig::new(chain.clone());

        let gas_limit = chain.genesis().gas_limit;

        let genesis_timestamp = chain.genesis_header().timestamp;

        let genesis_header =
            SealedHeader::new(chain.genesis_header().clone(), chain.genesis_hash().into());

        Self {
            provider_factory,
            parent_header: genesis_header,
            parent_timestamp: genesis_timestamp,
            gas_limit,
            evm_config,
            receiver,
            block_writer,
        }
    }

    pub fn finish_file_writer(mut self) -> eyre::Result<()> {
        self.block_writer.finish()?;
        Ok(())
    }

    async fn finish_block_and_commit(
        &mut self,
        outcome: BlockBuilderOutcome<EthPrimitives>,
        mut state_db: State<StateProviderDatabase<&Box<dyn StateProvider>>>,
    ) -> eyre::Result<()> {
        let bundle_state = state_db.take_bundle();

        self.parent_header = outcome.block.sealed_header().clone();
        self.parent_timestamp = outcome.block.sealed_header().timestamp;

        let block = outcome.block.clone().into_block();

        let execution_output = Arc::new(ExecutionOutcome {
            bundle: bundle_state,
            receipts: vec![outcome.execution_result.receipts],
            first_block: outcome.block.header().number(),
            requests: vec![outcome.execution_result.requests],
        });

        let executed_block: ExecutedBlock = ExecutedBlock {
            recovered_block: Arc::new(outcome.block),
            execution_output,
            hashed_state: Arc::new(outcome.hashed_state),
            trie_updates: Arc::new(outcome.trie_updates),
        };

        let mut buf = Vec::with_capacity(block.length());
        block.encode(&mut buf);

        self.block_writer.write_block(&buf)?;

        let provider_rw = self.provider_factory.provider_rw()?;
        provider_rw.save_blocks(vec![executed_block])?;
        provider_rw.commit()?;

        Ok(())
    }

    pub async fn start_building(&mut self) -> eyre::Result<()> {
        'block: loop {
            let state_provider = self.provider_factory.latest()?;
            let state = StateProviderDatabase::new(&state_provider);
            let mut state_db: State<StateProviderDatabase<&Box<dyn StateProvider>>> =
                State::builder()
                    .with_database(state)
                    .with_bundle_update()
                    .build();

            let parent_header = self.parent_header.clone();

            let mut builder = self
                .evm_config
                .builder_for_next_block(
                    &mut state_db,
                    &parent_header,
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

            let mut cumulative_gas_used = 0;
            let mut tx_count = 0;

            // leave 20% in the block
            let max_gas_for_block = self.gas_limit * 80 / 100;

            builder.apply_pre_execution_changes().map_err(|err| {
                warn!(target: "sandbox", %err, "failed to apply pre-execution changes");
                err
            })?;

            while let Some(tx) = self.receiver.recv().await {
                // info!("Executing transaction: {:?}", tx);

                let tx_nonce = tx.nonce();

                // info!("Executing transaction for nonce: {}", tx_nonce);

                let gas_used = builder
                    .execute_transaction_with_result_closure(tx, |res| {
                        // info!(target: "sandbox", "transaction result: {:?}", res);
                    })
                    .map_err(|err| {
                        warn!(target: "sandbox", %err, "failed to execute transaction");
                        err
                    })?;

                cumulative_gas_used += gas_used;
                tx_count += 1;

                if cumulative_gas_used >= max_gas_for_block {
                    //finish the block
                    //commit to the db
                    //call build next block

                    //Last transaction in the block
                    info!(
                        "The last transaction in the block is for nonce: {}",
                        tx_nonce,
                    );

                    let outcome = builder.finish(&state_provider).map_err(|err| {
                        warn!(target: "sandbox", %err, "failed to finish building block");
                        err
                    })?;

                    self.finish_block_and_commit(outcome, state_db).await?;

                    continue 'block;
                }
            }

            let outcome = builder.finish(&state_provider).map_err(|err| {
                warn!(target: "sandbox", %err, "failed to finish building block");
                err
            })?;

            self.finish_block_and_commit(outcome, state_db).await?;

            return Ok(());
        }
    }
}
