use super::*;
pub(super) fn run(
    source: &ReservedValue<WorldRawCompleteQualificationV1>,
    roots: &RootSetV1,
    scope: &MailboxProvenanceScopeV1<'_>,
    memory: &MemoryBudget,
    _work: &mut WorldRawCompleteWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRawCompleteQualificationV1>, WorldRawCompleteStructuralErrorV1> {
    let charge = memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let agents = agents::readmit_agent_subtree_structure_ordinary_v1(
        &source.agents,
        roots,
        memory,
        &mut cancelled,
    )?;
    let module_states = module_states::readmit_module_state_family_ordinary_v1(
        &source.module_states,
        roots,
        memory,
        &mut cancelled,
    )?;
    let instances = instances::readmit_module_instance_family_ordinary_v1(
        &source.instances,
        roots,
        memory,
        &mut cancelled,
    )?;
    let allocators = allocators::readmit_runtime_allocators_ordinary_v1(
        &source.allocators,
        roots,
        memory,
        &mut cancelled,
    )?;
    let schedule = schedule::readmit_world_raw_schedule_ordinary_v1(
        &source.schedule,
        roots,
        memory,
        &mut cancelled,
    )?;
    let mailbox = mailbox::readmit(&source.mailbox, roots, scope, memory, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let value = WorldRawCompleteQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        agents,
        module_states,
        instances,
        allocators,
        schedule,
        mailbox,
    };
    value.validate_binding(roots)?;
    value.check_pool(memory)?;
    Ok(charge.retain(value))
}
