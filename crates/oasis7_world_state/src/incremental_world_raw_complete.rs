//! Required raw current-state structural/original conjunction. No Cognition,
//! retained-History authority, physical lease or WorldReady is granted here.
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_agent_subtree as agents,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_module_instance_family as instances,
    incremental_module_instance_receiver::accept_module_instance_verified_v1,
    incremental_module_instance_recovery::{InstanceScratchBudget, ModuleInstanceRecoveryContext},
    incremental_module_state_family as module_states,
    incremental_module_state_recovery::{ModuleStateContentBudgetV1, ModuleStateRecoveryContextV1},
    incremental_runtime_allocators_family as allocators,
    incremental_verified_world_transition::VerifiedWorldTransitionV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_scratch_counter::CounterScratchStore,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, sync::Arc};
#[path = "incremental_world_raw_complete_mailbox.rs"]
mod mailbox;
#[path = "incremental_world_raw_complete_original.rs"]
mod original;
#[path = "incremental_world_raw_complete_schedule.rs"]
mod schedule;

pub use original::{
    CheckedDomainEventOriginalV1, DomainEventOriginalErrorV1, DomainEventOriginalWorkV1,
    verify_domain_event_original_v1,
};
pub use schedule::{WorldRawScheduleQualificationV1, WorldRawScheduleWorkV1};
/// A checked result is mandatory. An implementation cannot fabricate a proof
/// from an ID or return success without a private, actually streamed token.
pub trait MailboxDomainEventOriginalReaderV1<O: OriginalObjectStreamReader> {
    #[allow(clippy::too_many_arguments)]
    fn checked_domain_event(
        &mut self,
        reference: agents::AgentMailboxReferenceV1<'_>,
        objects: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut DomainEventOriginalWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedDomainEventOriginalV1>>,
        DomainEventOriginalErrorV1<O::Error>,
    >;
}
pub struct WorldRawCompleteWorkV1 {
    pub state_scan: StateScanBudget,
    pub agents: agents::AgentSubtreeScanWorkV1,
    pub agent_changes: agents::AgentSubtreeTransitionWorkV1,
    pub module_states: ModuleStateContentBudgetV1,
    pub instance_scratch: InstanceScratchBudget,
    pub instance_relations_remaining: u64,
    pub max_instance_scalar_bytes: u64,
    pub allocators: allocators::RuntimeAllocatorsWorkV1,
    pub schedule: WorldRawScheduleWorkV1,
    pub mailbox_occurrences_remaining: u64,
    pub originals: DomainEventOriginalWorkV1,
}
pub struct WorldRawCompleteQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    agents: ReservedValue<agents::AgentSubtreeStructureQualificationV1>,
    module_states: ReservedValue<module_states::ModuleStateFamilyQualificationV1>,
    instances: ReservedValue<instances::ModuleInstanceFamilyQualificationV1>,
    allocators: ReservedValue<allocators::RuntimeAllocatorsQualificationV1>,
    schedule: ReservedValue<WorldRawScheduleQualificationV1>,
    mailbox: ReservedValue<mailbox::MailboxOriginalQualificationV1>,
}
const CARRIER: usize = std::mem::size_of::<WorldRawCompleteQualificationV1>() + 512;
impl WorldRawCompleteQualificationV1 {
    pub fn agents(&self) -> &ReservedValue<agents::AgentSubtreeStructureQualificationV1> {
        &self.agents
    }

    pub fn module_states(&self) -> &ReservedValue<module_states::ModuleStateFamilyQualificationV1> {
        &self.module_states
    }

    pub fn instances(&self) -> &ReservedValue<instances::ModuleInstanceFamilyQualificationV1> {
        &self.instances
    }

    pub fn allocators(&self) -> &ReservedValue<allocators::RuntimeAllocatorsQualificationV1> {
        &self.allocators
    }

