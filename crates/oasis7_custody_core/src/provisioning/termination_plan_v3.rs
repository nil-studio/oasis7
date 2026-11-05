//! Independent supplemental cleanup approval. Shape checks allocate no authority or I/O.
use super::{NativeSubjectRefV3, TerminationNoncePolicyV1, TerminationTriggerV1};
use crate::{
    DecimalU64,
    approval::{ApprovalContextV1, ApprovalOptionsRequestV1, CustodyIntentV1},
    time_units::UnixMillis,
};
use oasis7_agent_api::game::{GameTaskFenceV1, GameTaskStatusV1};
use serde::{Deserialize, Serialize};
pub const TERMINATION_PLAN_DOMAIN_V3: &str = "oasis7.custody.termination-plan.v3";
pub const TERMINATION_APPROVAL_DOMAIN_V3: &str = "oasis7.custody.termination-approval.v3";
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminationTransportPolicyV3 {
    /// Not a deployable budget policy, including when a configuration flag is mistaken.
    DevelopmentMeasuredGraphV1 {
        stop_lookup_limit: u8,
        close_lookup_limit: u8,
    },
}
impl TerminationTransportPolicyV3 {
    pub fn production_supported(&self) -> bool {
        false
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::DevelopmentMeasuredGraphV1 {
                stop_lookup_limit: 2,
                close_lookup_limit: 2,
            } => Ok(()),
            _ => Err("invalid bounded development lookup policy".into()),
        }
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerSigningRecoveryPolicyV3 {
    ExactOriginalEd25519RecoveryV1 {
        max_recomputations_per_effect: DecimalU64,
    },
}
impl OwnerSigningRecoveryPolicyV3 {
    pub fn max_recomputations_per_effect(&self) -> u64 {
        match self {
            Self::ExactOriginalEd25519RecoveryV1 {
                max_recomputations_per_effect,
            } => max_recomputations_per_effect.0,
        }
    }
    fn validate(&self) -> Result<(), String> {
        if self.max_recomputations_per_effect() != 1 {
            return Err("unsupported exact-original recovery attempt bound".into());
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationStopPolicyV3 {
    GuardedCommittedUtcStopV2,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationClosePolicyV3 {
    LiteralPersistentClosedV3,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationPredecessorPolicyV3 {
    InitialActiveOrExactOwnGuardedStopV3,
}
/// New approval-domain metadata; it does not alter the SDK delegation codec.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationDelegationV3 {
    pub world_id: String,
    pub branch_id: String,
    pub agent_id: String,
    pub owner_binding: String,
    pub agent_identity_generation: DecimalU64,
    pub generation: DecimalU64,
    pub delegate_public_key: String,
    pub revoked: bool,
    pub nonce: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationPlanV3 {
    pub api_version: u8,
    pub subject_session_id: String,
    pub native_subject: NativeSubjectRefV3,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub subject_v1_approval_digest: String,
    pub subject_v1_termination_plan_digest: String,
    pub prepared_result_digest: String,
    pub world: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub initial_fence: GameTaskFenceV1,
    pub original_delegation: TerminationDelegationV3,
    pub stop_operation_id: String,
    pub close_operation_id: String,
    pub cleanup_until: UnixMillis,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub transport_policy: TerminationTransportPolicyV3,
    pub owner_signing_recovery_policy: OwnerSigningRecoveryPolicyV3,
    pub trigger: TerminationTriggerV1,
    pub nonce_policy: TerminationNoncePolicyV1,
    pub stop_policy: TerminationStopPolicyV3,
    pub close_policy: TerminationClosePolicyV3,
    pub allowed_predecessors: TerminationPredecessorPolicyV3,
}
fn bounded(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn hash(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn public_key(v: &str) -> bool {
    if !hash(v) {
        return false;
    }
    let Ok(bytes) = (0..32)
        .map(|i| u8::from_str_radix(&v[i * 2..i * 2 + 2], 16))
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    let Ok(bytes) = <[u8; 32]>::try_from(bytes) else {
        return false;
    };
    ed25519_dalek::VerifyingKey::from_bytes(&bytes).is_ok_and(|k| !k.is_weak())
}
fn digest<T: Serialize>(v: &T) -> Result<String, String> {
    oasis7_wasm_abi::encode_canonical_cbor(v)
        .map(|b| crate::approval::bytes_digest(&b))
        .map_err(|_| "invalid termination V3 canonical bytes".into())
}
impl TerminationPlanV3 {
    pub fn expected_stop_operation_id(&self) -> Result<String, String> {
        digest(&(
            "oasis7.custody.termination-stop-id.v3",
            &self.subject_session_id,
            &self.prepared_result_digest,
        ))
    }
    pub fn expected_close_operation_id(&self) -> Result<String, String> {
        digest(&(
            "oasis7.custody.termination-close-id.v3",
            &self.subject_session_id,
            &self.prepared_result_digest,
        ))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.initial_fence.validate()?;
        self.native_subject.validate()?;
        self.transport_policy.validate()?;
        self.owner_signing_recovery_policy.validate()?;
        let f = &self.initial_fence;
        let d = &self.original_delegation;
        let binding = f
            .executor_binding
            .as_ref()
            .ok_or("termination V3 requires executor binding")?;
        if self.api_version != 3
            || [
                &self.subject_session_id,
                &self.custody_account_id,
                &self.provision_id,
                &self.setup_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
            ]
            .into_iter()
            .any(|v| !bounded(v))
            || [
                &self.subject_v1_approval_digest,
                &self.subject_v1_termination_plan_digest,
                &self.prepared_result_digest,
            ]
            .into_iter()
            .any(|v| !hash(v))
            || !self.world.valid()
            || !public_key(&self.world.service_public_key)
            || !public_key(&f.owner_public_key)
            || !public_key(&binding.executor_public_key)
            || f.world_id != self.world.world_id
            || f.genesis_digest != self.world.genesis_digest
            || f.branch_id != self.world.branch_id
            || f.reorg_epoch != self.world.reorg_epoch
            || f.task_status != GameTaskStatusV1::Active
            || f.active_goal_reference.is_none()
            || d.world_id != f.world_id
            || d.branch_id != f.branch_id
            || d.agent_id != f.agent_id
            || d.owner_binding != f.owner_binding
            || d.agent_identity_generation != f.owner_generation
            || d.generation.0 == 0
            || d.nonce.0 == 0
            || d.revoked
            || d.delegate_public_key != binding.executor_public_key
            || d.generation != binding.delegation_generation
            || self.cleanup_until.get() == 0
            || self.request_limit.0 != 2
            || self.transport_limit.0 == 0
            || self.stop_operation_id != self.expected_stop_operation_id()?
            || self.close_operation_id != self.expected_close_operation_id()?
            || self.stop_operation_id == self.close_operation_id
        {
            return Err("invalid termination V3 plan".into());
        }
        Ok(())
    }
    /// No subject store, version selection, fresh proof or HWM is manufactured here.
    pub fn validate_at(&self, now: u64) -> Result<(), String> {
        self.validate()?;
        if now >= self.cleanup_until.get() {
            Err("termination V3 expired".into())
        } else {
            Ok(())
        }
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest(&(TERMINATION_PLAN_DOMAIN_V3, self))
    }
    pub fn approval_request(
        &self,
        request_id: String,
        context: ApprovalContextV1,
        now: u64,
    ) -> Result<ApprovalOptionsRequestV1, String> {
        self.validate_at(now)?;
        let result = ApprovalOptionsRequestV1 {
            approval_request_id: request_id,
            context,
            intent: CustodyIntentV1::TerminationPlanV3 {
                plan: Box::new(self.clone()),
            },
        };
        result.validate(now)?;
        Ok(result)
    }
}
