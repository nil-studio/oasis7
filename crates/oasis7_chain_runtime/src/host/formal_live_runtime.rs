//! Production constructor and lifecycle owner for the formal single-authority
//! Node profile. It consumes existing configuration/keys and restores only
//! through endpoint-authenticated native evidence or an exact approved initial
//! publication.
use super::{
    execution_bridge::{
        formal_node_hook::FormalNodeHook, formal_package, formal_writer::FormalWriter,
    },
    formal_live_config::ValidatedFormalLiveConfigV1,
    formal_live_service::{
        FormalHttpServer, FormalLiveHistoryResolver, FormalLiveService, FormalLiveState,
        PublishedReadState,
    },
};
#[cfg(test)]
use crate::formal_live_config::FormalLiveConfigV1;
use ed25519_dalek::SigningKey;
use oasis7_client_api::world_service::WorldIdentity;
use oasis7_distfs::controlled_authority::{
    formal::{
        RecoveryTailV1, SignedNormalDurableSetTransitionV1, VerifiedAuthorityV1,
        VerifiedFormalReadAnchorV1, VerifiedRecoveryBootstrapV1,
        business_index::{
            BusinessIndexNodeReader, BusinessIndexNodeWriteV1, BusinessIndexPathV1,
            BusinessIndexRootV1,
        },
        coordinator::FormalDurabilityEndpoint,
        transport::RemoteFormalEndpoint,
    },
    replicated_protocol::EndpointRole,
};
use oasis7_node::{NodeConfig, NodeNetworkPolicy, NodeRole, NodeRuntime};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[path = "formal_live_open_timing.rs"]
mod open_timing;
#[path = "formal_live_runtime_quiesce.rs"]
mod quiesce;
use open_timing::OpenTiming;

pub(crate) const BUSINESS_NODE_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const BUSINESS_NODE_CACHE_ENTRIES: usize = 16_384;

#[cfg(unix)]
#[path = "formal_live_runtime_owner.rs"]
mod owner;
#[cfg(unix)]
pub(crate) use owner::{
    DurableSetControlRequestV1, control_durable_set_transition,
    control_prepare_durable_set_transition,
};
#[cfg(unix)]
pub(crate) use owner::{FormalRuntimeInstanceV1, FormalRuntimeOwnerSnapshotV1};

#[cfg(test)]
#[path = "formal_live_runtime_tests.rs"]
pub(crate) mod tests;

/// Opened, but not yet running, formal Node. Construction performs all trust,
/// policy, endpoint and bootstrap checks before starting an ingress listener.
pub(crate) struct FormalLiveRuntime {
    _identity_lease: Arc<super::node_keypair_config::RuntimeIdentityLease>,
    validated: ValidatedFormalLiveConfigV1,
    active_authority: VerifiedAuthorityV1,
    world: WorldIdentity,
    node_signer: SigningKey,
    runtime: Arc<Mutex<NodeRuntime>>,
    published: Arc<PublishedReadState>,
    history: Arc<dyn FormalLiveHistoryResolver>,
    state: FormalLiveState,
    admission_gate: Arc<Mutex<()>>,
    durable_set_arm:
        Arc<super::execution_bridge::formal_durable_set_arm::FormalDurableSetArmControlV1>,
    server: Option<FormalHttpServer>,
    stop_worker: Option<PendingNodeStop>,
    #[cfg(test)]
    stop_entry_signal: Option<std::sync::mpsc::SyncSender<std::time::Instant>>,
    effect_delivery: Option<super::formal_live_effect_delivery::FormalEffectDeliveryRuntime>,
    started: bool,
}

struct PendingNodeStop {
    result: std::sync::mpsc::Receiver<Result<(), String>>,
    worker: std::thread::JoinHandle<Result<(), String>>,
}

impl PendingNodeStop {
    fn spawn(
        runtime: Arc<Mutex<NodeRuntime>>,
        state: FormalLiveState,
    ) -> Result<Self, std::io::Error> {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("oasis7-formal-stop".into())
            .spawn(move || {
                let result = match runtime.lock() {
                    Ok(mut runtime) => runtime
                        .stop()
                        .map_err(|error| format!("formal NodeRuntime stop: {error:?}")),
                    Err(_) => Err("formal Node runtime lock poisoned".into()),
                };
                if result.is_ok() {
                    state
                        .node_running
                        .store(false, std::sync::atomic::Ordering::Release);
                } else if let Ok(mut last_error) = state.last_error.lock() {
                    *last_error = result.as_ref().err().cloned();
                }
                let _ = send.send(result.clone());
                result
            })?;
        Ok(Self {
            result: receive,
            worker,
        })
    }
}

