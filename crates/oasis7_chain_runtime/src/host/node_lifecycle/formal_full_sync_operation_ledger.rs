//! Durable body-I/O accounting for a single, explicitly initialized operation.
//!
//! This is local controlled-v2 accounting, not protected storage or a claim of
//! delivery to the peer. A pending reservation survives restart unchanged.
use super::super::local_io;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const API: &str = "oasis7.formal-full-sync-operation-ledger/v1";
const MAX_WINDOW: u64 = 64 * 1024;
const MAX_REQUEST_BODY: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub operation_id: String,
    pub request_digest: String,
    pub policy_digest: String,
    pub binding_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub api_version: String,
    pub attempt_timeout_ms: u64,
    pub max_attempts: u64,
    pub max_encoded_io_bytes: u64,
    pub max_exchanges: u64,
    pub max_proof_pages: u64,
    pub max_proof_bytes: u64,
    pub max_inflight_requests: u64,
    pub reopen_timeout_ms: u64,
}
impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != "oasis7.formal-full-sync-transfer-policy/v1"
            || !(100..=600_000).contains(&self.attempt_timeout_ms)
            || !(1..=4).contains(&self.max_attempts)
            || !(1..=8 * 1024 * 1024 * 1024).contains(&self.max_encoded_io_bytes)
            || !(1..=262_660).contains(&self.max_exchanges)
            || !(1..=512).contains(&self.max_proof_pages)
            || !(1..=2 * 1024 * 1024 * 1024).contains(&self.max_proof_bytes)
            || self.max_inflight_requests != 1
            || !(100..=600_000).contains(&self.reopen_timeout_ms)
            || self
                .max_attempts
                .checked_mul(self.attempt_timeout_ms)
                .is_none()
        {
            return Err("invalid full-sync transfer policy".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Direction {
    Tx,
    Rx,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reservation {
    pub identity: Identity,
    pub nonce: String,
    pub attempt_no: u64,
    pub request_seq: u64,
    pub request_nonce: String,
    pub window_seq: u64,
    pub direction: Direction,
    pub bytes: u64,
}

/// An interrupted attempt retains unresolved credits under its original identity.
/// These bytes are neither confirmed actual nor a refundable balance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InterruptedAttempt {
    attempt_no: u64,
    attempt_nonce: String,
    request: Option<(u64, u64)>,
    request_nonce: Option<String>,
    tx_credit: u64,
    rx_credit: u64,
    reservation: Option<Reservation>,
}
impl InterruptedAttempt {
    fn charged(&self) -> Option<u64> {
        self.tx_credit
            .checked_add(self.rx_credit)?
            .checked_add(self.reservation.as_ref().map_or(0, |r| r.bytes))
    }
}

/// Durable local CAS publication intent, not World installation or readiness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Publication {
    pub intent_digest: String,
    pub attempt_no: u64,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    api_version: String,
    pub identity: Identity,
    #[serde(default)]
    pub owner_binding: Option<String>,
    pub revision: u64,
    pub policy: Policy,
    pub encoded_tx_actual: u64,
    pub encoded_rx_actual: u64,
    pub unknown_final_charged: u64,
    pub exchanges_started: u64,
    pub original_request_json: Option<Vec<u8>>,
    pub verified_target_json: Option<Vec<u8>>,
    #[serde(default)]
    pub publication: Option<Publication>,
    #[serde(default)]
    pub publication_journal_version: Option<u32>,
    pub proof_pages: u64,
    pub proof_bytes: u64,
    pub material_limit: u64,
    pub material_bytes: u64,
    pub materials: std::collections::BTreeMap<String, u64>,
    pub window_seq: u64,
    pub attempts_used: u64,
    #[serde(default)]
    pub interrupted_attempts: Vec<InterruptedAttempt>,
    pub active_attempt_nonce: Option<String>,
    pub active_request: Option<(u64, u64)>,
    pub active_request_nonce: Option<String>,
    pub tx_credit: u64,
    pub rx_credit: u64,
    pub reservation: Option<Reservation>,
}

impl Snapshot {
    fn charged(&self) -> Result<u64, String> {
        self.encoded_tx_actual
            .checked_add(self.encoded_rx_actual)
            .and_then(|n| n.checked_add(self.unknown_final_charged))
            .and_then(|n| {
                self.interrupted_attempts
                    .iter()
                    .try_fold(n, |n, old| n.checked_add(old.charged()?))
            })
            .and_then(|n| n.checked_add(self.tx_credit))
            .and_then(|n| n.checked_add(self.rx_credit))
            .and_then(|n| n.checked_add(self.reservation.as_ref().map_or(0, |r| r.bytes)))
            .ok_or_else(|| "operation accounting overflow".into())
    }

    fn validate(&self) -> Result<(), String> {
        self.policy.validate()?;
        if self
            .publication_journal_version
            .is_some_and(|version| version != 1)
        {
            return Err("unsupported publication journal version".into());
        }
        if let Some(publication) = &self.publication {
            if self.publication_journal_version != Some(1)
                || self.verified_target_json.is_none()
                || publication.attempt_no == 0
                || publication.attempt_no > self.attempts_used
                || publication.intent_digest.len() != 64
                || !publication
                    .intent_digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit())
            {
                return Err("invalid operation publication identity".into());
            }
        }
        let mut previous = 0;
        for old in &self.interrupted_attempts {
            if old.attempt_no <= previous
                || old.attempt_no >= self.attempts_used
                || old.attempt_nonce.len() != 32
                || !old.attempt_nonce.bytes().all(|c| c.is_ascii_hexdigit())
                || old.tx_credit > MAX_REQUEST_BODY
                || old.rx_credit > MAX_WINDOW
                || old.request.is_some() != old.request_nonce.is_some()
                || old.request.is_none()
                    && (old.tx_credit != 0 || old.rx_credit != 0 || old.reservation.is_some())
                || old.request.is_some_and(|(a, r)| {
                    a != old.attempt_no || r == 0 || r > self.exchanges_started
                })
                || old
                    .request_nonce
                    .as_ref()
                    .is_some_and(|n| n.len() != 32 || !n.bytes().all(|c| c.is_ascii_hexdigit()))
                || old.reservation.as_ref().is_some_and(|r| {
                    r.identity != self.identity
                        || Some((r.attempt_no, r.request_seq)) != old.request
                        || old.request_nonce.as_ref() != Some(&r.request_nonce)
                        || r.bytes == 0
                        || r.bytes > MAX_WINDOW
                        || r.nonce.len() != 32
                        || !r.nonce.bytes().all(|c| c.is_ascii_hexdigit())
                        || r.window_seq > self.window_seq
                })
            {
                return Err("invalid interrupted attempt accounting".into());
            }
            previous = old.attempt_no;
        }
        if self.interrupted_attempts.len() > 4 {
            return Err("interrupted attempt capacity".into());
        }
        if let Some(bytes) = &self.original_request_json
            && (bytes.is_empty()
                || bytes.len() > 64 * 1024
                || hex::encode(sha2::Sha256::digest(bytes)) != self.identity.request_digest
                || serde_json::from_slice::<serde_json::Value>(bytes).is_err())
        {
            return Err("original operation request binding mismatch".into());
        }
        if local_io::digest(&self.policy)? != self.identity.policy_digest {
            return Err("operation policy digest mismatch".into());
        }
        if self
            .owner_binding
            .as_ref()
            .is_some_and(|v| v.len() != 64 || !v.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err("invalid native owner binding".into());
        }
        if self.api_version != API
            || self.identity.operation_id.len() != 32
            || !self
                .identity
                .operation_id
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
            || [
                &self.identity.request_digest,
                &self.identity.policy_digest,
                &self.identity.binding_digest,
            ]
            .into_iter()
            .any(|v| v.len() != 64 || !v.bytes().all(|c| c.is_ascii_hexdigit()))
            || !(1..=8 * 1024 * 1024 * 1024).contains(&self.policy.max_encoded_io_bytes)
            || !(1..=262_660).contains(&self.policy.max_exchanges)
            || self.attempts_used > self.policy.max_attempts
            || self
                .active_attempt_nonce
                .as_ref()
                .is_some_and(|v| v.len() != 32 || !v.bytes().all(|c| c.is_ascii_hexdigit()))
            || self.tx_credit > MAX_REQUEST_BODY
            || self.rx_credit > MAX_WINDOW
            || (self.active_request.is_none()
                && (self.tx_credit != 0
                    || self.rx_credit != 0
                    || self.active_request_nonce.is_some()))
            || self.active_request.is_some() != self.active_request_nonce.is_some()
            || self
                .active_request_nonce
                .as_ref()
                .is_some_and(|v| v.len() != 32 || !v.bytes().all(|c| c.is_ascii_hexdigit()))
            || self.proof_pages > self.policy.max_proof_pages
            || self.proof_bytes > self.policy.max_proof_bytes
            || self.verified_target_json.as_ref().is_some_and(|v| {
                v.is_empty()
                    || v.len() > 64 * 1024
                    || serde_json::from_slice::<serde_json::Value>(v).is_err()
            })
            || self.material_limit > 512 * 1024 * 1024
            || self.material_bytes > self.material_limit
            || self.materials.len() > 65_536
            || self.materials.iter().any(|(digest, bytes)| {
                *bytes == 0 || digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit())
            })
            || self
                .materials
                .values()
                .try_fold(0u64, |sum, n| sum.checked_add(*n))
                != Some(self.material_bytes)
            || self.exchanges_started > self.policy.max_exchanges
            || self.charged()? > self.policy.max_encoded_io_bytes
        {
            return Err("invalid full-sync operation accounting".into());
        }
        if let Some((attempt, request)) = self.active_request
            && (attempt == 0
                || attempt != self.attempts_used
                || self.active_attempt_nonce.is_none()
                || request != self.exchanges_started)
        {
            return Err("invalid active exchange identity".into());
        }
        if let Some(r) = &self.reservation
            && (r.identity != self.identity
                || r.nonce.len() != 32
                || !r.nonce.bytes().all(|c| c.is_ascii_hexdigit())
                || Some((r.attempt_no, r.request_seq)) != self.active_request
                || self.active_request_nonce.as_ref() != Some(&r.request_nonce)
                || r.window_seq != self.window_seq
                || r.bytes == 0
                || r.bytes > MAX_WINDOW)
        {
            return Err("invalid outstanding body reservation".into());
        }
        Ok(())
    }
}

