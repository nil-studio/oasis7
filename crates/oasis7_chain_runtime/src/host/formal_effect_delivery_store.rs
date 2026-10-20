//! Private durable state for post-commit effect dispatch.
//!
//! This store is a retry ledger only. A stored record never establishes that
//! an effect was committed; callers must revalidate its obligation from the
//! qualified formal history before acting on it after restart.

#[cfg(test)]
use super::formal_effect_outbox::ExternalEffectIdentityV1;
use super::formal_effect_outbox::{
    EffectObligationBindingV1, QualifiedEffectObligationV1, delivery_identity_key, valid_digest,
    validate_effect_obligation_binding, validate_qualified_effect_obligation,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const STORE_SCHEMA_VERSION: u16 = 2;
const STORE_RECORD_CHECKSUM_DOMAIN: &str = "oasis7.formal.effect.delivery_record.v2";
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = MAX_RECORD_BYTES as u64;

#[cfg(all(test, unix))]
#[path = "formal_effect_delivery_store_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeliveryRecordV1 {
    schema_version: u16,
    pub(super) obligation: EffectObligationBindingV1,
    pub(super) phase: DeliveryPhaseV1,
    record_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DeliveryPhaseV1 {
    Ready,
    AttemptStarted {
        attempts: u64,
        effect_request_digest: String,
    },
    QueryStarted {
        attempts: u64,
        queries: u64,
        query_digest: String,
        effect_request_digest: String,
    },
    RetryAuthorized {
        attempts: u64,
        effect_request_digest: String,
        query_digest: String,
        /// Complete receiver-signed query proof that this exact identity was
        /// absent. It is retained for audit and never substitutes for the
        /// receiver's idempotency guarantee.
        signed_query_not_found: Vec<u8>,
    },
    Unknown {
        attempts: u64,
        last_error: String,
        effect_request_digest: Option<String>,
    },
    Delivered {
        attempts: u64,
        effect_request_digest: String,
        signed_receipt: Vec<u8>,
    },
    RejectedNoEffect {
        attempts: u64,
        effect_request_digest: String,
        signed_rejection: Vec<u8>,
    },
}

impl DeliveryPhaseV1 {
    pub(super) fn is_terminal(&self) -> bool {
        matches!(self, Self::Delivered { .. } | Self::RejectedNoEffect { .. })
    }
}

/// A single-process, single-writer private ledger. Its lock file prevents a
/// second runtime from dispatching the same identity concurrently.
pub(super) struct FormalEffectDeliveryStore {
    root: PathBuf,
    _process_lock: File,
    gate: Mutex<()>,
    poisoned: Mutex<Option<String>>,
}

impl FormalEffectDeliveryStore {
    /// Open an existing private directory or create its final component under
    /// an already-owned private parent. No symlink or shared directory is
    /// accepted.
    pub(super) fn open(root: &Path) -> Result<Self, String> {
        ensure_private_root(root)?;
        let lock_path = root.join("dispatcher.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let lock = options
            .open(&lock_path)
            .map_err(|error| format!("open effect delivery lock failed: {error}"))?;
        super::controlled_live_config::require_private_owned(&lock_path, false)?;
        lock_exclusive_nonblocking(&lock)?;
        Ok(Self {
            root: root.to_path_buf(),
            _process_lock: lock,
            gate: Mutex::new(()),
            poisoned: Mutex::new(None),
        })
    }

    /// Create a Ready record from an already-qualified obligation, or verify
    /// that the same immutable identity was previously seen. A new commit
    /// reference does not replace the first committed provenance.
    pub(super) fn observe(
        &self,
        obligation: &QualifiedEffectObligationV1,
    ) -> Result<DeliveryRecordV1, String> {
        validate_qualified_effect_obligation(obligation)?;
        let _guard = self.lock_gate()?;
        self.require_healthy()?;
        let key = delivery_identity_key(&obligation.identity)?;
        let path = self.record_path(&key)?;
        match read_record(&path)? {
            Some(record) => {
                if !same_effect_binding(&record.obligation, obligation) {
                    let reason = format!(
                        "external effect identity conflict for {}",
                        obligation.identity.intent_id
                    );
                    self.poison(&reason);
                    return Err(reason);
                }
                Ok(record)
            }
            None => {
                let record = DeliveryRecordV1 {
                    schema_version: STORE_SCHEMA_VERSION,
                    obligation: EffectObligationBindingV1::from_qualified(obligation),
                    phase: DeliveryPhaseV1::Ready,
                    record_checksum: String::new(),
                };
                let mut record = record;
                self.persist(&key, &mut record)?;
                Ok(record)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn read(
        &self,
        identity: &ExternalEffectIdentityV1,
    ) -> Result<Option<DeliveryRecordV1>, String> {
        let _guard = self.lock_gate()?;
        self.require_healthy()?;
        let key = delivery_identity_key(identity)?;
        read_record(&self.record_path(&key)?)
    }

    #[cfg(test)]
    pub(super) fn replace_terminal_response_for_test(
        &self,
        identity: &ExternalEffectIdentityV1,
        response: Vec<u8>,
    ) -> Result<(), String> {
        let _guard = self.lock_gate()?;
        self.require_healthy()?;
        let key = delivery_identity_key(identity)?;
        let path = self.record_path(&key)?;
        let mut record = read_record(&path)?.ok_or("effect obligation is not durably observed")?;
        let stored = match &mut record.phase {
            DeliveryPhaseV1::Delivered { signed_receipt, .. } => signed_receipt,
            DeliveryPhaseV1::RejectedNoEffect {
                signed_rejection, ..
            } => signed_rejection,
            _ => return Err("test response replacement requires terminal record".into()),
        };
        *stored = response;
        self.persist(&key, &mut record)
    }

    /// Durably enter AttemptStarted before a delivery POST. This is the only
    /// transition that grants the transport permission to issue a delivery.
    pub(super) fn begin_attempt(
        &self,
        obligation: &QualifiedEffectObligationV1,
        effect_request_digest: &str,
    ) -> Result<DeliveryRecordV1, String> {
        if !valid_digest(effect_request_digest) {
            return Err("stable effect request digest has invalid shape".into());
        }
        self.transition(obligation, |record| {
            let attempts = match &record.phase {
                DeliveryPhaseV1::Ready => 0,
                DeliveryPhaseV1::RetryAuthorized {
                    attempts,
                    effect_request_digest: authorized_digest,
                    ..
                } if authorized_digest == effect_request_digest => *attempts,
                _ => return Err("delivery attempt is not authorized by durable state".into()),
            };
            record.phase = DeliveryPhaseV1::AttemptStarted {
                attempts: attempts.saturating_add(1),
                effect_request_digest: effect_request_digest.to_owned(),
            };
            Ok(())
        })
    }

    /// Persist QueryStarted before the receiver query POST. A crash during a
    /// query simply causes the same read-only query to be repeated.
    pub(super) fn begin_query(
        &self,
        obligation: &QualifiedEffectObligationV1,
        query_digest: &str,
        effect_request_digest: &str,
    ) -> Result<DeliveryRecordV1, String> {
        if !valid_digest(query_digest) || !valid_digest(effect_request_digest) {
            return Err("query or stable effect digest has invalid shape".into());
        }
        self.transition(obligation, |record| {
            let (attempts, queries, bound_effect_digest) = match &record.phase {
                DeliveryPhaseV1::AttemptStarted {
                    attempts,
                    effect_request_digest: stored_digest,
                } => {
                    if stored_digest != effect_request_digest {
                        return Err("query effect digest differs from prior attempt".into());
                    }
                    (*attempts, 0, stored_digest.clone())
                }
                DeliveryPhaseV1::Unknown {
                    attempts,
                    effect_request_digest: Some(stored_digest),
                    ..
                } => {
                    if stored_digest != effect_request_digest {
                        return Err("query effect digest differs from prior attempt".into());
                    }
                    (*attempts, 0, stored_digest.clone())
                }
                DeliveryPhaseV1::QueryStarted {
                    attempts,
                    queries,
                    effect_request_digest: stored_digest,
                    ..
                } => {
                    if stored_digest != effect_request_digest {
                        return Err("query effect digest differs from prior query".into());
                    }
                    (*attempts, *queries, stored_digest.clone())
                }
                DeliveryPhaseV1::Ready => (0, 0, effect_request_digest.to_owned()),
                _ => return Err("effect query is not required by durable state".into()),
            };
            record.phase = DeliveryPhaseV1::QueryStarted {
                attempts,
                queries: queries.saturating_add(1),
                query_digest: query_digest.to_owned(),
                effect_request_digest: bound_effect_digest,
            };
            Ok(())
        })
    }

    /// Authorize the sole safe redispatch case: a complete signed NotFound
    /// proof plus the route's explicit idempotent-POST guarantee. The
    /// transport verifies the proof before calling this method.
    pub(super) fn authorize_retry(
        &self,
        obligation: &QualifiedEffectObligationV1,
        signed_query_not_found: Vec<u8>,
    ) -> Result<DeliveryRecordV1, String> {
        if signed_query_not_found.is_empty() || signed_query_not_found.len() > 256 * 1024 {
            return Err("signed query proof is empty or exceeds bound".into());
        }
        self.transition(obligation, |record| {
            let (attempts, effect_request_digest, query_digest) = match &record.phase {
                DeliveryPhaseV1::QueryStarted {
                    attempts,
                    effect_request_digest,
                    query_digest,
                    ..
                } => (
                    *attempts,
                    effect_request_digest.clone(),
                    query_digest.clone(),
                ),
                _ => return Err("retry authorization requires a durable query".into()),
            };
            record.phase = DeliveryPhaseV1::RetryAuthorized {
                attempts,
                effect_request_digest,
                query_digest,
                signed_query_not_found,
            };
            Ok(())
        })
    }

    pub(super) fn mark_unknown(
        &self,
        obligation: &QualifiedEffectObligationV1,
        reason: &str,
    ) -> Result<DeliveryRecordV1, String> {
        let safe_reason: String = reason.chars().take(384).collect();
        self.transition(obligation, |record| {
            let (attempts, effect_request_digest) = match &record.phase {
                DeliveryPhaseV1::AttemptStarted {
                    attempts,
                    effect_request_digest,
                } => (*attempts, Some(effect_request_digest.clone())),
                DeliveryPhaseV1::QueryStarted {
                    attempts,
                    effect_request_digest,
                    ..
                } => (*attempts, Some(effect_request_digest.clone())),
                DeliveryPhaseV1::RetryAuthorized {
                    attempts,
                    effect_request_digest,
                    ..
                } => (*attempts, Some(effect_request_digest.clone())),
                DeliveryPhaseV1::Unknown {
                    attempts,
                    effect_request_digest,
                    ..
                } => (*attempts, effect_request_digest.clone()),
                DeliveryPhaseV1::Ready => (0, None),
                DeliveryPhaseV1::Delivered { .. } | DeliveryPhaseV1::RejectedNoEffect { .. } => {
                    return Err("terminal delivery record cannot become Unknown".into());
                }
            };
            record.phase = DeliveryPhaseV1::Unknown {
                attempts,
                last_error: safe_reason,
                effect_request_digest,
            };
            Ok(())
        })
    }

    pub(super) fn mark_delivered(
        &self,
        obligation: &QualifiedEffectObligationV1,
        signed_receipt: Vec<u8>,
        effect_request_digest: &str,
    ) -> Result<DeliveryRecordV1, String> {
        if signed_receipt.is_empty() || signed_receipt.len() > 256 * 1024 {
            return Err("signed effect receipt is empty or exceeds bound".into());
        }
        self.transition(obligation, |record| {
            let (attempts, expected_digest) = match &record.phase {
                DeliveryPhaseV1::AttemptStarted {
                    attempts,
                    effect_request_digest,
                } => (*attempts, effect_request_digest.as_str()),
                DeliveryPhaseV1::QueryStarted {
                    attempts,
                    effect_request_digest,
                    ..
                } => (*attempts, effect_request_digest.as_str()),
                _ => return Err("delivery receipt has no matching attempted identity".into()),
            };
            if expected_digest != effect_request_digest {
                return Err("delivery receipt request digest differs from attempt".into());
            }
            record.phase = DeliveryPhaseV1::Delivered {
                attempts,
                effect_request_digest: effect_request_digest.to_owned(),
                signed_receipt,
            };
            Ok(())
        })
    }

    pub(super) fn mark_rejected_no_effect(
        &self,
        obligation: &QualifiedEffectObligationV1,
        signed_rejection: Vec<u8>,
        effect_request_digest: &str,
    ) -> Result<DeliveryRecordV1, String> {
        if signed_rejection.is_empty() || signed_rejection.len() > 256 * 1024 {
            return Err("signed rejection is empty or exceeds bound".into());
        }
        self.transition(obligation, |record| {
            let (attempts, expected_digest) = match &record.phase {
                DeliveryPhaseV1::AttemptStarted {
                    attempts,
                    effect_request_digest,
                } => (*attempts, effect_request_digest.as_str()),
                _ => return Err("no-effect rejection has no matching delivery attempt".into()),
            };
            if expected_digest != effect_request_digest {
                return Err("no-effect rejection request digest differs from attempt".into());
            }
            record.phase = DeliveryPhaseV1::RejectedNoEffect {
                attempts,
                effect_request_digest: effect_request_digest.to_owned(),
                signed_rejection,
            };
            Ok(())
        })
    }

    fn transition(
        &self,
        obligation: &QualifiedEffectObligationV1,
        change: impl FnOnce(&mut DeliveryRecordV1) -> Result<(), String>,
    ) -> Result<DeliveryRecordV1, String> {
        validate_qualified_effect_obligation(obligation)?;
        let _guard = self.lock_gate()?;
        self.require_healthy()?;
        let key = delivery_identity_key(&obligation.identity)?;
        let path = self.record_path(&key)?;
        let mut record = read_record(&path)?.ok_or("effect obligation is not durably observed")?;
        if !same_effect_binding(&record.obligation, obligation) {
            let reason = format!(
                "external effect identity conflict for {}",
                obligation.identity.intent_id
            );
            self.poison(&reason);
            return Err(reason);
        }
        change(&mut record)?;
        self.persist(&key, &mut record)?;
        Ok(record)
    }

    fn persist(&self, key: &str, record: &mut DeliveryRecordV1) -> Result<(), String> {
        let path = self.record_path(key)?;
        record.record_checksum = delivery_record_checksum(record)?;
        let bytes = serde_cbor::to_vec(record)
            .map_err(|error| format!("encode effect delivery state failed: {error}"))?;
        if bytes.is_empty() || bytes.len() > MAX_RECORD_BYTES {
            return Err("effect delivery state exceeds bounded record size".into());
        }
        let nonce = TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp = self
            .root
            .join(format!(".{key}.{}.{}.tmp", std::process::id(), nonce));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(&temp)
            .map_err(|error| format!("create effect delivery temp failed: {error}"))?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temp);
            return Err(format!(
                "durably write effect delivery state failed: {error}"
            ));
        }
        if let Err(error) = fs::rename(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err(format!("publish effect delivery state failed: {error}"));
        }
        if let Err(error) = File::open(&self.root).and_then(|directory| directory.sync_all()) {
            let reason = format!("effect delivery state publication outcome unknown: {error}");
            self.poison(&reason);
            return Err(reason);
        }
        Ok(())
    }

    fn record_path(&self, key: &str) -> Result<PathBuf, String> {
        if !valid_digest(key) {
            return Err("effect delivery identity key has invalid shape".into());
        }
        Ok(self.root.join(format!("{key}.cbor")))
    }

    fn lock_gate(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.gate
            .lock()
            .map_err(|_| "effect delivery store lock poisoned".into())
    }

    fn require_healthy(&self) -> Result<(), String> {
        match self
            .poisoned
            .lock()
            .map_err(|_| "effect delivery poison lock poisoned")?
            .as_ref()
        {
            Some(reason) => Err(format!("effect delivery store poisoned: {reason}")),
            None => Ok(()),
        }
    }

    fn poison(&self, reason: &str) {
        if let Ok(mut poisoned) = self.poisoned.lock() {
            *poisoned = Some(reason.to_owned());
        }
    }
}

static TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn same_effect_binding(
    stored: &EffectObligationBindingV1,
    incoming: &QualifiedEffectObligationV1,
) -> bool {
    stored.identity == incoming.identity
        && stored.payload_digest == incoming.payload_digest
        && stored.target_id == incoming.target_id
        && stored.receiver_contract_digest == incoming.receiver_contract_digest
        && stored.intent == incoming.intent
}

fn read_record(path: &Path) -> Result<Option<DeliveryRecordV1>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("inspect effect delivery state failed: {error}")),
    };
    super::controlled_live_config::require_private_owned(path, false)?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err("effect delivery state exceeds file size bound".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("open effect delivery state failed: {error}"))?;
    let opened = file
        .metadata()
        .map_err(|error| format!("inspect opened effect delivery state failed: {error}"))?;
    if !opened.is_file() || opened.len() != metadata.len() || opened.len() > MAX_FILE_BYTES {
        return Err("effect delivery state changed during bounded open".into());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read effect delivery state failed: {error}"))?;
    if bytes.len() as u64 != opened.len() || bytes.len() > MAX_RECORD_BYTES {
        return Err("effect delivery state size changed while reading".into());
    }
    let record: DeliveryRecordV1 = serde_cbor::from_slice(&bytes)
        .map_err(|error| format!("decode effect delivery state: {error}"))?;
    validate_effect_obligation_binding(&record.obligation)?;
    if record.schema_version != STORE_SCHEMA_VERSION
        || delivery_identity_key(&record.obligation.identity)?
            != path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
    {
        return Err("effect delivery record schema or identity key mismatch".into());
    }
    let expected_checksum = delivery_record_checksum(&record)?;
    if record.record_checksum != expected_checksum {
        return Err("effect delivery record checksum mismatch".into());
    }
    validate_delivery_phase(&record.phase)?;
    Ok(Some(record))
}

fn delivery_record_checksum(record: &DeliveryRecordV1) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        STORE_RECORD_CHECKSUM_DOMAIN,
        record.schema_version,
        &record.obligation,
        &record.phase,
    ))
    .map_err(|error| format!("encode effect delivery record checksum failed: {error}"))?;
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

