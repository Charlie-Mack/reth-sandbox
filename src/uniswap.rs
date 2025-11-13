//! Helpers that deploy Uniswap v2 artifacts and craft router interactions.

use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolConstructor};
use tracing::info;

use crate::actor::Actor;
use crate::orchestrator::TX;
use crate::transaction::tx;

sol!(
    #[allow(missing_docs)]
    WETH9,
    "artifacts/WETH9.json"
);
sol!(
    #[allow(missing_docs)]
    UniswapV2Factory,
    "artifacts/UniswapV2Factory.json"
);
sol!(
    #[allow(missing_docs)]
    UniswapV2Pair,
    "artifacts/UniswapV2Pair.json"
);
sol!(
    #[allow(missing_docs)]
    UniswapV2Router02,
    "artifacts/UniswapV2Router02.json"
);
sol!(
    #[allow(missing_docs)]
    UniswapV2ERC20,
    "artifacts/UniswapV2ERC20.json"
);

/// Holds relevant Uniswap contract addresses
pub struct Uniswap {
    factory_address: Address,
    router_address: Address,
    weth_address: Address,
}

impl Uniswap {
    /// Record the deployed addresses.
    pub fn new(factory_address: Address, router_address: Address, weth_address: Address) -> Self {
        info!(target: "sandbox", "Uniswap created: factory_address: {:?}, router_address: {:?}, weth_address: {:?}", factory_address, router_address, weth_address);
        Self {
            factory_address,
            router_address,
            weth_address,
        }
    }

    /// Deploy WETH, factory, and router contracts using the provided deployer.
    pub fn init(deployer: &Actor) -> (Uniswap, Vec<TX>) {
        let mut txs = Vec::new();

        let signer = deployer.signer().clone();

        let weth9_addr = deployer.contract_address(deployer.nonce());

        let weth9_tx = tx(
            &signer,
            deployer.nonce(),
            TxKind::Create,
            None,
            Some(WETH9::BYTECODE.clone()),
        );

        txs.push(weth9_tx);

        let factory_addr = deployer.contract_address(deployer.nonce() + 1);

        let factory_tx = tx(
            &signer,
            deployer.nonce() + 1,
            TxKind::Create,
            None,
            Some(UniswapV2FactoryHelper::deploy(deployer.address())),
        );

        txs.push(factory_tx);

        let router_addr = deployer.contract_address(deployer.nonce() + 2);

        let router_tx = tx(
            &signer,
            deployer.nonce() + 2,
            TxKind::Create,
            None,
            Some(UniswapV2Router02Helper::deploy(factory_addr, weth9_addr)),
        );

        txs.push(router_tx);

        let uniswap = Self::new(factory_addr, router_addr, weth9_addr);

        (uniswap, txs)
    }

    /// Factory address accessor.
    pub fn factory(&self) -> Address {
        self.factory_address
    }
    /// Router address accessor.
    pub fn router(&self) -> Address {
        self.router_address
    }
    /// WETH token address accessor.
    pub fn weth(&self) -> Address {
        self.weth_address
    }
}

/// Encode commonly used factory contract calls.
pub struct UniswapV2FactoryHelper;

impl UniswapV2FactoryHelper {
    /// Build bytecode + constructor calldata payload.
    pub fn deploy(deployer: Address) -> Bytes {
        [
            UniswapV2Factory::BYTECODE.as_ref(),
            &UniswapV2Factory::constructorCall::new((deployer,)).abi_encode(),
        ]
        .concat()
        .into()
    }

    /// Encode `createPair(token0, token1)`.
    pub fn create_pair(token0: Address, token1: Address) -> Bytes {
        UniswapV2Factory::createPairCall::new((token0, token1))
            .abi_encode()
            .into()
    }
}

/// Encode router interactions used throughout the simulation.
pub struct UniswapV2Router02Helper;

impl UniswapV2Router02Helper {
    /// Build bytecode + constructor calldata payload.
    pub fn deploy(factory: Address, weth: Address) -> Bytes {
        [
            UniswapV2Router02::BYTECODE.as_ref(),
            &UniswapV2Router02::constructorCall::new((factory, weth)).abi_encode(),
        ]
        .concat()
        .into()
    }

    /// Build calldata for `addLiquidityETH`.
    pub fn add_liquidity(token: Address, to: Address, amount_token_desired: U256) -> Bytes {
        let amount_token_min = U256::from(0);
        let amount_eth_min = U256::from(0);

        let call_data = UniswapV2Router02::addLiquidityETHCall::new((
            token,
            amount_token_desired,
            amount_token_min,
            amount_eth_min,
            to,
            Self::get_deadline(),
        ))
        .abi_encode();
        call_data.into()
    }

    /// Build calldata for `swapExactETHForTokens`.
    pub fn swap_eth_for_token(weth: Address, token_out: Address, to: Address) -> Bytes {
        let amount_out_min = U256::from(0);

        let path = vec![weth, token_out];
        UniswapV2Router02::swapExactETHForTokensCall::new((
            amount_out_min,
            path,
            to,
            Self::get_deadline(),
        ))
        .abi_encode()
        .into()
    }

    /// Build calldata for `swapExactTokensForETH`.
    pub fn swap_token_for_eth(
        token_in: Address,
        weth: Address,
        amount_in: U256,
        to: Address,
    ) -> Bytes {
        let amount_out_min = U256::from(0);
        let path = vec![token_in, weth];
        UniswapV2Router02::swapExactTokensForETHCall::new((
            amount_in,
            amount_out_min,
            path,
            to,
            Self::get_deadline(),
        ))
        .abi_encode()
        .into()
    }

    /// Timestamps for simulation start at 0 and increment by 1 so this should be good
    fn get_deadline() -> U256 {
        U256::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )
    }
}
