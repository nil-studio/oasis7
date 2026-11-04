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
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawCompleteWorkV1,
    scratch: &S,
    objects: &mut O,
    originals: &mut B,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteErrorV1<O::Error>> {
    check(ctx)?;
    if !objects.allocation_budget().same_pool(ctx.values.memory)
        || objects.allocation_class() != ctx.values.class
    {
        return Err(invalid().into());
    }
    let charge = ctx
        .values
        .memory
        .reserve(CARRIER, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let (agents, mailbox) = mailbox::qualify(roots, ctx, work, objects, originals)?;
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
        local.qualify_family(roots, &mut ctx.cancelled)?
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
        local.qualify_family(roots, &mut ctx.cancelled)?
    };
    check(ctx)?;
    let allocators = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Allocator));
        allocators::qualify_runtime_allocators_v1(roots, &mut local, &mut work.allocators)?
    };
    check(ctx)?;
    let schedule = schedule::qualify_world_raw_schedule_v1(
        roots,
        ctx,
        &mut work.state_scan,
        &mut work.schedule,
    )?;
    check(ctx)?;
    let value = WorldRawCompleteQualificationV1 {
        roots: roots.digest().map_err(BodyMutationError::from)?,
        class: ctx.values.class,
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