/// Constructed only after durable attempt admission; Instant is never serialized.
pub(super) struct AttemptPermit {
    identity: Identity,
    number: u64,
    nonce: String,
    deadline: Instant,
}
impl AttemptPermit {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}

/// An original exchange identity, not a pair of reusable integer counters.
pub(super) struct ExchangePermit {
    identity: Identity,
    attempt_nonce: String,
    attempt_no: u64,
    request_seq: u64,
    nonce: String,
}

/// The process owner must already hold the original node's ownership lease.
/// This additional file lock serializes local ledger mutations only.
pub(super) struct Ledger {
    path: PathBuf,
    _lock: local_io::WriterLock,
    state: Snapshot,
    poisoned: bool,
    recovered: bool,
    reopen_deadline: Option<Instant>,
    owner: Option<std::sync::Arc<super::super::process::ProcessOwnership>>,
}

impl Ledger {
    /// Explicit first initialization only; never called as an open fallback.
    pub fn create(path: &Path, identity: Identity, policy: Policy) -> Result<Self, String> {
        Self::create_with_material_limit(path, identity, policy, 0)
    }

    pub fn create_with_material_limit(
        path: &Path,
        identity: Identity,
        policy: Policy,
        material_limit: u64,
    ) -> Result<Self, String> {
        let lock = Self::lock(path)?;
        if path.try_exists().map_err(|e| e.to_string())? {
            return Err("operation ledger already exists; reopen its original identity".into());
        }
        let state = Snapshot {
            api_version: API.into(),
            identity,
            owner_binding: None,
            revision: 0,
            policy,
            encoded_tx_actual: 0,
            encoded_rx_actual: 0,
            unknown_final_charged: 0,
            exchanges_started: 0,
            original_request_json: None,
            verified_target_json: None,
            publication: None,
            publication_journal_version: Some(1),
            proof_pages: 0,
            proof_bytes: 0,
            material_limit,
            material_bytes: 0,
            materials: Default::default(),
            window_seq: 0,
            attempts_used: 0,
            interrupted_attempts: Vec::new(),
            active_attempt_nonce: None,
            active_request: None,
            active_request_nonce: None,
            tx_credit: 0,
            rx_credit: 0,
            reservation: None,
        };
        state.validate()?;
        // Failure may have published the file. The caller must reconcile it,
        // not retry create or generate another operation identity.
        local_io::write(path, &state)?;
        let actual: Snapshot = local_io::read(path)?;
        if actual != state {
            return Err("operation initialization readback mismatch".into());
        }
        Ok(Self {
            path: path.into(),
            _lock: lock,
            state,
            poisoned: false,
            recovered: false,
            reopen_deadline: None,
            owner: None,
        })
    }

