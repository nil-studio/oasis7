//! Local engineering writer: private candidate storage, exact continuation,
//! and qualified publication. No formal activation, remote export or takeover.
use super::{
    ExecutionBridgeState, controlled_capture as capture, controlled_live_history as history,
    controlled_live_package as package, driver::NodeRuntimeExecutionDriver, durable_transaction,
    to_cbor,
};
use crate::controlled_live_config::{
    GuardedAuthority, GuardedReadAuthority, read_existing_signing_key,
};
use oasis7_distfs::controlled_authority::replicated_protocol::{
    ClosedRecord, EndpointRole, EndpointStatus, FileEndpoint, ProtocolOutcome,
    ReplicatedCoordinator,
    transport::{DurableEndpoint, RemoteEndpoint},
};
use oasis7_node::{
    NodeExecutionBootstrap, NodeExecutionCommitContext, NodeExecutionCommitOutcome,
    NodeExecutionCommitResult, NodeExecutionHook, NodeLocalExecutionContinuation,
};
use oasis7_runtime::LocalCasStore;
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingTransaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frozen_certificate_ref: Option<String>,
    schema_version: u32,
    configuration_digest: String,
    continuation: NodeLocalExecutionContinuation,
    candidate_ref: Option<String>,
    published_head: Option<oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor>,
}
pub(crate) struct GuardedExecutionDriver {
    authority: GuardedAuthority,
    records: PathBuf,
    read_authority: GuardedReadAuthority,
    private_store: LocalCasStore,
    coordinator: ReplicatedCoordinator<Box<dyn DurableEndpoint>>,
    pending: Option<PendingTransaction>,
    fresh_execution: bool,
    #[cfg(all(test, unix))]
    kill_before_execution: bool,
    #[cfg(all(test, unix))]
    kill_before_candidate_save: bool,
    continuation_consumed: bool,
}
fn pending_path(records: &Path) -> PathBuf {
    records.join("controlled-local-continuation.cbor")
}
#[cfg(unix)]
pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    for component in path.ancestors() {
        match std::fs::symlink_metadata(component) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => return Err("guarded private directory component refused".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta)
            if meta.is_dir()
                && !meta.file_type().is_symlink()
                && meta.permissions().mode() & 0o077 == 0 =>
        {
            crate::controlled_live_config::require_private_owned(path, true)
        }
        Ok(_) => Err("guarded operator directory must be private and not a symlink".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            durable_transaction::ensure_dir_all_durable(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            durable_transaction::sync_existing_file_durable(path)
        }
        Err(error) => Err(error.to_string()),
    }
}
#[cfg(not(unix))]
pub(super) fn private_directory(_: &Path) -> Result<(), String> {
    Err("guarded private storage requires Unix".into())
}
impl GuardedExecutionDriver {
    pub(crate) fn new(authority: GuardedAuthority, records: PathBuf) -> Result<Self, String> {
        if !cfg!(unix) || !cfg!(feature = "wasmtime") {
            return Err("guarded engineering execution requires Unix and real wasmtime".into());
        }
        if authority.config.remote_endpoints.is_none() {
            crate::controlled_live_config::require_private_owned(
                &authority.config.primary_directory,
                true,
            )?;
            crate::controlled_live_config::require_private_owned(
                &authority.config.replica_directory,
                true,
            )?;
        }
        // Read and verify all existing publication before any directory creation.
        let current = history::load_head(&authority, &records, None)?;
        let pending = match crate::controlled_history_cli::read_bounded(
            &pending_path(&records),
            2 * 1024 * 1024,
        ) {
            Ok(bytes) => Some(capture::decode_generic::<PendingTransaction>(&bytes)?),
            Err(error) => {
                if std::fs::symlink_metadata(pending_path(&records))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    None
                } else {
                    return Err(error);
                }
            }
        };
        if pending.as_ref().is_some_and(|p| {
            p.schema_version != 1 || p.configuration_digest != authority.configuration_digest
        }) {
            return Err("guarded original continuation configuration changed".into());
        }
        if pending
            .as_ref()
            .and_then(|p| p.published_head.as_ref())
            .is_some_and(|h| h != &current.head)
        {
            return Err("guarded durable published checkpoint tail is missing or changed".into());
        }
        let writer = read_existing_signing_key(&authority.config.writer_key_file)?;
        let (mut primary, mut replica): (Box<dyn DurableEndpoint>, Box<dyn DurableEndpoint>) =
            if let Some(remote) = &authority.config.remote_endpoints {
                let timeout = std::time::Duration::from_millis(remote.timeout_ms);
                let p = RemoteEndpoint::connect(
                    remote.primary,
                    authority.policy.trust.clone(),
                    EndpointRole::Primary,
                    writer.clone(),
                    current.head.clone(),
                    timeout,
                )
                .map_err(|e| e.to_string())?;
                let r = RemoteEndpoint::connect(
                    remote.replica,
                    authority.policy.trust.clone(),
                    EndpointRole::Replica,
                    writer.clone(),
                    current.head.clone(),
                    timeout,
                )
                .map_err(|e| e.to_string())?;
                (Box::new(p), Box::new(r))
            } else {
                let p = FileEndpoint::open(
                    &authority.config.primary_directory,
                    authority.policy.trust.clone(),
                    EndpointRole::Primary,
                    read_existing_signing_key(&authority.config.primary_key_file)?,
                    &current.head,
                )
                .map_err(|e| e.to_string())?;
                let r = FileEndpoint::open(
                    &authority.config.replica_directory,
                    authority.policy.trust.clone(),
                    EndpointRole::Replica,
                    read_existing_signing_key(&authority.config.replica_key_file)?,
                    &current.head,
                )
                .map_err(|e| e.to_string())?;
                (Box::new(p), Box::new(r))
            };
        let mut recovered_candidate = None;
        if let Some(original) = pending.as_ref() {
            let previous = history::load_at_height(
                &authority,
                &records,
                original.continuation.predecessor_committed_height,
            )?;
            let private_store = LocalCasStore::new(records.join("controlled-private-cas"));
            let retained = match original.candidate_ref.as_deref() {
                Some(reference) => {
                    let retained = capture::load_package(&private_store, reference)?;
                    package::verify_package(
                        &retained,
                        &authority.policy.trust,
                        &original.continuation,
                        &previous.record.execution_block_hash,
                        &authority.release_policy,
                    )?;
                    Some(retained)
                }
                None if authority.config.schema_version == 2 => None,
                None => {
                    return Err(
                        "guarded incomplete candidate cannot recover write authority".into(),
                    );
                }
            };
            let request = history::request(&original.continuation)?;
            for endpoint in [&mut primary, &mut replica] {
                let status = endpoint.lookup(&request).map_err(|e| e.to_string())?;
                let proposal = match &status {
                    EndpointStatus::Prepared(p) | EndpointStatus::DecisionUnqualified(p) => {
                        Some(p.as_ref())
                    }
                    EndpointStatus::Qualified(e) => Some(&e.proposal),
                    EndpointStatus::NotRecorded => None,
                };
                let head = endpoint.head();
                if let Some(proposal) = proposal
                    && (retained
                        .as_ref()
                        .is_some_and(|r| &proposal.body.record != r)
                        || proposal.body.request != request
                        || proposal.body.parent_hash != previous.head.decision_hash
                        || proposal.body.position
                            != previous
                                .head
                                .position
                                .checked_add(1)
                                .ok_or("guarded position overflow")?)
                {
                    return Err("guarded endpoint original candidate binding changed".into());
                }
                if retained.is_none()
                    && let Some(proposal) = proposal
                {
                    package::verify_package(
                        &proposal.body.record,
                        &authority.policy.trust,
                        &original.continuation,
                        &previous.record.execution_block_hash,
                        &authority.release_policy,
                    )?;
                    if recovered_candidate
                        .as_ref()
                        .is_some_and(|r| r != &proposal.body.record)
                    {
                        return Err(
                            "endpoints disagree about the original retained candidate".into()
                        );
                    }
                    recovered_candidate = Some(proposal.body.record.clone());
                }
                let retained_decision = matches!(
                    &status,
                    EndpointStatus::DecisionUnqualified(_) | EndpointStatus::Qualified(_)
                );
                if head != previous.head
                    && !(retained_decision
                        && proposal.is_some_and(|p| p.body.position == head.position))
                {
                    return Err(
                        "guarded endpoint tail is not the retained original candidate".into(),
                    );
                }
                if current.head != previous.head && head != current.head {
                    return Err("guarded published tail and endpoint disagree".into());
                }
            }
        }
        let coordinator =
            ReplicatedCoordinator::new(primary, replica, writer).map_err(|e| e.to_string())?;
        let (p, r) = coordinator.heads();
        if pending.is_none() && (p != current.head || r != current.head) {
            return Err("guarded endpoint has an untracked original decision".into());
        }
        private_directory(&records)?;
        let private_root = records.join("controlled-private-cas");
        private_directory(&private_root)?;
        let read_authority = GuardedReadAuthority::from_verified(
            current.head.clone(),
            authority.configuration_digest.clone(),
        );
        let mut driver = Self {
            authority,
            records,
            read_authority,
            private_store: LocalCasStore::new(private_root),
            coordinator,
            pending,
            fresh_execution: false,
            #[cfg(all(test, unix))]
            kill_before_execution: false,
            #[cfg(all(test, unix))]
            kill_before_candidate_save: false,
            continuation_consumed: false,
        };
        if let Some(candidate) = recovered_candidate {
            let reference =
                capture::persist_capture_bytes(&driver.private_store, &to_cbor(&candidate)?)?;
            let mut original = driver
                .pending
                .as_ref()
                .ok_or("missing original candidate identity")?
                .clone();
            original.candidate_ref = Some(reference);
            driver.persist_pending(&original)?;
            driver.pending = Some(original);
        }
        if driver
            .pending
            .as_ref()
            .is_some_and(|p| p.candidate_ref.is_none())
            && driver.authority.config.schema_version == 2
        {
            if !driver.freeze_original()? {
                return Err("original frozen input outcome is unknown; no executor replay".into());
            }
            driver.fresh_execution = true;
        }
        if let Some(pending) = driver
            .pending
            .as_ref()
            .filter(|p| p.candidate_ref.is_some())
        {
            let package = driver.original_candidate(pending)?;
            let previous = history::load_at_height(
                &driver.authority,
                &driver.records,
                pending.continuation.predecessor_committed_height,
            )?;
            package::verify_package(
                &package,
                &driver.authority.policy.trust,
                &pending.continuation,
                &previous.record.execution_block_hash,
                &driver.authority.release_policy,
            )?;
        }
        Ok(driver)
    }
    pub(crate) fn read_authority(&self) -> GuardedReadAuthority {
        self.read_authority.clone()
    }
    pub(crate) fn bootstrap(&self) -> Result<NodeExecutionBootstrap, String> {
        let head = if let Some(p) = self.pending.as_ref() {
            history::load_at_height(
                &self.authority,
                &self.records,
                p.continuation.predecessor_committed_height,
            )?
        } else {
            history::load_head(&self.authority, &self.records, None)?
        };
        Ok(NodeExecutionBootstrap {
            height: head.record.height,
            consensus_block_hash: head
                .record
                .node_block_hash
                .ok_or("guarded predecessor node hash missing")?,
            execution_block_hash: head.record.execution_block_hash,
            execution_state_root: head.record.execution_state_root,
        })
    }
    fn persist_pending(&self, pending: &PendingTransaction) -> Result<(), String> {
        let bytes = to_cbor(pending)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("guarded original continuation exceeds byte bound".into());
        }
        capture::decode_generic::<PendingTransaction>(&bytes)?;
        durable_transaction::write_file_durable(&pending_path(&self.records), &bytes)
    }
    fn original_candidate(&self, p: &PendingTransaction) -> Result<ClosedRecord, String> {
        capture::load_package(&self.private_store, p.candidate_ref.as_deref().ok_or(
            "guarded execution interrupted before complete candidate; no automatic reexecution or repair")?)
    }
    fn execute_once(&mut self) -> Result<(), String> {
        #[cfg(test)]
        let execution_started = std::time::Instant::now();
        #[cfg(test)]
        eprintln!("guarded execution started");
        #[cfg(all(test, unix))]
        if self.kill_before_execution {
            // SAFETY: test-only crash injection kills this isolated subprocess.
            unsafe {
                libc::raise(libc::SIGKILL);
            }
            unreachable!("SIGKILL crash injection returned");
        }
        if !self.fresh_execution {
            return Err("guarded original candidate unavailable; executor replay forbidden".into());
        }
        self.fresh_execution = false;
        let pending = self
            .pending
            .as_ref()
            .ok_or("guarded original continuation missing")?
            .clone();
        if self.authority.config.schema_version == 2 {
            self.retained_frozen(&pending)?;
        }
        let c = &pending.continuation.context;
        let before = history::load_head(&self.authority, &self.records, None)?;
        if before.record.height != pending.continuation.predecessor_committed_height {
            return Err("guarded predecessor changed before execution".into());
        }
        let candidate_root = self
            .records
            .join(format!("controlled-candidate-{}", c.height));
        let candidate_root = if candidate_root.exists() && self.authority.config.schema_version == 2
        {
            self.retained_frozen(&pending)?;
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            self.records.join(format!(
                "controlled-replay-{}-{}-{suffix}",
                c.height,
                std::process::id()
            ))
        } else {
            candidate_root
        };
        if candidate_root.exists() {
            return Err(
                "guarded original candidate directory already exists; reexecution forbidden".into(),
            );
        }
        private_directory(&candidate_root)?;
        let world_dir = candidate_root.join("world");
        private_directory(&world_dir)?;
        let identity = oasis7::world_service::WorldIdentity {
            world_id: self.authority.policy.trust.world_id.clone(),
            genesis_digest: self.authority.policy.trust.genesis_digest.clone(),
        };
        durable_transaction::write_file_durable(
            &world_dir.join("world-service-identity.json"),
            &serde_json::to_vec(&identity).map_err(|e| e.to_string())?,
        )?;
        let state = ExecutionBridgeState {
            last_applied_committed_height: before.record.height,
            last_execution_block_hash: Some(before.record.execution_block_hash.clone()),
            last_execution_state_root: Some(before.record.execution_state_root.clone()),
            last_node_block_hash: before.record.node_block_hash.clone(),
        };
        let sandbox = if self.authority.config.schema_version == 2 {
            self.authority.execution_policy.executor()?
        } else {
            WasmExecutor::new(WasmExecutorConfig::default()).map_err(|e| e.to_string())?
        };
        let (hot_heights, checkpoint_interval, checkpoint_keep) =
            if self.authority.config.schema_version == 2 {
                (
                    self.authority.execution_policy.hot_head_heights,
                    self.authority.execution_policy.checkpoint_interval,
                    self.authority.execution_policy.checkpoint_keep,
                )
            } else {
                (32, 32, 4)
            };
        let mut candidate = NodeRuntimeExecutionDriver::new_with_sandbox(
            candidate_root.join("state.json"),
            world_dir,
            candidate_root.join("records"),
            candidate_root.join("storage"),
            state,
            before.world(&self.authority.release_policy)?,
            Box::new(sandbox),
            hot_heights,
            checkpoint_interval,
            checkpoint_keep,
        );
        candidate.resource_identity = Some(super::driver::ExecutionResourceIdentity {
            chain_id: self.authority.policy.trust.chain_id.clone(),
            genesis_ref: self.authority.policy.trust.genesis_digest.clone(),
        });
        // Only this private candidate executor runs. It cannot become a public
        // checkpoint, mutable live world, read cursor or external-effect source.
        candidate.on_commit(c.clone())?;
        #[cfg(test)]
        eprintln!(
            "guarded executor finished after {:?}",
            execution_started.elapsed()
        );
        let mut record = super::checkpoint::load_execution_bridge_record(
            &super::checkpoint::execution_bridge_record_path(&candidate.records_dir, c.height),
        )?;
        record.commit_log_ref = None;
        record.checkpoint_ref = None;
        record.world_head_proof_ref = None;
        record.world_head_proof_hash = None;
        let snapshot_bytes = candidate
            .execution_store
            .get_verified(
                record
                    .snapshot_ref
                    .as_deref()
                    .ok_or("candidate snapshot missing")?,
            )
            .map_err(|e| format!("{e:?}"))?;
        let journal_bytes = candidate
            .execution_store
            .get_verified(
                record
                    .journal_ref
                    .as_deref()
                    .ok_or("candidate journal missing")?,
            )
            .map_err(|e| format!("{e:?}"))?;
        let effect_bytes = candidate
            .execution_store
            .get_verified(
                record
                    .external_effect_ref
                    .as_deref()
                    .ok_or("candidate effect missing")?,
            )
            .map_err(|e| format!("{e:?}"))?;
        let effect = capture::decode_generic(&effect_bytes)?;
        let closed = package::build(
            &self.authority.policy.trust,
            &pending.continuation,
            &before.record.execution_block_hash,
            &to_cbor(&before.snapshot)?,
            &to_cbor(&before.journal)?,
            &snapshot_bytes,
            &journal_bytes,
            &record,
            &effect,
            &self.authority.release_policy,
        )?;
        #[cfg(test)]
        eprintln!(
            "guarded closed candidate built after {:?}",
            execution_started.elapsed()
        );
        #[cfg(all(test, unix))]
        if self.kill_before_candidate_save {
            // Keep only a test oracle, never a candidate consumed by recovery.
            durable_transaction::write_file_durable(
                &self.records.join("test-interrupted-candidate-digest"),
                oasis7_runtime::blake3_hex(&to_cbor(&closed)?).as_bytes(),
            )?;
            // SAFETY: this test-only boundary runs in an isolated subprocess.
            unsafe {
                libc::raise(libc::SIGKILL);
            }
            unreachable!("SIGKILL crash injection returned");
        }
        let reference = capture::persist_capture_bytes(&self.private_store, &to_cbor(&closed)?)?;
        let mut complete = pending;
        complete.candidate_ref = Some(reference);
        self.persist_pending(&complete)?;
        self.pending = Some(complete);
        #[cfg(test)]
        eprintln!(
            "guarded candidate saved after {:?}",
            execution_started.elapsed()
        );
        Ok(())
    }
    fn resume_original(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        if self
            .pending
            .as_ref()
            .is_none_or(|p| p.continuation.context != context)
        {
            return Err("guarded original context changed".into());
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.candidate_ref.is_none())
        {
            self.execute_once()?;
        }
        let pending = self
            .pending
            .as_ref()
            .ok_or("guarded continuation missing")?
            .clone();
        let retained_current = history::load_head(&self.authority, &self.records, None)?;
        if pending
            .published_head
            .as_ref()
            .is_some_and(|h| h != &retained_current.head)
            || retained_current.head != self.read_authority.current()?
        {
            return Err("guarded original continuation publication fence changed".into());
        }
        let original = self.original_candidate(&pending)?;
        let previous = history::load_at_height(
            &self.authority,
            &self.records,
            pending.continuation.predecessor_committed_height,
        )?;
        package::verify_package(
            &original,
            &self.authority.policy.trust,
            &pending.continuation,
            &previous.record.execution_block_hash,
            &self.authority.release_policy,
        )?;
        let request = history::request(&pending.continuation)?;
        #[cfg(test)]
        let reconciliation_started = std::time::Instant::now();
        let outcome = self
            .coordinator
            .submit(
                self.authority.policy.trust.authority_epoch,
                &previous.head,
                request,
                original.clone(),
            )
            .map_err(|e| e.to_string())?;
        #[cfg(test)]
        eprintln!(
            "guarded reconciliation finished after {:?}",
            reconciliation_started.elapsed()
        );
        let ProtocolOutcome::DurabilityQualified(evidence) = outcome else {
            #[cfg(test)]
            eprintln!(
                "guarded original protocol deferred: {:?}",
                self.coordinator.last_reconciliation_error()
            );
            return Ok(NodeExecutionCommitOutcome::Deferred);
        };
        if evidence.proposal.body.record != original {
            return Err("guarded qualified original record changed".into());
        }
        let qualified = history::verify_successor(&self.authority, &previous, &evidence)?;
        let path = history::evidence_path(&self.records, qualified.head.position);
        let bytes = serde_json::to_vec(&evidence).map_err(|e| e.to_string())?;
        match crate::controlled_history_cli::read_bounded(&path, 64 * 1024 * 1024) {
            Ok(existing) if existing != bytes => {
                return Err("guarded immutable publication conflict".into());
            }
            Ok(_) => {}
            Err(error) => {
                if !std::fs::symlink_metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    return Err(error);
                }
                durable_transaction::write_file_durable(&path, &bytes)?;
            }
        }
        // Re-read the independently verified immutable prefix before Applied.
        let published = history::load_head(&self.authority, &self.records, None)?;
        if published.head != qualified.head {
            return Err("guarded published qualified head conflict".into());
        }
        let mut checkpoint = pending;
        checkpoint.published_head = Some(published.head.clone());
        self.persist_pending(&checkpoint)?;
        self.pending = Some(checkpoint);
        self.read_authority
            .publish_verified(published.head.clone())?;
        Ok(NodeExecutionCommitOutcome::Applied(
            NodeExecutionCommitResult {
                execution_height: published.record.height,
                execution_block_hash: published.record.execution_block_hash,
                execution_state_root: published.record.execution_state_root,
            },
        ))
    }
}
impl NodeExecutionHook for GuardedExecutionDriver {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("guarded engineering execution requires captured original local decision".into())
    }
    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        block: Option<&str>,
        root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        if block.is_some() || root.is_some() {
            return Err(
                "guarded engineering execution rejects synthetic network continuation".into(),
            );
        }
        self.resume_original(context)
    }
    fn prepare_local_continuation(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
    ) -> Result<(), String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| &p.continuation == continuation)
        {
            return Ok(());
        }
        if self.pending.is_some() && !self.continuation_consumed {
            return Err("guarded successor blocked by original continuation".into());
        }
        if history::load_head(&self.authority, &self.records, None)?.head
            != self.read_authority.current()?
        {
            return Err("guarded predecessor publication fence changed".into());
        }
        if continuation.context.node_id != self.authority.config.node_id {
            return Err("guarded original node policy mismatch".into());
        }
        let p = PendingTransaction {
            frozen_certificate_ref: None,
            schema_version: 1,
            configuration_digest: self.authority.configuration_digest.clone(),
            continuation: continuation.clone(),
            candidate_ref: None,
            published_head: None,
        };
        self.persist_pending(&p)?;
        self.pending = Some(p);
        if self.authority.config.schema_version == 2 && !self.freeze_original()? {
            return Err("original frozen inputs not confirmed at both endpoints".into());
        }
        self.fresh_execution = true;
        self.continuation_consumed = false;
        Ok(())
    }
    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        if self.continuation_consumed {
            return Ok(None);
        }
        let Some(p) = self.pending.as_ref() else {
            return Ok(None);
        };
        if p.candidate_ref.is_none() && self.authority.config.schema_version == 2 {
            self.retained_frozen(p)?;
            return Ok(Some(p.continuation.clone()));
        }
        let package = self.original_candidate(p)?;
        let previous = history::load_at_height(
            &self.authority,
            &self.records,
            p.continuation.predecessor_committed_height,
        )?;
        package::verify_package(
            &package,
            &self.authority.policy.trust,
            &p.continuation,
            &previous.record.execution_block_hash,
            &self.authority.release_policy,
        )?;
        // A published candidate is reverified here even when Node counters
        // already reached its height. Missing/corrupt evidence never clears it.
        let current = history::load_head(&self.authority, &self.records, None)?;
        if p.published_head
            .as_ref()
            .is_some_and(|h| h != &current.head)
            || current.head != self.read_authority.current()?
        {
            return Err("guarded original published checkpoint tail is missing or changed".into());
        }
        if current.record.height > p.continuation.context.height {
            return Err("guarded original continuation is behind an alien successor".into());
        }
        Ok(Some(p.continuation.clone()))
    }
    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let p = self
            .pending
            .as_ref()
            .ok_or("guarded completed continuation missing")?;
        let current = history::load_head(&self.authority, &self.records, None)?;
        if &p.continuation.context != context
            || current.record.height != context.height
            || current.continuation.as_ref() != Some(&p.continuation)
        {
            return Err("guarded completion lacks original qualified publication".into());
        }
        // Keep durable original DTO/candidate for Node crash recovery. Only the
        // in-memory marker is consumed; a new decided successor replaces it.
        self.continuation_consumed = true;
        Ok(())
    }
    fn restore_to_height(&mut self, _: &str, _: u64) -> Result<bool, String> {
        Err("guarded qualified history cannot be rolled back by legacy recovery".into())
    }
}

#[cfg(test)]
#[path = "tests/controlled_live.rs"]
mod tests;

#[path = "controlled_live_frozen.rs"]
mod frozen;
