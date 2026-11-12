//! Closed classification of every actual Result in one checked FactSet.
//! This proves type dispatch only, not retention, History source or business
//! semantics. Each original is bounded and released before reading the next.
use crate::{
    incremental_cognition_history::{
        COGNITION_HISTORY_VIEW_V3_MAGIC, CognitionHistoryFieldV1, decode_cognition_history_view_v3,
    },
    incremental_cognition_history_cbor::{
        CognitionHistoryCborWorkV1, CognitionOriginalCborRequestV1,
        verify_cognition_history_original_record_schema_v1,
    },
    incremental_runtime_history_decode::{
        RuntimeHistoryDecodeKindV1, RuntimeHistoryDecodeWorkV1, decode_runtime_history_original_v1,
    },
    incremental_runtime_history_manifest::{RUNTIME_HISTORY_MAGIC_V1, RuntimeHistoryManifestV1},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedCandidateContextV1,
    incremental_execution::{EXECUTION_MAX_FACTS, ExecutionFactKindV1},
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CheckedCommitCore,
        CheckedFactOriginals, CoreError, FactBudget, OriginalObjectStreamReader,
        check_candidate_fact_v1,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::{
    cbor_decode_preflight::CborPreflightWorkV1,
    memory_budget::{MemoryBudget, MemoryClass, ReservedValue},
};
use std::{
    convert::Infallible,
    io::{self, Write},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldResultProfileV1 {
    CognitionHistoryV3,
    CognitionProofLedgerV1,
    RuntimeHistoryV1,
    SnapshotV1,
    TickV1,
    AuditV1,
    CognitionResponseV1,
}
pub struct RoutedWorldResultV1 {
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    profile: WorldResultProfileV1,
}
impl RoutedWorldResultV1 {
    pub fn occurrence(&self) -> &ReservedValue<CheckedCandidateFactOccurrenceV1> {
        &self.occurrence
    }
    pub fn profile(&self) -> WorldResultProfileV1 {
        self.profile
    }
}
pub struct CheckedWorldResultRoutingV1 {
    rows: ReservedValue<Vec<RoutedWorldResultV1>>,
    cognition: usize,
    runtime: usize,
    proof_ledger: Option<usize>,
}
impl CheckedWorldResultRoutingV1 {
    pub fn rows(&self) -> &[RoutedWorldResultV1] {
        &self.rows
    }
    pub fn cognition(&self) -> &RoutedWorldResultV1 {
        &self.rows[self.cognition]
    }
    /// Independent side-ledger Result, absent in the legacy two-Result profile.
    pub fn proof_ledger(&self) -> Option<&RoutedWorldResultV1> {
        self.proof_ledger.map(|index| &self.rows[index])
    }
    pub fn runtime_history(&self) -> &RoutedWorldResultV1 {
        &self.rows[self.runtime]
    }
}
pub struct WorldResultRouterRequestV1<'a> {
    pub candidate: &'a VerifiedCandidateContextV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
}
pub struct WorldResultRouterWorkV1 {
    pub facts: FactBudget,
    pub envelope: CborPreflightWorkV1,
    pub runtime: RuntimeHistoryDecodeWorkV1,
    pub cognition: CognitionHistoryCborWorkV1,
}
#[derive(Debug)]
pub enum WorldResultRouterErrorV1<E> {
    Value(ValueTreeError),
    Original(E),
    Formal(CoreError<Infallible>),
}
impl<E> From<ValueTreeError> for WorldResultRouterErrorV1<E> {
    fn from(value: ValueTreeError) -> Self {
        Self::Value(value)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("World Result closed dispatch")
}
struct Sink<'a, C> {
    bytes: &'a mut Vec<u8>,
    expected: usize,
    hash: blake3::Hasher,
    remaining: &'a mut u64,
    cancelled: &'a mut C,
    stopped: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for Sink<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.stopped.is_some() {
            return Err(io::Error::other("Result stream stopped"));
        }
        self.stopped = Some(invalid());
        let bytes = &bytes[..bytes
            .len()
            .min(oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES)];
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Result cancelled"));
        }
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.expected)
            .ok_or_else(|| io::Error::other("Result length"))?;
        let Some(left) = self.remaining.checked_sub(bytes.len() as u64) else {
            self.stopped = Some(ValueTreeError::ResourceLimited("Result original bytes"));
            return Err(io::Error::other("Result bytes"));
        };
        *self.remaining = left;
        self.hash.update(bytes);
        self.bytes.extend_from_slice(bytes);
        debug_assert_eq!(self.bytes.len(), end);
        self.stopped = None;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stopped.is_some() {
            Err(io::Error::other("Result stopped"))
        } else {
            Ok(())
        }
    }
}

