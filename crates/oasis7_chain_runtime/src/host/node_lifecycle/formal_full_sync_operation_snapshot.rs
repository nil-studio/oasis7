//! Shared durable operation record and bounded read-only verification.
use super::super::local_io;
use super::operation_config::FormalFullSyncTransferPolicyV1 as Policy;
use super::operation_terminal::{AttemptOwner, Installation, TerminalIndex};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::Path;

pub(super) const MAX_LEDGER_JSON: u64 = 16 * 1024 * 1024;

pub(super) fn read_snapshot(path: &Path) -> Result<Snapshot, String> {
    super::super::super::controlled_live_config::require_private_owned(path, false)?;
    let snapshot: Snapshot = local_io::read_bounded_json(path, MAX_LEDGER_JSON)?;
    snapshot.validate()?;
    Ok(snapshot)
}

pub(super) const API: &str = "oasis7.formal-full-sync-operation-ledger/v1";
pub(super) const MAX_WINDOW: u64 = 64 * 1024;
pub(super) const MAX_REQUEST_BODY: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub operation_id: String,
    pub request_digest: String,
    pub policy_digest: String,
    pub binding_digest: String,
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

/// Durable local CAS publication intent, not World installation or readiness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Publication {
    pub intent_digest: String,
    #[serde(default)]
    pub installation_input_digest: Option<String>,
    pub attempt_no: u64,
    pub completed: bool,
}

