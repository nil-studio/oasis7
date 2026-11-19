//! First-attempt startup for the original operation and actual native owner.
use super::super::{
    LocalInstance, local_io,
    model::{CONFIG_FILE, NodeConfig},
    process::ProcessOwnership,
};
use super::{
    FormalFullSyncSession, FormalFullSyncStatusV1, operation_ledger, shared_exchange,
    validate_grant_for_node,
};
use std::{fs, io, sync::Arc};

pub(super) struct InitialBudgetedSession {
    owner: Arc<ProcessOwnership>,
    directory: std::path::PathBuf,
    session: FormalFullSyncSession,
    exchange: Arc<shared_exchange::SharedOperationExchange>,
}

impl InitialBudgetedSession {
    pub(super) fn prepare(
        node: &NodeConfig,
        instance: &LocalInstance,
        owner: Arc<ProcessOwnership>,
    ) -> Result<Self, String> {
        let directory = node.identity.data_dir.clone();
        owner.require_directory(&directory)?;
        if NodeConfig::load(&directory.join(CONFIG_FILE))? != *node {
            return Err("operation requires the exact persisted local node configuration".into());
        }
        node.verify_identity()?;
        node.identity.chain.verify()?;
        let original = node
            .identity
            .chain
            .budgeted_formal_full_sync()
            .ok_or("budgeted operation requires its exact V4/V2 node binding")?;
        if instance.config_digest != local_io::digest(node)? {
            return Err("operation requires the admitted instance configuration digest".into());
        }

        let grant = original
            .grant
            .as_ref()
            .ok_or("initial budgeted operation requires an independent issuer grant")?;
        let (keypair, _) =
            super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                &directory.join("keys"),
            )?;
        let read = original.read_material_config();
        let now = super::now_unix_ms().ok_or("system clock before Unix epoch")?;
        validate_grant_for_node(&read, grant, &keypair.public_key_hex, now)?;
        let native_owner_binding = owner.binding_digest()?;
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
        let endpoints: Vec<_> = read
            .providers()
            .into_iter()
            .map(|provider| provider.endpoint)
            .collect();
        let transport = shared_exchange::PreparedTransport::prepare_builtin_trust(&endpoints)?;
        owner.require_directory(&directory)?;
        local_io::private_dir(&state)?;
        let mut session =
            FormalFullSyncSession::new(read, keypair.private_key_hex, &sync_root, instance)?;
        let policy = &original.transfer_policy;
        let mut request = session.prepare_operation_request(policy, &transport)?;
        request["node_identity"] =
            serde_json::to_value(&node.identity).map_err(|error| error.to_string())?;
        request["native_owner_binding"] = native_owner_binding.clone().into();
        let identity = operation_ledger::Identity {
            operation_id: local_io::random_id()?,
            request_digest: local_io::digest(&request)?,
            policy_digest: local_io::digest(policy)?,
            binding_digest: native_owner_binding,
        };
        owner.require_directory(&directory)?;
        let mut ledger = operation_ledger::Ledger::create_with_material_limit(
            &sync_root.join("operation.json"),
            identity,
            policy.clone(),
            original.max_total_sync_bytes,
        )?;
        ledger.bind_original_request(&request)?;
        let attempt = ledger.begin_attempt()?;
        let exchange = Arc::new(
            transport
                .admit(ledger, attempt)?
                .with_owner(Arc::clone(&owner), directory.clone())?,
        );
        session.bind_shared_exchange(exchange.clone(), &request)?;
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
