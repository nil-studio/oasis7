//! Resident ANY evidence projected once from authenticated accepted originals.
//! This cache is DATA: its presence does not admit a continuation or authorize
//! a Current read. Ordinary service also needs the actual pending/ledger delta.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::*,
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryWorkV1, with_current_cognition_io_v1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    cognition_recovery::{CognitionReceiptViewV1, WorldCommitRecordV1},
    incremental_cognition_history::*,
    incremental_cognition_history_cbor::{
        CognitionOriginalCborRequestV1, verify_cognition_history_original_record_schema_v1,
    },
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CognitionProofLedgerFieldV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
use serde::Deserialize;
#[path = "incremental_normal_cognition_service_evidence_index.rs"]
pub(in crate::world) mod index;
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("accepted Cognition ANY evidence source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
/// Cold/bootstrap cache. No public constructor, Clone, root setter or empty
/// fallback. Even an empty cache was built from both actual accepted sources.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EvidenceJournalProfileV2 {
    Active,
    Disabled,
}
pub(in crate::world) struct AcceptedCognitionWakeEvidenceV2 {
    head: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    ledger_original: [u8; 32],
    profile: EvidenceJournalProfileV2,
    journal_records: u64,
    marker_rows: u64,
    receipt_rows: u64,
    index: index::Index,
    _scope: ReservedValue<()>,
}
impl AcceptedCognitionWakeEvidenceV2 {
    pub(in crate::world) fn journal_records(&self) -> usize {
        self.journal_records as usize
    }
    pub(in crate::world) fn validate_parent(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        ledger: &ReservedValue<AcceptedCognitionProofLedgerV1>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        common.charge(1, 128).map_err(formal)?;
        let selected = parent.refs().cognition.selected();
        if self.head != selected.occurrence().head_record_id()
            || self.body != selected.occurrence().head_body_id()
            || self.roots != selected.manifest().roots
            || self.ledger_original != ledger.occurrence().location().original
            || !ledger.uses_budget(common.memory())
            || !self._scope.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
    /// Original ANY semantics: every authenticated Journal semantic digest is
    /// eligible, independent of its lifecycle kind or a latest-state alias.
    pub(in crate::world) fn contains_event(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        common.charge(1, digest.len() as u64).map_err(formal)?;
        let Some(hex) = digest.strip_prefix("blake3:") else {
            return Ok(false);
        };
        if hex.len() != 64 {
            return Ok(false);
        }
        let mut bytes = [0u8; 32];
        for (i, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
            fn nibble(c: u8) -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    _ => None,
                }
            }
            let (Some(a), Some(b)) = (nibble(pair[0]), nibble(pair[1])) else {
                return Ok(false);
            };
            bytes[i] = (a << 4) | b;
        }
        self.index.has_event(&bytes, common)
    }
    /// Preserve the original existential pair join. A receipt identifier alone
    /// or an uncommitted marker never becomes evidence.
    pub(in crate::world) fn contains_receipt(
        &self,
        id: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        self.index.has_receipt(id, common)
    }
}
struct Count<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    bytes: usize,
    error: Option<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
}
impl std::io::Write for Count<'_, '_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self.error.is_some() {
            return Err(std::io::ErrorKind::Other.into());
        }
        if let Err(e) = self.common.charge(1, b.len() as u64) {
            self.error = Some(e);
            return Err(std::io::ErrorKind::Other.into());
        }
        self.bytes = self
            .bytes
            .checked_add(b.len())
            .ok_or(std::io::ErrorKind::InvalidData)?;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
