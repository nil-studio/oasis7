//! Closed Account-to-Owner Actor grant. Signature verification is not Current.
use crate::{
    DecimalU64,
    http::Role,
    native_cleanup_v2::{self as codec, *},
    provisioning::{PreparedGuardedStopSignatureV2, PublicRoleKey},
    role_key_slot::RoleKeySlotScopeV1,
};
use oasis7_agent_api::game::GameTaskStopV2;
use serde::{Deserialize, Serialize};
pub const TERM_GUARDED_STOP_SIGNER_DOMAIN_V2: &str = "oasis7.custody.term-guarded-stop-signer.v2";
pub const TERM_GUARDED_STOP_SIGNER_PATH_V2: &str = "/internal/role-key/term-guarded-stop-sign-v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermGuardedStopSignerPermitV2 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_lineage: Option<crate::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2>,
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub recipient_role: Role,
    pub recipient_path: String,
    pub session_id: String,
    pub approval_input_digest: String,
    pub source_operation_id: String,
    pub source_record_digest: String,
    pub paid_attempt_id: String,
    pub paid_record_digest: String,
    pub owner_scope: RoleKeySlotScopeV1,
    pub owner_key: PublicRoleKey,
    pub original: GameTaskStopV2,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTermGuardedStopSignerPermitV2 {
    pub payload: TermGuardedStopSignerPermitV2,
    pub signature_hex: String,
}
fn denied() -> String {
    "Term Guarded signer unavailable".into()
}
impl PreparedGuardedStopSignatureV2 {
    /// Pure fixed wire projection of the one actual durable claim. Only the
    /// shared issuer child can turn it into a configured Account signature.
    pub fn signer_permit(&self, issuer: AccountMachineIssuerV2) -> TermGuardedStopSignerPermitV2 {
        TermGuardedStopSignerPermitV2 {
            account_lineage: Some(self.lineage().clone()),
            api_version: 2,
            issuer,
            recipient_role: Role::Owner,
            recipient_path: TERM_GUARDED_STOP_SIGNER_PATH_V2.into(),
            session_id: self.session_id().into(),
            approval_input_digest: self.input_digest().into(),
            source_operation_id: self.source_operation_id().into(),
            source_record_digest: self.source_record_digest().into(),
            paid_attempt_id: self.attempt_id().into(),
            paid_record_digest: self.paid_record_digest().into(),
            owner_scope: self.owner_scope().clone(),
            owner_key: self.owner_key().clone(),
            original: self.payload().clone(),
            issued_at: DecimalU64(self.issued_at().get()),
            expires_at: DecimalU64(self.until().get()),
        }
    }
}
impl TermGuardedStopSignerPermitV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.owner_scope.validate().map_err(|_| denied())?;
        if let Some(lineage) = &self.account_lineage {
            lineage.validate_for(&self.owner_scope)?;
        }
        self.original.validate()?;
        if self.api_version != 2
            || self.recipient_role != Role::Owner
            || self.recipient_path != TERM_GUARDED_STOP_SIGNER_PATH_V2
            || ![
                &self.session_id,
                &self.source_operation_id,
                &self.paid_attempt_id,
            ]
            .into_iter()
            .all(|v| codec::id(v))
            || !codec::hex64(&self.approval_input_digest)
            || !codec::hex64(&self.source_record_digest)
            || !codec::hex64(&self.paid_record_digest)
            || self.owner_scope.context.identity.role != Role::Owner
            || self.owner_scope.context.identity.account_id != self.original.custody_account_id
            || self.owner_scope.context.identity.environment != self.issuer.environment
            || self.owner_scope.context.identity.key_id != self.owner_key.key_id
            || self.owner_scope.context.identity.key_version != self.owner_key.key_version
            || self.owner_key.key_version.0 == 0
            || self.owner_key.public_key != self.original.initial_fence.owner_public_key
            || self.original.observation_grant.is_none()
            || self.issued_at.0 == 0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.issued_at.0.checked_add(5000).ok_or_else(denied)?
            || self.expires_at.0 > self.original.execute_until_ms.0
        {
            return Err(denied());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        codec::bytes(TERM_GUARDED_STOP_SIGNER_DOMAIN_V2, self)
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        codec::hash(self)
    }
}
impl SignedTermGuardedStopSignerPermitV2 {
    /// The caller supplies fixed configuration and the exact opened slot. This
    /// returns no key/Current/dispatch capability and never initializes storage.
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        scope: &RoleKeySlotScopeV1,
        key: &PublicRoleKey,
        now: u64,
    ) -> Result<(), String> {
        self.payload.validate()?;
        self.payload.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::TermGuardedStopSignerV2,
            self.payload.issued_at.0,
            self.payload.expires_at.0,
            now,
        )?;
        if &self.payload.owner_scope != scope || &self.payload.owner_key != key {
            return Err(denied());
        }
        codec::verify(
            TERM_GUARDED_STOP_SIGNER_DOMAIN_V2,
            &self.payload,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
