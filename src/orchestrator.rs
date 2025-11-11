use alloy_consensus::{EthereumTxEnvelope, TxEip4844};
use alloy_primitives::{Address, U256};
use rand::Rng;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use reth_primitives_traits::Recovered;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::{
    actor::ActorPool,
    config::SimulationConfig,
    token::{SandboxTokenHelper, TokenPool},
    transaction::TransactionOperations,
    uniswap::UniswapV2FactoryHelper,
};

const STD_BATCH_SIZE: u64 = 1000; //TODO: Make this configurable

struct TransactionBatchOutcome {
    gas_used: u64,
    txs: Vec<Recovered<EthereumTxEnvelope<TxEip4844>>>,
}

#[derive(Debug, Clone, Copy)]
enum TransactionType {
    EthTransfer,
    TokenTransfer,
}

enum SimulationPhase {
    Funding,
    TokenDeployment,
    UniswapSetup,
    RandomTransfers,
    Complete,
}

impl SimulationPhase {
    fn generate_batch(
        &self,
        orch: &mut TransactionOrchestrator,
    ) -> Option<TransactionBatchOutcome> {
        match self {
            Self::Funding => {
                let remaining_capacity = orch.remaining_gas / orch.config.tx_gas_limits.transfer;
                let batch_size = std::cmp::min(STD_BATCH_SIZE, remaining_capacity);

                if remaining_capacity == 0 {
                    return None;
                }

                let (g_signer, g_nonce) = orch.actor_pool.genesis_actor_info();

                let txs = (0..batch_size)
                    .into_par_iter()
                    .map(|i| {
                        TransactionOperations::transfer_tx(
                            orch.config.chain_id,
                            &g_signer,
                            orch.actor_pool.actor_address((g_nonce + i + 1) as usize),
                            g_nonce + i,
                            U256::from(10e18),
                        )
                    })
                    .collect::<Vec<Recovered<EthereumTxEnvelope<TxEip4844>>>>();

                orch.actor_pool.increment_genesis_actor_nonce_by(batch_size);
                orch.actor_pool.increment_actors_funded_by(batch_size);

                Some(TransactionBatchOutcome {
                    gas_used: batch_size * orch.config.tx_gas_limits.transfer,
                    txs,
                })
            }

            Self::TokenDeployment => {
                let remaining_capacity = orch.remaining_gas / orch.config.tx_gas_limits.deploy;
                let batch_size = std::cmp::min(STD_BATCH_SIZE, remaining_capacity);

                if remaining_capacity == 0 {
                    return None;
                }

                let assignments: Vec<(usize, u64)> = (0..batch_size)
                    .map(|_| {
                        let actor_index = rand::rng().random_range(0..orch.actor_pool.len());
                        let nonce = orch.actor_pool.get_and_increment_nonce(actor_index);
                        let actor_address = orch.actor_pool.actor_address(actor_index);
                        let token_address: Address = actor_address.create(nonce);
                        orch.token_contract_pool.add_token(token_address);
                        (actor_index, nonce)
                    })
                    .collect();

                let txs = (0..batch_size)
                    .into_par_iter()
                    .map(|i| {
                        let (actor_index, nonce) = assignments[i as usize];
                        let (signer, _) = orch.actor_pool.actor_info(actor_index);
                        let data = SandboxTokenHelper::deploy();
                        TransactionOperations::deploy_contract(
                            orch.config.chain_id,
                            &signer,
                            nonce,
                            data,
                        )
                    })
                    .collect::<Vec<Recovered<EthereumTxEnvelope<TxEip4844>>>>();

                Some(TransactionBatchOutcome {
                    gas_used: batch_size * orch.config.tx_gas_limits.deploy,
                    txs,
                })
            }
            Self::UniswapSetup => {
                let remaining_capacity = orch.remaining_gas / orch.config.tx_gas_limits.deploy;
                let batch_size = std::cmp::min(STD_BATCH_SIZE, remaining_capacity);

                if remaining_capacity == 0 {
                    return None;
                }

                let (g_signer, g_nonce) = orch.actor_pool.genesis_actor_info();

                //factory deployment transaction

                let data = UniswapV2FactoryHelper::deploy_factory_calldata(g_signer.address());
                let tx = TransactionOperations::deploy_contract(
                    orch.config.chain_id,
                    &g_signer,
                    g_nonce,
                    data,
                );

                let txs = vec![tx];

                orch.uniswap_setup_complete = true;

                Some(TransactionBatchOutcome {
                    gas_used: batch_size * orch.config.tx_gas_limits.deploy,
                    txs,
                })
            }
            Self::RandomTransfers => {
                let mut gas_used_for_batch = 0;

                let batch_size = STD_BATCH_SIZE;

                let assignments: Vec<(usize, u64, usize, TransactionType)> = (0..batch_size)
                    .map(|i| {
                        let sending_actor_index =
                            rand::rng().random_range(0..orch.actor_pool.len());
                        let receiving_actor_index =
                            rand::rng().random_range(0..orch.actor_pool.len());

                        let transaction_type = match rand::rng().random_range(0..10) {
                            0..=6 => {
                                gas_used_for_batch += orch.config.tx_gas_limits.transfer_token;
                                TransactionType::TokenTransfer
                            }
                            _ => {
                                gas_used_for_batch += orch.config.tx_gas_limits.transfer;
                                TransactionType::EthTransfer
                            }
                        };

                        let nonce = orch.actor_pool.get_and_increment_nonce(sending_actor_index);
                        (
                            sending_actor_index,
                            nonce,
                            receiving_actor_index,
                            transaction_type,
                        )
                    })
                    .collect();

                let txs = (0..batch_size)
                    .into_par_iter()
                    .map(|i| {
                        let (sending_actor_index, nonce, receiving_actor_index, transaction_type) =
                            assignments[i as usize];

                        let (signer, _) = orch.actor_pool.actor_info(sending_actor_index);
                        let receiving_address =
                            orch.actor_pool.actor_address(receiving_actor_index);

                        match transaction_type {
                            TransactionType::EthTransfer => TransactionOperations::transfer_tx(
                                orch.config.chain_id,
                                &signer,
                                receiving_address,
                                nonce,
                                U256::from(100),
                            ),
                            TransactionType::TokenTransfer => {
                                let token_address = orch.token_contract_pool.token_address(
                                    rand::rng().random_range(
                                        0..orch.token_contract_pool.tokens_deployed(),
                                    ),
                                );

                                TransactionOperations::transfer_token(
                                    orch.config.chain_id,
                                    &signer,
                                    nonce,
                                    token_address,
                                    receiving_address,
                                    U256::from(1e18),
                                )
                            }
                        }
                    })
                    .collect::<Vec<Recovered<EthereumTxEnvelope<TxEip4844>>>>();

                Some(TransactionBatchOutcome {
                    gas_used: gas_used_for_batch,
                    txs,
                })
            }
            Self::Complete => None,
        }
    }
}

