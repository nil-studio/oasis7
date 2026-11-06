//! CleanupV3 only. Existing V1/V2 wire and reducers remain unchanged.
//! Wire types only: no signature, current-state, persistence or dispatch authority.
use super::{DecimalU64, GameTaskFenceV1};
use serde::{Deserialize, Serialize};
pub const GAME_TASK_CLEANUP_DOMAIN_V3: &str = "game_task_cleanup/v3";
pub const GAME_TASK_CLOSED_DOMAIN_V3: &str = "game_task_closed/v3";
/// Runtime must resolve this exact reference from authenticated canonical history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOwnGuardedStopReferenceV3 {
    pub operation_id: String,
    pub original_digest: String,
    pub canonical_result_digest: String,
    pub post_fence: GameTaskFenceV1,
}
/// Both alternatives require actual World atomic current comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "predicate",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GameTaskClosePredicateV3 {
    InitialActive,
    OwnGuardedStopSuspended(Box<GameOwnGuardedStopReferenceV3>),
}
/// Persisted unsigned GameTaskStopV2 intent: hash(game_task_stop/v2, Owner key, typed request).
/// No Stop signature or receipt is required to construct this condition.
/// World resolves its actual canonical result and derives any Suspended fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOwnGuardedStopIntentV3 {
    pub operation_id: String,
    pub intent_digest: String,
}
/// Mandatory exact unsigned GuardedStopV2 intent; it is not a Stop receipt.
/// Fixed two-branch policy, not a caller-selected assertion of actual state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskCloseConditionsV3 {
    pub exact_initial_active: GameTaskFenceV1,
    pub own_stop_intent: GameOwnGuardedStopIntentV3,
}
/// A new signed domain; it cannot reinterpret V1 Stop or CleanupV2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskRevokeAndCloseV3 {
    pub api_revision: u32,
    pub client_operation_id: String,
    pub custody_account_id: String,
    pub termination_plan_digest: String,
    pub permitted_branches: GameTaskCloseConditionsV3,
    pub signer_public_key: String,
    pub signer_generation: DecimalU64,
    pub signer_nonce: DecimalU64,
    pub revoke_generation: DecimalU64,
    pub revoke_nonce: DecimalU64,
    pub cleanup_until_ms: DecimalU64,
}
fn bounded_identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn exact_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl GameTaskRevokeAndCloseV3 {
    /// Shape checks only, never signature, current, plan approval or HWM admission.
    pub fn validate(&self) -> Result<(), String> {
        self.permitted_branches.exact_initial_active.validate()?;
        if self.api_revision != 3
            || !bounded_identity(&self.client_operation_id)
            || !bounded_identity(&self.custody_account_id)
            || !exact_digest(&self.termination_plan_digest)
            || !exact_digest(&self.signer_public_key)
            || self.permitted_branches.exact_initial_active.task_status
                != super::GameTaskStatusV1::Active
            || self.signer_generation.0 == 0
            || self.signer_nonce.0 == 0
            || self.revoke_generation.0 <= self.signer_generation.0
            || self.revoke_nonce.0 <= self.signer_nonce.0
            || self.cleanup_until_ms.0 == 0
        {
            return Err("invalid task cleanup v3 shape".into());
        }
        let intent = &self.permitted_branches.own_stop_intent;
        if !bounded_identity(&intent.operation_id) || !exact_digest(&intent.intent_digest) {
            return Err("invalid guarded StopV2 intent in cleanupV3".into());
        }
        Ok(())
    }
}
/// Explicit terminal fact, not a boolean or a legacy Suspended/None projection.
/// A future Runtime reducer must retain authenticated Closed facts permanently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskClosedV3 {
    pub schema_revision: u32,
    pub close_operation_id: String,
    pub close_original_digest: String,
    pub termination_plan_digest: String,
    pub initial_fence: GameTaskFenceV1,
    pub actual_before_fence: GameTaskFenceV1,
    pub selected_predicate: GameTaskClosePredicateV3,
    pub revoked_signer_public_key: String,
    pub revoked_generation: DecimalU64,
    pub permanent_nonce_high_water: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GameTaskStateV3 {
    Open(Box<GameTaskFenceV1>),
    Closed(Box<GameTaskClosedV3>),
}