trait EvidenceRowDto: for<'de> Deserialize<'de> {}
impl EvidenceRowDto for WorldCommitRecordV1 {}
impl EvidenceRowDto for CognitionReceiptViewV1 {}
fn decode_rows<T: EvidenceRowDto>(
    value: &serde_json::Value,
    max_heap: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(Vec<T>, MemoryReservation), BodyMutationError> {
    let mut count = Count {
        common,
        bytes: 0,
        error: None,
    };
    let result = serde_json::to_writer(&mut count, value);
    if let Some(e) = count.error.take() {
        return Err(formal(e));
    }
    result.map_err(formal)?;
    // Only WorldCommitRecordV1 (fixed scalar/String/OptionString fields) and
    // CognitionReceiptViewV1 call this helper. Four complete row buffers cover
    // Vec growth peaks; all String payloads fit twice the actual JSON wire.
    // The legacy hash expressions have their own per-marker phase below.
    let rows = value.as_array().ok_or_else(bad)?.len();
    let heap = rows
        .checked_mul(std::mem::size_of::<T>())
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| {
            count
                .bytes
                .checked_mul(2)
                .and_then(|bytes| n.checked_add(bytes))
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited(
            "Cognition evidence selected rows",
        ))?;
    if heap > max_heap {
        return Err(ValueTreeError::ResourceLimited("Cognition evidence selected rows").into());
    }
    count
        .common
        .charge(count.bytes as u64, count.bytes as u64)
        .map_err(formal)?;
    let fee = count
        .common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let rows = Vec::<T>::deserialize(value).map_err(formal)?;
    Ok((rows, fee))
}
enum LedgerSource<'a> {
    Accepted(&'a ReservedValue<AcceptedCognitionProofLedgerV1>),
    Checked(
        &'a ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1,
        >,
    ),
}
impl LedgerSource<'_> {
    fn manifest(
        &self,
    ) -> &oasis7_world_state::incremental_cognition_proof_ledger::CognitionProofLedgerManifestV1
    {
        match self {
            Self::Accepted(value) => value.manifest(),
            Self::Checked(value) => value.manifest(),
        }
    }
    fn read_field<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        &self,
        field: CognitionProofLedgerFieldV1,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
        work: &mut oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerReadWorkV1,
    ) -> Result<
        Option<
            oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerRecoveredV1<
                serde_json::Value,
            >,
        >,
        ValueTreeError,
    > {
        match self {
            Self::Accepted(value) => value.read_field(field, read, scan, work),
            Self::Checked(value) => value.read_field(field, read, scan, work),
        }
    }
}
// The accepted ledger reader has independent JSON/scan lanes. Reconcile
// them directly under one child rather than hiding them under Journal IO.
fn read_accepted_ledger_fields<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ledger: LedgerSource<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    (
        Option<
            oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerRecoveredV1<
                serde_json::Value,
            >,
        >,
        Option<
            oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerRecoveredV1<
                serde_json::Value,
            >,
        >,
    ),
    BodyMutationError,
