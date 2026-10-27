//! Shared isolated World execution for the V2 BFT block wire.
//!
//! This returns the concrete post-execution World and roots without choosing
//! the profile-specific execution commitment. The engineering V2 caller keeps
//! its existing hash payload and domain in `driver_bft_v2.rs`.
use super::*;
use oasis7_consensus::tendermint_v2::VerifiedBftHandoffTargetV1;
use oasis7_formal_protocol::bft::VerifiedFormalBftPrefixV1;

pub(super) struct ExecutedWorldTransition {
    pub(super) world: RuntimeWorld,
    pub(super) snapshot_bytes: Vec<u8>,
    pub(super) journal_bytes: Vec<u8>,
    pub(super) event_root: String,
    pub(super) execution_state_root: String,
    pub(super) state_root: String,
    pub(super) post_manifest_hash: String,
    pub(super) transition: Option<BftValidatorTransitionV2>,
    pub(super) transition_ref: Option<BftValidatorTransitionRefV2>,
}

pub(super) struct WorldTransitionRequest<'a> {
    pub(super) parent: &'a RuntimeWorld,
    pub(super) scope: &'a BftScopeV2,
    pub(super) authority_epoch: u64,
    pub(super) schedule: &'a BftHeightProposerScheduleV2,
    pub(super) block: &'a BftBlockV2,
    pub(super) expected_parent: &'a str,
    pub(super) input_hash: &'a str,
    pub(super) tick_authority: &'a str,
    pub(super) formal_handoff: Option<&'a VerifiedBftHandoffTargetV1>,
    pub(super) formal_prefix: Option<&'a VerifiedFormalBftPrefixV1>,
}

