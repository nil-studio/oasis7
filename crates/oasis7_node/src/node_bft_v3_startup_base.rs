//! Static preflight followed by authenticated private base reconstruction.
use super::*;
pub(super) struct PreparedBase {
    pub(super) lease: Box<dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3>,
    pub(super) schedule: BftHeaderHeightScheduleV3,
    pub(super) history: Vec<FormalBftCertifiedHeaderV3>,
    pub(super) pending: Vec<crate::NodeConsensusAction>,
    pub(super) recovery: Vec<crate::NodeCommittedActionBatch>,
}
pub(super) fn prepare(
    runtime: &NodeRuntime,
    selected: &SelectedNodeBftConfigV3,
    hook: &Hook,
) -> Result<PreparedBase, NodeError> {
    // Static identity/codec checks happen before acquisition. Parent-dependent
    // admission uses the authenticated active base after private reconstruction.
    {
        let pending = runtime
            .pending_consensus_actions
            .lock()
            .map_err(|_| error("wire-3 pending queue poisoned"))?;
        let mut execution = lock_hook(hook)?;
        for action in pending.iter() {
            super::super::validate_single_action(action)?;
            action
                .validate()
                .map_err(|_| error("wire-3 queued action rejected"))?;
            crate::node_runtime_core::validate_consensus_action_payload_auth(
                &runtime.config,
                &action.payload_cbor,
            )?;
            execution
                .validate_bft_v3_static_action_payload(&action.payload_cbor)
                .map_err(|_| error("wire-3 queued payload is unsupported"))?;
        }
    }
    let original_pending = runtime
        .pending_consensus_actions
        .lock()
        .map_err(|_| error("wire-3 pending queue poisoned"))?
        .clone();
    let cached_recovery = runtime
        .bft_v3_recovery_batches
        .lock()
        .map_err(|_| error("wire-3 recovery completion cache poisoned"))?
        .clone();
    let mut lease = selected
        .provider
        .acquire(&selected.binding)
        .map_err(|_| error("wire-3 protected provider acquire rejected"))?;
    lock_hook(hook)?
        .prepare_bft_v3_execution(
            &selected.binding,
            lease.as_mut(),
            std::time::Instant::now() + std::time::Duration::from_secs(120),
        )
        .map_err(|reason| {
            error(format!(
                "wire-3 authenticated private startup rejected: {reason}"
            ))
        })?;
    let authenticated = lease
        .checkpoint()
        .map_err(|_| error("wire-3 startup checkpoint unavailable"))?;
    if authenticated
        .state
        .successor_window
        .as_ref()
        .is_some_and(|window| {
            window.reservation.is_some()
                || window.requests.iter().any(|record| {
                    matches!(record.request.status,
            oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Prepared
            | oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::InstalledPending)
                })
        })
    {
        return Err(error(
            "wire-3 unresolved original successor transaction cannot select a writable base",
        ));
    }
    let base_schedule = if authenticated.state.active_window.is_some() {
        lease
            .verified_active_window()
            .map_err(|_| error("wire-3 authenticated active base unavailable"))?
            .ok_or_else(|| error("wire-3 active checkpoint lacks verified base"))?
            .base_schedule()
            .clone()
    } else {
        selected.binding.initial_schedule().clone()
    };
    validate_schedule(&base_schedule, selected)?;
    if selected.membership_enabled {
        lock_hook(hook)?
            .enable_bft_v3_membership_execution()
            .map_err(|_| error("wire-3 driver membership opt-in rejected"))?;
    }
    lock_hook(hook)?
        .configure_bft_v3_execution(&selected.binding)
        .map_err(|_| error("wire-3 durable configuration rejected"))?;
    let mut driver_history = Vec::new();
    loop {
        let after = driver_history.last().map_or(
            base_schedule
                .height()
                .checked_sub(1)
                .ok_or_else(|| error("wire-3 base height underflow"))?,
            |c: &FormalBftCertifiedHeaderV3| c.execution_commitment.height,
        );
        let page = lock_hook(hook)?
            .load_bft_v3_commit_history_page(&selected.binding, after, 16)
            .map_err(|_| error("wire-3 driver history unavailable"))?;
        if page.is_empty() {
            break;
        }
        if page.len() > 16 || driver_history.len() + page.len() > 64 {
            return Err(error("wire-3 driver history capacity"));
        }
        for c in page {
            let expected = base_schedule
                .height()
                .checked_add(driver_history.len() as u64)
                .ok_or_else(|| error("wire-3 height overflow"))?;
            if c.execution_commitment.height != expected {
                return Err(error("wire-3 driver history gap"));
            }
            driver_history.push(c);
        }
    }
    Ok(PreparedBase {
        lease,
        schedule: base_schedule,
        history: driver_history,
        pending: original_pending,
        recovery: cached_recovery,
    })
}

pub(super) fn validate_runtime(
    runtime: &NodeRuntime,
    selected: &SelectedNodeBftConfigV3,
) -> Result<(), NodeError> {
    let b = &selected.binding;
    b.validate().map_err(|_| error("wire-3 binding rejected"))?;
    validate_schedule(b.initial_schedule(), selected)?;
    if let Some(network) = &selected.network {
        if network.transport.context() != &BftPeerContextV3::from_handoff(b.handoff()) {
            return Err(error("wire-3 peer transport differs from qualified H"));
        }
        if network.transport.peers().is_empty() || network.transport.peers().len() > 256 {
            return Err(error("wire-3 peer routes missing or oversized"));
        }
        network.exchange.validate_binding(b).map_err(error)?;
    } else if b.initial_schedule().active_set().validators().len() != 1 {
        return Err(error(
            "wire-3 multiple validators require explicit public and private peer planes",
        ));
    }
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
    if !selected.membership_enabled
        && !b.provisioned_signing_keys.iter().any(|key| {
            b.initial_schedule()
                .active_set()
                .validator(&b.local_validator_id)
                .is_some_and(|v| v.public_key_hex == key.public_key_hex)
        })
    {
        return Err(error(
            "wire-3 local validator has no provisioned protected key",
        ));
    }
    Ok(())
}
