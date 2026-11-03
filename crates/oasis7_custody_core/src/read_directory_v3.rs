//! Closed Read Directory leaf. Complete signed originals are historical proof
//! material, never a fresh Native/Source/LF or global Current observation.
use crate::{
    DecimalU64,
    approval::{
        CustodyIntentV1, OriginStateV1, ProofStateV1, SignedApprovalVerificationV1,
        VerificationPurposeV1,
    },
    approval_admission::{ApprovalAdmissionInput, ApprovalAdmissionRecord},
    native_cleanup_v2::*,
    read_claim_source_v3::SignedReadClaimSourceV3,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
pub const READ_DIRECTORY_DOMAIN_V3: &str = "oasis7.custody.read-directory.v3";
pub const READ_DIRECTORY_PATH_V3: &str = "/internal/read-directory-v3";
pub const READ_DIRECTORY_MAX_WIRE_BYTES_V3: usize = 32768;
fn rejected() -> String {
    "read directory rejected".into()
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadDirectoryPhaseV3 {
    ClaimSource,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryManagerReferenceV3 {
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub account_id: String,
    pub session_ref: String,
    pub credential_ref: String,
    pub credential_version: DecimalU64,
    pub security_version: DecimalU64,
    pub policy_version: DecimalU64,
    pub expires_at: DecimalU64,
    pub idle_expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryOriginalV3 {
    pub input: ApprovalAdmissionInput,
    pub verification: SignedApprovalVerificationV1,
    pub verify_nonce: String,
}
impl ReadDirectoryOriginalV3 {
    pub fn from_record(record: &ApprovalAdmissionRecord) -> Result<Self, String> {
        Ok(Self {
            input: record.input().clone(),
            verification: record.verification().ok_or_else(rejected)?.clone(),
            verify_nonce: record.verify_nonce().into(),
        })
    }
    fn validate(&self, settled_at: u64) -> Result<(), String> {
        let p = &self.input.approval.proof;
        let v = &self.verification.verification;
        self.input.request.validate(p.issued_at.0)?;

        if p.version != 1
            || p.context != self.input.request.context
            || p.intent_digest != self.input.request.intent_digest()?
            || p.approval_request_id != self.input.request.approval_request_id
            || p.issued_at.0 >= p.expires_at.0
            || v.version != 1
            || v.purpose != VerificationPurposeV1::ConsumeApproval
            || v.proof_state != ProofStateV1::Active
            || v.origin_state != OriginStateV1::Active
            || v.proof_digest != self.input.approval.digest()?
            || v.intent_digest != p.intent_digest
            || v.approval_request_id != p.approval_request_id
            || v.audience != p.context.audience
            || v.identity_namespace != p.identity_namespace
            || v.identity_instance_epoch != p.identity_instance_epoch
            || v.verify_nonce != self.verify_nonce
            || !id(&self.verify_nonce)
            || v.manager_policy_version != p.manager_policy_version
            || v.manager_security_version != p.manager_security_version
            || v.origin_credential_version != p.origin_credential_version
            || v.credential_version != p.passkey_credential_version
            || v.observed_at.0 < p.issued_at.0
            || v.observed_at.0 >= p.expires_at.0
            || v.observed_at.0 > settled_at
            || v.proof_expires_at != p.expires_at
            || v.source_session_expires_at.0 <= v.observed_at.0
            || v.source_session_idle_expires_at.0 <= v.observed_at.0
        {
            return Err(rejected());
        }
        Ok(())
    }
}
impl ReadDirectoryManagerReferenceV3 {
    pub fn from_original(original: &ReadDirectoryOriginalV3) -> Self {
        let m = &original.input.approval.proof;
        let v = &original.verification.verification;
        Self {
            identity_namespace: m.identity_namespace.clone(),
            identity_instance_epoch: v.identity_instance_epoch,
            account_id: m.manager_account_id.clone(),
            session_ref: m.manager_session_ref.clone(),
            credential_ref: m.origin_credential_id.clone(),
            credential_version: v.origin_credential_version,
            security_version: v.manager_security_version,
            policy_version: v.manager_policy_version,
            expires_at: v.source_session_expires_at,
            idle_expires_at: v.source_session_idle_expires_at,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryQueryV3 {
    pub api_version: u8,
    pub phase: ReadDirectoryPhaseV3,
    pub manager_reference: ReadDirectoryManagerReferenceV3,
    pub provision_id: String,
    pub custody_account_id: String,
    pub original: ReadDirectoryOriginalV3,
}
impl ReadDirectoryQueryV3 {
    pub fn validate_against(&self, coordinator: &SignedReadClaimSourceV3) -> Result<(), String> {
        coordinator.claim.validate_shape()?;
        let c = &coordinator.claim;
        self.original.validate(c.settled_at.0)?;
        let input = &self.original.input;
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(rejected());
        };
        if self.api_version != 3
            || self.manager_reference
                != ReadDirectoryManagerReferenceV3::from_original(&self.original)
            || input.operation_id != c.session_id
            || scope.session_id != c.session_id
            || input.approved_test_session_v2_digest()? != c.approved_input_digest
            || self.provision_id != plan.termination.provision_id
            || self.provision_id != c.scope.provision_id
            || self.custody_account_id != plan.termination.custody_account_id
            || self.custody_account_id != c.scope.context.identity.account_id
            || input.request.context.environment != c.issuer.environment
        {
            return Err(rejected());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryClaimV3 {
    pub api_version: u8,
    pub coordinator: SignedReadClaimSourceV3,
    pub query: ReadDirectoryQueryV3,
    pub directory_object_id: String,
    pub physical_attempt_id: String,
    pub claim_ordinal: DecimalU64,
    pub claimed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ReadDirectoryClaimV3 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.query.validate_against(&self.coordinator)?;
        let c = &self.coordinator.claim;
        if self.api_version != 3
            || !hex64(&self.directory_object_id)
            || !hex64(&self.physical_attempt_id)
            || self.physical_attempt_id != read_directory_attempt_id_v3(&self.coordinator)?
            || self.claim_ordinal.0 <= c.account_claim_ordinal.0
            || self.claimed_at.0 < c.issued_at.0
            || self.claimed_at.0 >= self.expires_at.0
            || self.expires_at != c.expires_at
        {
            return Err(rejected());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(READ_DIRECTORY_DOMAIN_V3, self)
    }
}
pub fn read_directory_attempt_id_v3(
    coordinator: &SignedReadClaimSourceV3,
) -> Result<String, String> {
    let c = &coordinator.claim;
    c.validate_shape()?;
    hash(&(
        "oasis7.custody.read-directory-attempt.v3",
        &c.issuer.namespace,
        &c.issuer.service_id,
        &c.issuer.environment,
        &c.session_id,
        &c.ingress_attempt_id,
        &c.read_graph_digest,
        "claim_source.directory",
    ))
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReadDirectoryV3 {
    pub claim: ReadDirectoryClaimV3,
    pub signature_hex: String,
}
fn native_signature(bytes: Vec<u8>, signature: &str, key: &str) -> Result<(), String> {
    let raw = URL_SAFE_NO_PAD.decode(signature).map_err(|_| rejected())?;
    if URL_SAFE_NO_PAD.encode(&raw) != signature {
        return Err(rejected());
    }
    let signature = ed25519_dalek::Signature::from_slice(&raw).map_err(|_| rejected())?;
    ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(key)?)
        .map_err(|_| rejected())?
        .verify_strict(&bytes, &signature)
        .map_err(|_| rejected())
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryVerifierV3 {
    pub directory_pin: AccountMachineIssuerPinV2,
    pub coordinator_pin: AccountMachineIssuerPinV2,
    pub floor: AccountMachineIssuerFloorV2,
    pub native_key_id: String,
    pub native_public_key: String,
}
impl ReadDirectoryVerifierV3 {
    pub fn decode_configuration(raw: &[u8]) -> Result<Self, String> {
        let value: Self = strict_json::decode(raw, 32768)?;
        value.validate()?;
        Ok(value)
    }
    pub fn decode_history(raw: &[u8]) -> Result<Vec<Self>, String> {
        let values: Vec<Self> = strict_json::decode(raw, 32768)?;
        for value in &values {
            value.validate()?;
        }
        Ok(values)
    }

    pub fn validate(&self) -> Result<(), String> {
        let p = &self.directory_pin;
        let c = &self.coordinator_pin;
        if p.allowed_purpose != AccountMachinePurposeV2::ReadDirectoryV3
            || c.allowed_purpose != AccountMachinePurposeV2::ReadClaimSourceV3
            || p.namespace != c.namespace
            || p.service_id != c.service_id
            || p.environment != c.environment
            || p.key_id != c.key_id
            || p.public_key != c.public_key
            || p.issuer_epoch != c.issuer_epoch
            || p.config_generation != c.config_generation
            || p.native_audience_origin != c.native_audience_origin
            || p.public_key == self.native_public_key
            || p.key_id == self.native_key_id
            || !id(&self.native_key_id)
            || !hex64(&self.native_public_key)
            || self.floor.issuer_epoch.0 == 0
            || self.floor.config_generation.0 == 0
        {
            return Err(rejected());
        }
        let issuer = AccountMachineIssuerV2 {
            namespace: p.namespace.clone(),
            service_id: p.service_id.clone(),
            environment: p.environment.clone(),
            key_id: p.key_id.clone(),
            issuer_epoch: p.issuer_epoch,
            config_generation: p.config_generation,
            native_audience_origin: p.native_audience_origin.clone(),
        };
        for pin in [p, c] {
            issuer.verify_pin(
                pin,
                &self.floor,
                pin.allowed_purpose,
                pin.not_before.0,
                pin.not_after.0,
                pin.not_before.0,
            )?;
        }
        ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(&self.native_public_key)?)
            .map_err(|_| rejected())?;
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        hash(&("oasis7.custody.read-directory-verifier.v3", self))
    }
}
#[path = "read_directory_json_v3.rs"]
mod strict_json;
pub(crate) fn decode_saved_read_directory_v3<T: serde::de::DeserializeOwned + Serialize>(
    raw: &[u8],
    limit: usize,
) -> Result<T, String> {
    strict_json::decode(raw, limit)
}
impl SignedReadDirectoryV3 {
    pub fn decode_wire(raw: &[u8]) -> Result<Self, String> {
        let proof: Self = strict_json::decode(raw, READ_DIRECTORY_MAX_WIRE_BYTES_V3)?;
        proof.claim.validate_shape()?;
        Ok(proof)
    }
    pub fn verify(
        &self,
        verifier: &ReadDirectoryVerifierV3,
        actual_directory: &str,
        actual_account: &str,
        now: u64,
    ) -> Result<(), String> {
        verifier.validate()?;
        self.claim.validate_shape()?;
        let c = &self.claim.coordinator.claim;
        if self.claim.directory_object_id != actual_directory
            || self.claim.query.original.input.approval.proof.key_id != verifier.native_key_id
            || self.claim.query.original.verification.verification.key_id != verifier.native_key_id
        {
            return Err(rejected());
        }
        self.claim.coordinator.verify(
            &verifier.coordinator_pin,
            &verifier.floor,
            actual_account,
            now,
        )?;
        c.issuer.verify_pin(
            &verifier.directory_pin,
            &verifier.floor,
            AccountMachinePurposeV2::ReadDirectoryV3,
            self.claim.claimed_at.0,
            self.claim.expires_at.0,
            now,
        )?;
        verify(
            READ_DIRECTORY_DOMAIN_V3,
            &self.claim,
            &self.signature_hex,
            &verifier.directory_pin.public_key,
        )?;
        let original = &self.claim.query.original;
        original.input.request.validate_native_summary_contract(
            original
                .input
                .approval
                .proof
                .model_budget_summary_contract
                .as_ref(),
            &original.input.approval.proof.manager_account_id,
            Some(&verifier.native_public_key),
            original.input.approval.proof.issued_at.0,
        )?;
        native_signature(
            original.input.approval.proof.signing_bytes()?,
            &original.input.approval.signature,
            &verifier.native_public_key,
        )?;
        let observed = &original.verification;
        native_signature(
            observed.verification.signing_bytes()?,
            &observed.signature,
            &verifier.native_public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.claim.validate_shape()?;
        hash(self)
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.claim.validate_shape()?;
        let raw = serde_json::to_vec(self).map_err(|_| rejected())?;
        if raw.is_empty() || raw.len() > READ_DIRECTORY_MAX_WIRE_BYTES_V3 {
            return Err(rejected());
        }
        Ok(raw)
    }
}
