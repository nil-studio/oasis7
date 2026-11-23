//! Initial operation admission and fenced pre-target restart.
use super::super::{
    LocalInstance, local_io,
    model::{CONFIG_FILE, NodeConfig},
    process::ProcessOwnership,
};
#[cfg(test)]
use super::FormalFullSyncStatusV1;
use super::operation_failure::{LocalBlocker, OperationFailure};
use super::{
    FormalFullSyncConfigV1, FormalFullSyncConfigV2, FormalFullSyncCutV1, FormalFullSyncSession,
    LocalCasStore, attempt::FullSyncTargetV1, cut_from_anchor, history_chain, nondecreasing_cut,
    now_unix_ms, operation_ledger, operation_snapshot::read_snapshot, providers, shared_exchange,
    validate_grant_for_node,
};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(in super::super) struct InitialBudgetedSession {
    // Keep the native ownership guard alive for this entire session.
    _owner: Arc<ProcessOwnership>,
    _directory: PathBuf,
    session: FormalFullSyncSession,
    exchange: Arc<shared_exchange::SharedOperationExchange>,
}

/// A supervisor retains this consumer and its World for the native incarnation.
/// Every later poll inspects the one original ledger; errors cannot allocate a
/// fresh operation. Only an acknowledged local installation enables rollover.
pub(in super::super) struct BudgetedDriver {
    node: NodeConfig,
    instance: LocalInstance,
    owner: Arc<ProcessOwnership>,
    stop: Arc<AtomicBool>,
    pub(in super::super) observation: Arc<Mutex<super::operation_status::LiveObservation>>,
    restored: Option<super::RestoredFormalFullSyncWorldV1>,
    advance: bool,
    blocked: bool,
}

impl BudgetedDriver {
    #[cfg(test)]
    pub(super) fn retained_world(&self) -> Option<&oasis7_runtime::World> {
        self.restored.as_ref().map(|restored| &restored.world)
    }
    pub(in super::super) fn new(
        node: NodeConfig,
        instance: LocalInstance,
        owner: Arc<ProcessOwnership>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        owner.require_directory(&node.identity.data_dir)?;
        let mut base =
            super::operation_status::observe(&node).ok_or("missing operation configuration")?;
        base["incarnation"] = instance.incarnation.clone().into();
        base["generation"] = instance.generation.into();
        base["config_digest"] = instance.config_digest.clone().into();
        base["state"] = "starting".into();
        base["blocker"] = Value::Null;
        Ok(Self {
            node,
            instance,
            owner,
            stop,
            observation: Arc::new(Mutex::new(super::operation_status::LiveObservation {
                base,
                ..Default::default()
            })),
            restored: None,
            advance: false,
            blocked: false,
        })
    }

    pub(in super::super) fn poll_interval(&self) -> Duration {
        Duration::from_millis(
            self.node
                .identity
                .chain
                .budgeted_formal_full_sync()
                .unwrap()
                .poll_interval_ms,
        )
    }

    fn publish(
        &self,
        session: &FormalFullSyncSession,
        clock: Option<(String, u64, Instant, Instant)>,
    ) -> Result<(), String> {
        let mut live = self.observation.lock().map_err(|e| e.to_string())?;
        live.base = serde_json::to_value(session.status()).map_err(|e| e.to_string())?;
        live.clock = clock;
        live.history = session.history.observation()?;
        Ok(())
    }

    fn fail(&mut self, error: OperationFailure, permanent: bool) {
        self.blocked |= permanent;
        let mut live = self
            .observation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.clock = None;
        live.base["ready"] = false.into();
        live.base["world_loaded"] = false.into();
        live.base["state"] = "blocked".into();
        live.base["error"] = error.message.into();
        live.base["blocker"] = error.blocker.as_str().into();
    }

