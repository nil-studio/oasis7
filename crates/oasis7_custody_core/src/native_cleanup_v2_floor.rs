//! Native proof is a fact. Only the separately configured Account response
//! attests its actual durable floor; neither value alone is dispatch authority.
use super::*;
use crate::native_binding::SignedManagedNativeSessionLifetimeV1;
pub const NATIVE_CURRENT_FACT_V2_DOMAIN: &str = "oasis7.custody.native-current-fact.v2";
pub const ACCOUNT_FINAL_CURRENT_V2_DOMAIN: &str = "oasis7.custody.account-native-final-current.v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStorageHeadV2 {
    pub instance_id: String,
    pub revision: DecimalU64,
    pub commitment: String,
}
impl NativeStorageHeadV2 {
    pub fn validate(&self) -> Result<(), String> {
        if !hex64(&self.instance_id) || !hex64(&self.commitment) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeMachineOperationV2 {
    ObserveGuardedStop,
    ObserveConditionalClose,
    ObserveStopKnownLookup,
    ObserveCloseKnownLookup,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMachineCurrentFactV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub identity_namespace: String,
    pub identity_epoch: DecimalU64,
    pub custody_account_id: String,
    pub session_id: String,
    pub approved_input_digest: String,
    pub active_certificate_digest: String,
    pub opaque_registration_ref: String,
    pub operation: NativeMachineOperationV2,
    pub operation_id: String,
    pub source_operation_id: String,
    pub native_transport_attempt_id: String,
    pub native_request_digest: String,
    pub nonce: String,
    pub origin_nonce: String,
    pub lifetime_nonce: String,
    pub storage_head: NativeStorageHeadV2,
    pub registry_cursor: DecimalU64,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
    pub origin: crate::approval::SignedApprovalVerificationV1,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeMachineCurrentFactV2 {
    pub fact: NativeMachineCurrentFactV2,
    pub signature_hex: String,
}
impl NativeMachineCurrentFactV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.storage_head.validate()?;
        self.lifetime.validate().map_err(|_| invalid())?;
        if self.api_version != 2
            || ![
                &self.native_key_id,
                &self.identity_namespace,
                &self.custody_account_id,
                &self.session_id,
                &self.opaque_registration_ref,
                &self.operation_id,
                &self.source_operation_id,
                &self.native_transport_attempt_id,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.approved_input_digest,
                &self.active_certificate_digest,
                &self.native_request_digest,
                &self.nonce,
                &self.origin_nonce,
                &self.lifetime_nonce,
            ]
            .iter()
            .all(|s| hex64(s))
            || self.origin.verification.key_id != self.native_key_id
            || self.origin.verification.identity_namespace != self.identity_namespace
            || self.origin.verification.identity_instance_epoch != self.identity_epoch
            || self.origin.verification.purpose
                != crate::approval::VerificationPurposeV1::InspectOrigin
            || self.origin.verification.origin_state != crate::approval::OriginStateV1::Active
            || self.origin.verification.verify_nonce != self.origin_nonce
            || self.lifetime.lifetime.status.nonce != self.lifetime_nonce
            || self.origin.verification.observed_at != self.issued_at
            || self.expires_at.0 > self.origin.verification.source_session_expires_at.0
            || self.expires_at.0 > self.origin.verification.source_session_idle_expires_at.0
            || !self.lifetime.lifetime.status.active
            || self.lifetime.lifetime.status.frozen
            || self.lifetime.lifetime.status.pending
            || self.identity_epoch.0 == 0
            || self.registry_cursor.0 == 0
            || self.identity_namespace != self.lifetime.lifetime.status.receipt.identity_namespace
            || self.identity_epoch
                != self
                    .lifetime
                    .lifetime
                    .status
                    .receipt
                    .identity_instance_epoch
            || self.expires_at.0 <= self.issued_at.0
            || self.expires_at.0 > self.issued_at.0.checked_add(5000).ok_or_else(invalid)?
            || self.lifetime.lifetime.status.observed_at.0 != self.issued_at.0
            || self.expires_at.0 > self.lifetime.lifetime.status.expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_expires_at.0
            || self.expires_at.0 > self.lifetime.lifetime.session_idle_expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_envelope(&self, request: &NativeCurrentEnvelopeV2) -> Result<(), String> {
        self.validate_shape()?;
        let body = &request.body;
        let paid = &request.paid.edge;
        if self.session_id != body.session_id
            || self.operation != body.operation
            || self.operation_id != body.operation_id
            || self.source_operation_id != body.source_operation_id
            || self.nonce != body.nonce
            || self.origin_nonce != body.origin_nonce
            || self.lifetime_nonce != body.lifetime_nonce
            || self.native_request_digest != request.request_digest()?
            || self.native_transport_attempt_id != paid.native_transport_attempt_id
            || self.approved_input_digest != paid.approved_input_digest
            || self.active_certificate_digest != paid.active_certificate_digest
            || self.opaque_registration_ref != paid.opaque_registration_ref
            || self.native_key_id != paid.native_key_id
            || self.identity_namespace != paid.identity_namespace
            || self.identity_epoch != paid.identity_epoch
            || self.issued_at.0 < paid.issued_at.0
            || self.expires_at.0 > paid.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_approval(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        ack: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        self.validate_shape()?;
        active.validate_registration(pending, ack)?;
        let c = &pending.certificate;
        let p = &c.input.approval.proof;
        let v = &self.origin.verification;
        let expired = self.issued_at.0 >= p.expires_at.0;
        if self.active_certificate_digest != active.digest()?
            || self.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.identity_namespace != c.input.approval.proof.identity_namespace
            || self.identity_epoch != c.input.approval.proof.identity_instance_epoch
            || self.lifetime.lifetime.status.receipt != c.native_lifetime.lifetime.status.receipt
            || self.approved_input_digest != c.approved_input_digest
            || self.session_id != active.certificate.session_id
            || self.custody_account_id != c.input.request.context.custody_account_id
            || self.issued_at.0 < active.certificate.issued_at.0
            || self.expires_at.0 > active.certificate.expires_at.0
            || self.lifetime.lifetime.status.session_ref != c.participant_session.session_ref
            || self.lifetime.lifetime.status.generation
                != c.participant_session.native_binding_generation
            || v.proof_digest != c.input.approval.digest()?
            || v.approval_request_id != p.approval_request_id
            || v.audience != c.input.request.context.audience
            || v.intent_digest != c.intent_digest
            || v.manager_policy_version != p.manager_policy_version
            || v.manager_security_version != p.manager_security_version
            || v.origin_credential_version != p.origin_credential_version
            || v.proof_expires_at != p.expires_at
            || v.proof_state
                != if expired {
                    crate::approval::ProofStateV1::Expired
                } else {
                    crate::approval::ProofStateV1::Active
                }
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        bytes(NATIVE_CURRENT_FACT_V2_DOMAIN, self)
    }
}
impl SignedNativeMachineCurrentFactV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(NATIVE_CURRENT_FACT_V2_DOMAIN, self))
    }
    pub fn verify_native(&self, key_id: &str, public_key: &str, now: u64) -> Result<(), String> {
        let f = &self.fact;
        f.validate_shape()?;
        if f.native_key_id != key_id || f.issued_at.0 > now || now >= f.expires_at.0 {
            return Err(invalid());
        }
        f.lifetime
            .verify_root(key_id, public_key)
            .map_err(|_| invalid())?;
        use base64::Engine;
        let signature: [u8; 64] = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&f.origin.signature)
            .map_err(|_| invalid())?
            .try_into()
            .map_err(|_| invalid())?;
        ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(public_key)?)
            .map_err(|_| invalid())?
            .verify_strict(
                &f.origin.verification.signing_bytes()?,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| invalid())?;
        verify(
            NATIVE_CURRENT_FACT_V2_DOMAIN,
            f,
            &self.signature_hex,
            public_key,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExternalFloorSnapshotV2 {
    pub native_store_instance_id: String,
    pub minimum_native_revision: DecimalU64,
    pub native_revision_commitment: String,
    pub registry_cursor: DecimalU64,
    pub native_identity_epoch: DecimalU64,
    pub native_key_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountNativeFinalCurrentV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub session_id: String,
    pub approved_input_digest: String,
    pub native_fact_digest: String,
    pub native_nonce: String,
    pub native_transport_attempt_id: String,
    pub identity_attestation_digest: String,
    pub source_operation_id: String,
    pub source_record_digest: String,
    pub external_floor: NativeExternalFloorSnapshotV2,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountNativeFinalCurrentV2 {
    pub current: AccountNativeFinalCurrentV2,
    pub signature_hex: String,
}
impl AccountNativeFinalCurrentV2 {
    pub fn validate_relative(
        &self,
        native: &SignedNativeMachineCurrentFactV2,
    ) -> Result<(), String> {
        native.fact.validate_shape()?;
        let n = &native.fact;
        let f = &self.external_floor;
        if self.api_version != 2
            || self.session_id != n.session_id
            || self.approved_input_digest != n.approved_input_digest
            || self.native_fact_digest != native.digest()?
            || self.native_nonce != n.nonce
            || self.native_transport_attempt_id != n.native_transport_attempt_id
            || self.source_operation_id != n.source_operation_id
            || !id(&self.source_operation_id)
            || !hex64(&self.source_record_digest)
            || !hex64(&self.identity_attestation_digest)
            || f.native_store_instance_id != n.storage_head.instance_id
            || f.minimum_native_revision != n.storage_head.revision
            || f.native_revision_commitment != n.storage_head.commitment
            || f.registry_cursor != n.registry_cursor
            || f.native_identity_epoch != n.identity_epoch
            || f.native_key_id != n.native_key_id
            || self.issued_at.0 < n.issued_at.0
            || self.expires_at.0 <= self.issued_at.0
            || self.expires_at.0 > n.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SignedAccountNativeFinalCurrentV2 {
    pub fn verify_issuer(
        &self,
        native: &SignedNativeMachineCurrentFactV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        let c = &self.current;
        c.validate_relative(native)?;
        c.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::NativeFinalCurrent,
            c.issued_at.0,
            c.expires_at.0,
            now,
        )?;
        verify(
            ACCOUNT_FINAL_CURRENT_V2_DOMAIN,
            c,
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
impl core::fmt::Debug for NativeMachineCurrentFactV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeMachineCurrentFactV2(<private>)")
    }
}
impl core::fmt::Debug for SignedNativeMachineCurrentFactV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeMachineCurrentFactV2(<private>)")
    }
}
impl core::fmt::Debug for AccountNativeFinalCurrentV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountNativeFinalCurrentV2(<private>)")
    }
}
impl core::fmt::Debug for SignedAccountNativeFinalCurrentV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountNativeFinalCurrentV2(<private>)")
    }
}
