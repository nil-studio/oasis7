use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn run<
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
    check(ctx)?;
    if !objects.allocation_budget().same_pool(ctx.values.memory)
        || objects.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    let roots = &witness.record().after;
    let charge = ctx
        .values
        .memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let agents = agents::accept_agent_subtree_verified_v1(
        &parent.agents,
        witness,
        ctx,
        &mut work.agent_changes,
    )?;
    check(ctx)?;
    let mailbox = mailbox::accept(witness, &agents, ctx, work, objects, originals)?;
    check(ctx)?;
    let module_states = {
        let mut local = ModuleStateRecoveryContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: ctx.state,
            values: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            state_io: &mut work.state_scan,
            value_io: &mut *ctx.values.io,
            content: &mut work.module_states,
        };
        module_states::accept_module_state_verified_v1(
            &parent.module_states,
            witness,
            &mut local,
            &mut ctx.cancelled,
        )?
    };
    check(ctx)?;
    let instances = {
        let mut local = ModuleInstanceRecoveryContext {
            root: roots.state_root(StateNamespaceV1::Business),
            state: ctx.state,
            values: ctx.values.source,
            scratch,
            memory: ctx.values.memory,
            class: ctx.values.class,
            state_io: &mut work.state_scan,
            value_io: &mut *ctx.values.io,
            scratch_io: &mut work.instance_scratch,
            max_scalar_bytes: work.max_instance_scalar_bytes,
        };
        accept_module_instance_verified_v1(
            &parent.instances,
            witness,
            &mut local,
            &mut work.instance_relations_remaining,
            &mut ctx.cancelled,
        )?
    };
    check(ctx)?;
    let allocators = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Allocator));
        allocators::accept_runtime_allocators_verified_v1(
            &parent.allocators,
            witness,
            &mut local,
            &mut work.allocators,
        )?
    };
    check(ctx)?;
    let schedule = schedule::accept_world_raw_schedule_verified_v1(
        &parent.schedule,
        witness,
        ctx,
        &mut work.schedule,
    )?;
    check(ctx)?;
    let value = WorldRawCompleteQualificationV1 {
        roots: *witness.after_digest(),
        class: MemoryClass::Ordinary,
        agents,
        module_states,
        instances,
        allocators,
        schedule,
        mailbox,
    };
    value.validate_binding(roots)?;
    value.check_pool(ctx.values.memory)?;
    check(ctx)?;
    Ok(charge.retain(value))
}
