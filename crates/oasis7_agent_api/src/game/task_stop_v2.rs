//! Guarded Stop is a new Owner domain. No reusable legacy Stop signature is carried.
use super::{DecimalU64, GameTaskFenceV1, GameTaskStatusV1, StopObservationGrantV2};
use serde::{Deserialize, Serialize};
pub const GAME_TASK_STOP_DOMAIN_V2: &str = "game_task_stop/v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskStopV2 {
    pub api_revision: u32,
    pub client_operation_id: String,
    pub custody_account_id: String,
    pub termination_plan_digest: String,
    pub initial_fence: GameTaskFenceV1,
    /// Target of this Owner-controlled Stop, never the authorizing signer.
    pub target_executor_public_key: String,
    pub target_executor_generation: DecimalU64,
    pub target_executor_nonce: DecimalU64,
    pub execute_until_ms: DecimalU64,
    /// None preserves the original Stop-only signing bytes; no implicit read grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_grant: Option<StopObservationGrantV2>,
}
impl GameTaskStopV2 {
    /// Deterministic transition calculation only, not a grant or World mutation.
    pub fn derive_post_control(
        &self,
        before: &super::GameTaskControlV1,
    ) -> Result<super::GameTaskControlV1, String> {
        self.validate()?;
        if GameTaskFenceV1::from_control(before) != self.initial_fence {
            return Err("guarded Stop predecessor mismatch".into());
        }
        let mut after = before.clone();
        after.task_revision = DecimalU64(
            before
                .task_revision
                .0
                .checked_add(1)
                .ok_or("guarded Stop task revision overflow")?,
        );
        after.executor_epoch = DecimalU64(
            before
                .executor_epoch
                .0
                .checked_add(1)
                .ok_or("guarded Stop executor epoch overflow")?,
        );
        after.executor_binding = None;
        after.executor_id = None;
        after.task_status = GameTaskStatusV1::Suspended;
        Ok(after)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.initial_fence.validate()?;
        let f = &self.initial_fence;
        let binding = f
            .executor_binding
            .as_ref()
            .ok_or("guarded Stop requires bound initial task")?;
        let id = |s: &str| !s.trim().is_empty() && s.len() <= 256;
        let hex = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.api_revision != 2
            || !id(&self.client_operation_id)
            || !id(&self.custody_account_id)
            || !hex(&self.termination_plan_digest)
            || !hex(&self.target_executor_public_key)
            || f.task_status != GameTaskStatusV1::Active
            || f.active_goal_reference.is_none()
            || binding.executor_public_key != self.target_executor_public_key
            || binding.delegation_generation != self.target_executor_generation
            || self.target_executor_generation.0 == 0
            || self.target_executor_nonce.0 == 0
            || self.execute_until_ms.0 == 0
        {
            return Err("invalid guarded Stop shape".into());
        }
        if let Some(grant) = &self.observation_grant {
            grant.validate_for(self)?;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedGameTaskStopV2 {
    pub request: GameTaskStopV2,
    pub subject_public_key: String,
    pub signature_hex: String,
}
impl core::fmt::Debug for SignedGameTaskStopV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedGameTaskStopV2(<private>)")
    }
}
impl SignedGameTaskStopV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.request.validate()?;
        if self.subject_public_key != self.request.initial_fence.owner_public_key
            || self.signature_hex.len() != 128
            || !self
                .signature_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid guarded Stop signature shape".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> GameTaskStopV2 {
        GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: "guarded-stop".into(),
            custody_account_id: "account".into(),
            termination_plan_digest: "44".repeat(32),
            initial_fence: GameTaskFenceV1 {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
                owner_public_key: "11".repeat(32),
                task_id: "task".into(),
                task_revision: DecimalU64(3),
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
            },
            target_executor_public_key: "33".repeat(32),
            target_executor_generation: DecimalU64(1),
            target_executor_nonce: DecimalU64(1),
            execute_until_ms: DecimalU64(5000),
            observation_grant: None,
        }
    }
    #[test]
    fn guarded_stop_v2_payload_roundtrip_has_no_inner_legacy_signed_request() {
        let p = valid();
        p.validate().unwrap();
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(
            serde_json::from_value::<GameTaskStopV2>(v.clone()).unwrap(),
            p
        );
        assert!(v.get("signed_payload").is_none());
        assert!(v.get("signature_hex").is_none());
        assert!(v.get("legacy_stop").is_none());
        assert_eq!(v["execute_until_ms"], "5000");
    }
    #[test]
    fn guarded_stop_v2_rejects_unknown_legacy_signature_and_missing_deadline() {
        let p = valid();
        p.validate().unwrap();
        let v = serde_json::to_value(&p).unwrap();
        let mut extra = v.clone();
        extra["legacy_stop"] = serde_json::json!({"request":{},"subject_public_key":"11".repeat(32),"signature_hex":"22".repeat(64)});
        assert!(serde_json::from_value::<GameTaskStopV2>(extra).is_err());
        let mut missing = v;
        missing.as_object_mut().unwrap().remove("execute_until_ms");
        assert!(serde_json::from_value::<GameTaskStopV2>(missing).is_err());
    }
    #[test]
    fn guarded_stop_v2_rejects_wrong_signer_lineage_and_empty_deadline() {
        let good = valid();
        good.validate().unwrap();
        let mut bad = good.clone();
        bad.target_executor_generation = DecimalU64(2);
        assert!(bad.validate().is_err());
        let mut bad = good.clone();
        bad.target_executor_nonce = DecimalU64(0);
        assert!(bad.validate().is_err());
        let mut bad = good;
        bad.execute_until_ms = DecimalU64(0);
        assert!(bad.validate().is_err());
    }
}
