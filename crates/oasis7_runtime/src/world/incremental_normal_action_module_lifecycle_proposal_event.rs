//! Event data is reconstructed only from the exact proposal after the real CAS.
use super::super::governance_state::{CommittedModuleProposalStateV1, ModuleProposalStateKindV1};
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};
use oasis7_world_state::{
    GovernanceEvent, ProposalDecision, ProposalStatus,
    incremental_field_overlay::WorldFieldOverlayContextV1,
};

pub(in crate::world::incremental_normal_action_module_lifecycle) fn validate_proposal_body_v1(
    state: &CommittedModuleProposalStateV1,
    body: &GovernanceEvent,
) -> Result<(), BodyMutationError> {
    let proposal = state.after.value().ok_or_else(invalid)?;
    let valid = match (state.kind, &proposal.status, body) {
        (
            ModuleProposalStateKindV1::Proposed,
            ProposalStatus::Proposed,
            GovernanceEvent::Proposed {
                proposal_id,
                author,
                base_manifest_hash,
                ..
            },
        ) => {
            *proposal_id == state.after.original_id()
                && author == &proposal.author
                && base_manifest_hash == &proposal.base_manifest_hash
        }
        (
            ModuleProposalStateKindV1::Shadow,
            ProposalStatus::Shadowed { manifest_hash },
            GovernanceEvent::ShadowReport {
                proposal_id,
                manifest_hash: actual,
            },
        ) => *proposal_id == state.after.original_id() && actual == manifest_hash,
        (
            ModuleProposalStateKindV1::Approved,
            ProposalStatus::Approved { approver, .. },
            GovernanceEvent::Approved {
                proposal_id,
                approver: actual,
                decision: ProposalDecision::Approve,
            },
        ) => *proposal_id == state.after.original_id() && actual == approver,
        (
            ModuleProposalStateKindV1::Queued,
            ProposalStatus::Approved { manifest_hash, .. },
            GovernanceEvent::Queued {
                proposal_id,
                manifest_hash: actual,
                queued_at_tick,
                not_before_tick,
                activate_epoch,
                timelock_ticks,
            },
        ) => {
            *proposal_id == state.after.original_id()
                && actual == manifest_hash
                && Some(*queued_at_tick) == proposal.queued_at_tick
                && Some(*not_before_tick) == proposal.not_before_tick
                && Some(*activate_epoch) == proposal.activate_epoch
                && *timelock_ticks == proposal.timelock_ticks
        }
        _ => false,
    };
    if valid { Ok(()) } else { Err(invalid()) }
}

/// Does not mutate proposal state or accept a free-form WorldEvent body.
/// The caller publishes this owner only after the corresponding fixed writer.
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_action_module_lifecycle) fn prepare_proposal_event_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
>(
    selected: &SelectedPendingActionV1,
    accepted: Option<SelectedActionAuditV1>,
    state: CommittedModuleProposalStateV1,
    step: &mut crate::world::incremental_step::IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<'_, P, V, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalModuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        selected.validate(step.transaction)?;
        if let Some(a) = &accepted {
            a.validate(step.transaction)?;
        }
        let now = step.time();
        state.validate_evidence_v1(
            step.transaction,
            common.memory(),
            now,
            common.work().identity(),
            live,
        )?;
        // Retain a separate bound for the newly owned event graph. The decoder's
        // reservation remains live with the source; it cannot also pay for this copy.
        let heap = state
            .after
            .output_reserved_bytes_v1()?
            .checked_add(std::mem::size_of::<crate::WorldEvent>())
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(invalid)?;
        common
            .charge(
                u64::try_from(heap).map_err(|_| invalid())?,
                u64::try_from(heap).map_err(|_| invalid())?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let proposal = state.after.value().ok_or_else(invalid)?;
        let body = match (state.kind, &proposal.status) {
            (ModuleProposalStateKindV1::Proposed, ProposalStatus::Proposed) => {
                GovernanceEvent::Proposed {
                    proposal_id: state.after.original_id(),
                    author: proposal.author.clone(),
                    base_manifest_hash: proposal.base_manifest_hash.clone(),
                    manifest: proposal.manifest.clone(),
                    patch: proposal.patch.clone(),
                }
            }
            (ModuleProposalStateKindV1::Shadow, ProposalStatus::Shadowed { manifest_hash }) => {
                GovernanceEvent::ShadowReport {
                    proposal_id: state.after.original_id(),
                    manifest_hash: manifest_hash.clone(),
                }
            }
            (ModuleProposalStateKindV1::Approved, ProposalStatus::Approved { approver, .. }) => {
                GovernanceEvent::Approved {
                    proposal_id: state.after.original_id(),
                    approver: approver.clone(),
                    decision: ProposalDecision::Approve,
                }
            }
            (ModuleProposalStateKindV1::Queued, ProposalStatus::Approved { manifest_hash, .. }) => {
                GovernanceEvent::Queued {
                    proposal_id: state.after.original_id(),
                    manifest_hash: manifest_hash.clone(),
                    queued_at_tick: proposal.queued_at_tick.ok_or_else(invalid)?,
                    not_before_tick: proposal.not_before_tick.ok_or_else(invalid)?,
                    activate_epoch: proposal.activate_epoch.ok_or_else(invalid)?,
                    timelock_ticks: proposal.timelock_ticks,
                }
            }
            _ => return Err(invalid()),
        };
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
        let id =
        crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(
            &mut ctx, common,
        )?;
        let layout = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedNormalModuleLifecycleEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let cap = PreparedNormalModuleLifecycleEventV1 {
            event: fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: None,
                body: crate::WorldEventBody::Governance(body),
            }),
            accepted,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            action_id: selected.envelope().id,
            memory: common.memory().clone(),
            state: ModuleLifecycleCommittedStateV1::Proposal(state),
            _layout: layout,
            _prepare_fees: std::array::from_fn(|_| None),
        };
        cap.validate(
            step.transaction,
            common.memory(),
            now,
            common.work().identity(),
            live,
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
