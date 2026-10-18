//! Encrypted immutable complete task-context records; no public CAS/lineage file.
use super::PrivateGoalStore;
use crate::{SignedReadRequest, private_task_context::*};
#[cfg(unix)]
use serde::Serialize;
#[cfg(unix)]
use zeroize::Zeroizing;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateTaskContextError {
    Invalid,
    Conflict,
    Unavailable,
}
impl std::fmt::Display for PrivateTaskContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid private task context",
            Self::Conflict => "private task operation conflict",
            Self::Unavailable => "private task recovery required",
        })
    }
}
impl std::error::Error for PrivateTaskContextError {}
#[cfg(unix)]
const MAGIC: &[u8; 8] = b"O7CTX001";
#[cfg(unix)]
const DOMAIN: &str = "oasis7/private-task-record/v1";
#[cfg(unix)]
#[derive(Serialize)]
struct Scope<'a> {
    world: &'a str,
    genesis: &'a str,
    task: &'a str,
    agent: &'a str,
    owner: &'a str,
    owner_key: &'a str,
    owner_generation: oasis7_agent_api::game::DecimalU64,
    branch: &'a str,
    reorg: oasis7_agent_api::game::DecimalU64,
    subject: &'a str,
    operation: &'a str,
}
#[cfg(unix)]
impl<'a> Scope<'a> {
    fn new(
        f: &'a oasis7_agent_api::game::GameTaskFenceV1,
        subject: &'a str,
        operation: &'a str,
    ) -> Self {
        Self {
            world: &f.world_id,
            genesis: &f.genesis_digest,
            task: &f.task_id,
            agent: &f.agent_id,
            owner: &f.owner_binding,
            owner_key: &f.owner_public_key,
            owner_generation: f.owner_generation,
            branch: &f.branch_id,
            reorg: f.reorg_epoch,
            subject,
            operation,
        }
    }
}
impl PrivateGoalStore {
    #[cfg(unix)]
    fn context_name(&self, scope: &Scope<'_>) -> Result<String, PrivateTaskContextError> {
        let index = Zeroizing::new(blake3::derive_key(
            "oasis7/private-task-operation-index/v1",
            self.key.as_slice(),
        ));
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(scope)
            .map_err(|_| PrivateTaskContextError::Invalid)?;
        Ok(format!(
            "context-{}",
            blake3::keyed_hash(&index, &bytes).to_hex()
        ))
    }
    #[cfg(unix)]
    fn read_context_record(
        &self,
        scope: &Scope<'_>,
    ) -> Result<Option<PreparedTaskContextV1>, PrivateTaskContextError> {
        let name = self.context_name(scope)?;
        let aad =
            serde_cbor::to_vec(&(DOMAIN, &name)).map_err(|_| PrivateTaskContextError::Invalid)?;
        let Some(bytes) = super::unix::read_bounded_record(
            &self.directory,
            &self.key,
            &name,
            MAGIC,
            &aad,
            MAX_PRIVATE_CONTEXT_BYTES,
        )
        .map_err(|_| PrivateTaskContextError::Unavailable)?
        else {
            return Ok(None);
        };
        let record: PreparedTaskContextV1 =
            serde_cbor::from_slice(&bytes).map_err(|_| PrivateTaskContextError::Unavailable)?;
        record
            .validate()
            .map_err(|_| PrivateTaskContextError::Unavailable)?;
        Ok(Some(record))
    }
    pub fn lookup_task_context(
        &self,
        proof: &SignedReadRequest<TaskContextPrepareV1>,
    ) -> Result<Option<PreparedTaskContextV1>, PrivateTaskContextError> {
        verify_prepare(proof).map_err(|_| PrivateTaskContextError::Invalid)?;
        #[cfg(unix)]
        {
            let value = self.read_context_record(&Scope::new(
                &proof.request.fence,
                &proof.subject_public_key,
                &proof.request.client_operation_id,
            ))?;
            if let Some(record) = &value {
                record
                    .validate_against(proof)
                    .map_err(|_| PrivateTaskContextError::Conflict)?;
            }
            Ok(value)
        }
        #[cfg(not(unix))]
        {
            Err(PrivateTaskContextError::Unavailable)
        }
    }
    pub fn prepare_task_context(
        &self,
        proof: &SignedReadRequest<TaskContextPrepareV1>,
        snapshot: GoalExecutionSnapshotV1,
    ) -> Result<PreparedTaskContextV1, PrivateTaskContextError> {
        if let Some(record) = self.lookup_task_context(proof)? {
            return Ok(record);
        }
        #[cfg(unix)]
        {
            let record = crate::private_task_context::create(proof.clone(), snapshot)
                .map_err(|_| PrivateTaskContextError::Invalid)?;
            let name = self.context_name(&Scope::new(
                &proof.request.fence,
                &proof.subject_public_key,
                &proof.request.client_operation_id,
            ))?;
            let aad = serde_cbor::to_vec(&(DOMAIN, &name))
                .map_err(|_| PrivateTaskContextError::Invalid)?;
            let bytes = Zeroizing::new(
                serde_cbor::to_vec(&record).map_err(|_| PrivateTaskContextError::Invalid)?,
            );
            super::unix::publish_bounded_record(
                &self.directory,
                &self.key,
                &name,
                MAGIC,
                &aad,
                &bytes,
                MAX_PRIVATE_CONTEXT_BYTES,
            )
            .map_err(|_| PrivateTaskContextError::Unavailable)?;
            self.lookup_task_context(proof)?
                .ok_or(PrivateTaskContextError::Unavailable)
        }
        #[cfg(not(unix))]
        {
            let _ = snapshot;
            Err(PrivateTaskContextError::Unavailable)
        }
    }
    /// The caller must verify current executor and canonical active-prefix BEFORE decrypt.
    pub fn get_task_context(
        &self,
        request: &SignedReadRequest<TaskContextGetV1>,
    ) -> Result<PreparedTaskContextV1, PrivateTaskContextError> {
        verify_get(request).map_err(|_| PrivateTaskContextError::Invalid)?;
        #[cfg(unix)]
        {
            let record = self
                .read_context_record(&Scope::new(
                    &request.request.context.fence,
                    &request.subject_public_key,
                    &request.request.prepare_operation_id,
                ))?
                .ok_or(PrivateTaskContextError::Unavailable)?;
            if record.identity != request.request.context
                || record.prepare_proof.request.client_operation_id
                    != request.request.prepare_operation_id
                || record.prepare_proof.subject_public_key != request.subject_public_key
            {
                return Err(PrivateTaskContextError::Conflict);
            }
            Ok(record)
        }
        #[cfg(not(unix))]
        {
            Err(PrivateTaskContextError::Unavailable)
        }
    }
}