pub(super) fn startup_governance_history_anchor(
    writer_anchor: &VerifiedFormalReadAnchorV1,
    recovery: Option<&VerifiedRecoveryBootstrapV1>,
    trust: &oasis7_distfs::controlled_authority::formal::InitialActivationTrustV1,
    initial: &oasis7_distfs::controlled_authority::formal::SignedInitialActivationV1,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let Some(recovery) = recovery else {
        return Ok(writer_anchor.clone());
    };

    let recovery_cut = recovery.cut();
    let writer_position = writer_anchor.head().record().position;
    let recovery_position = recovery_cut.record().position;
    if writer_position < recovery_position {
        return Err("HistoryUnavailable: restored writer precedes its recovery cut".into());
    }
    if writer_position > recovery_position {
        // A completed recovery may already have advanced this writer. Leave
        // that qualified descendant on the ordinary exact-replay path.
        return Ok(writer_anchor.clone());
    }

    let successor_context = recovery
        .recovery()
        .successor()
        .context_digest()
        .map_err(|error| error.to_string())?;
    let writer_context = writer_anchor
        .authority()
        .context_digest()
        .map_err(|error| error.to_string())?;
    if writer_context != successor_context {
        return Err(
            "HistoryUnavailable: restored writer authority differs from recovery successor".into(),
        );
    }

    if writer_anchor.head().record() != recovery_cut.record()
        || writer_anchor.head().head() != recovery_cut.head()
        || writer_anchor.head().finalizing_authority_id() != recovery_cut.finalizing_authority_id()
    {
        return Err("HistoryUnavailable: restored writer differs from exact recovery cut".into());
    }
    let expected_pending = recovery
        .external_anchor()
        .with_pending_recovery(recovery.recovery())
        .map_err(|error| error.to_string())?;
    if writer_anchor.checkpoint() != expected_pending.checkpoint()
        || writer_context
            != expected_pending
                .authority()
                .context_digest()
                .map_err(|error| error.to_string())?
    {
        return Err(
            "HistoryUnavailable: restored pending writer differs from its verified recovery permission"
                .into(),
        );
    }

    if recovery.recovery().body().tail == RecoveryTailV1::Empty {
        // An empty recovery qualifies its successor at the same cut; replay
        // that exact successor normally.
        return Ok(writer_anchor.clone());
    }

    // A non-empty tail is still pending and therefore cannot be used as a
    // read target. The entire retained chain shares this sealed cut; its
    // oldest schema-1 permission identifies the cursor before any retained
    // recovery context was installed.
    let mut first_evidence = recovery.permission_evidence();
    while let Some(prior) = first_evidence.prior_permission.as_deref() {
        first_evidence = prior;
    }
    let first = first_evidence
        .verify(trust, initial)
        .map_err(|error| format!("HistoryUnavailable: recovery chain root: {error}"))?;
    if first.cut().record() != recovery_cut.record()
        || first.cut().head() != recovery_cut.head()
        || first.cut().finalizing_authority_id() != recovery_cut.finalizing_authority_id()
    {
        return Err(
            "HistoryUnavailable: retained recovery chain differs from exact pending cut".into(),
        );
    }
    Ok(first.external_anchor().clone())
}

#[path = "formal_live_business_reader.rs"]
mod business_reader;
#[cfg(test)]
use business_reader::{
    BoundedBusinessNodeCache, seed_verified_business_writes, validate_business_node_digest,
    verify_dual_business_node, verify_dual_business_path,
};
pub(crate) use business_reader::{RemoteBusinessIndexReader, business_reader_for};

impl FormalLiveRuntime {
    /// Load and validate the operator profile once, then construct both the
    /// native history resolver and writer around one shared verified CAS
    /// reader. This prevents resolver and writer endpoints from observing
    /// different config-file contents across a replacement race.
    #[cfg(test)]
    pub(crate) fn open_with_native_history(config_path: &Path) -> Result<Self, String> {
        Self::open_from_validated_with_native_history(FormalLiveConfigV1::load(config_path)?)
    }

    pub(crate) fn open_from_validated_with_native_history(
        validated: ValidatedFormalLiveConfigV1,
    ) -> Result<Self, String> {
        let identity_lease = acquire_runtime_identity(&validated)?;
        let timing = OpenTiming::new();
        let _open = timing.stage("open_total");
        let initial = timing.stage("history_resolver_and_startup_anchor");
        let business_reader = business_reader_for(&validated)?;
        let (history, startup) = super::formal_live_history::from_validated_for_runtime(
            &validated,
            business_reader.clone(),
        )?;
        business_reader.bind_verified_startup(&validated, &startup)?;
        drop(initial);
        Self::open_validated_with_startup(
            validated,
            history,
            business_reader,
            Some(startup),
            &timing,
            identity_lease,
        )
    }

