//! Initial operation admission and fenced pre-target restart.
use super::super::super::super::{
    LocalInstance, local_io,
    model::{CONFIG_FILE, NodeConfig},
    process::ProcessOwnership,
};
use super::super::super::{
    FormalFullSyncConfigV1, FormalFullSyncConfigV2, FormalFullSyncCutV1, FormalFullSyncSession,
    FormalFullSyncStatusV1, LocalCasStore, cut_from_anchor, history_chain, now_unix_ms,
    operation_ledger, operation_snapshot::read_snapshot, providers, shared_exchange,
    validate_grant_for_node,
};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct InitialBudgetedSession {
    owner: Arc<ProcessOwnership>,
    directory: PathBuf,
    session: FormalFullSyncSession,
    exchange: Arc<shared_exchange::SharedOperationExchange>,
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
            super::super::super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
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
    fn verify_pre_target_minimum(&self, root: &Path, original: &Value) -> Result<(), String> {
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
        history_chain::PersistedFormalReadHistoryV1::verify_read_only_at_least(
            &self.read,
            &client,
            Arc::new(LocalCasStore::new(root)),
            root,
            Some(&minimum),
        )?;
        let anchor = client
            .formal_current_anchor()
            .map_err(|error| error.to_string())?;
        if cut_from_anchor(&anchor)? != minimum
            || original["initial_minimum_record"]
                != serde_json::to_value(anchor.head().record())
                    .map_err(|error| error.to_string())?
        {
            return Err(
                "original minimum and record differ before recovery CAS initialization".into(),
            );
        }
        Ok(())
    }
}

impl InitialBudgetedSession {
    pub(super) fn prepare(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, String> {
        let directory = node.identity.data_dir.clone();
        let inputs = OperationInputs::validate(node, instance, &owner)?;
        let state = directory.join("state");
        let sync_root = state.join("full-sync");
        local_io::reject_symlinks(&sync_root)?;
        match fs::symlink_metadata(&sync_root) {
            Ok(_) => {
                return Err("operation_recovery_required: original full-sync state exists".into());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
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
        let attempt = ledger.begin_attempt()?;
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

    pub(super) fn resume_pre_target(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, String> {
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
            || snapshot.verified_target_json.is_some()
            || snapshot.publication_journal_version != Some(1)
            || snapshot.publication.is_some()
        {
            return Err("operation_recovery_required: pre-target continuity is unavailable".into());
        }
        if snapshot.attempts_used >= policy.max_attempts {
            return Err("operation_budget_exhausted: attempts".into());
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
        let original: Value = serde_json::from_slice(
            snapshot
                .original_request_json
                .as_deref()
                .ok_or("operation recovery requires the full original request")?,
        )
        .map_err(|error| error.to_string())?;
        inputs.verify_pre_target_minimum(&sync_root, &original)?;
        owner.require_directory(&directory)?;
        if Instant::now() >= deadline {
            return Err("operation_reopen_expired".into());
        }
        let session = FormalFullSyncSession::new(
            inputs.read.clone(),
            inputs.private_key_hex.clone(),
            &sync_root,
            instance,
        )?;
        let request =
            inputs.attach_binding(session.prepare_operation_request(policy, &inputs.transport)?);
        if request != original {
            return Err("operation differs from original request after CAS initialization".into());
        }
        let attempt = ledger.resume_before_target(&owner, &directory, &request, deadline)?;
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

    fn attach(
        owner: Arc<ProcessOwnership>,
        directory: PathBuf,
        transport: shared_exchange::PreparedTransport,
        mut session: FormalFullSyncSession,
        ledger: operation_ledger::Ledger,
        attempt: operation_ledger::AttemptPermit,
        request: &Value,
    ) -> Result<Self, String> {
        let exchange = Arc::new(
            transport
                .admit(ledger, attempt)?
                .with_owner(Arc::clone(&owner), directory.clone())?,
        );
        session.bind_shared_exchange(exchange.clone(), request)?;
        Ok(Self {
            owner,
            directory,
            session,
            exchange,
        })
    }

    pub(super) fn run_once(
        mut self,
    ) -> Result<(FormalFullSyncStatusV1, operation_ledger::Snapshot), String> {
        self.owner.require_directory(&self.directory)?;
        self.session.begin_poll();
        let restored = self.session.sync_latest()?;
        self.owner.require_directory(&self.directory)?;
        drop(restored);
        self.session.set_world_loaded(false);
        let status = self.session.status().clone();
        Ok((status, self.exchange.snapshot()))
    }
}
