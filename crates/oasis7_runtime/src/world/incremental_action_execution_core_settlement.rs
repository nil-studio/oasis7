//! Actual global rule settlement between Core preflight and publication.
use super::*;
use crate::world::incremental_rule_resources::{
    RuleResourceError, RuleResourceOutcome, settle_current_resources,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_inventory_overlay_read::InventoryOverlayReadContext,
};

#[derive(Debug)]
pub(in crate::world) enum CoreSettlementErrorV1 {
    Body(BodyMutationError),
    Resources(RuleResourceError),
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError> for CoreSettlementErrorV1 {
    fn from(error: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(BodyMutationError::Execution(error))
    }
}

enum CoreSettlementV1 {
    RuleDenied,
    Resources(ReservedValue<RuleResourceOutcome>),
}

/// Only the genuine Rule denial or the real Current resource owner constructs
/// this capability. Insufficiency and overflow are retained business outcomes;
/// resource/I/O/budget failures instead abort the same transaction.
pub(in crate::world) struct SettledCoreActionV1<'rule> {
    prepared: PreparedCoreActionV1<'rule>,
    settlement: CoreSettlementV1,
    identity: PreparedGuard,
}

pub(in crate::world) fn settle_prepared_core_action_v1<'rule, M, R, E, C, W, CW>(
    mut prepared: PreparedCoreActionV1<'rule>,
    selected: &SelectedPendingActionV1,
    read: &mut InventoryOverlayReadContext<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &ExecutionFence,
) -> Result<SettledCoreActionV1<'rule>, CoreSettlementErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        // A genuine Rule denial is a business rejection, before any resource
        // read/debit. The private Rule owner, not a caller flag, selects it.
        prepared
            .validate(selected, read.transaction, read.values.memory)
            .map_err(CoreSettlementErrorV1::Body)?;
        if read.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary {
            return Err(CoreSettlementErrorV1::Body(BodyMutationError::Invalid(
                "Core settlement read class",
            )));
        }
        read.transaction
            .validate_guard(&read.transaction.guard(), live)?;
        if (read.cancelled)() || (write.cancelled)() {
            return Err(CoreSettlementErrorV1::Body(
                ValueTreeError::Cancelled.into(),
            ));
        }
        if prepared.outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Ok(SettledCoreActionV1 {
                prepared,
                settlement: CoreSettlementV1::RuleDenied,
                identity: read.transaction.guard(),
            });
        }
        let cost = prepared
            .cost(selected, read.transaction, read.values.memory)
            .map_err(CoreSettlementErrorV1::Body)?;
        let settlement = settle_current_resources(cost, read, write, live)
            .map_err(CoreSettlementErrorV1::Resources)?;
        // These clones stay within the pre-admitted Core plan charge. The
        // ResourceKind domain has two keys; overflow text comes exclusively
        // from the shared checked balance arithmetic, not caller diagnostics.
        match &*settlement {
            RuleResourceOutcome::Applied => {}
            RuleResourceOutcome::Insufficient(deficits) => {
                prepared.body = crate::DomainEvent::ActionRejected {
                    action_id: prepared.action_id,
                    reason: crate::RejectReason::InsufficientResources {
                        deficits: deficits.iter().copied().collect(),
                    },
                };
            }
            RuleResourceOutcome::Overflow(error) => {
                prepared.body = crate::DomainEvent::ActionRejected {
                    action_id: prepared.action_id,
                    reason: crate::RejectReason::RuleDenied {
                        notes: vec![format!("rule decision cost apply rejected: {error:?}")],
                    },
                };
            }
        }
        if (read.cancelled)() || (write.cancelled)() {
            return Err(CoreSettlementErrorV1::Body(
                ValueTreeError::Cancelled.into(),
            ));
        }
        read.transaction
            .validate_guard(&read.transaction.guard(), live)?;
        Ok(SettledCoreActionV1 {
            prepared,
            settlement: CoreSettlementV1::Resources(settlement),
            identity: read.transaction.guard(),
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            // A point owner may already have aborted. Do not let contribute's
            // Aborted sentinel replace that first concrete storage error.
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                read.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
    }
}

/// Publication requires the actual settlement capability, not a caller Boolean
/// or an unverified cost/plan pair. The business-only shortcut is test-only.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn apply_settled_core_action_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    settled: SettledCoreActionV1<'_>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    persist: F,
) -> Result<CompletedCoreActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    step.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        transaction.validate_transaction_identity(&settled.identity)?;
        settled
            .prepared
            .validate(selected, transaction, step.mutation.memory)?;
        if let CoreSettlementV1::Resources(resources) = &settled.settlement
            && !resources.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid("Core settlement pool binding"));
        }
        Ok(())
    })?;
    let event_before_post_action = matches!(&settled.settlement,
        CoreSettlementV1::Resources(resources) if matches!(&**resources, RuleResourceOutcome::Applied));
    let SettledCoreActionV1 {
        prepared,
        settlement: _settlement,
        ..
    } = settled;
    let mut completed = apply_prepared_core_action_v1(
        step,
        materials,
        selected,
        prepared,
        agents,
        read,
        audit_work,
        max_event_bytes,
        guard,
        live,
        persist,
    )?;
    completed.event_before_post_action = event_before_post_action;
    Ok(completed)
}
