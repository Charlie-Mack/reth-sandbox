use alloy_consensus::{EthereumTxEnvelope, TxEip4844};
use alloy_primitives::{Address, TxKind, U256};
use rand::Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use reth_primitives_traits::Recovered;
use tokio::sync::mpsc::Sender;
use tracing::{debug, info};

use crate::{
    actor::ActorPool,
    config::SimulationConfig,
    token::{SandboxTokenHelper, TokenPool},
    transaction::tx,
    uniswap::{Uniswap, UniswapV2FactoryHelper, UniswapV2Router02Helper},
};

pub type TX = Recovered<EthereumTxEnvelope<TxEip4844>>;

#[derive(Debug, Clone, Copy)]
enum TransactionType {
    EthTransfer,
    TokenTransfer,
    UniswapSwapForEth,
    UniswapSwapForToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimulationPhase {
    ActorFunding,
    TokenDeployment,
    UniswapDeployment,
    UniswapPoolCreation,
    TransactionLoad,
}

pub struct TransactionOrchestrator {
    sender: Sender<TX>,
    config: SimulationConfig,
    actor_pool: ActorPool,
    token_contract_pool: TokenPool,
    uniswap: Option<Uniswap>,
    actors_funded: u64,
    tokens_deployed: u64,
    token_pools_created: u64,
}

impl TransactionOrchestrator {
    pub fn new(sender: Sender<TX>, config: SimulationConfig) -> Self {
        let actor_pool = ActorPool::new(
            config.genesis_private_key,
            config.genesis_address,
            config.chain_id,
        );

        let token_contract_pool = TokenPool::new();

        Self {
            sender,
            config,
            actor_pool,
            token_contract_pool,
            uniswap: None,
            actors_funded: 0,
            tokens_deployed: 0,
            token_pools_created: 0,
        }
    }

    pub async fn run(mut self) -> eyre::Result<()> {
        tokio::spawn(async move {
            info!(
                target: "sandbox::orchestrator",
                accounts = self.config.unique_accounts,
                tokens = self.config.unique_tokens,
                gas_limit = self.config.gas_limit,
                "starting transaction orchestration"
            );
            //generate actors to use
            self.actor_pool.generate_actors(self.config.unique_accounts);
            debug!(
                target: "sandbox::orchestrator",
                generated_actors = self.actor_pool.len(),
                "actor pool ready"
            );

            let mut last_phase: Option<SimulationPhase> = None;

            loop {
                //run main loop

                let phase = self.current_phase();
                if last_phase != Some(phase) {
                    info!(
                        target: "sandbox::orchestrator",
                        ?phase,
                        "entering simulation phase"
                    );
                    last_phase = Some(phase);
                }

                let batch = self.generate_batch();

                for tx in batch {
                    if let Err(e) = self.sender.send(tx).await {
                        // Channel closed - builder is done
                        debug!(target: "sandbox::orchestrator", "channel closed, stopping orchestration");
                        return;
                    }
                }
            }
        });

        Ok(())
    }

    fn generate_batch(&mut self) -> Vec<TX> {
        match self.current_phase() {
            SimulationPhase::ActorFunding => self.generate_actor_funding_batch(),
            SimulationPhase::TokenDeployment => self.generate_token_deployment_batch(),
            SimulationPhase::UniswapDeployment => self.generate_uniswap_deployment_batch(),
            SimulationPhase::UniswapPoolCreation => self.generate_uniswap_pool_creation_batch(),
            SimulationPhase::TransactionLoad => self.generate_transaction_load_batch(),
        }
    }