    fn open_validated_with_startup(
        validated: ValidatedFormalLiveConfigV1,
        history: Arc<dyn FormalLiveHistoryResolver>,
        business_reader: Arc<RemoteBusinessIndexReader>,
        startup: Option<super::formal_live_history::FormalLiveStartupAnchorV1>,
        timing: &OpenTiming,
        identity_lease: Arc<super::node_keypair_config::RuntimeIdentityLease>,
    ) -> Result<Self, String> {
        validate_loaded_execution_policy(&validated)?;
        // Seed only content-addressed bytes from the exact configured initial
        // closure. Native restore still verifies the protected current state,
        // inclusion, proof and typed material beneath the external pin.
        if let Some(initial) = &validated.approved_initial {
            business_reader.seed_verified_writes(
                super::execution_bridge::formal_writer::initial_business_nodes(&initial.candidate)?,
            )?;
        }
        let world = WorldIdentity {
            world_id: validated.anchor.head().record().world.world_id.clone(),
            genesis_digest: validated
                .anchor
                .head()
                .record()
                .world
                .genesis_digest
                .clone(),
        };
        world.validate().map_err(|error| error.to_string())?;
        let node_signer = existing_node_signer(&validated)?;
        let node_id = validated.config.node_id.clone();
        let primary_address = validated.config.primary;
        let replica_address = validated.config.replica;
        let timeout = validated.timeout;
        let writer_key = validated.writer_key.clone();
        let startup_anchor = startup
            .as_ref()
            .map_or(&validated.anchor, |startup| &startup.anchor);
        let authority = startup
            .as_ref()
            .and_then(|startup| startup.activation.as_ref())
            .map_or_else(
                || validated.active_authority().clone(),
                |activation| activation.successor().clone(),
            );
        let minimum = startup_anchor.head().head().clone();

        let primary = RemoteFormalEndpoint::new(
            primary_address,
            timeout,
            authority.clone(),
            EndpointRole::Primary,
            writer_key.clone(),
            minimum.clone(),
        )
        .map_err(|error| format!("formal primary endpoint: {error}"))?;
        let replica = RemoteFormalEndpoint::new(
            replica_address,
            timeout,
            authority.clone(),
            EndpointRole::Replica,
            writer_key.clone(),
            minimum.clone(),
        )
        .map_err(|error| format!("formal replica endpoint: {error}"))?;

        let prewarm = timing.stage("protected_head_hint_prewarm");
        let prewarm_receipt = prewarm_from_matching_native_heads(
            &validated,
            startup_anchor,
            &authority,
            &*history,
            &*business_reader,
        )?;
        timing.flag("hint_prewarm_completed", prewarm_receipt.is_some());
        drop(prewarm);

        let writer_root = validated.config.writer_directory.clone();
        let release_policy = validated.release_policy.clone();
        // Approved initial material does not reset an existing writer to C0.
        // Every open starts from both protected native heads beneath the same
        // external origin, including the first already-qualified activation.
        let writer = {
            let restore = timing.stage("dual_native_restore");
            // The request carries the original trusted authority. A signed
            // raw head or endpoint checkpoint is never used as a new pin.
            let dual = if let Some(activation) = startup
                .as_ref()
                .and_then(|startup| startup.activation.as_ref())
            {
                if validated.recovery.is_some() {
                    return Err(
                        "formal healthy durable-set startup cannot combine with recovery bootstrap"
                            .into(),
                    );
                }
                super::formal_live_reconcile::restore_dual_native_with_durable_set(
                    primary,
                    replica,
                    activation.predecessor(),
                    activation,
                    &*business_reader,
                )?
            } else {
                super::formal_live_reconcile::restore_dual_native(
                    primary,
                    replica,
                    validated
                        .recovery
                        .as_ref()
                        .map_or(&authority, |p| p.original_authority()),
                    validated.recovery.as_ref().map(|p| p.recovery()),
                    validated.recovery.as_ref().map(|p| p.cut()),
                    &*business_reader,
                )?
            };
            drop(restore);
            let _writer = timing.stage("writer_open_and_verified_material");
            let primary_restore = dual.primary;
            let replica_restore = dual.replica;
            if let Some(initial) = validated.approved_initial.as_ref()
                && primary_restore.1.commit().record().position == 1
                && (primary_restore.1.candidate() != &initial.candidate
                    || primary_restore.1.proof() != &initial.proof)
            {
                return Err(
                    "native initial publication differs from exact approved closure".into(),
                );
            }
            if startup
                .as_ref()
                .and_then(|startup| startup.activation.as_ref())
                .is_some()
            {
                FormalWriter::open_native(
                    writer_root,
                    writer_key,
                    release_policy,
                    startup_anchor.clone(),
                    &primary_restore.1,
                    &replica_restore.1,
                    writer_endpoint(primary_restore.0, EndpointRole::Primary),
                    writer_endpoint(replica_restore.0, EndpointRole::Replica),
                    business_reader.clone(),
                )
            } else if let Some(permission) = validated.recovery.as_ref() {
                FormalWriter::open_native_recovery(
                    writer_root,
                    writer_key,
                    release_policy,
                    permission.external_anchor().clone(),
                    permission,
                    &primary_restore.1,
                    &replica_restore.1,
                    writer_endpoint(primary_restore.0, EndpointRole::Primary),
                    writer_endpoint(replica_restore.0, EndpointRole::Replica),
                    business_reader.clone(),
                )
            } else {
                FormalWriter::open_native(
                    writer_root,
                    writer_key,
                    release_policy,
                    validated.anchor.clone(),
                    &primary_restore.1,
                    &replica_restore.1,
                    writer_endpoint(primary_restore.0, EndpointRole::Primary),
                    writer_endpoint(replica_restore.0, EndpointRole::Replica),
                    business_reader.clone(),
                )
            }
            .map_err(|error| format!("open restored formal native writer: {error}"))?
        };
        let attach = timing.stage("writer_business_reader");
        let writer = writer
            .with_business_reader(business_reader.clone())
            .map_err(|error| format!("open formal profile-2 business reader: {error}"))?;
        drop(attach);
        let active_authority = writer.anchor().authority().clone();
        let prewarm = timing.stage("exact_writer_prewarm");
        let reused = history
            .prewarm_qualified_business_history_after_hint(
                writer.anchor(),
                business_reader.as_ref(),
                prewarm_receipt,
            )
            .map_err(|error| format!("prewarm verified formal business history: {error}"))?;
        timing.flag("exact_prewarm_reused", reused);
        drop(prewarm);
        let seed = timing.stage("seed_committed_business_nodes");
        let committed_business_nodes = writer
            .committed_business_nodes()
            .map_err(|error| format!("read exact qualified formal business nodes: {error}"))?;
        business_reader.seed_verified_writes(committed_business_nodes)?;
        drop(seed);
        let governance_anchor = startup_governance_history_anchor(
            writer.anchor(),
            validated.recovery.as_ref(),
            &validated.trust.initial_trust,
            &validated.trust.checkpoint.initial_certificate,
        )?;
        history
            .verify_governance_history(&governance_anchor)
            .map_err(|error| format!("verify restored formal governance history: {error}"))?;

        let _bootstrap = timing.stage("publication_and_node_bootstrap");
        let published = Arc::new(PublishedReadState::default());
        let (effect_delivery, sink): (
            _,
            Arc<dyn super::execution_bridge::formal_node_hook::FormalPublicationSink>,
        ) = if let Some(config) = &validated.config.effect_delivery {
            let (delivery, sink) =
                super::formal_live_effect_delivery::FormalEffectDeliveryRuntime::open(
                    config,
                    &validated.config.writer_directory,
                    node_signer.clone(),
                    published.clone(),
                    history.clone(),
                )?;
            (
                Some(delivery.with_identity_lease(Arc::clone(&identity_lease))),
                sink,
            )
        } else {
            (None, published.clone())
        };
        let admission_gate = Arc::new(Mutex::new(()));
        let durable_set_arm = Arc::new(
            super::execution_bridge::formal_durable_set_arm::FormalDurableSetArmControlV1::new(
                validated.config.writer_directory.clone(),
                false,
            )?,
        );
        let hook = FormalNodeHook::new(writer)?
            .with_identity_lease(Arc::clone(&identity_lease))
            .with_durable_set_arm(Arc::clone(&durable_set_arm), Arc::clone(&admission_gate))?
            .with_publication_sink(sink)?;
        let node_config = local_single_authority_node_config(&node_id, &world.world_id)?;
        let genesis = hook.genesis_bootstrap()?;
        let mut runtime = NodeRuntime::new(node_config);
        if let Some(genesis) = genesis {
            runtime = runtime.with_local_execution_genesis_bootstrap(genesis);
        } else {
            let bootstrap = hook.bootstrap()?;
            runtime = runtime.with_local_execution_bootstrap(bootstrap);
        }
        runtime = runtime.with_execution_hook(hook);

        Ok(Self {
            _identity_lease: identity_lease,
            validated,
            active_authority,
            world,
            node_signer,
            runtime: Arc::new(Mutex::new(runtime)),
            published,
            history,
            state: FormalLiveState::default(),
            admission_gate,
            durable_set_arm,
            server: None,
            stop_worker: None,
            #[cfg(test)]
            stop_entry_signal: None,
            effect_delivery,
            started: false,
        })
    }

