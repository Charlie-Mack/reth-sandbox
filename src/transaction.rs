use alloy_consensus::{EthereumTxEnvelope, TxEip4844, TxEnvelope};
use alloy_eips::eip7702::SignedAuthorization;
use alloy_network::{Ethereum, EthereumWallet, TransactionBuilder, TxSignerSync};
use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_rpc_types_eth::{TransactionInput, TransactionRequest};
use alloy_signer::{Signer, SignerSync};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use k256::ecdsa::SigningKey;
use reth_ethereum::TransactionSigned;
use reth_primitives_traits::{Recovered, SignedTransaction};

use crate::token::SandboxTokenHelper;

pub const DEFAULT_GAS_LIMIT: u64 = 21_000;
pub const DEAFULT_GAS_LIMIT_DEPLOY: u64 = 1_200_000;
pub const DEAFULT_GAS_LIMIT_TOKEN_TRANSFER: u64 = 100_000;

/// Helper for transaction operations
#[derive(Debug)]
pub struct TransactionOperations;

impl TransactionOperations {
    /// Creates a static transfer and signs it, returning an envelope.
    pub fn transfer_tx(
        chain_id: u64,
        sender: &LocalSigner<SigningKey>,
        to: Address,
        nonce: u64,
        value: U256,
    ) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
        let tx = TransactionRequest {
            nonce: Some(nonce),
            value: Some(value),
            to: Some(TxKind::Call(to)),
            gas: Some(DEFAULT_GAS_LIMIT),
            max_fee_per_gas: Some(20e9 as u128),
            max_priority_fee_per_gas: Some(20e9 as u128),
            chain_id: Some(chain_id),
            input: TransactionInput {
                input: None,
                data: None,
            },
            authorization_list: None,
            ..Default::default()
        };
        Self::sign_tx(sender, tx)
    }

    pub fn deploy_contract(
        chain_id: u64,
        sender: &LocalSigner<SigningKey>,
        nonce: u64,
        data: Bytes,
    ) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
        let tx = TransactionRequest {
            nonce: Some(nonce),
            value: None,
            to: Some(TxKind::Create),
            gas: Some(DEAFULT_GAS_LIMIT_DEPLOY),
            max_fee_per_gas: Some(20e9 as u128),
            max_priority_fee_per_gas: Some(20e9 as u128),
            chain_id: Some(chain_id),
            input: TransactionInput {
                input: None,
                data: Some(data),
            },
            authorization_list: None,
            ..Default::default()
        };
        Self::sign_tx(sender, tx)
    }

    pub fn transfer_token(
        chain_id: u64,
        sender: &LocalSigner<SigningKey>,
        nonce: u64,
        token_address: Address,
        to: Address,
        token_amount: U256,
    ) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
        let data = SandboxTokenHelper::transfer(to, token_amount);

        let tx = TransactionRequest {
            nonce: Some(nonce),
            value: None,
            to: Some(TxKind::Call(token_address)),
            gas: Some(DEAFULT_GAS_LIMIT_TOKEN_TRANSFER),
            max_fee_per_gas: Some(20e9 as u128),
            max_priority_fee_per_gas: Some(20e9 as u128),
            chain_id: Some(chain_id),
            input: TransactionInput {
                input: None,
                data: Some(data),
            },
            authorization_list: None,
            ..Default::default()
        };
        Self::sign_tx(sender, tx)
    }

    /// Signs an arbitrary [`TransactionRequest`] using the provided wallet
    fn sign_tx(
        signer: &LocalSigner<SigningKey>,
        tx: TransactionRequest,
    ) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
        let mut typed_tx = tx.build_typed_tx().unwrap();
        let signature = signer.sign_transaction_sync(&mut typed_tx).unwrap();
        let signed_tx = typed_tx.into_envelope(signature);
        let reth_tx: TransactionSigned = signed_tx.into();
        Recovered::new_unchecked(reth_tx, signer.address())
    }
}
