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
    pub(super) transaction: &'tx mut ExecutionTransaction<'parent, R>,
    pub(super) values: &'context V,
    pub(super) mutation: &'context mut BodyMutationContext<'io, W, C>,
    pub(super) original_work: &'context mut CanonicalCborWorkV1,
    now: u64,
    world_events: Option<super::incremental_world_event_counter::WorldEventCounterV1>,
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
            world_events: None,
        })
    }

    /// Bind the same genuine parent ledger counter used by every WorldEvent
    /// sink in this step. Input and Cognition facts never advance this count.
    #[expect(
        clippy::too_many_arguments,
        reason = "The actual transaction, clock and finite stream contexts remain explicit."
    )]
    pub fn begin_counted(
        transaction: &'tx mut ExecutionTransaction<'parent, R>,
        expected_time: u64,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        values: &'context V,
        context: &'context mut BodyMutationContext<'io, W, C>,
        original_work: &'context mut CanonicalCborWorkV1,
        counter: &super::incremental_world_event_counter::WorldEventCounterV1,
    ) -> Result<Self, BodyMutationError> {
        transaction.contribute(|tx| counter.validate(tx))?;
        let mut step = Self::begin(
            transaction,
            expected_time,
            guard,
            live,
            values,
            context,
            original_work,
        )?;
        step.world_events = Some(counter.clone());
        Ok(step)
    }

    pub fn journal_height(&self) -> Result<u64, BodyMutationError> {
        self.world_events
            .as_ref()
            .ok_or(BodyMutationError::Invalid(
                "step requires WorldEvent counter",
            ))?
            .journal_height(self.transaction)
    }

    pub(super) fn begin_world_event(&mut self) -> Result<usize, BodyMutationError> {
        if let Some(counter) = &self.world_events {
            self.transaction.contribute(|tx| counter.validate(tx))?;
        }
        Ok(self.transaction.ordered_fact_count())
    }

    pub(super) fn finish_world_event(&mut self, before: usize) -> Result<(), BodyMutationError> {
        if let Some(counter) = &self.world_events {
            self.transaction
                .contribute(|tx| counter.note_success(tx, before))?;
        }
        Ok(())
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
        let before = self.begin_world_event()?;
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
        result?;
        self.finish_world_event(before)
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
        let before = self.begin_world_event()?;
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
        result?;
        self.finish_world_event(before)
    }

    /// Register a qualified absent cell using this step's authenticated clock.
    pub fn dispatch_agent_registration<F, E>(
        &mut self,
        qualification: &oasis7_proto::memory_budget::ReservedValue<
            oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
        >,
        event: &WorldEvent,
        key_work: &mut oasis7_world_state::incremental_semantic_key::SemanticKeyBudget,
        read_io: &mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
        semantic_bytes: u64,
        record_bytes: u64,
        take_state_error: E,
        live: &ExecutionFence,
        max_event_bytes: usize,
        persist_original: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        E: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    {
        let before = self.begin_world_event()?;
        let guard = self.transaction.guard();
        let result = World::contribute_iws_agent_registration_at(
            self.now,
            event,
            qualification,
            &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
                transaction: self.transaction,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: self.values,
                    memory: self.mutation.memory,
                    class: oasis7_proto::memory_budget::MemoryClass::Ordinary,
                    io: read_io,
                },
                key_work,
                semantic_bytes,
                record_bytes,
                take_state_error,
                cancelled: || false,
            },
            self.mutation,
            &guard,
            live,
            self.original_work,
            max_event_bytes,
            persist_original,
        );
        result?;
        self.transaction.contribute(|transaction| {
            super::incremental_clock::check_clock(self.now, transaction, self.mutation)
        })?;
        self.finish_world_event(before)
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
