use super::ClosedRuntimeAction;
use super::schedule::ExecutionSchedule;
use oasis7_consensus::tendermint_v2::BftValidatorSetV2;
use oasis7_node::{BftValidatorTransitionV2, BftValidatorV2, NodeConsensusAction};
use oasis7_proto::distributed_bft::v2::BftValidatorTransitionRefV2;
use oasis7_runtime::World as RuntimeWorld;

pub(super) fn transition_metadata_for_schedule(
    schedule: &(impl ExecutionSchedule + ?Sized),
    actions: &[NodeConsensusAction],
    has_set_change_action: bool,
    candidate_world: Option<&RuntimeWorld>,
) -> Result<
    (
        Option<BftValidatorTransitionV2>,
        Option<BftValidatorTransitionRefV2>,
    ),
    String,
> {
    if let Some(reference) = schedule.pending_transition() {
        if let Some(world) = candidate_world {
            validate_pending_transition_projection_v2(schedule, reference, world)?;
        }
        return Ok((None, Some(reference.clone())));
    }
    let activation_height = schedule
        .height()
        .checked_add(2)
        .ok_or("BFT V2 transition activation height overflow")?;
    let epoch_length = schedule.policy().epoch_length_ticks;
    if epoch_length == 0 || activation_height % epoch_length != 0 {
        return Ok((None, None));
    }
    let target_epoch = activation_height / epoch_length;
    if target_epoch <= schedule.parent_tick() / epoch_length {
        return Ok((None, None));
    }
    let Some(world) = candidate_world else {
        return Ok((None, None));
    };
    let projection = world
        .project_governance_finality_registry_for_candidate_epoch(target_epoch)
        .map_err(|error| format!("project BFT V2 candidate validator registry: {error:?}"))?;
    let Some(registry) = projection.effective_registry else {
        if has_set_change_action {
            return Err(
                "BFT V2 validator set change has no projected governance finality registry".into(),
            );
        }
        return Ok((None, None));
    };
    let validators = validators_from_registry(&registry)?;
    let next_set = BftValidatorSetV2::new(schedule.scope().clone(), validators.clone())?;
    if next_set.set_hash() == schedule.active_set().set_hash() {
        return Ok((None, None));
    }
    let transition = BftValidatorTransitionV2 {
        source_commit_ref: schedule.parent_commit_ref().to_string(),
        source_state_root: schedule.parent_state_root().to_string(),
        source_profile: schedule.policy().profile_id.clone(),
        source_epoch: schedule.parent_tick() / epoch_length,
        target_epoch,
        epoch_length_ticks: epoch_length,
        mapping_ruleset_hash: schedule.policy_hash().to_string(),
        scheduled_height: schedule.height(),
        activation_height,
        old_validator_set_hash: schedule.active_set().set_hash().to_string(),
        new_validators: validators,
        new_validator_set_hash: next_set.set_hash().to_string(),
        governance_action_digest: super::governance_action_digest(actions)?,
        projected_registry_digest: runtime_digest_to_wire_hex(
            &projection.effective_registry_digest,
        )?,
    };
    transition.validate()?;
    Ok((Some(transition), None))
}

fn runtime_digest_to_wire_hex(digest: &str) -> Result<String, String> {
    let payload = digest
        .strip_prefix("blake3:")
        .ok_or("BFT V2 projected registry digest is missing the Runtime blake3 prefix")?;
    let bytes = hex::decode(payload)
        .map_err(|error| format!("BFT V2 projected registry digest is invalid hex: {error}"))?;
    let _: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "BFT V2 projected registry digest must contain exactly 32 bytes")?;
    if hex::encode(&bytes) != payload {
        return Err("BFT V2 projected registry digest is not canonical lowercase hex".into());
    }
    Ok(payload.to_string())
}

pub(super) fn has_validator_set_change_action_v2(actions: &[ClosedRuntimeAction]) -> bool {
    actions.iter().any(|(action, _, _)| {
        matches!(
            action,
            oasis7_runtime::Action::ActivateGovernanceValidatorAdmission { .. }
                | oasis7_runtime::Action::RevokeGovernanceValidatorAdmission { .. }
        )
    })
}

pub(super) fn validate_set_change_actions_have_registry_v2(
    world: &RuntimeWorld,
    actions: &[ClosedRuntimeAction],
) -> Result<(), String> {
    if !has_validator_set_change_action_v2(actions) {
        return Ok(());
    }
    let registry = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|error| format!("resolve BFT V2 governance finality registry: {error:?}"))?;
    if registry.is_none() {
        return Err(
            "BFT V2 validator set changes require a configured governance finality registry".into(),
        );
    }
    Ok(())
}

