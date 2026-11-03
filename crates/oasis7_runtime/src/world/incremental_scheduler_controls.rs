//! Original deterministic scheduler controls; no scheduling or provider authorization.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionWriteV1, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_scheduler_controls::{
        RuntimeRoutingChangeV1, RuntimeRoutingColumnV1, RuntimeRoutingQualificationV1,
        RuntimeSchedulerControlValueRefV1, RuntimeSchedulerControlsRefV1,
        RuntimeSchedulerControlsWorkV1, RuntimeSchedulerScalarChangeV1, RuntimeSchedulerScalarV1,
        change_qualified_runtime_routing_column_v1, change_qualified_runtime_scheduler_scalar_v1,
        prepare_runtime_scheduler_controls_genesis_v1,
    },
};
impl World {
    /// Borrow the actual Runtime values. Wall-clock metrics and process-only
    /// counters never enter these rows. All output remains provisional.
    pub fn write_iws_scheduler_controls_genesis<
        W: ValueNodeReader + StatePageWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        context: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeSchedulerControlsWorkV1,
        sink: &mut S,
    ) -> Result<(), MemberRecordError> {
        let routing = self.module_tick_routing_metrics.deterministic_snapshot();
        prepare_runtime_scheduler_controls_genesis_v1(
            &RuntimeSchedulerControlsRefV1 {
                cursor: self.scheduler_cursor.as_deref(),
                limits: &self.runtime_memory_limits,
                backpressure: &self.runtime_backpressure_stats,
                routing: &routing,
            },
            context,
            work,
            sink,
        )
    }
}

impl World {
    /// Contribute the selected actual Runtime scalar without scanning other controls.
    pub fn contribute_iws_scheduler_control<'a, M, R, E, C, W, CW>(
        &'a self,
        mut change: RuntimeSchedulerScalarChangeV1<'a>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<bool, WorldFieldOverlayErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: ValueNodeWriter,
        CW: FnMut() -> bool,
    {
        use RuntimeSchedulerControlValueRefV1 as V;
        use RuntimeSchedulerScalarV1 as S;
        change.after = match change.field {
            S::Cursor => self.scheduler_cursor.as_deref().map_or(V::Unit, V::Text),
            S::MaxPendingActions => V::U64(self.runtime_memory_limits.max_pending_actions as u64),
            S::MaxPendingEffects => V::U64(self.runtime_memory_limits.max_pending_effects as u64),
            S::MaxInflightEffects => V::U64(self.runtime_memory_limits.max_inflight_effects as u64),
            S::PendingActionsEvicted => {
                V::U64(self.runtime_backpressure_stats.pending_actions_evicted)
            }
            S::PendingEffectsEvicted => {
                V::U64(self.runtime_backpressure_stats.pending_effects_evicted)
            }
            S::InflightEffectsEvicted => {
                V::U64(self.runtime_backpressure_stats.inflight_effects_evicted)
            }
        };
        change_qualified_runtime_scheduler_scalar_v1(change, context, write, guard, live)
    }
    /// The deterministic snapshot is six stack primitives; wall-clock metrics stay Process.
    pub fn contribute_iws_routing_control<M, R, E, C, W, CW>(
        &self,
        mut change: RuntimeRoutingChangeV1<'_>,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<Option<ReservedValue<RuntimeRoutingQualificationV1>>, WorldFieldOverlayErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    {
        use RuntimeRoutingColumnV1 as S;
        use RuntimeSchedulerControlValueRefV1 as V;
        let source = self.module_tick_routing_metrics.deterministic_snapshot();
        change.after = match change.column {
            S::LastDueCount => V::U64(source.last_due_count as u64),
            S::LastInvokedCount => V::U64(source.last_invoked_count as u64),
            S::LastMissingInvocationCount => V::U64(source.last_missing_invocation_count as u64),
            S::MissingInvocationCount => V::U64(source.missing_invocation_count),
            S::OldestOverdueTicks => source.oldest_overdue_ticks.map_or(V::Unit, V::U64),
            S::RoutingCount => V::U64(source.routing_count),
        };
        change_qualified_runtime_routing_column_v1(change, context, write, guard, live)
    }
}
