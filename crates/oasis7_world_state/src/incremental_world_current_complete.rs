//! Complete current-state conjunction. Retained History, actual accepted-head
//! authority, execution legality and physical availability remain required by
//! the enclosing new-world driver; this is never a WorldReady constructor.
use crate::{
    incremental_cognition_current_complete_v3 as cognition,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_typed_complete as runtime,
    incremental_verified_world_transition::{
        VerifiedWorldTransitionV1, verify_world_transition_v1,
    },
    incremental_world_key_router as keys, incremental_world_native_complete as native,
    incremental_world_raw_complete as raw,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_material::OriginalObjectStreamReader,
    incremental_scratch_counter::CounterScratchStore,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
pub struct WorldCurrentStateWorkV1 {
    pub key_scan: StateScanBudget,
    pub native_outer_scan: StateScanBudget,
    pub native_inner_scan: StateScanBudget,
    pub key_changes_remaining: u64,
    pub native: native::WorldNativeCompleteWorkV1,
    pub runtime: runtime::RuntimeTypedCompleteWorkV1,
    pub raw: raw::WorldRawCompleteWorkV1,
    pub cognition: cognition::CognitionCurrentCompleteWorkV3,
}
pub struct WorldCurrentStateQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    keys: ReservedValue<keys::WorldKeyRoutingQualificationV1>,
    native: ReservedValue<native::WorldNativeCompleteQualificationV1>,
    runtime: ReservedValue<runtime::RuntimeTypedCompleteQualificationV1>,
    raw: ReservedValue<raw::WorldRawCompleteQualificationV1>,
    cognition: ReservedValue<cognition::CompleteCognitionCurrentQualificationV3>,
}
const CARRIER: usize = std::mem::size_of::<WorldCurrentStateQualificationV1>() + 512;
impl WorldCurrentStateQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn native(&self) -> &ReservedValue<native::WorldNativeCompleteQualificationV1> {
        &self.native
    }
    pub fn runtime(&self) -> &ReservedValue<runtime::RuntimeTypedCompleteQualificationV1> {
        &self.runtime
    }
    pub fn raw(&self) -> &ReservedValue<raw::WorldRawCompleteQualificationV1> {
        &self.raw
    }
    pub fn cognition(&self) -> &ReservedValue<cognition::CompleteCognitionCurrentQualificationV3> {
        &self.cognition
    }
    pub fn validate_binding(
        &self,
        roots: &RootSetV1,
    ) -> Result<(), WorldCurrentStateStructuralErrorV1> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid().into());
        }
        self.keys.validate_binding(roots)?;
        self.native
            .validate_binding(roots)
            .map_err(WorldCurrentStateStructuralErrorV1::Native)?;
        self.runtime.validate_binding(roots)?;
        self.raw
            .validate_binding(roots)
            .map_err(WorldCurrentStateStructuralErrorV1::Raw)?;
        self.cognition.validate_binding(roots)?;
        if self.keys.class() != self.class
            || self.native.allocation_class() != self.class
            || self.runtime.allocation_class() != self.class
            || self.raw.allocation_class() != self.class
            || self.cognition.allocation_class() != self.class
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub(crate) fn check_pool(&self, memory: &MemoryBudget) -> Result<(), ValueTreeError> {
        macro_rules! slot {
            ($name:ident) => {
                if !self.$name.uses_budget(memory)
                    || self.$name.reserved_bytes() < std::mem::size_of_val(&*self.$name)
                {
                    return Err(invalid());
                }
            };
        }
        slot!(keys);
        slot!(native);
        slot!(runtime);
        slot!(raw);
        slot!(cognition);
        Ok(())
    }
}
#[derive(Debug)]
pub enum WorldCurrentStateStructuralErrorV1 {
    Value(ValueTreeError),
    Native(native::WorldNativeCompleteErrorV1),
    Raw(raw::WorldRawCompleteStructuralErrorV1),
}
impl From<ValueTreeError> for WorldCurrentStateStructuralErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
#[derive(Debug)]
pub enum WorldCurrentStateErrorV1<O> {
    Binding(WorldCurrentStateStructuralErrorV1),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Native(native::WorldNativeCompleteErrorV1),
    Runtime(runtime::RuntimeTypedCompleteErrorV1<O>),
    Raw(raw::WorldRawCompleteErrorV1<O>),
    RawStructure(raw::WorldRawCompleteStructuralErrorV1),
}
impl<O> From<ValueTreeError> for WorldCurrentStateErrorV1<O> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<O> From<WorldFieldOverlayErrorV1> for WorldCurrentStateErrorV1<O> {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete current World binding/pool/class")
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
macro_rules! local {
    ($roots:expr,$ctx:expr) => {
        WorldFieldReadContextV1 {
            root: $roots.state_root(Ns::Authorization),
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
/// Every family is required, including empty collections and disabled Cognition.
/// The selector chooses which actual Cognition presence proof must succeed.
#[allow(clippy::too_many_arguments)]
pub fn qualify_world_current_state_v1<M, R, E, C, S, O, B>(
    roots: &RootSetV1,
    presence: cognition::CognitionCurrentPresenceV3,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCurrentStateWorkV1,
    scratch: &S,
    objects: &mut O,
    mailbox: &mut B,
) -> Result<ReservedValue<WorldCurrentStateQualificationV1>, WorldCurrentStateErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: raw::MailboxDomainEventOriginalReaderV1<O>,
{
    let result = (|| {
        check(ctx)?;
        roots.validate().map_err(|_| invalid())?;
        if !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let retained = ctx
            .values
            .memory
            .reserve(CARRIER, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let keys = keys::qualify_world_key_routing_v1(
            roots,
            ctx.state,
            ctx.values.memory,
            ctx.values.class,
            &mut work.key_scan,
            &mut ctx.take_state_error,
            &mut ctx.cancelled,
        )?;
        let native = native::qualify_world_native_complete_v1(
            roots,
            &mut local!(roots, ctx),
            &mut work.native_outer_scan,
            &mut work.native_inner_scan,
            &mut work.native,
        )
        .map_err(WorldCurrentStateErrorV1::Native)?;
        let runtime = runtime::qualify_runtime_typed_complete_v1(
            roots,
            &mut local!(roots, ctx),
            objects,
            &mut work.runtime,
        )
        .map_err(WorldCurrentStateErrorV1::Runtime)?;
        let raw = raw::qualify_world_raw_complete_v1(
            roots,
            &mut local!(roots, ctx),
            &mut work.raw,
            scratch,
            objects,
            mailbox,
        )
        .map_err(WorldCurrentStateErrorV1::Raw)?;
        let cognition = cognition::qualify_complete_cognition_current_v3(
            roots,
            presence,
            &mut local!(roots, ctx),
            &mut work.cognition,
        )?;
        let value = WorldCurrentStateQualificationV1 {
            roots: roots.digest().map_err(|_| invalid())?,
            class: ctx.values.class,
            keys,
            native,
            runtime,
            raw,
            cognition,
        };
        value
            .validate_binding(roots)
            .map_err(WorldCurrentStateErrorV1::Binding)?;
        check(ctx)?;
        Ok(retained.retain(value))
    })();
    check(ctx)?;
    result
}
fn bind<M, R, E, C, O>(
    source: &ReservedValue<WorldCurrentStateQualificationV1>,
    roots: &RootSetV1,
    class: MemoryClass,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<MemoryReservation, WorldCurrentStateErrorV1<O>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    check(ctx)?;
    if source.class != class
        || !source.uses_budget(ctx.values.memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid().into());
    }
    source.check_pool(ctx.values.memory)?;
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    source
        .validate_binding(roots)
        .map_err(WorldCurrentStateErrorV1::Binding)?;
    check(ctx)?;
    Ok(phase)
}
/// All typed owners consume one immutable generic replay, never one per family.
#[allow(clippy::too_many_arguments)]
pub fn accept_world_current_state_transition_v1<M, P, R, E, C, S, O, B>(
    parent: &ReservedValue<WorldCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    after_presence: cognition::CognitionCurrentPresenceV3,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCurrentStateWorkV1,
    scratch: &S,
    objects: &mut O,
    mailbox: &mut B,
) -> Result<ReservedValue<WorldCurrentStateQualificationV1>, WorldCurrentStateErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: raw::MailboxDomainEventOriginalReaderV1<O>,
{
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
            let _phase =
                bind::<_, _, _, _, O::Error>(parent, &record.before, MemoryClass::Ordinary, ctx)?;
        }
        let witness = verify_world_transition_v1(verifier, record, ctx)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        accept_world_current_state_verified_v1(
            parent,
            &witness,
            after_presence,
            ctx,
            work,
            scratch,
            objects,
            mailbox,
        )
    })();
    check(ctx)?;
    result
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_world_current_state_verified_v1<M, R, E, C, S, O, B>(
    parent: &ReservedValue<WorldCurrentStateQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    after_presence: cognition::CognitionCurrentPresenceV3,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCurrentStateWorkV1,
    scratch: &S,
    objects: &mut O,
    mailbox: &mut B,
) -> Result<ReservedValue<WorldCurrentStateQualificationV1>, WorldCurrentStateErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    S: CounterScratchStore,
    O: OriginalObjectStreamReader,
    B: raw::MailboxDomainEventOriginalReaderV1<O>,
{
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        if ctx.values.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let _phase = bind::<_, _, _, _, O::Error>(
            parent,
            &witness.record().before,
            MemoryClass::Ordinary,
            ctx,
        )?;
        let retained = ctx
            .values
            .memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let roots = &witness.record().after;
        let keys = keys::accept_world_key_routing_verified_v1(
            &parent.keys,
            witness,
            ctx.values.memory,
            &mut work.key_changes_remaining,
            &mut ctx.cancelled,
        )?;
        let native = native::accept_world_native_complete_verified_v1(
            &parent.native,
            witness,
            &mut local!(roots, ctx),
            &mut work.native_inner_scan,
            &mut work.native,
        )
        .map_err(WorldCurrentStateErrorV1::Native)?;
        let runtime = runtime::accept_runtime_typed_complete_verified_v1(
            &parent.runtime,
            witness,
            &mut local!(roots, ctx),
            objects,
            &mut work.runtime,
        )
        .map_err(WorldCurrentStateErrorV1::Runtime)?;
        let raw = raw::accept_world_raw_complete_verified_v1(
            &parent.raw,
            witness,
            &mut local!(roots, ctx),
            &mut work.raw,
            scratch,
            objects,
            mailbox,
        )
        .map_err(WorldCurrentStateErrorV1::Raw)?;
        let cognition = cognition::accept_complete_cognition_current_verified_v3(
            &parent.cognition,
            witness,
            &mut local!(roots, ctx),
            after_presence,
            &mut work.cognition,
        )?;
        let value = WorldCurrentStateQualificationV1 {
            roots: *witness.after_digest(),
            class: MemoryClass::Ordinary,
            keys,
            native,
            runtime,
            raw,
            cognition,
        };
        value
            .validate_binding(roots)
            .map_err(WorldCurrentStateErrorV1::Binding)?;
        check(ctx)?;
        Ok(retained.retain(value))
    })();
    check(ctx)?;
    result
}
/// Re-admit every owned proof/index into Ordinary; no Recovery child survives.
/// The Ordinary reader context supplies allocation identity, not a replay.
pub fn readmit_world_current_state_ordinary_v1<M, R, E, C>(
    source: &ReservedValue<WorldCurrentStateQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldCurrentStateWorkV1,
) -> Result<
    ReservedValue<WorldCurrentStateQualificationV1>,
    WorldCurrentStateErrorV1<std::convert::Infallible>,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary || !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let _phase = bind::<_, _, _, _, std::convert::Infallible>(
            source,
            roots,
            MemoryClass::Recovery,
            ctx,
        )?;
        let memory = ctx.values.memory;
        let retained = memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let keys = keys::readmit_world_key_routing_ordinary_v1(
            &source.keys,
            roots,
            memory,
            &mut ctx.cancelled,
        )?;
        let native = native::readmit_world_native_complete_ordinary_v1(
            &source.native,
            roots,
            &mut local!(roots, ctx),
            &mut work.native,
        )
        .map_err(WorldCurrentStateErrorV1::Native)?;
        let runtime = runtime::readmit_runtime_typed_complete_ordinary_v1(
            &source.runtime,
            roots,
            memory,
            &mut work.runtime.pending.index,
            &mut ctx.cancelled,
        )?;
        let raw = raw::readmit_world_raw_complete_ordinary_v1(
            &source.raw,
            roots,
            memory,
            &mut work.raw,
            &mut ctx.cancelled,
        )
        .map_err(WorldCurrentStateErrorV1::RawStructure)?;
        let cognition = cognition::readmit_complete_cognition_current_ordinary_v3(
            &source.cognition,
            roots,
            memory,
            &mut ctx.cancelled,
        )?;
        let value = WorldCurrentStateQualificationV1 {
            roots: source.roots,
            class: MemoryClass::Ordinary,
            keys,
            native,
            runtime,
            raw,
            cognition,
        };
        value
            .validate_binding(roots)
            .map_err(WorldCurrentStateErrorV1::Binding)?;
        check(ctx)?;
        Ok(retained.retain(value))
    })();
    check(ctx)?;
    result
}
