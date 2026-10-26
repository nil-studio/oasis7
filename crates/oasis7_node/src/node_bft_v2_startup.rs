use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use oasis7_consensus::tendermint_v2::{
    BftConsensusActionV2, BftConsensusMachineV2, BftHeightProposerScheduleV2, BftSigningJournalV2,
    BftValidatorSetV2, VerifiedBftCommitCertificateV2, replay_bft_signing_journal_v2,
    replay_bft_signing_journal_with_effects_v2, verify_commit_certificate_token_v2,
};
use oasis7_distfs::protected_bft_v2::{
    BftLocalInstallEvidenceV2, BftSignerLeaseV2, BftSignerProviderV2,
    BftTransitionInstallPreparationV2,
};
use oasis7_proto::distributed_bft::v2::{BftCommitCertificateV2, BftValidatorTransitionRefV2};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1,
};

use super::{SelectedNodeBftConfigV2, execution_config, genesis_schedule, worker};
use crate::runtime_util::lock_state;
use crate::{
    GossipEndpoint, NodeBftExecutionCommitResultV2, NodeError, NodeExecutionHook, NodeRuntime,
};

#[cfg(test)]
#[path = "node_bft_v2_startup_tests.rs"]
mod tests;

pub(super) type Hook = Arc<Mutex<Box<dyn NodeExecutionHook>>>;

pub(super) fn error(reason: impl Into<String>) -> NodeError {
    NodeError::Consensus {
        reason: reason.into(),
    }
}

pub(super) fn lock_hook(
    hook: &Hook,
) -> Result<std::sync::MutexGuard<'_, Box<dyn NodeExecutionHook>>, NodeError> {
    hook.lock()
        .map_err(|_| error("BFT V2 execution hook lock poisoned; stop"))
}

pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    if runtime.running.load(Ordering::SeqCst) || runtime.worker.is_some() {
        return Err(NodeError::AlreadyRunning {
            node_id: runtime.config.node_id.clone(),
        });
    }
    let selected = runtime
        .bft_v2_config
        .clone()
        .ok_or_else(|| error("missing explicit BFT V2 config"))?;
    validate_runtime_configuration(runtime, &selected)?;

    // Hold the actual driver lifecycle lock before advancing a remote lease.
    // Duplicate local processes must fail without fencing the healthy owner.
    let hook = runtime
        .execution_hook
        .clone()
        .ok_or_else(|| error("BFT V2 requires a durable execution hook"))?;
    let config = execution_config(&selected.binding);
    let (genesis, _) = validated_genesis(&selected)?;
    lock_hook(&hook)?
        .configure_bft_v2_execution(&config)
        .map_err(error)?;
    let driver_history = load_driver_history(&hook, &config)?;

    let mut lease = selected
        .provider
        .acquire(&selected.binding)
        .map_err(|err| error(format!("acquire protected BFT V2 signer lease: {err}")))?;
    let mut checkpoint = lease
        .checkpoint()
        .map_err(|err| error(format!("read protected BFT V2 checkpoint: {err}")))?;
    let protected_now = lease
        .protected_time_ms()
        .map_err(|err| error(format!("read protected BFT V2 time: {err}")))?;
    let history = checkpoint.state.history.clone();
    let history_bytes = validate_history_capacity(&history)?;
    let schedule = verify_history(&history, &genesis, selected.binding.timeouts)?;

    if driver_history.len() > history.len() {
        return Err(error(
            "BFT V2 execution driver is ahead of the protected signer history",
        ));
    }
    for (index, certificate) in driver_history.iter().enumerate() {
        if canonical_certificate_bytes(certificate)?
            != canonical_certificate_bytes(&history[index])?
        {
            return Err(error(
                "BFT V2 execution history conflicts with the protected signer prefix",
            ));
        }
    }

    let mut replay_schedule = genesis;
    let mut last_result = None;
    let mut tip_token = None;
    for certificate in &history {
        let token =
            verify_commit_certificate_token_v2(&replay_schedule, certificate).map_err(error)?;
        crate::node_bft_v2_transport::validate_candidate_capacity(
            token.block(),
            token.active_validator_set(),
        )?;
        validate_candidate_with_hook(&runtime.config, &hook, &replay_schedule, token.block())?;
        let result = lock_hook(&hook)?
            .commit_bft_v2_certificate(token.clone())
            .map_err(error)?;
        validate_execution_result_v2(&token, &result)?;
        // Historical ACKs remain applicable without being reissued at every
        // block, but their original driver receipt must still match actual
        // independent execution replay at the installed ancestor.
        for receipt in checkpoint
            .state
            .install_receipts
            .iter()
            .filter(|receipt| receipt.body.installed_head_height == result.height)
        {
            let body = &receipt.body;
            if body.installed_head_commit_ref != result.bft_commit_ref
                || body.installed_state_root != result.state_root
                || body.installed_manifest_hash != result.post_manifest_hash
                || body.driver_install_digest != result.driver_install_digest
            {
                return Err(error(
                    "BFT V2 historical installation receipt differs from replayed execution",
                ));
            }
        }
        replay_schedule = token.next_height_schedule().clone();
        last_result = Some(result);
        tip_token = Some(token);
    }
    if replay_schedule != schedule {
        return Err(error("BFT V2 protected history changed during replay"));
    }
    let current_context = lock_hook(&hook)?
        .current_bft_v2_execution_context(&schedule)
        .map_err(error)?;
    if current_context != crate::NodeBftExecutionContextV2::from_schedule(&schedule) {
        return Err(error(
            "BFT V2 execution context differs from the fully replayed schedule",
        ));
    }

    let local_key = provisioned_active_key(&selected, &schedule);
    let (mut machine, mut replay_effects) =
        restore_machine_with_effects(&selected, &schedule, &checkpoint, protected_now)?;

    // A new validator joins only after the exact h+1 execution record exists.
    // Replaying the protected journal into the actual consensus machine comes
    // before the provider signs the local installation acknowledgement.
    if let Some(result) = last_result.as_ref()
        && let Some((confirmation, reference)) = history.iter().rev().find_map(|certificate| {
            certificate
                .proposal
                .block
                .transition_ref
                .as_ref()
                .and_then(|reference| {
                    (reference.activation_height == schedule.last_activation_height())
                        .then_some((certificate, reference))
                })
        })
    {
        maybe_install_transition(
            &selected,
            &schedule,
            reference,
            confirmation,
            result,
            &machine,
            &mut *lease,
            &mut checkpoint,
        )?;
    }
    if local_key.is_some()
        && schedule.last_activation_height() > 0
        && !checkpoint.state.install_receipts.iter().any(|receipt| {
            receipt.body.activation_height == schedule.last_activation_height()
                && receipt.body.local_validator_id == selected.binding.local_validator_id
        })
    {
        return Err(error(
            "active BFT V2 validator has no durable installation receipt for its current set",
        ));
    }

    if let Some(journal) = checkpoint.state.journal.as_ref()
        && journal.height_started_at_ms().is_none()
    {
        checkpoint = lease
            .start_height_timer(checkpoint.state.revision)
            .map_err(|err| error(format!("start protected BFT V2 height timer: {err}")))?;
        (machine, replay_effects) = replay_bft_signing_journal_with_effects_v2(
            checkpoint
                .state
                .journal
                .as_ref()
                .ok_or_else(|| error("protected signer omitted active BFT V2 journal"))?,
            schedule.active_set(),
            &schedule,
            selected.binding.timeouts,
        )
        .map_err(error)?;
    }

    let prepared = worker::Worker::restore(worker::WorkerSetup {
        config: runtime.config.clone(),
        selected,
        set: schedule.active_set().clone(),
        schedule,
        machine,
        replay_effects,
        lease,
        checkpoint,
        local_key,
        hook,
        pending: Arc::clone(&runtime.pending_consensus_actions),
        pending_bytes: Arc::clone(&runtime.pending_consensus_action_queue_bytes),
        state: Arc::clone(&runtime.state),
        tip: tip_token,
        history_bytes,
        history,
    })?;
    prepared.publish_status(None);

    let gossip = runtime
        .config
        .gossip
        .as_ref()
        .map(GossipEndpoint::bind)
        .transpose()?
        .map(Arc::new);
    let (stop_tx, stop_rx) = mpsc::channel();
    lock_state(&runtime.state).last_error = None;
    let running = Arc::clone(&runtime.running);
    let state = Arc::clone(&runtime.state);
    let worker_gossip = gossip.clone();
    runtime.running.store(true, Ordering::SeqCst);
    let spawn = runtime
        .runtime_worker_spawner(
            thread::Builder::new().name(format!("aw-bft-v2-{}", runtime.config.node_id)),
        )
        .spawn(move || {
            let result = prepared.run(stop_rx, worker_gossip);
            if let Err(error) = result {
                lock_state(&state).last_error = Some(error.to_string());
            }
            running.store(false, Ordering::SeqCst);
        });
    match spawn {
        Ok(worker) => {
            runtime.stop_tx = Some(stop_tx);
            runtime.worker = Some(worker);
            runtime.gossip_endpoint = gossip;
            Ok(())
        }
        Err(error) => {
            runtime.running.store(false, Ordering::SeqCst);
            Err(NodeError::ThreadSpawnFailed {
                reason: error.to_string(),
            })
        }
    }
}