    pub(in super::super) fn poll(&mut self) {
        if self.blocked || self.stop.load(Ordering::SeqCst) {
            return;
        }
        let path = self
            .node
            .identity
            .data_dir
            .join("state/full-sync/operation.json");
        let snapshot = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            _ => match read_snapshot(&path) {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    self.fail(error.into(), true);
                    return;
                }
            },
        };
        if snapshot.as_ref().is_some_and(|s| s.publication.is_some()) && !self.advance {
            let started = Instant::now();
            {
                let mut live = self
                    .observation
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                live.reopen_elapsed_ms = None;
                live.reopen_result = None;
            }
            let result = InitialBudgetedSession::recover_installation(
                &self.node,
                &self.instance,
                Arc::clone(&self.owner),
                Arc::clone(&self.stop),
                &self.observation,
            );
            match result {
                Ok((session, restored)) => {
                    self.restored = Some(restored);
                    self.advance = true;
                    if let Err(error) = self.publish(&session, None) {
                        self.fail(error.into(), true);
                    }
                }
                Err(error) => {
                    let may_retry = error.retry_installation;
                    self.fail(error, !may_retry);
                }
            }
            let mut live = self
                .observation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // A repair is a separately admitted attempt. Its HTTP duration is
            // not time spent reopening the original local ledger/history.
            if live.reopen_result.as_deref() != Some("repair_attempt_admitted") {
                live.reopen_elapsed_ms =
                    Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
                live.reopen_result = Some(
                    if self.advance {
                        "original_installation_read_back"
                    } else {
                        "blocked"
                    }
                    .into(),
                );
            }
            return;
        }
        let preparation = if self.advance {
            InitialBudgetedSession::prepare_next(
                &self.node,
                &self.instance,
                Arc::clone(&self.owner),
                self.restored
                    .as_ref()
                    .expect("completed cycle retains its World"),
            )
        } else if let Some(snapshot) = &snapshot {
            InitialBudgetedSession::resume_unpublished(
                &self.node,
                &self.instance,
                Arc::clone(&self.owner),
                snapshot.verified_target_json.is_some(),
            )
        } else {
            InitialBudgetedSession::prepare(&self.node, &self.instance, Arc::clone(&self.owner))
        };
        let mut worker = match preparation {
            Ok(worker) => worker,
            Err(error) => {
                self.fail(error, true);
                return;
            }
        };
        self.advance = false;
        let result = (|| {
            worker.exchange.set_cancellation(Arc::clone(&self.stop))?;
            worker.session.cancellation = Some(Arc::clone(&self.stop));
            worker.session.begin_poll();
            let state = worker.exchange.snapshot();
            let (started, deadline) = worker.exchange.clock();
            self.publish(
                &worker.session,
                Some((
                    state.identity.operation_id,
                    state.attempts_used,
                    started,
                    deadline,
                )),
            )?;
            let restored = worker.session.sync_latest()?;
            let receipt = worker.session.confirm_installed(&restored, deadline)?;
            worker.exchange.complete_installation(&receipt)?;
            // The actual World moves into the same consumer before readiness
            // is observable. A cold process must restore it again from originals.
            self.restored = Some(restored);
            worker.session.set_world_loaded(true);
            self.advance = true;
            if snapshot.is_none() {
                self.observation
                    .lock()
                    .map_err(|e| e.to_string())?
                    .checkpoint_observed = Some(started);
            }
            self.publish(&worker.session, None)
        })();
        if let Err(error) = result {
            let _ = self.publish(&worker.session, None);
            // The synchronous poll has returned and dropping it ends the
            // transport/writer lease. The next poll may admit a bounded attempt
            // on this exact ledger, or perform one bounded local recovery.
            self.fail(worker.exchange.local_failure(error), false);
        }
    }
}

struct OperationInputs<'a> {
    configured: &'a FormalFullSyncConfigV2,
    read: FormalFullSyncConfigV1,
    private_key_hex: String,
    owner_binding: String,
    node_identity: Value,
    transport: shared_exchange::PreparedTransport,
}

