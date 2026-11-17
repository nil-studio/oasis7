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
        let http = HttpClient::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
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
            #[cfg(unix)]
            cas_root: cas_root.to_path_buf(),
            config,
            client,
            http,
            active_provider_index: 0,
            shared_exchange: None,
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
    pub(in super::super) fn prepare_operation_request(
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

    /// Explicit operation dependency; does not enable V2 configuration by itself.
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

    /// Recover only an original pre-target operation on its exact native node.
    /// A selected/installed target requires the separate history/install recovery
    /// path and is rejected here rather than replaced with a fresh request.
    #[cfg(unix)]
    pub(in super::super) fn resume_pre_target_operation(
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