/// An interrupted attempt retains unresolved credits under its original identity.
/// These bytes are neither confirmed actual nor a refundable balance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InterruptedAttempt {
    pub(super) attempt_no: u64,
    pub(super) attempt_nonce: String,
    pub(super) request: Option<(u64, u64)>,
    pub(super) request_nonce: Option<String>,
    pub(super) tx_credit: u64,
    pub(super) rx_credit: u64,
    pub(super) reservation: Option<Reservation>,
    #[serde(default)]
    pub(super) owner: Option<AttemptOwner>,
}
impl InterruptedAttempt {
    fn charged(&self) -> Option<u64> {
        self.tx_credit
            .checked_add(self.rx_credit)?
            .checked_add(self.reservation.as_ref().map_or(0, |r| r.bytes))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub(super) api_version: String,
    pub identity: Identity,
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
    #[serde(default)]
    pub installation: Option<Installation>,
    #[serde(default)]
    pub terminal_index: Option<TerminalIndex>,
    #[serde(default)]
    pub attempt_owner: Option<AttemptOwner>,
    #[serde(default)]
    pub repair_attempt: bool,
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
    pub(super) fn unresolved_reserved(&self) -> Result<u64, String> {
        let active = self
            .tx_credit
            .checked_add(self.rx_credit)
            .and_then(|bytes| bytes.checked_add(self.reservation.as_ref().map_or(0, |r| r.bytes)));
        self.interrupted_attempts
            .iter()
            .try_fold(
                active.ok_or("operation reservation accounting overflow")?,
                |total, old| total.checked_add(old.charged()?),
            )
            .ok_or_else(|| "operation reservation accounting overflow".into())
    }

    pub(super) fn charged(&self) -> Result<u64, String> {
        let reserved = self.unresolved_reserved()?;
        self.encoded_tx_actual
            .checked_add(self.encoded_rx_actual)
            .and_then(|n| n.checked_add(self.unknown_final_charged))
            .and_then(|n| n.checked_add(reserved))
            .ok_or_else(|| "operation accounting overflow".into())
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        self.policy.validate()?;
        if let Some(index) = &self.terminal_index {
            index.validate()?;
        }
        for owner in self.attempt_owner.iter().chain(
            self.interrupted_attempts
                .iter()
                .filter_map(|a| a.owner.as_ref()),
        ) {
            if owner.generation == 0
                || owner.incarnation.len() != 32
                || !owner.incarnation.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("invalid operation attempt owner".into());
            }
        }
        if let Some(installation) = &self.installation {
            let manifest: oasis7_client_api::world_service_full_sync::FormalFullSyncManifestV1 =
                serde_json::from_value(installation.manifest.clone()).map_err(|e| e.to_string())?;
            manifest.validate()?;
            let target: serde_json::Value = serde_json::from_slice(
                self.verified_target_json
                    .as_deref()
                    .ok_or("installation has no original target")?,
            )
            .map_err(|e| e.to_string())?;
            if serde_json::to_value(&manifest.cut).map_err(|e| e.to_string())? != target[0]
                || installation.retained_cas_hashes.is_empty()
                || installation
                    .retained_cas_hashes
                    .iter()
                    .any(|hash| !self.materials.contains_key(hash))
                || serde_json::to_vec(&installation.manifest)
                    .map_err(|e| e.to_string())?
                    .len()
                    > 8 * 1024 * 1024
            {
                return Err("installation differs from original verified material".into());
            }
            if let Some(publication) = &self.publication {
                let roots = local_io::digest(&installation.retained_cas_hashes)?;
                if publication.intent_digest != local_io::digest(&(&target[0], &roots))?
                    || publication.installation_input_digest.as_deref()
                        != Some(
                            local_io::digest(&(
                                &installation.manifest,
                                &installation.retained_cas_hashes,
                            ))?
                            .as_str(),
                        )
                {
                    return Err(
                        "operation_recovery_required: original installation intent changed".into(),
                    );
                }
            }
            let installation_digest = local_io::digest(installation)?;
            if installation.completed
                && (self.active_attempt_nonce.is_some()
                    || self.active_request.is_some()
                    || self.unresolved_reserved()? != 0
                    || !self.publication.as_ref().is_some_and(|p| p.completed)
                    || !self
                        .terminal_index
                        .as_ref()
                        .and_then(|i| i.summaries.last())
                        .is_some_and(|s| {
                            s.identity == self.identity
                                && s.target == target[0]
                                && s.encoded_tx_actual == self.encoded_tx_actual
                                && s.encoded_rx_actual == self.encoded_rx_actual
                                && s.unknown_final_charged == self.unknown_final_charged
                                && s.exchanges_started == self.exchanges_started
                                && s.attempts_used == self.attempts_used
                                && s.proof_pages == self.proof_pages
                                && s.proof_bytes == self.proof_bytes
                                && s.material_bytes == self.material_bytes
                                && s.installation_digest == installation_digest
                                && s.history_manifest_digest == installation.history_manifest_digest
                        }))
            {
                return Err("completed installation lacks its exact terminal accounting".into());
            }
        }
        if self
            .publication_journal_version
            .is_some_and(|version| version != 1)
        {
            return Err("unsupported publication journal version".into());
        }
        if let Some(publication) = &self.publication
            && (self.publication_journal_version != Some(1)
                || self.verified_target_json.is_none()
                // Publication is written only once the preceding request and
                // reservation have ended; a cold ledger must preserve that
                // ordering rather than admitting an invented later exchange.
                || (!self.repair_attempt && (self.active_request.is_some() || self.reservation.is_some()))
                || publication.attempt_no == 0
                || publication.attempt_no > self.attempts_used
                || publication.intent_digest.len() != 64
                || !publication
                    .intent_digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit()))
        {
            return Err("invalid operation publication identity".into());
        }
        if self.repair_attempt
            && (self.attempts_used == 0
                || self.publication.is_none()
                || self.installation.as_ref().is_none_or(|i| i.completed))
        {
            return Err("invalid original installation repair attempt".into());
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
                        || r.window_seq == 0
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
            // All charged or in-flight work is admitted by an attempt first.
            // In particular, a cold ledger with attempts_used=0 cannot have
            // an invented live nonce, exchange, material or proof progress.
            || (self.attempts_used == 0
                && (self.active_attempt_nonce.is_some()
                    || self.encoded_tx_actual != 0
                    || self.encoded_rx_actual != 0
                    || self.unknown_final_charged != 0
                    || self.exchanges_started != 0
                    || self.window_seq != 0
                    || self.proof_pages != 0
                    || self.proof_bytes != 0
                    || self.material_bytes != 0
                    || self.verified_target_json.is_some()))
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
            // charge_proof accepts a nonzero byte count and atomically
            // increments both counters; cold replay cannot invent an
            // unmatched page or charge without its original page.
            || self.proof_bytes < self.proof_pages
            || (self.proof_pages == 0) != (self.proof_bytes == 0)
            // Every settled encoded byte and window sequence is downstream
            // of a durably admitted exchange, not just of an attempt.
            || (self.exchanges_started == 0
                && (self.window_seq != 0
                    || self.encoded_tx_actual != 0
                    || self.encoded_rx_actual != 0
                    || self.unknown_final_charged != 0))
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
                // begin_exchange assigns one-based sequence numbers. An
                // invented request #0 must not survive durable cold reopen.
                || request == 0
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
                || r.window_seq == 0
                || r.window_seq != self.window_seq
                || r.bytes == 0
                || r.bytes > MAX_WINDOW)
        {
            return Err("invalid outstanding body reservation".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "formal_full_sync_operation_snapshot_tests.rs"]
mod tests;
