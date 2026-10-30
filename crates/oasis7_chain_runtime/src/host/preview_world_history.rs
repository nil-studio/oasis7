//! Explicit DevLocal offline history reconstruction. No shipping read fallback.
//! All execution writes go to a caller-owned, initially absent scratch directory.
#[cfg(not(test))]
use super::execution_bridge as bridge;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
use super::execution_bridge_real_tests::real_execution_bridge as bridge;
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7::world_service::CommitRef;
use oasis7_node::{NodeConsensusAction, NodeExecutionCommitContext, NodeExecutionHook};
use oasis7_runtime::{LocalCasStore, ReleaseSecurityPolicy, blake3_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Instant,
};

const MAX_FILE: usize = 16 * 1024 * 1024;
#[path = "preview_world_history_publish.rs"]
mod publication;
pub(super) use publication::run;
#[cfg(test)]
pub(super) use publication::{PublicationBudget, rehydrate};
/// Trusted local configuration, never decoded from a remote request.
pub(super) struct ReplayBudget {
    pub max_heights: u64,
    pub max_total_bytes: usize,
    pub deadline: Instant,
    #[cfg(test)]
    pub diagnostic_sink: Option<fn(&Path, &'static str, u64, &str)>,
}

#[derive(Deserialize)]
struct Envelope {
    version: u8,
    world_id: String,
    node_id: String,
    record: oasis7_distfs::FileReplicationRecord,
    payload: Vec<u8>,
    public_key_hex: Option<String>,
    signature_hex: Option<String>,
}
#[derive(Serialize)]
struct SigningPayload<'a> {
    version: u8,
    world_id: &'a str,
    node_id: &'a str,
    record: &'a oasis7_distfs::FileReplicationRecord,
    payload: &'a [u8],
    public_key_hex: Option<&'a str>,
}
#[derive(Deserialize)]
struct Commit {
    world_id: String,
    node_id: String,
    proposer_id: Option<String>,
    height: u64,
    slot: u64,
    epoch: u64,
    block_hash: String,
    action_root: String,
    actions: Vec<NodeConsensusAction>,
    committed_at_ms: i64,
    execution_block_hash: String,
    execution_state_root: String,
}
struct Budget {
    bytes: usize,
    limits: ReplayBudget,
}
impl Budget {
    fn check(&self) -> Result<(), String> {
        if Instant::now() >= self.limits.deadline {
            return Err("preview history deadline exhausted".into());
        }
        Ok(())
    }
    fn charge(&mut self, bytes: usize) -> Result<(), String> {
        self.check()?;
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or("preview history byte overflow")?;
        if self.bytes > self.limits.max_total_bytes {
            return Err("preview history byte budget exhausted".into());
        }
        Ok(())
    }
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        self.read_limit(path, MAX_FILE)
    }
    fn read_limit(&mut self, path: &Path, max: usize) -> Result<Vec<u8>, String> {
        self.check()?;
        // Reject symlinks and special descriptors before a bounded read.
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "preview history closure file unavailable")?;
        if !file
            .metadata()
            .map_err(|_| "preview history file metadata unavailable")?
            .is_file()
        {
            return Err("preview history closure must be a regular file".into());
        }
        let mut bytes = Vec::new();
        file.take((max + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "preview history closure read failed")?;
        if bytes.len() > max {
            return Err("preview history file budget exhausted".into());
        }
        self.charge(bytes.len())?;
        Ok(bytes)
    }
    fn cas(&mut self, store: &LocalCasStore, hash: &str) -> Result<Vec<u8>, String> {
        self.check()?;
        let bytes = bridge::historical_material::cas(store, hash)?
            .ok_or("preview history CAS closure unavailable")?;
        self.charge(bytes.len())?;
        Ok(bytes)
    }
}

