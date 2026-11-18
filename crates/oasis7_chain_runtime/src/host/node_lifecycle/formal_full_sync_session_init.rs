//! Session construction and immutable operation attachment.
use super::*;

impl FormalFullSyncSession {
    /// Construct from an already loaded, privately held node identity. The
    /// grant must bind that key; local process ownership alone grants nothing.
    pub(crate) fn new(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        cas_root: &Path,
        instance: &super::super::LocalInstance,
    ) -> Result<Self, String> {
        config.validate()?;
        let signed = authority::sign_read_request(
            "formal-full-sync-node-key-check",
            (),
            &node_private_key_hex,
        )?;
        if config
            .grant
            .as_ref()
            .is_some_and(|grant| signed.subject_public_key != grant.body.node_public_key)
        {
            return Err("full-sync node key differs from issuer grant".into());
        }
        super::super::local_io::private_dir(cas_root)?;
        let providers = config.providers();
        let active_provider = providers
            .first()
            .ok_or("full-sync has no configured provider")?;
        let timeout = Duration::from_millis(config.request_timeout_ms);
        let client =
            providers::build_read_client(&config, active_provider, &node_private_key_hex, timeout)?;
        let http = oasis7_game_client::formal_read_http_client(timeout)
            .map_err(|error| error.to_string())?;
        let node_public_key_hex = signed.subject_public_key;
        let cas = Arc::new(LocalCasStore::new(cas_root));
        let cas_integrity_poisoned = Arc::new(AtomicBool::new(false));
        let history = history_chain::PersistedFormalReadHistoryV1::open(
            &config,
            &client,
            Arc::clone(&cas),
            cas_root,
        )?;
        cas_lifecycle::stabilize_loaded_world_pins(&cas)?;
        cas_lifecycle::prune_unpinned(&cas)?;
        let status = FormalFullSyncStatusV1 {
            api_version: "oasis7.formal-full-sync-status/v1".into(),
            runtime_profile: PROFILE.into(),
            state: if config.grant.is_some() {
                "starting"
            } else {
                "blocked_missing_issuer_grant"
            }
            .into(),
            ready: false,
            can_propose: false,
            state_root_verified: false,
            world_loaded: false,
            sync_scope: "issuer_granted_full_state_read_only".into(),
            world_id: config.world_id.clone(),
            chain_id: config.chain_id.clone(),
            node_public_key: node_public_key_hex.clone(),
            incarnation: instance.incarnation.clone(),
            generation: instance.generation,
            config_digest: instance.config_digest.clone(),
            grant_epoch: config.grant.as_ref().map(|grant| grant.body.grant_epoch),
            last_verified_cut: None,
            loaded_cut: None,
            lag_positions: None,
            catch_up_pending: false,
            last_checked_unix_ms: None,
            error: None,
        };
        let mut session = Self {
            #[cfg(all(test, unix))]
            cas_root: cas_root.to_path_buf(),
            config,
            client,
            http,
            active_provider_index: 0,
            shared_exchange: None,
            recovery_target: None,
            node_private_key_hex,
            node_public_key_hex,
            cas,
            cas_integrity_poisoned,
            history,
            cas_publication_poisoned: false,
            status,
        };
        session.status.last_verified_cut = session.history.last_verified_cut().cloned();
        Ok(session)
    }

    /// Capture the original verified minimum, not a provider's advertised head.
    /// This is request preparation only, not owner admission or recovery.
    #[cfg(all(test, unix))]
    pub(super) fn prepare_operation_request(
        &self,
        policy: &operation_ledger::Policy,
        transport: &shared_exchange::PreparedTransport,
    ) -> Result<serde_json::Value, String> {
        self.config.validate()?;
        policy.validate()?;
        if self.shared_exchange.is_some()
            || self.history.is_poisoned()
            || self.cas_publication_poisoned
            || self.cas_integrity_poisoned.load(Ordering::SeqCst)
        {
            return Err(
                "operation request requires an unbound, unpoisoned original session".into(),
            );
        }
        let endpoints: Vec<_> = self
            .config
            .providers()
            .iter()
            .map(|provider| provider.endpoint.clone())
            .collect();
        if !transport.matches_endpoints(&endpoints) {
            return Err("prepared transport differs from the original configured providers".into());
        }
        let anchor = self
            .client
            .formal_current_anchor()
            .map_err(|error| error.to_string())?;
        let minimum = cut_from_anchor(&anchor)?;
        if self
            .history
            .last_verified_cut()
            .is_some_and(|cut| cut != &minimum)
        {
            return Err("original minimum differs from the verified persisted history".into());
        }
        Ok(serde_json::json!({
            "api_version": "oasis7.full-sync-original-operation-request/v1",
            "config": &self.config,
            "transfer_policy": policy,
            "node_public_key": &self.node_public_key_hex,
            "initial_minimum": minimum,
            "initial_minimum_record": anchor.head().record(),
            "transport_binding": transport.binding_digest(),
        }))
    }