impl<'a> OperationInputs<'a> {
    fn validate(
        node: &'a NodeConfig,
        instance: &LocalInstance,
        owner: &ProcessOwnership,
    ) -> Result<Self, String> {
        let data = &node.identity.data_dir;
        owner.require_directory(data)?;
        if NodeConfig::load(&data.join(CONFIG_FILE))? != *node {
            return Err("operation requires the exact persisted local node configuration".into());
        }
        node.verify_identity()?;
        node.identity.chain.verify()?;
        let configured = node
            .identity
            .chain
            .budgeted_formal_full_sync()
            .ok_or("budgeted operation requires its exact V4/V2 node binding")?;
        if instance.config_digest != local_io::digest(node)? {
            return Err("operation requires the admitted instance configuration digest".into());
        }
        let grant = configured
            .grant
            .as_ref()
            .ok_or("initial budgeted operation requires an independent issuer grant")?;
        let (keypair, _) =
            super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                &data.join("keys"),
            )?;
        let read = configured.read_material_config();
        validate_grant_for_node(
            &read,
            grant,
            &keypair.public_key_hex,
            now_unix_ms().ok_or("system clock before Unix epoch")?,
        )?;
        let endpoints: Vec<_> = read
            .providers()
            .into_iter()
            .map(|provider| provider.endpoint)
            .collect();
        let transport = shared_exchange::PreparedTransport::prepare_builtin_trust(&endpoints)?;
        Ok(Self {
            configured,
            read,
            private_key_hex: keypair.private_key_hex,
            owner_binding: owner.binding_digest()?,
            node_identity: serde_json::to_value(&node.identity)
                .map_err(|error| error.to_string())?,
            transport,
        })
    }

    fn attach_binding(&self, mut request: Value) -> Value {
        request["node_identity"] = self.node_identity.clone();
        request["native_owner_binding"] = self.owner_binding.clone().into();
        request
    }

    /// Replay and compare the exact original minimum without invoking the
    /// session constructor, which may stabilize and prune local CAS roots.
    fn verify_recovery_history(
        &self,
        root: &Path,
        original: &Value,
        selected: Option<&[u8]>,
    ) -> Result<Option<FullSyncTargetV1>, String> {
        self.verify_recovery_history_mode(root, original, selected, false)
    }

    fn verify_recovery_history_mode(
        &self,
        root: &Path,
        original: &Value,
        selected: Option<&[u8]>,
        repair: bool,
    ) -> Result<Option<FullSyncTargetV1>, String> {
        if original["node_identity"] != self.node_identity
            || original["native_owner_binding"].as_str() != Some(self.owner_binding.as_str())
            || original["transport_binding"].as_str() != Some(self.transport.binding_digest())
            || original["config"]
                != serde_json::to_value(&self.read).map_err(|error| error.to_string())?
            || original["transfer_policy"]
                != serde_json::to_value(&self.configured.transfer_policy)
                    .map_err(|error| error.to_string())?
        {
            return Err("operation recovery differs from original configuration and owner".into());
        }
        let providers = self.read.providers();
        let client = providers::build_read_client(
            &self.read,
            providers.first().ok_or("missing recovery provider")?,
            &self.private_key_hex,
            Duration::from_millis(self.read.request_timeout_ms),
        )?;
        let minimum: FormalFullSyncCutV1 =
            serde_json::from_value(original["initial_minimum"].clone())
                .map_err(|error| error.to_string())?;
        let minimum_anchor = if repair {
            history_chain::PersistedFormalReadHistoryV1::verify_original_for_repair(
                &self.read,
                &client,
                Arc::new(LocalCasStore::new(root)),
                root,
                &minimum,
            )?
        } else {
            history_chain::PersistedFormalReadHistoryV1::verify_read_only(
                &self.read,
                &client,
                Arc::new(LocalCasStore::new(root)),
                root,
                Some(&minimum),
                true,
            )?
            .1
            .ok_or("original minimum is missing from authenticated history")?
        };
        let grant = self.read.grant.as_ref().ok_or("missing recovery grant")?;
        let expected = self.attach_binding(serde_json::json!({
            "api_version": "oasis7.full-sync-original-operation-request/v1",
            "config": &self.read,
            "transfer_policy": &self.configured.transfer_policy,
            "node_public_key": &grant.body.node_public_key,
            "initial_minimum": &minimum,
            "initial_minimum_record": minimum_anchor.head().record(),
            "transport_binding": self.transport.binding_digest(),
        }));
        if expected != *original {
            return Err("original request differs from the authenticated minimum and owner".into());
        }
        let anchor = client
            .formal_current_anchor()
            .map_err(|error| error.to_string())?;
        let cut = cut_from_anchor(&anchor)?;
        let Some(selected) = selected else {
            if !nondecreasing_cut(&minimum, &cut) {
                return Err("original minimum is absent before pre-target recovery".into());
            }
            return Ok(None);
        };
        let saved: Value = serde_json::from_slice(selected).map_err(|e| e.to_string())?;
        let selected_cut: FormalFullSyncCutV1 =
            serde_json::from_value(saved[0].clone()).map_err(|e| e.to_string())?;
        let selected_client = providers::build_read_client(
            &self.read,
            providers.first().ok_or("missing recovery provider")?,
            &self.private_key_hex,
            Duration::from_millis(self.read.request_timeout_ms),
        )?;
        let anchor = if repair {
            history_chain::PersistedFormalReadHistoryV1::verify_original_for_repair(
                &self.read,
                &selected_client,
                Arc::new(LocalCasStore::new(root)),
                root,
                &selected_cut,
            )?
        } else {
            history_chain::PersistedFormalReadHistoryV1::verify_read_only(
                &self.read,
                &selected_client,
                Arc::new(LocalCasStore::new(root)),
                root,
                Some(&selected_cut),
                true,
            )?
            .1
            .ok_or("original selected target missing from authenticated history")?
        };
        let expected = serde_json::to_vec(&(&selected_cut, anchor.head().record()))
            .map_err(|e| e.to_string())?;
        if selected != expected
            || !nondecreasing_cut(&minimum, &selected_cut)
            || !nondecreasing_cut(&selected_cut, &cut)
        {
            return Err("selected target differs from authenticated durable history".into());
        }
        let cut = selected_cut;
        grant.verify(
            &self.read.formal_read_trust.initial_trust.issuer_public_key,
            &cut.world,
            &cut.chain_id,
            &grant.body.node_public_key,
            &self.read.expected_role,
            cut.authority_epoch,
            now_unix_ms().ok_or("system clock before Unix epoch")?,
        )?;
        Ok(Some(FullSyncTargetV1 { anchor, cut }))
    }
}

