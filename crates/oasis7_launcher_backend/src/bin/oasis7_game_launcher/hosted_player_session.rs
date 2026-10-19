use super::hosted_access::{DeploymentMode, hosted_player_access_contract};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const HOSTED_PLAYER_SESSION_ISSUE_ROUTE: &str = "/api/public/player-session/issue";
pub(super) const HOSTED_PLAYER_SESSION_RELEASE_ROUTE: &str = "/api/public/player-session/release";
pub(super) const HOSTED_PLAYER_SESSION_ADMISSION_ROUTE: &str =
    "/api/public/player-session/admission";
pub(super) const HOSTED_PLAYER_SESSION_REFRESH_ROUTE: &str = "/api/public/player-session/refresh";
const ISSUE_WINDOW_MS: u64 = 60_000;
const PENDING_REGISTRATION_TTL_MS: u64 = 30_000;
const SLOT_LEASE_TTL_MS: u64 = 120_000;
#[path = "hosted_player_session_v2.rs"]
mod v2;
// Registration and runtime commits can straddle one snapshot; require a second
// successful absence before presence reconciliation revokes a seen lease.
const RUNTIME_MISSING_SNAPSHOT_CONFIRMATIONS: u8 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct HostedPlayerSessionReleaseTelemetry {
    pub(super) reason_counts: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_release_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_probe_sequence: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
enum SlotReleaseReason {
    ExplicitRelease,
    PendingExpiry,
    SeenExpiry,
    RuntimeAbsence,
    Reissue,
    PersistenceRollback,
}

impl SlotReleaseReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRelease => "explicit_release",
            Self::PendingExpiry => "pending_expiry",
            Self::SeenExpiry => "seen_expiry",
            Self::RuntimeAbsence => "runtime_absence",
            Self::Reissue => "reissue",
            Self::PersistenceRollback => "persistence_rollback",
        }
    }

    fn marks_runtime_revoked(self) -> bool {
        matches!(self, Self::RuntimeAbsence | Self::Reissue)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedPlayerSessionAdmissionSnapshot {
    pub(super) issue_rate_limit_per_minute: u64,
    pub(super) max_player_sessions: u64,
    pub(super) active_player_sessions: u64,
    pub(super) effective_player_sessions: u64,
    pub(super) runtime_bound_player_sessions: u64,
    pub(super) runtime_only_player_sessions: u64,
    pub(super) runtime_probe_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) runtime_probe_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_runtime_probe_unix_ms: Option<u64>,
    pub(super) slot_lease_ttl_ms: u64,
    pub(super) pending_registration_ttl_ms: u64,
    pub(super) issued_players_total: u64,
    pub(super) released_players_total: u64,
    pub(super) issued_in_current_window: u64,
    pub(super) remaining_issue_budget: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) release_telemetry: Option<HostedPlayerSessionReleaseTelemetry>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedPlayerSessionIssueGrant {
    pub(super) player_id: String,
    pub(super) device_session_id: String,
    pub(super) issued_at_unix_ms: u64,
    pub(super) auth_mode: String,
    pub(super) release_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) registration_grant: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedPlayerSessionIssueResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    pub(super) deployment_mode: String,
    pub(super) admission: HostedPlayerSessionAdmissionSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) grant: Option<HostedPlayerSessionIssueGrant>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedPlayerSessionReleaseResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    pub(super) deployment_mode: String,
    pub(super) admission: HostedPlayerSessionAdmissionSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedPlayerSessionAdmissionResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    pub(super) deployment_mode: String,
    pub(super) admission: HostedPlayerSessionAdmissionSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) registration_grant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) device_session_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HostedPlayerSessionLedger {
    next_sequence: u64,
    issued_players_total: u64,
    released_players_total: u64,
    issue_timestamps_unix_ms: VecDeque<u64>,
    #[serde(default)]
    release_telemetry: HostedPlayerSessionReleaseTelemetry,
    active_release_tokens_by_player: BTreeMap<String, String>,
    active_players_by_release_token: BTreeMap<String, String>,
    last_seen_unix_ms_by_release_token: BTreeMap<String, u64>,
    runtime_seen_players: BTreeSet<String>,
    runtime_revoked_players: BTreeSet<String>,
}

