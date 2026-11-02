//! Closed read-only RF dispatcher and authoritative read receipts. These are
//! physical transport claims; neither is a global Current or control capability.
use crate::{DecimalU64, native_cleanup_v2::*, role_key_slot::RoleKeySlotScopeV1};
use serde::{Deserialize, Serialize};
pub const EXECUTION_FENCE_DISPATCH_DOMAIN_V2: &str = "oasis7.custody.execution-fence-dispatch.v2";
pub const EXECUTION_FENCE_READ_DOMAIN_V2: &str = "oasis7.custody.execution-fence-read.v2";
pub const EXECUTION_FENCE_DISPATCH_PATH_V2: &str = "/internal/execution-fence-dispatch-v2";
pub const EXECUTION_FENCE_READ_PATH_V2: &str = "/internal/execution-fence-read-v2";
fn rejected() -> String {
    "execution fence transport rejected".into()
}
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFencePhaseV2 {
    PendingPreInspect,
}
/// Fixed metadata only. No URL, arbitrary method/body, FENCES binding or write verb.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFenceBindingV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub scope: RoleKeySlotScopeV1,
    pub session_id: String,
    pub approved_input_digest: String,
    pub master_digest: String,
    pub source_operation_id: String,
    pub accepted_ordinal: DecimalU64,
    pub dispatcher_claim_ordinal: DecimalU64,
    pub phase: ExecutionFencePhaseV2,
    pub dispatcher_attempt_id: String,
    pub fence_attempt_id: String,
    pub dispatcher_object_id: String,
    pub fence_object_id: String,
    pub expected_service_epoch: DecimalU64,
    pub business_body_digest: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ExecutionFenceBindingV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.scope.validate().map_err(|_| rejected())?;
        if self.business_body_digest
            != crate::approval::bytes_digest(
                &serde_json::to_vec(&ExecutionFenceQueryV2 {
                    api_version: 2,
                    expected_service_epoch: self.expected_service_epoch,
                })
                .map_err(|_| rejected())?,
            )
        {
            return Err(rejected());
        }
        if self.api_version != 2
            || !id(&self.session_id)
            || !id(&self.source_operation_id)
            || !id(&self.dispatcher_attempt_id)
            || !id(&self.fence_attempt_id)
            || self.dispatcher_attempt_id == self.fence_attempt_id
            || !hex64(&self.approved_input_digest)
            || !hex64(&self.master_digest)
            || !hex64(&self.dispatcher_object_id)
            || !hex64(&self.fence_object_id)
            || self.accepted_ordinal.0 == 0
            || self.dispatcher_claim_ordinal.0 <= self.accepted_ordinal.0
            || self.expected_service_epoch.0 == 0
            || self.issued_at.0 == 0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 - self.issued_at.0 > 5000
            || self.scope.context.identity.role != crate::http::Role::Identity
            || self.scope.context.identity.environment != self.issuer.environment
        {
            return Err(rejected());
        }
        Ok(())
    }
    /// Stable receiver name excludes rotatable key/certificate/config epochs.
    pub fn receiver_name(&self) -> Result<String, String> {
        execution_fence_receiver_name_v2(&self.issuer, &self.dispatcher_attempt_id)
    }
}
/// Pure configured-namespace name calculation, before an object ID is known.
/// It confers no issuer verification, payment or dispatch permission.
pub fn execution_fence_receiver_name_v2(
    issuer: &AccountMachineIssuerV2,
    attempt: &str,
) -> Result<String, String> {
    if [&issuer.namespace, &issuer.service_id, &issuer.environment]
        .iter()
        .any(|v| !id(v))
        || !id(attempt)
    {
        return Err(rejected());
    }
    hash(&(
        "oasis7.custody.execution-fence-dispatch-object.v2",
        &issuer.namespace,
        &issuer.service_id,
        &issuer.environment,
        "execution_fence_dispatcher_v2",
        attempt,
    ))
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExecutionFenceDispatchV2 {
    pub binding: ExecutionFenceBindingV2,
    pub signature_hex: String,
}
impl SignedExecutionFenceDispatchV2 {
    pub fn signing_bytes(binding: &ExecutionFenceBindingV2) -> Result<Vec<u8>, String> {
        binding.validate_shape()?;
        bytes(EXECUTION_FENCE_DISPATCH_DOMAIN_V2, binding)
    }
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        actual_object_id: &str,
        now: u64,
    ) -> Result<(), String> {
        self.binding.validate_shape()?;
        if actual_object_id != self.binding.dispatcher_object_id {
            return Err(rejected());
        }
        self.binding.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionFenceDispatchV2,
            self.binding.issued_at.0,
            self.binding.expires_at.0,
            now,
        )?;
        verify(
            EXECUTION_FENCE_DISPATCH_DOMAIN_V2,
            &self.binding,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.binding.validate_shape()?;
        hash(self)
    }
}
/// Issued only after the Account SQL causal gate claims the one FENCES leaf.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFenceReadV2 {
    pub binding: ExecutionFenceBindingV2,
    pub dispatch_digest: String,
    pub claim_ordinal: DecimalU64,
    pub claimed_at: DecimalU64,
}
impl ExecutionFenceReadV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.binding.validate_shape()?;
        if !hex64(&self.dispatch_digest)
            || self.claim_ordinal.0 <= self.binding.dispatcher_claim_ordinal.0
            || self.claimed_at.0 < self.binding.issued_at.0
            || self.claimed_at.0 >= self.binding.expires_at.0
        {
            return Err(rejected());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(EXECUTION_FENCE_READ_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExecutionFenceReadV2 {
    pub claim: ExecutionFenceReadV2,
    pub signature_hex: String,
}
impl SignedExecutionFenceReadV2 {
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.claim.validate_shape()?;
        let b = &self.claim.binding;
        b.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionFenceReadV2,
            self.claim.claimed_at.0,
            b.expires_at.0,
            now,
        )?;
        verify(
            EXECUTION_FENCE_READ_DOMAIN_V2,
            &self.claim,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}

/// Fact-only private Actor reply. It is not a portable signature or Current.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFenceObservationV2 {
    pub api_version: u8,
    pub dispatch_digest: String,
    pub read_digest: String,
    pub fence: crate::recovery::RecoveryFence,
    pub observed_at: DecimalU64,
}
impl SignedExecutionFenceReadV2 {
    pub fn digest(&self) -> Result<String, String> {
        self.claim.validate_shape()?;
        hash(self)
    }
}

/// The only business query on this read-only route. No namespace, object,
/// arbitrary URL, caller capacity, Current timestamp or control verb.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFenceQueryV2 {
    pub api_version: u8,
    pub expected_service_epoch: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFenceReadRequestV2 {
    pub query: ExecutionFenceQueryV2,
    pub authorization: SignedExecutionFenceReadV2,
}
impl ExecutionFenceReadRequestV2 {
    pub fn validate_binding(&self) -> Result<(), String> {
        self.authorization.claim.validate_shape()?;
        if self.query.api_version != 2
            || self.query.expected_service_epoch
                != self.authorization.claim.binding.expected_service_epoch
            || crate::approval::bytes_digest(
                &serde_json::to_vec(&self.query).map_err(|_| rejected())?,
            ) != self.authorization.claim.binding.business_body_digest
        {
            return Err(rejected());
        }
        Ok(())
    }
}
