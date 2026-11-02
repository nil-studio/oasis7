//! Fixed actual HTTP-stage coordinator. All permanent claim and private-file
//! I/O occurs outside the caller's Node/publication/cache locks.
use crate::formal_read_error::{FormalReadError, FormalReadResult};
use crate::formal_read_service::{
    AuthorizedResourceArchiveReadV1, PreparedResourceWorldReadV1, ResourceCurrentDeliveryCutV1,
    ResourceGoalArchiveReaderV2, VerifiedResourceGoalArchiveV2,
};
use crate::private_goal_store::PrivateGoalStore;
use crate::resource_world_http_codec_v1::ResourceWorldHttpOriginalV1;
use crate::resource_world_paid_ledger_v1::{
    ConsumedWorldResourceAttemptV1, WorldResourcePaidLedgerV1,
};
use oasis7_client_api::world_service::{WorldIdentity, private_goal::PreparedGoalReceiptV1};
use std::sync::Mutex;
/// Configured one-way World-local service; no HTTP input selects its stores,
/// issuer, external origin, key or physical-attempt namespace.
pub struct ResourceWorldReadServiceV1 {
    world: WorldIdentity,
    node_public_key: String,
    ledger: Mutex<WorldResourcePaidLedgerV1>,
    archive: ResourceGoalArchiveReaderV2,
    goals: PrivateGoalStore,
}
impl ResourceWorldReadServiceV1 {
    pub fn from_configured_stores(
        world: WorldIdentity,
        node_public_key: String,
        ledger: WorldResourcePaidLedgerV1,
        archive: ResourceGoalArchiveReaderV2,
        goals: PrivateGoalStore,
    ) -> FormalReadResult<Self> {
        ledger
            .validate_static_service_identity(&world, &node_public_key)
            .map_err(FormalReadError::Integrity)?;
        Ok(Self {
            world,
            node_public_key,
            ledger: Mutex::new(ledger),
            archive,
            goals,
        })
    }
    /// Called before the handler obtains a Current cut or touches archive,
    /// private Goal store or quote. The lock covers this store's durable commit,
    /// never Node/Viewer/publication state or a network operation.
    pub fn claim_http(
        &self,
        path: &str,
        body: &[u8],
        header: &str,
    ) -> FormalReadResult<ConsumedWorldResourceAttemptV1> {
        let original = ResourceWorldHttpOriginalV1::decode(path, body, header)
            .map_err(FormalReadError::RequestRejected)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| FormalReadError::NotReady("Resource World ledger unavailable".into()))?;
        ledger
            .validate_static_service_identity(&self.world, &self.node_public_key)
            .map_err(FormalReadError::Integrity)?;
        original
            .claim(&mut ledger)
            .map_err(FormalReadError::GrantRejected)
    }
    /// This method is invoked only after the caller has released the cache lock
    /// holding the opaque cut that created `authorized`. It reenters the actual
    /// live floor before any private reads, then releases that separate lock.
    pub fn load_original_goal(
        &self,
        authorized: &AuthorizedResourceArchiveReadV1,
        consumed: &ConsumedWorldResourceAttemptV1,
    ) -> FormalReadResult<(VerifiedResourceGoalArchiveV2, PreparedGoalReceiptV1)> {
        authorized.validate_original(consumed)?;
        {
            let ledger = self.ledger.lock().map_err(|_| {
                FormalReadError::NotReady("Resource World ledger unavailable".into())
            })?;
            ledger
                .validate_delivery(consumed)
                .map_err(FormalReadError::GrantRejected)?;
        }
        let archive = self.archive.load_exact(authorized)?;
        let goal = authorized.load_private_goal(&self.goals)?;
        Ok((archive, goal))
    }
    /// The caller first captures fresh opaque-cut metadata and releases its
    /// cache lock. This actual current floor and the original deadline are
    /// checked again before borrowing the fixed Node signing key.
    pub fn finish(
        &self,
        prepared: PreparedResourceWorldReadV1,
        fresh: &ResourceCurrentDeliveryCutV1,
        signer: &ed25519_dalek::SigningKey,
    ) -> FormalReadResult<Vec<u8>> {
        if hex::encode(signer.verifying_key().to_bytes()) != self.node_public_key {
            return Err(FormalReadError::Integrity(
                "Resource fixed Node signing key".into(),
            ));
        }
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| FormalReadError::NotReady("Resource World ledger unavailable".into()))?;
        prepared.finish(fresh, &ledger, signer)
    }
}
