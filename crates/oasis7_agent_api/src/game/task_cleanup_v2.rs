//! Wire types only: no signature, current-state, persistence or dispatch authority.
use super::{DecimalU64, GameTaskFenceV1};
use serde::{Deserialize, Serialize};
pub const GAME_TASK_CLEANUP_DOMAIN_V2: &str = "game_task_cleanup/v2";
pub const GAME_TASK_CLOSED_DOMAIN_V2: &str = "game_task_closed/v2";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameStopIntentProfileV2 {
    GuardedTaskStopV2,
}
/// Runtime must resolve this exact reference from authenticated canonical history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOwnStopReferenceV2 {
    pub profile: GameStopIntentProfileV2,
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
pub enum GameTaskClosePredicateV2 {
    InitialActive,
    OwnStopSuspended(Box<GameOwnStopReferenceV2>),
}
/// Persisted unsigned Stop intent: hash(domain, Owner public key, typed request).
/// No Stop signature or receipt is required to construct this condition.
/// World resolves its actual canonical result and derives any Suspended fence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameOwnStopIntentV2 {
    pub profile: GameStopIntentProfileV2,
    pub operation_id: String,
    pub intent_digest: String,
}
fn required_stop_intent<'de, D>(d: D) -> Result<Option<GameOwnStopIntentV2>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<GameOwnStopIntentV2>::deserialize(d)
}
/// Explicit null allows only InitialActive. Missing field is never a default.
/// Fixed two-branch policy, not a caller-selected assertion of actual state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskCloseConditionsV2 {
    pub exact_initial_active: GameTaskFenceV1,
    #[serde(deserialize_with = "required_stop_intent")]
    pub own_stop_intent: Option<GameOwnStopIntentV2>,
}
/// A new signed domain; it cannot reinterpret V1 Stop or Revoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskRevokeAndCloseV2 {
    pub api_revision: u32,
    pub client_operation_id: String,
    pub custody_account_id: String,
    pub termination_plan_digest: String,
    pub permitted_branches: GameTaskCloseConditionsV2,
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
impl GameTaskRevokeAndCloseV2 {
    /// Shape checks only, never signature, current, plan approval or HWM admission.
    pub fn validate(&self) -> Result<(), String> {
        self.permitted_branches.exact_initial_active.validate()?;
        if self.api_revision != 2
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
            return Err("invalid task cleanup v2 shape".into());
        }
        if self
            .permitted_branches
            .own_stop_intent
            .as_ref()
            .is_some_and(|intent| {
                !bounded_identity(&intent.operation_id) || !exact_digest(&intent.intent_digest)
            })
        {
            return Err("invalid own Stop intent v2 shape".into());
        }
        Ok(())
    }
}
/// Explicit terminal fact, not a boolean or a legacy Suspended/None projection.
/// A future Runtime reducer must retain authenticated Closed facts permanently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskClosedV2 {
    pub schema_revision: u32,
    pub close_operation_id: String,
    pub close_original_digest: String,
    pub termination_plan_digest: String,
    pub initial_fence: GameTaskFenceV1,
    pub actual_before_fence: GameTaskFenceV1,
    pub selected_predicate: GameTaskClosePredicateV2,
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
pub enum GameTaskStateV2 {
    Open(Box<GameTaskFenceV1>),
    Closed(Box<GameTaskClosedV2>),
}
#[cfg(test)]
mod tests {
    use super::super::{GAME_EXECUTOR_CONTROL_DOMAIN, GameTaskStatusV1};
    use super::*;
    #[test]
    fn cleanup_v2_does_not_reinterpret_legacy_status_or_domain() {
        assert_ne!(GAME_TASK_CLEANUP_DOMAIN_V2, GAME_EXECUTOR_CONTROL_DOMAIN);
        assert_eq!(
            serde_json::to_string(&GameTaskStatusV1::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&GameTaskStatusV1::Suspended).unwrap(),
            "\"suspended\""
        );
        assert!(serde_json::from_str::<GameTaskStatusV1>("\"closed\"").is_err());
        assert!(
            serde_json::from_str::<GameTaskStateV2>("{\"state\":\"closed\",\"value\":{}}").is_err()
        );
    }
    #[test]
    fn cleanup_v2_tagged_envelopes_reject_unknown_outer_fields() {
        // Full positive values first: a later error must be the added field.
        let predicate = GameTaskClosePredicateV2::InitialActive;
        let value = serde_json::to_value(&predicate).unwrap();
        assert_eq!(
            serde_json::from_value::<GameTaskClosePredicateV2>(value.clone()).unwrap(),
            predicate
        );
        let mut extra = value;
        extra
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(serde_json::from_value::<GameTaskClosePredicateV2>(extra).is_err());
        let fence = GameTaskFenceV1 {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
            owner_public_key: "11".repeat(32),
            task_id: "task".into(),
            task_revision: DecimalU64(1),
            task_status: GameTaskStatusV1::Active,
            latest_goal_revision: DecimalU64(1),
            active_goal_revision: DecimalU64(1),
            active_goal_reference: Some(super::super::GameGoalReferenceV1 {
                goal_revision: DecimalU64(1),
                goal_ref: "22".repeat(32),
                prepare_operation_id: "prepare".into(),
                publish_operation_id: "publish".into(),
                activation_committed_height: DecimalU64(1),
            }),
            executor_epoch: DecimalU64(1),
            executor_binding: Some(super::super::GameExecutorBindingV1 {
                executor_id: "executor".into(),
                executor_public_key: "33".repeat(32),
                delegation_generation: DecimalU64(1),
                executor_epoch: DecimalU64(1),
            }),
        };
        fence.validate().unwrap();
        assert_boxed_wire_is_unchanged(&fence);
        let state = GameTaskStateV2::Open(Box::new(fence));
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(
            serde_json::from_value::<GameTaskStateV2>(value.clone()).unwrap(),
            state
        );
        let mut extra = value;
        extra
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(serde_json::from_value::<GameTaskStateV2>(extra).is_err());
    }

