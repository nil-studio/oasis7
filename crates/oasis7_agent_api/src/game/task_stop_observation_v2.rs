//! Owner-signed narrow read permission, separate from Stop mutation expiry.
//! This data constructor is not Account paid/current authority.
use super::{DecimalU64, GameTaskStopV2};
use serde::{Deserialize, Serialize};
pub const STOP_OBSERVATION_SCOPE_DOMAIN_V2: &str = "oasis7.game-task-stop-observation-scope.v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum StopObservationGrantV2 {
    KnownStopControlV2 {
        target_digest: String,
        approved_cleanup_until_ms: DecimalU64,
        historical_read_until_ms: DecimalU64,
        control_read_until_ms: DecimalU64,
    },
}
impl StopObservationGrantV2 {
    /// Excludes the grant itself. The full original including this grant is
    /// subsequently signed under game_task_stop/v2 by the actual Owner.
    pub fn target_digest(original: &GameTaskStopV2) -> Result<String, String> {
        oasis7_wasm_abi::canonical_hash(&(
            STOP_OBSERVATION_SCOPE_DOMAIN_V2,
            &original.custody_account_id,
            &original.termination_plan_digest,
            &original.client_operation_id,
            &original.initial_fence,
            &original.target_executor_public_key,
            original.target_executor_generation,
            original.target_executor_nonce,
        ))
        .map_err(|e| e.to_string())
    }
    /// Pure codec construction. Only the actual Account/Vault producer can
    /// match approved_cleanup_until to its consumed full Term plan and sign.
    pub fn known_control_for_exact_target(
        original: &GameTaskStopV2,
        approved_cleanup_until_ms: u64,
        historical_read_until_ms: u64,
        control_read_until_ms: u64,
    ) -> Result<Self, String> {
        let grant = Self::KnownStopControlV2 {
            target_digest: Self::target_digest(original)?,
            approved_cleanup_until_ms: DecimalU64(approved_cleanup_until_ms),
            historical_read_until_ms: DecimalU64(historical_read_until_ms),
            control_read_until_ms: DecimalU64(control_read_until_ms),
        };
        grant.validate_for(original)?;
        Ok(grant)
    }
    pub fn validate_for(&self, original: &GameTaskStopV2) -> Result<(), String> {
        let Self::KnownStopControlV2 {
            target_digest,
            approved_cleanup_until_ms,
            historical_read_until_ms,
            control_read_until_ms,
        } = self;
        if target_digest != &Self::target_digest(original)?
            || control_read_until_ms.0 == 0
            || control_read_until_ms.0 > historical_read_until_ms.0
            || historical_read_until_ms.0 > approved_cleanup_until_ms.0
            || original.execute_until_ms.0 > approved_cleanup_until_ms.0
        {
            return Err("invalid Owner Stop observation grant".into());
        }
        Ok(())
    }
    pub fn historical_read_until_ms(&self) -> u64 {
        let Self::KnownStopControlV2 {
            historical_read_until_ms,
            ..
        } = self;
        historical_read_until_ms.0
    }
    pub fn control_read_until_ms(&self) -> u64 {
        let Self::KnownStopControlV2 {
            control_read_until_ms,
            ..
        } = self;
        control_read_until_ms.0
    }
}
