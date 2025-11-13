//! Helpers for the synthetic ERC20 used within the sandbox.

use alloy_primitives::{Address, Bytes, U256};

use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolConstructor, SolEvent};
use tracing::info;

//We use a custom ERC20 token for the sandbox which auto-mints tokens that actors try to send
sol!(
    #[allow(missing_docs)]
    SandboxToken,
    "artifacts/SandboxToken.json"
);

/// Static helpers for constructing calls against the sandbox ERC20.
pub struct SandboxTokenHelper;

impl SandboxTokenHelper {
    /// Combine bytecode and constructor args into deployable payload.
    pub fn deploy() -> Bytes {
        [
            SandboxToken::BYTECODE.as_ref(),
            &SandboxToken::constructorCall::new((U256::from(1e18),)).abi_encode(),
        ]
        .concat()
        .into()
    }

    /// ABI-encode a token transfer call.
    pub fn transfer(to: Address, value: U256) -> Bytes {
        let call_data = SandboxToken::transferCall::new((to, value)).abi_encode();
        call_data.into()
    }

    /// ABI-encode an approval call for the Uniswap router.
    pub fn approve(spender: Address, value: U256) -> Bytes {
        let call_data = SandboxToken::approveCall::new((spender, value)).abi_encode();
        call_data.into()
    }
}

/// Tracks deterministic ERC20 addresses so the orchestrator can reuse them.
pub struct TokenPool {
    tokens: Vec<Token>,
}

impl TokenPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Record a new token deployment
    pub fn add_token(&mut self, token_address: Address) {
        self.tokens.push(Token::new(token_address));
    }

    /// Get the address for the provided index.
    pub fn token_address(&self, index: u64) -> Address {
        self.tokens[index as usize].address()
    }
}

/// Lightweight token handle stored in [`TokenPool`].
pub struct Token {
    address: Address,
}

impl Token {
    /// Remember the deployed address.
    pub fn new(address: Address) -> Self {
        Self { address }
    }

    /// Returns the token address.
    pub fn address(&self) -> Address {
        self.address.clone()
    }
}