fn authenticate(envelope: &Envelope, pin: &str, world: &str, node: &str) -> Result<Commit, String> {
    if envelope.version != 1
        || envelope.world_id != world
        || envelope.node_id != node
        || envelope.public_key_hex.as_deref() != Some(pin)
        || envelope.record.writer_id != pin
        || envelope.record.world_id != world
        || envelope.record.content_hash != blake3_hex(&envelope.payload)
        || envelope.record.size_bytes != envelope.payload.len() as u64
    {
        return Err("preview history envelope identity rejected".into());
    }
    let public: [u8; 32] = hex::decode(pin)
        .map_err(|_| "preview history signer pin invalid")?
        .try_into()
        .map_err(|_| "preview history signer pin invalid")?;
    let signature = Signature::from_slice(
        &hex::decode(
            envelope
                .signature_hex
                .as_deref()
                .ok_or("preview history signature missing")?,
        )
        .map_err(|_| "preview history signature invalid")?,
    )
    .map_err(|_| "preview history signature invalid")?;
    let signed = SigningPayload {
        version: envelope.version,
        world_id: &envelope.world_id,
        node_id: &envelope.node_id,
        record: &envelope.record,
        payload: &envelope.payload,
        public_key_hex: envelope.public_key_hex.as_deref(),
    };
    VerifyingKey::from_bytes(&public)
        .map_err(|_| "preview history signer pin invalid")?
        .verify_strict(
            &serde_json::to_vec(&signed).map_err(|_| "preview history signing encoding failed")?,
            &signature,
        )
        .map_err(|_| "preview history signature rejected")?;
    let commit: Commit =
        serde_json::from_slice(&envelope.payload).map_err(|_| "preview history commit invalid")?;
    if commit.world_id != world || commit.node_id != node {
        return Err("preview history commit scope rejected".into());
    }
    for action in &commit.actions {
        action
            .validate()
            .map_err(|_| "preview history action hash invalid")?;
    }
    if oasis7_node::compute_consensus_action_root(&commit.actions)
        .map_err(|_| "preview history action root invalid")?
        != commit.action_root
    {
        return Err("preview history action root mismatch".into());
    }
    Ok(commit)
}

