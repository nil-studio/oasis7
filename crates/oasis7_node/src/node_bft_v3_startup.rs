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
pub(super) fn validate_schedule(
    schedule: &BftHeaderHeightScheduleV3,
    selected: &SelectedNodeBftConfigV3,
) -> Result<(), NodeError> {
    if !selected.membership_enabled
        && (schedule
            .active_set()
            .validator(&selected.binding.local_validator_id)
            .is_none()
            || schedule.pending_transition().is_some()
            || schedule.last_activation_height() > 0)
    {
        return Err(error(
            "wire-3 local validator must be active and membership transitions are not enabled",
        ));
    }
    Ok(())
}
pub(super) fn validate_runtime(
    runtime: &NodeRuntime,
    selected: &SelectedNodeBftConfigV3,
) -> Result<(), NodeError> {
    base::validate_runtime(runtime, selected)
}
pub(super) fn candidate(
    config: &crate::NodeConfig,
    hook: &Hook,
    schedule: &BftHeaderHeightScheduleV3,
    value: &BftHeaderValueV3,
    selected: &SelectedNodeBftConfigV3,
) -> Result<NodeBftPrivateCandidateV3, NodeError> {
    validate_schedule(schedule, selected)?;
    value
        .validate_for_schedule(schedule)
        .map_err(|_| error("wire-3 public value differs from schedule"))?;
    if !selected.membership_enabled
        && (value.execution_commitment.transition.is_some()
            || value.execution_commitment.transition_ref.is_some())
    {
        return Err(error("wire-3 Node transition activation is not enabled"));
    }
    let mut hook = lock_hook(hook)?;
    let private = hook
        .load_bft_v3_private_candidate(value)
        .map_err(|_| error("wire-3 original private candidate missing"))?;
    validate_private(config, &mut **hook, schedule, &private, selected)?;
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
    selected: &SelectedNodeBftConfigV3,
) -> Result<(), NodeError> {
    candidate
        .public_value
        .validate_for_schedule(schedule)
        .map_err(|_| error("wire-3 private candidate schedule rejected"))?;
    if !selected.membership_enabled
        && (candidate
            .public_value
            .execution_commitment
            .transition
            .is_some()
            || candidate
                .public_value
                .execution_commitment
                .transition_ref
                .is_some())
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
    if super::membership::active_key(selected, schedule).is_none() {
        if checkpoint.state.journal.is_some() {
            return Err(error("wire-3 inactive follower has a signing journal"));
        }
        return Ok((
            BftConsensusMachineV3::new(
                schedule.clone(),
                selected.binding.local_validator_id.clone(),
                0,
                selected.binding.timeouts,
            )
            .map_err(error)?,
            Vec::new(),
        ));
    }
    let journal = checkpoint
        .state
        .journal
        .as_ref()
        .ok_or_else(|| error("wire-3 protected journal missing"))?;
    for event in &journal.events {
        if let BftSigningJournalEventV3::Proposal { proposal, .. } = event {
            candidate(config, hook, schedule, &proposal.block, selected)?;
        }
    }
    if let Some(intent) = &journal.pending_proposal_intent {
        candidate(config, hook, schedule, &intent.block, selected)?;
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
    if let Some(network) = &selected.network {
        network.execution.stop_serving();
    }
    base::validate_runtime(runtime, &selected)?;
    let hook = runtime
        .execution_hook
        .clone()
        .ok_or_else(|| error("wire-3 requires actual execution hook"))?;
    let base::PreparedBase {
        mut lease,
        schedule: base_schedule,
        history: driver_history,
        pending: original_pending,
        recovery: cached_recovery,
    } = base::prepare(runtime, &selected, &hook)?;
    let mut checkpoint = lease
        .checkpoint()
        .map_err(|_| error("wire-3 checkpoint unavailable"))?;
    if selected.membership_enabled
        && checkpoint.state.membership_install.is_none()
        && checkpoint.state.pending_driver_apply.is_none()
    {
        checkpoint = lease
            .enable_membership(checkpoint.state.revision, &checkpoint.state.state_digest)
            .map_err(|_| error("wire-3 protected membership enable rejected"))?;
    }
    let history = checkpoint.state.history.clone();
    let initial_pending_apply = checkpoint.state.pending_driver_apply.clone();
    if history.len() > 64 {
        return Err(error("wire-3 H prefix capacity exceeded"));
    }
    selected
        .capacity_exhausted
        .store(history.len() == 64, Ordering::Release);
    if driver_history.len() > history.len()
        || driver_history.iter().zip(&history).any(|(a, b)| a != b)
    {
        return Err(error(
            "wire-3 driver is ahead of or conflicts with protected history",
        ));
    }
    let mut schedule = base_schedule.clone();
    let mut recovery_batches = Vec::new();
    let mut actual_tip = None;
    for certificate in &history {
        let token = verify_commit_certificate_token_v3(&schedule, certificate).map_err(error)?;
        let value = BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let private = candidate(&runtime.config, &hook, &schedule, &value, &selected)?;
        let actions = oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(
            &private.actions_cbor,
        )
        .map_err(|_| error("wire-3 recovered actions rejected"))?;
        let result = lock_hook(&hook)?
            .commit_bft_v3_certificate(token.clone())
            .map_err(|_| error("wire-3 actual history install rejected"))?;
        validate_result(&token, &result)?;
        if let Some(membership) = &checkpoint.state.membership_install {
            for receipt in membership
                .receipts
                .iter()
                .filter(|r| r.body.installed_head_height == result.height)
            {
                let b = &receipt.body;
                if b.installed_head_commit_ref != result.header_id
                    || b.installed_state_root != result.state_root
                    || b.installed_manifest_hash != result.post_manifest_hash
                    || b.driver_install_digest != result.driver_install_digest
                {
                    return Err(error(
                        "wire-3 membership receipt differs from actual replay",
                    ));
                }
            }
        }
        let cached = cached_recovery
            .iter()
            .find(|batch| batch.height == result.height);
        if initial_pending_apply.as_ref().is_some_and(|pending| {
            pending.height == result.height && pending.header_id == result.header_id
        }) || actions
            .iter()
            .any(|action| original_pending.contains(action))
            || cached.is_some()
        {
            let commitment = token.bound_header().commitment();
            let batch = crate::NodeCommittedActionBatch {
                height: result.height,
                slot: certificate.certificate.proposal.round,
                epoch: commitment.authority_epoch,
                block_hash: result.header_id.clone(),
                action_root: commitment.action_root.clone(),
                committed_at_unix_ms: cached
                    .map_or_else(crate::runtime_util::now_unix_ms, |batch| {
                        batch.committed_at_unix_ms
                    }),
                actions,
            };
            if cached.is_some_and(|original| original != &batch) {
                return Err(error(
                    "wire-3 retained recovery completion differs from replay",
                ));
            }
            recovery_batches.push(batch.clone());
            // Retain eligibility before any later replay/network failure.
            let mut cache = runtime
                .bft_v3_recovery_batches
                .lock()
                .map_err(|_| error("wire-3 recovery completion cache poisoned"))?;
            if !cache.contains(&batch) {
                cache.push(batch);
            }
        }
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
        actual_tip = Some(result);
        schedule = token.next_height_schedule().clone();
        validate_schedule(&schedule, &selected)?;
    }
    if checkpoint.state.pending_driver_apply.is_some() {
        return Err(error("wire-3 pending driver apply was not reconciled"));
    }
    if selected.membership_enabled && checkpoint.state.membership_install.is_none() {
        checkpoint = lease
            .enable_membership(checkpoint.state.revision, &checkpoint.state.state_digest)
            .map_err(|_| error("wire-3 protected membership enable rejected"))?;
    }
    // Preserve confirmed notification eligibility even if later current-journal
    // validation fails. A subsequent start replays and compares this cache again.
    if cached_recovery
        .iter()
        .any(|cached| !recovery_batches.iter().any(|batch| batch == cached))
    {
        return Err(error(
            "wire-3 retained recovery completion lost its original history",
        ));
    }
    *runtime
        .bft_v3_recovery_batches
        .lock()
        .map_err(|_| error("wire-3 recovery completion cache poisoned"))? = recovery_batches;
    if lock_hook(&hook)?
        .current_bft_v3_execution_context(&schedule)
        .map_err(|_| error("wire-3 context unavailable"))?
        != NodeBftExecutionContextV3::from_schedule(&schedule)
    {
        return Err(error("wire-3 replay tip context mismatch"));
    }
    restore(&selected, &schedule, &checkpoint, &hook, &runtime.config)?;
    // Parent-sensitive queued admission uses the reconstructed current tip.
    // An already fixed original candidate must run before a future queued batch.
    {
        let expected = NodeBftExecutionContextV3::from_schedule(&schedule);
        let mut execution = lock_hook(&hook)?;
        if execution
            .pending_bft_v3_private_candidate(&expected)
            .map_err(error)?
            .is_none()
        {
            let pending = runtime
                .pending_consensus_actions
                .lock()
                .map_err(|_| error("wire-3 pending queue poisoned"))?
                .clone();
            let actions = super::actions::select_prefix(
                pending,
                runtime.config.max_engine_pending_consensus_actions,
            )?;
            if !actions.is_empty() {
                execution
                    .validate_bft_v3_action_batch(&schedule, &actions)
                    .map_err(error)?;
            }
        }
    }

    if let Some(result) = &actual_tip {
        super::membership::install(&selected, &schedule, result, &mut *lease, &mut checkpoint)?;
    }

    if let Some(network) = &selected.network
        && super::membership::active_key(&selected, &schedule).is_some()
    {
        network
            .exchange
            .ensure_local_ready(&schedule)
            .map_err(error)?;
    }
    if super::membership::active_key(&selected, &schedule).is_some()
        && history.len() < 64
        && checkpoint
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
    let worker = worker::Worker {
        config: runtime.config.clone(),
        selected,
        hook,
        lease: Arc::new(Mutex::new(lease)),
        peers: worker::peer::PeerRuntime::default(),
        checkpoint,
        base_schedule,
        schedule,
        machine,
        effects: effects.into(),
        recovery_batches: runtime.bft_v3_recovery_batches.clone(),
        pending: runtime.pending_consensus_actions.clone(),
        pending_bytes: runtime.pending_consensus_action_queue_bytes.clone(),
        state: runtime.state.clone(),
        last_commit: std::time::Instant::now(),
        batches: runtime.committed_action_batches.clone(),
    };
    let (stop_tx, stop_rx) = mpsc::channel();
    let running = runtime.running.clone();
    let state = runtime.state.clone();
    let peer_state = worker
        .selected
        .network
        .as_ref()
        .map(|n| n.execution.clone());
    worker.selected.rotation_requests.open()?;
    crate::runtime_util::lock_state(&runtime.state).last_error = None;
    runtime.running.store(true, Ordering::SeqCst);
    let spawn = runtime
        .runtime_worker_spawner(std::thread::Builder::new().name("oasis7-wire3-node".into()))
        .spawn(move || {
            if let Err(e) = worker.run(stop_rx) {
                crate::runtime_util::lock_state(&state).last_error = Some(e.to_string());
            }
            if let Some(peer_state) = peer_state {
                peer_state.stop_serving();
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

#[path = "node_bft_v3_startup_base.rs"]
mod base;
