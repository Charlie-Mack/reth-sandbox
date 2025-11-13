use alloy_primitives::{Address, hex};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use k256::ecdsa::SigningKey;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub struct ActorPool {
    deployer: Actor,
    actors: Vec<Actor>,
}

impl ActorPool {
    pub fn new(private_key: &str, address: Address, chain_id: u64) -> Self {
        let actors = Vec::new();

        let mut privkey_bytes = [0u8; 32];
        hex::decode_to_slice(private_key, &mut privkey_bytes).unwrap();

        let signing_key =
            SigningKey::from_slice(&privkey_bytes).expect("failed to parse signing key");

        let signer = PrivateKeySigner::new_with_credential(signing_key, address, Some(chain_id));

        let deployer = Actor {
            signer: signer,
            nonce: 0,
        };

        Self { deployer, actors }
    }

    pub fn generate_actors(&mut self, num_of_actors: u64) {
        let actors = (0..num_of_actors)
            .into_par_iter()
            .map(|_| Actor::new())
            .collect::<Vec<Actor>>();
        self.actors.extend(actors);
    }

    pub fn actor_info(&self, index: usize) -> (&LocalSigner<SigningKey>, u64) {
        (self.actors[index].signer(), self.actors[index].nonce)
    }

    pub fn actor_address(&self, index: usize) -> Address {
        self.actors[index].address()
    }

    pub fn deployer(&self) -> &Actor {
        &self.deployer
    }

    pub fn deployer_info(&self) -> (&LocalSigner<SigningKey>, u64) {
        (self.deployer.signer(), self.deployer.nonce)
    }

    pub fn increment_deployer_nonce_by(&mut self, amount: u64) {
        self.deployer.increment_nonce_by(amount);
    }

    pub fn get_and_increment_nonce_by(&mut self, index: usize, amount: u64) -> u64 {
        let nonce = self.actors[index].nonce();
        self.actors[index].increment_nonce_by(amount);
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

    pub fn signer(&self) -> &LocalSigner<SigningKey> {
        &self.signer
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn increment_nonce_by(&mut self, amount: u64) {
        self.nonce += amount;
    }

    pub fn contract_address(&self, nonce: u64) -> Address {
        self.address().create(nonce)
    }
}
