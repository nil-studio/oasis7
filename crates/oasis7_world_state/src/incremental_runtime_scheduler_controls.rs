//! Deterministic Runtime scheduler controls only; not a complete Runtime capability.
use crate::{
    ModuleTickRoutingDeterministicSnapshot, WorldRuntimeBackpressureStats,
    WorldRuntimeMemoryLimits, incremental_field_member::*,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
pub(super) const ROUTING: &str = "module_tick_routing_metrics";
pub(super) fn descriptor(name: &'static str) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: FieldShape::Value,
        destination: FieldDestination::State(StateNamespaceV1::Scheduler),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSchedulerScalarV1 {
    Cursor,
    MaxPendingActions,
    MaxPendingEffects,
    MaxInflightEffects,
    PendingActionsEvicted,
    PendingEffectsEvicted,
    InflightEffectsEvicted,
}
impl RuntimeSchedulerScalarV1 {
    pub const ALL: [Self; 7] = [
        Self::Cursor,
        Self::MaxPendingActions,
        Self::MaxPendingEffects,
        Self::MaxInflightEffects,
        Self::PendingActionsEvicted,
        Self::PendingEffectsEvicted,
        Self::InflightEffectsEvicted,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Cursor => "scheduler_cursor",
            Self::MaxPendingActions => "runtime_memory_limits.max_pending_actions",
            Self::MaxPendingEffects => "runtime_memory_limits.max_pending_effects",
            Self::MaxInflightEffects => "runtime_memory_limits.max_inflight_effects",
            Self::PendingActionsEvicted => "runtime_backpressure_stats.pending_actions_evicted",
            Self::PendingEffectsEvicted => "runtime_backpressure_stats.pending_effects_evicted",
            Self::InflightEffectsEvicted => "runtime_backpressure_stats.inflight_effects_evicted",
        }
    }
    pub(super) fn profile(self) -> (MemberPayloadCodecV1, bool, u64) {
        match self {
            Self::Cursor => (MemberPayloadCodecV1::Text, true, u64::MAX),
            Self::MaxPendingActions => (MemberPayloadCodecV1::U64, false, usize::MAX as u64),
            Self::MaxPendingEffects => (MemberPayloadCodecV1::U64, false, usize::MAX as u64),
            Self::MaxInflightEffects => (MemberPayloadCodecV1::U64, false, usize::MAX as u64),
            Self::PendingActionsEvicted => (MemberPayloadCodecV1::U64, false, u64::MAX),
            Self::PendingEffectsEvicted => (MemberPayloadCodecV1::U64, false, u64::MAX),
            Self::InflightEffectsEvicted => (MemberPayloadCodecV1::U64, false, u64::MAX),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRoutingColumnV1 {
    LastDueCount,
    LastInvokedCount,
    MissingInvocationCount,
    LastMissingInvocationCount,
    OldestOverdueTicks,
    RoutingCount,
}
impl RuntimeRoutingColumnV1 {
    pub const ALL: [Self; 6] = [
        Self::LastDueCount,
        Self::LastInvokedCount,
        Self::MissingInvocationCount,
        Self::LastMissingInvocationCount,
        Self::OldestOverdueTicks,
        Self::RoutingCount,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::LastDueCount => "last_due_count",
            Self::LastInvokedCount => "last_invoked_count",
            Self::MissingInvocationCount => "missing_invocation_count",
            Self::LastMissingInvocationCount => "last_missing_invocation_count",
            Self::OldestOverdueTicks => "oldest_overdue_ticks",
            Self::RoutingCount => "routing_count",
        }
    }
    pub(super) fn profile(self) -> (MemberPayloadCodecV1, bool, u64) {
        match self {
            Self::LastDueCount => (MemberPayloadCodecV1::U64, false, usize::MAX as u64),
            Self::LastInvokedCount => (MemberPayloadCodecV1::U64, false, usize::MAX as u64),
            Self::MissingInvocationCount => (MemberPayloadCodecV1::U64, false, u64::MAX),
            Self::LastMissingInvocationCount => {
                (MemberPayloadCodecV1::U64, false, usize::MAX as u64)
            }
            Self::OldestOverdueTicks => (MemberPayloadCodecV1::U64, true, u64::MAX),
            Self::RoutingCount => (MemberPayloadCodecV1::U64, false, u64::MAX),
        }
    }
}
pub struct RuntimeSchedulerControlsRefV1<'a> {
    pub cursor: Option<&'a str>,
    pub limits: &'a WorldRuntimeMemoryLimits,
    pub backpressure: &'a WorldRuntimeBackpressureStats,
    pub routing: &'a ModuleTickRoutingDeterministicSnapshot,
}
/// Exhaustive borrowed original deterministic controls, excluding declared Process state.
pub fn visit_runtime_scheduler_scalars_v1<'a, E: From<MemberRecordError>>(
    source: &RuntimeSchedulerControlsRefV1<'a>,
    mut emit: impl FnMut(RuntimeSchedulerScalarV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use MemberPayloadRefV1 as P;
    use RuntimeSchedulerScalarV1 as S;
    let WorldRuntimeMemoryLimits {
        max_pending_actions,
        max_pending_effects,
        max_inflight_effects,
        max_journal_events: _,
    } = source.limits;
    let WorldRuntimeBackpressureStats {
        pending_actions_evicted,
        pending_effects_evicted,
        inflight_effects_evicted,
        inflight_effect_dispatch_blocked: _,
        journal_events_evicted: _,
    } = source.backpressure;
    emit(S::Cursor, source.cursor.map_or(P::Unit, P::Text))?;
    for (cell, n) in [
        (S::MaxPendingActions, *max_pending_actions),
        (S::MaxPendingEffects, *max_pending_effects),
        (S::MaxInflightEffects, *max_inflight_effects),
    ] {
        emit(
            cell,
            P::U64(
                u64::try_from(n)
                    .map_err(|_| MemberRecordError::Invalid("runtime scheduler usize"))?,
            ),
        )?;
    }
    for (cell, n) in [
        (S::PendingActionsEvicted, *pending_actions_evicted),
        (S::PendingEffectsEvicted, *pending_effects_evicted),
        (S::InflightEffectsEvicted, *inflight_effects_evicted),
    ] {
        emit(cell, P::U64(n))?;
    }
    Ok(())
}
pub fn visit_runtime_routing_columns_v1<E: From<MemberRecordError>>(
    source: &ModuleTickRoutingDeterministicSnapshot,
    mut emit: impl FnMut(RuntimeRoutingColumnV1, MemberPayloadRefV1<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let ModuleTickRoutingDeterministicSnapshot {
        last_due_count,
        last_invoked_count,
        missing_invocation_count,
        last_missing_invocation_count,
        oldest_overdue_ticks,
        routing_count,
    } = source;
    use MemberPayloadRefV1 as P;
    use RuntimeRoutingColumnV1 as S;
    for (cell, n) in [
        (S::LastDueCount, *last_due_count),
        (S::LastInvokedCount, *last_invoked_count),
        (
            S::LastMissingInvocationCount,
            *last_missing_invocation_count,
        ),
    ] {
        emit(
            cell,
            P::U64(
                u64::try_from(n)
                    .map_err(|_| MemberRecordError::Invalid("runtime routing usize"))?,
            ),
        )?;
    }
    emit(S::MissingInvocationCount, P::U64(*missing_invocation_count))?;
    emit(
        S::OldestOverdueTicks,
        oldest_overdue_ticks.map_or(P::Unit, P::U64),
    )?;
    emit(S::RoutingCount, P::U64(*routing_count))?;
    Ok(())
}
pub(super) fn routing_schema() -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.iws.runtime-routing-controls.v1");
    h.update(&[0]);
    *h.finalize().as_bytes()
}
#[path = "incremental_runtime_scheduler_controls_genesis.rs"]
mod genesis;
pub use genesis::{RuntimeSchedulerControlsWorkV1, prepare_runtime_scheduler_controls_genesis_v1};

#[path = "incremental_runtime_scheduler_controls_read.rs"]
mod read;
pub use read::{
    RuntimeSchedulerControlValueV1, open_qualified_runtime_routing_v1,
    read_qualified_runtime_routing_column_v1, read_qualified_runtime_scheduler_scalar_v1,
};
#[path = "incremental_runtime_scheduler_controls_qualification.rs"]
mod qualification;
pub use qualification::{
    RuntimeRoutingQualificationV1, RuntimeSchedulerControlsQualificationV1,
    qualify_runtime_scheduler_controls_v1,
};

#[path = "incremental_runtime_scheduler_controls_point.rs"]
mod point;
pub use point::{
    RuntimeSchedulerControlValueRefV1, RuntimeSchedulerScalarChangeV1,
    RuntimeSchedulerScalarRequestV1, change_qualified_runtime_scheduler_scalar_v1,
    read_qualified_runtime_scheduler_scalar_overlay_v1,
};

#[path = "incremental_runtime_scheduler_controls_routing.rs"]
mod routing;
pub use routing::{
    RuntimeRoutingChangeV1, RuntimeRoutingRequestV1, change_qualified_runtime_routing_column_v1,
    read_qualified_runtime_routing_overlay_v1,
};
#[path = "incremental_runtime_scheduler_controls_receiver.rs"]
mod receiver;
pub use receiver::accept_runtime_scheduler_controls_transition_v1;

#[path = "incremental_runtime_scheduler_controls_pages.rs"]
mod pages;
pub use pages::*;