    pub fn schedule(&self) -> &ReservedValue<schedule::WorldRawScheduleQualificationV1> {
        &self.schedule
    }

    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn mailbox_reference_count(&self) -> u64 {
        self.mailbox.references
    }
    pub fn validate_binding(
        &self,
        roots: &RootSetV1,
    ) -> Result<(), WorldRawCompleteStructuralErrorV1> {
        if self.roots != roots.digest().map_err(BodyMutationError::from)? {
            return Err(invalid().into());
        }
        self.agents.validate_binding(roots)?;
        self.module_states.validate_binding(roots)?;
        self.instances.validate_binding(roots)?;
        self.allocators.validate_binding(roots)?;
        self.schedule.validate_binding(roots)?;
        if self.mailbox.roots != self.roots
            || self.mailbox.class != self.class
            || self.mailbox.references != self.agents.mailbox_reference_count()
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    fn check_pool(&self, memory: &MemoryBudget) -> Result<(), ValueTreeError> {
        macro_rules! token {
            ($slot:ident) => {
                if !self.$slot.uses_budget(memory)
                    || self.$slot.reserved_bytes() < std::mem::size_of_val(&*self.$slot)
                {
                    return Err(invalid());
                }
            };
        }
        token!(agents);
        token!(module_states);
        token!(instances);
        token!(allocators);
        token!(schedule);
        token!(mailbox);
        Ok(())
    }
}
#[derive(Debug)]
pub enum WorldRawCompleteStructuralErrorV1 {
    Value(ValueTreeError),
    Body(BodyMutationError),
    Overlay(WorldFieldOverlayErrorV1),
}
#[derive(Debug)]
pub enum WorldRawCompleteErrorV1<E> {
    Structure(WorldRawCompleteStructuralErrorV1),
    Original(DomainEventOriginalErrorV1<E>),
}
impl std::fmt::Display for WorldRawCompleteStructuralErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Body(e) => e.fmt(f),
            Self::Overlay(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for WorldRawCompleteStructuralErrorV1 {}
impl<E: std::fmt::Display> std::fmt::Display for WorldRawCompleteErrorV1<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Structure(e) => e.fmt(f),
            Self::Original(e) => e.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for WorldRawCompleteErrorV1<E> {}
impl From<ValueTreeError> for WorldRawCompleteStructuralErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<BodyMutationError> for WorldRawCompleteStructuralErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for WorldRawCompleteStructuralErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl<E> From<WorldRawCompleteStructuralErrorV1> for WorldRawCompleteErrorV1<E> {
    fn from(e: WorldRawCompleteStructuralErrorV1) -> Self {
        Self::Structure(e)
    }
}
impl<E> From<ValueTreeError> for WorldRawCompleteErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Structure(e.into())
    }
}
impl<E> From<BodyMutationError> for WorldRawCompleteErrorV1<E> {
    fn from(e: BodyMutationError) -> Self {
        Self::Structure(e.into())
    }
}
impl<E> From<WorldFieldOverlayErrorV1> for WorldRawCompleteErrorV1<E> {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Structure(e.into())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete raw current-state conjunction")
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
fn bind(
    source: &ReservedValue<WorldRawCompleteQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MemoryReservation, WorldRawCompleteStructuralErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !source.uses_budget(memory) || source.class != class || source.reserved_bytes() < CARRIER {
        return Err(invalid().into());
    }
    source.check_pool(memory)?;
    let phase = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    source.validate_binding(roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(phase)
}
macro_rules! local {
    ($ctx:expr,$root:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $ctx.state,
            values: ValueReadContext {
                source: $ctx.values.source,
                memory: $ctx.values.memory,
                class: $ctx.values.class,
                io: &mut *$ctx.values.io,
            },
            key_work: &mut *$ctx.key_work,
            semantic_bytes: $ctx.semantic_bytes,
            record_bytes: $ctx.record_bytes,
            take_state_error: &mut $ctx.take_state_error,
            cancelled: &mut $ctx.cancelled,
        }
    };
}
#[allow(clippy::too_many_arguments)]
pub fn qualify_world_raw_complete_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: MailboxDomainEventOriginalReaderV1<O>,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    scratch: &S,
    objects: &mut O,
    originals: &mut B,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteErrorV1<O::Error>> {
    let result = (|| {
        check(ctx)?;
        roots.validate().map_err(BodyMutationError::from)?;
        if !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        qualify::run(roots, ctx, work, scratch, objects, originals)
    })();
    check(ctx)?;
    result
}
#[allow(clippy::too_many_arguments)]
pub fn accept_world_raw_complete_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: MailboxDomainEventOriginalReaderV1<O>,
>(
    parent: &ReservedValue<WorldRawCompleteQualificationV1>,
    verifier: &oasis7_formal_protocol::incremental_execution::TransitionVerifier<'_, P>,
    record: &oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    scratch: &S,
    objects: &mut O,
    originals: &mut B,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteErrorV1<O::Error>> {
    let result = (|| {
        check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || verifier.class != MemoryClass::Ordinary
            || !verifier.memory.same_pool(ctx.values.memory)
            || !record.after.state.iter().any(|r| r == ctx.root)
        {
            return Err(invalid().into());
        }
        {
            let _phase = bind(
                parent,
                &record.before,
                ctx.values.memory,
                MemoryClass::Ordinary,
                &mut ctx.cancelled,
            )?;
        }
        let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
            verifier, record, ctx,
        )
        .map_err(WorldFieldOverlayErrorV1::from)?;
        accept_world_raw_complete_verified_v1(
            parent, &witness, ctx, work, scratch, objects, originals,
        )
    })();
    check(ctx)?;
    result
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_world_raw_complete_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: MailboxDomainEventOriginalReaderV1<O>,
>(
    parent: &ReservedValue<WorldRawCompleteQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    scratch: &S,
    objects: &mut O,
    originals: &mut B,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteErrorV1<O::Error>> {
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        if ctx.values.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let _phase = bind(
            parent,
            &witness.record().before,
            ctx.values.memory,
            MemoryClass::Ordinary,
            &mut ctx.cancelled,
        )?;
        receiver::run(parent, witness, ctx, work, scratch, objects, originals)
    })();
    check(ctx)?;
    result
}
pub fn readmit_world_raw_complete_ordinary_v1(
    source: &ReservedValue<WorldRawCompleteQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut WorldRawCompleteWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteStructuralErrorV1> {
    let _phase = bind(source, roots, memory, MemoryClass::Recovery, &mut cancelled)?;
    let result = readmission::run(source, roots, memory, work, &mut cancelled);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result
}

#[path = "incremental_world_raw_complete_qualify.rs"]
mod qualify;
#[path = "incremental_world_raw_complete_readmission.rs"]
mod readmission;
#[path = "incremental_world_raw_complete_receiver.rs"]
mod receiver;