fn validate_runtime_configuration(
    runtime: &NodeRuntime,
    selected: &SelectedNodeBftConfigV2,
) -> Result<(), NodeError> {
    runtime
        .config
        .network_policy
        .validate_for_runtime_role(runtime.config.role)?;
    if runtime.config.network_policy.node_role_claim
        != oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore
    {
        return Err(error(
            "BFT V2 requires network node_role_claim=validator_core",
        ));
    }
    if runtime.config.node_id != selected.binding.local_validator_id {
        return Err(error(
            "BFT V2 local validator identity must equal NodeRuntime node identity",
        ));
    }
    if runtime.config.world_id != selected.binding.scope.world_id {
        return Err(error("BFT V2 scope does not match the runtime world"));
    }
    if runtime.local_execution_bootstrap.is_some()
        || runtime.local_execution_genesis_bootstrap.is_some()
        || runtime.replication_network_consensus_enabled
    {
        return Err(error(
            "BFT V2 cannot share a legacy/formal local bootstrap or PoS consensus subscription",
        ));
    }
    if runtime.config.replication.is_none() {
        return Err(error("BFT V2 requires durable replication configuration"));
    }
    let set = selected
        .binding
        .validate()
        .map_err(|err| error(format!("invalid protected BFT V2 binding: {err}")))?;
    if runtime.config.gossip.as_ref().is_some_and(|gossip| {
        gossip.peers.len() > oasis7_proto::distributed_bft::v2::BFT_MAX_VALIDATORS_V2
    }) {
        return Err(error("BFT V2 gossip peer capacity exceeded"));
    }
    if set.validators().len() > 1 && runtime.config.gossip.is_none() {
        return Err(error(
            "multi-validator BFT V2 requires an explicit gossip transport",
        ));
    }
    crate::node_bft_v2_transport::validate_configuration(&set)
}

fn validated_genesis(
    selected: &SelectedNodeBftConfigV2,
) -> Result<(BftHeightProposerScheduleV2, BftValidatorSetV2), NodeError> {
    let set = selected
        .binding
        .validate()
        .map_err(|err| error(format!("invalid BFT V2 genesis set: {err}")))?;
    let schedule = genesis_schedule(&selected.binding)?;
    Ok((schedule, set))
}

pub(super) fn validate_history_capacity(
    history: &[BftCommitCertificateV2],
) -> Result<usize, NodeError> {
    if history.len() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
        return Err(error("BFT V2 commit history exceeds height bound"));
    }
    history.iter().try_fold(0usize, |total, certificate| {
        certificate.validate_wire_bounds().map_err(error)?;
        let bytes = serde_cbor::to_vec(certificate)
            .map_err(|err| error(format!("encode BFT V2 history certificate: {err}")))?
            .len();
        let total = total
            .checked_add(bytes)
            .ok_or_else(|| error("BFT V2 history byte count overflow"))?;
        if total > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1 {
            return Err(error("BFT V2 commit history exceeds byte bound"));
        }
        Ok(total)
    })
}