    pub(crate) fn active_authority(&self) -> &VerifiedAuthorityV1 {
        &self.active_authority
    }

    /// Test observation of maintenance on the same stopped writer/endpoint owners.
    #[cfg(test)]
    pub(crate) fn advance_storage_maintenance_for_test(&mut self) -> Result<bool, String> {
        if self.started || self.stop_worker.is_some() {
            return Err("storage maintenance observation requires completed runtime stop".into());
        }
        self.runtime
            .lock()
            .map_err(|_| "Node runtime lock poisoned during storage maintenance observation")?
            .advance_stopped_storage_maintenance()
    }

    /// Arm the exact next healthy durable-set boundary for the currently
    /// running owner. This only persists old-writer-signed intent; it does not
    /// execute a boundary, seal endpoints, or activate a successor authority.
    pub(crate) fn arm_durable_set_transition(
        &self,
        operation_id: &str,
        transition: SignedNormalDurableSetTransitionV1,
    ) -> Result<super::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1, String>
    {
        let _admission = self
            .admission_gate
            .lock()
            .map_err(|_| "formal Node admission gate poisoned")?;
        if !self.started || !self.state.ready.load(std::sync::atomic::Ordering::Acquire) {
            return Err("formal runtime is not ready to admit a durable-set arm".into());
        }
        let cut = self.published.current()?;
        self.durable_set_arm
            .arm(operation_id, transition, &cut.anchor)
    }

