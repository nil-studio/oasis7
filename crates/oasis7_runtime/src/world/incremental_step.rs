//! Root-backed contributions for one step in the caller's sole transaction.
//! This is not a driver, whole-tick executor, acceptance or cache installer.
use super::{
    World,
    incremental_allocators::{RuntimeAllocatorV1, RuntimeSequenceV1},
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_body_event::{AgentEventContext, AgentEventOriginalV1},
    incremental_module_state::{ModuleStateEventContext, contribute_module_state_event},
};
use crate::WorldEvent;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_schedule::SchedulePageWriter,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_wasm_abi::{CanonicalCborWorkV1, ModuleTickLifecycleDirective};

/// Borrows, rather than begins or seals, the one parent-bound transaction.
/// The private clock is minted only after authenticating the overlay's old
/// clock and staging its successor. No materialized World is retained.
pub struct IwsStepContext<'tx, 'parent, 'context, 'io, R: MerkleNodeReader, V, W, C> {
    transaction: &'tx mut ExecutionTransaction<'parent, R>,
    values: &'context V,
    mutation: &'context mut BodyMutationContext<'io, W, C>,
    original_work: &'context mut CanonicalCborWorkV1,
    now: u64,
}

impl<'tx, 'parent, 'context, 'io, R, V, W, C>
    IwsStepContext<'tx, 'parent, 'context, 'io, R, V, W, C>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
{
    pub fn begin(
        transaction: &'tx mut ExecutionTransaction<'parent, R>,
        expected_time: u64,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        values: &'context V,
        context: &'context mut BodyMutationContext<'io, W, C>,
        original_work: &'context mut CanonicalCborWorkV1,
    ) -> Result<Self, BodyMutationError> {
        let now = transaction.contribute(|transaction| {
            transaction.validate_guard(guard, live)?;
            super::incremental_clock::advance_clock(expected_time, transaction, context)
        })?;
        Ok(Self {
            transaction,
            values,
            mutation: context,
            original_work,
            now,
        })
    }

    pub fn time(&self) -> u64 {
        self.now
    }

    pub fn read_allocator(
        &mut self,
        allocator: RuntimeAllocatorV1,
    ) -> Result<RuntimeSequenceV1, BodyMutationError> {
        World::read_iws_allocator(allocator, self.transaction, self.values, self.mutation)
    }

    /// Dispatch the actual borrowed original through the shared reducer core.
    /// Unsupported events fail closed and poison all preceding contributions.
    pub fn dispatch_agent_event<F>(
        &mut self,
        event: &WorldEvent,
        max_event_bytes: usize,
        persist_original: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    {
        // Preserve the public stream context's owned work API without copying
        // or resetting allowances between events, including failed attempts.
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
                cancelled: &mut self.mutation.cancelled,
            },
            persist_original,
            max_event_bytes,
            original_work: std::mem::replace(self.original_work, placeholder),
        };
        let result =
            World::contribute_iws_agent_event_at(self.now, event, self.transaction, &mut context);
        *self.original_work = context.original_work;
        result
    }

    /// Contribute a ModuleStateUpdated event without creating or advancing an
    /// agent mailbox entry. The original WorldEvent is streamed through the
    /// same canonical producer and sink contract as other event facts.
    pub fn dispatch_module_state_event<F>(
        &mut self,
        qualification: &oasis7_proto::memory_budget::ReservedValue<
            super::incremental_module_state_family::ModuleStateFamilyQualificationV1,
        >,
        event: &WorldEvent,
        max_event_bytes: usize,
        persist_original: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    {
        let placeholder = CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: self.original_work.max_depth,
            max_map_keys: self.original_work.max_map_keys,
            max_key_bytes: self.original_work.max_key_bytes,
        };
        let mut context = ModuleStateEventContext {
            qualification,
            values: self.values,
            mutation: BodyMutationContext {
                sink: &mut *self.mutation.sink,
                memory: self.mutation.memory,
                io: &mut *self.mutation.io,
                cancelled: &mut self.mutation.cancelled,
            },
            persist_original,
            max_event_bytes,
            original_work: std::mem::replace(self.original_work, placeholder),
        };
        let result = contribute_module_state_event(self.now, event, self.transaction, &mut context);
        *self.original_work = context.original_work;
        result
    }

    pub fn allocate(
        &mut self,
        allocator: RuntimeAllocatorV1,
        expected: RuntimeSequenceV1,
    ) -> Result<(u64, RuntimeSequenceV1), BodyMutationError> {
        World::contribute_iws_allocate(
            allocator,
            expected,
            self.transaction,
            self.values,
            self.mutation,
        )
    }

    pub fn module_wake(
        &mut self,
        instance: &str,
        wake: Option<u64>,
    ) -> Result<(), BodyMutationError>
    where
        W: ValueNodeReader + SchedulePageWriter,
    {
        World::contribute_iws_module_wake_at(
            self.now,
            instance,
            wake,
            self.transaction,
            self.values,
            self.mutation,
        )
    }

    pub fn due_module_tick<F>(
        &mut self,
        instance: &str,
        execute: F,
    ) -> Result<bool, BodyMutationError>
    where
        W: ValueNodeReader + SchedulePageWriter,
        F: FnOnce(
            &str,
            u64,
            &mut ExecutionTransaction<'parent, R>,
            &mut BodyMutationContext<'io, W, C>,
        ) -> Result<Option<ModuleTickLifecycleDirective>, BodyMutationError>,
    {
        self.transaction.contribute(|transaction| {
            let executed = World::contribute_iws_due_module_tick_at(
                self.now,
                instance,
                transaction,
                self.values,
                self.mutation,
                execute,
            )?;
            // The executor borrows the common transaction for its own writes;
            // it cannot silently invalidate the step's authenticated clock.
            super::incremental_clock::check_clock(self.now, transaction, self.mutation)?;
            Ok(executed)
        })
    }
}
