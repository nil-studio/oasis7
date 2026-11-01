//! The Pending producer reads the actual V2 Book and paid OwnerRead original.
//! Its fixed inner World port is not debited to Native160 or E/R/Cleanup.
use super::*;
use crate::native_cleanup_v2::{
    AccountConsumedPendingV2, AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2,
    AccountMachineIssuerV2, AccountMachinePurposeV2, NativeReadAndRegisterReplyV2,
    NativeRegistrationReadClaimV2, NativeRegistrationReadProfileV2, SignedAccountConsumedPendingV2,
    SignedNativeRegistrationAckV2, registration_read_digest, registration_response_digest,
};
fn required_signature<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> core::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMachineRegistrationRecordV2 {
    pub(in crate::provisioning) pending: Box<AccountConsumedPendingV2>,
    #[serde(deserialize_with = "required_signature")]
    pub(in crate::provisioning) signed: Option<SignedAccountConsumedPendingV2>,
    #[serde(deserialize_with = "required_signature")]
    pub(in crate::provisioning) registered: Option<NativeMachineRegisteredFactV2>,
    #[serde(deserialize_with = "required_signature")]
    pub(in crate::provisioning) active: Option<NativeMachineActiveRecordV2>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeMachineRegisteredFactV2 {
    pub(in crate::provisioning) ack: SignedNativeRegistrationAckV2,
    // Complete typed Node response, retained as canonical bytes to preserve Eq
    // and never converted through an untyped JSON value.
    pub(in crate::provisioning) world_response_cbor: Vec<u8>,
}
impl core::fmt::Debug for NativeMachineRegistrationRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeMachineRegistrationRecordV2(<private>)")
    }
}
/// Borrowed, fixed issuer admission inputs for this one purpose only.
/// Grouping does not validate them or confer signing authority.
pub struct NativeMachinePendingIssuerV2<'a> {
    pub issuer: AccountMachineIssuerV2,
    pub pin: &'a AccountMachineIssuerPinV2,
    pub floor: &'a AccountMachineIssuerFloorV2,
}
/// Only the Account-state factory can create this exact signing input. It has
/// no Clone/serde/debug and cannot be reconstructed from a browser request.
pub struct PreparedNativeMachinePendingV2 {
    session_id: String,
    pending: AccountConsumedPendingV2,
    pin: AccountMachineIssuerPinV2,
}
impl PreparedNativeMachinePendingV2 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.pending.issuer
    }
    pub fn issuer_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.pending.issued_at.0, self.pending.expires_at.0)
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        self.pending.signing_bytes()
    }
}
pub(in crate::provisioning) fn source_id(s: &TestSessionAuthorityV2) -> Result<String> {
    // Exact same nonce domain/tuple as the existing V2 descriptor. The saved
    // preflight_source_nonce is the verification nonce, not this map key.
    digest(&(
        "oasis7.custody.test-session-preflight-source.v2",
        &s.input.operation_id,
        s.input.request.intent_digest().map_err(|_| invalid())?,
        s.input.approval.digest().map_err(|_| invalid())?,
    ))
}
fn read_claim(s: &TestSessionAuthorityV2) -> Result<NativeRegistrationReadClaimV2> {
    let source_id = source_id(s)?;
    let source = s.sources.get(&source_id).ok_or_else(invalid)?;
    if Some(&source.source.verify_nonce) != s.preflight_source_nonce.as_ref() {
        return Err(invalid());
    }
    let read = s.preflight_read.as_deref().ok_or_else(invalid)?;
    let lifetime = s.preflight_lifetime.as_ref().ok_or_else(invalid)?;
    let claimed = *s
        .preflight_steps
        .get(&SessionPreflightStepV1::OwnerRead)
        .ok_or_else(invalid)?;
    let input_digest = s.input_digest()?;
    let preflight_digest = digest(&(
        "oasis7.custody.machine-preflight-owner-read.v2",
        &input_digest,
        source,
        read,
        lifetime,
        claimed,
    ))?;
    let attempt = |port: &str| {
        digest(&(
            "oasis7.custody.machine-owner-read-attempt.v2",
            &input_digest,
            &preflight_digest,
            port,
        ))
    };
    let native_attempt = attempt("identity-to-native")?;
    let world_attempt = attempt("native-to-world")?;
    Ok(NativeRegistrationReadClaimV2 {
        profile: NativeRegistrationReadProfileV2::ExactMove0OwnerView,
        source_operation_id: source_id,
        source_record_digest: digest(source)?,
        preflight_record_digest: preflight_digest,
        owner_read_original_digest: registration_read_digest(read).map_err(|_| invalid())?,
        native_transport_attempt_id: native_attempt,
        world_transport_attempt_id: world_attempt,
        native_http_units: DecimalU64(1),
        world_http_units: DecimalU64(1),
        claimed_at: DecimalU64(claimed.get()),
    })
}
impl NativeMachineRegistrationRecordV2 {
    pub(in crate::provisioning) fn transport_id_in_use(&self, attempt: &str) -> bool {
        self.pending
            .preflight_read_claim
            .native_transport_attempt_id
            == attempt
            || self.pending.preflight_read_claim.world_transport_attempt_id == attempt
            || self.active.as_ref().is_some_and(|a| {
                a.certificate
                    .activation_transport
                    .native_transport_attempt_id
                    == attempt
            })
    }
    pub(in crate::provisioning) fn validate_native_history(
        &self,
        s: &TestSessionAuthorityV2,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        let Some(reg) = &self.registered else {
            return Ok(());
        };
        let pending = self.signed.as_ref().ok_or_else(invalid)?;
        reg.ack
            .verify_native(
                pending,
                native.native_root_key_id,
                native.native_root_public_key,
                &s.session.scope.identity_namespace,
                s.session.scope.identity_instance_epoch.0,
                reg.ack.ack.issued_at.0,
            )
            .map_err(|_| invalid())?;
        if let Some(ready) = self.active.as_ref().and_then(|a| a.ready.as_ref()) {
            ready
                .ack
                .verify_native(
                    pending,
                    self.active
                        .as_ref()
                        .and_then(|a| a.signed.as_ref())
                        .ok_or_else(invalid)?,
                    &reg.ack,
                    native.native_root_key_id,
                    native.native_root_public_key,
                )
                .map_err(|_| invalid())?;
        }
        Ok(())
    }
    pub(in crate::provisioning) fn ready(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.ready())
    }
    pub(in crate::provisioning) fn validate_session(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        self.pending.validate_shape().map_err(|_| invalid())?;
        for attempt in [
            &self
                .pending
                .preflight_read_claim
                .native_transport_attempt_id,
            &self.pending.preflight_read_claim.world_transport_attempt_id,
        ] {
            if super::source_v2::transport_id_in_non_machine_state_v2(s, attempt) {
                return Err(invalid());
            }
        }
        if self.pending.input.as_ref() != &s.input
            || self.pending.approved_input_digest != s.input_digest()?
            || self.pending.participant_session != s.session
            // The next closed phase must persist/validate the real registration
            // before ordinary activation can publish anything. No partial
            // producer falls through to the old V2 activation API.
            || (s.approved_at.is_some() || !s.roots.is_empty()) && self.registered.is_none()
            || Some(self.pending.consumed_at.0) != s.consumed_at.map(UnixMillis::get)
            || self.pending.preflight_read_claim != read_claim(s)?
            || s.preflight_lifetime.as_ref() != Some(&self.pending.native_lifetime)
            || s.book.original(&s.input.operation_id).is_none_or(|v|
                v.stage() != ApprovalAdmissionStage::Consumed || v.input() != &s.input)
            || self.signed.as_ref().is_some_and(|v| v.certificate != *self.pending)
        {
            return Err(invalid());
        }
        if let Some(registered) = &self.registered {
            let signed = self.signed.as_ref().ok_or_else(invalid)?;
            registered
                .ack
                .ack
                .validate_relative(signed)
                .map_err(|_| invalid())?;
            let response: ws::SignedServiceResponse<
                ws::participant_discovery::OwnerSetupObservationV1,
            > = serde_cbor::from_slice(&registered.world_response_cbor).map_err(|_| invalid())?;
            if registration_response_digest(&response).map_err(|_| invalid())?
                != registered.ack.ack.owner_read_response_digest
                || oasis7_wasm_abi::encode_canonical_cbor(&response).map_err(|_| invalid())?
                    != registered.world_response_cbor
            {
                return Err(invalid());
            }
            let envelope = NativeReadAndRegisterReplyV2 {
                api_version: 2,
                world_response: response,
                registration_ack: registered.ack.clone(),
            };
            if serde_json::to_vec(&envelope).map_err(|_| invalid())?.len() > 65_536 {
                return Err(invalid());
            }
        }
        if let Some(active) = &self.active {
            active.validate(s, self)?;
        }
        // Awaiting phases cannot contain externally issued credentials. A snapshot cannot
        // smuggle already-issued capabilities into its externally inert Roots.
        if !self.ready()
            && (s
                .roots
                .values()
                .any(|r| r.credential.is_some() || !r.credential_history.is_empty())
                || !s.credential_claims.is_empty()
                || !s.actions.is_empty()
                || !s.tool_nonce_ingresses.is_empty()
                || !s.control_records.is_empty())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// The caller has already persisted the ordinary OwnerRead Native1 claim.
    /// This registers its exact Native1+fixed internal World1 once tuple; it
    /// does not send either request or create roots. Existing profile bounds
    /// must separately cover the other two preflight World operations.
    pub fn prepare_test_session_v2_machine_pending(
        &mut self,
        id: &str,
        issuer_scope: NativeMachinePendingIssuerV2<'_>,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedNativeMachinePendingV2> {
        let NativeMachinePendingIssuerV2 { issuer, pin, floor } = issuer_scope;
        verifier.verify(&self.test_session_v2_preflight_checks(id, current, now)?)?;
        let s = self.test_session_v2_record(id)?;
        let (plan, _) = s.header()?;
        if self.frozen()
            || s.closing.is_some()
            || s.approved_at.is_some()
            || !s.roots.is_empty()
            || s.native_machine_registration.is_some()
            || plan.termination.cleanup_transport_policy
                != CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2
            || pin.native_audience_origin != current.native_scope.http_origin
        {
            return Err(invalid());
        }
        s.input
            .request
            .validate_native_summary_contract(
                s.input
                    .approval
                    .proof
                    .model_budget_summary_contract
                    .as_ref(),
                &s.input.approval.proof.manager_account_id,
                Some(current.native_scope.native_root_public_key),
                s.input.approval.proof.issued_at.0,
            )
            .map_err(|_| invalid())?;
        let lifetime = s.preflight_lifetime.as_ref().ok_or_else(invalid)?.clone();
        lifetime
            .verify_root(
                current.native_scope.native_root_key_id,
                current.native_scope.native_root_public_key,
            )
            .map_err(|_| invalid())?;

        let consumed_at = s.consumed_at.ok_or_else(invalid)?;
        let source = &s.sources.get(&source_id(s)?).ok_or_else(invalid)?.source;
        let source_until = source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0
            .checked_add(5000)
            .ok_or_else(invalid)?;
        let current_until = current
            .observed_at
            .get()
            .checked_add(5000)
            .ok_or_else(invalid)?;
        let expires = plan
            .write_deadline
            .0
            .min(s.input.approval.proof.expires_at.0)
            .min(source_until)
            .min(current_until)
            .min(lifetime.lifetime.status.expires_at.0)
            .min(lifetime.lifetime.session_expires_at.0)
            .min(lifetime.lifetime.session_idle_expires_at.0);
        issuer
            .verify_pin(
                pin,
                floor,
                AccountMachinePurposeV2::ConsumedPending,
                now.get(),
                expires,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let pending = AccountConsumedPendingV2 {
            api_version: 2,
            issuer,
            certificate_id: digest(&(
                "oasis7.custody.machine-pending-certificate.v2",
                s.input_digest()?,
            ))?,
            input: Box::new(s.input.clone()),
            approved_input_digest: s.input_digest()?,
            intent_digest: s.input.request.intent_digest().map_err(|_| invalid())?,
            plan_digest: plan.digest().map_err(|_| invalid())?,
            participant_session: s.session.clone(),
            native_lifetime: lifetime,
            preflight_read_claim: read_claim(s)?,
            consumed_at: DecimalU64(consumed_at.get()),
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(expires),
        };
        pending.validate_shape().map_err(|_| invalid())?;
        self.test_session_v2_transport_id_available(
            id,
            &pending.preflight_read_claim.native_transport_attempt_id,
        )?;
        self.test_session_v2_transport_id_available(
            id,
            &pending.preflight_read_claim.world_transport_attempt_id,
        )?;
        let mut next = self.clone();
        next.test_sessions_v2
            .get_mut(id)
            .ok_or_else(invalid)?
            .native_machine_registration = Some(NativeMachineRegistrationRecordV2 {
            pending: Box::new(pending.clone()),
            signed: None,
            registered: None,
            active: None,
        });
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedNativeMachinePendingV2 {
            session_id: id.into(),
            pending,
            pin: pin.clone(),
        })
    }
    /// A repeated signature completion is not a new dispatch authorization.
    pub fn finish_test_session_v2_machine_pending(
        &mut self,
        prepared: PreparedNativeMachinePendingV2,
        signature_hex: String,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SignedAccountConsumedPendingV2> {
        verifier.verify(&self.test_session_v2_preflight_checks(
            &prepared.session_id,
            current,
            now,
        )?)?;
        if self.frozen() {
            return Err(invalid());
        }
        let s = self.test_session_v2_record(&prepared.session_id)?;
        let record = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        if s.closing.is_some()
            || s.approved_at.is_some()
            || !s.roots.is_empty()
            || record.signed.is_some()
            || record.pending.as_ref() != &prepared.pending
            || prepared.pin.native_audience_origin != current.native_scope.http_origin
            || now.get() < prepared.pending.issued_at.0
            || now.get() >= prepared.pending.expires_at.0
        {
            return Err(invalid());
        }
        prepared
            .pending
            .native_lifetime
            .verify_root(
                current.native_scope.native_root_key_id,
                current.native_scope.native_root_public_key,
            )
            .map_err(|_| invalid())?;
        let signed = SignedAccountConsumedPendingV2 {
            certificate: prepared.pending,
            signature_hex,
        };
        let issuer_floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: prepared.pin.issuer_epoch,
            config_generation: prepared.pin.config_generation,
        };
        signed
            .verify_issuer(&prepared.pin, &issuer_floor, now.get())
            .map_err(|_| invalid())?;
        let mut next = self.clone();
        next.test_sessions_v2
            .get_mut(&prepared.session_id)
            .ok_or_else(invalid)?
            .native_machine_registration
            .as_mut()
            .ok_or_else(invalid)?
            .signed = Some(signed.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(signed)
    }
}
