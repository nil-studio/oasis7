//! One fixed master-paid Current read leaf. A signature identifies its prior
//! Account SQL claim; it is neither a World capability nor an executable action.
use crate::{
    DecimalU64, approval::CustodySessionStatusV1, native_cleanup_v2::*,
    role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize};
pub const EXECUTION_PENDING_CURRENT_DOMAIN_V2: &str =
    "oasis7.custody.execution-pending-current-leaf.v2";
pub const EXECUTION_PENDING_DIRECTORY_PATH_V2: &str = "/internal/execution-pending-directory-v2";
pub const EXECUTION_PENDING_FENCE_PATH_V2: &str = "/internal/execution-pending-fence-v2";
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCurrentPhaseV2 {
    PreInspect,
    PreArchive,
    BeforePrivateRead,
    AfterPrivateRead,
    BeforeLifetime,
    Complete,
    PreparePrefix,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCurrentReceiverV2 {
    ManagerAssociationsActor,
    RecoveryFenceDispatcherActor,
    RecoveryFenceActor,
}
fn invalid() -> String {
    "pending Current leaf rejected".into()
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPendingCurrentLeafV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub scope: RoleKeySlotScopeV1,
    pub session_id: String,
    pub approved_input_digest: String,
    pub expected_service_epoch: DecimalU64,
    pub master_digest: String,
    pub accepted_ordinal: DecimalU64,
    pub claim_ordinal: DecimalU64,
    pub physical_attempt_id: String,
    pub phase: PendingCurrentPhaseV2,
    pub receiver: PendingCurrentReceiverV2,
    pub manager: CustodySessionStatusV1,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ExecutionPendingCurrentLeafV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.scope.validate().map_err(|_| invalid())?;
        if self.api_version != 2
            || !id(&self.session_id)
            || !id(&self.physical_attempt_id)
            || !hex64(&self.approved_input_digest)
            || !hex64(&self.master_digest)
            || self.expected_service_epoch.0 == 0
            || self.accepted_ordinal.0 == 0
            || self.claim_ordinal.0 <= self.accepted_ordinal.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 - self.issued_at.0 > 5000
            || self.manager.api_version != 1
            || self.manager.identity_instance_epoch.0 == 0
            || self.manager.security_version.0 == 0
            || self.manager.policy_version.0 == 0
            || self.manager.credential_version.0 == 0
            || [
                &self.manager.identity_namespace,
                &self.manager.account_id,
                &self.manager.session_ref,
                &self.manager.credential_ref,
            ]
            .iter()
            .any(|v| !id(v))
            || self.issued_at.0 >= self.manager.expires_at.0
            || self.issued_at.0 >= self.manager.idle_expires_at.0
            || self.expires_at.0 > self.manager.expires_at.0
            || self.expires_at.0 > self.manager.idle_expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(EXECUTION_PENDING_CURRENT_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExecutionPendingCurrentLeafV2 {
    pub claim: ExecutionPendingCurrentLeafV2,
    pub signature_hex: String,
}
impl SignedExecutionPendingCurrentLeafV2 {
    pub fn verify_issuer(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        receiver: PendingCurrentReceiverV2,
        now: u64,
    ) -> Result<(), String> {
        self.claim.validate_shape()?;
        if self.claim.receiver != receiver {
            return Err(invalid());
        }
        self.claim.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionPendingCurrentLeafV2,
            self.claim.issued_at.0,
            self.claim.expires_at.0,
            now,
        )?;
        verify(
            EXECUTION_PENDING_CURRENT_DOMAIN_V2,
            &self.claim,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.claim.validate_shape()?;
        hash(self)
    }
}
