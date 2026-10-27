use super::{SelectedNodeBftConfigV3, error, worker};
use crate::{
    NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3, NodeBftPrivateCandidateV3,
    NodeError, NodeExecutionHook, NodeRuntime,
};
use oasis7_consensus::tendermint_v3::*;
use oasis7_distfs::protected_bft_v3::{BftSignerCheckpointV3, BftSignerProviderV3};
use std::sync::{Arc, Mutex, atomic::Ordering, mpsc};
pub(super) type Hook = Arc<Mutex<Box<dyn NodeExecutionHook>>>;
pub(super) fn lock_hook(
    hook: &Hook,
) -> Result<std::sync::MutexGuard<'_, Box<dyn NodeExecutionHook>>, NodeError> {
    hook.lock()
        .map_err(|_| error("wire-3 execution hook poisoned"))
}
pub(super) fn singleton(
    schedule: &BftHeaderHeightScheduleV3,
    local: &str,
) -> Result<(), NodeError> {
    if schedule.active_set().validators().len() != 1
        || schedule.active_set().validator(local).is_none()
        || schedule.pending_transition().is_some()
    {
        return Err(error(
            "this wire-3 Node supports exactly one active local validator and no transition",
        ));
    }
    Ok(())
}
pub(super) fn validate_runtime(
    runtime: &NodeRuntime,
    selected: &SelectedNodeBftConfigV3,
) -> Result<(), NodeError> {
    let b = &selected.binding;
    b.validate().map_err(|_| error("wire-3 binding rejected"))?;
    singleton(b.initial_schedule(), &b.local_validator_id)?;
    runtime
        .config
        .network_policy
        .validate_for_runtime_role(runtime.config.role)?;
    if b.scope.world_id != runtime.config.world_id
        || b.local_validator_id != runtime.config.node_id
        || runtime.config.network_policy.node_role_claim
            != oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore
        || runtime.config.gossip.is_some()
        || runtime.local_execution_bootstrap.is_some()
        || runtime.local_execution_genesis_bootstrap.is_some()
        || runtime.replication_network_consensus_enabled
    {
        return Err(error(
            "wire-3 Node scope/role/bootstrap or peer transport is unsupported",
        ));
    }
    let replication = runtime
        .config
        .replication
        .as_ref()
        .ok_or_else(|| error("wire-3 Node requires durable replication configuration"))?;
    if replication.consensus_signer()?.is_some() {
        return Err(error("wire-3 Node refuses a local consensus signing key"));
    }
    if !b.provisioned_signing_keys.iter().any(|key| {
        b.initial_schedule()
            .active_set()
            .validator(&b.local_validator_id)
            .is_some_and(|v| v.public_key_hex == key.public_key_hex)
    }) {
        return Err(error(
            "wire-3 local validator has no provisioned protected key",
        ));
    }
    Ok(())
}
pub(super) fn candidate(
    config: &crate::NodeConfig,
    hook: &Hook,
    schedule: &BftHeaderHeightScheduleV3,
    value: &BftHeaderValueV3,
) -> Result<NodeBftPrivateCandidateV3, NodeError> {
    singleton(schedule, &config.node_id)?;
    value
        .validate_for_schedule(schedule)
        .map_err(|_| error("wire-3 public value differs from schedule"))?;
    if value.execution_commitment.transition.is_some()
        || value.execution_commitment.transition_ref.is_some()
    {
        return Err(error("wire-3 Node transition activation is not enabled"));
    }
    let mut hook = lock_hook(hook)?;
    let private = hook
        .load_bft_v3_private_candidate(value)
        .map_err(|_| error("wire-3 original private candidate missing"))?;
    validate_private(config, &mut **hook, schedule, &private)?;
    if private.public_value != *value {
        return Err(error("wire-3 loaded private value changed"));
    }
    Ok(private)
}
pub(super) fn validate_private(
    config: &crate::NodeConfig,
    hook: &mut dyn NodeExecutionHook,
    schedule: &BftHeaderHeightScheduleV3,
    candidate: &NodeBftPrivateCandidateV3,
) -> Result<(), NodeError> {
    candidate
        .public_value
        .validate_for_schedule(schedule)
        .map_err(|_| error("wire-3 private candidate schedule rejected"))?;
    if candidate
        .public_value
        .execution_commitment
        .transition
        .is_some()
        || candidate
            .public_value
            .execution_commitment
            .transition_ref
            .is_some()
    {
        return Err(error("wire-3 Node transitions are not enabled"));
    }
    let bound = candidate
        .public_value
        .validate_binding()
        .map_err(|_| error("wire-3 candidate header binding rejected"))?;
    let payload = oasis7_consensus::formal_bft_payload_v3::ValidatedBftPrivatePayloadV3::decode(
        bound,
        &candidate.actions_cbor,
    )
    .map_err(|_| error("wire-3 private payload rejected"))?;
    let actions = payload.actions();
    if actions.len() > config.max_engine_pending_consensus_actions {
        return Err(error("wire-3 candidate action count exceeded"));
    }
    for action in actions {
        if action.payload_cbor.len() > config.max_consensus_action_payload_bytes {
            return Err(error("wire-3 action byte bound exceeded"));
        }
        crate::node_runtime_core::validate_consensus_action_payload_auth(
            config,
            &action.payload_cbor,
        )?;
    }
    hook.validate_bft_v3_action_batch(schedule, actions)
        .map_err(|_| error("wire-3 parent-policy admission rejected"))?;
    let expected = NodeBftExecutionContextV3::from_schedule(schedule);
    if hook
        .current_bft_v3_execution_context(schedule)
        .map_err(|_| error("wire-3 current context unavailable"))?
        != expected
    {
        return Err(error("wire-3 driver current context differs from schedule"));
    }
    hook.preview_bft_v3_candidate(&expected, candidate)
        .map_err(|_| error("wire-3 actual execution rejected"))
}
pub(super) fn validate_result(
    token: &VerifiedBftHeaderCertificateV3,
    result: &NodeBftExecutionCommitResultV3,
) -> Result<(), NodeError> {
    let c = token.bound_header().commitment();
    if result.height != c.height
        || result.header_id != token.block_id()
        || result.execution_commitment_hash
            != token.certified_header().header.execution_commitment_hash
        || result.execution_state_root != c.execution_state_root
        || result.state_root != c.state_root
        || result.post_manifest_hash != c.post_manifest_hash
        || result.driver_install_digest.len() != 64
        || !result
            .driver_install_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error(
            "wire-3 driver install receipt differs from verified header",
        ));
    }
    Ok(())
}
pub(super) fn restore(
    selected: &SelectedNodeBftConfigV3,
    schedule: &BftHeaderHeightScheduleV3,
    checkpoint: &BftSignerCheckpointV3,
    hook: &Hook,
    config: &crate::NodeConfig,
) -> Result<(BftConsensusMachineV3, Vec<BftConsensusActionV3>), NodeError> {
    let journal = checkpoint
        .state
        .journal
        .as_ref()
        .ok_or_else(|| error("wire-3 protected journal missing"))?;
    for event in &journal.events {
        if let BftSigningJournalEventV3::Proposal { proposal, .. } = event {
            candidate(config, hook, schedule, &proposal.block)?;
        }
    }
    if let Some(intent) = &journal.pending_proposal_intent {
        candidate(config, hook, schedule, &intent.block)?;
    }
    replay_bft_signing_journal_with_effects_v3(
        journal,
        schedule.active_set(),
        schedule,
        selected.binding.timeouts,
    )
    .map_err(error)
}
pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    if runtime.running.load(Ordering::SeqCst) || runtime.worker.is_some() {
        return Err(NodeError::AlreadyRunning {
            node_id: runtime.config.node_id.clone(),
        });
    }
    let selected = runtime
        .bft_v3_config
        .clone()
        .ok_or_else(|| error("wire-3 mode missing"))?;
    validate_runtime(runtime, &selected)?;
    let hook = runtime
        .execution_hook
        .clone()
        .ok_or_else(|| error("wire-3 requires actual execution hook"))?;
    lock_hook(&hook)?
        .configure_bft_v3_execution(&selected.binding)
        .map_err(|_| error("wire-3 durable configuration rejected"))?;
    let mut driver_history = Vec::new();
    loop {
        let after = driver_history.last().map_or(
            selected.binding.handoff_position,
            |c: &FormalBftCertifiedHeaderV3| c.execution_commitment.height,
        );
        let page = lock_hook(&hook)?
            .load_bft_v3_commit_history_page(&selected.binding, after, 16)
            .map_err(|_| error("wire-3 driver history unavailable"))?;
        if page.is_empty() {
            break;
        }
        if page.len() > 16 || driver_history.len() + page.len() > 64 {
            return Err(error("wire-3 driver history capacity"));
        }
        for c in page {
            let expected = selected
                .binding
                .initial_schedule()
                .height()
                .checked_add(driver_history.len() as u64)
                .ok_or_else(|| error("wire-3 height overflow"))?;
            if c.execution_commitment.height != expected {
                return Err(error("wire-3 driver history gap"));
            }
            driver_history.push(c);
        }
    }
    // Existing queued input must pass the actual H parent policy before a
    // remote acquisition can fence another owner or any signing is possible.
    {
        let pending = runtime
            .pending_consensus_actions
            .lock()
            .map_err(|_| error("wire-3 pending queue poisoned"))?;
        let mut execution = lock_hook(&hook)?;
        for action in pending.iter() {
            action
                .validate()
                .map_err(|_| error("wire-3 queued action rejected"))?;
            crate::node_runtime_core::validate_consensus_action_payload_auth(
                &runtime.config,
                &action.payload_cbor,
            )?;
            execution
                .validate_bft_v3_action_payload(&action.payload_cbor)
                .map_err(|_| error("wire-3 queued payload is unsupported"))?;
        }
    }
    let mut lease = selected
        .provider
        .acquire(&selected.binding)
        .map_err(|_| error("wire-3 protected provider acquire rejected"))?;
    let mut checkpoint = lease
        .checkpoint()
        .map_err(|_| error("wire-3 checkpoint unavailable"))?;
    let history = checkpoint.state.history.clone();
    if driver_history.len() > history.len()
        || driver_history.iter().zip(&history).any(|(a, b)| a != b)
    {
        return Err(error(
            "wire-3 driver is ahead of or conflicts with protected history",
        ));
    }
    let mut schedule = selected.binding.initial_schedule().clone();
    let mut committed_actions = Vec::new();
    for certificate in &history {
        let token = verify_commit_certificate_token_v3(&schedule, certificate).map_err(error)?;
        let value = BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let private = candidate(&runtime.config, &hook, &schedule, &value)?;
        committed_actions.extend(
            oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(
                &private.actions_cbor,
            )
            .map_err(|_| error("wire-3 recovered actions rejected"))?,
        );
        let result = lock_hook(&hook)?
            .commit_bft_v3_certificate(token.clone())
            .map_err(|_| error("wire-3 actual history install rejected"))?;
        validate_result(&token, &result)?;
        if let Some(receipt) = checkpoint
            .state
            .applied_installs
            .iter()
            .find(|r| r.height == result.height)
        {
            if receipt.header_id != result.header_id
                || receipt.driver_install_digest != result.driver_install_digest
            {
                return Err(error("wire-3 applied receipt differs from replay"));
            }
        } else {
            checkpoint = lease
                .confirm_applied(
                    checkpoint.state.revision,
                    &checkpoint.state.state_digest,
                    result.height,
                    &result.header_id,
                    &result.driver_install_digest,
                )
                .map_err(|_| error("wire-3 pending install confirmation rejected"))?;
        }
        schedule = token.next_height_schedule().clone();
        singleton(&schedule, &selected.binding.local_validator_id)?;
    }
    if checkpoint.state.pending_driver_apply.is_some() {
        return Err(error("wire-3 pending driver apply was not reconciled"));
    }
    if lock_hook(&hook)?
        .current_bft_v3_execution_context(&schedule)
        .map_err(|_| error("wire-3 context unavailable"))?
        != NodeBftExecutionContextV3::from_schedule(&schedule)
    {
        return Err(error("wire-3 replay tip context mismatch"));
    }
    if history.len() >= 64 {
        return Err(error("wire-3 H prefix capacity exhausted"));
    }
    restore(&selected, &schedule, &checkpoint, &hook, &runtime.config)?;
    if checkpoint
        .state
        .journal
        .as_ref()
        .is_some_and(|j| j.height_started_at_ms.is_none())
    {
        checkpoint = lease
            .start_height_timer(checkpoint.state.revision)
            .map_err(|_| error("wire-3 protected timer rejected"))?;
    }
    let (machine, effects) = restore(&selected, &schedule, &checkpoint, &hook, &runtime.config)?;
    {
        let mut pending = runtime
            .pending_consensus_actions
            .lock()
            .map_err(|_| error("wire-3 pending queue poisoned"))?;
        pending.retain(|action| !committed_actions.contains(action));
        runtime.pending_consensus_action_queue_bytes.store(
            crate::action_payload_bytes(pending.iter()),
            Ordering::Release,
        );
    }
    let worker = worker::Worker {
        config: runtime.config.clone(),
        selected,
        hook,
        lease,
        checkpoint,
        schedule,
        machine,
        effects: effects.into(),
        pending: runtime.pending_consensus_actions.clone(),
        pending_bytes: runtime.pending_consensus_action_queue_bytes.clone(),
        state: runtime.state.clone(),
        last_commit: std::time::Instant::now(),
        batches: runtime.committed_action_batches.clone(),
    };
    let (stop_tx, stop_rx) = mpsc::channel();
    let running = runtime.running.clone();
    let state = runtime.state.clone();
    crate::runtime_util::lock_state(&runtime.state).last_error = None;
    runtime.running.store(true, Ordering::SeqCst);
    let spawn = runtime
        .runtime_worker_spawner(std::thread::Builder::new().name("oasis7-wire3-node".into()))
        .spawn(move || {
            if let Err(e) = worker.run(stop_rx) {
                crate::runtime_util::lock_state(&state).last_error = Some(e.to_string());
            }
            running.store(false, Ordering::SeqCst);
        });
    match spawn {
        Ok(worker) => {
            runtime.worker = Some(worker);
            runtime.stop_tx = Some(stop_tx);
            Ok(())
        }
        Err(e) => {
            runtime.running.store(false, Ordering::SeqCst);
            Err(NodeError::ThreadSpawnFailed {
                reason: e.to_string(),
            })
        }
    }
}
