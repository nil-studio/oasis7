//! Actual Current Responses index. This participant preserves original response
//! wrappers; it grants neither Cognition execution nor accepted History authority.
use super::*;
use crate::incremental_cognition_history_cbor::{
    CognitionHistoryCborWorkV1, CognitionOriginalCborRequestV1,
    materialize_cognition_history_original_json_v1,
    verify_cognition_history_original_record_schema_v1,
};
use crate::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedFormalCommitV1,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_material::FactLocationV1,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub struct CurrentCognitionResponsesWorkV1<'a> {
    pub indexes: &'a mut CognitionHistoryIndexWorkV1,
    pub cbor: &'a mut CognitionHistoryCborWorkV1,
    pub processing: &'a mut BusinessDiffProcessingWorkV1,
}
pub struct PendingCurrentCognitionResponsesV1<'a> {
    parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    root: StateRootV1,
    count: u64,
    height: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    valid: bool,
    fee: ReservedValue<()>,
}
pub struct WrittenCurrentCognitionResponsesV1<'a> {
    parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    root: StateRootV1,
    count: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Cognition Responses source")
}
fn formal(e: FormalError) -> ValueTreeError {
    match e {
        FormalError::Invalid(ref s) if s == "bounded archive read cancelled" => {
            ValueTreeError::Cancelled
        }
        FormalError::Invalid(ref s)
            if s == "bounded archive read steps" || s == "bounded archive read bytes" =>
        {
            ValueTreeError::ResourceLimited("Responses Common work")
        }
        _ => bad(),
    }
}
fn exact<M: MerkleNodeReader>(
    guard: &PreparedGuard,
    tx: &ExecutionTransaction<'_, M>,
    live: &ExecutionFence,
) -> Result<(), ValueTreeError> {
    tx.validate_exact_transaction_identity(guard)
        .map_err(|_| bad())?;
    tx.validate_guard(&tx.guard(), live).map_err(|_| bad())
}
fn abort<M: MerkleNodeReader>(tx: &mut ExecutionTransaction<'_, M>) {
    let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
}
impl<'a> PendingCurrentCognitionResponsesV1<'a> {
    pub fn from_accepted<M: MerkleNodeReader>(
        parent: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
        actual_head: &VerifiedFormalCommitV1,
        roots: &RootSetV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, ValueTreeError> {
        common.charge(8192, 8192).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>() + 8192, MemoryClass::Ordinary)?
            .retain(());
        let view = parent.selected();
        let current = parent.current();
        let index = parent.indexes()[1].as_ref().ok_or_else(bad)?;
        exact(&tx.guard(), tx, live)?;
        let digest = roots.digest().map_err(|_| bad())?;
        if !tx.uses_budget(common.memory())
            || !parent.uses_budget(common.memory())
            || !view.uses_budget(common.memory())
            || !current.uses_budget(common.memory())
            || !index.uses_budget(common.memory())
            || view.class() != MemoryClass::Ordinary
            || current.class() != MemoryClass::Ordinary
            || index.allocation_class() != MemoryClass::Ordinary
            || !current.enabled()
            || !view.manifest().enabled
            || !view
                .manifest()
                .index_ref(CognitionHistoryFieldV1::Responses)
                .present
            || digest != tx.parent_fence().parent_roots
            || view.manifest().roots != digest
            || current.roots_id() != digest
            || index.roots_id() != digest
            || actual_head.verified_record_id() != view.occurrence().head_record_id()
            || current.head_record_id() != view.occurrence().head_record_id()
            || index.head_record_id() != view.occurrence().head_record_id()
            || view.occurrence().head_body_id() != tx.parent_fence().parent_commit
            || index.field() != CognitionHistoryFieldV1::Responses
            || index.root()
                != &view
                    .manifest()
                    .index_ref(CognitionHistoryFieldV1::Responses)
                    .root
            || index.record_count()
                != view
                    .manifest()
                    .index_ref(CognitionHistoryFieldV1::Responses)
                    .records
        {
            return Err(bad());
        }
        common.consume_steps(0).map_err(formal)?;
        Ok(Self {
            parent,
            root: index.root().clone(),
            count: index.record_count(),
            height: actual_head
                .record()
                .position
                .checked_add(1)
                .ok_or_else(bad)?,
            guard: tx.guard(),
            core: common.work().identity(),
            valid: true,
            fee,
        })
    }
    /// Borrowed locator DATA only. Every consumer must still validate this
    /// participant and bind its actual root/count to its retained source.
    pub fn root_data_v1(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(formal)?;
        exact(&self.guard, tx, live)?;
        if !self.valid
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.parent.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn read_index_point<M, P, R, E, C>(
        &mut self,
        ordinal: u64,
        tx: &ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        work: &mut CognitionHistoryIndexWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<CognitionHistoryIndexPointV1>, ValueTreeError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        if let Err(error) = self.validate_context(tx, common, live) {
            self.valid = false;
            return Err(error);
        }
        self.valid = false;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if ordinal >= self.count
                || read.values.class != MemoryClass::Ordinary
                || !self.fee.uses_budget(read.values.memory)
            {
                return Err(bad());
            }
            let (_, meter, stop) = common.parts();
            let child = meter.begin_child().map_err(formal)?;
            let original = [
                work.cells_remaining,
                read.values.io.nodes_remaining,
                read.values.io.read_bytes_remaining,
                read.key_work.bytes_remaining,
            ];
            work.cells_remaining = work.cells_remaining.min(child.remaining_steps() / 2);
            read.values.io.nodes_remaining = read
                .values
                .io
                .nodes_remaining
                .min(child.remaining_steps() / 2);
            read.values.io.read_bytes_remaining = read
                .values
                .io
                .read_bytes_remaining
                .min(child.remaining_read_bytes() / 2);
            read.key_work.bytes_remaining = read
                .key_work
                .bytes_remaining
                .min(child.remaining_read_bytes() / 2);
            let admitted = [
                work.cells_remaining,
                read.values.io.nodes_remaining,
                read.values.io.read_bytes_remaining,
                read.key_work.bytes_remaining,
            ];
            let call = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if stop() {
                    return Err(ValueTreeError::Cancelled);
                }
                super::index::read_cognition_history_index_point_parts_v1(
                    &self.root,
                    CognitionHistoryFieldV1::Responses,
                    ordinal,
                    read,
                    work,
                )
            }));
            let call = match call {
                Ok(r) => r,
                Err(p) => {
                    work.cells_remaining = 0;
                    read.values.io.nodes_remaining = 0;
                    read.values.io.read_bytes_remaining = 0;
                    read.key_work.bytes_remaining = 0;
                    drop(child);
                    std::panic::resume_unwind(p)
                }
            };
            let after = [
                work.cells_remaining,
                read.values.io.nodes_remaining,
                read.values.io.read_bytes_remaining,
                read.key_work.bytes_remaining,
            ];
            let mut delta = [0; 4];
            for i in 0..4 {
                delta[i] = admitted[i].checked_sub(after[i]).ok_or_else(bad)?;
            }
            work.cells_remaining = original[0] - delta[0];
            read.values.io.nodes_remaining = original[1] - delta[1];
            read.values.io.read_bytes_remaining = original[2] - delta[2];
            read.key_work.bytes_remaining = original[3] - delta[3];
            let paid = child
                .consume(delta[0] + delta[1], delta[2] + delta[3])
                .map_err(formal);
            child.finish();
            let value = call?;
            paid?;
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            Ok(value)
        }));
        match result {
            Ok(Ok(value)) => {
                self.valid = true;
                Ok(value)
            }
            Ok(Err(e)) => Err(e),
            Err(p) => std::panic::resume_unwind(p),
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub fn append<M, P, R, E, C, W, D>(
        &mut self,
        raw: &[u8],
        id: [u8; 32],
        location: FactLocationV1,
        tx: &mut ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CurrentCognitionResponsesWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        D: FnMut() -> bool,
    {
        self.edit(None, raw, id, location, tx, read, write, work, common, live)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    pub fn replace<M, P, R, E, C, W, D>(
        &mut self,
        ordinal: u64,
        expected: CognitionHistoryIndexRowV1,
        before_raw: &[u8],
        after_raw: &[u8],
        id: [u8; 32],
        location: FactLocationV1,
        tx: &mut ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CurrentCognitionResponsesWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        D: FnMut() -> bool,
    {
        self.edit(
            Some((ordinal, expected, before_raw)),
            after_raw,
            id,
            location,
            tx,
            read,
            write,
            work,
            common,
            live,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Bounded inputs are independently validated"
    )]
    fn edit<M, P, R, E, C, W, D>(
        &mut self,
        before: Option<(u64, CognitionHistoryIndexRowV1, &[u8])>,
        raw: &[u8],
        id: [u8; 32],
        location: FactLocationV1,
        tx: &mut ExecutionTransaction<'_, M>,
        read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, D>,
        work: &mut CurrentCognitionResponsesWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), MemberRecordError>
    where
        M: MerkleNodeReader,
        P: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        D: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.validate_context(tx, common, live)?;
            self.valid = false;
            if location.original != id
                || location.height != self.height
                || !tx
                    .ordered_fact(usize::try_from(location.ordinal).map_err(|_| bad())?)
                    .is_some_and(|f| f.kind == ExecutionFactKindV1::Result && f.id == id)
            {
                return Err(bad().into());
            }
            let next = metered(read, write, work, common, |read, write, work, stop| {
                let after_row = CognitionHistoryIndexRowV1 {
                    location,
                    encoded_bytes: u64::try_from(raw.len()).map_err(|_| bad())?,
                };
                if let Some((ordinal, expected, old_raw)) = before {
                    if ordinal >= self.count
                        || expected.encoded_bytes
                            != u64::try_from(old_raw.len()).map_err(|_| bad())?
                    {
                        return Err(bad().into());
                    }
                    let actual = super::index::read_cognition_history_index_point_parts_v1(
                        &self.root,
                        CognitionHistoryFieldV1::Responses,
                        ordinal,
                        read,
                        work.indexes,
                    )?;
                    if actual.row() != expected {
                        return Err(bad().into());
                    }
                    let (_, old) = materialize_cognition_history_original_json_v1(
                        CognitionOriginalCborRequestV1 {
                            field: CognitionHistoryFieldV1::Responses,
                            id: expected.location.original,
                            bytes: expected.encoded_bytes,
                        },
                        &mut &old_raw[..],
                        read.values.memory,
                        MemoryClass::Ordinary,
                        work.cbor,
                        work.processing,
                        || stop() || (read.cancelled)() || (write.cancelled)(),
                    )?;
                    let (_, new) = materialize_cognition_history_original_json_v1(
                        CognitionOriginalCborRequestV1 {
                            field: CognitionHistoryFieldV1::Responses,
                            id,
                            bytes: after_row.encoded_bytes,
                        },
                        &mut &raw[..],
                        read.values.memory,
                        MemoryClass::Ordinary,
                        work.cbor,
                        work.processing,
                        || stop() || (read.cancelled)() || (write.cancelled)(),
                    )?;
                    compare_response_change(
                        &old,
                        &new,
                        old_raw.len(),
                        read.values.memory,
                        work.processing,
                        &mut || stop() || (read.cancelled)() || (write.cancelled)(),
                    )?;
                    let root = super::index::replace_cognition_history_index_parts_v3(
                        &self.root,
                        CognitionHistoryFieldV1::Responses,
                        self.count,
                        ordinal,
                        expected,
                        after_row,
                        read,
                        write,
                        work.indexes,
                    )?;
                    Ok((root, self.count))
                } else {
                    let _schema = verify_cognition_history_original_record_schema_v1(
                        CognitionOriginalCborRequestV1 {
                            field: CognitionHistoryFieldV1::Responses,
                            id,
                            bytes: after_row.encoded_bytes,
                        },
                        &mut &raw[..],
                        read.values.memory,
                        MemoryClass::Ordinary,
                        work.cbor,
                        || stop() || (read.cancelled)() || (write.cancelled)(),
                    )?;
                    super::index::append_cognition_history_index_parts_v3(
                        &self.root,
                        CognitionHistoryFieldV1::Responses,
                        self.count,
                        after_row,
                        read,
                        write,
                        work.indexes,
                    )
                }
            })?;
            self.root = next.0;
            self.count = next.1;
            self.guard = tx.guard();
            self.valid = true;
            Ok(())
        }));
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.valid = false;
                abort(tx);
                Err(e)
            }
            Err(p) => {
                self.valid = false;
                abort(tx);
                std::panic::resume_unwind(p)
            }
        }
    }
    pub fn finish<M: MerkleNodeReader>(
        self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<WrittenCurrentCognitionResponsesV1<'a>, ValueTreeError> {
        self.validate_context(tx, common, live)?;
        Ok(WrittenCurrentCognitionResponsesV1 {
            parent: self.parent,
            root: self.root,
            count: self.count,
            guard: tx.guard(),
            core: self.core,
            fee: self.fee,
        })
    }
}
impl WrittenCurrentCognitionResponsesV1<'_> {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn validate_context<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        common.consume_steps(1).map_err(formal)?;
        tx.validate_exact_transaction_identity(&self.guard)
            .map_err(|_| bad())?;
        tx.validate_read_source_guard(&self.guard, live)
            .map_err(|_| bad())?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.parent.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[path = "incremental_cognition_history_current_responses_io.rs"]
mod io;
use io::metered;
#[path = "incremental_cognition_history_current_responses_compare.rs"]
mod compare;
use compare::compare_response_change;
