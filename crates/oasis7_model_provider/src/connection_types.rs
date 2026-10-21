//! Non-secret connection identities, authority callbacks and bounded deployment policy.
use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Principal {
    pub account_id: String,
    pub session_id: String,
    /// Host lookup reference, never the raw application session credential.
    pub auth_ref: String,
}
impl std::fmt::Debug for Principal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Principal").finish_non_exhaustive()
    }
}
pub trait SessionAuthority: Send + Sync {
    fn is_current(&self, principal: &Principal) -> bool;
}
impl<F: Fn(&Principal) -> bool + Send + Sync> SessionAuthority for F {
    fn is_current(&self, principal: &Principal) -> bool {
        self(principal)
    }
}
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}
pub struct MonotonicClock {
    epoch_ms: u64,
    start: Instant,
}
impl Default for MonotonicClock {
    fn default() -> Self {
        Self {
            epoch_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            start: Instant::now(),
        }
    }
}
impl Clock for MonotonicClock {
    fn now_ms(&self) -> u64 {
        self.epoch_ms
            .saturating_add(self.start.elapsed().as_millis().min(u64::MAX as u128) as u64)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BindingTarget {
    /// Opaque complete-authority binding minted by the trusted host. A browser
    /// target reference must never supply this value or become an authority.
    pub full_fence_digest: String,
    pub world_id: String,
    pub agent_id: String,
    pub task_id: String,
    pub goal_revision: u64,
    pub executor_id: String,
    pub executor_epoch: u64,
}
pub trait BindingAuthority: Send + Sync {
    /// Must consult trusted current owner/task/grant/executor authority. A host
    /// without an executor must reject; client-provided validity flags are invalid.
    fn authorize(&self, principal: &Principal, target: &BindingTarget) -> bool;
}
impl<F: Fn(&Principal, &BindingTarget) -> bool + Send + Sync> BindingAuthority for F {
    fn authorize(&self, p: &Principal, t: &BindingTarget) -> bool {
        self(p, t)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_calls: u64,
    pub max_tokens: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_concurrent: u64,
    pub lease_ms: u64,
    pub absolute_ms: u64,
    pub renew_interval_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_calls: 100,
            max_tokens: 100_000,
            max_input_tokens: 8_192,
            max_output_tokens: 2_048,
            max_concurrent: 2,
            lease_ms: 120_000,
            absolute_ms: 1_800_000,
            renew_interval_ms: 30_000,
        }
    }
}
impl Limits {
    pub(super) fn valid(&self) -> bool {
        self.max_calls > 0
            && self.max_calls <= 10_000
            && self.max_tokens > 0
            && self.max_tokens <= 100_000_000
            && self.max_input_tokens > 0
            && self.max_input_tokens <= 1_048_576
            && self.max_output_tokens > 0
            && self.max_output_tokens <= 32_768
            && self.max_concurrent > 0
            && self.max_concurrent <= 8
            && self.lease_ms > 0
            && self.lease_ms <= 120_000
            && self.absolute_ms >= self.lease_ms
            && self.absolute_ms <= 7_200_000
            && self.renew_interval_ms > 0
            && self.renew_interval_ms <= self.lease_ms
            && self
                .max_input_tokens
                .checked_add(self.max_output_tokens)
                .is_some()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionError {
    Unavailable,
    Invalid,
    Conflict,
    BudgetExceeded,
    CapacityExceeded,
    BindingDenied,
    NotReady,
    AlreadySent,
    AlreadyFinished,
}
impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ConnectionError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Draft,
    Checking,
    Ready,
    Blocked,
    Revoked,
    Expired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Stopped,
    Active,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct BudgetSnapshot {
    pub calls: u64,
    pub tokens: u64,
    pub in_flight: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ConnectionSnapshot {
    pub id: String,
    pub preset_id: String,
    pub version: u64,
    pub generation: u64,
    /// Immutable active-run revision; lease renewal only advances `version`.
    pub active_version: Option<u64>,
    pub state: ConnectionState,
    pub run_state: RunState,
    pub expires_at_ms: u64,
    pub absolute_expires_at_ms: u64,
    pub budget: BudgetSnapshot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    Pending,
    Ready,
    Failed,
}
