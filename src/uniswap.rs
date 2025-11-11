use alloy_primitives::{Address, Bytes};
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolConstructor, SolEvent};

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

pub struct UniswapV2FactoryHelper {
    factory_address: Address,
}

impl UniswapV2FactoryHelper {
    pub fn deploy_factory_calldata(deployer: Address) -> Bytes {
        let mut init = UniswapV2Factory::BYTECODE.clone();
        let ctor = UniswapV2Factory::constructorCall::new((deployer,)).abi_encode();
        let mut combined = Vec::new();
        combined.extend_from_slice(&init);
        combined.extend_from_slice(&ctor);
        Bytes::from(combined)
    }
}
