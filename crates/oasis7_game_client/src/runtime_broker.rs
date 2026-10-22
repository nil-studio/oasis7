//! Operator-local game tool transport. OS isolation must expose only socket + Skill
//! to the model, never this profile, unlock, identity state or executor vaults.
use crate::private_storage as store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
const MAX_REQUEST: usize = 32768;
// Native runtime/client distributions can exceed the small-input bound. Hash
// these artifacts incrementally; never retain executable bytes in memory.
const MAX_BINARY_ARTIFACT_BYTES: usize = 512 * 1024 * 1024;
const MAX_PINNED_INPUT_BYTES: usize = 128 * 1024 * 1024;
const MAX_RESPONSE: usize = 12 * 1024 * 1024 + 65536;
const MAGIC: &[u8; 8] = b"O7TOOL01";
const AAD: &[u8] = b"oasis7/local-tool-request-budget/v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedArtifact {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerResource {
    pub executor_config: PinnedArtifact,
    pub executor_vault_directory: PathBuf,
    pub identity_state_directory: PathBuf,
    pub state_directory: PathBuf,
    pub budget_directory: PathBuf,
    pub max_tool_requests: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerConfig {
    pub profile_version: u32,
    pub runtime_id: String,
    pub runtime_version: String,
    pub runtime_binary: PinnedArtifact,
    pub approved: bool,
    pub source_revision: String,
    pub source_attestation: PinnedArtifact,
    pub skill: PinnedArtifact,
    pub client_binary: PinnedArtifact,
    pub socket_directory: PathBuf,
    pub socket_name: String,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub resources: BTreeMap<String, BrokerResource>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRequest {
    pub request_id: String,
    pub resource_id: String,
    pub args: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerResponse {
    pub request_id: String,
    pub ok: bool,
    pub stdout: String,
    pub error: Option<String>,
}
fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn trusted_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors().skip(1) {
        let m = std::fs::symlink_metadata(ancestor)
            .map_err(|_| "broker artifact ancestor unavailable")?;
        let root_sticky = m.uid() == 0
            && m.mode() & 0o1000 != 0
            && [Path::new("/tmp"), Path::new("/private/tmp")].contains(&ancestor);
        if !m.is_dir()
            || ![0, unsafe { geteuid() }].contains(&m.uid())
            || (m.mode() & 0o022 != 0 && !root_sticky)
        {
            return Err("broker artifact ancestor is writable or untrusted".into());
        }
    }
    Ok(())
}
fn valid_file_metadata(m: &std::fs::Metadata, private: bool, limit: usize) -> bool {
    m.is_file()
        && m.nlink() == 1
        && m.uid() == unsafe { geteuid() }
        && m.mode() & 0o022 == 0
        && (!private || m.mode() & 0o777 == 0o600)
        && m.len() <= limit as u64
}
fn artifact_file(path: &Path, private: bool, limit: usize) -> Result<std::fs::File, String> {
    trusted_ancestors(path)?;
    let m = std::fs::symlink_metadata(path).map_err(|_| "broker artifact unavailable")?;
    if !path.is_absolute()
        || path
            .canonicalize()
            .map_err(|_| "broker artifact path unavailable")?
            != path
        || !valid_file_metadata(&m, private, limit)
    {
        return Err("broker artifact permissions, path or size invalid".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "broker artifact unavailable")?;
    let opened = file.metadata().map_err(|_| "broker artifact unavailable")?;
    if opened.dev() != m.dev()
        || opened.ino() != m.ino()
        || !valid_file_metadata(&opened, private, limit)
    {
        return Err("broker artifact permissions, path or size changed".into());
    }
    Ok(file)
}
fn file(path: &Path, private: bool, limit: usize) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    artifact_file(path, private, limit)?
        .take(limit as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| "broker artifact unreadable")?;
    if b.len() > limit {
        return Err("broker artifact too large".into());
    }
    Ok(b)
}
unsafe extern "C" {
    fn geteuid() -> u32;
}
fn pinned(a: &PinnedArtifact, private: bool) -> Result<Vec<u8>, String> {
    let b = file(&a.path, private, MAX_PINNED_INPUT_BYTES)?;
    if a.sha256.len() != 64 || hex::encode(Sha256::digest(&b)) != a.sha256 {
        return Err("blocked: broker pinned artifact hash mismatch".into());
    }
    Ok(b)
}
fn pinned_binary(a: &PinnedArtifact) -> Result<(), String> {
    let mut input = artifact_file(&a.path, false, MAX_BINARY_ARTIFACT_BYTES)?
        .take(MAX_BINARY_ARTIFACT_BYTES as u64 + 1);
    if a.sha256.len() != 64 {
        return Err("blocked: broker pinned artifact hash mismatch".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0usize;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|_| "broker artifact unreadable")?;
        if count == 0 {
            break;
        }
        total += count;
        if total > MAX_BINARY_ARTIFACT_BYTES {
            return Err("broker artifact too large".into());
        }
        hash.update(&buffer[..count]);
    }
    if hex::encode(hash.finalize()) != a.sha256 {
        return Err("blocked: broker pinned artifact hash mismatch".into());
    }
    Ok(())
}
fn private_dir(p: &Path) -> Result<(), String> {
    let _ = store::open_root(p)?;
    if p.canonicalize().map_err(|_| "private path unavailable")? != p {
        return Err("broker private path alias rejected".into());
    }
    Ok(())
}
impl BrokerConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let b = file(path, true, 65536)?;
        let c: Self =
            oasis7_agent_api::game::parse_request(&b).map_err(|_| "invalid broker profile")?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.approved
            || self.profile_version != 1
            || !["codex", "openclaw"].contains(&self.runtime_id.as_str())
            || !id(&self.runtime_version)
            || self.source_revision.len() != 40
            || !self.source_revision.bytes().all(|b| b.is_ascii_hexdigit())
            || !(1..=120000).contains(&self.timeout_ms)
            || !(1..=2 * 1024 * 1024).contains(&self.max_output_bytes)
            || self.resources.is_empty()
            || self.resources.len() > 16
            || !id(&self.socket_name)
        {
            return Err("blocked: approved supported broker profile required".into());
        }
        private_dir(&self.socket_directory)?;
        trusted_ancestors(&self.socket_directory.join(&self.socket_name))?;
        if self
            .socket_directory
            .join(&self.socket_name)
            .as_os_str()
            .len()
            > 100
        {
            return Err("broker socket path exceeds Unix bound".into());
        }
        // Attestation is a pinned nonsecret UTF-8 source revision, not a claim of source blindness.
        if std::str::from_utf8(&pinned(&self.source_attestation, true)?)
            .map_err(|_| "source attestation invalid")?
            .trim()
            != self.source_revision
        {
            return Err("blocked: source attestation differs from profile".into());
        }
        pinned(&self.skill, false)?;
        pinned_binary(&self.client_binary)?;
        pinned_binary(&self.runtime_binary)?;
        let mut all = vec![(self.socket_directory.clone(), false)];
        for (name, r) in &self.resources {
            if !id(name) || r.max_tool_requests == 0 || r.max_tool_requests > 100000 {
                return Err("invalid broker resource budget".into());
            }
            let bytes = pinned(&r.executor_config, true)?;
            let cfg: crate::device_client::ExecutorDeviceConfig =
                oasis7_agent_api::game::parse_request(&bytes)
                    .map_err(|_| "invalid broker executor config")?;
            if cfg.executor_vault_directory != r.executor_vault_directory
                || cfg.identity.state_directory != r.identity_state_directory
                || cfg.state_directory != r.state_directory
            {
                return Err("broker executor resource paths differ".into());
            }
            for (p, identity) in [
                (&r.executor_vault_directory, false),
                (&r.identity_state_directory, true),
                (&r.state_directory, false),
                (&r.budget_directory, false),
            ] {
                private_dir(p)?;
                if all.iter().any(|(other, other_identity)| {
                    (p.starts_with(other) || other.starts_with(p))
                        && !(identity && *other_identity && p == other)
                }) {
                    return Err("broker resources must use independent private directories".into());
                }
                all.push((p.clone(), identity));
            }
        }
        Ok(())
    }
}
fn allowed(args: &[String]) -> Result<(), String> {
    let command = args.first().ok_or("missing native game command")?;
    let opts: &[&str] = match command.as_str() {
        "describe" | "task" | "capabilities" | "industry" => &[],
        "context" => &["--operation"],
        "move" => &["--context", "--to"],
        "lookup" => &["--context", "--kind"],
        "outcome" => &["--context", "--reason"],
        "quote-build" | "build" => &["--context", "--site", "--factory", "--kind"],
        "quote-schedule" | "schedule" => &["--context", "--factory", "--recipe", "--batches"],
        "schedule-essential" => &[
            "--context",
            "--factory",
            "--recipe",
            "--batches",
            "--obligation",
        ],
        "transfer" => &[
            "--context",
            "--from",
            "--to",
            "--material",
            "--amount",
            "--distance-km",
            "--priority",
            "--routes",
        ],
        "world-rule-key" => &["--request"],
        "world-rule" => &["--context", "--request", "--command"],
        _ => return Err("broker permits native executor game commands only".into()),
    };
    if args.len() > 25 || !(args.len() - 1).is_multiple_of(2) {
        return Err("invalid native game arguments".into());
    }
    let mut seen = BTreeSet::new();
    for pair in args[1..].chunks_exact(2) {
        if !opts.contains(&pair[0].as_str())
            || !seen.insert(&pair[0])
            || pair[1].is_empty()
            || pair[1].len() > 8192
            || pair[1].contains('\0')
        {
            return Err("unsupported or duplicate game option".into());
        }
    }
    Ok(())
}
fn reserve(
    c: &BrokerConfig,
    r: &BrokerResource,
    unlock: &[u8],
    q: &BrokerRequest,
) -> Result<(), String> {
    let d = store::open_root(&r.budget_directory)?;
    let executor = oasis7_client_storage::identity_store::ExecutorControlStore::open(
        &r.executor_vault_directory,
        unlock,
    )?;
    let key = Zeroizing::new(executor.runtime_tool_budget_key());
    store::initialize_key(&d, &key)?;
    let (_lock, created) = store::lock_record(&d, &key, "budget.lock")?;
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&(c, &q.resource_id)).map_err(|_| "broker budget encode")?,
    ));
    // Fixed vault slot: neither renaming a resource nor deleting/moving its
    // budget root can mint a fresh lifetime allowance.
    let binding = (r.budget_directory.clone(), fingerprint.clone());
    let previous_binding: Option<(PathBuf, String)> =
        executor.read_original("runtime-tool-budget/v1", "lifetime", "root-profile")?;
    if previous_binding
        .as_ref()
        .is_some_and(|saved| saved != &binding)
    {
        return Err("broker budget root or profile changed".into());
    }
    let old = store::read_record(&d, &key, "counter", MAGIC, AAD)?;
    if previous_binding.is_some() && (created || old.is_none()) {
        return Err("broker budget history missing; operator reconciliation required".into());
    }
    if previous_binding.is_none() {
        if !created || old.is_some() {
            return Err(
                "broker budget lifetime binding missing; operator reconciliation required".into(),
            );
        }
        let saved = executor.put_original(
            "runtime-tool-budget/v1",
            "lifetime",
            "root-profile",
            &binding,
        )?;
        if saved != binding {
            return Err("broker budget root or profile changed".into());
        }
    }
    let (previous, count): (String, u64) = match old {
        Some(b) => serde_json::from_slice(&b).map_err(|_| "broker budget corrupt")?,
        None if created => (fingerprint.clone(), 0),
        None => {
            return Err("broker budget history missing; operator reconciliation required".into());
        }
    };
    if previous != fingerprint || count >= r.max_tool_requests {
        return Err("broker tool request budget exhausted or profile changed".into());
    }
    let original =
        serde_json::to_vec(&(&q.resource_id, &q.args)).map_err(|_| "broker request encode")?;
    let name = format!(
        "request-{}",
        hex::encode(Sha256::digest(q.request_id.as_bytes()))
    );
    if let Some(old) = store::read_record(&d, &key, &name, MAGIC, AAD)? {
        if *old != original.as_slice() {
            return Err("broker request id conflicts with original game command".into());
        }
    } else {
        store::publish_record(&d, &key, &name, MAGIC, AAD, &original)?;
    }
    store::replace_bounded_record(
        &d,
        &key,
        "counter",
        MAGIC,
        AAD,
        &serde_json::to_vec(&(fingerprint, count + 1)).map_err(|_| "broker budget encode")?,
        4096,
    )
}
/// Serial execution bounds concurrency to one child. Every attempt, including
/// duplicate reconciliation, consumes a durable tool request (never model budget).
fn execute(c: &BrokerConfig, unlock: &[u8], q: &BrokerRequest) -> Result<String, String> {
    if !id(&q.request_id) || !id(&q.resource_id) {
        return Err("invalid broker request identifiers".into());
    }
    allowed(&q.args)?;
    c.validate()?;
    let r = c
        .resources
        .get(&q.resource_id)
        .ok_or("unknown broker resource")?;
    reserve(c, r, unlock, q)?;
    let mut child = Command::new(&c.client_binary.path)
        .arg("--executor-config")
        .arg(&r.executor_config.path)
        .args(&q.args)
        .env_clear()
        .current_dir(&c.socket_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "broker child unavailable")?;
    let write = child
        .stdin
        .take()
        .ok_or("broker child stdin unavailable")?
        .write_all(unlock);
    if write.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("broker child unlock delivery failed".into());
    }
    let stdout = child
        .stdout
        .take()
        .ok_or("broker child stdout unavailable")?;
    let limit = c.max_output_bytes;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + Duration::from_millis(c.timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let bytes = rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| "broker child output timeout")?
                    .map_err(|_| "broker child output failed")?;
                if bytes.len() > limit {
                    return Err("broker child output exceeds bound".into());
                }
                if !status.success() {
                    return Err(
                        "game command failed; reconcile original operation with lookup".into(),
                    );
                }
                return String::from_utf8(bytes)
                    .map_err(|_| "broker child output is not UTF-8".into());
            }
            Ok(None) => (),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("broker child wait failed".into());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("game command timeout; reconcile original operation with lookup".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn receive(stream: &mut UnixStream) -> Result<BrokerRequest, String> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_REQUEST as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "broker RPC read failed")?;
    if bytes.len() > MAX_REQUEST {
        return Err("broker RPC exceeds bound".into());
    }
    oasis7_agent_api::game::parse_request(&bytes).map_err(|_| "invalid broker RPC".to_string())
}
/// Starts no model/runtime and creates no identity. Operator supplies unlock
/// through trusted private stdin; it is never in profile, RPC or child argv/env.
pub fn serve(path: &Path, unlock: Zeroizing<Vec<u8>>) -> Result<(), String> {
    let c = BrokerConfig::load(path)?;
    if !(12..=1024).contains(&unlock.len()) {
        return Err("broker requires private trusted stdin unlock".into());
    }
    let socket = c.socket_directory.join(&c.socket_name);
    if socket.exists() {
        return Err("broker socket already exists; operator reconciliation required".into());
    }
    let listener = UnixListener::bind(&socket).map_err(|_| "broker socket bind failed")?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "broker socket permission failed")?;
    for incoming in listener.incoming() {
        let mut stream = incoming.map_err(|_| "broker socket accept failed")?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| "broker RPC timeout setup failed")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| "broker RPC timeout setup failed")?;
        let response = match receive(&mut stream) {
            Ok(q) => match execute(&c, &unlock, &q) {
                Ok(stdout) => BrokerResponse {
                    request_id: q.request_id,
                    ok: true,
                    stdout,
                    error: None,
                },
                Err(error) => BrokerResponse {
                    request_id: q.request_id,
                    ok: false,
                    stdout: String::new(),
                    error: Some(error),
                },
            },
            Err(error) => BrokerResponse {
                request_id: String::new(),
                ok: false,
                stdout: String::new(),
                error: Some(error),
            },
        };
        let bytes = serde_json::to_vec(&response).map_err(|_| "broker RPC response encode")?;
        if bytes.len() > MAX_RESPONSE {
            return Err("broker encoded response exceeds bound".into());
        }
        let _ = stream.write_all(&bytes);
    }
    Ok(())
}
pub fn call(socket: &Path, q: &BrokerRequest) -> Result<BrokerResponse, String> {
    if !socket.is_absolute() {
        return Err("broker socket must be absolute".into());
    }
    let m = std::fs::symlink_metadata(socket).map_err(|_| "broker socket unavailable")?;
    use std::os::unix::fs::FileTypeExt;
    if !m.file_type().is_socket() || m.uid() != unsafe { geteuid() } || m.mode() & 0o777 != 0o600 {
        return Err("broker socket permissions invalid".into());
    }
    let mut s = UnixStream::connect(socket).map_err(|_| "broker unavailable")?;
    s.set_read_timeout(Some(Duration::from_secs(125)))
        .map_err(|_| "broker timeout setup failed")?;
    s.set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "broker timeout setup failed")?;
    let b = serde_json::to_vec(q).map_err(|_| "broker request encode")?;
    if b.len() > MAX_REQUEST {
        return Err("broker request too large".into());
    }
    s.write_all(&b).map_err(|_| "broker request failed")?;
    s.shutdown(std::net::Shutdown::Write)
        .map_err(|_| "broker request failed")?;
    let mut b = Vec::new();
    s.take(MAX_RESPONSE as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| "broker response failed")?;
    if b.len() > MAX_RESPONSE {
        return Err("broker encoded response exceeds bound".into());
    }
    let response: BrokerResponse = oasis7_agent_api::game::parse_bounded_json(&b, MAX_RESPONSE)
        .map_err(|_| "invalid broker response".to_string())?;
    if response.request_id != q.request_id {
        return Err("broker response request mismatch".into());
    }
    Ok(response)
}
#[cfg(test)]
#[path = "runtime_broker_tests.rs"]
mod tests;
