//! Test-only simulated protected-signer oracle for the formal live process test.
//! Its independent in-memory lease/floor is protocol evidence only; it does not
//! establish HSM custody, nonrollback storage, or physical fault independence.
#![allow(
    dead_code,
    reason = "The shared test oracle includes provider variants not used by this process harness"
)]
use self::fencing::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::{
    formal::{self, CanonicalRecordV1, FormalHeadRefV1, VerifiedAuthorityV1},
    replicated_protocol::{EndpointRole, HeadAnchor},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct HotBody {
    pub version: u32,
    pub instance_digest: String,
    pub authority_id: String,
    pub activation_ref: String,
    pub epoch: u64,
    pub revision: u64,
    pub head: FormalHeadRefV1,
    pub head_node: Option<String>,
    pub imported_head_node: Option<String>,
    pub import_digest: Option<String>,
    pub base_parent: Option<CheckpointParent>,
    pub index: Option<String>,
    pub pending: Option<String>,
    pub decision_head: Option<FormalHeadRefV1>,
    pub seal: Option<EpochSealBodyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_set: Option<DurableSetNativeBindingV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckpointParent {
    pub record: CanonicalRecordV1,
    pub finalizing_authority_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Hot {
    body: HotBody,
    signature: String,
}

pub(super) struct Store;
impl Store {
    pub(super) fn checkpoint_signing_bytes(body: &HotBody) -> Result<Vec<u8>, ()> {
        super::crypto::bytes("oasis7.formal.endpoint_checkpoint.sign.v1", body)
    }

    pub(super) fn verify_public(bytes: &[u8], key: &str) -> Result<HotBody, ()> {
        let hot: Hot = serde_cbor::from_slice(bytes).map_err(|_| ())?;
        super::crypto::verify(
            "oasis7.formal.endpoint_checkpoint.sign.v1",
            &hot.body,
            &hot.signature,
            key,
        )?;
        Ok(hot.body)
    }

    pub(super) fn binding(body: &HotBody, bytes: &[u8]) -> EndpointCheckpointBindingV1 {
        let head = body.decision_head.as_ref().unwrap_or(&body.head);
        EndpointCheckpointBindingV1 {
            revision: body.revision,
            commitment: blake3::hash(bytes).to_hex().to_string(),
            head: HeadAnchor {
                position: head.position,
                decision_hash: head.record_id.clone(),
                qualified: body.decision_head.is_none(),
            },
        }
    }
}

#[derive(Clone)]
pub(super) struct Ledger {
    checkpoint: EndpointCheckpointBindingV1,
    native: Vec<u8>,
    pending: Option<CheckpointIntentV1>,
    seal: Option<EpochSealBodyV1>,
}

pub(super) struct Fixture {
    pub path: PathBuf,
    pub pdir: PathBuf,
    pub rdir: PathBuf,
    pub authority: formal::VerifiedAuthorityV1,
}
impl Fixture {
    pub(super) fn new(
        path: PathBuf,
        pdir: PathBuf,
        rdir: PathBuf,
        authority: formal::VerifiedAuthorityV1,
    ) -> Self {
        Self {
            path,
            pdir,
            rdir,
            authority,
        }
    }
}

pub(super) fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub(super) fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
pub(super) fn h(n: u8) -> String {
    hex::encode([n; 32])
}
pub(super) fn signed(bytes: Vec<u8>, n: u8) -> String {
    hex::encode(key(n).sign(&bytes).to_bytes())
}

pub(super) mod fencing {
    pub use oasis7_distfs::controlled_authority::formal::fencing::*;

    pub fn hash_value<T: serde::Serialize>(value: &T) -> Result<String, ()> {
        Ok(blake3::hash(&serde_cbor::to_vec(value).map_err(|_| ())?)
            .to_hex()
            .to_string())
    }
}

#[path = "../../../../oasis7_distfs/src/controlled_authority/formal/endpoint_test_oracle_frame.rs"]
mod oracle_frame;
use oracle_frame::recv;

fn send(stream: &mut TcpStream, reply: &SignedMonotonicOracleReplyV1) -> Result<(), ()> {
    use std::io::Write;
    let bytes = serde_cbor::to_vec(reply).map_err(|_| ())?;
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| ())
}

#[path = "../../../../oasis7_distfs/src/controlled_authority/formal/endpoint_test_oracle.rs"]
pub(super) mod oracle;

pub(super) struct OracleHandle(oracle::Oracle);
impl OracleHandle {
    pub(super) fn address(&self) -> std::net::SocketAddr {
        self.0.address
    }
}

pub(super) fn start_oracle(fixture: &Fixture, role: EndpointRole) -> OracleHandle {
    OracleHandle(oracle::Oracle::start(fixture, role))
}

pub(super) struct PrivateRoot(PathBuf);
impl PrivateRoot {
    pub(super) fn new() -> Result<Self, String> {
        unique_private_root().map(Self)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        if std::env::var_os("OASIS7_KEEP_FORMAL_FIXTURE").is_some() {
            eprintln!("preserved formal fixture root={}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub(super) struct EndpointChild(std::process::Child);
impl EndpointChild {
    pub(super) fn kill_sigkill_and_wait(&mut self) -> Result<(), String> {
        use std::os::unix::process::ExitStatusExt;
        self.0.kill().map_err(|error| error.to_string())?;
        let status = self.0.wait().map_err(|error| error.to_string())?;
        if status.signal() != Some(9) {
            return Err(format!("endpoint child did not exit by SIGKILL: {status}"));
        }
        Ok(())
    }
}
impl Drop for EndpointChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn start_endpoint(
    binary: &std::ffi::OsStr,
    config: &Path,
) -> Result<(EndpointChild, std::net::SocketAddr), String> {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };

    let mut child = Command::new(binary)
        .arg("--config")
        .arg(config)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start protected endpoint child: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("protected endpoint child stdout unavailable")?;
    let mut reader = std::io::BufReader::new(stdout);
    let endpoint = EndpointChild(child);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("read protected endpoint readiness: {error}"))?;
    let address = line
        .trim()
        .strip_prefix("formal_endpoint_listening=")
        .ok_or_else(|| format!("protected endpoint exited before readiness: {line:?}"))?
        .parse()
        .map_err(|error| format!("parse protected endpoint address: {error}"))?;
    Ok((endpoint, address))
}

pub(super) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("create private fixture {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("write private fixture {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("sync private fixture {}: {error}", path.display()))
}

pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir(path).map_err(|error| format!("create fixture dir: {error}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("protect fixture dir: {error}"))
}

pub(super) fn unique_private_root() -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let root = std::env::temp_dir()
        .canonicalize()
        .map_err(|error| format!("canonicalize temp dir: {error}"))?
        .join(format!(
            "oasis7-formal-live-protected-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir(&root).map_err(|error| format!("create fixture root: {error}"))?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("protect fixture root: {error}"))?;
    Ok(root)
}

// Keep the type imported through this support module for the included oracle's
// `use super::*`; it is an independent simulation, not production custody.
#[allow(dead_code)]
fn _oracle_contract_is_test_only(_: Arc<AtomicBool>, _: Ordering, _: Duration) {}

#[cfg(test)]
#[path = "formal_live_protected_handoff_tests.rs"]
mod handoff_tests;

#[path = "formal_live_protected_survivor_tests.rs"]
pub(super) mod survivor_tests;

use crate::formal_live_recovery_test_support as recovery_test_support;

#[path = "formal_live_protected_durable_set_oracle.rs"]
pub(super) mod durable_set_oracle;