fn verify_history(
    history: &[BftCommitCertificateV2],
    genesis: &BftHeightProposerScheduleV2,
    timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1,
) -> Result<BftHeightProposerScheduleV2, NodeError> {
    let mut schedule = genesis.clone();
    for certificate in history {
        let token = verify_commit_certificate_token_v2(&schedule, certificate).map_err(error)?;
        crate::node_bft_v2_transport::validate_candidate_capacity(
            token.block(),
            token.active_validator_set(),
        )?;
        schedule = token.next_height_schedule().clone();
    }
    let _ = timeouts.validate().map_err(error)?;
    Ok(schedule)
}

fn load_driver_history(
    hook: &Hook,
    config: &crate::NodeBftExecutionConfigV2,
) -> Result<Vec<BftCommitCertificateV2>, NodeError> {
    let mut history = Vec::new();
    let mut bytes = 0usize;
    loop {
        let page = lock_hook(hook)?
            .load_bft_v2_commit_history_page(
                config,
                history.len() as u64,
                BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1,
            )
            .map_err(error)?;
        if page.is_empty() {
            return Ok(history);
        }
        if page.len() > BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1 {
            return Err(error("BFT V2 execution history exceeded page bound"));
        }
        for certificate in page {
            let expected_height = (history.len() as u64)
                .checked_add(1)
                .ok_or_else(|| error("BFT V2 execution history height overflow"))?;
            if certificate.proposal.block.height != expected_height {
                return Err(error("BFT V2 execution history is not contiguous"));
            }
            certificate.validate_wire_bounds().map_err(error)?;
            bytes = bytes
                .checked_add(canonical_certificate_bytes(&certificate)?.len())
                .ok_or_else(|| error("BFT V2 execution history byte count overflow"))?;
            if bytes > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1
                || history.len() >= BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1
            {
                return Err(error("BFT V2 execution history exceeds its bound"));
            }
            history.push(certificate);
        }
    }
}

pub(super) fn canonical_certificate_bytes(
    certificate: &BftCommitCertificateV2,
) -> Result<Vec<u8>, NodeError> {
    serde_cbor::to_vec(certificate)
        .map_err(|err| error(format!("encode BFT V2 certificate: {err}")))
}

pub(super) fn validate_candidate_with_hook(
    config: &crate::NodeConfig,
    hook: &Hook,
    schedule: &BftHeightProposerScheduleV2,
    block: &oasis7_proto::distributed_bft::v2::BftBlockV2,
) -> Result<(), NodeError> {
    block.validate().map_err(error)?;
    crate::node_bft_v2_transport::validate_candidate_capacity(block, schedule.active_set())?;
    let actions: Vec<crate::NodeConsensusAction> = serde_cbor::from_slice(&block.actions_cbor)
        .map_err(|err| error(format!("decode BFT V2 candidate actions: {err}")))?;
    if serde_cbor::to_vec(&actions)
        .map_err(|err| error(format!("encode BFT V2 candidate actions: {err}")))?
        != block.actions_cbor
    {
        return Err(error("BFT V2 candidate action bytes are not canonical"));
    }
    if actions.len() > config.max_engine_pending_consensus_actions {
        return Err(error("BFT V2 candidate exceeds configured action count"));
    }
    for action in &actions {
        action.validate().map_err(|err| error(err.reason))?;
        if action.payload_cbor.len() > config.max_consensus_action_payload_bytes {
            return Err(error(
                "BFT V2 candidate action exceeds configured payload size",
            ));
        }
        // Keep ordinary action authentication in valid(v), including main-token
        // signatures. Only governance parent verification differs from V1.
        crate::node_runtime_core::validate_consensus_action_payload_auth(
            config,
            &action.payload_cbor,
        )?;
    }
    let mut hook = lock_hook(hook)?;
    hook.validate_bft_v2_action_batch(schedule, &actions)
        .map_err(error)?;
    let expected = hook
        .current_bft_v2_execution_context(schedule)
        .map_err(error)?;
    hook.preview_bft_v2_candidate(&expected, block)
        .map_err(error)
}

