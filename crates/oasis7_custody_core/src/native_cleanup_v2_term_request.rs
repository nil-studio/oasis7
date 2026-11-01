//! Explicit Term-wide Native observation; never a wrapper around slot-bound authority.
use super::*;
pub const NATIVE_TERM_CURRENT_V2_PATH: &str =
    "/api/public/hosted-account/v2/custody/cleanup/term-current-v2";
pub const NATIVE_TERM_OBSERVATION_V2_DOMAIN: &str = "oasis7.custody.native-term-observation.v2";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermCurrentRequestV2 {
    pub api_version: u8,
    pub session_id: String,
    pub owned_invocation_id: String,
    pub source_operation_id: String,
    pub stop_operation_id: String,
    pub close_operation_id: String,
    pub nonce: String,
    pub lifetime_nonce: String,
    pub origin_nonce: String,
}
impl NativeTermCurrentRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != 2
            || ![
                &self.session_id,
                &self.owned_invocation_id,
                &self.source_operation_id,
                &self.stop_operation_id,
                &self.close_operation_id,
            ]
            .iter()
            .all(|v| id(v))
            || self.stop_operation_id == self.close_operation_id
            || ![&self.nonce, &self.lifetime_nonce, &self.origin_nonce]
                .iter()
                .all(|v| hex64(v))
            || self.nonce == self.lifetime_nonce
            || self.nonce == self.origin_nonce
            || self.lifetime_nonce == self.origin_nonce
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn body_digest(&self) -> Result<String, String> {
        self.validate()?;
        hash(&("oasis7.custody.native-term-body.v2", self))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermCurrentPaidEdgeV2 {
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
    pub graph_reservation_digest: String,
    pub paid_record_digest: String,
    pub native_transport_attempt_id: String,
    pub native_paid_ordinal: DecimalU64,
    pub native_http_units: DecimalU64,
    /// Account start precedes every Directory/RF/Identity/Native observation.
    pub observation_started_at: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub cleanup_until: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeTermCurrentPaidEdgeV2 {
    pub edge: NativeTermCurrentPaidEdgeV2,
    pub signature_hex: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTermCurrentEnvelopeV2 {
    pub body: NativeTermCurrentRequestV2,
    pub paid: SignedNativeTermCurrentPaidEdgeV2,
}
impl NativeTermCurrentPaidEdgeV2 {
    pub fn validate_relative(
        &self,
        body: &NativeTermCurrentRequestV2,
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
        if self.api_version != 2
            || term.cleanup_transport_policy
                != crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2
            || self.session_id != body.session_id
            || self.session_id != scope.session_id
            || body.stop_operation_id != term.stop_operation_id
            || body.close_operation_id != term.close_operation_id
            || self.approved_input_digest != pending.certificate.approved_input_digest
            || self.termination_plan_digest != term.digest()?
            || self.active_certificate_digest != active.digest()?
            || self.opaque_registration_ref != ack.ack.opaque_registration_ref
            || self.native_key_id != ack.ack.native_key_id
            || self.identity_namespace != ack.ack.identity_namespace
            || self.identity_epoch != ack.ack.identity_epoch
            || self.native_body_digest != body.body_digest()?
            || !hex64(&self.graph_reservation_digest)
            || !hex64(&self.paid_record_digest)
            || !id(&self.native_transport_attempt_id)
            || !(1..=24).contains(&self.native_paid_ordinal.0)
            || self.native_http_units.0 != 1
            || self.observation_started_at.0 < active.certificate.issued_at.0
            || self.issued_at.0 < self.observation_started_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0
                > self
                    .observation_started_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
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
            NATIVE_TERM_OBSERVATION_V2_DOMAIN,
            &("POST", NATIVE_TERM_CURRENT_V2_PATH, self),
        )
    }
}
impl NativeTermCurrentEnvelopeV2 {
    pub fn request_digest(&self) -> Result<String, String> {
        self.body.validate()?;
        hash(&("oasis7.custody.native-term-envelope.v2", self))
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
            AccountMachinePurposeV2::NativeTermCurrentAttempt,
            edge.issued_at.0,
            edge.expires_at.0,
            now,
        )?;
        verify(
            NATIVE_TERM_OBSERVATION_V2_DOMAIN,
            &("POST", NATIVE_TERM_CURRENT_V2_PATH, edge),
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
    NativeTermCurrentRequestV2,
    NativeTermCurrentPaidEdgeV2,
    SignedNativeTermCurrentPaidEdgeV2,
    NativeTermCurrentEnvelopeV2
);

#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> NativeTermCurrentRequestV2 {
        NativeTermCurrentRequestV2 {
            api_version: 2,
            session_id: "session".into(),
            owned_invocation_id: "owned".into(),
            source_operation_id: "source".into(),
            stop_operation_id: "stop".into(),
            close_operation_id: "close".into(),
            nonce: "a".repeat(64),
            lifetime_nonce: "b".repeat(64),
            origin_nonce: "c".repeat(64),
        }
    }
    #[test]
    fn term_current_closed_body_has_no_implicit_slot_projection() {
        let value = body();
        value.validate().unwrap();
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(
            serde_json::from_value::<NativeTermCurrentRequestV2>(json.clone()).unwrap(),
            value
        );
        for key in [
            "owned_invocation_id",
            "source_operation_id",
            "stop_operation_id",
            "close_operation_id",
        ] {
            let mut missing = json.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<NativeTermCurrentRequestV2>(missing).is_err());
        }
        let mut extra = json;
        extra.as_object_mut().unwrap().insert(
            "operation".into(),
            serde_json::json!("observe_guarded_stop"),
        );
        assert!(serde_json::from_value::<NativeTermCurrentRequestV2>(extra).is_err());
    }
    #[test]
    fn term_current_tuple_rejects_aliasing_and_hashes_both_slots_and_invocation() {
        let value = body();
        let original = value.body_digest().unwrap();
        let mut changed = value.clone();
        changed.close_operation_id = changed.stop_operation_id.clone();
        assert!(changed.validate().is_err());
        changed = value.clone();
        changed.origin_nonce = changed.nonce.clone();
        assert!(changed.validate().is_err());
        changed = value.clone();
        changed.owned_invocation_id = "owned-two".into();
        assert_ne!(changed.body_digest().unwrap(), original);
        changed = value;
        changed.close_operation_id = "close-two".into();
        assert_ne!(changed.body_digest().unwrap(), original);
    }
}
