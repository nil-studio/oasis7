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
        source,
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
    pub(super) source: Arc<dyn FormalArchiveSourceV1>,
}
impl VerifiedFormalArchiveTransferV1 {
    pub fn cut(&self) -> &FormalHeadRefV1 {
        &self.cut
    }
    pub fn digest(&self) -> Result<String, FormalError> {
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
            source: Arc::new(LocalArchiveSource {
                source: FileFormalArchiveSourceV1::new(self.store.path.clone())?,
                fallback: self.store.sources().to_vec(),
            }),
        })
    }
}

#[cfg(test)]
pub(super) fn bounded_local_for_test(
    source: FileFormalArchiveSourceV1,
    fallback: Vec<Arc<dyn FormalArchiveSourceV1>>,
) -> Arc<dyn FormalArchiveSourceV1> {
    Arc::new(LocalArchiveSource { source, fallback })
}