impl InitialBudgetedSession {
    #[cfg(test)]
    pub(super) fn interrupt_publication_at(&mut self, phase: &'static str) {
        self.session.publication_fault = Some(Arc::new(move |observed| {
            assert_ne!(observed, phase, "injected worker termination at {phase}");
        }));
    }
    fn recover_installation(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
        stop: Arc<AtomicBool>,
        observation: &Arc<Mutex<super::operation_status::LiveObservation>>,
    ) -> Result<(FormalFullSyncSession, super::RestoredFormalFullSyncWorldV1), OperationFailure>
    {
        let started = Instant::now();
        let inputs = OperationInputs::validate(node, instance, &owner)?;
        let deadline = started
            .checked_add(Duration::from_millis(
                inputs.configured.transfer_policy.reopen_timeout_ms,
            ))
            .ok_or("operation reopen deadline overflow")?;
        let root = node.identity.data_dir.join("state/full-sync");
        let path = root.join("operation.json");
        let snapshot = read_snapshot(&path)?;
        if snapshot.identity.binding_digest != inputs.owner_binding
            || snapshot.publication_journal_version != Some(1)
            || snapshot.publication.is_none()
        {
            return Err(
                "operation_recovery_required: original publication continuity missing".into(),
            );
        }
        let original: Value = serde_json::from_slice(
            snapshot
                .original_request_json
                .as_deref()
                .ok_or("original request missing")?,
        )
        .map_err(|e| e.to_string())?;
        let cas = LocalCasStore::new(&root);
        let mut missing = false;
        for hash in &snapshot
            .installation
            .as_ref()
            .ok_or("original installation inputs missing")?
            .retained_cas_hashes
        {
            missing |= super::verified_local_object(&cas, hash)?.is_none();
        }
        let target = inputs.verify_recovery_history_mode(
            &root,
            &original,
            snapshot.verified_target_json.as_deref(),
            missing,
        )?;
        if Instant::now() >= deadline || stop.load(Ordering::SeqCst) {
            return Err("operation_reopen_expired".into());
        }
        let mut ledger = operation_ledger::Ledger::open(
            &path,
            &snapshot.identity,
            &inputs.configured.transfer_policy,
            inputs.configured.max_total_sync_bytes,
        )?;
        if ledger.snapshot() != &snapshot {
            return Err("original installation changed during recovery".into());
        }
        let mut session = FormalFullSyncSession::open(
            inputs.read.clone(),
            inputs.private_key_hex.clone(),
            &root,
            instance,
            false,
        )?;
        session.recovery_target = target.clone();
        session.cancellation = Some(Arc::clone(&stop));
        if missing {
            use oasis7_game_client::formal_sync_exchange::FullSyncExchange;
            let repair = super::installation::VerifiedRepair::prepare(
                target.ok_or("original repair target missing")?,
                original.clone(),
                &snapshot,
                &cas,
            )?;
            ledger.bind_attempt_owner(&owner, &node.identity.data_dir, instance)?;
            let attempt = ledger
                .resume_installation_repair(&owner, &node.identity.data_dir, &repair, deadline)
                .map_err(|error| ledger.failure(error))?;
            let (attempt_started, attempt_deadline) = attempt.clock();
            let exchange = Arc::new(
                inputs
                    .transport
                    .admit(ledger, attempt)?
                    .with_owner(Arc::clone(&owner), node.identity.data_dir.clone())?,
            );
            exchange.set_cancellation(Arc::clone(&stop))?;
            session.recovery_minimum = Some(
                serde_json::from_value(original["initial_minimum"].clone())
                    .map_err(|e| e.to_string())?,
            );
            session
                .bind_shared_exchange(exchange.clone(), &original)
                .map_err(|error| exchange.local_failure(error))?;
            let admitted = exchange.snapshot();
            {
                let mut live = observation.lock().map_err(|e| e.to_string())?;
                live.clock = Some((
                    admitted.identity.operation_id.clone(),
                    admitted.attempts_used,
                    attempt_started,
                    attempt_deadline,
                ));
                live.reopen_elapsed_ms =
                    Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
                live.reopen_result = Some("repair_attempt_admitted".into());
                live.base["state"] = "repairing_original_installation".into();
                live.base["ready"] = false.into();
            }
            let result = session.recover_original_installation(&admitted, attempt_deadline);
            let finished = exchange.finish_attempt().map_err(|e| e.to_string());
            let outcome = (|| {
                let (restored, receipt) = result?;
                finished?;
                exchange.complete_installation(&receipt)?;
                session.set_world_loaded(true);
                Ok((session, restored))
            })();
            return outcome.map_err(|error: String| exchange.local_failure(error).repair_attempt());
        }
        ledger.bind_cancellation(Arc::clone(&stop))?;
        let (restored, receipt) = session.recover_original_installation(&snapshot, deadline)?;
        owner.require_directory(&node.identity.data_dir)?;
        if stop.load(Ordering::SeqCst) {
            return Err("operation_recovery_required: worker cancelled".into());
        }
        ledger
            .complete_installation(&owner, &node.identity.data_dir, &receipt, deadline)
            .map_err(|error| ledger.failure(error))?;
        session.set_world_loaded(true);
        Ok((session, restored))
    }