    fn assert_boxed_wire_is_unchanged(fence: &GameTaskFenceV1) {
        #[derive(Serialize)]
        struct PredicateWire<'a, T> {
            predicate: &'static str,
            value: &'a T,
        }
        #[derive(Serialize)]
        struct StateWire<'a, T> {
            state: &'static str,
            value: &'a T,
        }
        let reference = GameOwnStopReferenceV2 {
            profile: GameStopIntentProfileV2::GuardedTaskStopV2,
            operation_id: "stop".into(),
            original_digest: "44".repeat(32),
            canonical_result_digest: "55".repeat(32),
            post_fence: fence.clone(),
        };
        let predicate = GameTaskClosePredicateV2::OwnStopSuspended(Box::new(reference.clone()));
        let legacy_predicate = PredicateWire {
            predicate: "own_stop_suspended",
            value: &reference,
        };
        assert_eq!(
            serde_json::to_value(&predicate).unwrap(),
            serde_json::to_value(&legacy_predicate).unwrap()
        );
        assert_eq!(
            serde_cbor::to_vec(&predicate).unwrap(),
            serde_cbor::to_vec(&legacy_predicate).unwrap()
        );
        let closed = GameTaskClosedV2 {
            schema_revision: 2,
            close_operation_id: "close".into(),
            close_original_digest: "66".repeat(32),
            termination_plan_digest: "77".repeat(32),
            initial_fence: fence.clone(),
            actual_before_fence: fence.clone(),
            selected_predicate: predicate,
            revoked_signer_public_key: "88".repeat(32),
            revoked_generation: DecimalU64(2),
            permanent_nonce_high_water: DecimalU64(2),
        };
        let closed_state = GameTaskStateV2::Closed(Box::new(closed.clone()));
        let closed_wire = StateWire {
            state: "closed",
            value: &closed,
        };
        assert_eq!(
            serde_json::to_value(&closed_state).unwrap(),
            serde_json::to_value(&closed_wire).unwrap()
        );
        assert_eq!(
            serde_cbor::to_vec(&closed_state).unwrap(),
            serde_cbor::to_vec(&closed_wire).unwrap()
        );
        assert_eq!(
            serde_json::from_value::<GameTaskStateV2>(serde_json::to_value(&closed_wire).unwrap())
                .unwrap(),
            closed_state
        );
        let open_state = GameTaskStateV2::Open(Box::new(fence.clone()));
        let open_wire = StateWire {
            state: "open",
            value: fence,
        };
        assert_eq!(
            serde_json::to_value(&open_state).unwrap(),
            serde_json::to_value(&open_wire).unwrap()
        );
        assert_eq!(
            serde_cbor::to_vec(&open_state).unwrap(),
            serde_cbor::to_vec(&open_wire).unwrap()
        );
    }

    #[test]
    fn close_conditions_require_original_identity_not_stop_result() {
        let fence = GameTaskFenceV1 {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
            owner_public_key: "11".repeat(32),
            task_id: "task".into(),
            task_revision: DecimalU64(1),
            task_status: GameTaskStatusV1::Active,
            latest_goal_revision: DecimalU64(1),
            active_goal_revision: DecimalU64(1),
            active_goal_reference: Some(super::super::GameGoalReferenceV1 {
                goal_revision: DecimalU64(1),
                goal_ref: "22".repeat(32),
                prepare_operation_id: "prepare".into(),
                publish_operation_id: "publish".into(),
                activation_committed_height: DecimalU64(1),
            }),
            executor_epoch: DecimalU64(1),
            executor_binding: Some(super::super::GameExecutorBindingV1 {
                executor_id: "executor".into(),
                executor_public_key: "33".repeat(32),
                delegation_generation: DecimalU64(1),
                executor_epoch: DecimalU64(1),
            }),
        };
        fence.validate().unwrap();
        let conditions = GameTaskCloseConditionsV2 {
            exact_initial_active: fence,
            own_stop_intent: Some(GameOwnStopIntentV2 {
                profile: GameStopIntentProfileV2::GuardedTaskStopV2,
                operation_id: "stop-original".into(),
                intent_digest: "22".repeat(32),
            }),
        };
        let value = serde_json::to_value(&conditions).unwrap();
        assert_eq!(
            serde_json::from_value::<GameTaskCloseConditionsV2>(value.clone()).unwrap(),
            conditions
        );
        assert!(
            value["own_stop_intent"]
                .get("canonical_result_digest")
                .is_none()
        );
        assert!(value["own_stop_intent"].get("post_fence").is_none());
        let mut extra = value.clone();
        extra["own_stop_intent"]["canonical_result_digest"] = "33".repeat(32).into();
        assert!(serde_json::from_value::<GameTaskCloseConditionsV2>(extra).is_err());
        let mut missing = value;
        missing["own_stop_intent"]
            .as_object_mut()
            .unwrap()
            .remove("intent_digest");
        assert!(serde_json::from_value::<GameTaskCloseConditionsV2>(missing).is_err());
        let mut no_stop = conditions;
        no_stop.own_stop_intent = None;
        let value = serde_json::to_value(&no_stop).unwrap();
        assert!(value["own_stop_intent"].is_null());
        assert_eq!(
            serde_json::from_value::<GameTaskCloseConditionsV2>(value.clone()).unwrap(),
            no_stop
        );
        let mut missing_field = value;
        missing_field
            .as_object_mut()
            .unwrap()
            .remove("own_stop_intent");
        assert!(serde_json::from_value::<GameTaskCloseConditionsV2>(missing_field).is_err());
    }
}
