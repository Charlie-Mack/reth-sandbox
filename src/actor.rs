//! Lightweight representation of EOAs used to sign the synthetic load.

use alloy_primitives::{Address, hex};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use k256::ecdsa::SigningKey;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

/// Maintains the deterministic deployer plus a collection of ephemeral EOAs
/// that will drive transaction load.
pub struct ActorPool {
    deployer: Actor,
    actors: Vec<Actor>,
}

impl ActorPool {
    /// Instantiate the pool with a genesis deployer that has all the funds
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

    /// Populate the pool with fresh EOAs created in parallel.
    pub fn generate_actors(&mut self, num_of_actors: u64) {
        let actors = (0..num_of_actors)
            .into_par_iter()
            .map(|_| Actor::new())
            .collect::<Vec<Actor>>();
        self.actors.extend(actors);
    }

    /// Return signer + nonce info for an actor at index.
    pub fn actor_info(&self, index: usize) -> (&LocalSigner<SigningKey>, u64) {
        (self.actors[index].signer(), self.actors[index].nonce)
    }

    /// Convenience to access the actor's address.
    pub fn actor_address(&self, index: usize) -> Address {
        self.actors[index].address()
    }

    /// Deployer accessor.
    pub fn deployer(&self) -> &Actor {
        &self.deployer
    }

    /// Returns signer + nonce pair for the deployer.
    pub fn deployer_info(&self) -> (&LocalSigner<SigningKey>, u64) {
        (self.deployer.signer(), self.deployer.nonce)
    }

    /// Increment the deployer nonce after a batch of txs has been emitted.
    pub fn increment_deployer_nonce_by(&mut self, amount: u64) {
        self.deployer.increment_nonce_by(amount);
    }

    /// Atomically fetch the actor nonce and increment it by `amount`.
    pub fn get_and_increment_nonce_by(&mut self, index: usize, amount: u64) -> u64 {
        let nonce = self.actors[index].nonce();
        self.actors[index].increment_nonce_by(amount);
        nonce
    }

    /// Total number of available actors.
    pub fn len(&self) -> usize {
        self.actors.len()
    }
}

/// Simple wrapper around [`LocalSigner`] that tracks nonce mutations.
#[derive(Debug, Clone)]
pub struct Actor {
    signer: LocalSigner<SigningKey>,
    nonce: u64,
}

impl Actor {
    /// Create a random local signer with zero nonce.
    pub fn new() -> Self {
        let signer = LocalSigner::random();
        Self { signer, nonce: 0 }
    }

    /// Returns the EOA address.
    pub fn address(&self) -> Address {
        self.signer.address().clone()
    }

    /// Returns the signer so callers can sign transactions.
    pub fn signer(&self) -> &LocalSigner<SigningKey> {
        &self.signer
    }

    /// Current nonce.
    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    /// Bump the nonce by an arbitrary amount (handy for batched approvals).
    pub fn increment_nonce_by(&mut self, amount: u64) {
        self.nonce += amount;
    }

    /// Predict the contract address created by this actor at a specific nonce.
    pub fn contract_address(&self, nonce: u64) -> Address {
        self.address().create(nonce)
    }
}