    /// Explicit operation dependency for the shared-exchange validation fixture.
    #[cfg(all(test, unix))]
    pub(in super::super) fn bind_shared_exchange(
        &mut self,
        exchange: Arc<dyn oasis7_game_client::formal_sync_exchange::FullSyncExchange>,
        original_request: &serde_json::Value,
    ) -> Result<(), String> {
        if self.shared_exchange.is_some() {
            return Err("full-sync operation already bound".into());
        }
        if self.history.is_poisoned()
            || self.cas_publication_poisoned
            || self.cas_integrity_poisoned.load(Ordering::SeqCst)
        {
            return Err("operation session became poisoned before attachment".into());
        }
        let anchor = self
            .client
            .formal_current_anchor()
            .map_err(|error| error.to_string())?;
        let minimum = cut_from_anchor(&anchor)?;
        if original_request.get("config")
            != Some(&serde_json::to_value(&self.config).map_err(|error| error.to_string())?)
            || original_request
                .get("node_public_key")
                .and_then(serde_json::Value::as_str)
                != Some(self.node_public_key_hex.as_str())
            || original_request.get("initial_minimum")
                != Some(&serde_json::to_value(&minimum).map_err(|error| error.to_string())?)
            || original_request.get("initial_minimum_record")
                != Some(
                    &serde_json::to_value(anchor.head().record())
                        .map_err(|error| error.to_string())?,
                )
            || self
                .history
                .last_verified_cut()
                .is_some_and(|cut| cut != &minimum)
            || super::super::local_io::digest(original_request)?
                != exchange
                    .original_request_digest()
                    .map_err(|error| error.to_string())?
        {
            return Err("session differs from the retained original operation request".into());
        }
        let next = self
            .client
            .independent_clone()
            .map_err(|e| e.to_string())?
            .with_full_sync_exchange(exchange.clone())
            .map_err(|e| e.to_string())?;
        self.client = next;
        self.shared_exchange = Some(exchange);
        Ok(())
    }