/// A label probe is deliberately not a decoder or proof. It borrows only the
/// exact outer domain/schema labels, spending bounded work while skipping the
/// value. The selected owner subsequently checks the complete typed canonical
/// original, so no failed serde decoder is used as a dispatch probe.
struct LabelProbe<'a, 'b, C> {
    raw: &'a [u8],
    at: usize,
    work: &'b mut CborPreflightWorkV1,
    cancelled: &'b mut C,
}
impl<'a, C: FnMut() -> bool> LabelProbe<'a, '_, C> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ValueTreeError> {
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        self.work.bytes_remaining = self
            .work
            .bytes_remaining
            .checked_sub(n as u64)
            .ok_or(ValueTreeError::ResourceLimited("Result label bytes"))?;
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.raw.len())
            .ok_or_else(invalid)?;
        let bytes = &self.raw[self.at..end];
        self.at = end;
        Ok(bytes)
    }
    fn head(&mut self) -> Result<(u8, u64), ValueTreeError> {
        self.work.values_remaining = self
            .work
            .values_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Result label visits"))?;
        let initial = self.take(1)?[0];
        let major = initial >> 5;
        let small = initial & 31;
        let value = match small {
            0..=23 => small as u64,
            24 => self.take(1)?[0] as u64,
            25 => u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| invalid())?) as u64,
            26 => u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| invalid())?) as u64,
            27 => u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| invalid())?),
            _ => return Err(invalid()),
        };
        if major == 6 {
            return Err(invalid());
        }
        Ok((major, value))
    }
    fn text(&mut self) -> Result<&'a str, ValueTreeError> {
        let (major, n) = self.head()?;
        if major != 3 {
            return Err(invalid());
        }
        // All closed envelope labels fit this fixed bound. Never UTF-8 scan
        // an arbitrary payload-sized string just to select an owner decoder.
        if n > 64 {
            return Err(invalid());
        }
        std::str::from_utf8(self.take(usize::try_from(n).map_err(|_| invalid())?)?)
            .map_err(|_| invalid())
    }
    fn skip(&mut self, depth: usize) -> Result<(), ValueTreeError> {
        if depth > self.work.max_depth || depth > 64 {
            return Err(ValueTreeError::ResourceLimited("Result label depth"));
        }
        let (major, n) = self.head()?;
        match major {
            0 | 1 | 7 => {}
            2 | 3 => {
                self.take(usize::try_from(n).map_err(|_| invalid())?)?;
            }
            4 | 5 => {
                let count = if major == 5 {
                    n.checked_mul(2).ok_or_else(invalid)?
                } else {
                    n
                };
                for _ in 0..count {
                    self.skip(depth.checked_add(1).ok_or_else(invalid)?)?;
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}
fn runtime_label<C: FnMut() -> bool>(
    raw: &[u8],
    work: &mut CborPreflightWorkV1,
    cancelled: &mut C,
) -> Result<Option<RuntimeHistoryDecodeKindV1>, ValueTreeError> {
    // Every Runtime History Result is exactly a three-member envelope.
    if raw.first().copied() != Some(0xa3) {
        return Ok(None);
    }
    let mut p = LabelProbe {
        raw,
        at: 0,
        work,
        cancelled,
    };
    if p.head()? != (5, 3) {
        return Ok(None);
    }
    let mut domain = None;
    let mut schema = None;
    let mut value = false;
    for _ in 0..3 {
        match p.text()? {
            "domain" if domain.is_none() => domain = Some(p.text()?),
            "schema" if schema.is_none() => schema = Some(p.text()?),
            "value" if !value => {
                value = true;
                p.skip(1)?;
            }
            _ => return Ok(None),
        }
    }
    if p.at != raw.len() || !value || domain != Some("oasis7.runtime.history.original.v1") {
        return Ok(None);
    }
    Ok(match schema {
        Some("snapshot-record.v1") => Some(RuntimeHistoryDecodeKindV1::Snapshot),
        Some("tick-consensus-record.v1") => Some(RuntimeHistoryDecodeKindV1::Tick),
        Some("tick-consensus-rejection-audit.v1") => Some(RuntimeHistoryDecodeKindV1::Audit),
        _ => None,
    })
}

/// Shared bounded stream/hash admission. Failure spends its whole admitted
/// bound; success refunds only the unused bound. No read_original shortcut.
#[allow(
    clippy::too_many_arguments,
    reason = "The object id, per-object bound, exactness, memory pool/class, aggregate byte allowance, and cancellation state are independent physical-read controls."
)]
pub(super) fn read_world_router_original_v1<O: OriginalObjectStreamReader, C: FnMut() -> bool>(
    objects: &mut O,
    id: &[u8; 32],
    bound: usize,
    exact: bool,
    memory: &MemoryBudget,
    class: MemoryClass,
    remaining: &mut u64,
    cancelled: &mut C,
) -> Result<ReservedValue<Vec<u8>>, WorldResultRouterErrorV1<O::Error>> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if objects.allocation_class() != class || !objects.allocation_budget().same_pool(memory) {
        return Err(invalid().into());
    }
    *remaining = remaining
        .checked_sub(bound as u64)
        .ok_or(ValueTreeError::ResourceLimited("Result original bytes"))?;
    let charge = memory
        .reserve(bound.checked_add(1024).ok_or_else(invalid)?, class)
        .map_err(ValueTreeError::Memory)?;
    let mut raw = Vec::with_capacity(bound);
    let mut allowance = bound as u64;
    let mut sink = Sink {
        bytes: &mut raw,
        expected: bound,
        hash: blake3::Hasher::new(),
        remaining: &mut allowance,
        cancelled,
        stopped: None,
    };
    let observed = objects.stream_original(id, bound as u64, memory, &mut sink);
    if let Some(error) = sink.stopped.take() {
        return Err(error.into());
    }
    let observed = observed.map_err(WorldResultRouterErrorV1::Original)?;
    let hash = sink.hash.finalize();
    if observed != raw.len() as u64 || (exact && raw.len() != bound) || hash.as_bytes() != id {
        return Err(invalid().into());
    }
    *remaining = remaining.checked_add(allowance).ok_or_else(invalid)?;
    Ok(charge.retain(raw))
}

