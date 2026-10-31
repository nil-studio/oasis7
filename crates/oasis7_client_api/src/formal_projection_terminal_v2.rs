//! Explicit terminal-only payload for a private formal audience. It is never
//! serialized as the legacy WorldServiceProjection or execution authority.
use super::*;
use oasis7_agent_api::game::{GameTaskClosePredicateV2, GameTaskClosedV2, GameTaskStatusV1};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedTaskFormalProjectionV2 {
    pub api_revision: u32,
    pub closed: Box<GameTaskClosedV2>,
}
impl ClosedTaskFormalProjectionV2 {
    pub fn validate(&self, audience: &ProjectionAudienceV1) -> Result<(), String> {
        audience.validate()?;
        let c = &self.closed;
        c.initial_fence.validate()?;
        c.actual_before_fence.validate()?;
        if self.api_revision != 2
            || c.schema_revision != 2
            || audience.scope_id != format!("agent:{}", c.initial_fence.agent_id)
            || audience.reader_public_key.is_none()
            || c.initial_fence.task_status != GameTaskStatusV1::Active
            || c.close_operation_id.trim().is_empty()
            || c.close_operation_id.len() > 256
            || !hash_valid(&c.close_original_digest)
            || !hash_valid(&c.termination_plan_digest)
            || !hash_valid(&c.revoked_signer_public_key)
            || c.revoked_generation.0 == 0
            || c.permanent_nonce_high_water.0 == 0
            || c.initial_fence
                .executor_binding
                .as_ref()
                .is_none_or(|binding| {
                    binding.executor_public_key != c.revoked_signer_public_key
                        || c.revoked_generation.0 <= binding.delegation_generation.0
                })
        {
            return Err("invalid terminal formal projection shape/audience".into());
        }
        match &c.selected_predicate {
            GameTaskClosePredicateV2::InitialActive if c.actual_before_fence != c.initial_fence => {
                Err("terminal initial predicate mismatch".into())
            }
            GameTaskClosePredicateV2::OwnStopSuspended(reference)
                if reference.post_fence != c.actual_before_fence
                    || c.actual_before_fence.task_status != GameTaskStatusV1::Suspended
                    || !hash_valid(&reference.original_digest)
                    || !hash_valid(&reference.canonical_result_digest) =>
            {
                Err("terminal OwnStop predicate mismatch".into())
            }
            _ => Ok(()),
        }
    }
    /// Keep the existing tree algorithm and all V1 leaf bytes unchanged. Only
    /// this explicitly versioned typed payload uses the terminal representation.
    pub fn leaf(&self, audience: &ProjectionAudienceV1) -> Result<String, String> {
        self.validate(audience)?;
        projection_leaf(audience, self)
    }
}
