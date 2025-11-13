use alloy_consensus::{EthereumTxEnvelope, TxEip4844};
use alloy_network::TxSignerSync;
use alloy_primitives::{Bytes, TxKind, U256};
use alloy_rpc_types_eth::{TransactionInput, TransactionRequest};

use alloy_signer_local::LocalSigner;
use k256::ecdsa::SigningKey;
use reth_ethereum::TransactionSigned;
use reth_primitives_traits::Recovered;

pub const DEFAULT_GAS_LIMIT: u64 = 10_000_000;

pub fn tx(
    sender: &LocalSigner<SigningKey>,
    nonce: u64,
    to: TxKind,
    value: Option<U256>,
    data: Option<Bytes>,
) -> Recovered<EthereumTxEnvelope<TxEip4844>> {
    let tx = TransactionRequest {
        nonce: Some(nonce),
        value: value,
        to: Some(to),
        gas: Some(DEFAULT_GAS_LIMIT),
        max_fee_per_gas: Some(20e9 as u128),
        max_priority_fee_per_gas: Some(20e9 as u128),
        chain_id: Some(2600u64), //FIXME
        input: TransactionInput {
            input: None,
            data: data,
        },
        authorization_list: None,
        ..Default::default()
    };
    sign_tx(sender, tx)
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