pub(super) fn validate_execution_result_v2(
    token: &VerifiedBftCommitCertificateV2,
    result: &NodeBftExecutionCommitResultV2,
) -> Result<(), NodeError> {
    let block = token.block();
    let digest_is_canonical = result.driver_install_digest.len() == 64
        && result
            .driver_install_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if result.height != block.height
        || result.bft_commit_ref != token.block_id()
        || result.execution_block_hash != block.execution_block_hash
        || result.execution_state_root != block.execution_state_root
        || result.state_root != block.state_root
        || result.post_manifest_hash != block.post_manifest_hash
        || !digest_is_canonical
    {
        return Err(error(
            "BFT V2 execution hook returned a result outside the verified certificate",
        ));
    }
    Ok(())
}

pub(super) fn provisioned_active_key(
    selected: &SelectedNodeBftConfigV2,
    schedule: &BftHeightProposerScheduleV2,
) -> Option<String> {
    let validator = schedule
        .active_set()
        .validator(&selected.binding.local_validator_id)?;
    selected
        .binding
        .provisioned_signing_keys
        .iter()
        .find(|key| {
            key.validator_id == validator.validator_id
                && key.public_key_hex == validator.public_key_hex
        })
        .map(|key| key.public_key_hex.clone())
}

pub(super) fn restore_machine(
    selected: &SelectedNodeBftConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    checkpoint: &oasis7_distfs::protected_bft_v2::BftSignerCheckpointV2,
    protected_now: i64,
) -> Result<BftConsensusMachineV2, NodeError> {
    restore_machine_with_effects(selected, schedule, checkpoint, protected_now)
        .map(|(machine, _)| machine)
}

