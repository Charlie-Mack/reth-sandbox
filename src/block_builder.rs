//! Builds executed blocks from streamed transactions and persists them to disk.

use std::sync::Arc;

use alloy_consensus::BlockHeader;
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
use reth_primitives_traits::SealedHeader;
use reth_provider::{ExecutionOutcome, ProviderFactory, StateProvider};
use reth_revm::{State, database::StateProviderDatabase};
use tokio::sync::mpsc::Receiver;
use tracing::{debug, info, warn};

use crate::{block_writer::BlockFileHeader, config::SimulationConfig};
use crate::{block_writer::BlockFileWriter, orchestrator::TX};

/// Concrete provider factory type used throughout the builder.
type PF = ProviderFactory<NodeTypesWithDBAdapter<EthereumNode, Arc<DatabaseEnv>>>;

/// Consumes recovered transactions, executes them with Reth's block builder, and
/// writes both RLP bytes and state updates to disk.
pub struct SandboxBlockBuilder {
    provider_factory: PF,
    parent_header: SealedHeader,
    parent_timestamp: u64,
    gas_limit: u64,
    evm_config: EthEvmConfig,
    receiver: Receiver<TX>,
    block_writer: BlockFileWriter,
    simulation_config: SimulationConfig,
}

impl SandboxBlockBuilder {
    /// Prepare the builder with the genesis header and file writer output path.
    pub fn new(
        provider_factory: PF,
        chain: Arc<ChainSpec>,
        receiver: Receiver<TX>,
        simulation_config: SimulationConfig,
    ) -> Self {
        let output_path = std::env::current_dir().unwrap().join("blocks.bin");

        let block_writer =
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
            simulation_config,
        }
    }

    /// Flush any buffered block bytes and close the backing file handle.
    pub fn finish_file_writer(self) -> eyre::Result<()> {
        self.block_writer.finish()?;
        Ok(())
    }

    /// Persist the executed block both to the binary file and the Reth database.
    async fn finish_block_and_commit(
        &mut self,
        outcome: BlockBuilderOutcome<EthPrimitives>,
        mut state_db: State<StateProviderDatabase<&Box<dyn StateProvider>>>,
    ) -> eyre::Result<()> {
        let bundle_state = state_db.take_bundle();

        self.parent_header = outcome.block.sealed_header().clone();
        self.parent_timestamp = outcome.block.sealed_header().timestamp;

        let block = outcome.block.clone().into_block();
        let block_number = outcome.block.header().number();
        let txs_in_block = outcome.block.body().transactions.len();

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
        debug!(
            target: "sandbox::block_builder",
            block = block_number,
            txs = txs_in_block,
            "wrote block bytes to file"
        );

        let provider_rw = self.provider_factory.provider_rw()?;
        provider_rw.save_blocks(vec![executed_block])?;
        provider_rw.commit()?;
        info!(
            target: "sandbox::block_builder",
            block = block_number,
            txs = txs_in_block,
            "persisted executed block to database"
        );

        Ok(())
    }

    /// Pull transactions from the orchestrator, keep building blocks until the gas budget is
    /// exhausted,
    pub async fn start_building(&mut self) -> eyre::Result<()> {
        let mut total_tx_count = 0;
        let mut total_gas_used = 0;
        let mut total_blocks_built = 0;

        let gas_limit = self.gas_limit;
        // Keep at 50% so the base fee doesnt change
        let max_gas_for_block = gas_limit * 50 / 100;

        'block_building: loop {
            if self
                .simulation_config
                .limits_hit(total_blocks_built, total_tx_count, total_gas_used)
            {
                self.receiver.close();
                info!(
                    target: "sandbox::block_builder",
                    total_tx_count,
                    total_gas_used,
                    "simulation limits reached, stopping builder"
                );
                return Ok(());
            }

            info!(
                target: "sandbox::block_builder",
                total_blocks_built,
                total_tx_count,
                total_gas_used,
                "Simulation progress: {total_blocks_built} blocks built, {total_tx_count} transactions processed, {total_gas_used} gas used"
            );

            let state_provider = self.provider_factory.latest()?;
            let state = StateProviderDatabase::new(&state_provider);
            let mut state_db: State<StateProviderDatabase<&Box<dyn StateProvider>>> =
                State::builder()
                    .with_database(state)
                    .with_bundle_update()
                    .build();

            let parent_header = self.parent_header.clone();

            //base fee for the block
            let base_fee = parent_header.base_fee_per_gas.unwrap_or(0);

            info!(
                target: "sandbox::block_builder",
                base_fee = base_fee,
                "base fee for the block"
            );

            let next_block_number = parent_header.number + 1;
            debug!(
                target: "sandbox::block_builder",
                parent = parent_header.number,
                next = next_block_number,
                timestamp = self.parent_timestamp + 1,
                "initializing block builder"
            );

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

            let mut block_gas_used = 0;
            let mut block_tx_count = 0;

            builder.apply_pre_execution_changes().map_err(|err| {
                warn!(target: "sandbox", %err, "failed to apply pre-execution changes");
                err
            })?;
            debug!(
                target: "sandbox::block_builder",
                block = next_block_number,
                "pre-execution changes applied"
            );

            while let Some(tx) = self.receiver.recv().await {
                let gas_used = builder
                    .execute_transaction_with_result_closure(tx.clone(), |res| {
                        if !res.is_success() {
                            info!(target: "sandbox", "transaction result: {:?}", res);
                            info!(target: "sandbox", "transaction: {:?}", tx);
                        }
                    })
                    .map_err(|err| {
                        warn!(target: "sandbox", %err, "failed to execute transaction {:?}", tx);
                        err
                    })?;

                block_gas_used += gas_used;
                block_tx_count += 1;

                if block_gas_used >= max_gas_for_block {
                    //finish the block
                    //commit to the db
                    //call build next block

                    //Last transaction in the block
                    info!(
                        target: "sandbox::block_builder",
                        block = next_block_number,
                        block_tx_count,
                        block_gas_used,
                    );

                    let outcome = builder.finish(&state_provider).map_err(|err| {
                        warn!(target: "sandbox", %err, "failed to finish building block");
                        err
                    })?;

                    info!(
                        target: "sandbox::block_builder",
                        block = next_block_number,
                        txs_in_block = block_tx_count,
                        gas_used = block_gas_used,
                        "sealing full block"
                    );

                    self.finish_block_and_commit(outcome, state_db).await?;

                    total_tx_count += block_tx_count;
                    total_gas_used += block_gas_used;
                    total_blocks_built += 1;
                    continue 'block_building;
                }
            }
        }
    }
}
