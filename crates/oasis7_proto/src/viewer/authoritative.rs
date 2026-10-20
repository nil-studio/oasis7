use super::AuthoritativeRollbackReceipt;
use serde::{Deserialize, Serialize};

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritativeFinalityState {
    Pending,
    Confirmed,
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeBatchFinality {
    pub batch_id: String,
    pub tx_hash: String,
    pub commit_tick: u64,
    pub confirm_height: u64,
    pub final_height: u64,
    pub state_root: String,
    pub data_root: String,
    pub finality_state: AuthoritativeFinalityState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq_end: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub settlement_ready: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ranking_ready: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub challenge_open: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub slashed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_challenge_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritativeChallengeStatus {
    Challenged,
    ResolvedNoFraud,
    ResolvedFraudSlashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeChallengeAck<Time> {
    pub challenge_id: String,
    pub batch_id: String,
    pub watcher_id: String,
    pub status: AuthoritativeChallengeStatus,
    pub submitted_at_tick: Time,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at_tick: Option<Time>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub slash_applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeChallengeError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritativeRecoveryStatus {
    SessionRegistered,
    RolledBack,
    CatchUpReady,
    SessionRevoked,
    SessionRotated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeRecoveryAck<Time> {
    pub status: AuthoritativeRecoveryStatus,
    pub reorg_epoch: u64,
    pub snapshot_height: u64,
    pub snapshot_hash: String,
    pub log_cursor: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_pubkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_pubkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoke_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_receipt: Option<AuthoritativeRollbackReceipt>,
    pub acknowledged_at_tick: Time,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeRecoveryError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_pubkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoke_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
}