/// The single closed classification kernel for candidate and accepted results.
/// Callers must first authenticate the actual original ID/length and occurrence.
pub(super) fn classify_world_result_original_v1<C: FnMut() -> bool>(
    mut raw: &[u8],
    id: [u8; 32],
    roots: [u8; 32],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldResultRouterWorkV1,
    cancelled: &mut C,
) -> Result<WorldResultProfileV1, ValueTreeError> {
    let profile = if raw.starts_with(COGNITION_HISTORY_VIEW_V3_MAGIC) {
        let decoded = decode_cognition_history_view_v3(raw, memory, class, cancelled)?;
        if decoded.view.roots != roots {
            return Err(invalid());
        }
        WorldResultProfileV1::CognitionHistoryV3
    } else if raw
        .starts_with(crate::incremental_cognition_proof_ledger::COGNITION_PROOF_LEDGER_MAGIC_V1)
    {
        let decoded = crate::incremental_cognition_proof_ledger::CognitionProofLedgerManifestV1::from_canonical_bytes(raw)?;
        if decoded.roots != roots {
            return Err(invalid());
        }
        WorldResultProfileV1::CognitionProofLedgerV1
    } else if raw.starts_with(RUNTIME_HISTORY_MAGIC_V1) {
        let decoded = RuntimeHistoryManifestV1::from_canonical_bytes(raw)?;
        if decoded.roots != roots {
            return Err(invalid());
        }
        WorldResultProfileV1::RuntimeHistoryV1
    } else if let Some(kind) = runtime_label(raw, &mut work.envelope, cancelled)? {
        let profile = match kind {
            RuntimeHistoryDecodeKindV1::Snapshot => WorldResultProfileV1::SnapshotV1,
            RuntimeHistoryDecodeKindV1::Tick => WorldResultProfileV1::TickV1,
            RuntimeHistoryDecodeKindV1::Audit => WorldResultProfileV1::AuditV1,
        };
        decode_runtime_history_original_v1(kind, raw, memory, class, &mut work.runtime, cancelled)?;
        profile
    } else {
        verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: CognitionHistoryFieldV1::Responses,
                id,
                bytes: raw.len() as u64,
            },
            &mut raw,
            memory,
            class,
            &mut work.cognition,
            cancelled,
        )?;
        WorldResultProfileV1::CognitionResponseV1
    };
    Ok(profile)
}

