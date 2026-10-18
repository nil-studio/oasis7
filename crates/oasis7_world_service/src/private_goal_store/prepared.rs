//! One immutable encrypted record per owner/task/history/operation. The public
//! reference commits to the exact signed body with a private random salt.
use super::PrivateGoalStore;
use crate::private_goal_prepare::SignedGoalPrepareV1;
#[cfg(unix)]
use oasis7_agent_api::game::GameGoalPrepareV1;
use oasis7_agent_api::game::GameGoalPublishV1;
use serde::Serialize;
use zeroize::Zeroizing;

#[cfg(unix)]
const DOMAIN: &str = "oasis7/private-prepared-goal/v1";
#[cfg(unix)]
const MAGIC: &[u8; 8] = b"O7PREP01";

use oasis7_client_api::world_service::private_goal::prepared_goal_commitment as commitment;
pub use oasis7_client_api::world_service::private_goal::{
    PreparedGoalError, PreparedGoalReceiptV1,
};

// Revisions, nonce, body and signature are deliberately excluded. Changing any
// of them under the same operation must conflict rather than select a new file.
#[derive(Serialize)]
#[cfg(unix)]
struct OperationScope<'a> {
    world_id: &'a str,
    genesis_digest: &'a str,
    task_id: &'a str,
    agent_id: &'a str,
    owner_public_key: &'a str,
    owner_binding: &'a str,
    owner_generation: oasis7_agent_api::game::DecimalU64,
    branch_id: &'a str,
    reorg_epoch: oasis7_agent_api::game::DecimalU64,
    operation_id: &'a str,
}
#[cfg(unix)]
impl<'a> OperationScope<'a> {
    fn prepare(r: &'a GameGoalPrepareV1) -> Self {
        Self {
            world_id: &r.world_id,
            genesis_digest: &r.genesis_digest,
            task_id: &r.task_id,
            agent_id: &r.agent_id,
            owner_public_key: &r.owner_public_key,
            owner_binding: &r.owner_binding,
            owner_generation: r.owner_generation,
            branch_id: &r.branch_id,
            reorg_epoch: r.reorg_epoch,
            operation_id: &r.client_operation_id,
        }
    }
    fn publish(r: &'a GameGoalPublishV1) -> Self {
        Self {
            world_id: &r.world_id,
            genesis_digest: &r.genesis_digest,
            task_id: &r.task_id,
            agent_id: &r.agent_id,
            owner_public_key: &r.owner_public_key,
            owner_binding: &r.owner_binding,
            owner_generation: r.owner_generation,
            branch_id: &r.branch_id,
            reorg_epoch: r.reorg_epoch,
            operation_id: &r.prepare_operation_id,
        }
    }
}
impl PrivateGoalStore {
    #[cfg(unix)]
    fn prepared_name(&self, scope: &OperationScope<'_>) -> Result<String, PreparedGoalError> {
        let index_key = Zeroizing::new(blake3::derive_key(
            "oasis7/private-goal-operation-index/v1",
            &*self.key,
        ));
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(scope)
            .map_err(|_| PreparedGoalError::Invalid)?;
        Ok(format!(
            "prepared-{}",
            blake3::keyed_hash(&index_key, &bytes).to_hex()
        ))
    }
    #[cfg(unix)]
    fn read_prepared(
        &self,
        scope: &OperationScope<'_>,
    ) -> Result<Option<PreparedGoalReceiptV1>, PreparedGoalError> {
        let name = self.prepared_name(scope)?;
        let aad = serde_cbor::to_vec(&(DOMAIN, &name)).map_err(|_| PreparedGoalError::Invalid)?;
        let Some(bytes) = super::unix::read_record(&self.directory, &self.key, &name, MAGIC, &aad)
            .map_err(|_| PreparedGoalError::Unavailable)?
        else {
            return Ok(None);
        };
        let record: PreparedGoalReceiptV1 =
            serde_cbor::from_slice(&bytes).map_err(|_| PreparedGoalError::Unavailable)?;
        record
            .validate()
            .map_err(|_| PreparedGoalError::Unavailable)?;
        if self.prepared_name(&OperationScope::prepare(&record.owner_proof.request))? != name {
            return Err(PreparedGoalError::Unavailable);
        }
        Ok(Some(record))
    }
    /// Owner authorization is the service's responsibility on every call.
    /// A retry may replay after activation; it does not re-create the goal.
    pub fn lookup_prepared(
        &self,
        proof: &SignedGoalPrepareV1,
    ) -> Result<Option<PreparedGoalReceiptV1>, PreparedGoalError> {
        proof.verify().map_err(|_| PreparedGoalError::Invalid)?;
        #[cfg(unix)]
        {
            let record = self.read_prepared(&OperationScope::prepare(&proof.request))?;
            if let Some(record) = &record {
                record.validate_against(proof)?;
            }
            Ok(record)
        }
        #[cfg(not(unix))]
        {
            Err(PreparedGoalError::Unavailable)
        }
    }
    /// Single-file durable commit. Concurrent callers read the immutable winner.
    pub fn prepare(
        &self,
        proof: &SignedGoalPrepareV1,
    ) -> Result<PreparedGoalReceiptV1, PreparedGoalError> {
        if let Some(record) = self.lookup_prepared(proof)? {
            return Ok(record);
        }
        #[cfg(unix)]
        {
            let mut salt = Zeroizing::new([0; 32]);
            getrandom::fill(&mut *salt).map_err(|_| PreparedGoalError::Unavailable)?;
            let record = PreparedGoalReceiptV1 {
                schema_revision: 1,
                goal_ref: commitment(&salt, proof)?,
                reference_salt: hex::encode(salt.as_slice()),
                owner_proof: proof.clone(),
            };
            let name = self.prepared_name(&OperationScope::prepare(&proof.request))?;
            let aad =
                serde_cbor::to_vec(&(DOMAIN, &name)).map_err(|_| PreparedGoalError::Invalid)?;
            let bytes = Zeroizing::new(
                serde_cbor::to_vec(&record).map_err(|_| PreparedGoalError::Invalid)?,
            );
            super::unix::publish_record(&self.directory, &self.key, &name, MAGIC, &aad, &bytes)
                .map_err(|_| PreparedGoalError::Unavailable)?;
            // Always validate/read the winner; no second index/body transaction.
            self.lookup_prepared(proof)?
                .ok_or(PreparedGoalError::Unavailable)
        }
        #[cfg(not(unix))]
        {
            Err(PreparedGoalError::Unavailable)
        }
    }
    /// Publish must be obtained from validated canonical owner control. No old
    /// prepare precondition is compared to the post-publication control revision.
    pub fn get_prepared(
        &self,
        publish: &GameGoalPublishV1,
    ) -> Result<PreparedGoalReceiptV1, PreparedGoalError> {
        publish.validate().map_err(|_| PreparedGoalError::Invalid)?;
        #[cfg(unix)]
        {
            let record = self
                .read_prepared(&OperationScope::publish(publish))?
                .ok_or(PreparedGoalError::Unavailable)?;
            record.validate_publish(publish)?;
            Ok(record)
        }
        #[cfg(not(unix))]
        {
            Err(PreparedGoalError::Unavailable)
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
