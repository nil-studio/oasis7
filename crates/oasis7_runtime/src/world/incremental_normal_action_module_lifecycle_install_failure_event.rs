//! Fixed rejection publication from the real install phase owner. A reason or
//! boolean alone can never enter this constructor.
use super::super::install_driver::CurrentSelectedInstallFailureV1;
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use std::fmt::Write as _;
struct NoteWriter<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    output: Option<String>,
    count: usize,
    limit: usize,
    first: Option<BodyMutationError>,
}
impl std::fmt::Write for NoteWriter<'_, '_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        if self.first.is_some() {
            return Err(std::fmt::Error);
        }
        for bytes in text.as_bytes().chunks(8192) {
            if let Err(error) = pay(self.common, bytes.len()) {
                self.first = Some(error);
                return Err(std::fmt::Error);
            }
        }
        let Some(count) = self.count.checked_add(text.len()) else {
            self.first = Some(invalid());
            return Err(std::fmt::Error);
        };
        if count > self.limit {
            self.first = Some(invalid());
            return Err(std::fmt::Error);
        }
        self.count = count;
        if let Some(output) = &mut self.output {
            output.push_str(text)
        }
        Ok(())
    }
}
fn reason(
    source: &CurrentSelectedInstallFailureV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(crate::RejectReason, MemoryReservation), BodyMutationError> {
    if let Some(reason) = source.prelude_reason_v1() {
        let measured = super::super::governance_json::module_json_value_paid_v1(reason, common)
            .map_err(|error| match error {
                super::super::governance_json::ModuleGovernanceJsonErrorV1::Body(error) => error,
                super::super::governance_json::ModuleGovernanceJsonErrorV1::Json { .. } => {
                    invalid()
                }
            })?;
        // JSON's String/Value/map graph is larger than this fixed RejectReason
        // schema (String/Vec<String>/the two ResourceKind deficit entries).
        let heap = measured
            .fee
            .bytes()
            .checked_mul(2)
            .and_then(|n| n.checked_add(std::mem::size_of::<crate::RejectReason>()))
            .ok_or_else(invalid)?;
        pay(common, heap)?;
        let fee = common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let copied = reason.clone();
        drop(measured);
        return Ok((copied, fee));
    }
    let prefix = source.reason_prefix_v1().ok_or_else(invalid)?;
    let error = source.world_error_v1().ok_or_else(invalid)?;
    let scratch = common
        .memory()
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut count = NoteWriter {
        common,
        output: None,
        count: 0,
        limit: usize::MAX,
        first: None,
    };
    let counted = write!(&mut count, "{prefix}{error:?}");
    if let Some(error) = count.first.take() {
        return Err(error);
    }
    counted.map_err(|_| invalid())?;
    let len = count.count;
    let heap = len
        .checked_add(std::mem::size_of::<String>())
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::RejectReason>()))
        .ok_or_else(invalid)?;
    pay(common, heap)?;
    let fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut out = NoteWriter {
        common,
        output: Some(String::with_capacity(len)),
        count: 0,
        limit: len,
        first: None,
    };
    let written = write!(&mut out, "{prefix}{error:?}");
    if let Some(error) = out.first.take() {
        return Err(error);
    }
    written.map_err(|_| invalid())?;
    if out.count != len {
        return Err(invalid());
    }
    let note = out.output.take().ok_or_else(invalid)?;
    drop(scratch);
    Ok((crate::RejectReason::RuleDenied { notes: vec![note] }, fee))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_action_module_lifecycle) fn publish_current_module_install_failure_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    ME,
    F,
>(
    source: CurrentSelectedInstallFailureV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<PreparedNormalModuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut source = source.into_business_v1()?;
        let now = step.time();
        source.validate(
            step.transaction,
            common.memory(),
            common.work().identity(),
            now,
            live,
        )?;
        selected.validate(step.transaction)?;
        outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if source.action_id() != selected.envelope().id || !outcome.uses_budget(common.memory()) {
            return Err(invalid());
        }
        let (reason, reason_fee) = reason(&source, common)?;
        let id = {
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(&mut ctx,common)?
        };
        let layout_bytes = std::mem::size_of::<PreparedNormalModuleLifecycleEventV1>()
            .checked_add(std::mem::size_of::<CurrentSelectedInstallFailureV1>())
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
            .ok_or_else(invalid)?;
        pay(common, layout_bytes)?;
        let layout = common
            .memory()
            .reserve(layout_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let event_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<crate::WorldEvent>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let accepted = source.accepted.take();
        let mut fees = std::array::from_fn(|_| None);
        fees[0] = Some(reason_fee);
        let cap = PreparedNormalModuleLifecycleEventV1 {
            event: event_fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: Some(crate::CausedBy::Action(source.action_id())),
                body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id: source.action_id(),
                    reason,
                }),
            }),
            accepted,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            action_id: source.action_id(),
            memory: common.memory().clone(),
            state: ModuleLifecycleCommittedStateV1::InstallFailure(Box::new(source)),
            _layout: layout,
            _prepare_fees: fees,
        };
        step.append_prepared_module_lifecycle_event_v1(
            &cap,
            materials,
            common,
            live,
            max_event_bytes,
            persist,
        )?;
        Ok(cap)
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                step.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
