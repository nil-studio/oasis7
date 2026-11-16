//! Counted system-event publication after the successful same-Core reducer.
//! All original IO and normalization lanes debit the caller Common context.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_system_event::PreparedNormalSystemEventV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
impl<M, V, W, C> IwsStepContext<'_, '_, '_, '_, M, V, W, C>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
{
    /// Publish the actual marketplace reducer's event without applying its
    /// domain mutation twice. The concrete source retains all paid state.
    pub(in crate::world) fn append_prepared_module_lifecycle_event_v1<F, ME>(
        &mut self,
        source: &crate::world::incremental_normal_action_module_lifecycle::PreparedNormalModuleLifecycleEventV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        max_event_bytes: usize,
        persist: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cap = PreparedNormalSystemEventV1::from_module_lifecycle(
                source,
                self.transaction,
                self.now,
                common,
                live,
            )?;
            self.append_prepared_normal_system_event_v1(
                &cap,
                materials,
                common,
                live,
                max_event_bytes,
                persist,
            )
        }));
        let result = match attempt {
            Ok(result) => result,
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        };
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    /// Publish the committed Bootstrap authorization event without applying its
    /// domain mutation twice. The concrete source retains all paid state.
    pub(in crate::world) fn append_committed_bootstrap_authorization_event_v1<F, ME>(
        &mut self,
        source: &crate::world::normal_bootstrap_current::CommittedCurrentBootstrapAuthorizationV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        max_event_bytes: usize,
        persist: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cap = PreparedNormalSystemEventV1::from_bootstrap_authorization(
                source,
                self.transaction,
                self.now,
                common,
                live,
            )?;
            self.append_prepared_normal_system_event_v1(
                &cap,
                materials,
                common,
                live,
                max_event_bytes,
                persist,
            )
        }));
        let result = match attempt {
            Ok(result) => result,
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        };
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    pub(in crate::world) fn append_prepared_normal_system_event_v1<F, ME>(
        &mut self,
        cap: &PreparedNormalSystemEventV1<'_>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        max_event_bytes: usize,
        persist: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            common
                .consume_steps(1)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if !self.mutation.memory.same_pool(common.memory())
                || !materials.parent.uses_budget(common.memory())
            {
                return Err(BodyMutationError::Invalid(
                    "normal system event Common pool",
                ));
            }
            cap.validate_common(common)?;
            cap.validate(self.transaction, self.mutation.memory, self.now)?;
            self.transaction
                .validate_guard(&self.transaction.guard(), live)?;
            let before_event = self.begin_world_event()?;
            let metadata = materials
                .roots
                .world_id
                .len()
                .checked_add(6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
                .and_then(|n| n.checked_add(1024))
                .and_then(|n| n.checked_mul(16))
                .ok_or(BodyMutationError::Invalid(
                    "normal system event metadata layout",
                ))?;
            let metadata = u64::try_from(metadata)
                .map_err(|_| BodyMutationError::Invalid("normal system event metadata width"))?;
            common
                .charge(metadata, metadata)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let (_, meter, stop) = common.parts();
            let child = meter
                .begin_child()
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let visits = child.remaining_steps() / 8;
            let bytes = child.remaining_read_bytes() / 9;
            macro_rules! apply {
                (clamp_bytes; $($args:tt)*) => { clamp_bytes!($($args)*) };
                (clamp_steps; $($args:tt)*) => { clamp_steps!($($args)*) };
                (restore_bytes; $($args:tt)*) => { restore_bytes!($($args)*) };
                (restore_steps; $($args:tt)*) => { restore_steps!($($args)*) };
                (snapshot; $($args:tt)*) => { snapshot!($($args)*) };
            }
            macro_rules! steps {
                ($m:ident) => {
                    apply!($m;
                        self.mutation.io.nodes_remaining,
                        self.original_work.visits_remaining,
                        materials.read_io.nodes_remaining,
                        materials.family_work.records_remaining,
                        materials.family_work.index_steps_remaining,
                        materials.group_work.steps_remaining,
                        *materials.normalization_cells_remaining
                    )
                };
            }
            macro_rules! bytes {
                ($m:ident) => {
                    apply!($m;
                        self.mutation.io.read_bytes_remaining,
                        self.mutation.io.write_bytes_remaining,
                        self.original_work.bytes_remaining,
                        materials.read_io.read_bytes_remaining,
                        materials.read_io.write_bytes_remaining,
                        materials.read_keys.bytes_remaining,
                        materials.write_keys.bytes_remaining,
                        materials.scope_work.comparison_bytes_remaining,
                        *materials.normalization_comparison_bytes_remaining
                    )
                };
            }
            macro_rules! clamp_steps {($($lane:expr),*)=>{$(($lane)=($lane).min(visits);)*};}
            macro_rules! clamp_bytes {($($lane:expr),*)=>{$(($lane)=($lane).min(bytes);)*};}
            macro_rules! snapshot {($($lane:expr),*)=>{[$($lane),*]};}
            let original_steps = steps!(snapshot);
            let original_bytes = bytes!(snapshot);
            let original_entries = materials.scope_work.pending_entries_remaining;
            steps!(clamp_steps);
            bytes!(clamp_bytes);
            materials.scope_work.pending_entries_remaining = materials
                .scope_work
                .pending_entries_remaining
                .min(usize::try_from(visits).unwrap_or(usize::MAX));
            let before_steps = steps!(snapshot);
            let before_bytes = bytes!(snapshot);
            let before_entries = materials.scope_work.pending_entries_remaining;
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let placeholder = CanonicalCborWorkV1 {
                    visits_remaining: 0,
                    bytes_remaining: 0,
                    max_depth: self.original_work.max_depth,
                    max_map_keys: self.original_work.max_map_keys,
                    max_key_bytes: self.original_work.max_key_bytes,
                };
                let mut context = AgentEventContext {
                    values: self.values,
                    mutation: BodyMutationContext {
                        sink: &mut *self.mutation.sink,
                        memory: self.mutation.memory,
                        io: &mut *self.mutation.io,
                        cancelled: || (self.mutation.cancelled)() || stop(),
                    },
                    original_work: std::mem::replace(self.original_work, placeholder),
                    persist_original: persist,
                    max_event_bytes,
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    World::contribute_iws_normal_system_event_at(
                        self.now,
                        cap,
                        self.transaction,
                        &mut context,
                        materials,
                        live,
                    )
                }));
                *self.original_work = context.original_work;
                result
            }));
            let after_steps = steps!(snapshot);
            let after_bytes = bytes!(snapshot);
            let delta = |before: &[u64], after: &[u64]| {
                before
                    .iter()
                    .zip(after)
                    .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
            };
            // Restore the parked allowance, retaining every actual debit even
            // when the reducer fails or unwinds. Clamping is a short loan.
            macro_rules! restore_steps {($($lane:expr),*)=>{{
                let mut i = 0; $( $lane = original_steps[i].saturating_sub(before_steps[i].saturating_sub(after_steps[i])); i += 1; )*
                let _ = i;
            }};}
            macro_rules! restore_bytes {($($lane:expr),*)=>{{
                let mut i = 0; $( $lane = original_bytes[i].saturating_sub(before_bytes[i].saturating_sub(after_bytes[i])); i += 1; )*
                let _ = i;
            }};}
            steps!(restore_steps);
            bytes!(restore_bytes);
            let after_entries = materials.scope_work.pending_entries_remaining;
            materials.scope_work.pending_entries_remaining =
                original_entries.saturating_sub(before_entries.saturating_sub(after_entries));
            let entries = before_entries
                .checked_sub(after_entries)
                .and_then(|n| u64::try_from(n).ok());
            let spent_steps =
                delta(&before_steps, &after_steps).and_then(|n| n.checked_add(entries?));
            let Some(steps) = spent_steps else {
                return Err(BodyMutationError::Invalid(
                    "normal system event step accounting",
                ));
            };
            let Some(bytes) = delta(&before_bytes, &after_bytes) else {
                return Err(BodyMutationError::Invalid(
                    "normal system event byte accounting",
                ));
            };
            let debit = child
                .consume(steps, bytes)
                .map_err(|e| BodyMutationError::Original(Box::new(e)));
            child.finish();
            let result = match attempt {
                Ok(Ok(result)) => result,
                Ok(Err(payload)) | Err(payload) => {
                    // Debit the same child before propagating the panic.
                    let _ = debit;
                    std::panic::resume_unwind(payload)
                }
            };
            result?;
            debit?;
            common
                .consume_steps(0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            self.finish_world_event(before_event)
        }));
        let result = match attempt {
            Ok(result) => result,
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        };
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
