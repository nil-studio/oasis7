//! Explicit funded Identity metadata-read domain. Native audience in the issuer
//! pin remains its existing trust tuple; recipient authority is this fixed role,
//! route and complete actual Identity scope, never a caller-selected address.
use crate::{
    DecimalU64,
    http::Role,
    native_binding::SignedManagedNativeSessionLifetimeV1,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
    provisioning::{
        ExecutionPrivateCurrentHopV2, ExecutionPrivateCurrentMetadataV2,
        ExecutionPrivateCurrentPortV2, ParticipantSessionRefV1,
    },
};
use serde::{Deserialize, Serialize};
pub const EXECUTION_PRIVATE_CURRENT_DOMAIN_V2: &str = "oasis7.custody.execution-private-current.v2";
pub const EXECUTION_PRIVATE_CURRENT_ACTOR_PATH_V2: &str = "/internal/execution-private-current-v2";
pub const EXECUTION_PRIVATE_CURRENT_MAX_WIRE_V2: usize = 32768;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountExecutionPrivateCurrentV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub recipient_role: Role,
    pub recipient_path: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub action_request_id: String,
    pub stage_operation_id: String,
    pub factory_key: String,
    pub source_operation_id: String,
    pub accepted_ordinal: DecimalU64,
    pub session: ParticipantSessionRefV1,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub ports: [ExecutionPrivateCurrentPortV2; 2],
}
fn invalid() -> String {
    "execution private Current rejected".into()
}
impl AccountExecutionPrivateCurrentV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        let id = |v: &str| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control);
        let hash = |v: &str| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        self.session.scope.validate().map_err(|_| invalid())?;
        self.lifetime.lifetime.validate().map_err(|_| invalid())?;
        if self.api_version != 2
            || self.recipient_role != Role::Identity
            || self.recipient_path != EXECUTION_PRIVATE_CURRENT_ACTOR_PATH_V2
            || self.session.scope.role_scope.context.identity.role != Role::Identity
            || self.session.scope.role_scope.context.identity.environment != self.issuer.environment
            || ![
                &self.session_id,
                &self.action_request_id,
                &self.stage_operation_id,
                &self.source_operation_id,
            ]
            .iter()
            .all(|v| id(v))
            || !hash(&self.approved_input_digest)
            || self.factory_key.len() <= 256
            || self.factory_key.len() > 1024
            || self.factory_key.chars().any(char::is_control)
            || self.accepted_ordinal.0 == 0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.status.expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_idle_expires_at.0
            || self.issued_at.0 < self.lifetime.lifetime.status.observed_at.0
            || self.expires_at.0
                > self
                    .lifetime
                    .lifetime
                    .status
                    .observed_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
            || self.lifetime.lifetime.status.session_ref != self.session.session_ref
            || self.lifetime.lifetime.status.generation != self.session.native_binding_generation
            || self.ports[0].hop != ExecutionPrivateCurrentHopV2::AccountToIdentityHistory
            || self.ports[1].hop != ExecutionPrivateCurrentHopV2::IdentityHistoryToManifest
            || !self.ports.iter().all(|p| id(&p.physical_id))
            || self.ports[0].physical_id == self.ports[1].physical_id
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        crate::native_cleanup_v2::bytes(EXECUTION_PRIVATE_CURRENT_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountExecutionPrivateCurrentV2 {
    pub certificate: AccountExecutionPrivateCurrentV2,
    pub signature_hex: String,
}
impl SignedAccountExecutionPrivateCurrentV2 {
    pub fn verify_issuer(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.certificate.validate_shape()?;
        self.certificate.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionPrivateCurrentV2,
            self.certificate.issued_at.0,
            self.certificate.expires_at.0,
            now,
        )?;
        crate::native_cleanup_v2::verify(
            EXECUTION_PRIVATE_CURRENT_DOMAIN_V2,
            &self.certificate,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        crate::native_cleanup_v2::hash(&(EXECUTION_PRIVATE_CURRENT_DOMAIN_V2, self))
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.certificate.validate_shape()?;
        crate::native_cleanup_v2::decode::<64>(&self.signature_hex)?;
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        if bytes.len() > EXECUTION_PRIVATE_CURRENT_MAX_WIRE_V2 {
            return Err(invalid());
        }
        Ok(bytes)
    }
}
impl core::fmt::Debug for SignedAccountExecutionPrivateCurrentV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountExecutionPrivateCurrentV2(<unverified-public>)")
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrivateCurrentReplyV2 {
    pub api_version: u8,
    pub proof_digest: String,
    pub metadata: ExecutionPrivateCurrentMetadataV2,
}
