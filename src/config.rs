use alloy_primitives::Address;

#[derive(Clone, Debug)]
pub struct SimulationConfig {
    pub chain_id: u64,
    pub num_of_blocks: u64,
    pub unique_accounts: u64,
    pub unique_tokens: u64,
    pub gas_limit: u64,
    pub genesis_private_key: &'static str,
    pub genesis_address: Address,
    pub std_batch_size: u64,
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
        std_batch_size: u64,
    ) -> Self {
        Self {
            chain_id,
            num_of_blocks,
            unique_accounts,
            unique_tokens,
            gas_limit,
            genesis_private_key,
            genesis_address,
            std_batch_size,
        }
    }
}