    fn generate_actor_funding_batch(&mut self) -> Vec<TX> {
        let batch_size = std::cmp::min(
            self.config.std_batch_size,
            self.config.unique_accounts - self.actors_funded,
        );

        let (g_signer, g_nonce) = self.actor_pool.deployer_info();

        let txs = (0..batch_size)
            .into_par_iter()
            .map(|i| {
                tx(
                    &g_signer,
                    g_nonce + i,
                    TxKind::Call(self.actor_pool.actor_address((g_nonce + i) as usize)),
                    Some(U256::from(1_000_000e18)),
                    None,
                )
            })
            .collect::<Vec<TX>>();

        self.actor_pool.increment_deployer_nonce_by(batch_size);
        self.actors_funded += batch_size;

        txs
    }

    fn generate_token_deployment_batch(&mut self) -> Vec<TX> {
        let batch_size = std::cmp::min(
            self.config.std_batch_size,
            self.config.unique_tokens - self.tokens_deployed,
        );

        let (g_signer, g_nonce) = self.actor_pool.deployer_info();

        for i in 0..batch_size {
            let token_address: Address = g_signer.address().create(g_nonce + i);
            self.token_contract_pool.add_token(token_address);
        }

        let data = SandboxTokenHelper::deploy();

        let txs = (0..batch_size)
            .into_par_iter()
            .map(|i| {
                tx(
                    &g_signer,
                    g_nonce + i,
                    TxKind::Create,
                    None,
                    Some(data.clone()),
                )
            })
            .collect::<Vec<TX>>();

        self.actor_pool.increment_deployer_nonce_by(batch_size);
        self.tokens_deployed += batch_size;

        txs
    }

    fn generate_uniswap_deployment_batch(&mut self) -> Vec<TX> {
        let (uniswap, deployment_txs) = Uniswap::init(self.actor_pool.deployer());
        self.uniswap = Some(uniswap);
        self.actor_pool
            .increment_deployer_nonce_by(deployment_txs.len() as u64);
        deployment_txs
    }

    fn generate_uniswap_pool_creation_batch(&mut self) -> Vec<TX> {
        let batch_size = std::cmp::min(
            self.config.std_batch_size / 3,
            self.config.unique_tokens - self.token_pools_created,
        );

        let mut txs = Vec::with_capacity(batch_size as usize);

        let uniswap = self.uniswap.as_ref().unwrap();

        let (g_signer, g_nonce) = self.actor_pool.deployer_info();
        let pool_created = self.token_pools_created;

        txs.extend(
            (0..batch_size)
                .into_par_iter()
                .map(|i| {
                    let mut txs = Vec::with_capacity(3);
                    let nonce_offset = i * 3;
                    //create pair
                    txs.push(tx(
                        &g_signer,
                        g_nonce + nonce_offset,
                        TxKind::Call(uniswap.factory()),
                        None,
                        Some(UniswapV2FactoryHelper::create_pair(
                            uniswap.weth(),
                            self.token_contract_pool.token_address(pool_created + i),
                        )),
                    ));

                    //approve token
                    txs.push(tx(
                        &g_signer,
                        g_nonce + nonce_offset + 1,
                        TxKind::Call(self.token_contract_pool.token_address(pool_created + i)),
                        None,
                        Some(SandboxTokenHelper::approve(
                            uniswap.router(),
                            U256::from(1_000_000e18),
                        )),
                    ));

                    //add liquidity
                    txs.push(tx(
                        &g_signer,
                        g_nonce + nonce_offset + 2,
                        TxKind::Call(uniswap.router()),
                        Some(U256::from(10_000e18)),
                        Some(UniswapV2Router02Helper::add_liquidity(
                            self.token_contract_pool.token_address(pool_created + i),
                            g_signer.address(),
                            U256::from(1_000_000e18),
                        )),
                    ));

                    txs
                })
                .flatten()
                .collect::<Vec<TX>>(),
        );

        self.actor_pool.increment_deployer_nonce_by(batch_size * 3);
        self.token_pools_created += batch_size;

        txs
    }

