use alloy_primitives::{Address, Bytes, TxKind, U256};

use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolConstructor, SolEvent};
use tracing::info;

sol!(
    #[allow(missing_docs)]
    SandboxToken,
    "artifacts/SandboxToken.json"
);

pub struct SandboxTokenHelper;

impl SandboxTokenHelper {
    pub fn deploy() -> Bytes {
        let mut init = SandboxToken::BYTECODE.clone();

        let ctor = SandboxToken::constructorCall::new((U256::from(1e18),)).abi_encode();
        let mut combined = Vec::new();
        combined.extend_from_slice(&init);
        combined.extend_from_slice(&ctor);
        Bytes::from(combined)
    }

    pub fn transfer(to: Address, value: U256) -> Bytes {
        let call_data = SandboxToken::transferCall::new((to, value)).abi_encode();
        call_data.into()
    }
}

pub struct TokenPool {
    tokens: Vec<Token>,
}

impl TokenPool {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    pub fn add_token(&mut self, token_address: Address) {
        self.tokens.push(Token::new(token_address));
    }

    pub fn tokens_deployed(&self) -> u64 {
        self.tokens.len() as u64
    }

    pub fn token_address(&self, index: u64) -> Address {
        self.tokens[index as usize].address()
    }
}

pub struct Token {
    address: Address,
}

impl Token {
    pub fn new(address: Address) -> Self {
        Self { address }
    }

    pub fn address(&self) -> Address {
        self.address.clone()
    }
}