    /// Select and sign the next boundary under the same gate as public submit.
    pub(crate) fn prepare_durable_set_transition(
        &self,
        operation_id: &str,
        mut replica: oasis7_distfs::controlled_authority::formal::EndpointSignerBindingV1,
        timeout: Duration,
    ) -> Result<SignedNormalDurableSetTransitionV1, String> {
        use ed25519_dalek::Signer;
        use oasis7_distfs::controlled_authority::formal::{
            NormalDurableSetTransitionBodyV1, normal_durable_set_transition_signing_bytes,
        };
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err("durable-set preparation timeout in (0,60s]".into());
        }
        let deadline =
            std::time::Instant::now() + timeout.saturating_sub(Duration::from_millis(100));
        {
            let _admission = quiesce::acquire_for_idempotency_read(&self.admission_gate, deadline)?;
            let cut = self.published.current()?;
            if let Some(existing) = self
                .durable_set_arm
                .existing_for_operation(operation_id, &cut.anchor)?
            {
                replica.instance.authority_epoch = existing.body.successor.epoch;
                if existing.body.successor.replica != replica {
                    return Err("Conflict: original durable-set replica target differs".into());
                }
                return Ok(existing);
            }
        }
        let _admission = quiesce::acquire_after_pending_clear(
            &self.admission_gate,
            || self.durable_set_arm.pending_open(),
            deadline,
        )?;
        let cut = self.published.current()?;
        if let Some(existing) = self
            .durable_set_arm
            .existing_for_operation(operation_id, &cut.anchor)?
        {
            replica.instance.authority_epoch = existing.body.successor.epoch;
            if existing.body.successor.replica != replica {
                return Err("Conflict: original durable-set replica target differs".into());
            }
            return Ok(existing);
        }
        if !self.started || !self.state.ready.load(std::sync::atomic::Ordering::Acquire) {
            return Err("formal runtime is not ready to prepare a durable-set arm".into());
        }
        let authority = cut.anchor.authority();
        let mut successor = authority.definition().clone();
        let boundary = cut
            .anchor
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("durable-set boundary overflow")?;
        successor.epoch = successor
            .epoch
            .checked_add(1)
            .ok_or("durable-set epoch overflow")?;
        successor.valid_from_position = boundary
            .checked_add(1)
            .ok_or("durable-set valid position overflow")?;
        successor.predecessor_authority_id =
            Some(authority.authority_id().map_err(|e| e.to_string())?);
        successor.primary.instance.authority_epoch = successor.epoch;
        replica.instance.authority_epoch = successor.epoch;
        successor.replica = replica;
        let mut body = NormalDurableSetTransitionBodyV1 {
            schema_version: 1,
            world: successor.world.clone(),
            predecessor_authority_id: authority.authority_id().map_err(|e| e.to_string())?,
            predecessor: cut.anchor.head().head(),
            boundary_position: boundary,
            operation_id: operation_id.to_owned(),
            successor,
            independent_closure_root: "0".repeat(64),
        };
        body.independent_closure_root = body.copy_contract_digest().map_err(|e| e.to_string())?;
        let transition = SignedNormalDurableSetTransitionV1 {
            writer_signature_hex: hex::encode(
                self.validated
                    .writer_key
                    .sign(
                        &normal_durable_set_transition_signing_bytes(&body)
                            .map_err(|e| e.to_string())?,
                    )
                    .to_bytes(),
            ),
            body,
        };
        quiesce::arm_before_deadline(deadline, || {
            self.durable_set_arm
                .arm(operation_id, transition.clone(), &cut.anchor)
        })?;
        Ok(transition)
    }

    /// Read the durable arm status against the exact currently published
    /// opaque anchor. Local arm bytes never count as a qualified commit or
    /// activation proof.
    pub(crate) fn query_durable_set_transition(
        &self,
        operation_id: &str,
        boundary_position: u64,
    ) -> Result<
        Option<super::execution_bridge::formal_durable_set_arm::DurableSetArmStatusV1>,
        String,
    > {
        let _admission = self
            .admission_gate
            .lock()
            .map_err(|_| "formal Node admission gate poisoned")?;
        let cut = self.published.current()?;
        self.durable_set_arm
            .query(operation_id, boundary_position, &cut.anchor)
    }

    pub(crate) fn start(&mut self) -> Result<(), String> {
        if self.started {
            return Err("formal live runtime is already started".into());
        }
        // Bind first so a listener failure cannot leave a running Node without
        // its required operator status surface.
        let service = Arc::new(FormalLiveService::new(
            self.validated.config.node_id.clone(),
            self.world.clone(),
            self.node_signer.clone(),
            Arc::clone(&self.runtime),
            Arc::clone(&self.published),
            self.validated.trust.clone(),
            Arc::clone(&self.history),
            self.state.clone(),
            Arc::clone(&self.admission_gate),
        ));
        let _admission = self
            .admission_gate
            .lock()
            .map_err(|_| "formal Node admission gate poisoned")?;
        let server = service.serve_loopback(self.validated.config.status_bind)?;
        if let Some(delivery) = &mut self.effect_delivery
            && let Err(error) = delivery.start(self.state.clone())
        {
            let mut server = server;
            let _ = server.stop();
            return Err(error);
        }
        let start_result = match self.runtime.lock() {
            Ok(mut runtime) => runtime
                .start()
                .map_err(|error| format!("formal NodeRuntime start: {error:?}")),
            Err(_) => Err("formal Node runtime lock poisoned".into()),
        };
        if let Err(error) = start_result {
            drop(_admission);
            let mut server = server;
            let _ = server.stop();
            return Err(error);
        }
        let tick_count = match self.runtime.lock() {
            Ok(runtime) => runtime.snapshot().tick_count,
            Err(_) => {
                drop(_admission);
                let mut server = server;
                let _ = server.stop();
                return Err("formal Node runtime lock poisoned after start".into());
            }
        };
        self.state
            .node_running
            .store(true, std::sync::atomic::Ordering::Release);
        self.state
            .tick_count
            .store(tick_count, std::sync::atomic::Ordering::Release);
        self.state
            .stopping
            .store(false, std::sync::atomic::Ordering::Release);
        if let Ok(mut last_error) = self.state.last_error.lock() {
            *last_error = None;
        }
        self.state
            .ready
            .store(true, std::sync::atomic::Ordering::Release);
        self.server = Some(server);
        self.started = true;
        Ok(())
    }

    /// Graceful stop budgets admission and Node waits by the supplied interval.
    /// A timeout keeps the owner alive and leaves status at
    /// `stopping`; callers can poll status and retry the same stop operation.
    /// Admission and worker waits share one deadline; OS scheduling can delay return.
    pub(crate) fn stop(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        #[cfg(test)]
        if let Some(signal) = self.stop_entry_signal.take() {
            let _ = signal.try_send(deadline);
        }
        if let Some(delivery) = &self.effect_delivery {
            delivery.request_stop();
        }
        // Close ingress before waiting for the already-admitted operation.
        self.state
            .ready
            .store(false, std::sync::atomic::Ordering::Release);
        self.state
            .stopping
            .store(true, std::sync::atomic::Ordering::Release);
        if self.admission_gate.is_poisoned() {
            return Err("formal Node admission gate poisoned".into());
        }
        // An idle owner still runs delivery/server cleanup, including stop(0).
        if self.started || self.stop_worker.is_some() {
            let Some(admission) = quiesce::acquire_for_stop(&self.admission_gate, deadline)? else {
                return Err("formal Node stop is still pending; runtime remains owned".into());
            };
            drop(admission);
        }
        // A guard obtained before expiry must never permit a late new worker.
        if self.started && self.stop_worker.is_none() && std::time::Instant::now() >= deadline {
            return Err("formal Node stop is still pending; runtime remains owned".into());
        }
        if self.started && self.stop_worker.is_none() {
            self.stop_worker = Some(
                match PendingNodeStop::spawn(Arc::clone(&self.runtime), self.state.clone()) {
                    Ok(pending) => pending,
                    Err(error) => {
                        self.state
                            .stopping
                            .store(false, std::sync::atomic::Ordering::Release);
                        if let Ok(mut last_error) = self.state.last_error.lock() {
                            *last_error = Some(format!("formal Node stop worker: {error}"));
                        }
                        return Err(format!("formal Node stop worker: {error}"));
                    }
                },
            );
        }
        if let Some(pending) = self.stop_worker.as_ref() {
            match pending
                .result
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            {
                Ok(result) => {
                    let pending = self.stop_worker.take().expect("stop worker exists");
                    let worker_result = pending
                        .worker
                        .join()
                        .map_err(|_| "formal Node stop worker panicked".to_owned())?;
                    if worker_result != result {
                        return Err("formal Node stop channel/worker result mismatch".into());
                    }
                    if let Err(error) = result {
                        if let Ok(runtime) = self.runtime.lock()
                            && runtime.snapshot().running
                        {
                            return Err(error);
                        }
                        if let Ok(mut last_error) = self.state.last_error.lock() {
                            *last_error = Some(error.clone());
                        }
                        self.state
                            .node_running
                            .store(false, std::sync::atomic::Ordering::Release);
                        self.state
                            .stopping
                            .store(false, std::sync::atomic::Ordering::Release);
                        self.started = false;
                        if let Some(mut server) = self.server.take() {
                            server.stop()?;
                        }
                        return Err(error);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    self.state
                        .stopping
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err("formal Node stop is still pending; runtime remains owned".into());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(pending) = self.stop_worker.take() {
                        let _ = pending.worker.join();
                    }
                    return Err("formal Node stop result channel closed".into());
                }
            }
        }
        if let Some(delivery) = &mut self.effect_delivery {
            delivery.stop(deadline.saturating_duration_since(std::time::Instant::now()))?;
        }
        if let Some(mut server) = self.server.take() {
            server.stop()?;
        }
        self.state
            .stopping
            .store(false, std::sync::atomic::Ordering::Release);
        self.started = false;
        Ok(())
    }
}