    fn prepare_next(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
        previous: &super::RestoredFormalFullSyncWorldV1,
    ) -> Result<Self, OperationFailure> {
        let started = Instant::now();
        let inputs = OperationInputs::validate(node, instance, &owner)?;
        let deadline = started
            .checked_add(Duration::from_millis(
                inputs.configured.transfer_policy.reopen_timeout_ms,
            ))
            .ok_or("operation reopen deadline overflow")?;
        let directory = node.identity.data_dir.clone();
        let root = directory.join("state/full-sync");
        let path = root.join("operation.json");
        let snapshot = read_snapshot(&path)?;
        let original: Value = serde_json::from_slice(
            snapshot
                .original_request_json
                .as_deref()
                .ok_or("original request missing")?,
        )
        .map_err(|e| e.to_string())?;
        inputs.verify_recovery_history(
            &root,
            &original,
            snapshot.verified_target_json.as_deref(),
        )?;
        let session = FormalFullSyncSession::open(
            inputs.read.clone(),
            inputs.private_key_hex.clone(),
            &root,
            instance,
            false,
        )?;
        let receipt = session.confirm_installed(previous, deadline)?;
        let request = inputs.attach_binding(
            session
                .prepare_operation_request(&inputs.configured.transfer_policy, &inputs.transport)?,
        );
        let mut ledger = operation_ledger::Ledger::open(
            &path,
            &snapshot.identity,
            &inputs.configured.transfer_policy,
            inputs.configured.max_total_sync_bytes,
        )?;
        if ledger.snapshot() != &snapshot {
            return Err("previous operation changed before rollover".into());
        }
        ledger
            .begin_next_operation(&owner, &directory, &receipt, &request, deadline)
            .map_err(|error| ledger.failure(error))?;
        ledger.bind_attempt_owner(&owner, &directory, instance)?;
        let attempt = ledger
            .begin_attempt()
            .map_err(|error| ledger.failure(error))?;
        Self::attach(
            owner,
            directory,
            inputs.transport,
            session,
            ledger,
            attempt,
            &request,
        )
    }