pub(super) fn restore_machine_with_effects(
    selected: &SelectedNodeBftConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    checkpoint: &oasis7_distfs::protected_bft_v2::BftSignerCheckpointV2,
    protected_now: i64,
) -> Result<(BftConsensusMachineV2, Vec<BftConsensusActionV2>), NodeError> {
    match (
        provisioned_active_key(selected, schedule),
        checkpoint.state.journal.as_ref(),
    ) {
        (Some(public_key), Some(journal))
            if journal.local_validator_id() == selected.binding.local_validator_id
                && journal.local_signing_public_key() == public_key =>
        {
            replay_bft_signing_journal_with_effects_v2(
                journal,
                schedule.active_set(),
                schedule,
                selected.binding.timeouts,
            )
            .map_err(|reason| error(format!("restore protected BFT V2 machine: {reason}")))
        }
        (None, None) => Ok((
            BftConsensusMachineV2::new(
                schedule.clone(),
                selected.binding.local_validator_id.clone(),
                protected_now,
                selected.binding.timeouts,
            )
            .map_err(error)?,
            Vec::new(),
        )),
        _ => Err(error(
            "BFT V2 active identity and protected journal disagree",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn maybe_install_transition(
    selected: &SelectedNodeBftConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    reference: &BftValidatorTransitionRefV2,
    confirmation: &BftCommitCertificateV2,
    result: &NodeBftExecutionCommitResultV2,
    machine: &BftConsensusMachineV2,
    lease: &mut dyn BftSignerLeaseV2,
    checkpoint: &mut oasis7_distfs::protected_bft_v2::BftSignerCheckpointV2,
) -> Result<(), NodeError> {
    let Some(public_key) = provisioned_active_key(selected, schedule) else {
        return Ok(());
    };
    if schedule.height() < reference.activation_height
        || schedule.last_activation_height() != reference.activation_height
        || schedule.active_set().set_hash() != reference.new_validator_set_hash
        || reference.scheduled_height.checked_add(1) != Some(confirmation.proposal.block.height)
        || confirmation.proposal.block.transition_ref.as_ref() != Some(reference)
        || !checkpoint.state.history.contains(confirmation)
        || result.height.checked_add(1) != Some(schedule.height())
        || checkpoint.state.history.last().is_none_or(|tip| {
            tip.proposal.block.height != result.height
                || tip.proposal.block.block_id().ok().as_deref()
                    != Some(result.bft_commit_ref.as_str())
        })
    {
        return Err(error(
            "BFT V2 transition install context is not the confirmed current activation and verified tip",
        ));
    }
    if machine.schedule() != schedule || machine.height() != schedule.height() {
        return Err(error(
            "BFT V2 transition install machine was not replayed at the confirmed schedule",
        ));
    }
    let journal = checkpoint
        .state
        .journal
        .as_ref()
        .ok_or_else(|| error("newly active BFT V2 validator has no durable signing journal"))?;
    replay_bft_signing_journal_v2(
        journal,
        schedule.active_set(),
        schedule,
        selected.binding.timeouts,
    )
    .map_err(|reason| {
        error(format!(
            "replay BFT V2 journal before transition installation: {reason}"
        ))
    })?;
    let request_id = transition_install_request_id(selected, reference)?;
    if let Some(receipt) = checkpoint
        .state
        .install_receipts
        .iter()
        .find(|receipt| receipt.body.request_id == request_id)
    {
        validate_install_receipt(
            receipt,
            selected,
            schedule,
            reference,
            result,
            checkpoint,
            &request_id,
        )?;
        return Ok(());
    }
    let expected_revision = checkpoint
        .state
        .pending_install
        .as_ref()
        .filter(|intent| intent.body.request_id == request_id)
        .map_or(checkpoint.state.revision, |intent| {
            intent.body.expected_revision
        });
    let expected_digest = checkpoint
        .state
        .pending_install
        .as_ref()
        .filter(|intent| intent.body.request_id == request_id)
        .map_or_else(
            || checkpoint.state.state_digest.clone(),
            |intent| intent.body.expected_state_digest.clone(),
        );
    let preparation = lease
        .prepare_transition_install(
            expected_revision,
            &expected_digest,
            request_id.clone(),
            reference.scheduled_height,
            selected.binding.operator_instance_id.clone(),
        )
        .map_err(|err| error(format!("prepare BFT V2 transition install: {err}")))?;
    match preparation {
        BftTransitionInstallPreparationV2::Completed {
            receipt,
            checkpoint: updated,
        } => {
            if updated.state.journal != checkpoint.state.journal
                || updated.state.history != checkpoint.state.history
                || !updated.state.install_receipts.contains(&receipt)
            {
                return Err(error(
                    "BFT V2 completed install changed the restored journal/history or omitted its receipt",
                ));
            }
            validate_install_receipt(
                &receipt,
                selected,
                schedule,
                reference,
                result,
                &updated,
                &request_id,
            )?;
            *checkpoint = updated;
        }
        BftTransitionInstallPreparationV2::Pending {
            intent,
            checkpoint: updated,
        } => {
            if updated.state.journal != checkpoint.state.journal
                || updated.state.history != checkpoint.state.history
            {
                return Err(error(
                    "BFT V2 install preparation changed the restored journal/history",
                ));
            }
            let wal_install_digest = protected_journal_install_digest(schedule, journal)?;
            let evidence = BftLocalInstallEvidenceV2 {
                schema_version: 2,
                runtime_instance_id: selected.binding.operator_instance_id.clone(),
                installed_head_height: result.height,
                installed_head_commit_ref: result.bft_commit_ref.clone(),
                installed_state_root: result.state_root.clone(),
                installed_manifest_hash: result.post_manifest_hash.clone(),
                installed_active_set_hash: schedule.active_set().set_hash().into(),
                installed_authority_epoch: schedule.authority_epoch(),
                local_validator_id: selected.binding.local_validator_id.clone(),
                local_signing_public_key: public_key,
                wal_install_digest: wal_install_digest.clone(),
                driver_install_digest: result.driver_install_digest.clone(),
            };
            let installed = lease
                .complete_transition_install(intent, evidence)
                .map_err(|err| error(format!("complete BFT V2 transition install: {err}")))?;
            if installed.receipt.body.wal_install_digest != wal_install_digest
                || installed.checkpoint.state.journal != updated.state.journal
                || installed.checkpoint.state.history != updated.state.history
            {
                return Err(error(
                    "BFT V2 completed install differs from the restored journal/history",
                ));
            }
            validate_install_receipt(
                &installed.receipt,
                selected,
                schedule,
                reference,
                result,
                &installed.checkpoint,
                &request_id,
            )?;
            *checkpoint = installed.checkpoint;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_install_receipt(
    receipt: &oasis7_distfs::protected_bft_v2::BftTransitionInstallReceiptV2,
    selected: &SelectedNodeBftConfigV2,
    schedule: &BftHeightProposerScheduleV2,
    reference: &BftValidatorTransitionRefV2,
    result: &NodeBftExecutionCommitResultV2,
    checkpoint: &oasis7_distfs::protected_bft_v2::BftSignerCheckpointV2,
    request_id: &str,
) -> Result<(), NodeError> {
    let body = &receipt.body;
    if body.request_id != request_id
        || body.source_height != reference.scheduled_height
        || body.activation_height != reference.activation_height
        || body.new_validator_set_hash != reference.new_validator_set_hash
        || reference.scheduled_height.checked_add(1) != Some(body.confirmation_height)
        || checkpoint
            .state
            .history
            .iter()
            .find(|certificate| certificate.proposal.block.height == body.confirmation_height)
            .is_none_or(|certificate| {
                certificate.proposal.block.block_id().ok().as_deref()
                    != Some(body.confirmation_block_id.as_str())
                    || certificate.proposal.block.transition_ref.as_ref() != Some(reference)
            })
        || body.installed_head_height < body.confirmation_height
        || body.installed_head_height > result.height
        || checkpoint
            .state
            .history
            .iter()
            .find(|certificate| certificate.proposal.block.height == body.installed_head_height)
            .is_none_or(|certificate| {
                let block = &certificate.proposal.block;
                block.block_id().ok().as_deref() != Some(body.installed_head_commit_ref.as_str())
                    || block.state_root != body.installed_state_root
                    || block.post_manifest_hash != body.installed_manifest_hash
            })
        || (body.installed_head_height == result.height
            && (body.installed_head_commit_ref != result.bft_commit_ref
                || body.installed_state_root != result.state_root
                || body.installed_manifest_hash != result.post_manifest_hash
                || body.driver_install_digest != result.driver_install_digest))
        || body.active_validator_set_hash != schedule.active_set().set_hash()
        || body.old_validator_set_hash != reference.old_validator_set_hash
        || body.transition_block_id != reference.source_block_id
        || body.transition_digest != reference.transition_digest
        || body.runtime_instance_id != selected.binding.operator_instance_id
        || body.local_validator_id != selected.binding.local_validator_id
        || Some(body.local_signing_public_key.clone()) != provisioned_active_key(selected, schedule)
        || body.authority_epoch != schedule.authority_epoch()
    {
        return Err(error(
            "BFT V2 transition install receipt differs from the verified activation history",
        ));
    }
    Ok(())
}

fn transition_install_request_id(
    selected: &SelectedNodeBftConfigV2,
    reference: &BftValidatorTransitionRefV2,
) -> Result<String, NodeError> {
    let binding = selected
        .binding
        .digest()
        .map_err(|err| error(format!("BFT V2 binding digest: {err}")))?;
    let input = format!(
        "oasis7.bft.v2.install-request|{}|{}|{}|{}",
        binding, reference.scheduled_height, reference.source_block_id, reference.transition_digest,
    );
    Ok(crate::blake3_hex(input.as_bytes()))
}

fn protected_journal_install_digest(
    schedule: &BftHeightProposerScheduleV2,
    journal: &BftSigningJournalV2,
) -> Result<String, NodeError> {
    if journal.height() != schedule.height()
        || journal.schedule_digest() != schedule.digest().map_err(error)?
        || journal.active_validator_set_hash() != schedule.active_set().set_hash()
    {
        return Err(error(
            "BFT V2 journal install digest schedule does not match the installed machine",
        ));
    }
    let encoded = serde_cbor::to_vec(&(schedule, journal))
        .map_err(|err| error(format!("encode BFT V2 journal install digest: {err}")))?;
    let mut preimage = b"oasis7.bft.v2.protected-journal-install.v1|".to_vec();
    preimage.extend_from_slice(&encoded);
    Ok(crate::blake3_hex(&preimage))
}