    /// Recovery entry with journal admission preceding any CAS mutation.
    /// A pending, acknowledged or legacy publication cannot enter the ordinary
    /// constructor, whose pin stabilization/pruning is not recovery authority.
    #[cfg(all(test, unix))]
    pub(super) fn recover_pre_target(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        instance: &super::super::LocalInstance,
        mut ledger: operation_ledger::Ledger,
        owner: Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        transport: shared_exchange::PreparedTransport,
    ) -> Result<(Self, Arc<shared_exchange::SharedOperationExchange>), String> {
        ledger.guard_pre_target_initialization(owner.clone(), data)?;
        let original: serde_json::Value = serde_json::from_slice(
            ledger
                .snapshot()
                .original_request_json
                .as_ref()
                .ok_or("missing original request")?,
        )
        .map_err(|e| e.to_string())?;
        let signed = authority::sign_read_request(
            "formal-full-sync-node-key-check",
            (),
            &node_private_key_hex,
        )?;
        if original.get("config")
            != Some(&serde_json::to_value(&config).map_err(|e| e.to_string())?)
            || original
                .get("node_public_key")
                .and_then(serde_json::Value::as_str)
                != Some(signed.subject_public_key.as_str())
            || original
                .get("transport_binding")
                .and_then(serde_json::Value::as_str)
                != Some(transport.binding_digest())
        {
            return Err("recovery constructor differs from original operation".into());
        }
        // Replay without pin publication or garbage collection. Both the full
        // original record and exact minimum must still be the local head for
        // this pre-target path; a later head needs selected-target recovery.
        let providers = config.providers();
        let endpoints: Vec<_> = providers.iter().map(|p| p.endpoint.clone()).collect();
        if ledger.snapshot().material_limit != config.max_total_sync_bytes
            || !transport.matches_endpoints(&endpoints)
        {
            return Err("recovery budget/providers differ before CAS initialization".into());
        }
        let provider = providers.first().ok_or("missing recovery provider")?;
        let client = providers::build_read_client(
            &config,
            provider,
            &node_private_key_hex,
            Duration::from_millis(config.request_timeout_ms),
        )?;
        let minimum: FormalFullSyncCutV1 = serde_json::from_value(
            original
                .get("initial_minimum")
                .cloned()
                .ok_or("missing original minimum")?,
        )
        .map_err(|e| e.to_string())?;
        let root = data.join("state/full-sync");
        history_chain::PersistedFormalReadHistoryV1::verify_read_only_exact_boundary(
            &config,
            &client,
            Arc::new(LocalCasStore::new(&root)),
            &root,
            &minimum,
        )?;
        let anchor = client.formal_current_anchor().map_err(|e| e.to_string())?;
        if cut_from_anchor(&anchor)? != minimum
            || original.get("initial_minimum_record")
                != Some(&serde_json::to_value(anchor.head().record()).map_err(|e| e.to_string())?)
        {
            return Err("original minimum/record differs before CAS initialization".into());
        }
        let expected = serde_json::json!({
            "api_version": "oasis7.full-sync-original-operation-request/v1",
            "config": &config,
            "transfer_policy": &ledger.snapshot().policy,
            "node_public_key": &signed.subject_public_key,
            "initial_minimum": minimum,
            "initial_minimum_record": anchor.head().record(),
            "transport_binding": transport.binding_digest(),
        });
        if original != expected {
            return Err("original request differs before CAS initialization".into());
        }
        ledger.guard_pre_target_initialization(owner.clone(), data)?;
        let mut session = Self::new(config, node_private_key_hex, &root, instance)?;
        let exchange = session.resume_pre_target_operation(ledger, owner, data, transport)?;
        Ok((session, exchange))
    }

