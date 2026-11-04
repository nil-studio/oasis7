//! Single-original decoding for the closed Runtime History DTOs. Raw buffers
//! remain caller-owned; this never loads a History collection or grants source.
use crate::{
    consensus::{TickConsensusRecord, TickConsensusRejectionAuditEvent},
    incremental_original_decode::check_decoded_canonical_original_v1,
    snapshot::SnapshotRecord,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::{
    cbor_decode_preflight::{
        CborPreflightErrorV1, CborPreflightWorkV1, preflight_definite_cbor_v1,
    },
    memory_budget::{MemoryBudget, MemoryClass, ReservedValue},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy)]
pub enum RuntimeHistoryDecodeKindV1 {
    Snapshot,
    Tick,
    Audit,
}
pub enum DecodedRuntimeHistoryOriginalV1 {
    Snapshot(SnapshotRecord),
    Tick(TickConsensusRecord),
    Audit(TickConsensusRejectionAuditEvent),
}
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    domain: String,
    schema: String,
    value: T,
}
pub struct RuntimeHistoryDecodeWorkV1 {
    pub structure: CborPreflightWorkV1,
    pub canonical: CanonicalCborWorkV1,
    pub max_original_bytes: usize,
    pub max_heap_bytes: usize,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime History original type/canonical encoding")
}
/// Layout contract is deliberately restricted to these three DTO graphs:
/// * Snapshot: String, usize, u64;
/// * Tick: inline Header/Digest/Certificate, String/Option<String>/unit enum,
///   Vec<u64> and BTreeMap<String,String>;
/// * Audit: String, Option<unit enum>, u64.
/// There is no serde Content-buffered enum or arbitrary JSON in these graphs.
/// Definite strings are borrowed by serde_cbor's slice reader before copying.
/// 16*string bytes conservatively covers owned storage and error formatting:
/// an unknown unit-enum string can Debug-escape each ASCII control byte into
/// six characters, and the error String can grow to twice that output length.
/// The remaining four shares cover owned strings and fixed formatting slop;
/// this is an admission bound, not a measurement or an increased heap limit.
/// Vec<u64> gets 4*8 per actual sequence element, covering doubling and its old
/// allocation. The current std BTree node holds at most 11 String/String pairs
/// and 12 pointer edges (<1024 bytes); a nonempty node needs at least one entry,
/// so charging 1024 for every actual map entry bounds every node, including
/// internal nodes. Struct maps are counted too, conservatively. Fixed scratch
/// includes defaults/error metadata. This contract MUST be revisited before
/// adding any reachable DTO field or changing the standard-library layout.
pub fn decode_runtime_history_original_v1<C: FnMut() -> bool>(
    kind: RuntimeHistoryDecodeKindV1,
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuntimeHistoryDecodeWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<DecodedRuntimeHistoryOriginalV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if raw.len() > work.max_original_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "Runtime History original bytes",
        ));
    }
    let shape = preflight_definite_cbor_v1(raw, &mut work.structure, &mut cancelled).map_err(
        |e| match e {
            CborPreflightErrorV1::Cancelled => ValueTreeError::Cancelled,
            CborPreflightErrorV1::ResourceLimited => {
                ValueTreeError::ResourceLimited("Runtime History structure")
            }
            CborPreflightErrorV1::Invalid => invalid(),
        },
    )?;
    // These closed DTOs contain no serde byte buffers. Reject alternative byte
    // encodings before any collection visitor can reinterpret their capacity.
    if shape.byte_strings != 0 {
        return Err(invalid());
    }
    // Spend the complete admitted second-pass allowance before serde starts.
    work.structure.values_remaining = work
        .structure
        .values_remaining
        .checked_sub(shape.values)
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime History typed visits",
        ))?;
    work.structure.bytes_remaining = work
        .structure
        .bytes_remaining
        .checked_sub(raw.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime History typed bytes",
        ))?;
    let fixed = std::mem::size_of::<Envelope<TickConsensusRecord>>()
        .max(std::mem::size_of::<Envelope<SnapshotRecord>>())
        .max(std::mem::size_of::<
            Envelope<TickConsensusRejectionAuditEvent>,
        >())
        .checked_add(std::mem::size_of::<DecodedRuntimeHistoryOriginalV1>())
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(invalid)?;
    let heap = shape
        .sequence_entries
        .checked_mul(32)
        .and_then(|n| {
            shape
                .map_entries
                .checked_mul(1024)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| {
            shape
                .string_bytes
                .checked_mul(16)
                .and_then(|s| n.checked_add(s))
        })
        .and_then(|n| usize::try_from(n).ok())
        .and_then(|n| n.checked_add(fixed))
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime History heap bound",
        ))?;
    if heap > work.max_heap_bytes {
        return Err(ValueTreeError::ResourceLimited(
            "Runtime History heap allowance",
        ));
    }
    let charge = memory.reserve(heap, class)?;
    fn decode<T: serde::de::DeserializeOwned + Serialize, C: FnMut() -> bool>(
        raw: &[u8],
        schema: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
    ) -> Result<T, ValueTreeError> {
        let envelope: Envelope<T> = serde_cbor::from_slice(raw).map_err(|_| invalid())?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if envelope.domain != "oasis7.runtime.history.original.v1" || envelope.schema != schema {
            return Err(invalid());
        }
        check_decoded_canonical_original_v1(&envelope, raw, memory, class, work, cancelled)?;
        Ok(envelope.value)
    }
    let value = match kind {
        RuntimeHistoryDecodeKindV1::Snapshot => DecodedRuntimeHistoryOriginalV1::Snapshot(decode(
            raw,
            "snapshot-record.v1",
            memory,
            class,
            &mut work.canonical,
            &mut cancelled,
        )?),
        RuntimeHistoryDecodeKindV1::Tick => DecodedRuntimeHistoryOriginalV1::Tick(decode(
            raw,
            "tick-consensus-record.v1",
            memory,
            class,
            &mut work.canonical,
            &mut cancelled,
        )?),
        RuntimeHistoryDecodeKindV1::Audit => DecodedRuntimeHistoryOriginalV1::Audit(decode(
            raw,
            "tick-consensus-rejection-audit.v1",
            memory,
            class,
            &mut work.canonical,
            &mut cancelled,
        )?),
    };
    match &value {
        DecodedRuntimeHistoryOriginalV1::Snapshot(v) => snapshot_layout(v),
        DecodedRuntimeHistoryOriginalV1::Tick(v) => tick_layout(v),
        DecodedRuntimeHistoryOriginalV1::Audit(v) => audit_layout(v),
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(value))
}