fn validate_pending_transition_projection_v2(
    schedule: &(impl ExecutionSchedule + ?Sized),
    reference: &BftValidatorTransitionRefV2,
    candidate_world: &RuntimeWorld,
) -> Result<(), String> {
    let epoch_length = schedule.policy().epoch_length_ticks;
    let expected_activation_height = schedule
        .height()
        .checked_add(1)
        .ok_or("BFT V2 pending transition activation height overflow")?;
    if epoch_length == 0
        || reference.scheduled_height.checked_add(1) != Some(schedule.height())
        || reference.activation_height != expected_activation_height
        || !reference.activation_height.is_multiple_of(epoch_length)
        || candidate_world.state().time != schedule.height()
    {
        return Err("BFT V2 pending transition does not match the exact h+1 World cursor".into());
    }
    let target_epoch = reference.activation_height / epoch_length;
    let projection = candidate_world
        .project_governance_finality_registry_for_candidate_epoch(target_epoch)
        .map_err(|error| format!("validate pending BFT V2 membership projection: {error}"))?;
    if projection.epoch_length_ticks != epoch_length {
        return Err("BFT V2 pending transition epoch length drifted in candidate World".into());
    }
    let registry = projection
        .effective_registry
        .ok_or("BFT V2 pending transition has no projected governance registry")?;
    let validators = validators_from_registry(&registry)?;
    let projected_set = BftValidatorSetV2::new(schedule.scope().clone(), validators)?;
    if projected_set.set_hash() != reference.new_validator_set_hash {
        return Err(
            "BFT V2 h+1 candidate World no longer matches the committed pending validator set"
                .into(),
        );
    }
    Ok(())
}

/// Keep governance membership changes aligned with the exact height at which
/// the V2 schedule can carry their resulting validator set. This is checked
/// during queue admission, candidate construction, and independent execution
/// so a validly signed but late action cannot be silently delayed to a later
/// epoch boundary.
pub(super) fn validate_validator_transition_action_schedule_v2(
    schedule: &(impl ExecutionSchedule + ?Sized),
    actions: &[ClosedRuntimeAction],
) -> Result<(), String> {
    let epoch_length = schedule.policy().epoch_length_ticks;
    if epoch_length == 0 {
        return Err("BFT V2 validator transition has zero epoch length".into());
    }
    let mut has_membership_change = false;
    for (action, _, _) in actions {
        match action {
            oasis7_runtime::Action::ActivateGovernanceValidatorAdmission {
                activation_epoch,
                ..
            } => {
                has_membership_change = true;
                let activation_height = activation_epoch
                    .checked_mul(epoch_length)
                    .ok_or("BFT V2 activation epoch height overflow")?;
                let scheduled_height = activation_height
                    .checked_sub(2)
                    .ok_or("BFT V2 activation must be scheduled two heights before activation")?;
                if schedule.height() > scheduled_height {
                    return Err(format!(
                        "BFT V2 validator activation arrived after its exact scheduling height: current={} latest={scheduled_height}",
                        schedule.height()
                    ));
                }
            }
            oasis7_runtime::Action::RevokeGovernanceValidatorAdmission { .. } => {
                has_membership_change = true;
                let activation_height = schedule
                    .height()
                    .checked_add(2)
                    .ok_or("BFT V2 revocation activation height overflow")?;
                if activation_height % epoch_length != 0 {
                    return Err(format!(
                        "BFT V2 validator revocation is not at an exact two-height epoch boundary: current={} epoch_length={epoch_length}",
                        schedule.height()
                    ));
                }
            }
            _ => {}
        }
    }
    if has_membership_change && schedule.pending_transition().is_some() {
        return Err(
            "BFT V2 validator membership changes are forbidden while a transition is pending"
                .into(),
        );
    }
    Ok(())
}

fn validators_from_registry(
    registry: &oasis7_runtime::GovernanceFinalitySignerRegistry,
) -> Result<Vec<BftValidatorV2>, String> {
    if registry.signer_bindings.len() != registry.validator_stakes.len() {
        return Err("projected governance signer and stake maps differ".into());
    }
    registry
        .signer_bindings
        .iter()
        .map(|(validator_id, public_key_hex)| {
            let voting_power = registry
                .validator_stakes
                .get(validator_id)
                .copied()
                .ok_or_else(|| "projected governance validator has no stake".to_string())?;
            let validator = BftValidatorV2 {
                validator_id: validator_id.clone(),
                public_key_hex: public_key_hex.clone(),
                voting_power,
            };
            validator.validate()?;
            Ok(validator)
        })
        .collect()
}
