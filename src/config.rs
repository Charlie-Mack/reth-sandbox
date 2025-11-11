use alloy_primitives::Address;

#[derive(Clone, Debug)]
pub struct TransactionGasLimits {
    pub transfer: u64,
    pub deploy: u64,
    pub transfer_token: u64,
}

impl TransactionGasLimits {
    pub fn new(transfer: u64, deploy: u64, transfer_token: u64) -> Self {
        Self {
            transfer,
            deploy,
            transfer_token,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SimulationConfig {
    pub chain_id: u64,
    pub num_of_blocks: u64,
    pub unique_accounts: u64,
    pub unique_tokens: u64,
    pub gas_limit: u64,
    pub genesis_private_key: &'static str,
    pub genesis_address: Address,
    pub tx_gas_limits: TransactionGasLimits,
}

impl SimulationConfig {
    pub fn new(
        chain_id: u64,
        num_of_blocks: u64,
        unique_accounts: u64,
        unique_tokens: u64,
        gas_limit: u64,
        genesis_private_key: &'static str,
        genesis_address: Address,
        tx_gas_limits: TransactionGasLimits,
    ) -> Self {
        Self {
            chain_id,
            num_of_blocks,
            unique_accounts,
            unique_tokens,
            gas_limit,
            genesis_private_key,
            genesis_address,
            tx_gas_limits,
        }
    }
}
