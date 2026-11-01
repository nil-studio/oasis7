//! Registration acknowledgement is Native evidence, not an Active certificate.
use super::*;
pub const NATIVE_REGISTRATION_ACK_V2_DOMAIN: &str = "oasis7.custody.native-registration-ack.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRegistrationAckV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub identity_namespace: String,
    pub identity_epoch: DecimalU64,
    pub session_id: String,
    pub approved_input_digest: String,
    pub pending_certificate_digest: String,
    pub owner_read_response_digest: String,
    pub opaque_registration_ref: String,
    pub managed_session_ref: String,
    pub managed_binding_generation: DecimalU64,
    pub registration_nonce: String,
    pub storage_head: NativeStorageHeadV2,
    pub registry_cursor: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeRegistrationAckV2 {
    pub ack: NativeRegistrationAckV2,
    pub signature_hex: String,
}
impl NativeRegistrationAckV2 {
    pub fn validate_relative(
        &self,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<(), String> {
        pending.certificate.validate_shape()?;
        self.storage_head.validate()?;
        let c = &pending.certificate;
        let crate::approval::CustodyIntentV1::TestSessionV2 { scope, .. } = &c.input.request.intent
        else {
            return Err(invalid());
        };
        if self.api_version != 2
            || ![
                &self.native_key_id,
                &self.identity_namespace,
                &self.session_id,
                &self.opaque_registration_ref,
                &self.managed_session_ref,
            ]
            .iter()
            .all(|v| id(v))
            || !hex64(&self.registration_nonce)
            || self.identity_epoch.0 == 0
            || self.registry_cursor.0 == 0
            || self.identity_namespace
                != c.native_lifetime.lifetime.status.receipt.identity_namespace
            || self.identity_epoch
                != c.native_lifetime
                    .lifetime
                    .status
                    .receipt
                    .identity_instance_epoch
            || self.session_id != scope.session_id
            || self.approved_input_digest != c.approved_input_digest
            || self.pending_certificate_digest != pending.digest()?
            || !hex64(&self.owner_read_response_digest)
            || self.managed_session_ref != c.participant_session.session_ref
            || self.managed_binding_generation != c.participant_session.native_binding_generation
            || self.issued_at.0 < c.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > c.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(
        &self,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<Vec<u8>, String> {
        self.validate_relative(pending)?;
        bytes(NATIVE_REGISTRATION_ACK_V2_DOMAIN, self)
    }
}
impl SignedNativeRegistrationAckV2 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(NATIVE_REGISTRATION_ACK_V2_DOMAIN, self))
    }
    pub fn verify_native(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        key_id: &str,
        public_key: &str,
        namespace: &str,
        epoch: u64,
        now: u64,
    ) -> Result<(), String> {
        self.ack.validate_relative(pending)?;
        if self.ack.native_key_id != key_id
            || self.ack.identity_namespace != namespace
            || self.ack.identity_epoch.0 != epoch
            || self.ack.issued_at.0 > now
            || now >= self.ack.expires_at.0
        {
            return Err(invalid());
        }
        verify(
            NATIVE_REGISTRATION_ACK_V2_DOMAIN,
            &self.ack,
            &self.signature_hex,
            public_key,
        )
    }
}
impl SignedAccountActiveThreeRootsV2 {
    pub fn validate_registration(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        ack: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        self.certificate.validate_relative(pending)?;
        ack.ack.validate_relative(pending)?;
        if self.certificate.native_registration_ack_digest != ack.digest()?
            || self.certificate.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.certificate.issued_at.0 < ack.ack.issued_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl core::fmt::Debug for NativeRegistrationAckV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeRegistrationAckV2(<private>)")
    }
}
impl core::fmt::Debug for SignedNativeRegistrationAckV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeRegistrationAckV2(<private>)")
    }
}