/// Separate probes keep the restore clients' original external minimum intact.
/// A protected head locates bytes; only canonical C0 replay authorizes seeding.
fn prewarm_from_matching_native_heads(
    validated: &ValidatedFormalLiveConfigV1,
    minimum_anchor: &oasis7_distfs::controlled_authority::formal::VerifiedFormalReadAnchorV1,
    authority: &oasis7_distfs::controlled_authority::formal::VerifiedAuthorityV1,
    history: &dyn FormalLiveHistoryResolver,
    reader: &dyn BusinessIndexNodeReader,
) -> Result<Option<super::formal_live_history::VerifiedBusinessPrewarmReceipt>, String> {
    let probe = |address, role| {
        let mut endpoint = RemoteFormalEndpoint::new(
            address,
            validated.timeout,
            authority.clone(),
            role,
            validated.writer_key.clone(),
            minimum_anchor.head().head().clone(),
        )
        .map_err(|error| format!("formal prewarm endpoint: {error}"))?;
        match endpoint.head() {
            Ok(head) => Ok(Some(head)),
            Err(oasis7_distfs::controlled_authority::formal::FormalError::Io(_)) => Ok(None),
            Err(error) => Err(format!("formal prewarm protected head: {error}")),
        }
    };
    let Some(primary) = probe(validated.config.primary, EndpointRole::Primary)? else {
        return Ok(None);
    };
    let Some(replica) = probe(validated.config.replica, EndpointRole::Replica)? else {
        return Ok(None);
    };
    if primary != replica {
        return Ok(None);
    }
    let hint = oasis7_client_api::world_service::FormalReadAnchorRefV1 {
        position: primary.position,
        record_id: primary.record_id,
        active_authority_id: authority
            .authority_id()
            .map_err(|error| error.to_string())?,
    };
    history.prewarm_qualified_business_history_hint(&hint, reader)
}