    fn generate_transaction_load_batch(&mut self) -> Vec<TX> {
        let batch_size = self.config.std_batch_size;

        let assignments: Vec<(usize, u64, usize, TransactionType)> = (0..batch_size)
            .map(|_| {
                let sending_actor_index = rand::rng().random_range(0..self.actor_pool.len() - 1);
                let receiving_actor_index = rand::rng().random_range(0..self.actor_pool.len() - 1);

                let transaction_type = match rand::rng().random_range(0..10) {
                    0..=3 => TransactionType::TokenTransfer,
                    4..=5 => TransactionType::UniswapSwapForEth,
                    6..=7 => TransactionType::UniswapSwapForToken,
                    _ => TransactionType::EthTransfer,
                };

                //We need to approve the token for the uniswap router
                let increment_nonce_by = match transaction_type {
                    TransactionType::UniswapSwapForEth => 2,
                    _ => 1,
                };

                let nonce = self
                    .actor_pool
                    .get_and_increment_nonce_by(sending_actor_index, increment_nonce_by);
                (
                    sending_actor_index,
                    nonce,
                    receiving_actor_index,
                    transaction_type,
                )
            })
            .collect();

        let payloads = (0..batch_size)
            .into_par_iter()
            .flat_map(|i| {
                let (sending_actor_index, nonce, receiving_actor_index, transaction_type) =
                    assignments[i as usize];

                let (signer, _) = self.actor_pool.actor_info(sending_actor_index);
                let receiving_address = self.actor_pool.actor_address(receiving_actor_index);

                let token_address = self
                    .token_contract_pool
                    .token_address(rand::rng().random_range(0..self.tokens_deployed));

                let uniswap = self.uniswap.as_ref().unwrap();

                let txs = match transaction_type {
                    TransactionType::EthTransfer => {
                        vec![tx(
                            &signer,
                            nonce,
                            TxKind::Call(receiving_address),
                            Some(U256::from(100)),
                            None,
                        )]
                    }

                    TransactionType::TokenTransfer => {
                        vec![tx(
                            &signer,
                            nonce,
                            TxKind::Call(token_address),
                            None,
                            Some(SandboxTokenHelper::transfer(
                                receiving_address,
                                U256::from(100),
                            )),
                        )]
                    }
                    TransactionType::UniswapSwapForEth => {
                        //create two transactions
                        //approve the token for the uniswap router

                        let approve_tx = tx(
                            &signer,
                            nonce,
                            TxKind::Call(token_address),
                            None,
                            Some(SandboxTokenHelper::approve(
                                uniswap.router(),
                                U256::from(1e18),
                            )),
                        );

                        let swap_tx = tx(
                            &signer,
                            nonce + 1,
                            TxKind::Call(uniswap.router()),
                            None,
                            Some(UniswapV2Router02Helper::swap_token_for_eth(
                                token_address,
                                uniswap.weth(),
                                U256::from(1e18),
                                signer.address(),
                            )),
                        );

                        vec![approve_tx, swap_tx]
                    }
                    TransactionType::UniswapSwapForToken => {
                        vec![tx(
                            &signer,
                            nonce,
                            TxKind::Call(uniswap.router()),
                            Some(U256::from(100)),
                            Some(UniswapV2Router02Helper::swap_eth_for_token(
                                uniswap.weth(),
                                token_address,
                                signer.address(),
                            )),
                        )]
                    }
                };
                txs
            })
            .collect::<Vec<TX>>();

        payloads
    }

    fn current_phase(&self) -> SimulationPhase {
        if self.actors_funded < self.config.unique_accounts {
            SimulationPhase::ActorFunding
        } else if self.tokens_deployed < self.config.unique_tokens {
            SimulationPhase::TokenDeployment
        } else if self.uniswap.is_none() {
            SimulationPhase::UniswapDeployment
        } else if self.token_pools_created < self.config.unique_tokens {
            SimulationPhase::UniswapPoolCreation
        } else {
            SimulationPhase::TransactionLoad
        }
    }
}