/// Fixed replacement of the already claimed V2 OwnerRead, not a resource probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeRegistrationReadProfileV2 {
    ExactMove0OwnerView,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRegistrationReadClaimV2 {
    pub profile: NativeRegistrationReadProfileV2,
    pub source_operation_id: String,
    pub source_record_digest: String,
    pub preflight_record_digest: String,
    pub owner_read_original_digest: String,
    pub native_transport_attempt_id: String,
    pub world_transport_attempt_id: String,
    pub native_http_units: DecimalU64,
    pub world_http_units: DecimalU64,
    pub claimed_at: DecimalU64,
}
impl NativeRegistrationReadClaimV2 {
    pub fn validate(&self) -> Result<(), String> {
        if ![
            &self.source_operation_id,
            &self.native_transport_attempt_id,
            &self.world_transport_attempt_id,
        ]
        .iter()
        .all(|v| id(v))
            || self.native_transport_attempt_id == self.world_transport_attempt_id
            || ![
                &self.source_record_digest,
                &self.preflight_record_digest,
                &self.owner_read_original_digest,
            ]
            .iter()
            .all(|v| hex64(v))
            || self.native_http_units.0 != 1
            || self.world_http_units.0 != 1
            || self.claimed_at.0 == 0
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn registration_read_digest(
    request: &oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1,
) -> Result<String, String> {
    hash(&("oasis7.custody.preflight-owner-read-original.v2", request))
}
impl core::fmt::Debug for NativeRegistrationReadClaimV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeRegistrationReadClaimV2(<private>)")
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReadAndRegisterRequestV2 {
    pub api_version: u8,
    pub pending: AccountConsumedPendingReferenceV2,
    pub owner_read:
        oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1,
    pub registration_nonce: String,
}
impl NativeReadAndRegisterRequestV2 {
    /// Every request field is either fixed or derived from the one signed
    /// Pending certificate. Nonce variation must not repurpose its paid slot.
    pub fn validate_relative(
        &self,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<(), String> {
        self.validate()?;
        if self.pending != pending.compact_reference()?
            || self.registration_nonce != native_registration_nonce_v2(pending)?
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != 2
            || !hex64(&self.registration_nonce)
            || self.pending.preflight_read_claim.owner_read_original_digest
                != registration_read_digest(&self.owner_read)?
        {
            return Err(invalid());
        }
        self.pending.preflight_read_claim.validate()
    }
    pub fn request_digest(&self) -> Result<String, String> {
        self.validate()?;
        hash(&("oasis7.custody.native-read-and-register-request.v2", self))
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReadAndRegisterReplyV2 {
    pub api_version: u8,
    pub world_response: oasis7_client_api::world_service::SignedServiceResponse<
        oasis7_client_api::world_service::participant_discovery::OwnerSetupObservationV1,
    >,
    pub registration_ack: SignedNativeRegistrationAckV2,
}
impl core::fmt::Debug for NativeReadAndRegisterRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeReadAndRegisterRequestV2(<private>)")
    }
}
impl core::fmt::Debug for NativeReadAndRegisterReplyV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeReadAndRegisterReplyV2(<private>)")
    }
}

pub fn registration_response_digest(
    response: &oasis7_client_api::world_service::SignedServiceResponse<
        oasis7_client_api::world_service::participant_discovery::OwnerSetupObservationV1,
    >,
) -> Result<String, String> {
    hash(&("oasis7.custody.preflight-owner-read-response.v2", response))
}
impl NativeReadAndRegisterReplyV2 {
    pub fn validate_relative(
        &self,
        request: &NativeReadAndRegisterRequestV2,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<(), String> {
        request.validate_relative(pending)?;
        self.registration_ack.ack.validate_relative(pending)?;
        if self.api_version != 2
            || self.registration_ack.ack.registration_nonce != request.registration_nonce
            || self.registration_ack.ack.owner_read_response_digest
                != registration_response_digest(&self.world_response)?
            || self.registration_ack.ack.pending_certificate_digest != pending.digest()?
        {
            return Err(invalid());
        }
        self.world_response.payload.validate(&request.owner_read)
    }
}

impl NativeReadAndRegisterReplyV2 {
    /// The approved complete header supplies the World pin; never a response key.
    pub fn verify_world(
        &self,
        request: &NativeReadAndRegisterRequestV2,
        pending: &SignedAccountConsumedPendingV2,
    ) -> Result<(), String> {
        self.validate_relative(request, pending)?;
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, .. } =
            &pending.certificate.input.request.intent
        else {
            return Err(invalid());
        };
        let term = &plan.termination;
        let path =
            oasis7_client_api::world_service::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH;
        let digest = oasis7_client_api::world_service::request_digest(path, &request.owner_read)?;
        if self.world_response.request_digest != digest
            || self.world_response.payload.control.world.world_id != term.world.world_id
            || self.world_response.payload.control.world.genesis_digest != term.world.genesis_digest
            || self.world_response.payload.signer_delegation.as_ref()
                != Some(&term.original_delegation)
            || self
                .world_response
                .payload
                .control
                .view
                .game_task_control
                .as_ref()
                .map(oasis7_agent_api::game::GameTaskFenceV1::from_control)
                != Some(term.initial_fence.clone())
        {
            return Err(invalid());
        }
        let key =
            ed25519_dalek::VerifyingKey::from_bytes(&decode::<32>(&term.world.service_public_key)?)
                .map_err(|_| invalid())?;
        if key.is_weak() {
            return Err(invalid());
        }
        key.verify_strict(
            &oasis7_client_api::world_service::signing_bytes(
                path,
                &(
                    &self.world_response.request_digest,
                    &self.world_response.payload,
                ),
            )?,
            &ed25519_dalek::Signature::from_bytes(&decode::<64>(
                &self.world_response.signature_hex,
            )?),
        )
        .map_err(|_| invalid())
    }
}

/// The signature is already fixed and strict-verified by the receiver; this
/// derivation has no hash cycle and does not create a new authority source.
pub fn native_registration_nonce_v2(
    pending: &SignedAccountConsumedPendingV2,
) -> Result<String, String> {
    hash(&(
        "oasis7.custody.native-registration-nonce.v2",
        pending.digest()?,
    ))
}

/// Pure content binding for Native's guarded-consent provenance. This is not a
/// signature, Current statement, capability or substitute for the saved record.
pub fn native_guarded_consent_request_digest_v2(
    request: &crate::approval::ApprovalOptionsRequestV1,
    summary: Option<&crate::approval::NativeCustodySummaryContractV1>,
) -> Result<String, String> {
    hash(&(
        "oasis7.custody.native-guarded-consent-request.v2",
        request,
        summary,
    ))
}
