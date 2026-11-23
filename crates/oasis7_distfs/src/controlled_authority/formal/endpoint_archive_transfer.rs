//! Sealed authenticated index inheritance. Source objects are immutable and are
//! loaded one at a time; losing a referenced source fails closed, never converts
//! a previously bound request into authenticated absence.
use super::*;
use serde::{Deserialize, Serialize};
pub trait FormalArchiveSourceV1: Send + Sync {
    /// Explicit paid entry point. Legacy providers fail closed rather than
    /// allocating through the old whole-object or range adapters.
    fn read_object_bounded(
        &self,
        _id: &str,
        _max: usize,
        _memory: &oasis7_proto::memory_budget::MemoryBudget,
        _work: &FormalArchiveReadWorkV1,
        _cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
        Err(invalid("bounded formal archive provider unsupported"))
    }
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError>;
    fn read_range(
        &self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
        self.read_object(id)?
            .map(|bytes| FormalArchiveChunkV1::from_verified_bytes(id, &bytes, offset, limit))
            .transpose()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFormalArchiveTransferV1 {
    pub checkpoint_bytes: Vec<u8>,
    pub seal: fencing::EpochSealReceiptV1,
}
/// Remote object providers remain untrusted: the sealed checkpoint commits the
/// index and ancestry roots, and every accessed byte string is hash checked.
pub fn verify_formal_archive_transfer(
    envelope: &SignedFormalArchiveTransferV1,
    original: &VerifiedAuthorityV1,
    source: Arc<dyn FormalArchiveSourceV1>,
) -> Result<VerifiedFormalArchiveTransferV1, FormalError> {
    if envelope.checkpoint_bytes.len() > 64 * 1024 {
        return Err(invalid("formal archive checkpoint byte bound"));
    }
    original.definition().verify_policy(original.policy())?;
    let instance = &envelope.seal.body.instance;
    let expected = match instance.endpoint_role {
        EndpointRole::Primary => &original.definition().primary.instance,
        EndpointRole::Replica => &original.definition().replica.instance,
    };
    if instance != expected {
        return Err(invalid("formal archive original endpoint instance"));
    }
    let verdict = fencing::verify_epoch_seal(
        &envelope.seal,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: original.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: original
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(fencing_error)?;
    let body = Store::verify_public(&envelope.checkpoint_bytes, &instance.endpoint_public_key)?;
    let binding = Store::binding(&body, &envelope.checkpoint_bytes);
    if body.version != 1
        || body.instance_digest != instance.digest().map_err(fencing_error)?
        || body.authority_id != original.authority_id()?
        || body.activation_ref != original.activation_ref()
        || body.epoch != original.definition().epoch
        || body.seal.as_ref() != Some(verdict.seal_body())
        || binding != envelope.seal.provider.statement.checkpoint
        || body.head.position != verdict.seal_body().highest_complete.position
        || body.head.record_id != verdict.seal_body().highest_complete.decision_hash
    {
        return Err(invalid(
            "formal archive signed checkpoint/monotonic seal context",
        ));
    }
    let pending = verify_recovery_handoff_source(&body, original, source.as_ref())?;
    Ok(VerifiedFormalArchiveTransferV1 {
        predecessor_authority_id: body.authority_id,
        instance_digest: body.instance_digest,
        seal_digest: verdict.seal_digest().into(),
        operation_id: verdict.seal_body().recovery_operation.clone(),
        next_epoch_floor: verdict.seal_body().next_epoch_floor,
        tail: verdict.seal_body().tail.clone(),
        cut: body.head,
        index: body.index,
        head_node: body.head_node.or(body.imported_head_node),
        recovery_plan: body.recovery_plan,
        recovery_handoff: body.recovery_handoff,
        pending,
        source,
        checkpoint_bytes: envelope.checkpoint_bytes.clone(),
        endpoint_public_key: instance.endpoint_public_key.clone(),
        profile_version: original.definition().profile_version,
        recovery_copy_max_bytes: None,
    })
}
struct LocalArchiveSource {
    source: FileFormalArchiveSourceV1,
    fallback: Vec<Arc<dyn FormalArchiveSourceV1>>,
}
impl FormalArchiveSourceV1 for LocalArchiveSource {
    fn read_object_bounded(
        &self,
        id: &str,
        max: usize,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
        super::archive_bounded_read::read_archive_chain_bounded_v1(
            &self.source,
            &self.fallback,
            id,
            max,
            memory,
            work,
            cancelled,
        )
    }
    fn read_range(
        &self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
        if let Some(chunk) = self.source.read_range(id, offset, limit)? {
            return Ok(Some(chunk));
        }
        for source in &self.fallback {
            if let Some(chunk) = source.read_range(id, offset, limit)? {
                return Ok(Some(chunk));
            }
        }
        Ok(None)
    }
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        if let Some(bytes) = self.source.read_object(id)? {
            return Ok(Some(bytes));
        }
        for source in &self.fallback {
            if let Some(bytes) = source.read_object(id)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }
}
/// No public constructor: only a native sealed endpoint with a checked trusted
/// verdict may authorize its authenticated permanent index as a recovery base.
pub struct VerifiedFormalArchiveTransferV1 {
    pub(super) predecessor_authority_id: String,
    pub(super) instance_digest: String,
    pub(super) seal_digest: String,
    pub(super) operation_id: String,
    pub(super) next_epoch_floor: u64,
    pub(super) tail: fencing::SealedTailV1,
    pub(super) cut: FormalHeadRefV1,
    pub(super) index: Option<String>,
    pub(super) head_node: Option<String>,
    pub(super) recovery_plan: Option<SignedRecoveryPlanV1>,
    pub(super) recovery_handoff: Option<String>,
    pub(super) pending: Option<RecoveryPendingHandoffV1>,
    pub(super) source: Arc<dyn FormalArchiveSourceV1>,
    checkpoint_bytes: Vec<u8>,
    endpoint_public_key: String,
    profile_version: u32,
    recovery_copy_max_bytes: Option<u64>,
}
impl VerifiedFormalArchiveTransferV1 {
    /// Explicit local copy budget for one recovery bootstrap attempt. This is
    /// not a protected transfer allowance and does not cap retained history.
    pub fn with_recovery_copy_budget(mut self, max_bytes: u64) -> Result<Self, FormalError> {
        if max_bytes == 0 {
            return Err(invalid(
                "recovery copy requires a positive explicit operation budget",
            ));
        }
        self.recovery_copy_max_bytes = Some(max_bytes);
        Ok(self)
    }
    pub(super) fn recovery_copy_budget(&self) -> Result<u64, FormalError> {
        self.recovery_copy_max_bytes
            .ok_or_else(|| invalid("schema3 recovery requires explicit recovery_copy_max_bytes"))
    }
    /// The endpoint already holds its native owner lock. A successful copy is
    /// followed by a fresh graph walk reading only the target's durable bytes.
    pub(super) fn materialize_recovery_closure(
        &self,
        path: &std::path::Path,
    ) -> Result<(), FormalError> {
        let budget = self.recovery_copy_budget()?;
        if self.recovery_plan.is_none() || self.tail != fencing::SealedTailV1::empty() {
            return Err(invalid(
                "recovery copy requires the exact planned drained source",
            ));
        }
        let copied = store::durable_set::copy_authenticated_closure(
            path,
            &self.checkpoint_bytes,
            &self.endpoint_public_key,
            self.source.as_ref(),
            self.profile_version,
            budget,
            true,
        )?;
        let local = FileFormalArchiveSourceV1::new(path.to_path_buf())?;
        let readback = store::durable_set::copy_authenticated_closure(
            path,
            &self.checkpoint_bytes,
            &self.endpoint_public_key,
            &local,
            self.profile_version,
            budget,
            false,
        )?;
        if copied != readback {
            return Err(invalid(
                "recovery copy independent local closure inventory differs",
            ));
        }
        Ok(())
    }
    pub fn cut(&self) -> &FormalHeadRefV1 {
        &self.cut
    }
    pub fn digest(&self) -> Result<String, FormalError> {
        if let Some(plan) = &self.recovery_plan {
            return hash(
                "oasis7.formal.sealed_archive_transfer.v2",
                &(
                    &self.predecessor_authority_id,
                    &self.instance_digest,
                    &self.seal_digest,
                    &self.operation_id,
                    self.next_epoch_floor,
                    &self.tail,
                    &self.cut,
                    &self.index,
                    &self.head_node,
                    plan,
                    &self.recovery_handoff,
                ),
            );
        }
        hash(
            "oasis7.formal.sealed_archive_transfer.v1",
            &(
                &self.predecessor_authority_id,
                &self.instance_digest,
                &self.seal_digest,
                &self.operation_id,
                self.next_epoch_floor,
                &self.tail,
                &self.cut,
                &self.index,
                &self.head_node,
            ),
        )
    }
}
impl FileFormalEndpoint {
    pub fn archive_manifest(&mut self) -> Result<SignedFormalArchiveTransferV1, FormalError> {
        let mut guard = self.guard()?;
        let body = self
            .store
            .body
            .seal
            .as_ref()
            .ok_or_else(|| invalid("formal archive manifest requires native seal"))?;
        let seal = self.signed_seal(body.clone(), guard.as_mut())?;
        Ok(SignedFormalArchiveTransferV1 {
            checkpoint_bytes: self.store.checkpoint_bytes()?,
            seal,
        })
    }
    pub fn archive_object(&mut self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        let _guard = self.guard()?;
        let bytes = self.store.raw_object(id)?;
        if let Some(bytes) = &bytes
            && (bytes.len() > 64 * 1024 * 1024 || blake3::hash(bytes).to_hex().as_str() != id)
        {
            self.poisoned = true;
            return Err(invalid("formal archived object checksum"));
        }
        Ok(bytes)
    }
    pub fn export_sealed_archive(
        &mut self,
        seal: &fencing::FencingVerdict,
    ) -> Result<VerifiedFormalArchiveTransferV1, FormalError> {
        let _guard = self.guard()?;
        let body = self
            .store
            .body
            .seal
            .as_ref()
            .ok_or_else(|| invalid("formal archive transfer requires native seal"))?;
        if body != seal.seal_body()
            || &body.instance != self.instance()
            || seal.provider_public_key() != self.authority.policy().fencing_provider_public_key
            || !self
                .authority
                .policy()
                .authorized_fencing_resource_bindings
                .iter()
                .any(|resource| resource == seal.resource_binding())
        {
            return Err(FormalError::FencingUnproven);
        }
        let source: Arc<dyn FormalArchiveSourceV1> = Arc::new(LocalArchiveSource {
            source: FileFormalArchiveSourceV1::new(self.store.path.clone())?,
            fallback: self.store.sources().to_vec(),
        });
        let pending =
            verify_recovery_handoff_source(&self.store.body, &self.authority, source.as_ref())?;
        Ok(VerifiedFormalArchiveTransferV1 {
            predecessor_authority_id: self.authority.authority_id()?,
            instance_digest: self.instance().digest().map_err(fencing_error)?,
            seal_digest: seal.seal_digest().into(),
            operation_id: body.recovery_operation.clone(),
            next_epoch_floor: body.next_epoch_floor,
            tail: body.tail.clone(),
            cut: self.store.body.head.clone(),
            index: self.store.body.index.clone(),
            head_node: self.store.body.head_node.clone().or(self
                .store
                .body
                .imported_head_node
                .clone()),
            recovery_plan: self.store.body.recovery_plan.clone(),
            recovery_handoff: self.store.body.recovery_handoff.clone(),
            pending,
            source,
            checkpoint_bytes: self.store.checkpoint_bytes()?,
            endpoint_public_key: self.public_key.clone(),
            profile_version: self.authority.definition().profile_version,
            recovery_copy_max_bytes: None,
        })
    }
}

fn verify_recovery_handoff_source(
    body: &HotBody,
    authority: &VerifiedAuthorityV1,
    source: &dyn FormalArchiveSourceV1,
) -> Result<Option<RecoveryPendingHandoffV1>, FormalError> {
    let Some(plan) = &body.recovery_plan else {
        if body
            .seal
            .as_ref()
            .is_some_and(|seal| seal.recovery_plan.is_some())
        {
            return Err(invalid(
                "sealed recovery plan private checkpoint binding missing",
            ));
        }
        return Ok(None);
    };
    plan.verify(authority)?;
    let ack = body
        .writer_plan_ack
        .as_ref()
        .ok_or_else(|| invalid("sealed archive original writer acknowledgement missing"))?;
    ack.verify(plan, authority)?;
    let writer = body
        .seal
        .as_ref()
        .and_then(|seal| seal.writer_seal.as_ref())
        .ok_or_else(|| invalid("sealed archive independent writer epoch seal missing"))?;
    writer.verify_with_ack(plan, authority, ack)?;
    if body
        .seal
        .as_ref()
        .and_then(|seal| seal.recovery_plan.as_ref())
        != Some(plan)
        || writer.drained.verify(plan, authority)? != body.head
        || body.pending.is_some()
        || body.decision_head.is_some()
        || body.recovery_progress.is_some()
        || body.recovery_drained.is_some()
    {
        return Err(invalid(
            "sealed archive and original recovery plan disagree",
        ));
    }
    let id = body
        .recovery_handoff
        .as_deref()
        .ok_or_else(|| invalid("sealed recovery handoff reference missing"))?;
    hash_hex(id)?;
    let bytes = source
        .read_object(id)?
        .ok_or_else(|| invalid("sealed recovery original pending object missing"))?;
    if bytes.len() > RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1
        || blake3::hash(&bytes).to_hex().as_str() != id
    {
        return Err(invalid("sealed recovery pending object hash/size"));
    }
    let pending: RecoveryPendingHandoffV1 =
        serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))?;
    if pending.root()? != plan.body.pending_handoff_root {
        return Err(invalid(
            "sealed recovery pending collection differs from issuer plan",
        ));
    }
    Ok(Some(pending))
}

#[cfg(test)]
pub(super) fn bounded_local_for_test(
    source: FileFormalArchiveSourceV1,
    fallback: Vec<Arc<dyn FormalArchiveSourceV1>>,
) -> Arc<dyn FormalArchiveSourceV1> {
    Arc::new(LocalArchiveSource { source, fallback })
}