pub struct TransactionOrchestrator {
    sender: Sender<Recovered<EthereumTxEnvelope<TxEip4844>>>,
    config: SimulationConfig,
    remaining_gas: u64,
    actor_pool: ActorPool,
    token_contract_pool: TokenPool,
    uniswap_setup_complete: bool,
}

impl TransactionOrchestrator {
    pub fn new(
        sender: Sender<Recovered<EthereumTxEnvelope<TxEip4844>>>,
        config: SimulationConfig,
    ) -> Self {
        let actor_pool = ActorPool::new_with_genesis(
            config.genesis_private_key,
            config.genesis_address,
            config.chain_id,
        );

        let token_contract_pool = TokenPool::new();
        let remaining_gas = config.gas_limit * config.num_of_blocks;

        Self {
            sender,
            config,
            remaining_gas,
            actor_pool,
            token_contract_pool,
            uniswap_setup_complete: false,
        }
    }

    pub async fn run(mut self) -> eyre::Result<()> {
        tokio::spawn(async move {
            //generate actors to use
            self.actor_pool.generate_actors(self.config.unique_accounts);

            while self.remaining_gas > 0 {
                //run main loop

                let phase = self.current_phase();

                if let Some(batch_outcome) = phase.generate_batch(&mut self) {
                    for tx in batch_outcome.txs {
                        let _ = self
                            .sender
                            .send(tx)
                            .await
                            .map_err(|e| eyre::eyre!("Failed to send transaction: {:?}", e));
                    }

                    self.remaining_gas = self.remaining_gas.saturating_sub(batch_outcome.gas_used);
                } else {
                    // no more batches to send
                    break;
                }
            }
        });

        Ok(())
    }

    fn current_phase(&self) -> SimulationPhase {
        if self.actor_pool.actors_funded() < self.config.unique_accounts {
            SimulationPhase::Funding
        } else if self.token_contract_pool.tokens_deployed() < self.config.unique_tokens {
            SimulationPhase::TokenDeployment
        } else if !self.uniswap_setup_complete {
            SimulationPhase::UniswapSetup
        } else if self.remaining_gas > 0 {
            SimulationPhase::RandomTransfers
        } else {
            SimulationPhase::Complete
        }
    }
}
