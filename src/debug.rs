use alloy_primitives::{Address, map::HashMap};
use reth_db::cursor::{DbCursorRO, DbDupCursorRO};
use reth_db::{tables, transaction::DbTx};
use reth_provider::{DBProvider, StateProvider};
use tracing::info;

pub fn get_basic_account_info(state_provider: &dyn StateProvider, address: Address) {
    let account = state_provider.basic_account(&address).unwrap();
    let Some(account) = account else {
        info!(
            target: "sandbox::debug",
            "Account not found: {}",
            address
        );
        return;
    };

    info!(
        target: "sandbox::debug",
        "Account info: {:?}",
        account,
    );
}

pub fn get_contract_storage(
    provider: &impl DBProvider,
    state_provider: &dyn StateProvider,
    contract: Address,
) {
    let account = state_provider.basic_account(&contract).unwrap();
    let Some(account) = account else {
        info!(
            target: "sandbox::debug",
            "Contract not found: {}",
            contract
        );
        return;
    };

    info!(
        target: "sandbox::debug",
        "Contract info: {:?}",
        account,
    );

    let mut storage_cursor = provider
        .tx_ref()
        .cursor_dup_read::<tables::PlainStorageState>()
        .unwrap();
    let mut storage = HashMap::new();

    if let Some((_, first_entry)) = storage_cursor.seek_exact(contract).unwrap() {
        storage.insert(first_entry.key, first_entry.value);

        while let Some((_, entry)) = storage_cursor.next_dup().unwrap() {
            storage.insert(entry.key, entry.value);
        }
    }

    info!(
        target: "sandbox::debug",
        "Storage slots: {:?}",
        storage.len(),
    );

    //Pretty print the storage
    for (key, value) in storage.iter() {
        info!(
            target: "sandbox::debug",
            "{}: {}",
            key,
            value
        );
    }
}
