//! Two fixed World reads inside the separately approved preflight profile.
//! This certificate does not spend Native160 or authorize a business mutation.
use super::*;
use crate::approval_admission::ApprovalAdmissionInput;
use crate::provisioning::{ParticipantSessionRefV1, TestSessionPreflightDescriptorV2};
pub const PREFLIGHT_ACCESS_DOMAIN_V2: &str = "oasis7.custody.native-preflight-access.v2";
pub const NATIVE_PREFLIGHT_POLICY_V2_PATH: &str =
    "/api/public/hosted-account/v2/participant/test-session-v2/policy-original";
pub const NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH: &str =
    "/api/public/hosted-account/v2/participant/test-session-v2/authorize";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePreflightWorldOperationV2 {
    PolicyOriginal,
    AuthorizePolicy,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightWorldPortV2 {
    pub operation: NativePreflightWorldOperationV2,
    pub physical_attempt_id: String,
    pub native_path: String,
    pub world_path: String,
    pub body_digest: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountPreflightAccessV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub certificate_id: String,
    pub input: Box<ApprovalAdmissionInput>,
    pub approved_input_digest: String,
    pub descriptor: Box<TestSessionPreflightDescriptorV2>,
    pub source_record_digest: String,
    pub fresh_access_claimed_at: DecimalU64,
    pub consumed_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub ports: [NativePreflightWorldPortV2; 2],
}
impl AccountPreflightAccessV2 {
    /// Typed canonical metadata codecs, not a signing or permission interface.
    pub fn expected_input_digest(input: &ApprovalAdmissionInput) -> Result<String, String> {
        hash(&("oasis7.custody.test-session-approved-input.v2", input))
    }
    pub fn policy_body_digest(body: &PreflightPolicyBodyV2) -> Result<String, String> {
        hash(body)
    }
    pub fn authorize_body_digest(
        body: &oasis7_client_api::world_service::SignedReadRequest<
            oasis7_agent_api::game::GameParticipantRegisterV1,
        >,
    ) -> Result<String, String> {
        hash(body)
    }
    pub fn validate_shape(&self) -> Result<(), String> {
        let d = &self.descriptor;
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, scope } =
            &self.input.request.intent
        else {
            return Err(invalid());
        };
        let expected_input = Self::expected_input_digest(&self.input)?;
        if self.api_version != 2
            || !hex64(&self.certificate_id)
            || !hex64(&self.source_record_digest)
            || self.approved_input_digest != expected_input
            || d.api_version != 2
            || d.approved_input_digest != expected_input
            || d.session_id != self.input.operation_id
            || d.session_id != scope.session_id
            || d.cleanup_transport_policy
                != Some(
                    crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2,
                )
            || d.world != plan.termination.world
            || self.issuer.environment != self.input.request.context.environment
            || d.native_binding.environment != self.input.request.context.environment
            || d.native_binding.custody_account_id != plan.termination.custody_account_id
            || d.native_binding.provision_id != plan.termination.provision_id
            || d.native_binding.native_account_id != scope.native_account_id
            || d.participant_owner.request.account_id != scope.native_account_id
            || d.participant_owner.subject_public_key
                != plan.termination.initial_fence.owner_public_key
            || d.participant_owner.request.owner_public_key
                != plan.termination.initial_fence.owner_public_key
            || d.session.scope.role_scope.context.identity.environment
                != self.input.request.context.environment
            || d.session.scope.role_scope.context.identity.account_id
                != plan.termination.custody_account_id
            || d.session.scope.role_scope.provision_id != plan.termination.provision_id
            || self.consumed_at.0 > self.fresh_access_claimed_at.0
            || self.fresh_access_claimed_at.0 > self.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > d.expires_at.get()
            || self.expires_at.0 > self.issued_at.0.checked_add(5000).ok_or_else(invalid)?
            || plan.termination.cleanup_transport_policy
                != crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
        {
            return Err(invalid());
        }
        self.input.request.validate(self.consumed_at.0)?;
        if self.input.approval.proof.approval_request_id != self.input.request.approval_request_id
            || self.input.approval.proof.context != self.input.request.context
            || self.input.approval.proof.intent_digest != self.input.request.intent_digest()?
        {
            return Err(invalid());
        }
        let policy_body = PreflightPolicyBodyV2 {
            observation_nonce: d.access_nonces.policy.clone(),
        };
        let expected = [
            (
                NativePreflightWorldOperationV2::PolicyOriginal,
                NATIVE_PREFLIGHT_POLICY_V2_PATH,
                Self::policy_body_digest(&policy_body)?,
            ),
            (
                NativePreflightWorldOperationV2::AuthorizePolicy,
                NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
                Self::authorize_body_digest(&d.participant_owner)?,
            ),
        ];
        for (port, (operation, path, body)) in self.ports.iter().zip(expected) {
            if port.operation != operation
                || port.native_path != path
                || port.world_path != "/v1/world/participant-policy"
                || port.body_digest != body
                || !hex64(&port.physical_attempt_id)
            {
                return Err(invalid());
            }
        }
        if self.ports[0].physical_attempt_id == self.ports[1].physical_attempt_id {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(PREFLIGHT_ACCESS_DOMAIN_V2, self)
    }
    pub fn participant_session(&self) -> &ParticipantSessionRefV1 {
        &self.descriptor.session
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountPreflightAccessV2 {
    pub certificate: AccountPreflightAccessV2,
    pub signature_hex: String,
}
impl SignedAccountPreflightAccessV2 {
    pub fn verify_issuer(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        let c = &self.certificate;
        c.validate_shape()?;
        c.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::NativePreflightAccessV2,
            c.issued_at.0,
            c.expires_at.0,
            now,
        )?;
        verify(
            PREFLIGHT_ACCESS_DOMAIN_V2,
            c,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightPolicyBodyV2 {
    pub observation_nonce: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightPolicyRequestV2 {
    pub proof: Box<SignedAccountPreflightAccessV2>,
    pub body: PreflightPolicyBodyV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePreflightAuthorizeRequestV2 {
    pub proof: Box<SignedAccountPreflightAccessV2>,
    pub body: oasis7_client_api::world_service::SignedReadRequest<
        oasis7_agent_api::game::GameParticipantRegisterV1,
    >,
}
impl core::fmt::Debug for SignedAccountPreflightAccessV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountPreflightAccessV2(<private>)")
    }
}
