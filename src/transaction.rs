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

pub const DEFAULT_GAS_LIMIT: u64 = 21000;

/// Helper for transaction operations
#[derive(Debug)]
pub struct TransactionOperations;

impl TransactionOperations {
    /// Creates a static transfer and signs it, returning an envelope.
    pub fn transfer_tx(
        chain_id: u64,
        sender: &LocalSigner<SigningKey>,
        receiver: Address,
        nonce: u64,
    ) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
        let tx = tx(
            receiver,
            chain_id,
            DEFAULT_GAS_LIMIT,
            None,
            None,
            nonce,
            Some(20e9 as u128),
        );
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

/// Creates a type 2 transaction
fn tx(
    to: Address,
    chain_id: u64,
    gas: u64,
    data: Option<Bytes>,
    delegate_to: Option<SignedAuthorization>,
    nonce: u64,
    max_fee_per_gas: Option<u128>,
) -> TransactionRequest {
    TransactionRequest {
        nonce: Some(nonce),
        value: Some(U256::from(100)),
        to: Some(TxKind::Call(to)),
        gas: Some(gas),
        max_fee_per_gas,
        max_priority_fee_per_gas: Some(20e9 as u128),
        chain_id: Some(chain_id),
        input: TransactionInput { input: None, data },
        authorization_list: delegate_to.map(|addr| vec![addr]),
        ..Default::default()
    }
}