/// `service_pin` must be independently provisioned. The receipt authenticates
/// the initial envelope bytes, which in turn fix the original consensus signer.
/// The result grants no authority and is never installed into the source root.
#[expect(
    clippy::too_many_arguments,
    reason = "Offline resource budget and separately provisioned local authority remain explicit."
)]
pub(super) fn reconstruct(
    root: &Path,
    scratch: &Path,
    world: &str,
    node: &str,
    service_pin: &str,
    target: u64,
    limits: ReplayBudget,
    local: Option<oasis7_runtime::DevLocalParticipantAuthority>,
) -> Result<bridge::world_service_read::PinnedWorld, String> {
    let mut budget = Budget { bytes: 0, limits };
    // Origin verification has existing fixed bounded reads. Reserve their full
    // conservative bound before invoking that unchanged identity verifier.
    budget.charge(16 * 1024 * 1024 + 2 * 256 * 1024)?;
    let identity =
        super::preview_world_init::verify_existing_origin(root, world, node, service_pin)?;
    let receipt: Value = serde_json::from_slice(
        &budget.read_limit(&root.join("initialization-receipt.json"), 256 * 1024)?,
    )
    .map_err(|_| "preview history receipt invalid")?;
    let initial: CommitRef = serde_json::from_value(receipt["published_commit"].clone())
        .map_err(|_| "preview history initial commit invalid")?;
    if target <= initial.position || target - initial.position > budget.limits.max_heights {
        return Err("preview history height budget exhausted".into());
    }
    let directory = root.join("genesis/initial-publication");
    let manifest_bytes = budget.read_limit(&directory.join("manifest.json"), 256 * 1024)?;
    if receipt["initial_publication_proof_hash"].as_str()
        != Some(blake3_hex(&manifest_bytes).as_str())
    {
        return Err("preview history initial manifest unauthenticated".into());
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "preview history initial manifest invalid")?;
    if manifest["schema"] != "oasis7.devlocal.preview-initial-publication/v1"
        || manifest["commit"]
            != serde_json::to_value(&initial)
                .map_err(|_| "preview history commit encoding failed")?
    {
        return Err("preview history initial manifest binding rejected".into());
    }
    let mut archived = Vec::new();
    for (name, hash) in [
        ("record.json", "record_hash"),
        ("consensus-envelope.json", "envelope_hash"),
        ("snapshot.cbor", "snapshot_hash"),
        ("journal.cbor", "journal_hash"),
        ("world-head-proof.cbor", "proof_hash"),
    ] {
        let max = match name {
            "record.json" => 1024 * 1024,
            "world-head-proof.cbor" => 4 * 1024 * 1024,
            _ => MAX_FILE,
        };
        let bytes = budget.read_limit(&directory.join(name), max)?;
        if manifest[hash].as_str() != Some(blake3_hex(&bytes).as_str()) {
            return Err("preview history initial archived hash rejected".into());
        }
        archived.push(bytes);
    }
    let policy = if receipt.get("provider").is_some_and(|v| !v.is_null()) {
        ReleaseSecurityPolicy::default()
    } else {
        ReleaseSecurityPolicy::production_hardened()
    };
    let restore = bridge::world_service_read::RestoreContext::new(policy, local)?;
    bridge::world_service_read::verify_archived_initial_publication_with_context(
        &archived[0],
        &archived[2],
        &archived[3],
        &archived[4],
        &identity,
        &initial,
        &restore,
    )?;
    let initial_envelope: Envelope = serde_json::from_slice(&archived[1])
        .map_err(|_| "preview history initial envelope invalid")?;
    let signer = initial_envelope
        .public_key_hex
        .as_deref()
        .ok_or("preview history original signer absent")?;
    let base = authenticate(&initial_envelope, signer, world, node)?;
    if base.height != initial.position
        || base.execution_block_hash != initial.execution_block_hash
        || base.execution_state_root != initial.state_root_ref
    {
        return Err("preview history original signed head mismatch".into());
    }
    let source_store = LocalCasStore::new(root.join("store"));
    let mut closure = Vec::new();
    let mut sequence = initial_envelope.record.sequence;
    for height in initial.position + 1..=target {
        let bytes = budget.read(
            &root
                .join("replication/replication_commit_messages")
                .join(format!("{height:020}.json")),
        )?;
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|_| "preview history envelope invalid")?;
        let commit = authenticate(&envelope, signer, world, node)?;
        if commit.height != height
            || envelope.record.writer_epoch != initial_envelope.record.writer_epoch
            || envelope.record.sequence
                != sequence
                    .checked_add(1)
                    .ok_or("preview history sequence exhausted")?
        {
            return Err("preview history ordered envelope gap".into());
        }
        sequence = envelope.record.sequence;
        let record = budget.read(&root.join("records").join(format!("{height:020}.json")))?;
        let value: Value =
            serde_json::from_slice(&record).map_err(|_| "preview history record invalid")?;
        let proof = budget.cas(
            &source_store,
            value["world_head_proof_ref"]
                .as_str()
                .ok_or("preview history proof unavailable")?,
        )?;
        let effect = budget.cas(
            &source_store,
            value["external_effect_ref"]
                .as_str()
                .ok_or("preview history external effect unavailable")?,
        )?;
        closure.push((commit, value, proof, effect));
    }
    budget.check()?;
    let identity_bytes =
        budget.read_limit(&root.join("world/world-service-identity.json"), 256 * 1024)?;
    let source_identity: oasis7::world_service::WorldIdentity =
        serde_json::from_slice(&identity_bytes)
            .map_err(|_| "preview history source service identity invalid")?;
    source_identity
        .validate()
        .map_err(|_| "preview history source service identity invalid")?;
    if source_identity != identity {
        return Err("preview history source service identity differs from verified origin".into());
    }
    create_scratch(root, scratch)?;
    // The genuine driver resolves service-intent identity from this Host
    // sidecar. Copy only the already authenticated original identity; do not
    // derive a new genesis or use a current-head state as the old boundary.
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        fs::DirBuilder::new()
            .mode(0o700)
            .create(scratch.join("world"))
            .map_err(|_| "preview history scratch World directory unavailable")?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(scratch.join("world/world-service-identity.json"))
            .and_then(|mut file| file.write_all(&identity_bytes))
            .map_err(|_| "preview history scratch service identity publication failed")?;
    }
    let snapshot = oasis7_world_state::codec::snapshot_from_cbor(&archived[2])
        .map_err(|_| "preview history initial snapshot invalid")?;
    let journal = oasis7_world_state::codec::journal_from_cbor(&archived[3])
        .map_err(|_| "preview history initial journal invalid")?;
    let runtime = restore
        .restore(snapshot, journal)
        .map_err(|_| "preview history initial World restore rejected")?;
    let executor = oasis7_wasm_executor::WasmExecutor::new_for_deterministic_replay(
        oasis7_wasm_executor::WasmExecutorConfig::default(),
    )
    .map_err(|_| "preview history real executor unavailable")?;
    let mut driver = bridge::NodeRuntimeExecutionDriver::new_with_sandbox(
        scratch.join("state.json"),
        scratch.join("world"),
        scratch.join("records"),
        scratch.join("store"),
        bridge::ExecutionBridgeState {
            last_applied_committed_height: initial.position,
            last_execution_block_hash: Some(initial.execution_block_hash.clone()),
            last_execution_state_root: Some(initial.state_root_ref.clone()),
            last_node_block_hash: Some(base.block_hash),
        },
        runtime,
        Box::new(executor),
        32,
        32,
        4,
    );
    for (commit, record, proof, effect) in closure {
        budget.check()?;
        if record["world_id"] != world
            || record["height"] != commit.height
            || record["execution_block_hash"] != commit.execution_block_hash
            || record["execution_state_root"] != commit.execution_state_root
            || record["node_block_hash"] != commit.block_hash
            || record["action_root"] != commit.action_root
        {
            return Err("preview history signed record mismatch".into());
        }
        let replay_height = commit.height;
        let result = driver
            .on_commit_with_expected(
                NodeExecutionCommitContext {
                    world_id: commit.world_id,
                    node_id: commit.node_id.clone(),
                    proposer_id: commit.proposer_id.unwrap_or(commit.node_id),
                    height: commit.height,
                    slot: commit.slot,
                    epoch: commit.epoch,
                    node_block_hash: commit.block_hash,
                    action_root: commit.action_root,
                    committed_actions: commit.actions,
                    committed_at_unix_ms: commit.committed_at_ms,
                },
                Some(&commit.execution_block_hash),
                Some(&commit.execution_state_root),
            )
            .map_err(|error| {
                #[cfg(test)]
                if let Some(sink) = budget.limits.diagnostic_sink {
                    sink(root, "deterministic_execution", replay_height, &error);
                }
                #[cfg(not(test))]
                let _ = error;
                format!("preview history deterministic execution rejected; height={replay_height}")
            })?;
        budget.check()?;
        let fresh: Value = serde_json::from_slice(
            &budget.read(
                &scratch
                    .join("records")
                    .join(format!("{:020}.json", result.execution_height)),
            )?,
        )
        .map_err(|_| "preview history scratch record invalid")?;
        let fresh_proof = budget.cas(
            &driver.execution_store,
            fresh["world_head_proof_ref"]
                .as_str()
                .ok_or("preview history replay proof absent")?,
        )?;
        let fresh_effect = budget.cas(
            &driver.execution_store,
            fresh["external_effect_ref"]
                .as_str()
                .ok_or("preview history replay effect absent")?,
        )?;
        if proof != fresh_proof || effect != fresh_effect {
            #[cfg(test)]
            if let Some(sink) = budget.limits.diagnostic_sink {
                sink(
                    root,
                    "proof_effect_equality",
                    replay_height,
                    "recomputed proof/effect byte equality rejected",
                );
            }
            return Err(format!(
                "preview history replay proof or external effect differs; height={replay_height}"
            ));
        }
    }
    budget.check()?;
    if target == initial.position {
        return Err("preview history initial-only diagnostic is not a replay target".into());
    }
    let restored = bridge::world_service_read::pin_at_height_with_context(
        &scratch.join("records"),
        &scratch.join("store"),
        &identity,
        target,
        &restore,
    )?;
    budget.check()?;
    Ok(restored)
}

pub(super) fn create_scratch(root: &Path, scratch: &Path) -> Result<(), String> {
    let source = root
        .canonicalize()
        .map_err(|_| "preview history source unavailable")?;
    let parent = scratch
        .parent()
        .ok_or("preview history scratch parent missing")?
        .canonicalize()
        .map_err(|_| "preview history scratch parent unavailable")?;
    if !scratch.is_absolute()
        || parent.starts_with(&source)
        || source.starts_with(
            parent.join(
                scratch
                    .file_name()
                    .ok_or("preview history scratch name missing")?,
            ),
        )
    {
        return Err("preview history scratch must be independent".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(scratch)
            .map_err(|_| "preview history requires absent independent scratch")?;
    }
    #[cfg(not(unix))]
    return Err("preview history scratch requires Unix private directory creation".into());
    Ok(())
}
