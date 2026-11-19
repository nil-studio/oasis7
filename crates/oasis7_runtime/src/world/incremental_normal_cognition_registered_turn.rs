//! Registration evidence from one real Journal locator, not the latest
//! lifecycle state. The locator is DATA until its authenticated row and actual
//! Original prove the original four-kind / request-or-origin predicate.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::*,
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryWorkV1, with_current_cognition_io_v1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_agent_api::iws_scheduler_v2::{IwsContinuationProposalV2, IwsJournalReferenceV2};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFence, ExecutionTransaction, PreparedGuard,
    },
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::*,
    incremental_cognition_history_cbor::{
        CognitionHistoryRecordSchemaV1, CognitionOriginalCborRequestV1,
        CognitionOriginalTextProjectionV1, verify_cognition_history_original_record_schema_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("registered Cognition turn actual source")
}
fn formal(error: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(error))
}
fn unregistered<OE>(common: &mut FormalBoundedContextV1<'_>) -> RootModuleExecutionErrorV1<OE> {
    const REASON: &str = "cognition validation failed: continuation_turn_unregistered";
    if let Err(error) = common.charge(1, REASON.len() as u64) {
        return RootModuleExecutionErrorV1::Body(formal(error));
    }
    let fee = match common.memory().reserve(
        std::mem::size_of::<crate::WorldError>() + REASON.len(),
        MemoryClass::Ordinary,
    ) {
        Ok(fee) => fee,
        Err(error) => return ValueTreeError::Memory(error).into(),
    };
    RootModuleExecutionErrorV1::Business(fee.retain(
        crate::WorldError::DistributedValidationFailed {
            reason: REASON.to_owned(),
        },
    ))
}
/// Authenticated Journal data, before any registration/admission predicate.
/// Its source check is shared by both business consumers below.
struct AuthenticatedCurrentCognitionJournalEventV2 {
    point: ReservedValue<CognitionHistoryIndexPointV1>,
    schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    raw: std::sync::Arc<ReservedValue<Vec<u8>>>,
    guard: PreparedGuard,
    parent: ExecutionFence,
    core: WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl AuthenticatedCurrentCognitionJournalEventV2 {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(2, 160).map_err(formal)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || tx.parent_fence() != self.parent
            || !self._fee.uses_budget(common.memory())
            || !self.point.uses_budget(common.memory())
            || !self.schema.uses_budget(common.memory())
            || !self.raw.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
}
/// Private mint. The paid identity strings were compared against the actual
/// Original's UTF8 commitments; the final string is the verified OR correlation
/// digest and need not be that event's newer request_digest.
pub(in crate::world) struct CurrentRegisteredCognitionTurnV2 {
    identity: [String; 5],
    registration: IwsJournalReferenceV2,
    source: ReservedValue<AuthenticatedCurrentCognitionJournalEventV2>,
    _fee: ReservedValue<()>,
}
impl CurrentRegisteredCognitionTurnV2 {
    pub(in crate::world) fn agent_id(&self) -> &str {
        &self.identity[0]
    }
    pub(in crate::world) fn agent_session_id(&self) -> &str {
        &self.identity[1]
    }
    pub(in crate::world) fn agent_turn_id(&self) -> &str {
        &self.identity[2]
    }
    pub(in crate::world) fn decision_request_id(&self) -> &str {
        &self.identity[3]
    }
    pub(in crate::world) fn correlation_digest(&self) -> &str {
        &self.identity[4]
    }
    pub(in crate::world) fn registration(&self) -> &IwsJournalReferenceV2 {
        &self.registration
    }
    pub(in crate::world) fn original(&self) -> &[u8] {
        &self.source.raw
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(tx, common, live)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(bad().into());
        }
        Ok(())
    }
}
/// Borrowed query data only. Implementing this trait cannot mint admission:
/// the fixed consumer below must match an actual authenticated Journal event.
pub(in crate::world) trait IwsContinuationAdmissionContextDataV2 {
    fn admission_reference(&self) -> &IwsJournalReferenceV2;
    fn admission_values(&self) -> [&str; 4];
}
/// Evidence for the original scheduling event and its exact context digest.
/// Full V2 frame/proposal/registration validation remains separately mandatory.
pub(in crate::world) struct CurrentAdmittedIwsContinuationV2 {
    source: ReservedValue<AuthenticatedCurrentCognitionJournalEventV2>,
    _fee: ReservedValue<()>,
}
impl CurrentAdmittedIwsContinuationV2 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(tx, common, live)?;
        if !self.source.uses_budget(common.memory()) || !self._fee.uses_budget(common.memory()) {
            return Err(bad().into());
        }
        Ok(())
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
    pub(in crate::world) fn prepare_current_registered_cognition_turn_v2(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        proposal: &IwsContinuationProposalV2,
        pending: Option<&PendingCurrentCognitionJournalV1<'_>>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<CurrentRegisteredCognitionTurnV2>, RootModuleExecutionErrorV1<O::Error>>
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let registration = proposal.registration();
            let source = self.read_authenticated_current_cognition_journal_event_v2(
                parent,
                registration,
                pending,
                work,
                common,
                live_owner,
            )?;
            let lifecycle = source.schema.journal_lifecycle().ok_or_else(bad)?;
            // Registration historically reads `kind`, not the independent
            // `event_kind` lifecycle alias. Preserve that exact predicate.
            let kinds = [
                "TurnStarted",
                "ContextCaptured",
                "RequestDispatched",
                "DecisionValidated",
            ];
            common
                .charge(4, kinds.iter().map(|kind| kind.len() as u64).sum())
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if !kinds.iter().any(|kind| {
                lifecycle.registration_kind()
                    == (CognitionOriginalTextProjectionV1::Text {
                        bytes: kind.len() as u64,
                        utf8_digest: *blake3::hash(kind.as_bytes()).as_bytes(),
                    })
            }) {
                return Err(unregistered(common));
            }
            let data = proposal.as_data();
            let query = [
                &data.agent_id,
                &data.agent_session_id,
                &data.agent_turn_id,
                &data.decision_request_id,
                &data.origin_request_digest,
            ];
            let mut commitments = [CognitionOriginalTextProjectionV1::Missing; 5];
            let mut bytes = 0usize;
            for (slot, value) in commitments.iter_mut().zip(query) {
                common
                    .charge(1, value.len() as u64)
                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                bytes = bytes.checked_add(value.len()).ok_or_else(bad)?;
                *slot = CognitionOriginalTextProjectionV1::Text {
                    bytes: value.len() as u64,
                    utf8_digest: *blake3::hash(value.as_bytes()).as_bytes(),
                };
            }
            if lifecycle.keys()[..4] != commitments[..4]
                || (lifecycle.keys()[4] != commitments[4]
                    && lifecycle.origin_request_digest() != commitments[4])
            {
                return Err(unregistered(common));
            }
            common
                .charge(5, bytes as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CurrentRegisteredCognitionTurnV2>()
                        .checked_add(bytes)
                        .ok_or_else(bad)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let identity = query.map(|value| value.as_str().to_owned());
            if identity
                .iter()
                .zip(query)
                .any(|(copy, source)| copy.capacity() > source.len())
            {
                return Err(bad().into());
            }
            let marker = common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let result = fee.retain(CurrentRegisteredCognitionTurnV2 {
                identity,
                registration: IwsJournalReferenceV2 {
                    journal_ordinal: source.point.ordinal(),
                    fact_height: source.point.row().location.height,
                    fact_ordinal: source.point.row().location.ordinal,
                    original: source.point.row().location.original,
                    encoded_bytes: source.point.row().encoded_bytes,
                },
                source,
                _fee: marker,
            });
            result
                .validate(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(result)
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
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
    pub(in crate::world) fn prepare_current_admitted_iws_continuation_v2<
        T: IwsContinuationAdmissionContextDataV2,
    >(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        context: &T,
        pending: Option<&PendingCurrentCognitionJournalV1<'_>>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<CurrentAdmittedIwsContinuationV2>, RootModuleExecutionErrorV1<O::Error>>
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let source = self.read_authenticated_current_cognition_journal_event_v2(
                parent,
                context.admission_reference(),
                pending,
                work,
                common,
                live_owner,
            )?;
            let projection = source.schema.journal_lifecycle().ok_or_else(bad)?;
            let kind = "ContinuationScheduled";
            common
                .charge(1, kind.len() as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if projection.registration_kind()
                != (CognitionOriginalTextProjectionV1::Text {
                    bytes: kind.len() as u64,
                    utf8_digest: *blake3::hash(kind.as_bytes()).as_bytes(),
                })
            {
                return Err(bad().into());
            }
            for (actual, expected) in projection
                .admission_keys_v2()
                .iter()
                .zip(context.admission_values())
            {
                common
                    .charge(1, expected.len() as u64)
                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                if *actual
                    != (CognitionOriginalTextProjectionV1::Text {
                        bytes: expected.len() as u64,
                        utf8_digest: *blake3::hash(expected.as_bytes()).as_bytes(),
                    })
                {
                    return Err(bad().into());
                }
            }
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CurrentAdmittedIwsContinuationV2>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let marker = common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let value = fee.retain(CurrentAdmittedIwsContinuationV2 {
                source,
                _fee: marker,
            });
            value
                .validate(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(value)
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
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
    fn read_authenticated_current_cognition_journal_event_v2(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        registration: &IwsJournalReferenceV2,
        pending: Option<&PendingCurrentCognitionJournalV1<'_>>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        ReservedValue<AuthenticatedCurrentCognitionJournalEventV2>,
        RootModuleExecutionErrorV1<O::Error>,
    > {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            // This producer proves registration only. The service's separate
            // paid V2 admission validates the proposal/conditions; invoking
            // that allocator-bearing validator here would be an unmetered
            // second validation and would confer no registration evidence.
            if self.ctx.transaction.parent_fence() != parent.fence() {
                return Err(bad().into());
            }
            let source = parent.refs().cognition;
            let selected = source.selected();
            let index = source.indexes()[0].as_ref().ok_or_else(bad)?;
            let roots = parent.refs().roots;
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
                    if registration.journal_ordinal < index.record_count() {
                        if !source.uses_budget(read.values.memory)
                            || !selected.uses_budget(read.values.memory)
                            || index.allocation_class() != MemoryClass::Ordinary
                            || selected.manifest().roots
                                != ctx.transaction.parent_fence().parent_roots
                            || selected.occurrence().head_body_id()
                                != ctx.transaction.parent_fence().parent_commit
                            || index.field() != CognitionHistoryFieldV1::JournalEvents
                            || index.head_record_id() != selected.occurrence().head_record_id()
                            || index.roots_id() != selected.manifest().roots
                            || index.root()
                                != &selected
                                    .manifest()
                                    .index_ref(CognitionHistoryFieldV1::JournalEvents)
                                    .root
                        {
                            return Err(bad().into());
                        }
                        read_attested_cognition_history_index_point_v3(
                            index,
                            registration.journal_ordinal,
                            &mut read,
                            &mut work.index,
                        )
                        .map_err(Into::into)
                    } else {
                        let pending = pending.ok_or_else(bad)?;
                        let point = pending.read_index_point(
                            registration.journal_ordinal,
                            ctx.transaction,
                            &mut read,
                            &mut work.index,
                            &live_owner(),
                        )?;
                        let row = point.row();
                        let ordinal = usize::try_from(row.location.ordinal).map_err(|_| bad())?;
                        if row.location.height != roots.height.checked_add(1).ok_or_else(bad)?
                            || !ctx.transaction.ordered_fact(ordinal).is_some_and(|fact| {
                                fact.kind == ExecutionFactKindV1::Event
                                    && fact.id == row.location.original
                            })
                        {
                            return Err(bad().into());
                        }
                        Ok(point)
                    }
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let row = point.row();
            if row.location.height != registration.fact_height
                || row.location.ordinal != registration.fact_ordinal
                || row.location.original != registration.original
                || row.encoded_bytes != registration.encoded_bytes
            {
                return Err(bad().into());
            }
            let raw = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
                if !root
                    .originals
                    .allocation_budget()
                    .same_pool(root.ctx.values.memory)
                    || root.originals.allocation_class() != MemoryClass::Ordinary
                {
                    return Err(bad().into());
                }
                root.work.input.bytes_remaining = root
                    .work
                    .input
                    .bytes_remaining
                    .checked_sub(row.encoded_bytes)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "registration Original bytes",
                    ))?;
                let raw = root.originals.read_original(&row.location.original, usize::try_from(row.encoded_bytes).map_err(|_| bad())?, root.ctx.values.memory)
                    .map_err(|e| RootModuleExecutionErrorV1::Invocation(ModuleInvocationPreparationErrorV1::Original(oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalErrorV1::Source(e))))?;
                if !raw.uses_budget(root.ctx.values.memory)
                    || raw.reserved_bytes() < raw.capacity()
                    || raw.len() as u64 != row.encoded_bytes
                {
                    return Err(bad().into());
                }
                Ok(raw)
            })?;
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
            if !matches!(
                schema.semantic().identity_status(),
                CognitionOriginalIdentityStatusV1::DigestMatched(_)
            ) {
                return Err(bad().into());
            }

            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<AuthenticatedCurrentCognitionJournalEventV2>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let marker = common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(());
            Ok(fee.retain(AuthenticatedCurrentCognitionJournalEventV2 {
                point,
                schema,
                raw,
                guard: self.ctx.transaction.guard(),
                parent: parent.fence(),
                core: common.work().identity(),
                _fee: marker,
            }))
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
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
}
