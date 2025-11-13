use alloy_genesis::Genesis;
use alloy_primitives::{Address, U256};
use reth_chainspec::ChainSpec;
use std::{fs, path::PathBuf, sync::Arc};
use tracing::{info, warn};

pub fn custom_chain(gas_limit: u64, chain_id: u64, genesis_address: Address) -> Arc<ChainSpec> {
    let balance = U256::MAX;

    // Construct genesis JSON
    let custom_genesis = format!(
        r#"{{
    "nonce": "0x42",
    "timestamp": "0x0",
    "extraData": "0x5343",
    "gasLimit": "0x{:x}",
    "difficulty": "0x400000000",
    "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "coinbase": "0x0000000000000000000000000000000000000000",
    "alloc": {{
        "{:x}": {{
            "balance": "0x{:x}"
        }}
    }},
    "number": "0x0",
    "gasUsed": "0x0",
    "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "config": {{
        "ethash": {{}},
        "chainId": {},
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true,
        "shanghaiTime": 0
    }}
}}"#,
        gas_limit, genesis_address, balance, chain_id
    );

    // ✅ Write to genesis.json in current directory
    let output_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("sandbox_genesis.json");

    if let Err(e) = fs::write(&output_path, &custom_genesis) {
        warn!("⚠️ Failed to write genesis file: {}", e);
    } else {
        info!("✅ Wrote genesis file to {:?}", output_path);
    }

    // Parse JSON into Genesis → ChainSpec
    let genesis: Genesis =
        serde_json::from_str(&custom_genesis).expect("Failed to parse custom genesis JSON");

    Arc::new(genesis.into())
}
