//! Cold ordinary business output: locate by the current body's declared graph,
//! then read the actual content-addressed originals and verify signed roots.
use super::formal_result::{
    PaidFormalBusinessDescriptorV1, PaidFormalBusinessErrorV1,
    decode_formal_business_descriptor_bounded_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, FormalRecordKindV1, VerifiedFormalCommitV1,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        business_index::{
            PaidBusinessIndexErrorV1, PaidBusinessIndexNodeReaderV1,
            PaidDecodedBusinessIndexDeltaV1, decode_business_index_delta_bounded_v1,
            verify_business_index_delta_bounded_v1,
        },
    },
    controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord},
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_business_output::{
        BUSINESS_OUTPUT_DOMAIN_V1, BUSINESS_OUTPUT_MAX_BYTES, BusinessOutputLocatorV1,
    },
    incremental_execution::ExecutionError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use std::sync::Arc;
/// IO only. The concrete Host adapter must debit its actual physical/logical
/// lanes from this same Common; there is deliberately no legacy read fallback.
pub trait PaidIwsBusinessOriginalReaderV1 {
    type Error;
    fn allocation_budget(&self) -> &oasis7_proto::memory_budget::MemoryBudget;
    fn allocation_class(&self) -> MemoryClass;
    fn read_business_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error>;
}
#[derive(Debug)]
pub enum IwsBusinessMaterialErrorV1<E> {
    Invalid(&'static str),
    Context(FormalError),
    Object(E),
    Descriptor(PaidFormalBusinessErrorV1),
    Index(PaidBusinessIndexErrorV1),
    Execution(ExecutionError),
}
impl<E: std::fmt::Debug> std::fmt::Display for IwsBusinessMaterialErrorV1<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IWS business material: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for IwsBusinessMaterialErrorV1<E> {}
/// Verified business DATA for one actual accepted head, not a serving permit.
pub struct PaidIwsBusinessOutputV1 {
    descriptor: PaidFormalBusinessDescriptorV1,
    delta: PaidDecodedBusinessIndexDeltaV1,
    locator: BusinessOutputLocatorV1,
    record: [u8; 32],
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidIwsBusinessOutputV1 {
    pub fn descriptor(&self) -> &PaidFormalBusinessDescriptorV1 {
        &self.descriptor
    }
    pub fn delta(&self) -> &PaidDecodedBusinessIndexDeltaV1 {
        &self.delta
    }
    pub fn locator(&self) -> &BusinessOutputLocatorV1 {
        &self.locator
    }
    pub fn validate_context(
        &self,
        head: &VerifiedFormalCommitV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.charge(1, 32)?;
        if self.record != head.verified_record_id()
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
        {
            return Err(FormalError::Invalid("IWS business material context".into()));
        }
        self.descriptor.validate_context(common)?;
        self.delta.validate_context(common)
    }
}
fn raw<O: PaidIwsBusinessOriginalReaderV1>(
    objects: &mut O,
    reference: OriginalArtifactRefV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Arc<ReservedValue<Vec<u8>>>, IwsBusinessMaterialErrorV1<O::Error>> {
    use IwsBusinessMaterialErrorV1 as E;
    let max =
        usize::try_from(reference.byte_len).map_err(|_| E::Invalid("business original length"))?;
    common.charge(1, reference.byte_len).map_err(E::Context)?;
    if !objects.allocation_budget().same_pool(common.memory())
        || objects.allocation_class() != MemoryClass::Ordinary
    {
        return Err(E::Invalid("business original source pool/class"));
    }
    let value = objects
        .read_business_original_bounded_v1(&reference.id, max, common)
        .map_err(E::Object)?;
    if !value.uses_budget(common.memory()) || value.len() != max {
        return Err(E::Invalid("business original exact length/pool"));
    }
    let _hash_phase = common
        .memory()
        .reserve(
            std::mem::size_of::<blake3::Hasher>() + std::mem::size_of::<blake3::Hash>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| E::Invalid("business original hash admission"))?;
    common.consume_steps(1).map_err(E::Context)?;
    let mut hash = blake3::Hasher::new();
    for chunk in value.chunks(8192) {
        common.charge(2, chunk.len() as u64).map_err(E::Context)?;
        hash.update(chunk);
    }
    if hash.finalize().as_bytes() != &reference.id {
        return Err(E::Invalid("business original content hash"));
    }
    Ok(value)
}
/// `material` supplies only graph location; its claimed bytes never replace an
/// actual original read. Parent identity and business roots are independently
/// authenticated by the two genuine verified heads.
pub fn read_iws_normal_business_output_bounded_v1<O: PaidIwsBusinessOriginalReaderV1>(
    head: &VerifiedFormalCommitV1,
    parent: &VerifiedFormalCommitV1,
    material: &ClosedRecord,
    objects: &mut O,
    nodes: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidIwsBusinessOutputV1, IwsBusinessMaterialErrorV1<O::Error>> {
    use IwsBusinessMaterialErrorV1 as E;
    let record = head.record();
    let previous = parent.record();
    let identity_bytes = [
        record.world.world_id.len(),
        record.world.chain_id.len(),
        record.world.genesis_digest.len(),
        previous.world.world_id.len(),
        previous.world.chain_id.len(),
        previous.world.genesis_digest.len(),
        record.parent_record_id.len(),
        256,
    ]
    .into_iter()
    .try_fold(0u64, |n, value| n.checked_add(value as u64))
    .ok_or(E::Invalid("business identity work overflow"))?;
    common.charge(1, identity_bytes).map_err(E::Context)?;
    if record.kind != FormalRecordKindV1::Execution
        || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
            record.schema_version,
        )
        || record.position
            != previous
                .position
                .checked_add(1)
                .ok_or(E::Invalid("business parent height overflow"))?
        || record.world != previous.world
    {
        return Err(E::Invalid("business actual normal parent"));
    }
    let mut parent_id = [0u8; 32];
    hex::decode_to_slice(&record.parent_record_id, &mut parent_id)
        .map_err(|_| E::Invalid("business parent ID encoding"))?;
    if parent_id != parent.verified_record_id() {
        return Err(E::Invalid("business actual parent ID"));
    }
    let locator = read_head_locator(head, material, objects, common)?;
    let descriptor_raw = raw(objects, locator.descriptor, common)?;
    let descriptor = decode_formal_business_descriptor_bounded_v1(&descriptor_raw, common)
        .map_err(E::Descriptor)?;
    descriptor
        .validate_record_bounded_v1(record, common)
        .map_err(E::Descriptor)?;
    let delta_raw = raw(objects, locator.delta, common)?;
    let delta = decode_business_index_delta_bounded_v1(&delta_raw, common).map_err(E::Index)?;
    let root = verify_business_index_delta_bounded_v1(&delta, &previous.result_root, nodes, common)
        .map_err(E::Index)?;
    common.charge(1, 256).map_err(E::Context)?;
    if &*root != descriptor.tree().index_root() || delta.delta().new_root != record.result_root {
        return Err(E::Invalid("business descriptor/delta signed result root"));
    }
    descriptor
        .validate_metadata_leaf_bounded_v1(nodes, common)
        .map_err(E::Descriptor)?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<PaidIwsBusinessOutputV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| E::Invalid("business material owner admission"))?;
    Ok(PaidIwsBusinessOutputV1 {
        descriptor,
        delta,
        locator,
        record: head.verified_record_id(),
        core: common.work().identity(),
        fee,
    })
}

/// The actual Qualified request supplies graph location; content is always
/// read from the paid Original source and checked against the signed head.
/// This loads DATA for the existing append kernel, not a new qualification.
pub fn read_iws_qualified_parent_business_descriptor_bounded_v1<
    O: PaidIwsBusinessOriginalReaderV1,
>(
    head: &VerifiedFormalCommitV1,
    source: &oasis7_distfs::controlled_authority::formal::endpoint::PaidHostQualifiedParentProofV1,
    objects: &mut O,
    nodes: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidFormalBusinessDescriptorV1, IwsBusinessMaterialErrorV1<O::Error>> {
    use IwsBusinessMaterialErrorV1 as E;
    source.validate_context(common).map_err(E::Context)?;
    common.charge(1, 32).map_err(E::Context)?;
    if source.actual_record_id() != head.verified_record_id()
        || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
            head.record().schema_version,
        )
    {
        return Err(E::Invalid("business descriptor actual Qualified head"));
    }
    let locator = read_head_locator(head, source.material(), objects, common)?;
    let bytes = raw(objects, locator.descriptor, common)?;
    let descriptor =
        decode_formal_business_descriptor_bounded_v1(&bytes, common).map_err(E::Descriptor)?;
    descriptor
        .validate_record_bounded_v1(head.record(), common)
        .map_err(E::Descriptor)?;
    descriptor
        .validate_metadata_leaf_bounded_v1(nodes, common)
        .map_err(E::Descriptor)?;
    Ok(descriptor)
}

fn read_head_locator<O: PaidIwsBusinessOriginalReaderV1>(
    head: &VerifiedFormalCommitV1,
    material: &ClosedRecord,
    objects: &mut O,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<BusinessOutputLocatorV1, IwsBusinessMaterialErrorV1<O::Error>> {
    use IwsBusinessMaterialErrorV1 as E;
    let record = head.record();
    let body = record
        .artifact_roots
        .get(&ArtifactRole::IncrementalCommitBody)
        .ok_or(E::Invalid("business current body root"))?;
    let mut body_id = [0u8; 32];
    hex::decode_to_slice(body, &mut body_id)
        .map_err(|_| E::Invalid("business body ID encoding"))?;
    let mut body_object = None;
    for object in &material.objects {
        common
            .charge(1, object.content_hash.len() as u64)
            .map_err(E::Context)?;
        if &object.content_hash == body {
            if body_object.replace(object).is_some() {
                return Err(E::Invalid("business duplicate body object"));
            }
        }
    }
    let body_object = body_object.ok_or(E::Invalid("business body graph missing"))?;
    let mut selected = None;
    let mut last = None;
    for reference in &body_object.references {
        common
            .charge(1, reference.len() as u64)
            .map_err(E::Context)?;
        if last.is_some_and(|old: &String| old >= reference) {
            return Err(E::Invalid("business body references order"));
        }
        last = Some(reference);
        for object in &material.objects {
            common
                .charge(
                    1,
                    object.content_hash.len() as u64 + BUSINESS_OUTPUT_DOMAIN_V1.len() as u64,
                )
                .map_err(E::Context)?;
            if &object.content_hash == reference
                && object.bytes.starts_with(BUSINESS_OUTPUT_DOMAIN_V1)
            {
                if selected.is_some() || object.bytes.len() > BUSINESS_OUTPUT_MAX_BYTES {
                    return Err(E::Invalid("business locator uniqueness/bound"));
                }
                let mut id = [0u8; 32];
                hex::decode_to_slice(reference, &mut id)
                    .map_err(|_| E::Invalid("business locator ID"))?;
                selected = Some(OriginalArtifactRefV1 {
                    id,
                    byte_len: object.bytes.len() as u64,
                });
            }
        }
    }
    let locator_ref = selected.ok_or(E::Invalid("business typed locator missing"))?;
    let locator_raw = raw(objects, locator_ref, common)?;
    let mut first = None;
    let locator =
        BusinessOutputLocatorV1::from_canonical_bytes_with_work(&locator_raw, &mut |s, b| {
            common.charge(s, b).map_err(|e| {
                first = Some(e);
                ExecutionError::Aborted
            })
        });
    if let Some(e) = first {
        return Err(E::Context(e));
    }
    let locator = locator.map_err(E::Execution)?;
    if locator.commit_body != body_id {
        return Err(E::Invalid("business locator current body"));
    }
    Ok(locator)
}
