use alloy_primitives::{Address, hex};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use k256::ecdsa::SigningKey;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub struct ActorPool {
    actors: Vec<Actor>,
    actors_funded: u64,
}

impl ActorPool {
    pub fn new_with_genesis(private_key: &str, address: Address, chain_id: u64) -> Self {
        let mut actors = Vec::new();

        let mut privkey_bytes = [0u8; 32];
        hex::decode_to_slice(private_key, &mut privkey_bytes).unwrap();

        let signing_key =
            SigningKey::from_slice(&privkey_bytes).expect("failed to parse signing key");

        let signer = PrivateKeySigner::new_with_credential(signing_key, address, Some(chain_id));

        let genesis_actor = Actor {
            signer: signer,
            nonce: 0,
        };

        actors.push(genesis_actor);

        Self {
            actors,
            actors_funded: 1,
        }
    }

    pub fn generate_actors(&mut self, num_of_actors: u64) {
        let actors = (0..num_of_actors)
            .into_par_iter()
            .map(|_| Actor::new())
            .collect::<Vec<Actor>>();
        self.actors.extend(actors);
    }

    pub fn actors_funded(&self) -> u64 {
        self.actors_funded
    }

    pub fn actor_info(&self, index: usize) -> (LocalSigner<SigningKey>, u64) {
        (self.actors[index].signer(), self.actors[index].nonce)
    }

    pub fn actor_address(&self, index: usize) -> Address {
        self.actors[index].address()
    }

    pub fn genesis_actor_info(&self) -> (LocalSigner<SigningKey>, u64) {
        (self.actors[0].signer(), self.actors[0].nonce)
    }

    pub fn increment_genesis_actor_nonce_by(&mut self, amount: u64) {
        self.actors[0].increment_nonce_by(amount);
    }

    pub fn increment_actor_nonce(&mut self, index: usize) {
        self.actors[index].increment_nonce();
    }

    pub fn increment_actors_funded_by(&mut self, amount: u64) {
        self.actors_funded += amount;
    }

    pub fn get_and_increment_nonce(&mut self, index: usize) -> u64 {
        let nonce = self.actors[index].nonce();
        self.increment_actor_nonce(index);
        nonce
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
        Self { signer, nonce: 0 }
    }

    pub fn address(&self) -> Address {
        self.signer.address().clone()
    }

    pub fn signer(&self) -> LocalSigner<SigningKey> {
        self.signer.clone()
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn increment_nonce(&mut self) {
        self.nonce += 1;
    }

    pub fn increment_nonce_by(&mut self, amount: u64) {
        self.nonce += amount;
    }
}
