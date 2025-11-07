use alloy_consensus::{EthereumTxEnvelope, TxEip4844, TxEnvelope};
use alloy_network::EthereumWallet;
use alloy_primitives::{
    Address, U256, address, hex,
    map::{DefaultHashBuilder, HashMap},
};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use k256::ecdsa::SigningKey;
use reth_ethereum::TransactionSigned;
use reth_primitives_traits::Recovered;

use crate::transaction::TransactionOperations;

const GENESIS_PRIVATE_KEY: &str =
    "5ba8b410b0d2161dacd190f8aa6dfbc54ad1c84c67ee3e80611d92cc3fda8abd";

pub struct ActorPool {
    actors: Vec<Actor>,
    address_to_index: HashMap<Address, usize>,
}

impl ActorPool {
    pub fn new_with_genesis() -> Self {
        let mut actors = Vec::new();
        let mut address_to_index = HashMap::default();

        let mut privkey_bytes = [0u8; 32];
        hex::decode_to_slice(GENESIS_PRIVATE_KEY, &mut privkey_bytes).unwrap();
        let signing_key =
            SigningKey::from_slice(&privkey_bytes).expect("failed to parse signing key");
        let signer = PrivateKeySigner::new_with_credential(
            signing_key,
            address!("0xFaa235fA90514d9083d0aa61878eBEb5Cf94FCD7"),
            Some(2600),
        );

        let wallet = EthereumWallet::from(signer.clone());

        let genesis_actor = Actor {
            signer: signer,
            nonce: 0,
        };

        address_to_index.insert(genesis_actor.signer.address().clone(), 0);
        actors.push(genesis_actor);

        Self {
            actors,
            address_to_index,
        }
    }

    pub fn get_actor(&mut self, address: Address) -> eyre::Result<&mut Actor> {
        let index = self
            .address_to_index
            .get(&address)
            .ok_or(eyre::eyre!("Actor not found"))?;
        Ok(&mut self.actors[*index])
    }

    pub fn get_actor_address(&mut self, index: usize) -> Address {
        self.actors[index].address()
    }

    pub fn get_genesis_actor(&mut self) -> &mut Actor {
        &mut self.actors[0]
    }

    pub fn genesis_actor(&self) -> &Actor {
        &self.actors[0]
    }

    pub fn genesis_actor_info(&self) -> (LocalSigner<SigningKey>, u64) {
        (self.actors[0].signer(), self.actors[0].nonce)
    }

    pub fn genesis_actor_signer(&self) -> LocalSigner<SigningKey> {
        self.actors[0].signer()
    }

    pub fn revert_genesis_actor_nonce(&mut self) {
        self.actors[0].nonce -= 1;
    }

    pub fn get_actor_signer(&self, index: usize) -> &LocalSigner<SigningKey> {
        &self.actors[index].signer
    }

    pub fn increment_actor_nonce(&mut self, index: usize) {
        self.actors[index].nonce += 1;
    }

    pub fn decrement_actor_nonce(&mut self, index: usize) {
        self.actors[index].nonce -= 1;
    }

    pub fn increment_genesis_actor_nonce(&mut self) {
        self.actors[0].nonce += 1;
    }

    pub fn genesis_actor_nonce(&self) -> u64 {
        self.actors[0].nonce
    }

    // pub fn generate_and_add_actor(&mut self) -> eyre::Result<Address> {
    //     let actor = Actor::new();
    //     let address = actor.address();
    //     let new_index = self.actors.len();
    //     self.actors.push(actor);
    //     self.address_to_index.insert(address, new_index);

    //     Ok(address.clone())
    // }

    pub fn add_actor_instance(&mut self, actor: Actor) {
        let address = actor.address();
        let new_index = self.actors.len();
        self.actors.push(actor);
        self.address_to_index.insert(address, new_index);
    }

    pub fn len(&self) -> usize {
        self.actors.len()
    }
}

#[derive(Debug, Clone)]
pub struct Actor {
    signer: LocalSigner<SigningKey>,
    nonce: u64,
}

impl Actor {
    pub fn new() -> Self {
        let signer = LocalSigner::random();
        let wallet = EthereumWallet::from(signer.clone());
        Self { signer, nonce: 0 }
    }

    pub fn address(&self) -> Address {
        self.signer.address().clone()
    }

    pub fn signer(&self) -> LocalSigner<SigningKey> {
        self.signer.clone()
    }

    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }

    pub fn decrement_nonce(&mut self) {
        self.nonce -= 1;
    }
}