/// Production CLI entry. It accepts only the formal profile path; history
/// resolution is built from that same operator configuration by the native
/// storage adapter, with no unavailable or synthetic fallback.
pub(crate) fn run(config_path: &Path) -> Result<(), String> {
    owner::run_standalone(config_path)
}

#[cfg(unix)]
#[cfg(test)]
pub(crate) fn run_operation_owner_worker(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
) -> Result<(), String> {
    owner::run_operation_worker(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
    )
}

#[cfg(unix)]
pub(crate) fn query_operation_owner(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_incarnation: Option<&str>,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    owner::query_operation(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        expected_incarnation,
    )
}

#[cfg(unix)]
pub(crate) fn current_operation_owner(
    config_path: &Path,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    owner::current_operation(config_path)
}

#[cfg(unix)]
pub(crate) fn stop_operation_owner(
    config_path: &Path,
    instance: &FormalRuntimeInstanceV1,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    owner::stop_operation(config_path, instance, timeout)
}

#[cfg(unix)]
pub(crate) fn launch_operation_owner(
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    owner::launch_operation(
        config_path,
        operation_id,
        request_digest,
        expected_config_digest,
        timeout,
    )
}

fn validate_loaded_execution_policy(validated: &ValidatedFormalLiveConfigV1) -> Result<(), String> {
    let (_policy_material, executable_digest, policy_digest) =
        formal_package::execution_policy_material(&validated.release_policy)?;
    if executable_digest.trim().is_empty() {
        return Err("formal loaded executable digest is empty".into());
    }
    validated.require_execution_policy_digest(&policy_digest)
}

