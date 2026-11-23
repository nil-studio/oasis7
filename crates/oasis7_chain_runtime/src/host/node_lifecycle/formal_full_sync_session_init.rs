//! Session lifecycle and immutable operation attachment.
use super::*;

impl FormalFullSyncSession {
    pub(crate) fn status(&self) -> &FormalFullSyncStatusV1 {
        &self.status
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.config.poll_interval_ms)
    }

    pub(crate) fn begin_poll(&mut self) {
        self.status.ready = false;
        self.status.can_propose = false;
        self.status.state_root_verified = false;
        self.status.world_loaded = false;
        self.status.loaded_cut = None;
        self.status.lag_positions = None;
        self.status.catch_up_pending = false;
        self.status.state = if self.config.grant.is_some() {
            "syncing"
        } else {
            "blocked_missing_issuer_grant"
        }
        .into();
        // Preserve the last observed failure while a retry is in flight. A
        // successful sync below is the only path that clears this field.
        self.status.last_checked_unix_ms = now_unix_ms();
    }

    pub(crate) fn set_world_loaded(&mut self, loaded: bool) {
        self.status.world_loaded = loaded;
        if !loaded {
            self.status.ready = false;
        }
    }

    /// Cold-restores the qualified snapshot; a failed poll withdraws readiness.
    pub(crate) fn sync_latest(&mut self) -> Result<RestoredFormalFullSyncWorldV1, String> {
        self.check_operation_active()?;
        let result = self.sync_latest_owned();
        if let Some(exchange) = &self.shared_exchange
            && let Err(error) = exchange.finish_attempt()
        {
            let error = error.to_string();
            self.fail(&error);
            return Err(error);
        }
        result
    }

    pub(super) fn require_operation_active(&self) -> Result<(), String> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            return Err("operation_recovery_required: worker cancelled".into());
        }
        if self
            .local_recovery_deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err("operation_reopen_expired".into());
        }
        if let Some(exchange) = &self.shared_exchange {
            exchange.check_active().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub(super) fn check_operation_active(&mut self) -> Result<(), String> {
        if let Err(error) = self.require_operation_active() {
            self.fail(&error);
            return Err(error);
        }
        Ok(())
    }

    /// Construct from an already loaded, privately held node identity. The
    /// grant must bind that key; local process ownership alone grants nothing.
    pub(crate) fn new(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        cas_root: &Path,
        instance: &super::super::LocalInstance,
    ) -> Result<Self, String> {
        Self::open(config, node_private_key_hex, cas_root, instance, true)
    }

    pub(super) fn open(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        cas_root: &Path,
        instance: &super::super::LocalInstance,
        initialize: bool,
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
        if initialize {
            super::super::local_io::private_dir(cas_root)?;
        }
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
            initialize,
        )?;
        if initialize {
            cas_lifecycle::stabilize_loaded_world_pins(&cas)?;
            cas_lifecycle::prune_unpinned(&cas)?;
        }
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
            config,
            client,
            http,
            active_provider_index: 0,
            shared_exchange: None,
            recovery_target: None,
            recovery_minimum: None,
            local_recovery_deadline: None,
            cancellation: None,
            #[cfg(test)]
            publication_fault: None,
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
    #[cfg(unix)]
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
    #[cfg(unix)]
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
        let minimum_matches = if let Some(original_minimum) = &self.recovery_minimum {
            original_request.get("initial_minimum")
                == Some(&serde_json::to_value(original_minimum).map_err(|e| e.to_string())?)
                && nondecreasing_cut(original_minimum, &minimum)
                && self
                    .recovery_target
                    .as_ref()
                    .is_none_or(|target| nondecreasing_cut(&target.cut, &minimum))
        } else if let Some(target) = &self.recovery_target {
            nondecreasing_cut(&target.cut, &minimum)
        } else {
            original_request.get("initial_minimum")
                == Some(&serde_json::to_value(&minimum).map_err(|error| error.to_string())?)
                && original_request.get("initial_minimum_record")
                    == Some(
                        &serde_json::to_value(anchor.head().record())
                            .map_err(|error| error.to_string())?,
                    )
        };
        if original_request.get("config")
            != Some(&serde_json::to_value(&self.config).map_err(|error| error.to_string())?)
            || original_request
                .get("node_public_key")
                .and_then(serde_json::Value::as_str)
                != Some(self.node_public_key_hex.as_str())
            || !minimum_matches
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
}
