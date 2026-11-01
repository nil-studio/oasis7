//! Pure canonical result core; FullCommitRef is attached only after real finalize.
use super::{GAME_TASK_STOP_DOMAIN_V2, GameTaskControlV1, SignedGameTaskStopV2};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskStopRecordV2 {
    pub schema_revision: u32,
    pub original: SignedGameTaskStopV2,
    pub before_control: GameTaskControlV1,
    pub after_control: GameTaskControlV1,
    pub committed_height: u64,
    pub committed_at_unix_ms: u64,
    pub result_core_digest: String,
}
impl GameTaskStopRecordV2 {
    pub fn operation_key(&self) -> Result<String, String> {
        let p = &self.original.request;
        let f = &p.initial_fence;
        oasis7_wasm_abi::canonical_hash(&(
            GAME_TASK_STOP_DOMAIN_V2,
            &self.original.subject_public_key,
            &f.world_id,
            &f.genesis_digest,
            &f.branch_id,
            f.reorg_epoch,
            &f.agent_id,
            &f.task_id,
            &p.client_operation_id,
        ))
        .map_err(|e| e.to_string())
    }
    /// No final FullCommitRef enters its own state-root preimage.
    pub fn compute_result_core_digest(&self) -> Result<String, String> {
        oasis7_wasm_abi::canonical_hash(&(
            "game_task_stop_result/v2",
            self.schema_revision,
            &self.original,
            &self.before_control,
            &self.after_control,
            self.committed_height,
            self.committed_at_unix_ms,
        ))
        .map_err(|e| e.to_string())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.original.validate_shape()?;
        if self.schema_revision != 2
            || self.committed_height == 0
            || self.committed_at_unix_ms >= self.original.request.execute_until_ms.0
            || self.after_control
                != self
                    .original
                    .request
                    .derive_post_control(&self.before_control)?
            || self.result_core_digest != self.compute_result_core_digest()?
        {
            return Err("invalid intrinsic guarded Stop record".into());
        }
        Ok(())
    }
}
