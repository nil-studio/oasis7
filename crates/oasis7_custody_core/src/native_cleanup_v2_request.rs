//! First-stage paid Native observation. It cannot seal or send a World edge.
use super::*;
pub const NATIVE_CURRENT_PAID_EDGE_V2_DOMAIN: &str = "oasis7.custody.native-current-paid-edge.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCurrentRequestV2 {
    pub api_version: u8,
    pub session_id: String,
    pub source_operation_id: String,
    pub operation: NativeMachineOperationV2,
    pub operation_id: String,
    #[serde(deserialize_with = "required_optional_digest")]
    pub existing_original_digest: Option<String>,
    pub nonce: String,
    pub lifetime_nonce: String,
    pub origin_nonce: String,
}
fn required_optional_digest<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    Option::deserialize(d)
}
impl NativeCurrentRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != 2
            || ![
                &self.session_id,
                &self.source_operation_id,
                &self.operation_id,
            ]
            .iter()
            .all(|v| id(v))
            || ![&self.nonce, &self.lifetime_nonce, &self.origin_nonce]
                .iter()
                .all(|v| hex64(v))
            || self
                .existing_original_digest
                .as_ref()
                .is_some_and(|v| !hex64(v))
            || matches!(
                self.operation,
                NativeMachineOperationV2::ObserveStopKnownLookup
                    | NativeMachineOperationV2::ObserveCloseKnownLookup
            ) && self.existing_original_digest.is_none()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn body_digest(&self) -> Result<String, String> {
        self.validate()?;
        hash(&("oasis7.custody.native-current-body.v2", self))
    }
    pub fn observation_intent_digest(
        &self,
        approved_input: &str,
        term: &str,
    ) -> Result<String, String> {
        self.validate()?;
        if !hex64(approved_input) || !hex64(term) {
            return Err(invalid());
        }
        hash(&(
            "oasis7.custody.native-cleanup-observation-intent.v2",
            approved_input,
            term,
            self,
        ))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCurrentPaidEdgeV2 {
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
    pub native_body_digest: String,
    pub observation_intent_digest: String,
    pub graph_reservation_digest: String,
    pub paid_record_digest: String,
    pub native_transport_attempt_id: String,
    pub native_paid_ordinal: DecimalU64,
    pub native_http_units: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub cleanup_until: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeCurrentPaidEdgeV2 {
    pub edge: NativeCurrentPaidEdgeV2,
    pub signature_hex: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCurrentEnvelopeV2 {
    pub body: NativeCurrentRequestV2,
    pub paid: SignedNativeCurrentPaidEdgeV2,
}
impl NativeCurrentPaidEdgeV2 {
    pub fn validate_relative(
        &self,
        body: &NativeCurrentRequestV2,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        ack: &SignedNativeRegistrationAckV2,
    ) -> Result<(), String> {
        body.validate()?;
        active.validate_registration(pending, ack)?;
        let crate::approval::CustodyIntentV1::TestSessionV2 { plan, scope } =
            &pending.certificate.input.request.intent
        else {
            return Err(invalid());
        };
        let term = &plan.termination;
        let expected_operation = match body.operation {
            NativeMachineOperationV2::ObserveGuardedStop
            | NativeMachineOperationV2::ObserveStopKnownLookup => &term.stop_operation_id,
            NativeMachineOperationV2::ObserveConditionalClose
            | NativeMachineOperationV2::ObserveCloseKnownLookup => &term.close_operation_id,
        };
        if self.api_version != 2
            || self.session_id != body.session_id
            || self.session_id != scope.session_id
            || &body.operation_id != expected_operation
            || self.approved_input_digest != pending.certificate.approved_input_digest
            || self.termination_plan_digest != term.digest()?
            || self.active_certificate_digest != active.digest()?
            || self.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.native_key_id != ack.ack.native_key_id
            || self.identity_namespace != ack.ack.identity_namespace
            || self.identity_epoch != ack.ack.identity_epoch
            || self.native_body_digest != body.body_digest()?
            || self.observation_intent_digest
                != body.observation_intent_digest(
                    &self.approved_input_digest,
                    &self.termination_plan_digest,
                )?
            || !hex64(&self.graph_reservation_digest)
            || !hex64(&self.paid_record_digest)
            || !id(&self.native_transport_attempt_id)
            || !(1..=24).contains(&self.native_paid_ordinal.0)
            || self.native_http_units.0 != 1
            || self.issued_at.0 < active.certificate.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.issued_at.0.checked_add(5000).ok_or_else(invalid)?
            || self.expires_at.0 > active.certificate.expires_at.0
            || self.cleanup_until.0 != term.cleanup_until.get()
            || self.expires_at.0 > self.cleanup_until.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        bytes(
            NATIVE_CURRENT_PAID_EDGE_V2_DOMAIN,
            &("POST", NATIVE_CURRENT_V2_PATH, self),
        )
    }
}
impl NativeCurrentEnvelopeV2 {
    pub fn request_digest(&self) -> Result<String, String> {
        self.body.validate()?;
        hash(&("oasis7.custody.native-current-envelope.v2", self))
    }
    pub fn verify_issuer(
        &self,
        pending: &SignedAccountConsumedPendingV2,
        active: &SignedAccountActiveThreeRootsV2,
        ack: &SignedNativeRegistrationAckV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        let edge = &self.paid.edge;
        edge.validate_relative(&self.body, pending, active, ack)?;
        edge.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::NativeCurrentAttempt,
            edge.issued_at.0,
            edge.expires_at.0,
            now,
        )?;
        verify(
            NATIVE_CURRENT_PAID_EDGE_V2_DOMAIN,
            &("POST", NATIVE_CURRENT_V2_PATH, edge),
            &self.paid.signature_hex,
            &pin.public_key,
        )
    }
}
macro_rules! redacted { ($($ty:ty),*) => {$ (
    impl core::fmt::Debug for $ty { fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(concat!(stringify!($ty), "(<private>)"))
    }}
)*}; }
redacted!(
    NativeCurrentRequestV2,
    NativeCurrentPaidEdgeV2,
    SignedNativeCurrentPaidEdgeV2,
    NativeCurrentEnvelopeV2
);
