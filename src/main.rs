use std::sync::Arc;

use alloy_consensus::{EthereumTxEnvelope, TxEip4844};
use alloy_primitives::{Address, address};
use reth_db::DatabaseEnv;
use reth_node_api::NodeTypesWithDBAdapter;
use reth_node_core::node_config::NodeConfig;
use reth_node_ethereum::EthereumNode;
use reth_primitives_traits::Recovered;

use reth_provider::ProviderFactory;

use tempfile::TempDir;
use tokio::sync::mpsc;
use tracing::{info, warn};

mod actor;
mod block_builder;
mod block_writer;
mod chain;
mod config;
mod metrics;
mod orchestrator;
mod token;
mod transaction;
mod uniswap;

use block_builder::SandboxBlockBuilder;
use orchestrator::TransactionOrchestrator;

use crate::{
    actor::ActorPool,
    config::{SimulationConfig, TransactionGasLimits},
};

//TODO: Figure out the num of blocks problem with 80% fill rate and gas limit / block num user request
const GENESIS_PRIVATE_KEY: &str =
    "5ba8b410b0d2161dacd190f8aa6dfbc54ad1c84c67ee3e80611d92cc3fda8abd";
const GENESIS_ADDRESS: Address = address!("0xFaa235fA90514d9083d0aa61878eBEb5Cf94FCD7");
const NUM_OF_BLOCKS: u64 = 5;
const GAS_LIMIT: u64 = 50_000_000;
const CHAIN_ID: u64 = 2600;
const UNIQUE_ACCOUNTS: u64 = 1_000;
const UNIQUE_TOKENS: u64 = 1_00;

//Gas Limits
const TRANSFER_GAS_LIMIT: u64 = 21_000;
const DEPLOY_TOKEN_GAS_LIMIT: u64 = 1_100_000;
const TRANSFER_TOKEN_GAS_LIMIT: u64 = 60_000;

#[tokio::main]
async fn main() -> Result<(), eyre::Error> {
    metrics::run_start();
    tracing_subscriber::fmt::init();

    let tx_gas_limits = TransactionGasLimits::new(
        TRANSFER_GAS_LIMIT,
        DEPLOY_TOKEN_GAS_LIMIT,
        TRANSFER_TOKEN_GAS_LIMIT,
    );

    let sim_config = SimulationConfig::new(
        CHAIN_ID,
        NUM_OF_BLOCKS,
        UNIQUE_ACCOUNTS,
        UNIQUE_TOKENS,
        GAS_LIMIT,
        GENESIS_PRIVATE_KEY,
        GENESIS_ADDRESS,
        tx_gas_limits,
    );

    let chain = chain::custom_chain(
        sim_config.gas_limit,
        sim_config.chain_id,
        sim_config.genesis_address,
    );

    let temp_dir = TempDir::new()?;
    let datadir = temp_dir.path().to_path_buf();
    let mut node_config = NodeConfig::new(chain.clone());
    node_config.datadir.datadir = reth_node_core::dirs::MaybePlatformPath::from(datadir.clone());

    let db_path = datadir.join("db");
    let static_files_path = datadir.join("static_files");

    let db_args = reth_node_core::args::DatabaseArgs::default().database_args();
    let db_env = reth_db::init_db(&db_path, db_args)?;
    let db = Arc::new(db_env);

    let provider_factory =
        ProviderFactory::<NodeTypesWithDBAdapter<EthereumNode, Arc<DatabaseEnv>>>::new(
            db.clone(),
            chain.clone(),
            reth_provider::providers::StaticFileProvider::read_write(static_files_path.clone())?,
        )?;
    // Initialize genesis if needed
    reth_db_common::init::init_genesis(&provider_factory)?;

    let (sender, receiver) = mpsc::channel::<Recovered<EthereumTxEnvelope<TxEip4844>>>(1000);

    let mut block_builder = SandboxBlockBuilder::new(provider_factory.clone(), chain, receiver);

    let tx_orchestrator = TransactionOrchestrator::new(sender, sim_config.clone());

    tx_orchestrator.run().await?;
    let result = block_builder.start_building().await?;

    block_builder.finish_file_writer()?;

    metrics::run_end();
    crate::metrics::print_section_summary();
    Ok(result)
}