    /// Resume the exact selected target only before any local publication.
    /// An unknown or acknowledged publication remains blocked for separate
    /// installation reconciliation; it is never treated as a fresh transfer.
    #[cfg(all(test, unix))]
    pub(super) fn recover_selected_target(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        instance: &super::super::LocalInstance,
        mut ledger: operation_ledger::Ledger,
        owner: Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        transport: shared_exchange::PreparedTransport,
    ) -> Result<(Self, Arc<shared_exchange::SharedOperationExchange>), String> {
        ledger.guard_selected_initialization(owner.clone(), data)?;
        config.validate()?;
        let original: serde_json::Value = serde_json::from_slice(
            ledger
                .snapshot()
                .original_request_json
                .as_ref()
                .ok_or("missing original request")?,
        )
        .map_err(|e| e.to_string())?;
        let target_bytes = ledger
            .snapshot()
            .verified_target_json
            .clone()
            .ok_or("missing original target")?;
        let (target, target_record): (FormalFullSyncCutV1, serde_json::Value) =
            serde_json::from_slice(&target_bytes).map_err(|e| e.to_string())?;
        let minimum: FormalFullSyncCutV1 = serde_json::from_value(
            original
                .get("initial_minimum")
                .cloned()
                .ok_or("missing original minimum")?,
        )
        .map_err(|e| e.to_string())?;
        let signed = authority::sign_read_request(
            "formal-full-sync-node-key-check",
            (),
            &node_private_key_hex,
        )?;
        let providers = config.providers();
        let endpoints: Vec<_> = providers.iter().map(|p| p.endpoint.clone()).collect();
        if ledger.snapshot().material_limit != config.max_total_sync_bytes
            || !transport.matches_endpoints(&endpoints)
        {
            return Err("recovery budget/providers differ before CAS initialization".into());
        }
        let expected = serde_json::json!({
            "api_version": "oasis7.full-sync-original-operation-request/v1",
            "config": &config, "transfer_policy": &ledger.snapshot().policy,
            "node_public_key": &signed.subject_public_key,
            "initial_minimum": &minimum,
            "initial_minimum_record": original.get("initial_minimum_record"),
            "transport_binding": transport.binding_digest(),
        });
        if expected != original {
            return Err("selected recovery differs from the complete original request".into());
        }
        let provider = providers.first().ok_or("missing recovery provider")?;
        let client = providers::build_read_client(
            &config,
            provider,
            &node_private_key_hex,
            Duration::from_millis(config.request_timeout_ms),
        )?;
        let root = data.join("state/full-sync");
        let summary = history_chain::PersistedFormalReadHistoryV1::verify_read_only_exact_boundary(
            &config,
            &client,
            Arc::new(LocalCasStore::new(&root)),
            &root,
            &minimum,
        )?
        .ok_or("missing authenticated operation history")?;
        if summary.get("minimum_record") != original.get("initial_minimum_record") {
            return Err("original minimum record differs from authenticated history".into());
        }
        let anchor = client.formal_current_anchor().map_err(|e| e.to_string())?;
        // This first selected-target increment requires the durable history to
        // end exactly at T. Later-than-T recovery needs explicit prefix capture.
        if cut_from_anchor(&anchor)? != target
            || serde_json::to_value(anchor.head().record()).map_err(|e| e.to_string())?
                != target_record
            || !nondecreasing_cut(&minimum, &target)
        {
            return Err("selected recovery target differs from authenticated durable head".into());
        }
        config
            .grant
            .as_ref()
            .ok_or("missing recovery grant")?
            .verify(
                &config.formal_read_trust.initial_trust.issuer_public_key,
                &target.world,
                &target.chain_id,
                &signed.subject_public_key,
                &config.expected_role,
                target.authority_epoch,
                now_unix_ms().ok_or("system clock before Unix epoch")?,
            )?;
        ledger.guard_selected_initialization(owner.clone(), data)?;
        let mut session = Self::new(config, node_private_key_hex, &root, instance)?;
        let current = session
            .client
            .formal_current_anchor()
            .map_err(|e| e.to_string())?;
        if cut_from_anchor(&current)? != target || current.head().record() != anchor.head().record()
        {
            return Err("selected history changed during recovery initialization".into());
        }
        let attempt = ledger.resume_selected_target(owner, data, &original, &target_bytes)?;
        let exchange = Arc::new(transport.admit(ledger, attempt)?);
        use oasis7_game_client::formal_sync_exchange::FullSyncExchange;
        if exchange
            .original_request_digest()
            .map_err(|e| e.to_string())?
            != super::super::local_io::digest(&original)?
        {
            return Err("selected recovery transport identity changed".into());
        }
        session.client = session
            .client
            .independent_clone()
            .map_err(|e| e.to_string())?
            .with_full_sync_exchange(exchange.clone())
            .map_err(|e| e.to_string())?;
        session.shared_exchange = Some(exchange.clone());
        session.recovery_target = Some(attempt::FullSyncTargetV1 {
            anchor,
            cut: target,
        });
        Ok((session, exchange))
    }

    /// Recover only an original pre-target operation on its exact native node.
    /// A selected/installed target requires the separate history/install recovery
    /// path and is rejected here rather than replaced with a fresh request.
    #[cfg(all(test, unix))]
    pub(super) fn resume_pre_target_operation(
        &mut self,
        mut ledger: operation_ledger::Ledger,
        owner: Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        transport: shared_exchange::PreparedTransport,
    ) -> Result<Arc<shared_exchange::SharedOperationExchange>, String> {
        owner.require_directory(data)?;
        if self.cas_root.canonicalize().map_err(|e| e.to_string())?
            != data
                .join("state/full-sync")
                .canonicalize()
                .map_err(|e| e.to_string())?
        {
            return Err("operation recovery belongs to a different local CAS".into());
        }
        let request = self.prepare_operation_request(&ledger.snapshot().policy, &transport)?;
        let attempt = ledger.resume_before_target(owner, data, &request)?;
        let exchange = Arc::new(transport.admit(ledger, attempt)?);
        self.bind_shared_exchange(exchange.clone(), &request)?;
        Ok(exchange)
    }
}