pub(super) fn execute_world_transition(
    request: WorldTransitionRequest<'_>,
) -> Result<ExecutedWorldTransition, String> {
    let WorldTransitionRequest {
        parent,
        scope,
        authority_epoch,
        schedule,
        block,
        expected_parent,
        input_hash,
        tick_authority,
        formal_handoff,
        formal_prefix,
    } = request;
    block.validate()?;
    if formal_handoff.is_some() && formal_prefix.is_some() {
        return Err("BFT execution cannot combine initial handoff and successor prefix".into());
    }
    if formal_prefix.is_some_and(|prefix| prefix.next_height_schedule() != schedule) {
        return Err("BFT execution schedule differs from its complete formal prefix".into());
    }
    if block.height == 0 || block.parent_commit_ref != expected_parent {
        return Err("BFT V2 execution input has noncanonical height or parent".into());
    }
    let actions = decode_actions_v2(block.actions_cbor.as_slice())?;
    if super::super::action_root(actions.as_slice())? != block.action_root {
        return Err("BFT V2 action root differs from canonical committed actions".into());
    }
    let context = NodeBftExecutionContextV2::from_schedule(schedule);
    super::validate_expected_world_context_v2(
        parent,
        scope,
        authority_epoch,
        schedule,
        &context,
        expected_parent,
    )?;
    if block.manifest_hash != context.manifest_hash {
        return Err("BFT V2 parent manifest hash is not exact".into());
    }

    let parent_snapshot_bytes = super::super::super::super::snapshot_to_cbor(&parent.snapshot())?;
    let parent_journal_bytes = super::super::super::super::journal_to_cbor(parent.journal())?;
    let parent_snapshot = parent.snapshot();
    let mut next_world = if let Some(prefix) = formal_prefix {
        // H's retained-history proof authenticates H, not this later BFT
        // Snapshot. Fork the actual replayed parent, bind it to the complete
        // prefix and reject uncommitted queues instead of weakening H restore.
        let mut detached = parent.clone();
        detached
            .attach_formal_bft_prefix_parent_v1(prefix)
            .map_err(|error| format!("attach formal BFT prefix parent: {error:?}"))?;
        detached
    } else if parent_snapshot.state.retained_history_root.is_some() {
        parent
            .restore_snapshot_with_attached_formal_history(
                parent_snapshot,
                parent.journal().clone(),
            )
            .map_err(|error| format!("restore detached BFT V2 parent World: {error:?}"))?
    } else {
        RuntimeWorld::from_snapshot_with_release_security_policy(
            parent_snapshot,
            parent.journal().clone(),
            parent.release_security_policy().clone(),
        )
        .map_err(|error| format!("restore detached BFT V2 parent World: {error:?}"))?
    };
    if super::super::super::super::snapshot_to_cbor(&next_world.snapshot())?
        != parent_snapshot_bytes
        || super::super::super::super::journal_to_cbor(next_world.journal())?
            != parent_journal_bytes
    {
        return Err("detached BFT V2 parent restore changed snapshot or journal".into());
    }
    let commit_context = super::make_commit_context_for_binding(
        scope,
        authority_epoch,
        tick_authority,
        block.height,
        input_hash.to_owned(),
        actions.as_slice(),
    )?;
    let runtime_actions = super::super::decode_closed_runtime_actions(&commit_context)?;
    super::governance::validate_validator_transition_action_schedule_v2(
        schedule,
        &runtime_actions,
    )?;
    if let Some(handoff) = formal_handoff {
        next_world
            .attach_formal_bft_handoff_parent_v1(handoff, schedule)
            .map_err(|error| format!("attach formal BFT handoff parent: {error:?}"))?;
    } else if formal_prefix.is_none() {
        next_world
            .attach_bft_governance_parent_v2(schedule)
            .map_err(|error| format!("attach BFT V2 governance execution parent: {error:?}"))?;
    }
    let tick_context = RuntimeCommittedTickContext {
        height: block.height,
        slot: block.height.saturating_sub(1),
        epoch: block.authority_epoch,
        node_block_hash: input_hash.to_owned(),
        action_root: block.action_root.clone(),
        authority_node_id: tick_authority.to_owned(),
        committed_at_unix_ms: 0,
    };
    next_world
        .begin_bft_execution_tick(&tick_context)
        .map_err(|error| format!("begin BFT V2 governance execution tick: {error:?}"))?;
    super::validate_parent_governance_actions(&next_world, &commit_context)?;
    super::governance::validate_set_change_actions_have_registry_v2(&next_world, &runtime_actions)?;
    let has_set_change_action =
        super::governance::has_validator_set_change_action_v2(&runtime_actions);
    for (action, origin, authorization) in runtime_actions {
        if let Some(authorization) = authorization {
            next_world.submit_governance_action(action, authorization);
        } else if let Some(origin) = origin {
            next_world
                .submit_recipe_action_with_origin(action, origin)
                .map_err(|error| format!("submit BFT V2 recipe origin: {error:?}"))?;
        } else {
            next_world.submit_action(action);
        }
    }
    let event_start = next_world.journal().len();
    let mut executor = WasmExecutor::new_for_deterministic_replay(WasmExecutorConfig::default())
        .map_err(|error| format!("initialize isolated BFT V2 executor: {error}"))?;
    next_world
        .step_with_modules_for_committed_context(&mut executor, &tick_context)
        .map_err(|error| format!("isolated BFT V2 World execution failed: {error:?}"))?;
    next_world
        .finish_bft_execution_tick(&tick_context)
        .map_err(|error| format!("finish BFT V2 governance execution tick: {error:?}"))?;
    if next_world.pending_actions_len() != 0 || next_world.pending_effects_len() != 0 {
        return Err("BFT V2 execution produced pending local actions or effects".into());
    }

    let manifest = parent.chain_resource_manifest();
    let resource_context = ChainResourceDerivationContext {
        world_id: scope.world_id.as_str(),
        chain_id: scope.chain_id.as_str(),
        genesis_ref: Some(scope.genesis_hash.as_str()),
        created_at_height: manifest.created_at_height,
        manifest_height: block.height,
        commit_block_hash: Some(input_hash),
        tick: next_world.state().time,
    };
    let snapshot = next_world.snapshot_with_chain_resource_context(
        resource_context,
        manifest.world_config_hash.clone(),
        manifest.generation_algorithm_hash.clone(),
    );
    next_world = next_world
        .restore_candidate_snapshot_with_attached_formal_history(
            snapshot,
            next_world.journal().clone(),
        )
        .map_err(|error| format!("install BFT V2 derived chain-resource context: {error:?}"))?;
    if next_world.state().time != block.height {
        return Err("BFT V2 execution World tick does not equal block height".into());
    }

    let new_events = next_world
        .journal()
        .events
        .get(event_start..)
        .ok_or("BFT V2 journal moved backwards")?;
    let event_root = super::blake3_hex(
        &serde_cbor::to_vec(&new_events)
            .map_err(|error| format!("encode BFT V2 event root: {error}"))?,
    );
    let snapshot_bytes = super::super::super::super::snapshot_to_cbor(&next_world.snapshot())?;
    let journal_bytes = super::super::super::super::journal_to_cbor(next_world.journal())?;
    let execution_state_root = super::blake3_hex(snapshot_bytes.as_slice());
    let state_root = next_world
        .execution_state_identity()
        .map_err(|error| format!("compute BFT V2 state root: {error:?}"))?
        .state_root_hash;
    let post_manifest_hash = next_world
        .current_manifest_hash()
        .map_err(|error| format!("compute BFT V2 post manifest: {error:?}"))?;
    let (transition, transition_ref) = super::governance::transition_metadata_for_schedule(
        schedule,
        &actions,
        has_set_change_action,
        Some(&next_world),
    )?;
    Ok(ExecutedWorldTransition {
        world: next_world,
        snapshot_bytes,
        journal_bytes,
        event_root,
        execution_state_root,
        state_root,
        post_manifest_hash,
        transition,
        transition_ref,
    })
}
