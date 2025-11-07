use std::sync::Arc;

use alloy_consensus::{Block, Transaction, TxEip4844Variant};
use alloy_primitives::{Address, B256, BlockHash, address, hex};
use alloy_rlp::Encodable;
use alloy_signer_local::PrivateKeySigner;
use k256::ecdsa::SigningKey;
use reth_db::DatabaseEnv;
use reth_ethereum::TransactionSigned;
use reth_evm::{
    ConfigureEvm, NextBlockEnvAttributes, RecoveredTx,
    execute::{BlockBuilder, BlockBuilderOutcome, ExecutorTx},
};
use reth_node_api::NodeTypesWithDBAdapter;
use reth_node_core::node_config::NodeConfig;
use reth_node_ethereum::{EthEvmConfig, EthereumNode};
use reth_primitives_traits::{Recovered, SealedHeader, SignedTransaction};
use reth_provider::{ProviderFactory, StateProvider};
use reth_revm::{State, database::StateProviderDatabase};
use tempfile::TempDir;
use tracing::{info, warn};

mod actors;
mod block_builder;
mod block_writer;
mod config;
mod metrics;
mod transaction;

use block_builder::SandboxBlockBuilder;
use block_writer::{BlockFileHeader, BlockFileWriter};

use transaction::TransactionOperations;

use crate::actors::ActorPool;

const GENESIS_PRIVATE_KEY: &str =
    "5ba8b410b0d2161dacd190f8aa6dfbc54ad1c84c67ee3e80611d92cc3fda8abd";

#[tokio::main]
async fn main() -> Result<(), eyre::Error> {
    metrics::run_start();
    tracing_subscriber::fmt::init();

    let (chain, temp_dir, node_config, db_path, static_files_path) = {
        let _t = time_section!("initialize_chain");
        let chain = config::custom_chain();
        let temp_dir = TempDir::new()?;
        let datadir = temp_dir.path().to_path_buf();
        let mut node_config = NodeConfig::new(chain.clone());
        node_config.datadir.datadir =
            reth_node_core::dirs::MaybePlatformPath::from(datadir.clone());
        let db_path = datadir.join("db");
        let static_files_path = datadir.join("static_files");
        (chain, temp_dir, node_config, db_path, static_files_path)
    };

    let provider_factory = {
        let _t = time_section!("initialize_provider_factory");
        let db_args = reth_node_core::args::DatabaseArgs::default().database_args();
        let db_env = reth_db::init_db(&db_path, db_args)?;
        let db = Arc::new(db_env);

        let provider_factory = ProviderFactory::<
            NodeTypesWithDBAdapter<EthereumNode, Arc<DatabaseEnv>>,
        >::new(
            db.clone(),
            chain.clone(),
            reth_provider::providers::StaticFileProvider::read_write(static_files_path.clone())?,
        )?;
        // Initialize genesis if needed
        reth_db_common::init::init_genesis(&provider_factory)?;
        provider_factory
    };

    // Get the evm config
    let evm_config = EthEvmConfig::new(chain.clone());

    let state_provider = provider_factory.latest()?;
    let genesis_header = chain.genesis_header();
    let genesis_block_hash: BlockHash = chain.genesis_hash().into();

    let gas_limit = chain.genesis().gas_limit;

    let sealed_header = SealedHeader::new(genesis_header.clone(), genesis_block_hash);

    let mut actor_pool = ActorPool::new_with_genesis();

    let mut block_builder = SandboxBlockBuilder::new(
        &state_provider,
        sealed_header,
        genesis_header.timestamp,
        gas_limit,
        evm_config,
    );

    let output_path = std::env::current_dir()?.join("blocks.bin");

    let mut block_file_writer =
        BlockFileWriter::new(&output_path, BlockFileHeader::new(false, 0, 100))?;

    let num_of_blocks = 5;

    for i in 0..num_of_blocks {
        let block_number = i + 1;
        let _t = time_section!("total build for block number {}", block_number);
        let block = block_builder
            .build_next_block(block_number, &mut actor_pool)
            .await?;
        let mut buf = Vec::with_capacity(block.length());
        block.encode(&mut buf);

        block_file_writer.write_block(&buf)?;
    }

    block_file_writer.finish()?;

    metrics::run_end();
    crate::metrics::print_section_summary();

    Ok(())
}
