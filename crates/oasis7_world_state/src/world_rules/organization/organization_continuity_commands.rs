use super::*;
#[path = "organization_continuity_availability_commands.rs"]
mod availability_commands;
#[path = "organization_continuity_case_commands.rs"]
mod case_commands;
#[path = "organization_continuity_handoff_commands.rs"]
mod handoff_commands;
#[path = "organization_continuity_registration_commands.rs"]
mod registration_commands;
#[path = "organization_continuity_remedy_commands.rs"]
mod remedy_commands;

// Keep authenticated event facts and the distinct atomic state authorities explicit.
#[allow(clippy::too_many_arguments)]
pub fn apply_continuity_command(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    world_binding: &str,
    is_system_event: bool,
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
) -> Result<String, WorldError> {
    if world_binding.trim().is_empty() {
        return Err(invalid("world_binding_required"));
    }
    let organization_id = command_organization_id(continuity, command);
    if let Some(registration) =
        organization_id.and_then(|organization_id| continuity.registrations.get(organization_id))
        && (registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
            || registration.world_binding != world_binding)
    {
        return Err(invalid("organization_world_binding_or_policy_changed"));
    }
    if !is_system_event
        && let Some(registration) = organization_id
            .and_then(|organization_id| continuity.registrations.get(organization_id))
        && registration.controller_bindings.contains_key(actor)
        && !controller_binding_matches(state, registration, actor)
        && !member_controller_binding_matches(state, continuity, registration, actor)
    {
        return Err(invalid("organization_actor_controller_binding_changed"));
    }
    match command {
        Command::Enable { .. }
        | Command::RegisterActivityContract { .. }
        | Command::CloseActivityTick { .. } => {
            registration_commands::apply_continuity_registration_command(
                state,
                rules,
                continuity,
                actor,
                key,
                command,
                now,
                world_binding,
            )
        }
        Command::RecordAvailability { .. } | Command::FinalizeAvailabilityUnknown { .. } => {
            availability_commands::apply_continuity_availability_command(
                state,
                continuity,
                command,
                now,
                world_binding,
            )
        }
        Command::OpenDormancyCase { .. }
        | Command::OpenVoluntaryCase { .. }
        | Command::AdvancePhase { .. }
        | Command::PublishNotice { .. } => case_commands::apply_continuity_case_command(
            state, rules, continuity, actor, key, command, now,
        ),
        Command::SubmitSuccessor { .. } | Command::CommitHandoff { .. } => {
            handoff_commands::apply_continuity_handoff_command(
                state, continuity, actor, key, command, now,
            )
        }
        Command::SubmitRemedy { .. }
        | Command::ResolveRemedy { .. }
        | Command::AdvanceDisposition { .. }
        | Command::SettleEstateAsset { .. } => remedy_commands::apply_continuity_remedy_command(
            state, rules, continuity, actor, key, command, now, ledgers,
        ),
    }
}
