use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use oasis7_consensus::tendermint_v1::{
    BftHeightProposerScheduleV1, VerifiedBftCommitCertificateV1, verify_commit_certificate_token_v1,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1, BftCommitCertificateV1,
};

use super::{local_signing_key, worker};
use crate::node_bft_wal::BftWalStore;
use crate::runtime_util::lock_state;
use crate::{GossipEndpoint, NodeBftExecutionConfigV1, NodeError, NodeExecutionHook, NodeRuntime};

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
        .map_err(|_| error("BFT execution hook lock poisoned; stop"))
}

pub(super) fn start(runtime: &mut NodeRuntime) -> Result<(), NodeError> {
    if runtime.running.load(Ordering::SeqCst) || runtime.worker.is_some() {
        return Err(NodeError::AlreadyRunning {
            node_id: runtime.config.node_id.clone(),
        });
    }
    let selected = runtime
        .bft_config
        .clone()
        .ok_or_else(|| error("missing explicit BFT config"))?;
    let bft = selected.consensus;
    let protected = selected.protected;
    super::validate_voter_role(&runtime.config)?;
    let set = bft.validate(&runtime.config.world_id).map_err(error)?;
    runtime.validate_bft_pending_admission()?;
    worker::Worker::validate_configuration(&set, bft.authority_epoch)?;
    if runtime.config.gossip.as_ref().is_some_and(|gossip| {
        gossip.peers.len() > oasis7_consensus::tendermint_v1::BFT_MAX_VALIDATORS_V1
    }) {
        return Err(error("BFT engineering static peer capacity exceeded"));
    }
    if set.validators().len() > 1 && runtime.config.gossip.is_none() {
        return Err(error(
            "multi-validator BFT requires an explicit gossip transport",
        ));
    }
    if runtime.config.node_id != bft.local_validator_id {
        return Err(error(
            "BFT local validator identity must equal NodeRuntime node identity",
        ));
    }
    if runtime.local_execution_bootstrap.is_some()
        || runtime.local_execution_genesis_bootstrap.is_some()
        || runtime.replication_network_consensus_enabled
    {
        return Err(error(
            "BFT cannot share a legacy/formal local bootstrap or PoS consensus subscription",
        ));
    }
    let replication = runtime
        .config
        .replication
        .as_ref()
        .ok_or_else(|| error("BFT requires durable replication/signing configuration"))?;
    let key = if protected.is_some() {
        if replication.consensus_signer()?.is_some() {
            return Err(error(
                "protected BFT mode rejects a local consensus signing key",
            ));
        }
        None
    } else {
        Some(local_signing_key(replication, &bft)?)
    };
    let hook = runtime
        .execution_hook
        .clone()
        .ok_or_else(|| error("BFT requires a durable execution hook"))?;
    let config = NodeBftExecutionConfigV1 {
        scope: bft.scope.clone(),
        authority_epoch: bft.authority_epoch,
        validators: bft.validators.clone(),
        validator_set_hash: set.set_hash().into(),
    };
    let head = lock_hook(&hook)?
        .configure_bft_execution(&config)
        .map_err(error)?;
    let genesis = BftHeightProposerScheduleV1::genesis(&set, bft.authority_epoch).map_err(error)?;
    let (history, schedule) = load_history(&hook, &config, &set, &genesis)?;
    let last = history.last();
    match (&head, last) {
        (None, None) => {}
        (Some(head), Some(cert))
            if head.height == cert.proposal.block.height
                && head.block_id == cert.proposal.block.block_id().map_err(error)? => {}
        _ => {
            return Err(error(
                "BFT durable head differs from complete verified history",
            ));
        }
    }
    // Acquire the signer lifecycle lock before recovering any local signing state.
    let directory = replication.root_dir.join("tendermint-v1");
    let mut wal = if let Some(protected) = protected {
        BftWalStore::open_protected_for_recovery(
            &directory,
            &genesis,
            &protected.binding,
            !history.is_empty(),
            protected.provider.as_ref(),
            &history,
        )?
    } else {
        let key = key
            .as_ref()
            .ok_or_else(|| error("missing local BFT signer"))?;
        BftWalStore::open_for_recovery(
            &directory,
            &genesis,
            &bft.local_validator_id,
            &hex::encode(key.verifying_key().to_bytes()),
            !history.is_empty(),
        )?
    };
    let mut replay_schedule = genesis;
    let mut tip_token: Option<VerifiedBftCommitCertificateV1> = None;
    for cert in history {
        let token =
            verify_commit_certificate_token_v1(&set, &replay_schedule, &cert).map_err(error)?;
        let result = lock_hook(&hook)?
            .commit_bft_certificate(token.clone())
            .map_err(error)?;
        worker::validate_execution_result(&token, &result)?;
        replay_schedule = token.next_height_schedule().clone();
        tip_token = Some(token);
    }
    if replay_schedule != schedule {
        return Err(error("BFT history changed during recovery"));
    }
    if wal.state().height != schedule.height() {
        let token = tip_token
            .as_ref()
            .ok_or_else(|| error("BFT WAL cannot be reconciled without committed history"))?;
        // Only the exact pending QC may bridge the driver-committed/WAL-not-advanced seam.
        wal.complete_history_commit(token)?;
    }
    wal.validate_schedule(&schedule)?;
    if schedule.height() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 as u64 {
        return Err(error(
            "BFT engineering history capacity exhausted; no further signing",
        ));
    }
    let prepared = worker::Worker::restore(worker::WorkerSetup {
        config: runtime.config.clone(),
        bft,
        set,
        schedule,
        key,
        hook,
        wal,
        pending: Arc::clone(&runtime.pending_consensus_actions),
        pending_bytes: Arc::clone(&runtime.pending_consensus_action_queue_bytes),
        state: Arc::clone(&runtime.state),
        tip: tip_token,
    })?;
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
    runtime.running.store(true, Ordering::SeqCst);
    let worker_gossip = gossip.clone();
    let spawn = runtime
        .runtime_worker_spawner(
            thread::Builder::new().name(format!("aw-bft-{}", runtime.config.node_id)),
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

fn load_history(
    hook: &Hook,
    config: &NodeBftExecutionConfigV1,
    set: &oasis7_consensus::tendermint_v1::BftValidatorSetV1,
    genesis: &BftHeightProposerScheduleV1,
) -> Result<(Vec<BftCommitCertificateV1>, BftHeightProposerScheduleV1), NodeError> {
    let mut history = Vec::new();
    let mut schedule = genesis.clone();
    let mut bytes = 0usize;
    loop {
        let after = schedule
            .height()
            .checked_sub(1)
            .ok_or_else(|| error("BFT history height underflow"))?;
        let page = lock_hook(hook)?
            .load_bft_commit_history_page(config, after, BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1)
            .map_err(error)?;
        if page.is_empty() {
            break;
        }
        if page.len() > BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1 {
            return Err(error("BFT history page exceeds entry bound"));
        }
        for cert in page {
            bytes = bytes
                .checked_add(
                    serde_cbor::to_vec(&cert)
                        .map_err(|e| error(e.to_string()))?
                        .len(),
                )
                .ok_or_else(|| error("BFT history byte overflow"))?;
            if bytes > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1
                || history.len() >= BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1
            {
                return Err(error(
                    "BFT history capacity exhausted; preserve evidence and stop",
                ));
            }
            let token = verify_commit_certificate_token_v1(set, &schedule, &cert).map_err(error)?;
            schedule = token.next_height_schedule().clone();
            history.push(cert);
        }
    }
    Ok((history, schedule))
}
