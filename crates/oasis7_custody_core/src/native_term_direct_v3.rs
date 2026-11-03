//! V3 exact Actor+Native paid frame. A valid frame is not Current or a World grant.
use crate::native_binding::SignedManagedNativeSessionLifetimeV1;
use crate::provisioning::ParticipantSessionRefV1;
use crate::{DecimalU64, native_cleanup_v2::*};
use serde::{Deserialize, Serialize};
const ACTOR_DOMAIN: &str = "oasis7.custody.term-native-sessions-current.v3";
const NATIVE_DOMAIN: &str = "oasis7.custody.native-term-current-attempt.v3";
pub const TERM_NATIVE_SESSIONS_PATH_V3: &str = "/internal/term-current-direct-v3";
pub const NATIVE_TERM_DIRECT_PATH_V3: &str =
    "/api/public/hosted-account/v2/custody/cleanup/term-current-v3";
pub const TERM_DIRECT_MAX_BYTES_V3: usize = 32_768;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermDirectBodyV3 {
    pub api_version: u8,
    pub session_id: String,
    pub owned_invocation_id: String,
    pub source_operation_id: String,
    pub stop_operation_id: String,
    pub close_operation_id: String,
    pub nonce: String,
    pub lifetime_nonce: String,
    pub origin_nonce: String,
    /// Original signed metadata provenance, not a fresh Native observation.
    pub metadata_session: ParticipantSessionRefV1,
    pub metadata_lifetime: SignedManagedNativeSessionLifetimeV1,
}
impl NativeTermDirectBodyV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != 3
            || ![
                &self.session_id,
                &self.owned_invocation_id,
                &self.source_operation_id,
                &self.stop_operation_id,
                &self.close_operation_id,
            ]
            .iter()
            .all(|s| id(s))
            || self.stop_operation_id == self.close_operation_id
            || ![&self.nonce, &self.lifetime_nonce, &self.origin_nonce]
                .iter()
                .all(|s| hex64(s))
            || self.nonce == self.origin_nonce
            || self.nonce == self.lifetime_nonce
            || self.origin_nonce == self.lifetime_nonce
        {
            return Err(invalid());
        }
        self.metadata_session
            .scope
            .validate()
            .map_err(|_| invalid())?;
        self.metadata_lifetime.validate().map_err(|_| invalid())?;
        let status = &self.metadata_lifetime.lifetime.status;
        if status.session_ref != self.metadata_session.session_ref
            || status.generation != self.metadata_session.native_binding_generation
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        hash(&("oasis7.custody.term-direct-body.v3", self))
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermDirectPaidBindingV3 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub session_id: String,
    pub approved_input_digest: String,
    pub termination_plan_digest: String,
    pub active_certificate_digest: String,
    pub opaque_registration_ref: String,
    pub native_key_id: String,
    pub identity_namespace: String,
    pub identity_epoch: DecimalU64,
    pub observation_round: crate::provisioning::TermObservationRoundV3,
    pub round_id: String,
    pub actor_object_id: String,
    pub actor_attempt_id: String,
    pub native_attempt_id: String,
    pub actor_paid_ordinal: DecimalU64,
    pub native_paid_ordinal: DecimalU64,
    pub graph_digest: String,
    pub actor_paid_digest: String,
    pub native_paid_digest: String,
    pub body_digest: String,
    pub observation_started_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub cleanup_until: DecimalU64,
}
impl TermDirectPaidBindingV3 {
    pub fn validate_shape(&self, body: &NativeTermDirectBodyV3) -> Result<(), String> {
        body.validate()?;
        if self.api_version != 3
            || self.session_id != body.session_id
            || ![
                &self.session_id,
                &self.opaque_registration_ref,
                &self.native_key_id,
                &self.identity_namespace,
                &self.round_id,
                &self.actor_attempt_id,
                &self.native_attempt_id,
            ]
            .iter()
            .all(|s| id(s))
            || ![
                &self.approved_input_digest,
                &self.termination_plan_digest,
                &self.active_certificate_digest,
                &self.actor_object_id,
                &self.graph_digest,
                &self.actor_paid_digest,
                &self.native_paid_digest,
                &self.body_digest,
            ]
            .iter()
            .all(|s| hex64(s))
            || self.actor_attempt_id == self.native_attempt_id
            || self.identity_epoch.0 == 0
            || !(1..=23).contains(&self.actor_paid_ordinal.0)
            || self.actor_paid_ordinal.0.checked_add(1) != Some(self.native_paid_ordinal.0)
            || self.body_digest != body.digest()?
            || self.observation_started_at.0 > self.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.cleanup_until.0
            || self.expires_at.0
                > self
                    .observation_started_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_relative(
        &self,
        body: &NativeTermDirectBodyV3,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        ack: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        self.validate_shape(body)?;
        active.validate_registration(pending, ack)?;
        let c = &pending.certificate;
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, scope } =
            &c.input.request.intent
        else {
            return Err(invalid());
        };
        if plan.termination.cleanup_transport_policy
                != crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedDirectCurrent5V3
            || self.approved_input_digest != c.approved_input_digest
            || self.termination_plan_digest != plan.termination.digest()?
            || self.active_certificate_digest != active.digest()?
            || self.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.native_key_id != ack.ack.native_key_id
            || self.identity_namespace != ack.ack.identity_namespace || self.identity_epoch != ack.ack.identity_epoch
            || self.session_id != scope.session_id
            || body.stop_operation_id != plan.termination.stop_operation_id
            || body.close_operation_id != plan.termination.close_operation_id
            || body.metadata_session != c.participant_session
            || body.metadata_lifetime != c.native_lifetime
            || self.cleanup_until.0 != plan.termination.cleanup_until.get()
            || self.observation_started_at.0 < active.certificate.issued_at.0
            || self.expires_at.0 > active.certificate.expires_at.0 {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn actor_signing_bytes(&self) -> Result<Vec<u8>, String> {
        bytes(ACTOR_DOMAIN, &("POST", TERM_NATIVE_SESSIONS_PATH_V3, self))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTermDirectActorV3 {
    pub binding: TermDirectPaidBindingV3,
    pub signature_hex: String,
}
impl SignedTermDirectActorV3 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&(ACTOR_DOMAIN, self))
    }
    pub fn verify(
        &self,
        body: &NativeTermDirectBodyV3,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        object: &str,
        now: u64,
    ) -> Result<(), String> {
        if object != self.binding.actor_object_id {
            return Err(invalid());
        }
        self.verify_original(body, pin, floor, now)
    }
    fn verify_original(
        &self,
        body: &NativeTermDirectBodyV3,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.binding.validate_shape(body)?;
        self.binding.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::TermNativeSessionsCurrentV3,
            self.binding.issued_at.0,
            self.binding.expires_at.0,
            now,
        )?;
        verify(
            ACTOR_DOMAIN,
            &("POST", TERM_NATIVE_SESSIONS_PATH_V3, &self.binding),
            &self.signature_hex,
            &pin.public_key,
        )
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermDirectNativeClaimV3 {
    pub binding: TermDirectPaidBindingV3,
    /// Includes the original complete Actor signature, not only its payload.
    pub actor_proof_digest: String,
}
impl TermDirectNativeClaimV3 {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        if !hex64(&self.actor_proof_digest) {
            return Err(invalid());
        }
        bytes(NATIVE_DOMAIN, &("POST", NATIVE_TERM_DIRECT_PATH_V3, self))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTermDirectNativeV3 {
    pub claim: TermDirectNativeClaimV3,
    pub signature_hex: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermDirectFrameV3 {
    pub body: NativeTermDirectBodyV3,
    pub actor: SignedTermDirectActorV3,
    pub native: SignedTermDirectNativeV3,
}
impl NativeTermDirectFrameV3 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&("oasis7.custody.term-direct-frame.v3", self))
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.body.validate()?;
        let raw = serde_json::to_vec(self).map_err(|_| invalid())?;
        if raw.len() > TERM_DIRECT_MAX_BYTES_V3 {
            return Err(invalid());
        }
        Ok(raw)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        if raw.is_empty() || raw.len() > TERM_DIRECT_MAX_BYTES_V3 {
            return Err(invalid());
        }
        // Typed first: known duplicates must fail before Value can erase them.
        let value: Self = serde_json::from_slice(raw).map_err(|_| invalid())?;
        let tree: serde_json::Value = serde_json::from_slice(raw).map_err(|_| invalid())?;
        if serde_json::to_value(&value).map_err(|_| invalid())? != tree {
            return Err(invalid());
        }
        value.wire_bytes()?;
        Ok(value)
    }
    pub fn verify_issuer(
        &self,
        actor_pin: &AccountMachineIssuerPinV2,
        native_pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        actual_object: &str,
        now: u64,
    ) -> Result<(), String> {
        if self.actor.binding.actor_object_id != actual_object {
            return Err(invalid());
        }
        self.verify_at_native_recipient(actor_pin, native_pin, floor, now)
    }
    /// Native authenticates its own paid attempt and both original signatures.
    /// This does not attest that the CF Actor ran or validated its actual ID.
    pub fn verify_at_native_recipient(
        &self,
        actor_pin: &AccountMachineIssuerPinV2,
        native_pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.wire_bytes()?;
        self.actor
            .verify_original(&self.body, actor_pin, floor, now)?;
        let c = &self.native.claim;
        if c.binding != self.actor.binding || c.actor_proof_digest != self.actor.digest()? {
            return Err(invalid());
        }
        c.binding.issuer.verify_pin(
            native_pin,
            floor,
            AccountMachinePurposeV2::NativeTermCurrentAttemptV3,
            c.binding.issued_at.0,
            c.binding.expires_at.0,
            now,
        )?;
        verify(
            NATIVE_DOMAIN,
            &("POST", NATIVE_TERM_DIRECT_PATH_V3, c),
            &self.native.signature_hex,
            &native_pin.public_key,
        )
    }
}
