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
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
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
    pub(super) fn validate(&self, owner: &OriginalPendingOwnerV1) -> Result<(), String> {
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
    // Private, volatile exact Node action identity used only AFTER durable
    // writer Frozen. Never infer this from the outer Node queue snapshot.
    action_bindings: BTreeMap<LocalRequestIdentity, Vec<u8>>,
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
            match fs::symlink_metadata(owner_directory.join(leaf)) {
                Ok(_) => return Err("original pending owner has unresolved prior freeze".into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        reject_orphan_immutable_writes(owner_directory)?;
        Ok(Self {
            owner,
            requests: Vec::new(),
            action_bindings: BTreeMap::new(),
            phase: PendingPhase::Accepting,
        })
    }

    /// Bound the exact original before touching the real Node queue, preventing
    /// a successful enqueue from exceeding the handoff capacity. The real
    /// post-accept step below must still write the exact same bytes.
    pub(super) fn preflight_accepted_enqueue(
        &self,
        identity: &LocalRequestIdentity,
        original_signed_bytes: &[u8],
    ) -> Result<(), String> {
        if self.phase != PendingPhase::Accepting {
            return Err("original pending owner not accepting".into());
        }
        if let Some(existing) = self.requests.iter().find(|r| &r.identity == identity) {
            return if existing.signed_request == original_signed_bytes {
                Ok(())
            } else {
                Err("same original request identity has different signed bytes".into())
            };
        }
        let mut candidate = self.requests.clone();
        candidate.push(RecoveryPendingRequestV1 {
            identity: identity.clone(),
            signed_request: original_signed_bytes.to_vec(),
        });
        RecoveryPendingHandoffV1::from_requests(candidate)
            .map_err(|error| error.to_string())?;
        Ok(())
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

    /// Bind the exact original signed request to the *actual* Node action
    /// bytes that were successfully admitted. This is only an in-memory
    /// retirement index; the original request alone is the recovery material.
    pub(super) fn note_enqueued_with_action(
        &mut self,
        identity: LocalRequestIdentity,
        original_signed_bytes: Vec<u8>,
        node_action_bytes: Vec<u8>,
    ) -> Result<(), String> {
        if node_action_bytes.is_empty() {
            return Err("original pending missing real Node action binding".into());
        }
        self.note_accepted_enqueue(identity.clone(), original_signed_bytes)?;
        if let Some(existing) = self.action_bindings.get(&identity) {
            if existing == &node_action_bytes {
                return Ok(());
            }
            return Err("original pending Node action binding changed".into());
        }
        self.action_bindings.insert(identity, node_action_bytes);
        Ok(())
    }

    /// Called ONLY after the existing FormalNodeHook's real writer has
    /// completed its Frozen prepare with dual durable acknowledgements.
    /// A rejected or unknown prepare MUST leave the original pending intact.
    pub(super) fn note_successful_frozen_actions(
        &mut self,
        exact_frozen_actions: &[&[u8]],
    ) -> Result<(), String> {
        if self.phase != PendingPhase::Accepting {
            return Err("original pending owner frozen; cannot retire further actions".into());
        }
        // Node Frozen supplies action bytes, not the originating signed HTTP
        // identity. Different original requests may encode the SAME Node action;
        // one Frozen occurrence cannot prove which request retired. Preserve
        // every ambiguous original for exact cold recovery instead of dropping
        // unrelated requests merely because their action bytes compare equal.
        let mut binding_counts: BTreeMap<&[u8], usize> = BTreeMap::new();
        for action in self.action_bindings.values() {
            *binding_counts.entry(action.as_slice()).or_default() += 1;
        }
        self.requests.retain(|original| {
            !self.action_bindings.get(&original.identity).is_some_and(|node_bytes| {
                binding_counts.get(node_bytes.as_slice()) == Some(&1)
                    && exact_frozen_actions.contains(&node_bytes.as_slice())
            })
        });
        self.action_bindings.retain(|identity, _| {
            self.requests.iter().any(|original| &original.identity == identity)
        });
        Ok(())
    }

    pub(super) fn ensure_unfrozen_admission(&self) -> Result<(), String> {
        if self.phase != PendingPhase::Accepting {
            return Err("original pending owner no longer accepts Node Frozen".into());
        }
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

pub(super) fn read_prepared_pin_candidate(
    owner_directory: &Path,
    expected_owner: &OriginalPendingOwnerV1,
) -> Result<OriginalPendingPinV1, String> {
    validate_owned_directory(owner_directory)?;
    let pin = read_owner_pin(owner_directory, PREPARED_PIN_FILE)?;
    pin.validate(expected_owner)?;
    Ok(pin)
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
    // A durable terminal marker is not sufficient if the source preparation
    // disappeared or changed after an uncertain Node Frozen boundary.
    // Both same-owner original copies must be present and byte-identical.
    let exact_bytes = serde_cbor::to_vec(checked.handoff()).map_err(|err| err.to_string())?;
    ensure_exact_file(owner_directory, PREPARED_FILE, &exact_bytes)?;
    Ok(checked.handoff().clone())
}

/// An interrupted immutable write may have fsynced its private temporary
/// bytes but died before linking the canonical prepared/owner leaf. That is
/// an unknown freeze outcome, NOT evidence that no freeze ever started.
/// Do not let a restarted owner silently re-open admission in that namespace.
fn reject_orphan_immutable_writes(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        for leaf in [ORIGINAL_FILE, PREPARED_FILE, PREPARED_PIN_FILE, FROZEN_PIN_FILE] {
            let prefix = format!(".{leaf}.");
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some(ids) = rest.strip_suffix(".tmp") else {
                continue;
            };
            if let Some((pid, sequence)) = ids.split_once('.')
                && !pid.is_empty()
                && !sequence.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err("original pending owner has unresolved immutable write".into());
            }
        }
    }
    Ok(())
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
    // A prior lstat is insufficient: an old owner could replace this path
    // with a symlink between metadata validation and the actual open.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|err| err.to_string())?;
    let opened = file.metadata().map_err(|err| err.to_string())?;
    if !opened.file_type().is_file()
        || opened.dev() != meta.dev()
        || opened.ino() != meta.ino()
        || opened.len() != meta.len()
        || opened.permissions().mode() & 0o077 != 0
    {
        return Err("original pending file identity changed during open".into());
    }
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
    fn real_action_retirement_requires_exact_frozen_payload_and_preserves_other_pending() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_enqueued_with_action(identity("a"), vec![7, 9], vec![10, 11])
            .unwrap();
        producer
            .note_enqueued_with_action(identity("b"), vec![3, 4], vec![12, 13])
            .unwrap();
        producer
            .note_successful_frozen_actions(&[&[10, 12]])
            .unwrap();
        assert_eq!(producer.requests.len(), 2);
        producer
            .note_successful_frozen_actions(&[&[10, 11]])
            .unwrap();
        assert_eq!(producer.requests.len(), 1);
        assert_eq!(producer.requests[0].signed_request, [3, 4]);
        let frozen = producer.begin_freeze(&scratch.0).unwrap();
        assert_eq!(frozen.count, 1);
        assert!(producer
            .note_successful_frozen_actions(&[&[12, 13]])
            .is_err());
        check_ambiguous_actions_remain_in_original_frozen_cold_read();
    }

    // Reuse the existing selected native case. Two distinct authenticated
    // requests can carry identical Node action bytes; a successful Frozen
    // occurrence must not retire both original request identities.
    fn check_ambiguous_actions_remain_in_original_frozen_cold_read() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_enqueued_with_action(identity("collision-a"), vec![0, 255], vec![21, 22])
            .unwrap();
        producer
            .note_enqueued_with_action(identity("collision-b"), vec![1, 254], vec![21, 22])
            .unwrap();
        producer
            .note_enqueued_with_action(identity("unique"), vec![2, 253], vec![31, 32])
            .unwrap();
        producer
            .note_successful_frozen_actions(&[&[21, 22], &[31, 32]])
            .unwrap();
        assert_eq!(producer.requests.len(), 2);
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        assert_eq!(pin.count, 2);
        producer.finish_node_freeze(&scratch.0).unwrap();
        let recovered =
            cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, pin.count).unwrap();
        assert_eq!(recovered.requests.len(), 2);
        assert_eq!(recovered.requests[0].identity.request_id, "collision-a");
        assert_eq!(recovered.requests[0].signed_request, [0, 255]);
        assert_eq!(recovered.requests[1].identity.request_id, "collision-b");
        assert_eq!(recovered.requests[1].signed_request, [1, 254]);
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
        check_crashed_prepared_temporary_write_blocks_new_owner();
    }

    // Reuse this existing native regression entrypoint. A real writer fsyncs
    // the temporary file before linking its canonical durable marker. Killing
    // it at that gap must never make the next open appear freshly accepting.
    fn check_crashed_prepared_temporary_write_blocks_new_owner() {
        let scratch = Scratch::new();
        for (index, leaf) in [
            ORIGINAL_FILE,
            PREPARED_FILE,
            PREPARED_PIN_FILE,
            FROZEN_PIN_FILE,
        ]
        .into_iter()
        .enumerate()
        {
            let temp = scratch.0.join(format!(".{leaf}.999.{}.tmp", index));
            let mut write = File::create(&temp).unwrap();
            write.write_all(b"unresolved original frozen bytes").unwrap();
            write.sync_all().unwrap();
            File::open(&scratch.0).unwrap().sync_all().unwrap();
            assert!(OriginalPendingProducerV1::open(&scratch.0, owner()).is_err());
            fs::remove_file(temp).unwrap();
        }
        // An unrelated temporary artifact is not a pending freeze marker.
        fs::write(scratch.0.join(".unrelated.999.1.tmp"), b"unrelated").unwrap();
        assert!(OriginalPendingProducerV1::open(&scratch.0, owner()).is_ok());
    }

    #[test]
    fn missing_prepared_original_fails_cold_read_even_with_matching_frozen_marker() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_accepted_enqueue(identity("a"), vec![0, 255, 7])
            .unwrap();
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        producer.finish_node_freeze(&scratch.0).unwrap();
        fs::remove_file(scratch.0.join(PREPARED_FILE)).unwrap();
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, pin.count).is_err());
    }

    #[test]
    fn oversized_original_rejected_before_freeze_and_not_retained() {
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        assert!(producer
            .note_accepted_enqueue(identity("oversized"), vec![7; 16 * 1024 * 1024])
            .is_err());
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        assert_eq!(pin.count, 0);
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
        check_alias_does_not_replace_frozen_original();
    }

    // Reuse the existing selected native regression instead of adding a new
    // cfg/test entrypoint. A byte-identical alias is still not the original
    // private-directory object and must never be followed on cold reopen.
    fn check_alias_does_not_replace_frozen_original() {
        use std::os::unix::fs::symlink;
        let scratch = Scratch::new();
        let mut producer = OriginalPendingProducerV1::open(&scratch.0, owner()).unwrap();
        producer
            .note_accepted_enqueue(identity("alias"), vec![0x31, 0x00, 0xff, 0x42])
            .unwrap();
        let pin = producer.begin_freeze(&scratch.0).unwrap();
        producer.finish_node_freeze(&scratch.0).unwrap();
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 1).is_ok());
        let original_prepared = scratch.0.join(PREPARED_FILE);
        let alias = scratch.0.join("identical-bytes.cbor");
        fs::copy(&original_prepared, &alias).unwrap();
        fs::remove_file(&original_prepared).unwrap();
        symlink(&alias, &original_prepared).unwrap();
        assert!(cold_read_exact_frozen(&scratch.0, &owner(), &pin.root, 1).is_err());
        assert!(
            read_exact_file(
                &scratch.0,
                PREPARED_FILE,
                RECOVERY_PENDING_HANDOFF_ENCODED_LIMIT_V1 as u64,
            )
            .is_err()
        );
    }
}
