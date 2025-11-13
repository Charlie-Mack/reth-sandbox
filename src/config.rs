//! Simulation-wide knobs that describe how aggressively the sandbox should
//! generate state and transactions.

use alloy_primitives::Address;

/// Captures all tunable parameters the orchestrator and block builder need in
/// order to synthesize accounts, tokens, and blocks deterministically.
#[derive(Clone, Debug)]
pub struct SimulationConfig {
    /// Chain ID propagated into signed transactions and the genesis file.
    pub chain_id: u64,
    /// Number of blocks to target before the builder drains the channel.
    pub num_of_blocks: Option<u64>,
    /// Number of transactions to target before the orchestrator stops.
    pub num_of_transactions: Option<u64>,
    /// Total number of funded EOA actors that will participate in the load.
    pub unique_accounts: u64,
    /// Total number of ERC20 contracts to deploy before stress testing swaps.
    pub unique_tokens: u64,
    /// Per-block gas limit, also used to cap the global simulation budget.
    pub gas_limit: u64,
    /// Private key that owns the pre-funded genesis allocation.
    pub genesis_private_key: &'static str,
    /// Address that corresponds to `genesis_private_key`.
    pub genesis_address: Address,
    /// Batch size used by the orchestrator when emitting homogeneous work.
    pub std_batch_size: u64,
}

impl SimulationConfig {
    /// Helper constructor to keep call sites terse.
    pub fn new(
        chain_id: u64,
        num_of_blocks: Option<u64>,
        num_of_transactions: Option<u64>,
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
            num_of_transactions,
            unique_accounts,
            unique_tokens,
            gas_limit,
            genesis_private_key,
            genesis_address,
            std_batch_size,
        }
    }

    pub fn max_blocks(&self) -> Option<u64> {
        self.num_of_blocks
    }

    pub fn max_transactions(&self) -> Option<u64> {
        self.num_of_transactions
    }

    /// Optional total gas budget: gas_limit * num_blocks, if num_blocks is set.
    pub fn gas_budget(&self) -> Option<u64> {
        self.num_of_blocks
            .map(|blocks| self.gas_limit.saturating_mul(blocks))
    }

    pub fn limits_hit(&self, blocks: u64, txs: u64, gas_used: u64) -> bool {
        if let Some(max_blocks) = self.max_blocks() {
            if blocks >= max_blocks {
                return true;
            }
        }

        if let Some(max_txs) = self.max_transactions() {
            if txs >= max_txs {
                return true;
            }
        }

        if let Some(budget) = self.gas_budget() {
            if gas_used >= budget {
                return true;
            }
        }

        false
    }
}
