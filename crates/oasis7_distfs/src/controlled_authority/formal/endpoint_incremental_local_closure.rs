//! Local immutable data-closure record. The authenticated reference is distinct
//! from a fresh physical continuity check and from execution permission.
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use serde::{Deserialize, Serialize};

/// No public constructor: only the private complete consumer supplies evidence.
/// Every field is fixed width; archive readers never allocate arbitrary graphs.
pub(crate) struct LocalDataClosureRecordV1 {
    version: u32,
    instance: [u8; 32],
    authority: [u8; 32],
    activation: [u8; 32],
    epoch: u64,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    manifest: [u8; 32],
    page_lease: (u64, u64),
    native_lease: (u64, u64),
    native_generation: Option<[u8; 32]>,
    cognition: LocalFactLocationV1,
    runtime: LocalFactLocationV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalFactLocationV1 {
    height: u64,
    ordinal: u64,
    original: [u8; 32],
}
impl From<oasis7_formal_protocol::incremental_material::FactLocationV1> for LocalFactLocationV1 {
    fn from(source: oasis7_formal_protocol::incremental_material::FactLocationV1) -> Self {
        Self {
            height: source.height,
            ordinal: source.ordinal,
            original: source.original,
        }
    }
}
fn fixed_id(text: &str) -> Result<[u8; 32], FormalError> {
    hash_hex(text)?;
    let mut out = [0; 32];
    hex::decode_to_slice(text, &mut out).map_err(|_| invalid("local data closure identifier"))?;
    Ok(out)
}
/// The returned reference remains charged; it cannot be created from a hash.
/// Preparing it neither updates the checkpoint nor accepts the candidate.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct PreparedLocalDataClosureV1 {
    record: [u8; 32],
    reference: ReservedValue<String>,
}
#[cfg(not(target_arch = "wasm32"))]
impl PreparedLocalDataClosureV1 {
    pub(super) fn candidate_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub(super) fn reference_for(
        &self,
        accepted: &VerifiedFormalCommitV1,
    ) -> Result<&str, FormalError> {
        if self.record != accepted.verified_record_id() {
            return Err(invalid("local data closure accepted record"));
        }
        Ok(&self.reference)
    }
}
impl LocalDataClosureRecordV1 {
    pub(crate) fn validate_head(&self, head: &FormalHeadRefV1) -> Result<(), FormalError> {
        if self.version != 1 || self.record != fixed_id(&head.record_id)? {
            return Err(invalid("local data closure record/head"));
        }
        Ok(())
    }
    /// This authenticates local metadata, not the continued availability of all
    /// bytes. A cold consumer must additionally hold and check both real leases.
    pub(crate) fn validate_scope(&self, hot: &store::HotBody) -> Result<(), FormalError> {
        self.validate_head(&hot.head)?;
        if self.instance != fixed_id(&hot.instance_digest)?
            || self.authority != fixed_id(&hot.authority_id)?
            || self.activation != fixed_id(&hot.activation_ref)?
            || self.epoch != hot.epoch
        {
            return Err(invalid("local data closure protected domain"));
        }
        Ok(())
    }
}
/// Only a completed private byte consumer can reach this issuer. Namespace
/// continuity is deliberately not inferred from this record by cold readers.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn prepare_local_data_closure_v1(
    evidence: &ReservedValue<super::closure::CompleteDataClosureEvidenceV1>,
    store: &store::Store,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PreparedLocalDataClosureV1, FormalError> {
    if cancelled() {
        return Err(invalid("local data closure cancelled"));
    }
    if !evidence.uses_budget(memory)
        || evidence.reserved_bytes() < std::mem::size_of_val(&**evidence) + 512
    {
        return Err(invalid("local data closure evidence admission"));
    }
    // Fixed record encoding plus existing immutable-store path/hash buffers.
    // The output reservation remains live independently of this phase.
    let _phase = memory
        .reserve(16 * 1024, MemoryClass::Ordinary)
        .map_err(|_| invalid("local data closure metadata admission"))?;
    let output = memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(|_| invalid("local data closure reference admission"))?;
    let hot = &store.body;
    let native_generation =
        crate::controlled_authority::formal::physical::current_generation_digest_uncached(
            &store.path.join("formal-objects-v1"),
            memory,
            MemoryClass::Ordinary,
            cancelled,
        )?;
    let record = LocalDataClosureRecordV1 {
        version: 1,
        instance: fixed_id(&hot.instance_digest)?,
        authority: fixed_id(&hot.authority_id)?,
        activation: fixed_id(&hot.activation_ref)?,
        epoch: hot.epoch,
        record: evidence.candidate_record_id(),
        body: evidence.body_id(),
        roots: evidence.roots_id(),
        manifest: evidence.manifest_id(),
        page_lease: evidence.page_namespace_identity(),
        native_lease: evidence.native_namespace_identity(),
        native_generation,
        cognition: evidence.cognition_location().into(),
        runtime: evidence.runtime_location().into(),
    };
    let reference = store.put_local_closure_record(&record, memory, cancelled)?;
    if cancelled() {
        return Err(invalid("local data closure publication cancelled"));
    }
    Ok(PreparedLocalDataClosureV1 {
        record: record.record,
        reference: output.retain(reference),
    })
}

#[path = "endpoint_incremental_local_closure_codec.rs"]
mod codec;
pub(crate) use codec::WIRE_BYTES as LOCAL_DATA_CLOSURE_WIRE_BYTES;

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_cold_local.rs"]
mod recovery;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use recovery::{
    AuthenticatedLocalClosureV1, LocalClosureReadWorkV1, recover_authenticated_local_closure_v1,
};