#[cfg(all(test, unix))]
fn writer_endpoint(
    endpoint: RemoteFormalEndpoint,
    role: EndpointRole,
) -> super::formal_live_crash_endpoint::CrashBoundaryFormalEndpoint {
    super::formal_live_crash_endpoint::CrashBoundaryFormalEndpoint::new(endpoint, role)
}
#[cfg(not(all(test, unix)))]
fn writer_endpoint(endpoint: RemoteFormalEndpoint, _: EndpointRole) -> RemoteFormalEndpoint {
    endpoint
}

fn acquire_runtime_identity(
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<Arc<super::node_keypair_config::RuntimeIdentityLease>, String> {
    use super::node_keypair_config::{RuntimeIdentityBindingV1, acquire_runtime_identity_lease};
    if validated.anchor.authority().definition().profile_version != 2 {
        return Err("runtime identity lease requires controlled profile v2".into());
    }
    // Check original paths before canonicalization so an alias cannot cause
    // public binding writes before a later key reread rejects the alias.
    super::controlled_live_config::require_private_owned(
        &validated.config.node_keypair_directory,
        true,
    )?;
    super::controlled_live_config::require_private_owned(&validated.config.writer_directory, true)?;
    super::controlled_live_config::require_private_owned(
        &validated
            .config
            .node_keypair_directory
            .join(super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE),
        false,
    )?;
    let key_path = std::fs::canonicalize(
        validated
            .config
            .node_keypair_directory
            .join(super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE),
    )
    .map_err(|error| format!("canonical node identity path: {error}"))?;
    let derived = super::derive_node_libp2p_identity_keypair_config(
        &validated.config.node_id,
        &validated.node_keypair,
    )?;
    let peer_id = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
        .map_err(|error| format!("derive formal node peer identity: {error:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let world = &validated.anchor.head().record().world;
    let binding = RuntimeIdentityBindingV1 {
        version: 1,
        key_path: key_path.clone(),
        data_directory: std::fs::canonicalize(&validated.config.writer_directory)
            .map_err(|error| format!("canonical formal writer directory: {error}"))?,
        node_id: validated.config.node_id.clone(),
        root_public_key: validated.node_keypair.public_key_hex.clone(),
        peer_id,
        world_id: world.world_id.clone(),
        chain_id: world.chain_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        profile: "controlled-v2".into(),
    };
    let lease = acquire_runtime_identity_lease(&key_path, &binding)?;
    let (current, _) = super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
        &validated.config.node_keypair_directory,
    )
    .map_err(|_| "formal node identity verification failed after lease acquisition")?;
    if current.public_key_hex != validated.node_keypair.public_key_hex
        || current.private_key_hex != validated.node_keypair.private_key_hex
    {
        return Err("formal node identity changed after configuration validation".into());
    }
    Ok(Arc::new(lease))
}

fn local_single_authority_node_config(node_id: &str, world_id: &str) -> Result<NodeConfig, String> {
    let mut config = NodeConfig::new(node_id, world_id, NodeRole::Sequencer)
        .map_err(|error| format!("formal local Node configuration: {error:?}"))?;
    config.network_policy = NodeNetworkPolicy::for_runtime_role(NodeRole::Sequencer);
    config.allow_local_proposals = true;
    config.auto_attest_all_validators = false;
    config.require_execution_on_commit = true;
    config.require_peer_execution_hashes = false;
    config.gossip = None;
    config.replication = None;
    config.feedback_p2p = None;
    if config.auto_attest_all_validators
        || !config.allow_local_proposals
        || config.network_policy != NodeNetworkPolicy::for_runtime_role(NodeRole::Sequencer)
        || config.gossip.is_some()
        || config.replication.is_some()
        || config.feedback_p2p.is_some()
    {
        return Err("formal runtime local-authority Node profile is not closed".into());
    }
    Ok(config)
}

fn existing_node_signer(validated: &ValidatedFormalLiveConfigV1) -> Result<SigningKey, String> {
    let bytes = hex::decode(&validated.node_keypair.private_key_hex)
        .map_err(|_| "existing Node private key encoding invalid")?;
    let secret: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "existing Node private key length invalid")?;
    let signer = SigningKey::from_bytes(&secret);
    if hex::encode(signer.verifying_key().to_bytes()) != validated.node_keypair.public_key_hex {
        return Err("existing Node signer keypair mismatch".into());
    }
    Ok(signer)
}

#[cfg(unix)]
pub(crate) fn launch_operation_owner_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    expected_trust: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    owner::launch_operation_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        Some(expected_trust),
        timeout,
    )
}
#[cfg(unix)]
pub(crate) fn query_operation_owner_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    incarnation: Option<&str>,
    expected_trust: &str,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    owner::query_operation_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        incarnation,
        expected_trust,
    )
}
#[cfg(unix)]
pub(crate) fn run_operation_owner_worker_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    expected_trust: Option<&str>,
) -> Result<(), String> {
    owner::run_operation_worker_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        expected_trust,
    )
}