    /// Bounded observation of the original ledger only. This does not authorize
    /// resuming an interrupted attempt or reclaiming an unresolved reservation.
    pub fn open(
        path: &Path,
        expected: &Identity,
        policy: &Policy,
        material_limit: u64,
    ) -> Result<Self, String> {
        policy.validate()?;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(policy.reopen_timeout_ms))
            .ok_or("operation reopen deadline overflow")?;
        let lock = Self::lock(path)?;
        let state: Snapshot = local_io::read(path)?;
        state.validate()?;
        if &state.policy != policy
            || &state.identity != expected
            || state.material_limit != material_limit
        {
            return Err("operation identity differs from original request".into());
        }
        // A synchronous filesystem call may return late. Never turn its late
        // completion into successful recovery or manufacture a fresh deadline.
        if Instant::now() >= deadline {
            return Err("operation_reopen_expired".into());
        }
        Ok(Self {
            path: path.into(),
            _lock: lock,
            state,
            poisoned: false,
            recovered: true,
            reopen_deadline: Some(deadline),
            owner: None,
        })
    }

    fn lock(path: &Path) -> Result<local_io::WriterLock, String> {
        let lock_path = path.with_extension("lock");
        local_io::WriterLock::try_lock(local_io::private_file(&lock_path)?)
            .map_err(|e| format!("operation ledger writer unavailable: {e}"))
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.state
    }

    fn save(&mut self, next: Snapshot) -> Result<(), String> {
        if self.recovered {
            return Err("operation_recovery_required: reopened handle is observation-only until explicit fenced attempt admission".into());
        }
        if self.poisoned {
            return Err("operation_recovery_required: uncertain prior persistence".into());
        }
        self.save_exact(next)
    }

    fn save_exact(&mut self, mut next: Snapshot) -> Result<(), String> {
        if let Some(owner) = &self.owner {
            owner.require_current_directory()?;
        }
        // Check the complete predecessor, not merely a revision supplied by
        // the caller. Unknown responses poison this handle before any I/O.
        self.poisoned = true;
        let old: Snapshot = local_io::read(&self.path)?;
        if old != self.state {
            return Err("operation_recovery_required: ledger predecessor changed".into());
        }
        next.revision = old
            .revision
            .checked_add(1)
            .ok_or("operation revision overflow")?;
        next.validate()?;
        local_io::write(&self.path, &next)?;
        let actual: Snapshot = local_io::read(&self.path)?;
        if actual != next {
            return Err("operation_recovery_required: persistence readback mismatch".into());
        }
        if let Some(owner) = &self.owner {
            owner.require_current_directory()?;
        }
        self.state = next;
        self.poisoned = false;
        Ok(())
    }

    /// Bind and retain the complete original request before first admission.
    /// Hash-only ledgers remain useful for observing old experimental state,
    /// but cannot be attached to the shared operation transport.
    pub fn bind_original_request<T: Serialize>(&mut self, request: &T) -> Result<(), String> {
        if self.recovered || self.poisoned {
            return Err("operation_recovery_required: request binding unavailable".into());
        }
        let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        if bytes.is_empty()
            || bytes.len() > 64 * 1024
            || hex::encode(sha2::Sha256::digest(&bytes)) != self.state.identity.request_digest
        {
            return Err("original operation request binding mismatch".into());
        }
        if let Some(original) = &self.state.original_request_json {
            return if original == &bytes {
                Ok(())
            } else {
                Err("original operation request changed".into())
            };
        }
        if self.state.attempts_used != 0 || self.state.active_attempt_nonce.is_some() {
            return Err("original request must precede attempt admission".into());
        }
        let mut next = self.state.clone();
        next.original_request_json = Some(bytes);
        self.save(next)
    }

    /// Freeze native ownership before the first attempt. Old unbound ledgers
    /// cannot be upgraded during recovery or after any network admission.
    pub(super) fn bind_owner(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
    ) -> Result<(), String> {
        if self.recovered
            || self.poisoned
            || self.state.attempts_used != 0
            || self.state.owner_binding.is_some()
        {
            return Err("operation_recovery_required: owner binding must precede execution".into());
        }
        self.require_owner_path(&owner, data)?;
        let mut next = self.state.clone();
        next.owner_binding = Some(owner.binding_digest()?);
        self.owner = Some(owner);
        self.save(next)
    }

    fn require_owner_path(
        &self,
        owner: &super::super::process::ProcessOwnership,
        data: &Path,
    ) -> Result<(), String> {
        owner.require_directory(data)?;
        local_io::reject_symlinks(&self.path)?;
        local_io::reject_symlinks(&data.join("state/full-sync"))?;
        if self
            .path
            .parent()
            .ok_or("ledger parent missing")?
            .canonicalize()
            .map_err(|e| e.to_string())?
            != data
                .join("state/full-sync")
                .canonicalize()
                .map_err(|e| e.to_string())?
        {
            return Err("operation belongs to a different native directory".into());
        }
        Ok(())
    }

    /// Admit only inspection/initialization of the original pre-target CAS.
    /// This must run before Session construction can stabilize or prune roots.
    pub(super) fn guard_pre_target_initialization(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
    ) -> Result<(), String> {
        self.guard_initialization(owner, data, false)
    }

    pub(super) fn guard_selected_initialization(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
    ) -> Result<(), String> {
        self.guard_initialization(owner, data, true)
    }

    fn guard_initialization(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        selected: bool,
    ) -> Result<(), String> {
        let deadline = self.reopen_deadline.ok_or("not a reopened operation")?;
        if !self.recovered || self.poisoned || Instant::now() >= deadline {
            return Err("operation_recovery_required: initialization unavailable".into());
        }
        self.require_owner_path(&owner, data)?;
        if self.state.owner_binding.as_ref() != Some(&owner.binding_digest()?)
            || self.state.publication_journal_version != Some(1)
            || self.state.publication.is_some()
            || self.state.verified_target_json.is_some() != selected
            || self.state.original_request_json.is_none()
        {
            return Err("operation_recovery_required: publication/target must be reconciled before CAS initialization".into());
        }
        if self.state.attempts_used >= self.state.policy.max_attempts {
            return Err("operation_budget_exhausted: attempts before CAS initialization".into());
        }
        // Keep the native lock alive across all constructor side effects.
        self.owner = Some(owner);
        Ok(())
    }

    /// Narrow recovery before a target was selected. The caller must first
    /// reconstruct the original request from its verified local session. This
    /// never recovers an installed/selected target or authorizes a new cut.
    pub(super) fn resume_before_target(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        request: &serde_json::Value,
    ) -> Result<AttemptPermit, String> {
        self.resume_original(owner, data, request, None)
    }

    pub(super) fn resume_selected_target(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        request: &serde_json::Value,
        verified_target: &[u8],
    ) -> Result<AttemptPermit, String> {
        self.resume_original(owner, data, request, Some(verified_target))
    }

    fn resume_original(
        &mut self,
        owner: std::sync::Arc<super::super::process::ProcessOwnership>,
        data: &Path,
        request: &serde_json::Value,
        verified_target: Option<&[u8]>,
    ) -> Result<AttemptPermit, String> {
        let deadline = self.reopen_deadline.ok_or("not a reopened operation")?;
        if !self.recovered || self.poisoned || Instant::now() >= deadline {
            return Err("operation_recovery_required: reopen unavailable or expired".into());
        }
        self.require_owner_path(&owner, data)?;
        if self.state.owner_binding.as_ref() != Some(&owner.binding_digest()?) {
            return Err(
                "operation_recovery_required: original native owner binding unavailable".into(),
            );
        }
        let expected_parent = data.join("state/full-sync");
        local_io::reject_symlinks(&self.path)?;
        if self
            .path
            .parent()
            .ok_or("ledger parent missing")?
            .canonicalize()
            .map_err(|e| e.to_string())?
            != expected_parent.canonicalize().map_err(|e| e.to_string())?
            || self.state.original_request_json.as_ref()
                != Some(&serde_json::to_vec(request).map_err(|e| e.to_string())?)
            || self.state.verified_target_json.as_deref() != verified_target
            || self.state.publication_journal_version != Some(1)
            || self.state.publication.is_some()
        {
            return Err(
                "operation_recovery_required: original pre-target continuity unavailable".into(),
            );
        }
        if self.state.attempts_used >= self.state.policy.max_attempts {
            return Err("operation_budget_exhausted: attempts".into());
        }
        let mut next = self.state.clone();
        if let Some(nonce) = next.active_attempt_nonce.take() {
            next.interrupted_attempts.push(InterruptedAttempt {
                attempt_no: next.attempts_used,
                attempt_nonce: nonce,
                request: next.active_request.take(),
                request_nonce: next.active_request_nonce.take(),
                tx_credit: std::mem::take(&mut next.tx_credit),
                rx_credit: std::mem::take(&mut next.rx_credit),
                reservation: next.reservation.take(),
            });
        }
        next.attempts_used += 1;
        let nonce = local_io::random_id()?;
        next.active_attempt_nonce = Some(nonce.clone());
        if next.charged()? != self.state.charged()? {
            return Err("recovery changed charged budget".into());
        }
        owner.require_directory(data)?;
        if Instant::now() >= deadline {
            return Err("operation_reopen_expired".into());
        }
        // Retain the actual native lifetime fence before the only durable CAS.
        self.owner = Some(owner);
        self.save_exact(next)?;
        if Instant::now() >= deadline {
            self.poisoned = true;
            return Err("operation_reopen_expired".into());
        }
        self.recovered = false;
        self.reopen_deadline = None;
        Ok(AttemptPermit {
            identity: self.state.identity.clone(),
            number: self.state.attempts_used,
            nonce,
            deadline: Instant::now()
                .checked_add(Duration::from_millis(self.state.policy.attempt_timeout_ms))
                .ok_or("attempt deadline overflow")?,
        })
    }

    pub fn begin_attempt(&mut self) -> Result<AttemptPermit, String> {
        if self.recovered
            || self.state.publication.is_some()
            || self.state.active_attempt_nonce.is_some()
            || self.state.active_request.is_some()
        {
            return Err(
                "operation_recovery_required: old attempt requires explicit recovery".into(),
            );
        }
        if self.state.attempts_used == self.state.policy.max_attempts {
            return Err("operation_budget_exhausted: attempts".into());
        }
        let mut next = self.state.clone();
        next.attempts_used += 1;
        let nonce = local_io::random_id()?;
        next.active_attempt_nonce = Some(nonce.clone());
        self.save(next)?;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(self.state.policy.attempt_timeout_ms))
            .ok_or("attempt deadline overflow")?;
        Ok(AttemptPermit {
            identity: self.state.identity.clone(),
            number: self.state.attempts_used,
            nonce,
            deadline,
        })
    }

    pub(super) fn check_attempt(&self, permit: &AttemptPermit) -> Result<(), String> {
        if let Some(owner) = &self.owner {
            owner.require_current_directory()?;
        }
        if self.recovered
            || self.poisoned
            || permit.identity != self.state.identity
            || permit.number != self.state.attempts_used
            || self.state.active_attempt_nonce.as_ref() != Some(&permit.nonce)
        {
            return Err("operation_recovery_required: stale attempt permit".into());
        }
        if Instant::now() >= permit.deadline {
            return Err("attempt_expired".into());
        }
        Ok(())
    }

    pub fn finish_attempt(&mut self, permit: &AttemptPermit) -> Result<(), String> {
        // Expiry terminates rather than renews an attempt; only exact identity,
        // no live I/O, and no outstanding reservation authorize this transition.
        if self.recovered
            || self.poisoned
            || permit.identity != self.state.identity
            || permit.number != self.state.attempts_used
            || self.state.active_attempt_nonce.as_ref() != Some(&permit.nonce)
            || self.state.active_request.is_some()
            || self.state.reservation.is_some()
        {
            return Err("operation_recovery_required: attempt cannot terminate".into());
        }
        let mut next = self.state.clone();
        next.active_attempt_nonce = None;
        self.save(next)
    }

    pub fn bind_verified_target(
        &mut self,
        permit: &AttemptPermit,
        target_json: &[u8],
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        if target_json.is_empty() || target_json.len() > 64 * 1024 {
            return Err("operation target record exceeds supported bound".into());
        }
        serde_json::from_slice::<serde_json::Value>(target_json).map_err(|e| e.to_string())?;
        if let Some(original) = &self.state.verified_target_json {
            return if original == target_json {
                Ok(())
            } else {
                Err("operation target differs from original verified record".into())
            };
        }
        let mut next = self.state.clone();
        next.verified_target_json = Some(target_json.to_vec());
        self.save(next)
    }

    /// Persist before changing loaded-world pins. Even an identical existing
    /// intent is not replay permission: its external effect may already exist.
    pub fn begin_publication(
        &mut self,
        permit: &AttemptPermit,
        cut: &serde_json::Value,
        roots_digest: &str,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        if self.state.publication_journal_version != Some(1)
            || self.state.publication.is_some()
            || self.state.active_request.is_some()
            || self.state.reservation.is_some()
            || roots_digest.len() != 64
            || !roots_digest.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("operation_recovery_required: publication unavailable".into());
        }
        let target: serde_json::Value = serde_json::from_slice(
            self.state
                .verified_target_json
                .as_ref()
                .ok_or("publication has no selected target")?,
        )
        .map_err(|e| e.to_string())?;
        if target
            .as_array()
            .is_none_or(|parts| parts.len() != 2 || parts.first() != Some(cut))
        {
            return Err("publication differs from the original selected cut".into());
        }
        let mut next = self.state.clone();
        next.publication = Some(Publication {
            intent_digest: local_io::digest(&(cut, roots_digest))?,
            attempt_no: permit.number,
            completed: false,
        });
        self.save(next)
    }

    /// Only the same live attempt may record a positively acknowledged pin
    /// publication. Reopen never turns an unknown publication into success.
    pub fn complete_publication(
        &mut self,
        permit: &AttemptPermit,
        cut: &serde_json::Value,
        roots_digest: &str,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        let mut next = self.state.clone();
        let publication = next
            .publication
            .as_mut()
            .ok_or("missing publication intent")?;
        if publication.completed
            || publication.attempt_no != permit.number
            || publication.intent_digest != local_io::digest(&(cut, roots_digest))?
        {
            return Err("publication completion differs from original intent".into());
        }
        publication.completed = true;
        self.save(next)
    }

    pub fn charge_proof(&mut self, permit: &AttemptPermit, bytes: u64) -> Result<(), String> {
        self.check_attempt(permit)?;
        if bytes == 0 || bytes > 4 * 1024 * 1024 {
            return Err("invalid canonical proof size".into());
        }
        let mut next = self.state.clone();
        next.proof_pages = next
            .proof_pages
            .checked_add(1)
            .ok_or("proof page overflow")?;
        next.proof_bytes = next
            .proof_bytes
            .checked_add(bytes)
            .ok_or("proof bytes overflow")?;
        if next.proof_pages > next.policy.max_proof_pages
            || next.proof_bytes > next.policy.max_proof_bytes
        {
            return Err("operation_budget_exhausted: proof".into());
        }
        self.save(next)
    }

    pub fn charge_material(
        &mut self,
        permit: &AttemptPermit,
        digest: &str,
        bytes: u64,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) || bytes == 0 {
            return Err("invalid verified material identity".into());
        }
        if let Some(old) = self.state.materials.get(digest) {
            return if *old == bytes {
                Ok(())
            } else {
                Err("material size changed for original digest".into())
            };
        }
        let mut next = self.state.clone();
        next.material_bytes = next
            .material_bytes
            .checked_add(bytes)
            .ok_or("material bytes overflow")?;
        if next.material_bytes > next.material_limit || next.materials.len() >= 65_536 {
            return Err("operation_budget_exhausted: material".into());
        }
        next.materials.insert(digest.into(), bytes);
        self.save(next)
    }

    /// Called after encoding/preflight and immediately before connection I/O.
    pub fn begin_exchange(
        &mut self,
        permit: &AttemptPermit,
        tx_credit: u64,
        rx_credit: u64,
    ) -> Result<ExchangePermit, String> {
        self.check_attempt(permit)?;
        let attempt_no = permit.number;
        if self.state.active_request.is_some() || self.state.reservation.is_some() {
            return Err("operation_recovery_required: previous exchange has not ended".into());
        }
        if !(1..=4).contains(&attempt_no)
            || self.state.exchanges_started == self.state.policy.max_exchanges
            || self.state.charged()? == self.state.policy.max_encoded_io_bytes
        {
            return Err("operation_budget_exhausted: exchanges".into());
        }
        if tx_credit > MAX_REQUEST_BODY
            || rx_credit > MAX_WINDOW
            || (tx_credit == 0 && rx_credit == 0)
        {
            return Err("invalid initial HTTP body credit".into());
        }
        let remaining = self
            .state
            .policy
            .max_encoded_io_bytes
            .checked_sub(self.state.charged()?)
            .ok_or("invalid charged body credit")?;
        let rx_available = remaining
            .checked_sub(tx_credit)
            .ok_or("operation_budget_exhausted: initial HTTP body credit")?;
        if rx_credit > 0 && rx_available == 0 {
            return Err("operation_budget_exhausted: initial HTTP body credit".into());
        }
        let rx_credit = rx_credit.min(rx_available);
        let initial = tx_credit
            .checked_add(rx_credit)
            .ok_or("initial credit overflow")?;
        if self
            .state
            .charged()?
            .checked_add(initial)
            .ok_or("initial credit overflow")?
            > self.state.policy.max_encoded_io_bytes
        {
            return Err("operation_budget_exhausted: initial HTTP body credit".into());
        }
        let mut next = self.state.clone();
        next.exchanges_started = next
            .exchanges_started
            .checked_add(1)
            .ok_or("exchange overflow")?;
        let seq = next.exchanges_started;
        let nonce = local_io::random_id()?;
        next.active_request = Some((attempt_no, seq));
        next.active_request_nonce = Some(nonce.clone());
        next.tx_credit = tx_credit;
        next.rx_credit = rx_credit;
        self.save(next)?;
        Ok(ExchangePermit {
            identity: self.state.identity.clone(),
            attempt_nonce: permit.nonce.clone(),
            attempt_no,
            request_seq: seq,
            nonce,
        })
    }

    pub fn body_window_limit(&self, direction: Direction, desired: usize) -> Result<usize, String> {
        let unused = self
            .state
            .policy
            .max_encoded_io_bytes
            .checked_sub(self.state.charged()?)
            .ok_or("invalid charged body credit")?;
        let credit = match direction {
            Direction::Tx => self.state.tx_credit,
            Direction::Rx => self.state.rx_credit,
        };
        let available = unused
            .checked_add(credit)
            .ok_or("body window credit overflow")?;
        let size = (desired as u64).min(MAX_WINDOW).min(available);
        if size == 0 {
            return Err("operation_budget_exhausted: encoded body window".into());
        }
        Ok(size as usize)
    }

    pub fn reserve(
        &mut self,
        permit: &AttemptPermit,
        request: &ExchangePermit,
        direction: Direction,
        bytes: u64,
    ) -> Result<Reservation, String> {
        self.check_attempt(permit)?;
        let attempt = permit.number;
        self.check_exchange(request)?;
        if self.state.active_request != Some((attempt, request.request_seq))
            || self.state.reservation.is_some()
        {
            return Err("operation_recovery_required: wrong or unresolved exchange".into());
        }
        if bytes == 0 || bytes > MAX_WINDOW {
            return Err("invalid body I/O window".into());
        }
        let available = match direction {
            Direction::Tx => self.state.tx_credit,
            Direction::Rx => self.state.rx_credit,
        };
        let additional = bytes.saturating_sub(available);
        if self
            .state
            .charged()?
            .checked_add(additional)
            .ok_or("body credit overflow")?
            > self.state.policy.max_encoded_io_bytes
        {
            return Err("operation_budget_exhausted: encoded body bytes".into());
        }
        let mut next = self.state.clone();
        match direction {
            Direction::Tx => next.tx_credit = next.tx_credit.saturating_sub(bytes),
            Direction::Rx => next.rx_credit = next.rx_credit.saturating_sub(bytes),
        };
        next.window_seq = next
            .window_seq
            .checked_add(1)
            .ok_or("window sequence overflow")?;
        let token = Reservation {
            identity: self.state.identity.clone(),
            nonce: local_io::random_id()?,
            attempt_no: attempt,
            request_seq: request.request_seq,
            request_nonce: request.nonce.clone(),
            window_seq: next.window_seq,
            direction,
            bytes,
        };
        next.reservation = Some(token.clone());
        self.save(next)?;
        Ok(token)
    }

    /// `actual` must come from the matching controlled Write/Read return.
    /// Crash/unknown calls have no such return and must NOT call this method.
    pub fn settle(&mut self, token: &Reservation, actual: u64) -> Result<(), String> {
        if self.state.reservation.as_ref() != Some(token) || actual > token.bytes {
            return Err("operation_recovery_required: mismatched settlement".into());
        }
        let mut next = self.state.clone();
        let counter = match token.direction {
            Direction::Tx => &mut next.encoded_tx_actual,
            Direction::Rx => &mut next.encoded_rx_actual,
        };
        *counter = counter.checked_add(actual).ok_or("body counter overflow")?;
        next.reservation = None;
        self.save(next)
    }

    fn check_exchange(&self, request: &ExchangePermit) -> Result<(), String> {
        if request.identity != self.state.identity
            || self.state.active_attempt_nonce.as_ref() != Some(&request.attempt_nonce)
            || self.state.active_request != Some((request.attempt_no, request.request_seq))
            || self.state.active_request_nonce.as_ref() != Some(&request.nonce)
        {
            return Err("operation_recovery_required: stale exchange permit".into());
        }
        Ok(())
    }

    /// Only after the controlled socket/worker has actually terminated.
    /// Does not resolve an unknown byte reservation on a reopened handle.
    pub fn finish_exchange(
        &mut self,
        permit: &AttemptPermit,
        request: &ExchangePermit,
    ) -> Result<(), String> {
        self.check_exchange(request)?;
        if self.recovered
            || self.poisoned
            || permit.identity != self.state.identity
            || permit.number != self.state.attempts_used
            || self.state.active_attempt_nonce.as_ref() != Some(&permit.nonce)
            || self.state.active_request != Some((permit.number, request.request_seq))
            || self.state.reservation.is_some()
        {
            return Err(
                "operation_recovery_required: outstanding exchange or body reservation".into(),
            );
        }
        let mut next = self.state.clone();
        next.active_request = None;
        next.active_request_nonce = None;
        next.tx_credit = 0;
        next.rx_credit = 0;
        self.save(next)
    }
}