pub fn route_checked_world_results_v1<O: OriginalObjectStreamReader, C: FnMut() -> bool>(
    request: &WorldResultRouterRequestV1<'_>,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut WorldResultRouterWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<CheckedWorldResultRoutingV1>, WorldResultRouterErrorV1<O::Error>> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let facts = &request.core.facts().facts;
    if facts.len() > EXECUTION_MAX_FACTS {
        return Err(invalid().into());
    }
    let roots = request.core.roots().digest().map_err(|_| invalid())?;
    let count = facts
        .iter()
        .filter(|fact| fact.kind == ExecutionFactKindV1::Result)
        .count();
    let metadata = count
        .checked_mul(std::mem::size_of::<RoutedWorldResultV1>())
        .and_then(|n| n.checked_add(1024))
        .ok_or_else(invalid)?;
    let outer = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let rows_charge = memory
        .reserve(metadata, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(count);
    let mut cognition = None;
    let mut runtime = None;
    let mut proof_ledger = None;
    for (ordinal, fact) in facts.iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if fact.kind != ExecutionFactKindV1::Result {
            continue;
        }
        work.facts.facts_remaining = work
            .facts
            .facts_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Result dispatch facts"))?;
        let occurrence = check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                candidate: request.candidate,
                core: request.core,
                originals: request.originals,
                ordinal: ordinal as u64,
            },
            memory,
            &mut cancelled,
        )
        .map_err(WorldResultRouterErrorV1::Formal)?;
        let length = occurrence.original_bytes();
        if length > work.facts.max_fact_bytes || length > work.facts.original_bytes_remaining {
            return Err(ValueTreeError::ResourceLimited("Result original limit").into());
        }
        let length = usize::try_from(length).map_err(|_| invalid())?;
        let raw = read_world_router_original_v1(
            objects,
            &fact.id,
            length,
            true,
            memory,
            MemoryClass::Ordinary,
            &mut work.facts.original_bytes_remaining,
            &mut cancelled,
        )?;
        let profile = classify_world_result_original_v1(
            &raw,
            fact.id,
            roots,
            memory,
            MemoryClass::Ordinary,
            work,
            &mut cancelled,
        )?;
        match profile {
            WorldResultProfileV1::CognitionProofLedgerV1
                if proof_ledger.replace(rows.len()).is_some() =>
            {
                return Err(invalid().into());
            }
            WorldResultProfileV1::CognitionHistoryV3 if cognition.replace(rows.len()).is_some() => {
                return Err(invalid().into());
            }
            WorldResultProfileV1::RuntimeHistoryV1 if runtime.replace(rows.len()).is_some() => {
                return Err(invalid().into());
            }
            _ => {}
        }
        if rows.len() >= count {
            return Err(invalid().into());
        }
        rows.push(RoutedWorldResultV1 {
            occurrence,
            profile,
        });
    }
    let cognition = cognition.ok_or_else(invalid)?;
    let runtime = runtime.ok_or_else(invalid)?;
    if rows.len() != count {
        return Err(invalid().into());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(outer.retain(CheckedWorldResultRoutingV1 {
        rows: rows_charge.retain(rows),
        cognition,
        runtime,
        proof_ledger,
    }))
}

#[cfg(test)]
#[path = "incremental_world_result_router_tests.rs"]
mod tests;