#[derive(Debug, Default, Clone)]
pub(super) struct HostedPlayerSessionIssuer {
    next_sequence: u64,
    issued_players_total: u64,
    released_players_total: u64,
    issue_timestamps_unix_ms: VecDeque<u64>,
    release_telemetry: HostedPlayerSessionReleaseTelemetry,
    active_release_tokens_by_player: BTreeMap<String, String>,
    active_players_by_release_token: BTreeMap<String, String>,
    last_seen_unix_ms_by_release_token: BTreeMap<String, u64>,
    last_observed_runtime_bound_player_sessions: u64,
    last_runtime_probe_unix_ms: Option<u64>,
    last_runtime_probe_error: Option<String>,
    last_runtime_probe_sequence: Option<u64>,
    last_runtime_active_players: BTreeSet<String>,
    runtime_seen_players: BTreeSet<String>,
    runtime_missing_snapshot_counts: BTreeMap<String, u8>,
    runtime_revoked_players: BTreeSet<String>,
    ledger_path: Option<PathBuf>,
}

impl HostedPlayerSessionIssuer {
    pub(super) fn with_ledger_path(path: PathBuf) -> Result<Self, String> {
        let ledger = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<HostedPlayerSessionLedger>(&bytes)
                .map_err(|err| format!("decode hosted session ledger failed: {err}"))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                HostedPlayerSessionLedger::default()
            }
            Err(err) => return Err(format!("read hosted session ledger failed: {err}")),
        };
        let mut issuer = Self {
            next_sequence: ledger.next_sequence,
            issued_players_total: ledger.issued_players_total,
            released_players_total: ledger.released_players_total,
            issue_timestamps_unix_ms: ledger.issue_timestamps_unix_ms,
            release_telemetry: ledger.release_telemetry,
            active_release_tokens_by_player: ledger.active_release_tokens_by_player,
            active_players_by_release_token: ledger.active_players_by_release_token,
            last_seen_unix_ms_by_release_token: ledger.last_seen_unix_ms_by_release_token,
            runtime_seen_players: ledger.runtime_seen_players,
            runtime_revoked_players: ledger.runtime_revoked_players,
            ledger_path: Some(path),
            ..Self::default()
        };
        issuer.prune_old_timestamps();
        issuer.prune_expired_slots();
        issuer.persist_ledger()?;
        Ok(issuer)
    }

    fn persist_ledger(&self) -> Result<(), String> {
        let Some(path) = self.ledger_path.as_deref() else {
            return Ok(());
        };
        let ledger = HostedPlayerSessionLedger {
            next_sequence: self.next_sequence,
            issued_players_total: self.issued_players_total,
            released_players_total: self.released_players_total,
            issue_timestamps_unix_ms: self.issue_timestamps_unix_ms.clone(),
            release_telemetry: self.release_telemetry.clone(),
            active_release_tokens_by_player: self.active_release_tokens_by_player.clone(),
            active_players_by_release_token: self.active_players_by_release_token.clone(),
            last_seen_unix_ms_by_release_token: self.last_seen_unix_ms_by_release_token.clone(),
            runtime_seen_players: self.runtime_seen_players.clone(),
            runtime_revoked_players: self.runtime_revoked_players.clone(),
        };
        atomic_write_json(path, &ledger)
    }

    #[cfg(test)]
    pub(super) fn observe_runtime_active_players<'a, I>(&mut self, active_players: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let probe_sequence = self
            .last_runtime_probe_sequence
            .unwrap_or_default()
            .saturating_add(1);
        self.observe_runtime_active_players_for_probe(probe_sequence, active_players);
    }

    pub(super) fn observe_runtime_active_players_for_probe<'a, I>(
        &mut self,
        probe_sequence: u64,
        active_players: I,
    ) where
        I: IntoIterator<Item = &'a str>,
    {
        if self.last_runtime_probe_sequence == Some(probe_sequence) {
            return;
        }
        self.last_runtime_probe_sequence = Some(probe_sequence);
        self.prune_old_timestamps();
        let runtime_active_players: BTreeSet<String> = active_players
            .into_iter()
            .map(str::trim)
            .filter(|player_id| !player_id.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let observed_at_unix_ms = now_unix_ms();
        self.last_runtime_active_players = runtime_active_players.clone();
        self.last_observed_runtime_bound_player_sessions = runtime_active_players.len() as u64;
        self.last_runtime_probe_unix_ms = Some(observed_at_unix_ms);
        self.last_runtime_probe_error = None;

        for player_id in &runtime_active_players {
            if let Some(release_token) =
                self.active_release_tokens_by_player.get(player_id).cloned()
            {
                self.runtime_seen_players.insert(player_id.clone());
                self.runtime_missing_snapshot_counts.remove(player_id);
                self.runtime_revoked_players.remove(player_id);
                self.last_seen_unix_ms_by_release_token
                    .insert(release_token, observed_at_unix_ms);
            }
        }
        self.prune_expired_slots();

        for player_id in self.runtime_seen_players.iter().filter(|player_id| {
            self.active_release_tokens_by_player
                .contains_key(player_id.as_str())
                && !runtime_active_players.contains(player_id.as_str())
        }) {
            let count = self
                .runtime_missing_snapshot_counts
                .entry(player_id.clone())
                .or_default();
            *count = count.saturating_add(1);
        }
        let stale_players: Vec<String> = self
            .runtime_seen_players
            .iter()
            .filter(|player_id| {
                self.active_release_tokens_by_player
                    .contains_key(player_id.as_str())
                    && !runtime_active_players.contains(player_id.as_str())
                    && self
                        .runtime_missing_snapshot_counts
                        .get(player_id.as_str())
                        .copied()
                        .unwrap_or_default()
                        >= RUNTIME_MISSING_SNAPSHOT_CONFIRMATIONS
            })
            .cloned()
            .collect();
        for player_id in stale_players {
            let _ = self.release_slot_for_player_with_reason(
                player_id.as_str(),
                SlotReleaseReason::RuntimeAbsence,
                Some(probe_sequence),
            );
        }
        let _ = self.persist_ledger();
    }

    pub(super) fn record_runtime_probe_failure(&mut self, error: String) {
        self.runtime_missing_snapshot_counts.clear();
        self.last_runtime_probe_unix_ms = Some(now_unix_ms());
        self.last_runtime_probe_error = Some(error);
    }

    pub(super) fn admission(
        &mut self,
        deployment_mode: DeploymentMode,
    ) -> HostedPlayerSessionAdmissionResponse {
        let contract = hosted_player_access_contract(deployment_mode);
        self.prune_old_timestamps();
        self.prune_expired_slots();
        HostedPlayerSessionAdmissionResponse {
            ok: true,
            error_code: None,
            error: None,
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: self.admission_snapshot(
                contract.admission.issue_rate_limit_per_minute,
                contract.admission.max_player_sessions,
            ),
            registration_grant: None,
            device_session_id: None,
        }
    }

    pub(super) fn refresh(
        &mut self,
        deployment_mode: DeploymentMode,
        player_id: &str,
        release_token: &str,
        registration_public_key: Option<&str>,
    ) -> HostedPlayerSessionAdmissionResponse {
        let contract = hosted_player_access_contract(deployment_mode);
        self.prune_old_timestamps();
        self.prune_expired_slots();
        let admission = self.admission_snapshot(
            contract.admission.issue_rate_limit_per_minute,
            contract.admission.max_player_sessions,
        );
        if deployment_mode != DeploymentMode::HostedPublicJoin {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("player_session_refresh_disabled".to_string()),
                error: Some(
                    "hosted player-session refresh is only available on hosted_public_join"
                        .to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        let token = release_token.trim();
        if token.is_empty() {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("release_token_required".to_string()),
                error: Some("release_token is required".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        let expected_player_id = player_id.trim();
        if expected_player_id.is_empty() {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("player_id_required".to_string()),
                error: Some("player_id is required".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        if self.runtime_revoked_players.contains(expected_player_id) {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("session_revoked".to_string()),
                error: Some(
                    "player session was revoked by runtime presence reconciliation".to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        let token_digest = release_token_digest(token);
        let Some(bound_player_id) = self
            .active_players_by_release_token
            .get(token_digest.as_str())
        else {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("release_token_invalid".to_string()),
                error: Some("release_token does not map to an active player slot".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        };
        if bound_player_id != expected_player_id {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("player_id_mismatch".to_string()),
                error: Some("player_id does not match the active slot owner".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        self.last_seen_unix_ms_by_release_token
            .insert(token_digest, now_unix_ms());
        let (registration_grant, device_session_id) = match registration_public_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(public_key) => {
                let issued_at_unix_ms = now_unix_ms();
                self.next_sequence = self.next_sequence.saturating_add(1);
                let device_session_id =
                    build_device_session_id(issued_at_unix_ms, self.next_sequence);
                match build_registration_grant(
                    expected_player_id,
                    public_key,
                    device_session_id.as_str(),
                    issued_at_unix_ms,
                ) {
                    Ok(grant) => (Some(grant), Some(device_session_id)),
                    Err(error) => {
                        return HostedPlayerSessionAdmissionResponse {
                            ok: false,
                            error_code: Some("registration_grant_issue_failed".to_string()),
                            error: Some(error),
                            deployment_mode: deployment_mode.as_str().to_string(),
                            admission,
                            registration_grant: None,
                            device_session_id: None,
                        };
                    }
                }
            }
            None => (None, None),
        };
        if let Err(error) = self.persist_ledger() {
            return HostedPlayerSessionAdmissionResponse {
                ok: false,
                error_code: Some("session_ledger_persist_failed".to_string()),
                error: Some(error),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                registration_grant: None,
                device_session_id: None,
            };
        }
        HostedPlayerSessionAdmissionResponse {
            ok: true,
            error_code: None,
            error: None,
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: self.admission_snapshot(
                contract.admission.issue_rate_limit_per_minute,
                contract.admission.max_player_sessions,
            ),
            registration_grant,
            device_session_id,
        }
    }

    #[cfg(test)]
    pub(super) fn issue(
        &mut self,
        deployment_mode: DeploymentMode,
    ) -> HostedPlayerSessionIssueResponse {
        self.issue_internal(deployment_mode, None, None)
    }

    pub(super) fn issue_with_key(
        &mut self,
        deployment_mode: DeploymentMode,
        public_key: &str,
    ) -> HostedPlayerSessionIssueResponse {
        self.issue_internal(deployment_mode, None, Some(public_key))
    }

    pub(super) fn issue_for_player(
        &mut self,
        deployment_mode: DeploymentMode,
        player_id: &str,
    ) -> HostedPlayerSessionIssueResponse {
        self.issue_internal(deployment_mode, Some(player_id), None)
    }

    pub(super) fn issue_for_player_and_key(
        &mut self,
        deployment_mode: DeploymentMode,
        player_id: &str,
        public_key: &str,
    ) -> HostedPlayerSessionIssueResponse {
        self.issue_internal(deployment_mode, Some(player_id), Some(public_key))
    }

    fn issue_internal(
        &mut self,
        deployment_mode: DeploymentMode,
        player_id_override: Option<&str>,
        registration_public_key: Option<&str>,
    ) -> HostedPlayerSessionIssueResponse {
        let contract = hosted_player_access_contract(deployment_mode);
        self.prune_old_timestamps();
        self.prune_expired_slots();
        let mut admission = self.admission_snapshot(
            contract.admission.issue_rate_limit_per_minute,
            contract.admission.max_player_sessions,
        );

        if deployment_mode != DeploymentMode::HostedPublicJoin {
            return HostedPlayerSessionIssueResponse {
                ok: false,
                error_code: Some("player_session_issue_disabled".to_string()),
                error: Some(
                    "hosted player-session issue is only available on hosted_public_join"
                        .to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                grant: None,
            };
        }

        if admission.issued_in_current_window >= admission.issue_rate_limit_per_minute {
            return HostedPlayerSessionIssueResponse {
                ok: false,
                error_code: Some("rate_limited".to_string()),
                error: Some(
                    "hosted player-session issue rate limit exceeded; retry in a minute"
                        .to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                grant: None,
            };
        }
        let issued_at_unix_ms = now_unix_ms();
        self.next_sequence = self.next_sequence.saturating_add(1);
        let player_id = player_id_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| build_player_id(issued_at_unix_ms, self.next_sequence));
        let _ = self.release_slot_for_player_with_reason(
            player_id.as_str(),
            SlotReleaseReason::Reissue,
            None,
        );
        admission = self.admission_snapshot(
            contract.admission.issue_rate_limit_per_minute,
            contract.admission.max_player_sessions,
        );
        let reassociating_runtime_player = self.last_runtime_active_players.contains(&player_id);
        if admission.effective_player_sessions >= admission.max_player_sessions
            && !reassociating_runtime_player
        {
            return HostedPlayerSessionIssueResponse {
                ok: false,
                error_code: Some("world_full".to_string()),
                error: Some(
                    "hosted player-session effective occupancy is full; wait for a player to leave"
                        .to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
                grant: None,
            };
        }

        let device_session_id = build_device_session_id(issued_at_unix_ms, self.next_sequence);
        let release_token = build_release_token();
        let registration_grant = match registration_public_key {
            Some(public_key) => {
                let mut nonce = [0_u8; 32];
                fill_os_random(&mut nonce);
                let issuer_private_key = match std::env::var(
                    oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return HostedPlayerSessionIssueResponse {
                            ok: false,
                            error_code: Some("registration_issuer_not_configured".to_string()),
                            error: Some(
                                "hosted registration issuer private key is not configured"
                                    .to_string(),
                            ),
                            deployment_mode: deployment_mode.as_str().to_string(),
                            admission,
                            grant: None,
                        };
                    }
                };
                match oasis7_client_api::viewer_auth::issue_hosted_registration_grant(
                    player_id.as_str(),
                    public_key,
                    device_session_id.as_str(),
                    hex::encode(nonce).as_str(),
                    issued_at_unix_ms,
                    issuer_private_key.as_str(),
                ) {
                    Ok(grant) => Some(grant),
                    Err(error) => {
                        return HostedPlayerSessionIssueResponse {
                            ok: false,
                            error_code: Some("registration_grant_issue_failed".to_string()),
                            error: Some(error),
                            deployment_mode: deployment_mode.as_str().to_string(),
                            admission,
                            grant: None,
                        };
                    }
                }
            }
            None => None,
        };
        self.issued_players_total = self.issued_players_total.saturating_add(1);
        self.issue_timestamps_unix_ms.push_back(issued_at_unix_ms);
        let release_token_digest = release_token_digest(release_token.as_str());
        self.active_release_tokens_by_player
            .insert(player_id.clone(), release_token_digest.clone());
        self.active_players_by_release_token
            .insert(release_token_digest.clone(), player_id.clone());
        self.last_seen_unix_ms_by_release_token
            .insert(release_token_digest, issued_at_unix_ms);
        self.runtime_seen_players.remove(player_id.as_str());
        self.runtime_revoked_players.remove(player_id.as_str());
        if let Err(error) = self.persist_ledger() {
            let _ = self.release_slot_for_player_with_reason(
                player_id.as_str(),
                SlotReleaseReason::PersistenceRollback,
                None,
            );
            return HostedPlayerSessionIssueResponse {
                ok: false,
                error_code: Some("session_ledger_persist_failed".to_string()),
                error: Some(error),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission: self.admission_snapshot(
                    contract.admission.issue_rate_limit_per_minute,
                    contract.admission.max_player_sessions,
                ),
                grant: None,
            };
        }
        admission = self.admission_snapshot(
            contract.admission.issue_rate_limit_per_minute,
            contract.admission.max_player_sessions,
        );

        HostedPlayerSessionIssueResponse {
            ok: true,
            error_code: None,
            error: None,
            deployment_mode: deployment_mode.as_str().to_string(),
            admission,
            grant: Some(HostedPlayerSessionIssueGrant {
                player_id,
                device_session_id,
                issued_at_unix_ms,
                auth_mode: "browser_local_ephemeral_ed25519".to_string(),
                release_token,
                registration_grant,
            }),
        }
    }

    pub(super) fn release(
        &mut self,
        deployment_mode: DeploymentMode,
        player_id: &str,
        release_token: &str,
    ) -> HostedPlayerSessionReleaseResponse {
        let contract = hosted_player_access_contract(deployment_mode);
        self.prune_old_timestamps();
        self.prune_expired_slots();
        let admission = self.admission_snapshot(
            contract.admission.issue_rate_limit_per_minute,
            contract.admission.max_player_sessions,
        );
        if deployment_mode != DeploymentMode::HostedPublicJoin {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("player_session_release_disabled".to_string()),
                error: Some(
                    "hosted player-session release is only available on hosted_public_join"
                        .to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        }
        let token = release_token.trim();
        if token.is_empty() {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("release_token_required".to_string()),
                error: Some("release_token is required".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        }
        let expected_player_id = player_id.trim();
        if expected_player_id.is_empty() {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("player_id_required".to_string()),
                error: Some("player_id is required".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        }
        if self.runtime_revoked_players.contains(expected_player_id) {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("session_revoked".to_string()),
                error: Some(
                    "player session was revoked by runtime presence reconciliation".to_string(),
                ),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        }
        let token_digest = release_token_digest(token);
        let Some(bound_player_id) = self
            .active_players_by_release_token
            .get(token_digest.as_str())
            .cloned()
        else {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("release_token_invalid".to_string()),
                error: Some("release_token does not map to an active player slot".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        };
        if bound_player_id != expected_player_id {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("player_id_mismatch".to_string()),
                error: Some("player_id does not match the active slot owner".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission,
            };
        }
        let _ = self.release_slot_for_player_with_reason(
            bound_player_id.as_str(),
            SlotReleaseReason::ExplicitRelease,
            None,
        );
        if let Err(error) = self.persist_ledger() {
            return HostedPlayerSessionReleaseResponse {
                ok: false,
                error_code: Some("session_ledger_persist_failed".to_string()),
                error: Some(error),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission: self.admission_snapshot(
                    contract.admission.issue_rate_limit_per_minute,
                    contract.admission.max_player_sessions,
                ),
            };
        }
        HostedPlayerSessionReleaseResponse {
            ok: true,
            error_code: None,
            error: None,
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: self.admission_snapshot(
                contract.admission.issue_rate_limit_per_minute,
                contract.admission.max_player_sessions,
            ),
        }
    }

    fn prune_old_timestamps(&mut self) {
        let cutoff = now_unix_ms().saturating_sub(ISSUE_WINDOW_MS);
        while self
            .issue_timestamps_unix_ms
            .front()
            .is_some_and(|issued_at| *issued_at < cutoff)
        {
            let _ = self.issue_timestamps_unix_ms.pop_front();
        }
    }

    fn prune_expired_slots(&mut self) {
        let now_unix_ms = now_unix_ms();
        let mut expired_tokens = Vec::new();
        for (token, last_seen) in &self.last_seen_unix_ms_by_release_token {
            let Some(player_id) = self.active_players_by_release_token.get(token.as_str()) else {
                expired_tokens.push((token.clone(), SlotReleaseReason::PendingExpiry));
                continue;
            };
            // A failed presence probe cannot establish that a pending player
            // left the runtime. Keep the slot within the bounded lease while
            // the probe is uncertain; a successful empty snapshot still
            // retains the short pending-registration expiry below.
            let ttl_ms = if self.runtime_seen_players.contains(player_id.as_str())
                || self.last_runtime_probe_error.is_some()
            {
                SLOT_LEASE_TTL_MS
            } else {
                PENDING_REGISTRATION_TTL_MS
            };
            if now_unix_ms.saturating_sub(*last_seen) > ttl_ms {
                let reason = if self.runtime_seen_players.contains(player_id.as_str()) {
                    SlotReleaseReason::SeenExpiry
                } else {
                    SlotReleaseReason::PendingExpiry
                };
                expired_tokens.push((token.clone(), reason));
            }
        }
        let had_expired_tokens = !expired_tokens.is_empty();
        for (token, reason) in expired_tokens {
            if let Some(player_id) = self.active_players_by_release_token.remove(token.as_str()) {
                self.active_release_tokens_by_player
                    .remove(player_id.as_str());
                self.runtime_seen_players.remove(player_id.as_str());
                self.runtime_missing_snapshot_counts
                    .remove(player_id.as_str());
                self.runtime_revoked_players.remove(player_id.as_str());
                self.released_players_total = self.released_players_total.saturating_add(1);
                self.record_release(reason, None);
            }
            self.last_seen_unix_ms_by_release_token
                .remove(token.as_str());
        }
        if had_expired_tokens {
            let _ = self.persist_ledger();
        }
    }

    fn release_slot_for_player_with_reason(
        &mut self,
        player_id: &str,
        reason: SlotReleaseReason,
        probe_sequence: Option<u64>,
    ) -> bool {
        let player_id = player_id.trim();
        let Some(token) = self.active_release_tokens_by_player.remove(player_id) else {
            return false;
        };
        self.active_players_by_release_token.remove(token.as_str());
        self.last_seen_unix_ms_by_release_token
            .remove(token.as_str());
        self.runtime_seen_players.remove(player_id);
        self.runtime_missing_snapshot_counts.remove(player_id);
        if reason.marks_runtime_revoked() {
            self.runtime_revoked_players.insert(player_id.to_string());
        } else {
            self.runtime_revoked_players.remove(player_id);
        }
        self.released_players_total = self.released_players_total.saturating_add(1);
        self.record_release(reason, probe_sequence);
        true
    }

    fn record_release(&mut self, reason: SlotReleaseReason, probe_sequence: Option<u64>) {
        let reason_name = reason.as_str().to_string();
        self.release_telemetry
            .reason_counts
            .entry(reason_name.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        self.release_telemetry.last_reason = Some(reason_name);
        self.release_telemetry.last_release_unix_ms = Some(now_unix_ms());
        self.release_telemetry.last_probe_sequence = probe_sequence;
    }

    fn admission_snapshot(
        &self,
        issue_rate_limit_per_minute: u64,
        max_player_sessions: u64,
    ) -> HostedPlayerSessionAdmissionSnapshot {
        let issued_in_current_window = self.issue_timestamps_unix_ms.len() as u64;
        let active_player_sessions = self.active_release_tokens_by_player.len() as u64;
        let runtime_only_player_sessions = self
            .last_runtime_active_players
            .iter()
            .filter(|player_id| {
                !self
                    .active_release_tokens_by_player
                    .contains_key(player_id.as_str())
            })
            .count() as u64;
        let effective_player_sessions =
            active_player_sessions.saturating_add(runtime_only_player_sessions);
        let runtime_probe_status = if self.last_runtime_probe_unix_ms.is_none() {
            "not_started"
        } else if self.last_runtime_probe_error.is_some() {
            "error"
        } else {
            "ok"
        };
        HostedPlayerSessionAdmissionSnapshot {
            issue_rate_limit_per_minute,
            max_player_sessions,
            active_player_sessions,
            effective_player_sessions,
            runtime_bound_player_sessions: self.last_observed_runtime_bound_player_sessions,
            runtime_only_player_sessions,
            runtime_probe_status: runtime_probe_status.to_string(),
            runtime_probe_error: self.last_runtime_probe_error.clone(),
            last_runtime_probe_unix_ms: self.last_runtime_probe_unix_ms,
            slot_lease_ttl_ms: SLOT_LEASE_TTL_MS,
            pending_registration_ttl_ms: PENDING_REGISTRATION_TTL_MS,
            issued_players_total: self.issued_players_total,
            released_players_total: self.released_players_total,
            issued_in_current_window,
            remaining_issue_budget: issue_rate_limit_per_minute
                .saturating_sub(issued_in_current_window),
            release_telemetry: (!self.release_telemetry.reason_counts.is_empty())
                .then(|| self.release_telemetry.clone()),
        }
    }
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|err| format!("create hosted session ledger directory failed: {err}"))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("hosted-player-sessions.json");
    let temp_path = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    let bytes = serde_json::to_vec(value)
        .map_err(|err| format!("encode hosted session ledger failed: {err}"))?;
    let mut temp_file = fs::File::create(&temp_path)
        .map_err(|err| format!("create hosted session ledger temp file failed: {err}"))?;
    temp_file
        .write_all(bytes.as_slice())
        .map_err(|err| format!("write hosted session ledger temp file failed: {err}"))?;
    temp_file
        .sync_all()
        .map_err(|err| format!("sync hosted session ledger temp file failed: {err}"))?;
    platform_atomic_replace(&temp_path, path)
        .map_err(|err| format!("replace hosted session ledger failed: {err}"))?;
    sync_parent_directory(parent)
        .map_err(|err| format!("sync hosted session ledger directory failed: {err}"))
}

#[cfg(windows)]
fn platform_atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn platform_atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> std::io::Result<()> {
    fs::File::open(parent).and_then(|directory| directory.sync_all())
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

fn build_player_id(issued_at_unix_ms: u64, sequence: u64) -> String {
    format!("hosted-player-{issued_at_unix_ms:016x}-{sequence:08x}")
}

fn build_release_token() -> String {
    let mut credential = [0_u8; 32];
    fill_os_random(&mut credential);
    hex::encode(credential)
}

fn build_registration_grant(
    player_id: &str,
    public_key: &str,
    device_session_id: &str,
    issued_at_unix_ms: u64,
) -> Result<String, String> {
    let issuer_private_key =
        std::env::var(oasis7_client_api::viewer_auth::HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY_ENV)
            .map_err(|_| "hosted registration issuer private key is not configured".to_string())?;
    let mut nonce = [0_u8; 32];
    fill_os_random(&mut nonce);
    oasis7_client_api::viewer_auth::issue_hosted_registration_grant(
        player_id,
        public_key,
        device_session_id,
        hex::encode(nonce).as_str(),
        issued_at_unix_ms,
        issuer_private_key.as_str(),
    )
}

fn fill_os_random(destination: &mut [u8]) {
    getrandom::fill(destination).expect("OS randomness unavailable");
}

fn release_token_digest(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

fn build_device_session_id(issued_at_unix_ms: u64, sequence: u64) -> String {
    format!("hosted-device-session-{issued_at_unix_ms:016x}-{sequence:08x}")
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "hosted_player_session_tests.rs"]
mod tests;
