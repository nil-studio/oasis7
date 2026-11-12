//! Private side-ledger owner for one real execution transaction. It never adds
//! a Current RootSet leaf. Only its producer can replace a held field root.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};

use crate::incremental_json_records::{
    JsonChangeWorkV1, JsonPathSegmentV1, JsonSubtreeEditV1, JsonWriteWorkV1,
    advance_json_subtree_v1, copy_json_subtree_metadata_ordinary_v1,
    prepare_json_subtree_change_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
};
fn common_error(
    error: oasis7_formal_protocol::controlled_authority::formal::FormalError,
) -> ValueTreeError {
    match error {
        oasis7_formal_protocol::controlled_authority::formal::FormalError::Invalid(ref s)
            if s == "bounded archive read cancelled" =>
        {
            ValueTreeError::Cancelled
        }
        oasis7_formal_protocol::controlled_authority::formal::FormalError::Invalid(ref s)
            if s == "bounded archive read steps" || s == "bounded archive read bytes" =>
        {
            ValueTreeError::ResourceLimited("ProofLedger Common work")
        }
        _ => invalid(),
    }
}
pub struct CurrentCognitionProofLedgerV1 {
    core: WorkIdentityV1,
    _outer: ReservedValue<()>,
    parent_record: [u8; 32],
    parent_body: [u8; 32],
    parent_roots: [u8; 32],
    guard: PreparedGuard,
    fields: [Option<ReservedValue<JsonSubtreeQualificationV1>>; 5],
}
impl CurrentCognitionProofLedgerV1 {
    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(common_error)?;
        let memory = common.memory();
        tx.validate_transaction_identity(&self.guard)
            .map_err(|_| invalid())?;
        tx.validate_guard(&tx.guard(), live)
            .map_err(|_| invalid())?;
        if self.core != common.work().identity()
            || !self._outer.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.parent_body != tx.parent_fence().parent_commit
            || self.parent_roots != tx.parent_fence().parent_roots
            || self.fields.iter().flatten().any(|q| !q.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn read_field<M, P, R, E, C>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        field: CognitionProofLedgerFieldV1,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        scan: &mut StateScanBudget,
        work: &mut ProofLedgerReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<ProofLedgerRecoveredV1<serde_json::Value>>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.validate_context(tx, common, live)?;
        if read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        let Some(q) = self.fields[field.index()].as_ref() else {
            return Ok(None);
        };
        let root = q.root();
        let p = profile(field);
        q.validate_binding(
            JsonRecordProfileV1 {
                field: p.field,
                schema: p.schema,
            },
            root,
        )?;
        let (_, meter, stop) = common.parts();
        let child = meter.begin_child().map_err(|_| invalid())?;
        // Lower every independently consumable legacy lane so their SUM cannot
        // exceed the parent's balance. No quota is raised, reset or refunded.
        let visits = child.remaining_steps() / 5;
        let bytes = child.remaining_read_bytes() / 3;
        work.json.cells_remaining = work.json.cells_remaining.min(visits);
        work.json.json_nodes_remaining = work.json.json_nodes_remaining.min(visits);
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(visits);
        scan.nodes_remaining = scan.nodes_remaining.min(visits);
        scan.leaves_remaining = scan.leaves_remaining.min(visits);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
        scan.read_bytes_remaining = scan.read_bytes_remaining.min(bytes);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
        let before_steps = [
            work.json.cells_remaining,
            work.json.json_nodes_remaining,
            read.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
        ];
        let before_bytes = [
            read.values.io.read_bytes_remaining,
            scan.read_bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let result = {
            let read_stop = &mut read.cancelled;
            let mut field_read = WorldFieldReadContextV1 {
                root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || read_stop() || stop(),
            };
            typed::recover_typed_record_body_v1(root, &p, &mut field_read, scan, work).map(Some)
        };
        let after_steps = [
            work.json.cells_remaining,
            work.json.json_nodes_remaining,
            read.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
        ];
        let after_bytes = [
            read.values.io.read_bytes_remaining,
            scan.read_bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |sum, (b, a)| sum.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return result.and(Err(invalid()));
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return result.and(Err(invalid()));
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        match result {
            Err(e) => Err(e),
            Ok(value) => {
                paid.map_err(|_| ValueTreeError::ResourceLimited("ProofLedger Common read"))?;
                common.consume_steps(0).map_err(common_error)?;
                Ok(value)
            }
        }
    }
    pub fn freeze<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<FrozenCognitionProofLedgerV1, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        Ok(FrozenCognitionProofLedgerV1 {
            guard: tx.guard(),
            source: self,
        })
    }
}
pub fn begin_cognition_proof_ledger_current_v1<M: MerkleNodeReader>(
    parent: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    head: &VerifiedFormalCommitV1,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentCognitionProofLedgerV1, ValueTreeError> {
    let memory = common.memory().clone();
    let core = common.work().identity();
    let result = (|| {
        common.consume_steps(1).map_err(common_error)?;
        parent.check_pool(&memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(&memory)
            || !tx.uses_budget(&memory)
            || parent.occurrence.head_record_id() != head.verified_record_id()
            || parent.occurrence.head_body_id() != tx.parent_fence().parent_commit
            || parent.manifest.roots != tx.parent_fence().parent_roots
        {
            return Err(invalid());
        }
        tx.validate_guard(&tx.guard(), live)
            .map_err(|_| invalid())?;
        let fee = memory.reserve(
            std::mem::size_of::<CurrentCognitionProofLedgerV1>(),
            MemoryClass::Ordinary,
        )?;
        let mut fields = std::array::from_fn(|_| None);
        for field in CognitionProofLedgerFieldV1::ALL {
            if let Some(q) = &parent.fields[field.index()] {
                let p = profile(field);
                common.charge(1, 512).map_err(common_error)?;
                let (_, _, stop) = common.parts();
                fields[field.index()] = Some(copy_json_subtree_metadata_ordinary_v1(
                    q,
                    JsonRecordProfileV1 {
                        field: p.field,
                        schema: p.schema,
                    },
                    &memory,
                    stop,
                )?);
            }
        }
        common.consume_steps(1).map_err(common_error)?;
        Ok(CurrentCognitionProofLedgerV1 {
            core,
            _outer: fee.retain(()),
            parent_record: head.verified_record_id(),
            parent_body: parent.occurrence.head_body_id(),
            parent_roots: parent.manifest.roots,
            guard: tx.guard(),
            fields,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub fn edit_current_cognition_proof_ledger_v1<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionProofLedgerV1,
    field: CognitionProofLedgerFieldV1,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    producer: &mut JsonWriteWorkV1,
    changed: &mut JsonChangeWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    let result = (|| {
        current.validate_context(tx, common, live)?;
        if !read.values.memory.same_pool(write.memory)
            || !common.memory().same_pool(write.memory)
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let old = current.fields[field.index()].as_ref().ok_or_else(invalid)?;
        let p = profile(field);
        let jp = JsonRecordProfileV1 {
            field: p.field,
            schema: p.schema,
        };
        let (_, meter, stop) = common.parts();
        let child = meter.begin_child().map_err(|_| invalid())?;
        let visits = child.remaining_steps() / 7;
        let bytes = child.remaining_read_bytes() / 5;
        producer.cells_remaining = producer.cells_remaining.min(visits);
        producer.json_nodes_remaining = producer.json_nodes_remaining.min(visits);
        changed.json.cells_remaining = changed.json.cells_remaining.min(visits);
        changed.json.json_nodes_remaining = changed.json.json_nodes_remaining.min(visits);
        changed.paths_remaining = changed.paths_remaining.min(visits);
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(visits);
        write.io.nodes_remaining = write.io.nodes_remaining.min(visits);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
        write.io.read_bytes_remaining = write.io.read_bytes_remaining.min(bytes);
        write.io.write_bytes_remaining = write.io.write_bytes_remaining.min(bytes);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
        write.key_work.bytes_remaining = write.key_work.bytes_remaining.min(bytes);
        let before_steps = [
            producer.cells_remaining,
            producer.json_nodes_remaining,
            changed.json.cells_remaining,
            changed.json.json_nodes_remaining,
            changed.paths_remaining,
            read.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ];
        let before_bytes = [
            read.values.io.read_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            read.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ];
        let produced = (|| -> Result<_, WorldFieldOverlayErrorV1> {
            let stop = std::cell::RefCell::new(stop);
            let read_stop = &mut read.cancelled;
            let write_stop = &mut write.cancelled;
            let mut field_read = WorldFieldReadContextV1 {
                root: old.root(),
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || read_stop() || (stop.borrow_mut())(),
            };
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || write_stop() || (stop.borrow_mut())(),
            };
            let change = prepare_json_subtree_change_v1(
                jp,
                old,
                path,
                edit,
                &mut field_read,
                &mut output,
                producer,
            )?;
            let next = advance_json_subtree_v1(jp, old, change.after(), &mut field_read, changed)?;
            if field.index() < 3 {
                crate::incremental_json_records::require_json_container_records_v1(
                    jp,
                    &next,
                    true,
                    &mut field_read.records(),
                    &mut changed.json,
                )?;
            }
            if (output.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(next)
        })();
        let after_steps = [
            producer.cells_remaining,
            producer.json_nodes_remaining,
            changed.json.cells_remaining,
            changed.json.json_nodes_remaining,
            changed.paths_remaining,
            read.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ];
        let after_bytes = [
            read.values.io.read_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            read.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |sum, (b, a)| sum.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return produced.and(Err(invalid().into()));
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return produced.and(Err(invalid().into()));
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        let next = produced?;
        paid.map_err(|_| ValueTreeError::ResourceLimited("ProofLedger Common producer"))?;
        common.consume_steps(0).map_err(common_error)?;
        current.fields[field.index()] = Some(next);
        Ok(())
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Consumes the unique mutable owner before the genuine sole transaction seal.
pub struct FrozenCognitionProofLedgerV1 {
    source: CurrentCognitionProofLedgerV1,
    guard: PreparedGuard,
}
impl FrozenCognitionProofLedgerV1 {
    pub fn manifest(
        &self,
        after: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<CognitionProofLedgerManifestV1>, ValueTreeError> {
        common.consume_steps(1).map_err(common_error)?;
        if self.source.core != common.work().identity()
            || !self.source._outer.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        let _phase = common.memory().reserve(4096, MemoryClass::Ordinary)?;
        after.validate().map_err(|_| invalid())?;
        let fee = common.memory().reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            MemoryClass::Ordinary,
        )?;
        let fields =
            std::array::from_fn(|i| self.source.fields[i].as_ref().map(|q| q.root().clone()));
        Ok(fee.retain(CognitionProofLedgerManifestV1 {
            roots: after.digest().map_err(|_| invalid())?,
            previous_record: Some(self.source.parent_record),
            fields,
        }))
    }
    pub fn validate_sealed(
        &self,
        sealed: &oasis7_formal_protocol::incremental_execution::SealedExecutionBindingV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(common_error)?;
        if self.source.core != common.work().identity() {
            return Err(invalid());
        }
        sealed
            .validate_source_guard(&self.guard, common.memory())
            .map_err(|_| invalid())
    }
}