fn validate_delivery_phase(phase: &DeliveryPhaseV1) -> Result<(), String> {
    let valid_request_digest = |digest: &str| valid_digest(digest);
    match phase {
        DeliveryPhaseV1::Ready => Ok(()),
        DeliveryPhaseV1::AttemptStarted {
            effect_request_digest,
            ..
        } if valid_request_digest(effect_request_digest) => Ok(()),
        DeliveryPhaseV1::QueryStarted {
            query_digest,
            effect_request_digest,
            ..
        } if valid_request_digest(query_digest) && valid_request_digest(effect_request_digest) => {
            Ok(())
        }
        DeliveryPhaseV1::RetryAuthorized {
            effect_request_digest,
            query_digest,
            signed_query_not_found,
            ..
        } if valid_request_digest(effect_request_digest)
            && valid_request_digest(query_digest)
            && !signed_query_not_found.is_empty()
            && signed_query_not_found.len() <= 256 * 1024 =>
        {
            Ok(())
        }
        DeliveryPhaseV1::Unknown {
            effect_request_digest,
            ..
        } if effect_request_digest
            .as_deref()
            .is_none_or(valid_request_digest) =>
        {
            Ok(())
        }
        DeliveryPhaseV1::Delivered {
            effect_request_digest,
            signed_receipt,
            ..
        } if valid_request_digest(effect_request_digest)
            && !signed_receipt.is_empty()
            && signed_receipt.len() <= 256 * 1024 =>
        {
            Ok(())
        }
        DeliveryPhaseV1::RejectedNoEffect {
            effect_request_digest,
            signed_rejection,
            ..
        } if valid_request_digest(effect_request_digest)
            && !signed_rejection.is_empty()
            && signed_rejection.len() <= 256 * 1024 =>
        {
            Ok(())
        }
        _ => Err("effect delivery record phase has invalid digest or proof bounds".into()),
    }
}

fn ensure_private_root(root: &Path) -> Result<(), String> {
    if root.exists() {
        return super::controlled_live_config::require_private_owned(root, true);
    }
    let parent = root
        .parent()
        .ok_or("effect delivery store parent unavailable")?;
    super::controlled_live_config::require_private_owned(parent, true)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(root)
            .map_err(|error| format!("create private effect delivery directory failed: {error}"))?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!("set private effect delivery directory mode failed: {error}")
        })?;
    }
    #[cfg(not(unix))]
    {
        return Err("formal effect delivery store currently requires Unix private storage".into());
    }
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("durably create effect delivery directory failed: {error}"))?;
    super::controlled_live_config::require_private_owned(root, true)
}

fn lock_exclusive_nonblocking(file: &File) -> Result<(), String> {
    file.try_lock()
        .map_err(|error| format!("another formal effect dispatcher owns the store: {error}"))
}
