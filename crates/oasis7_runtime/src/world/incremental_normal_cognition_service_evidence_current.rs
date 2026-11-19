//! One read scope over the actual mutable side-ledger and Journal owners.
//! Borrowing both owners prevents either frontier from changing during ANY
//! queries; only their actual same-tick deltas are folded into the parent AVL.
use super::*;
use oasis7_formal_protocol::incremental_execution::{ExecutionTransaction, PreparedGuard};
use oasis7_world_state::incremental_cognition_proof_ledger::CurrentCognitionProofLedgerV1;
pub(in crate::world) struct CurrentCognitionWakeEvidenceV2<'a, 'p> {
    parent: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
    pub(super) ledger: &'a CurrentCognitionProofLedgerV1,
    pub(super) pending: &'a PendingCurrentCognitionJournalV1<'p>,
    pub(super) index: index::Index,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl CurrentCognitionWakeEvidenceV2<'_, '_> {
    /// Borrow the actual immutable ledger held by this evidence scope. No
    /// independent ledger or caller equality assertion enters retry lookup.
    pub(in crate::world) fn ledger_for_retry_data_v1(&self) -> &CurrentCognitionProofLedgerV1 {
        self.ledger
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        self.ledger.validate_context(tx, common, live)?;
        self.pending.validate_evidence(tx, common.memory(), live)?;
        if self.core != common.work().identity()
            || !self.parent.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub(in crate::world) fn contains_event<M: MerkleNodeReader>(
        &self,
        digest: &str,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<bool, BodyMutationError> {
        self.validate(tx, common, live)?;
        common.charge(1, digest.len() as u64).map_err(formal)?;
        let Some(hex) = digest.strip_prefix("blake3:") else {
            return Ok(false);
        };
        if hex.len() != 64 {
            return Ok(false);
        }
        let mut bytes = [0; 32];
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
            bytes[i] = a * 16 + b;
        }
        self.index.has_event(&bytes, common)
    }
    pub(in crate::world) fn contains_receipt<M: MerkleNodeReader>(
        &self,
        id: &str,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<bool, BodyMutationError> {
        self.validate(tx, common, live)?;
        self.index.has_receipt(id, common)
    }
}
#[derive(Debug)]
pub(in crate::world) enum CurrentEvidenceErrorV2 {
    Body(BodyMutationError),
    Business(ReservedValue<crate::WorldError>),
}
impl From<BodyMutationError> for CurrentEvidenceErrorV2 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ValueTreeError> for CurrentEvidenceErrorV2 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError>
    for CurrentEvidenceErrorV2
{
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
fn decode_row<T: EvidenceRowDto>(
    value: &serde_json::Value,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(T, MemoryReservation), BodyMutationError> {
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
    let heap = std::mem::size_of::<T>()
        .checked_mul(4)
        .and_then(|n| {
            count
                .bytes
                .checked_mul(2)
                .and_then(|bytes| n.checked_add(bytes))
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(bad)?;
    if heap > max {
        return Err(ValueTreeError::ResourceLimited("Current evidence selected row").into());
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
    Ok((T::deserialize(value).map_err(formal)?, fee))
}
pub(in crate::world) fn prepare_current_cognition_wake_evidence_v2<'a, 'p, M, P, R, E, C>(
    parent: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
    accepted_ledger: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    normal_parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    ledger: &'a CurrentCognitionProofLedgerV1,
    pending: &'a PendingCurrentCognitionJournalV1<'p>,
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CurrentCognitionExpiryWorkV1,
    path: &mut oasis7_world_state::incremental_json_records::JsonScalarPathReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentCognitionWakeEvidenceV2<'a, 'p>>, CurrentEvidenceErrorV2>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    parent.validate_parent(normal_parent, accepted_ledger, common)?;
    if parent.profile != EvidenceJournalProfileV2::Active {
        return Err(bad().into());
    }
    ledger.validate_context(tx, common, live)?;
    pending.validate_evidence(tx, common.memory(), live)?;
    let mut index = parent.index.share(common)?;
    // Only the genuine Pending can supply its successful semantic append notes.
    let notes = {
        let (memory, meter, stop) = common.parts();
        let lease = meter
            .begin_child()
            .map_err(|e| CurrentEvidenceErrorV2::Body(formal(e)))?;
        let old_steps = work.hash.hash_nodes_remaining;
        let old_bytes = work.hash.input_bytes_remaining;
        let steps = old_steps.min(lease.remaining_steps());
        let bytes = old_bytes.min(lease.remaining_read_bytes());
        work.hash.hash_nodes_remaining = steps;
        work.hash.input_bytes_remaining = bytes;
        let call = pending.appended_semantic_digests_v2(
            parent.journal_records,
            tx,
            memory,
            &mut work.hash,
            live,
            stop,
        );
        let used_steps = steps.saturating_sub(work.hash.hash_nodes_remaining);
        let used_bytes = bytes.saturating_sub(work.hash.input_bytes_remaining);
        let broken =
            work.hash.hash_nodes_remaining > steps || work.hash.input_bytes_remaining > bytes;
        work.hash.hash_nodes_remaining = old_steps - used_steps;
        work.hash.input_bytes_remaining = old_bytes - used_bytes;
        let paid = lease
            .consume(used_steps, used_bytes)
            .map_err(|e| CurrentEvidenceErrorV2::Body(formal(e)));
        lease.finish();
        let notes = call?;
        paid?;
        if broken {
            return Err(bad().into());
        }
        notes
    };
    for (ordinal, digest) in notes.iter() {
        common
            .charge(1, 8)
            .map_err(|e| CurrentEvidenceErrorV2::Body(formal(e)))?;
        if *ordinal < parent.journal_records || *ordinal >= pending.count() {
            return Err(bad().into());
        }
        index.add_event(digest, common)?;
    }
    let changes = ledger.evidence_change_count_v2(tx, common, live)?;
    for at in 0..changes {
        let change = ledger.evidence_change_v2(at, tx, common, live)?;
        let field = change.field();
        let current_len = ledger.evidence_array_len_v2(field, tx, common, live)?;
        let range = change.changed_range().unwrap_or(
            0..current_len.max(if field == CognitionProofLedgerFieldV1::CommitRecords {
                parent.marker_rows
            } else {
                parent.receipt_rows
            }),
        );
        for ordinal in range {
            let selected = ledger.read_evidence_row_v2(
                field,
                ordinal,
                tx,
                read,
                &mut work.ledger.json,
                path,
                work.max_selected_heap,
                common,
                live,
            )?;
            if field == CognitionProofLedgerFieldV1::CommitRecords {
                if let Some(selected) = selected {
                    let (marker, _fee) = decode_row::<WorldCommitRecordV1>(
                        &selected,
                        work.max_selected_heap,
                        common,
                    )?;
                    index.replace_request_row(
                        ordinal,
                        Some(index::marker_identity(&marker)),
                        common,
                    )?;
                    if marker.status == "committed" {
                        let mut count = Count {
                            common,
                            bytes: 0,
                            error: None,
                        };
                        let encoded = serde_json::to_writer(&mut count, &marker);
                        if let Some(e) = count.error.take() {
                            return Err(CurrentEvidenceErrorV2::Body(formal(e)));
                        }
                        encoded.map_err(|e| CurrentEvidenceErrorV2::Body(formal(e)))?;
                        count
                            .common
                            .charge(32, count.bytes.checked_mul(32).ok_or_else(bad)? as u64)
                            .map_err(|e| CurrentEvidenceErrorV2::Body(formal(e)))?;
                        let phase = count
                            .bytes
                            .checked_mul(8)
                            .and_then(|n| n.checked_add(64 << 10))
                            .ok_or_else(bad)?;
                        let _phase = count
                            .common
                            .memory()
                            .reserve(phase, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?;
                        if let Err(error) =
                            crate::world::cognition_persistence::validate_cognition_commit_marker_v1(
                                &marker,
                            )
                        {
                            let fee = common
                                .memory()
                                .reserve(1024, MemoryClass::Ordinary)
                                .map_err(ValueTreeError::Memory)?;
                            return Err(CurrentEvidenceErrorV2::Business(fee.retain(error)));
                        }
                        index.replace_row(
                            true,
                            ordinal,
                            Some((&marker.receipt_id, &marker.receipt_digest)),
                            common,
                        )?;
                    } else {
                        index.replace_row(true, ordinal, None, common)?;
                    }
                } else {
                    index.replace_row(true, ordinal, None, common)?;
                    index.replace_request_row(ordinal, None, common)?;
                }
            } else if field == CognitionProofLedgerFieldV1::ReceiptRegistry {
                if let Some(selected) = selected {
                    let (receipt, _fee) = decode_row::<CognitionReceiptViewV1>(
                        &selected,
                        work.max_selected_heap,
                        common,
                    )?;
                    index.replace_row(
                        false,
                        ordinal,
                        Some((&receipt.receipt_id, &receipt.receipt_digest)),
                        common,
                    )?;
                } else {
                    index.replace_row(false, ordinal, None, common)?;
                }
            } else {
                return Err(bad().into());
            }
        }
    }
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<CurrentCognitionWakeEvidenceV2>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let value = fee.retain(CurrentCognitionWakeEvidenceV2 {
        parent,
        ledger,
        pending,
        index,
        guard: tx.guard(),
        core: common.work().identity(),
        _fee: common
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(()),
    });
    value.validate(tx, common, live)?;
    Ok(value)
}

impl CurrentCognitionWakeEvidenceV2<'_, '_> {
    /// Read the original first matching marker from the actual Current ledger.
    /// This is retry DATA only: the caller must still compare action/response,
    /// lifecycle status and receipt lineage before returning a disposition.
    pub(in crate::world) fn read_first_request_marker_v1<M, P, R, E, C>(
        &self,
        request: &oasis7_agent_api::world_cognition::RuntimeCognitionCommitRequestV1,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        work: &mut CurrentCognitionExpiryWorkV1,
        path: &mut oasis7_world_state::incremental_json_records::JsonScalarPathReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<ReservedValue<WorldCommitRecordV1>>, BodyMutationError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.validate(tx, common, live)?;
        let identity = [
            request.agent_id.as_str(),
            request.agent_session_id.as_str(),
            request.agent_turn_id.as_str(),
            request.decision_request_id.as_str(),
            request.request_digest.as_str(),
        ];
        let Some(ordinal) = self.index.first_request_marker(identity, common)? else {
            return Ok(None);
        };
        let selected = self
            .ledger
            .read_evidence_row_v2(
                CognitionProofLedgerFieldV1::CommitRecords,
                ordinal,
                tx,
                read,
                &mut work.ledger.json,
                path,
                work.max_selected_heap,
                common,
                live,
            )?
            .ok_or_else(bad)?;
        let (marker, fee) =
            decode_row::<WorldCommitRecordV1>(&selected, work.max_selected_heap, common)?;
        let actual = index::marker_identity(&marker);
        for (actual, expected) in actual.into_iter().zip(identity) {
            common
                .charge(1, actual.len().max(expected.len()) as u64)
                .map_err(formal)?;
            if actual != expected {
                return Err(bad().into());
            }
        }
        self.validate(tx, common, live)?;
        Ok(Some(fee.retain(marker)))
    }
}
