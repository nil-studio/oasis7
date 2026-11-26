//! Original owner P/R pending capture: additive L2 Dark producer preparation.
//!
//! Capture only successful, authenticated Node enqueues. The caller MUST stop
//! new admission before invoking begin_freeze and MUST observe the actual Node
//! Frozen transition before invoking finish_node_freeze. Local file persistence
//! is not a protected root, endpoint receipt, custody or recovery permission.
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    formal::endpoint::read_native_recovery_pending_from_managed_archive,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1, RecoveryPendingHandoffV1,
    RecoveryPendingRequestV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

const ORIGINAL_FILE: &str = "pending-handoff.cbor";
const PREPARED_FILE: &str = "pending-handoff.prepared.cbor";
const PREPARED_PIN_FILE: &str = "pending-handoff.prepare.json";
const FROZEN_PIN_FILE: &str = "pending-handoff.owner.json";
const MAX_OWNER_MARKER: u64 = 16 * 1024;
static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginalPendingOwnerV1 {
    pub authority_id: String,
    pub operation_id: String,
    pub request_digest: String,
    pub config_digest: String,
    pub incarnation: String,
}

impl OriginalPendingOwnerV1 {
    fn validate(&self) -> Result<(), String> {
        for item in [
            &self.authority_id,
            &self.operation_id,
            &self.request_digest,
            &self.config_digest,
            &self.incarnation,
        ] {
            if item.is_empty() || item.len() > 256 {
                return Err("original pending owner identity invalid".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginalPendingPinV1 {
    schema_version: u32,
    owner: OriginalPendingOwnerV1,
    pub root: String,
    pub count: usize,
}

impl OriginalPendingPinV1 {
    fn validate(&self, owner: &OriginalPendingOwnerV1) -> Result<(), String> {
        owner.validate()?;
        if self.schema_version != 1
            || self.owner != *owner
            || self.root.len() != 64
            || !self.root.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.count > 4096
        {
            return Err("original pending pin/owner mismatch".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPhase {
    Accepting,
    FreezeUnknown,
    Prepared,
    Frozen,
}

/// The original pending bytes are never regenerated from a resumed action or
/// shortened to the subset still visible after a later runtime publication.
pub(super) struct OriginalPendingProducerV1 {
    owner: OriginalPendingOwnerV1,
    requests: Vec<RecoveryPendingRequestV1>,
    phase: PendingPhase,
}

impl OriginalPendingProducerV1 {
    /// The owner directory must be an isolated, private *already existing*
    /// writer directory. Any prior freeze artifact demands explicit recovery;
    /// a restarted process must never silently re-open new admission.
    pub(super) fn open(
        owner_directory: &Path,
        owner: OriginalPendingOwnerV1,
    ) -> Result<Self, String> {
        owner.validate()?;
        validate_owned_directory(owner_directory)?;
        for leaf in [
            ORIGINAL_FILE,
            PREPARED_FILE,
            PREPARED_PIN_FILE,
            FROZEN_PIN_FILE,
        ] {
            if fs::symlink_metadata(owner_directory.join(leaf)).is_ok() {
                return Err("original pending owner has unresolved prior freeze".into());
            }
        }
        Ok(Self {
            owner,
            requests: Vec::new(),
            phase: PendingPhase::Accepting,
        })
    }

    /// Called *only* after the established authenticated Node queue accepted
    /// the original request. The caller supplies exact wire bytes, not a decoded
    /// representation; this method itself grants no admission.
    pub(super) fn note_accepted_enqueue(
        &mut self,
        identity: LocalRequestIdentity,
        original_signed_bytes: Vec<u8>,
    ) -> Result<(), String> {
        if self.phase != PendingPhase::Accepting {
            return Err("original pending owner not accepting".into());
        }
        if let Some(existing) = self.requests.iter().find(|r| r.identity == identity) {
            if existing.signed_request == original_signed_bytes {
                return Ok(());
            }
            return Err("same original request identity has different signed bytes".into());
        }
        let request = RecoveryPendingRequestV1 {
            identity,
            signed_request: original_signed_bytes,
        };
        let mut next = self.requests.clone();
        next.push(request);
        RecoveryPendingHandoffV1::from_requests(next.clone())
            .map_err(|err| err.to_string())?;
        self.requests = next;
        Ok(())
    }

    /// The *real* Node must be quiesced by its owner first. Persist the entire
    /// original input before the owner attempts Node Frozen. On any I/O error,
    /// admission remains stopped and retry cannot silently refresh inputs.
    pub(super) fn begin_freeze(
        &mut self,
        owner_directory: &Path,
    ) -> Result<OriginalPendingPinV1, String> {
        if self.phase != PendingPhase::Accepting
            && self.phase != PendingPhase::FreezeUnknown
            && self.phase != PendingPhase::Prepared
        {
            return Err("original pending cannot restart completed freeze".into());
        }
        self.phase = PendingPhase::FreezeUnknown;
        validate_owned_directory(owner_directory)?;
        let handoff = RecoveryPendingHandoffV1::from_requests(self.requests.clone())
            .map_err(|err| err.to_string())?;
        let bytes = serde_cbor::to_vec(&handoff).map_err(|err| err.to_string())?;
        if bytes.len() > RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1 {
            return Err("original pending encoded capacity exceeded".into());
        }
        let pin = OriginalPendingPinV1 {
            schema_version: 1,
            owner: self.owner.clone(),
            root: handoff.root().map_err(|err| err.to_string())?,
            count: handoff.requests.len(),
        };
        create_immutable(owner_directory, PREPARED_FILE, &bytes)?;
        let marker = serde_json::to_vec(&pin).map_err(|err| err.to_string())?;
        create_immutable(owner_directory, PREPARED_PIN_FILE, &marker)?;
        self.phase = PendingPhase::Prepared;
        Ok(pin)
    }

    /// Only invoke after the actual owner observed a successful Frozen result.
    /// A failed or uncertain Node transition must NOT invoke this method.
    /// Publication is atomic per file, with a separate last durable marker;
    /// a crash between files remains a non-authorizing incomplete freeze.
    pub(super) fn finish_node_freeze(
        &mut self,
        owner_directory: &Path,
    ) -> Result<OriginalPendingPinV1, String> {
        if self.phase != PendingPhase::Prepared {
            return Err("original pending Node Frozen not prepared".into());
        }
        validate_owned_directory(owner_directory)?;
        let pin = read_owner_pin(owner_directory, PREPARED_PIN_FILE)?;
        pin.validate(&self.owner)?;
        let handoff = RecoveryPendingHandoffV1::from_requests(self.requests.clone())
            .map_err(|err| err.to_string())?;
        if pin.root != handoff.root().map_err(|err| err.to_string())?
            || pin.count != handoff.requests.len()
        {
            return Err("original pending changed across Node Frozen boundary".into());
        }
        let bytes = serde_cbor::to_vec(&handoff).map_err(|err| err.to_string())?;
        ensure_exact_file(owner_directory, PREPARED_FILE, &bytes)?;
        create_immutable(owner_directory, ORIGINAL_FILE, &bytes)?;
        let marker = serde_json::to_vec(&pin).map_err(|err| err.to_string())?;
        create_immutable(owner_directory, FROZEN_PIN_FILE, &marker)?;
        self.phase = PendingPhase::Frozen;
        Ok(pin)
    }

    pub(super) fn is_frozen(&self) -> bool {
        self.phase == PendingPhase::Frozen
    }
}

/// After process loss, require both exact owner identity and an independently
/// authenticated root/count. The self-contained marker is only a consistency
/// fence and cannot elevate the original pending into a recovery authority.
pub(super) fn cold_read_exact_frozen(
    owner_directory: &Path,
    expected_owner: &OriginalPendingOwnerV1,
    independent_root: &str,
    independent_count: usize,
) -> Result<RecoveryPendingHandoffV1, String> {
    validate_owned_directory(owner_directory)?;
    let prepared = read_owner_pin(owner_directory, PREPARED_PIN_FILE)?;
    let frozen = read_owner_pin(owner_directory, FROZEN_PIN_FILE)?;
    prepared.validate(expected_owner)?;
    frozen.validate(expected_owner)?;
    if prepared != frozen
        || frozen.root != independent_root
        || frozen.count != independent_count
    {
        return Err("original pending frozen pin differs from independent witness".into());
    }
    let checked = read_native_recovery_pending_from_managed_archive(
        owner_directory,
        ORIGINAL_FILE,
        independent_root,
    )
    .map_err(|err| err.to_string())?;
    if checked.handoff().requests.len() != frozen.count {
        return Err("original pending frozen count differs from original".into());
    }
    Ok(checked.handoff().clone())
}

fn validate_owned_directory(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute() {
        return Err("original pending directory must be absolute".into());
    }
    let meta = fs::symlink_metadata(directory).map_err(|err| err.to_string())?;
    if !meta.file_type().is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return Err("original pending owner directory must be private and non-symlink".into());
    }
    Ok(())
}

fn read_exact_file(directory: &Path, leaf: &str, limit: u64) -> Result<Vec<u8>, String> {
    let path = directory.join(leaf);
    let meta = fs::symlink_metadata(&path).map_err(|err| err.to_string())?;
    if !meta.file_type().is_file()
        || meta.permissions().mode() & 0o077 != 0
        || meta.len() > limit
    {
        return Err("original pending owned file invalid/oversized".into());
    }
    let file = File::open(&path).map_err(|err| err.to_string())?
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() as u64 != meta.len() || bytes.len() as u64 > limit {
        return Err("original pending file changed during read".into());
    }
    Ok(bytes)
}

fn ensure_exact_file(directory: &Path, leaf: &str, original: &[u8]) -> Result<(), String> {
    let read = read_exact_file(
        directory,
        leaf,
        RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1 as u64,
    )?;
    if read != original {
        return Err("original pending previous immutable file differs".into());
    }
    Ok(())
}

fn read_owner_pin(directory: &Path, leaf: &str) -> Result<OriginalPendingPinV1, String> {
    let raw = read_exact_file(directory, leaf, MAX_OWNER_MARKER)?;
    let marker: OriginalPendingPinV1 =
        serde_json::from_slice(&raw).map_err(|err| err.to_string())?;
    if serde_json::to_vec(&marker).map_err(|err| err.to_string())? != raw {
        return Err("original pending owner marker noncanonical".into());
    }
    Ok(marker)
}

fn create_immutable(directory: &Path, leaf: &str, bytes: &[u8]) -> Result<(), String> {
    let limit = if leaf.ends_with(".json") {
        MAX_OWNER_MARKER
    } else {
        RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1 as u64
    };
    if bytes.len() as u64 > limit {
        return Err("original pending immutable file exceeds bound".into());
    }
    let path = directory.join(leaf);
    if fs::symlink_metadata(&path).is_ok() {
        return ensure_immutable_same(directory, leaf, bytes, limit);
    }
    let n = NEXT_WRITE.fetch_add(1, Ordering::Relaxed);
    let temp = directory.join(format!(".{leaf}.{}.{}.tmp", std::process::id(), n));
    let create = || -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|err| err.to_string())?;
        file.write_all(bytes).map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
        match fs::hard_link(&temp, &path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure_immutable_same(directory, leaf, bytes, limit)?;
            }
            Err(err) => return Err(err.to_string()),
        }
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|err| err.to_string())?;
        Ok(())
    };
    let result = create();
    let _ = fs::remove_file(&temp);
    result
}

fn ensure_immutable_same(
    directory: &Path,
    leaf: &str,
    bytes: &[u8],
    limit: u64,
) -> Result<(), String> {
    if read_exact_file(directory, leaf, limit)? != bytes {
        return Err("original pending immutable destination conflict".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let n = NEXT_WRITE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("o7-original-owner-{}-{n}", std::process::id()));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn owner() -> OriginalPendingOwnerV1 {
        OriginalPendingOwnerV1 {
            authority_id: "authority".into(),
            operation_id: "op".into(),
            request_digest: "original-request".into(),
            config_digest: "config".into(),
            incarnation: "incarnation".into(),
        }
    }
    fn identity(id: &str) -> LocalRequestIdentity {
        LocalRequestIdentity {
            verified_subject: "original-subject".into(),
            operation_domain: "submit".into(),
            nonce_scope: "original-nonce".into(),
            request_id: id.into(),
        }
    }

    #[test]
    fn original_enqueue_freeze_cold_read_exact_bytes_and_owner() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_accepted_enqueue(identity("b"), vec![0, 255, 7])
            .unwrap();
        producer
            .note_accepted_enqueue(identity("a"), vec![1, 2, 3])
            .unwrap();
        producer
            .note_accepted_enqueue(identity("a"), vec![1, 2, 3])
            .unwrap();
        assert!(producer
            .note_accepted_enqueue(identity("a"), vec![8])
            .is_err());
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        assert!(producer.note_accepted_enqueue(identity("c"), vec![5]).is_err());
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 2).is_err());
        let settled = producer.finish_node_freeze(&scratch.0).unwrap();
        assert_eq!(settled, pin);
        assert!(producer.is_frozen());
        assert!(OriginalPendingProducerV1::open(&scratch.0, owner()).is_err());

        for _ in 0..2 {
            let restored =
                cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, pin.count).unwrap();
            assert_eq!(restored.requests[0].identity.request_id, "a");
            assert_eq!(restored.requests[0].signed_request, [1, 2, 3]);
            assert_eq!(restored.requests[1].signed_request, [0, 255, 7]);
        }
        let mut other = owner();
        other.incarnation = "another".into();
        assert!(cold_read_exact_frozen(&scratch.0, &other, &pin.root, 2).is_err());
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &"f".repeat(64), 2).is_err());
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 1).is_err());
    }

    #[test]
    fn unresolved_freeze_is_not_active_or_reopened_as_new_admission() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_accepted_enqueue(identity("old"), vec![0, 255])
            .unwrap();
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        assert!(OriginalPendingProducerV1::open(&scratch.0, owner()).is_err());
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 1).is_err());
        assert_eq!(producer.begin_freeze(&scratch.0).unwrap(), pin);
        assert!(!producer.is_frozen());
        assert!(producer.finish_node_freeze(&scratch.0).is_ok());
        assert!(producer.note_accepted_enqueue(identity("new"), vec![1]).is_err());
    }

    #[test]
    fn changed_original_after_freeze_is_rejected() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_accepted_enqueue(identity("a"), vec![7])
            .unwrap();
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        producer.finish_node_freeze(&scratch.0).unwrap();
        fs::write(scratch.0.join(ORIGINAL_FILE), b"wrong bytes").unwrap();
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 1).is_err());
    }
}
