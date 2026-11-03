//! One signed physical Owner/Account callback, not Source, Native or Current.
use crate::{DecimalU64, native_cleanup_v2::*, role_key_slot::RoleKeySlotScopeV1};
use serde::{Deserialize, Serialize};
fn invalid() -> String {
    "read claim source rejected".into()
}
pub const READ_CLAIM_SOURCE_DOMAIN_V3: &str = "oasis7.custody.read-claim-source.v3";
pub const READ_CLAIM_SOURCE_PATH_V3: &str = "/internal/read-claim-source-v3";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadClaimSourceV3 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub scope: RoleKeySlotScopeV1,
    pub session_id: String,
    pub ingress_attempt_id: String,
    pub approved_input_digest: String,
    pub original_digest: String,
    pub read_request: crate::provisioning::SessionActionRequestV2,
    pub signed_attempt_digest: String,
    pub credential_generation: DecimalU64,
    pub read_graph_digest: String,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub account_object_id: String,
    pub owner_attempt_id: String,
    pub account_attempt_id: String,
    pub reservation_ordinal: DecimalU64,
    pub owner_claim_ordinal: DecimalU64,
    pub account_claim_ordinal: DecimalU64,
    pub settled_at: DecimalU64,
    pub root_read_deadline: DecimalU64,
    pub request_until: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ReadClaimSourceV3 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.scope.validate().map_err(|_| invalid())?;
        if self.read_request.api_version != 2
            || self.read_request.session_id != self.session_id
            || !id(&self.read_request.request_id)
            || !hex64(&self.signed_attempt_digest)
            || self.credential_generation.0 == 0
            || !matches!(&self.read_request.action, crate::provisioning::SessionToolActionV1::ReconcileOperation { operation_id } if id(operation_id))
        {
            return Err(invalid());
        }
        if self.api_version != 3
            || !id(&self.session_id)
            || !id(&self.ingress_attempt_id)
            || !id(&self.source_operation_id)
            || [
                &self.approved_input_digest,
                &self.original_digest,
                &self.read_graph_digest,
                &self.source_nonce,
                &self.account_object_id,
                &self.owner_attempt_id,
                &self.account_attempt_id,
            ]
            .iter()
            .any(|v| !hex64(v))
            || self.owner_attempt_id == self.account_attempt_id
            || self.reservation_ordinal.0 == 0
            || self.owner_claim_ordinal.0 <= self.reservation_ordinal.0
            || self.owner_claim_ordinal.0.checked_add(1) != Some(self.account_claim_ordinal.0)
            || self.settled_at.0 == 0
            || self.issued_at.0 < self.settled_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.request_until.0
            || self.request_until.0 > self.root_read_deadline.0
            || self
                .settled_at
                .0
                .checked_add(5000)
                .is_none_or(|until| self.expires_at.0 > until)
            || self.scope.context.identity.role != crate::http::Role::Identity
            || self.scope.context.identity.environment != self.issuer.environment
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(READ_CLAIM_SOURCE_DOMAIN_V3, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReadClaimSourceV3 {
    pub claim: ReadClaimSourceV3,
    pub signature_hex: String,
}
impl SignedReadClaimSourceV3 {
    pub fn digest(&self) -> Result<String, String> {
        self.claim.validate_shape()?;
        hash(self)
    }
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        actual_account: &str,
        now: u64,
    ) -> Result<(), String> {
        self.claim.validate_shape()?;
        let c = &self.claim;
        if c.account_object_id != actual_account {
            return Err(invalid());
        }
        c.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ReadClaimSourceV3,
            c.issued_at.0,
            c.expires_at.0,
            now,
        )?;
        verify(
            READ_CLAIM_SOURCE_DOMAIN_V3,
            c,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