#[cfg(test)]
#[path = "incremental_runtime_history_decode_tests.rs"]
mod tests;

// Exhaustive patterns and exact field types make reachable-layout changes a
// compile error, rather than silently reusing the old pre-allocation bound.
macro_rules! layout_fields {
 ($name:path,$value:expr; $($field:ident:$ty:ty),+$(,)?)=>{{let $name{$($field),+}=$value;$(let _: &$ty=$field;)+}};
}
fn snapshot_layout(v: &SnapshotRecord) {
    layout_fields!(SnapshotRecord,v;snapshot_hash:String,journal_len:usize,created_at:u64,manifest_hash:String);
}
fn tick_layout(v: &TickConsensusRecord) {
    use crate::consensus::{
        TickBlock, TickBlockHeader, TickCertificate, TickConsensusSubmissionRole,
        TickExecutionDigest,
    };
    layout_fields!(TickConsensusRecord,v;block:TickBlock,certificate:TickCertificate);
    layout_fields!(TickBlock,&v.block;header:TickBlockHeader,ordered_action_ids:Vec<u64>,ordered_event_ids:Vec<u64>,event_count:u32,execution_digest:TickExecutionDigest);
    layout_fields!(TickBlockHeader,&v.block.header;schema_version:u16,epoch:u64,tick:u64,parent_hash:String,events_hash:String,state_root:String,executor_version:String,randomness_seed:String,chain_height:Option<u64>,chain_slot:Option<u64>,chain_epoch:Option<u64>,node_block_hash:Option<String>,action_root:Option<String>,committed_at_unix_ms:Option<i64>);
    layout_fields!(TickExecutionDigest,&v.block.execution_digest;action_batch_hash:String,domain_events_hash:String,state_projection_hash:String);
    layout_fields!(TickCertificate,&v.certificate;block_hash:String,consensus_height:u64,threshold:u16,authority_source:String,submission_role:TickConsensusSubmissionRole,signatures:std::collections::BTreeMap<String,String>);
    role_layout(v.certificate.submission_role);
}
fn role_layout(v: crate::consensus::TickConsensusSubmissionRole) {
    use crate::consensus::TickConsensusSubmissionRole;
    match v {
        TickConsensusSubmissionRole::Propagation | TickConsensusSubmissionRole::Authority => {}
    }
}
fn audit_layout(v: &TickConsensusRejectionAuditEvent) {
    use crate::consensus::TickConsensusSubmissionRole;
    layout_fields!(TickConsensusRejectionAuditEvent,v;recorded_at_tick:u64,tick:u64,consensus_height:u64,attempted_source:String,attempted_role:TickConsensusSubmissionRole,existing_source:Option<String>,existing_role:Option<TickConsensusSubmissionRole>,reason:String);
    role_layout(v.attempted_role);
    if let Some(role) = v.existing_role {
        role_layout(role);
    }
}