> {
    common.charge(1, 4096).map_err(formal)?;
    let (_, meter, stop) = common.parts();
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut lease = meter.begin_child().map_err(formal)?;
    fn lanes<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        work: &mut CurrentCognitionExpiryWorkV1,
        f: &mut impl FnMut(&mut u64, bool),
    ) {
        for value in [
            &mut ctx.values.io.nodes_remaining,
            &mut work.ledger.json.cells_remaining,
            &mut work.ledger.json.json_nodes_remaining,
            &mut work.ledger_scan.nodes_remaining,
            &mut work.ledger_scan.leaves_remaining,
        ] {
            f(value, false)
        }
        for value in [
            &mut ctx.values.io.read_bytes_remaining,
            &mut ctx.key_work.bytes_remaining,
            &mut work.ledger_scan.read_bytes_remaining,
        ] {
            f(value, true)
        }
    }
    let mut original = [0u64; 8];
    let mut before = [0u64; 8];
    let mut i = 0;
    lanes(ctx, work, &mut |lane, bytes| {
        original[i] = *lane;
        *lane = (*lane).min(if bytes {
            lease.remaining_read_bytes() / 3
        } else {
            lease.remaining_steps() / 5
        });
        before[i] = *lane;
        i += 1;
    });
    let call: Result<_, BodyMutationError> = (|| {
        let mut cancelled = || stop() || (ctx.cancelled)();
        let mut read = WorldFieldReadContextV1 {
            root: ledger.manifest().fields[0].as_ref().ok_or_else(bad)?,
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut cancelled,
        };
        let markers = ledger.read_field(
            CognitionProofLedgerFieldV1::CommitRecords,
            &mut read,
            &mut work.ledger_scan,
            &mut work.ledger,
        )?;
        let receipts = ledger.read_field(
            CognitionProofLedgerFieldV1::ReceiptRegistry,
            &mut read,
            &mut work.ledger_scan,
            &mut work.ledger,
        )?;
        Ok((markers, receipts))
    })();
    let mut steps = 0u64;
    let mut bytes = 0u64;
    let mut i = 0;
    let mut invalid = false;
    lanes(ctx, work, &mut |lane, is_bytes| {
        let used = before[i].saturating_sub(*lane);
        invalid |= *lane > before[i];
        *lane = original[i] - used;
        let delta = used;
        i += 1;
        let total = if is_bytes { &mut bytes } else { &mut steps };
        if let Some(next) = total.checked_add(delta) {
            *total = next
        } else {
            invalid = true
        }
    });
    let accounting = if invalid {
        Err(bad().into())
    } else {
        lease.consume(steps, bytes).map_err(formal)
    };
    lease.finish();
    match call {
        Err(error) => Err(error),
        Ok(value) => {
            accounting?;
            if stop() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(value)
        }
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Only GEN/cold/cache-admission calls this full-source constructor. Normal
    /// service consumes a mandatory retained cache and genuine changed rows.
    pub(in crate::world) fn build_accepted_cognition_wake_evidence_v2(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        ledger: &ReservedValue<AcceptedCognitionProofLedgerV1>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<AcceptedCognitionWakeEvidenceV2>, RootModuleExecutionErrorV1<O::Error>>
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let selected = parent.refs().cognition.selected();
            let roots = parent.refs().roots;
            let index = parent.refs().cognition.indexes()[0].as_ref();
            let profile = if index.is_some() {
                EvidenceJournalProfileV2::Active
            } else if parent.refs().cognition.disabled().is_some() {
                EvidenceJournalProfileV2::Disabled
            } else {
                return Err(bad().into());
            };
            common
                .charge(1, 256)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if !ledger.uses_budget(common.memory())
                || ledger.occurrence().head_record_id() != selected.occurrence().head_record_id()
                || ledger.occurrence().head_body_id() != selected.occurrence().head_body_id()
                || selected.manifest().roots != self.ctx.transaction.parent_fence().parent_roots
                || selected.occurrence().head_body_id()
                    != self.ctx.transaction.parent_fence().parent_commit
                || index.is_some_and(|index| {
                    index.field() != CognitionHistoryFieldV1::JournalEvents
                        || index.head_record_id() != selected.occurrence().head_record_id()
                        || index.roots_id() != selected.manifest().roots
                        || index.root()
                            != &selected
                                .manifest()
                                .index_ref(CognitionHistoryFieldV1::JournalEvents)
                                .root
                })
            {
                return Err(bad().into());
            }
            let count = usize::try_from(index.map_or(0, |index| index.record_count()))
                .map_err(|_| bad())?;
            let bytes = count.checked_mul(32).ok_or_else(bad)?;
            let _event_fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            common
                .charge(1, bytes as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let mut previous = None;
            let mut events = Vec::new();
            events
                .try_reserve_exact(count)
                .map_err(|_| ValueTreeError::ResourceLimited("Cognition evidence events"))?;
            if events.capacity() > count {
                return Err(
                    ValueTreeError::ResourceLimited("Cognition evidence events capacity").into(),
                );
            }
            for ordinal in 0..count as u64 {
                let index = index.ok_or_else(bad)?;
                let point = with_current_cognition_io_v1(
                    self.ctx,
                    self.members,
                    work,
                    common,
                    |ctx, _, work| {
                        let mut read = WorldFieldReadContextV1 {
                            root: roots.state_root(StateNamespaceV1::Artifacts),
                            state: self.state,
                            values: ValueReadContext {
                                source: ctx.values.source,
                                memory: ctx.values.memory,
                                class: ctx.values.class,
                                io: &mut *ctx.values.io,
                            },
                            key_work: &mut *ctx.key_work,
                            semantic_bytes: ctx.semantic_bytes,
                            record_bytes: ctx.record_bytes,
                            take_state_error: &mut ctx.take_state_error,
                            cancelled: &mut ctx.cancelled,
                        };
                        read_attested_cognition_history_index_point_v3(
                            index,
                            ordinal,
                            &mut read,
                            &mut work.index,
                        )
                        .map_err(Into::into)
                    },
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                let row = point.row();
                let raw = with_current_root_module_meter_v1(
                    self,
                    common,
                    live_owner,
                    |root, _| {
                        root.work.input.bytes_remaining = root
                            .work
                            .input
                            .bytes_remaining
                            .checked_sub(row.encoded_bytes)
                            .ok_or(ValueTreeError::ResourceLimited("evidence Original bytes"))?;
                        let raw=root.originals.read_original(&row.location.original,usize::try_from(row.encoded_bytes).map_err(|_|bad())?,root.ctx.values.memory).map_err(|e|RootModuleExecutionErrorV1::Invocation(ModuleInvocationPreparationErrorV1::Original(oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalErrorV1::Source(e))))?;
                        if !raw.uses_budget(root.ctx.values.memory)
                            || raw.len() as u64 != row.encoded_bytes
                        {
                            return Err(bad().into());
                        }
                        Ok(raw)
                    },
                )?;
                let schema = with_current_cognition_io_v1(
                    self.ctx,
                    self.members,
                    work,
                    common,
                    |ctx, _, work| {
                        verify_cognition_history_original_record_schema_v1(
                            CognitionOriginalCborRequestV1 {
                                field: CognitionHistoryFieldV1::JournalEvents,
                                id: row.location.original,
                                bytes: row.encoded_bytes,
                            },
                            &mut raw.as_slice(),
                            ctx.values.memory,
                            MemoryClass::Ordinary,
                            &mut work.cbor,
                            &mut ctx.cancelled,
                        )
                        .map_err(Into::into)
                    },
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                let CognitionOriginalIdentityStatusV1::DigestMatched(digest) =
                    schema.semantic().identity_status()
                else {
                    return Err(bad().into());
                };
                if schema.semantic().sequence() != ordinal.checked_add(1)
                    || schema.semantic().parent() != Some(previous)
                {
                    return Err(bad().into());
                }
                previous = Some(digest);
                events.push(digest);
            }
            let fields = {
                let ctx = &mut self.ctx;
                let mut read = WorldFieldReadContextV1 {
                    root: roots.state_root(StateNamespaceV1::Artifacts),
                    state: self.state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory: ctx.values.memory,
                        class: ctx.values.class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                read_accepted_ledger_fields(LedgerSource::Accepted(ledger), &mut read, work, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?
            };
            // Absent fields are not silently replaced with an empty array. The accepted
            // ledger's actual typed array (including a genuine empty one) is required.
            let (markers, _marker_fee) = decode_rows::<WorldCommitRecordV1>(
                fields.0.as_ref().ok_or_else(bad)?.value(),
                work.max_selected_heap,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let (receipts, _receipt_fee) = decode_rows::<CognitionReceiptViewV1>(
                fields.1.as_ref().ok_or_else(bad)?.value(),
                work.max_selected_heap,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            for marker in markers.iter().filter(|marker| marker.status == "committed") {
                let mut count = Count {
                    common,
                    bytes: 0,
                    error: None,
                };
                let encoded = serde_json::to_writer(&mut count, marker);
                if let Some(e) = count.error.take() {
                    return Err(RootModuleExecutionErrorV1::Body(formal(e)));
                }
                encoded.map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                let phase = count
                    .bytes
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(64 << 10))
                    .ok_or_else(bad)?;
                count
                    .common
                    .charge(32, count.bytes.checked_mul(32).ok_or_else(bad)? as u64)
                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                let _phase = count
                    .common
                    .memory()
                    .reserve(phase, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                if let Err(error) =
                    crate::world::cognition_persistence::validate_cognition_commit_marker_v1(marker)
                {
                    let fee = common
                        .memory()
                        .reserve(1024, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    return Err(RootModuleExecutionErrorV1::Business(fee.retain(error)));
                }
            }
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<AcceptedCognitionWakeEvidenceV2>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            if profile == EvidenceJournalProfileV2::Disabled
                && (!markers.is_empty() || !receipts.is_empty())
            {
                return Err(bad().into());
            }
            let index = index::Index::from_rows(&events, &markers, &receipts, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let owner = fee.retain(AcceptedCognitionWakeEvidenceV2 {
                head: selected.occurrence().head_record_id(),
                body: selected.occurrence().head_body_id(),
                roots: selected.manifest().roots,
                ledger_original: ledger.occurrence().location().original,
                profile,
                journal_records: count as u64,
                marker_rows: markers.len() as u64,
                receipt_rows: receipts.len() as u64,
                index,
                _scope: common
                    .memory()
                    .reserve(0, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?
                    .retain(()),
            });
            owner
                .validate_parent(parent, ledger, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(owner)
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}

#[path = "incremental_normal_cognition_service_evidence_checked.rs"]
mod checked;
pub(in crate::world) use checked::*;

#[path = "incremental_normal_cognition_service_evidence_current.rs"]
mod current;
pub(in crate::world) use current::*;

#[path = "incremental_normal_cognition_service_evidence_freeze.rs"]
mod freeze;
pub(in crate::world) use freeze::*;

#[cfg(test)]
#[path = "incremental_normal_cognition_service_evidence_tests.rs"]
mod tests;