    pub(super) fn prepare(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, OperationFailure> {
        let directory = node.identity.data_dir.clone();
        let inputs = OperationInputs::validate(node, instance, &owner)?;
        if instance.generation != 1 {
            return Err("operation_recovery_required: a later incarnation cannot initialize a missing original ledger".into());
        }
        let state = directory.join("state");
        let sync_root = state.join("full-sync");
        local_io::reject_symlinks(&sync_root)?;
        match fs::symlink_metadata(&sync_root) {
            Ok(_) => {
                return Err("operation_recovery_required: original full-sync state exists".into());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string().into()),
        }
        owner.require_directory(&directory)?;
        local_io::private_dir(&state)?;
        let session = FormalFullSyncSession::new(
            inputs.read.clone(),
            inputs.private_key_hex.clone(),
            &sync_root,
            instance,
        )?;
        let policy = &inputs.configured.transfer_policy;
        let request =
            inputs.attach_binding(session.prepare_operation_request(policy, &inputs.transport)?);
        let identity = operation_ledger::Identity {
            operation_id: local_io::random_id()?,
            request_digest: local_io::digest(&request)?,
            policy_digest: local_io::digest(policy)?,
            binding_digest: inputs.owner_binding.clone(),
        };
        owner.require_directory(&directory)?;
        let mut ledger = operation_ledger::Ledger::create_bound(
            &sync_root.join("operation.json"),
            identity,
            policy.clone(),
            inputs.configured.max_total_sync_bytes,
            &request,
            &owner,
            &directory,
        )?;
        ledger.bind_attempt_owner(&owner, &directory, instance)?;
        let attempt = ledger
            .begin_attempt()
            .map_err(|error| ledger.failure(error))?;
        Self::attach(
            owner,
            directory,
            inputs.transport,
            session,
            ledger,
            attempt,
            &request,
        )
    }

    #[cfg(test)]
    pub(super) fn resume_pre_target(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, OperationFailure> {
        Self::resume_unpublished(node, instance, owner, false)
    }

    #[cfg(test)]
    pub(super) fn resume_selected_target(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, OperationFailure> {
        Self::resume_unpublished(node, instance, owner, true)
    }

    fn resume_unpublished(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
        expect_selected: bool,
    ) -> Result<Self, OperationFailure> {
        let started = Instant::now();
        let directory = node.identity.data_dir.clone();
        let inputs = OperationInputs::validate(node, instance, &owner)?;
        let policy = &inputs.configured.transfer_policy;
        let deadline = started
            .checked_add(Duration::from_millis(policy.reopen_timeout_ms))
            .ok_or("operation reopen deadline overflow")?;
        let sync_root = directory.join("state/full-sync");
        let path = sync_root.join("operation.json");
        let snapshot = read_snapshot(&path)?;
        if snapshot.identity.binding_digest != inputs.owner_binding
            || snapshot.verified_target_json.is_some() != expect_selected
            || snapshot.publication_journal_version != Some(1)
            || snapshot.publication.is_some()
        {
            return Err(
                "operation_recovery_required: unpublished continuity is unavailable".into(),
            );
        }
        if snapshot.attempts_used >= policy.max_attempts {
            return Err(OperationFailure::new(
                LocalBlocker::BudgetExhausted,
                "operation_budget_exhausted: attempts",
            ));
        }
        let original: Value = serde_json::from_slice(
            snapshot
                .original_request_json
                .as_deref()
                .ok_or("operation recovery requires the full original request")?,
        )
        .map_err(|error| error.to_string())?;
        let selected = snapshot.verified_target_json.as_deref();
        let recovery_target = inputs.verify_recovery_history(&sync_root, &original, selected)?;
        owner.require_directory(&directory)?;
        if Instant::now() >= deadline {
            return Err("operation_reopen_expired".into());
        }
        let mut ledger = operation_ledger::Ledger::open(
            &path,
            &snapshot.identity,
            policy,
            inputs.configured.max_total_sync_bytes,
        )?;
        if ledger.snapshot() != &snapshot {
            return Err("operation ledger changed during reopen".into());
        }
        let mut session = FormalFullSyncSession::open(
            inputs.read.clone(),
            inputs.private_key_hex.clone(),
            &sync_root,
            instance,
            !expect_selected,
        )?;
        if let Some(target) = &recovery_target {
            let current = session
                .client
                .formal_current_anchor()
                .map_err(|error| error.to_string())?;
            if !nondecreasing_cut(&target.cut, &cut_from_anchor(&current)?) {
                return Err("selected history changed during session open".into());
            }
        }
        session.recovery_minimum = Some(
            serde_json::from_value(original["initial_minimum"].clone())
                .map_err(|e| e.to_string())?,
        );
        session.recovery_target = recovery_target;
        ledger.bind_attempt_owner(&owner, &directory, instance)?;
        let attempt = match selected {
            Some(target) => {
                ledger.resume_unpublished(&owner, &directory, &original, Some(target), deadline)
            }
            None => ledger.resume_before_target(&owner, &directory, &original, deadline),
        }
        .map_err(|error| ledger.failure(error))?;
        Self::attach(
            owner,
            directory,
            inputs.transport,
            session,
            ledger,
            attempt,
            &original,
        )
    }

    fn attach(
        owner: Arc<ProcessOwnership>,
        directory: PathBuf,
        transport: shared_exchange::PreparedTransport,
        mut session: FormalFullSyncSession,
        ledger: operation_ledger::Ledger,
        attempt: operation_ledger::AttemptPermit,
        request: &Value,
    ) -> Result<Self, OperationFailure> {
        let exchange = Arc::new(
            transport
                .admit(ledger, attempt)?
                .with_owner(Arc::clone(&owner), directory.clone())?,
        );
        session
            .bind_shared_exchange(exchange.clone(), request)
            .map_err(|error| exchange.local_failure(error))?;
        Ok(Self {
            _owner: owner,
            _directory: directory,
            session,
            exchange,
        })
    }

    #[cfg(test)]
    pub(super) fn sample_selected_target(
        &mut self,
        select: bool,
    ) -> Result<FormalFullSyncCutV1, String> {
        use oasis7_game_client::formal_sync_exchange::FullSyncExchange;

        self._owner.require_directory(&self._directory)?;
        let view = self.session.read_best_effort_current_view()?;
        let anchor = super::verified_anchor(&view, &self.session.config)?;
        let cut = cut_from_anchor(&anchor)?;
        if select {
            let target = serde_json::to_vec(&(&cut, anchor.head().record()))
                .map_err(|error| error.to_string())?;
            self.exchange
                .bind_verified_target(&target)
                .map_err(|error| error.to_string())?;
        }
        Ok(cut)
    }

    #[cfg(test)]
    pub(super) fn run_once(
        mut self,
    ) -> Result<(FormalFullSyncStatusV1, operation_ledger::Snapshot), String> {
        self._owner.require_directory(&self._directory)?;
        self.session.begin_poll();
        let restored = self.session.sync_latest()?;
        self._owner.require_directory(&self._directory)?;
        drop(restored);
        self.session.set_world_loaded(false);
        let status = self.session.status().clone();
        Ok((status, self.exchange.snapshot()))
    }
}
