//! All five RequestState variants as paid data, never verification authority.
use super::super::{FormalArchiveReadOwnerV1, FormalArchiveReadWorkV1};
use super::paid_read::PaidRead;
use super::*;
use crate::controlled_authority::replicated_protocol::{ArtifactObject, ArtifactRole};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

/// Owns the entire conservatively admitted decode phase until drop. There is
/// no Clone, constructor, detached DTO or conversion into verified authority.
#[derive(Debug)]
pub(in crate::controlled_authority::formal::endpoint) struct PaidRequestStateV1 {
    value: ReservedValue<FormalRequestStateV1>,
}
impl PaidRequestStateV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn state(&self) -> &FormalRequestStateV1 {
        &self.value
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.value.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.value.reserved_bytes()
    }
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded request heap layout"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("bounded request heap layout"))
}
/// Rust 1.96.0 ac68faa20 alloc/collections/btree/node.rs: B=6,
/// Leaf={parent Option<NonNull>,parent_idx u16,len u16,keys[11],vals[11]}.
/// Field order is not assumed: five fields allow five (max_alignment-1)
/// padding gaps including the final tail. Internal repr(C) adds 12 pointers,
/// with two further alignment gaps. Counts use the closed Role domain's 14
/// keys, plus three simultaneous root/split nodes, independently per map.
fn role_map_nodes() -> Result<usize, FormalError> {
    let pointer = std::mem::size_of::<std::ptr::NonNull<()>>();
    let alignment = std::mem::align_of::<ArtifactRole>()
        .max(std::mem::align_of::<String>())
        .max(std::mem::align_of::<std::ptr::NonNull<()>>())
        .max(std::mem::align_of::<u16>());
    let parent = std::mem::size_of::<Option<std::ptr::NonNull<()>>>();
    let leaf = add(
        add(parent, 2 * std::mem::size_of::<u16>())?,
        add(
            mul(11, std::mem::size_of::<ArtifactRole>())?,
            mul(11, std::mem::size_of::<String>())?,
        )?,
    )?;
    let leaf = add(leaf, mul(5, alignment - 1)?)?;
    let internal = add(add(leaf, mul(12, pointer)?)?, mul(2, alignment - 1)?)?;
    mul(4, mul(14 + 3, internal)?)
}
/// Fixed expected/field/variant labels from the COMPLETE reachable DTO graph.
/// These are diagnostic size accounting, not a parser or a semantic allowlist.
fn diagnostic_constant() -> Result<usize, FormalError> {
    const LABELS: &[&str] = &[
        "ArtifactObject",
        "ArtifactRole",
        "CanonicalRecordV1",
        "ClosedRecord",
        "CompletePrepared",
        "Decided",
        "EffectOutbox",
        "EndpointRole",
        "Execution",
        "ExecutionManifest",
        "FormalCandidateMaterialV1",
        "FormalCompletionBranchV1",
        "FormalDecisionProofV1",
        "FormalFinalReceiptBodyV1",
        "FormalFinalReceiptV1",
        "FormalFrozenBodyV1",
        "FormalFrozenCertificateV1",
        "FormalFrozenReceiptBodyV1",
        "FormalFrozenReceiptV1",
        "FormalHeadRefV1",
        "FormalPrepareReceiptBodyV1",
        "FormalPrepareReceiptV1",
        "FormalProofCompleteBodyV1",
        "FormalProofCompleteReceiptV1",
        "FormalQualifiedCommitProofV1",
        "FormalRecordKindV1",
        "FormalRequestStateV1",
        "FormalWorldIdentityV1",
        "Frozen",
        "IncrementalCommitBody",
        "IncrementalGenesisConfig",
        "IncrementalHistoryPrefix",
        "IncrementalRootSet",
        "InitialActivation",
        "Input",
        "Journal",
        "NativeExecutionEnvironment",
        "NonceIndex",
        "Normal",
        "Prepared",
        "Primary",
        "ProfileHandoffV1",
        "Qualified",
        "Ready",
        "ReplayFrozen",
        "Replica",
        "Result",
        "Rules",
        "SignedCanonicalCandidateV1",
        "SignedFormalFrozenV1",
        "Snapshot",
        "Wasm",
        "action_root",
        "activation_ref",
        "after_state_root",
        "artifact_roots",
        "authority_id",
        "before_state_root",
        "body",
        "branch",
        "bytes",
        "candidate",
        "candidate_material_digest",
        "certificate",
        "chain_id",
        "complete",
        "content_hash",
        "data_closure",
        "decision",
        "decision_proof_digest",
        "durable_set_transition_digest",
        "epoch",
        "event_root",
        "execution_policy_digest",
        "final_receipt",
        "finalizing_authority_id",
        "frozen",
        "frozen_material",
        "frozen_material_digest",
        "frozen_reservation_id",
        "genesis_digest",
        "governing_manifest_digest",
        "handoff",
        "input",
        "instance_digest",
        "kind",
        "material",
        "next_authority_digest",
        "next_position",
        "next_profile",
        "next_profile_version",
        "objects",
        "original_authority_id",
        "original_epoch",
        "parent",
        "parent_record_id",
        "payload_digest",
        "position",
        "pre_state_root",
        "prepare",
        "primary",
        "primary_complete",
        "projection_root",
        "proof",
        "option",
        "receipt",
        "record",
        "record_id",
        "recovery_certificate_digest",
        "references",
        "replica",
        "replica_complete",
        "request_digest",
        "reservation",
        "reservation_id",
        "result_root",
        "role",
        "roots",
        "runtime_manifest_digest",
        "schema_version",
        "signature_hex",
        "valid_from_position",
        "world",
        "world_id",
        "writer_signature_hex",
    ];
    const TEMPLATES: &[&str] = &[
        "unknown variant ``, expected ",
        "unknown field ``, expected ",
        "invalid type: , expected ",
        "invalid value: , expected ",
        "invalid length , expected ",
        "duplicate field ``",
        "missing field ``",
        " at offset ",
        "struct ",
        "struct variant ",
        "enum ",
        "variant identifier",
        "field identifier",
        "a string",
        "a sequence",
        "a map",
        "a boolean",
        "u8",
        "u16",
        "u32",
        "u64",
        "bounded typed sequence",
        "typed sequence capacity limit",
        "typed sequence count limit",
    ];
    // Six 20-digit integer/offset slots, 32 delimiters and four <=26-byte
    // short scalar descriptions cover fixed formatting independently of input.
    let mut total = 6 * 20 + 32 + 4 * 26;
    for label in LABELS.iter().chain(TEMPLATES) {
        total = add(total, add(label.len(), 4)?)?;
    }
    Ok(total)
}
/// Complete closed heap: two objects Vecs (geometric 512 slots each), and
/// 513 potentially constructed objects. The extra object matters: the custom
/// bounded visitor decodes next_element BEFORE rejecting len==256. Each has
/// a references Vec covered by 512 String slots, including the 257th temporary
/// reference String; an 8-byte minimum Vec capacity slack covers each bytes
/// Vec. Byte/text payloads share actual wire n, bounded together by 2n.
/// All reachable Vecs use package::bounded_vec, NOT serde_core's generic Vec
/// cautious(size_hint), which would otherwise preallocate up to 1MiB per Vec.
/// Qualified reaches at most four Role maps; certificates are flat structs,
/// Option and String, with no signature Vec hidden in their reachable graph.
pub(in crate::controlled_authority::formal::endpoint) fn decode_workspace(
    n: usize,
) -> Result<usize, FormalError> {
    let object_slots = mul(2 * 512, std::mem::size_of::<ArtifactObject>())?;
    let reference_slots = mul(513 * 512, std::mem::size_of::<String>())?;
    let minimum_byte_capacity = 513 * 8;
    let flat = add(
        add(add(object_slots, reference_slots)?, minimum_byte_capacity)?,
        role_map_nodes()?,
    )?;
    let headers = std::mem::size_of::<FormalRequestStateV1>()
        + std::mem::size_of::<PaidRead<'_>>()
        + std::mem::size_of::<serde_cbor::Error>()
        + 4 * std::mem::size_of::<String>()
        + 4 * std::mem::size_of::<Vec<u8>>();
    let flat = add(flat, headers)?;
    let k = diagnostic_constant()?;
    // Locked serde_cbor_2 0.13.0 / serde_core 1.0.229: scratch <=2n+16KiB;
    // all owned payloads <=2n; diagnostic Debug escape<=6n+K and String
    // growth<=2x. Thus parse <=flat+16n+16KiB+2K. On Err from_reader drops
    // partial DTO/scratch before Invalid(error.to_string()), so the bridge
    // holds only two error buffers <=24n+4K+64, plus error/reader headers.
    let parse = add(add(add(flat, mul(16, n)?)?, 16 * 1024)?, mul(2, k)?)?;
    let bridge = add(add(add(mul(24, n)?, mul(4, k)?)?, 64)?, headers)?;
    Ok(parse.max(bridge))
}
pub(in crate::controlled_authority::formal::endpoint) fn decode_request_bounded_v1(
    raw: &FormalArchiveReadOwnerV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<PaidRequestStateV1, FormalError> {
    work.consume_steps(1, cancelled)?;
    if !raw.uses_budget(memory) || raw.as_slice().len() > LIMIT {
        return Err(invalid("bounded request raw pool/limit"));
    }
    decode_request_bytes_bounded_v1(raw.as_slice(), memory, work, cancelled)
}
pub(in crate::controlled_authority::formal::endpoint) fn decode_request_bytes_bounded_v1(
    bytes: &[u8],
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<PaidRequestStateV1, FormalError> {
    work.consume_steps(1, cancelled)?;
    if bytes.len() > LIMIT {
        return Err(invalid("bounded request raw pool/limit"));
    }
    let fee = memory
        .reserve(decode_workspace(bytes.len())?, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded request heap admission"))?;
    let mut reader = PaidRead::new(bytes, work, cancelled);
    let result = serde_cbor::from_reader::<FormalRequestStateV1, _>(&mut reader);
    if let Some(first) = reader.take_first_error() {
        return Err(first);
    }
    let state = result.map_err(|error| invalid(error.to_string()))?;
    reader.check_cancelled()?;
    // Preserve the complete conservative phase; all owned children stay covered.
    // FormalError's existing String contract is unchanged, not falsely retained.
    Ok(PaidRequestStateV1 {
        value: fee.retain(state),
    })
}
pub(super) fn read_request_bounded_v1(
    store: &Store,
    request: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<PaidRequestStateV1>, FormalError> {
    work.consume_steps(1, cancelled)?;
    let Some(id) = store.lookup_bounded(request, memory, work, cancelled)? else {
        return Ok(None);
    };
    let raw = store
        .raw_object_bounded(&id, LIMIT, memory, work, cancelled)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    let state = decode_request_bounded_v1(&raw, memory, work, cancelled)?;
    work.consume_read_bytes(state.state().request_digest().len() as u64, cancelled)?;
    if state.state().request_digest() != request {
        return Err(invalid("formal index request binding"));
    }
    work.consume_steps(1, cancelled)?;
    Ok(Some(state))
}
#[cfg(all(test, unix))]
#[path = "endpoint_store_bounded_request_tests.rs"]
mod tests;
