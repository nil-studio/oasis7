//! Fixed real append/replace and resident locator maintenance. There is no
//! independent root setter or caller-envelope metadata admission.
use super::*;
use oasis7_formal_protocol::incremental_material::{FactBudget, FactLocationV1};
use oasis7_world_state::incremental_cognition_history::CurrentCognitionResponsesWorkV1;
use std::sync::Arc;
impl CurrentCognitionResponseLocatorV1 {
    fn bind_after_actual_edit<M: MerkleNodeReader>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &PendingCurrentCognitionResponsesV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        responses.validate_context(tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        let root = responses.root_data_v1();
        let n = layout(Some(root), size_of::<StateRootV1>())?;
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let next = clone_root(Some(root), common)?;
        self.rows.root = next;
        self.rows.count = responses.count();
        self._root_update_fee = Some(fee);
        self.validate(parent, responses, tx, common, live)
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// The supplied bytes/location are still DATA: the unique WS append verifies
    /// their original schema/hash and actual current-TX Result occurrence first.
    /// The locator derives the envelope only by reading back that true new row.
    pub(in crate::world) fn append_current_cognition_response_and_locator_v1<P, V, PE, PC>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &mut PendingCurrentCognitionResponsesV1<'_>,
        locator: &mut CurrentCognitionResponseLocatorV1,
        raw: &Arc<ReservedValue<Vec<u8>>>,
        location: FactLocationV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionResponsesWorkV1<'_>,
        originals: &mut FactBudget,
        max_selected_heap: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            locator.validate(parent, responses, self.ctx.transaction, common, &live())?;
            if !raw.uses_budget(common.memory()) {
                return Err(bad("response append actual original pool").into());
            }
            let ordinal = responses.count();
            let count = ordinal
                .checked_add(1)
                .ok_or_else(|| bad("response append ordinal overflow"))?;
            responses
                .append(
                    raw,
                    location.original,
                    location,
                    self.ctx.transaction,
                    read,
                    self.members,
                    work,
                    common,
                    &live(),
                )
                .map_err(|e| BodyMutationError::Value(e.into_value_error()))?;
            if responses.count() != count {
                return Err(bad("response append actual count delta").into());
            }
            let mut read_work = CurrentCognitionResponseReadWorkV1 {
                indexes: &mut *work.indexes,
                cbor: &mut *work.cbor,
                processing: &mut *work.processing,
                originals,
                max_selected_heap,
            };
            let actual = self.read_current_cognition_response_original_v1(
                parent,
                responses,
                ordinal,
                read,
                &mut read_work,
                common,
                live,
            )?;
            locator.rows.index.replace_row(
                ordinal,
                Some(&actual.response.envelope_digest),
                common,
            )?;
            locator.bind_after_actual_edit(
                parent,
                responses,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            Ok(())
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
    /// Replacement consumes an actual match cut. WS checks the old indexed row,
    /// both original blobs and equality of every field except journal_head.
    pub(in crate::world) fn replace_current_cognition_response_journal_head_v1<P, V, PE, PC>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &mut PendingCurrentCognitionResponsesV1<'_>,
        locator: &mut CurrentCognitionResponseLocatorV1,
        matched: CurrentCognitionResponseMatchV1,
        after_raw: &Arc<ReservedValue<Vec<u8>>>,
        location: FactLocationV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionResponsesWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            locator.validate(parent, responses, self.ctx.transaction, common, &live())?;
            matched.validate(responses, self.ctx.transaction, common, &live())?;
            if !after_raw.uses_budget(common.memory()) {
                return Err(bad("response replacement actual original pool").into());
            }
            let count = responses.count();
            responses
                .replace(
                    matched.original.point.ordinal(),
                    matched.original.point.row(),
                    &matched.original.raw,
                    after_raw,
                    location.original,
                    location,
                    self.ctx.transaction,
                    read,
                    self.members,
                    work,
                    common,
                    &live(),
                )
                .map_err(|e| BodyMutationError::Value(e.into_value_error()))?;
            if responses.count() != count {
                return Err(bad("response journal replacement count changed").into());
            }
            // This fixed replacement cannot change envelope/artifact. Preserve the
            // original ordinal mapping; do not perform a response-history rescan.
            locator.bind_after_actual_edit(
                parent,
                responses,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            Ok(())
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
}
