//! Account-only Native activation. Internal Roots stay inert until exact Ack.
use super::*;
use crate::native_cleanup_v2::{
    ACTIVE_THREE_ROOTS_V2_DOMAIN, AccountActiveThreeRootsV2, AccountMachineIssuerFloorV2,
    AccountMachineIssuerPinV2, AccountMachineIssuerV2, AccountMachinePurposeV2,
    NativeActivateRequestV2, NativeActivationPaidClaimV2, NativeFloorEvidenceV2,
    SignedAccountActiveThreeRootsV2, SignedAccountConsumedPendingV2, SignedNativeActivationAckV2,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeMachineActiveRecordV2 {
    pub(super) certificate: AccountActiveThreeRootsV2,
    pub(super) pin: AccountMachineIssuerPinV2,
    pub(super) dispatch_until: UnixMillis,
    #[serde(deserialize_with = "required_nullable")]
    pub(super) signed: Option<SignedAccountActiveThreeRootsV2>,
    #[serde(deserialize_with = "required_nullable")]
    pub(super) ready: Option<NativeMachineReadyFactV2>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeMachineReadyFactV2 {
    pub(super) ack: SignedNativeActivationAckV2,
    pub(super) admitted_at: UnixMillis,
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> core::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
pub struct PreparedNativeMachineActiveV2 {
    pending: SignedAccountConsumedPendingV2,
    session_id: String,
    certificate: AccountActiveThreeRootsV2,
    pin: AccountMachineIssuerPinV2,
}
impl PreparedNativeMachineActiveV2 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.certificate.issuer
    }
    pub fn issuer_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.certificate.issued_at.0, self.certificate.expires_at.0)
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        self.certificate.validate_relative(&self.pending)?;
        crate::native_cleanup_v2::bytes(ACTIVE_THREE_ROOTS_V2_DOMAIN, &self.certificate)
    }
}
/// Only returned by the once transition; no serde, Clone or Debug dispatch token.
pub struct NativeMachineActivationDispatchV2 {
    request: NativeActivateRequestV2,
    until: UnixMillis,
}
impl NativeMachineActivationDispatchV2 {
    pub fn request(&self) -> &NativeActivateRequestV2 {
        &self.request
    }
    pub fn dispatch_until(&self) -> UnixMillis {
        self.until
    }
}
impl NativeMachineActiveRecordV2 {
    pub(super) fn ready(&self) -> bool {
        self.ready.is_some()
    }
    pub(super) fn validate(
        &self,
        s: &TestSessionAuthorityV2,
        m: &NativeMachineRegistrationRecordV2,
    ) -> Result<()> {
        let pending = m.signed.as_ref().ok_or_else(invalid)?;
        let registration = &m.registered.as_ref().ok_or_else(invalid)?.ack;
        self.certificate
            .validate_relative(pending)
            .map_err(|_| invalid())?;
        let attempt = digest(&(
            "oasis7.custody.native-active-transport.v2",
            &s.input.operation_id,
            s.input_digest()?,
            &self.certificate.root_birth_digest,
        ))?;
        let claim = &self.certificate.activation_transport;
        if super::source_v2::transport_id_in_non_machine_state_v2(s, &attempt)
            || attempt == m.pending.preflight_read_claim.native_transport_attempt_id
            || attempt == m.pending.preflight_read_claim.world_transport_attempt_id
        {
            return Err(invalid());
        }
        let at = UnixMillis::new(self.certificate.issued_at.0);
        if claim.native_transport_attempt_id != attempt
            || claim.native_paid_record_digest
                != digest(&(
                    "oasis7.custody.native-active-paid.v2",
                    &s.input.operation_id,
                    s.input_digest()?,
                    &self.certificate.root_birth_digest,
                    &attempt,
                    at,
                    self.dispatch_until,
                ))?
            || claim.activation_nonce
                != digest(&(
                    "oasis7.custody.native-active-nonce.v2",
                    &s.input.operation_id,
                    s.input_digest()?,
                    &attempt,
                ))?
            || self.certificate.certificate_id
                != digest(&(
                    "oasis7.custody.native-active-certificate.v2",
                    &s.input.operation_id,
                    &self.certificate.root_birth_digest,
                    claim,
                ))?
        {
            return Err(invalid());
        }
        if self.certificate.root_birth_digest
            != super::machine_birth_v2::actual_root_birth_digest_v2(s)?
            || self.certificate.native_registration_ack_digest
                != registration.digest().map_err(|_| invalid())?
            || self.certificate.opaque_registration_ref != registration.ack.opaque_registration_ref
            || self.dispatch_until.get() > pending.certificate.expires_at.0
            || self.dispatch_until.get() <= self.certificate.issued_at.0
        {
            return Err(invalid());
        }
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.pin.issuer_epoch,
            config_generation: self.pin.config_generation,
        };
        self.certificate
            .issuer
            .verify_pin(
                &self.pin,
                &floor,
                AccountMachinePurposeV2::ActiveThreeRoots,
                self.certificate.issued_at.0,
                self.certificate.expires_at.0,
                self.certificate.issued_at.0,
            )
            .map_err(|_| invalid())?;
        if let Some(signed) = &self.signed {
            if signed.certificate != self.certificate {
                return Err(invalid());
            }
            signed
                .verify_issuer(pending, &self.pin, &floor, self.certificate.issued_at.0)
                .map_err(|_| invalid())?;
        }
        if let Some(ready) = &self.ready {
            let signed = self.signed.as_ref().ok_or_else(invalid)?;
            ready
                .ack
                .ack
                .validate_relative(pending, signed, registration)
                .map_err(|_| invalid())?;
            if ready.admitted_at.get() < ready.ack.ack.activated_at.0
                || ready.admitted_at.get() >= self.dispatch_until.get()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_machine_active(
        &mut self,
        id: &str,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedNativeMachineActiveV2> {
        for checks in self.activate_test_session_v2_checks(id, current, now)? {
            verifier.verify(&checks)?;
        }
        let s = self.test_session_v2_record(id)?;
        let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        let pending = m.signed.as_ref().ok_or_else(invalid)?;
        let registration = &m.registered.as_ref().ok_or_else(invalid)?.ack;
        if self.frozen() || s.approved_at.is_none() || s.closing.is_some() || m.active.is_some() {
            return Err(invalid());
        }
        let dispatch_until = UnixMillis::new(
            pending.certificate.expires_at.0.min(
                current
                    .approval
                    .observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
            ),
        );
        if now.get() >= dispatch_until.get() {
            return Err(invalid());
        }
        let root_birth = super::machine_birth_v2::actual_root_birth_digest_v2(s)?;
        let attempt = digest(&(
            "oasis7.custody.native-active-transport.v2",
            id,
            s.input_digest()?,
            &root_birth,
        ))?;
        self.test_session_v2_transport_id_available(id, &attempt)?;
        let claim = NativeActivationPaidClaimV2 {
            native_transport_attempt_id: attempt.clone(),
            native_paid_record_digest: digest(&(
                "oasis7.custody.native-active-paid.v2",
                id,
                s.input_digest()?,
                &root_birth,
                &attempt,
                now,
                dispatch_until,
            ))?,
            activation_nonce: digest(&(
                "oasis7.custody.native-active-nonce.v2",
                id,
                s.input_digest()?,
                &attempt,
            ))?,
            native_http_units: 1,
            claimed_at: DecimalU64(now.get()),
        };
        let certificate = AccountActiveThreeRootsV2 {
            api_version: 2,
            issuer: pending.certificate.issuer.clone(),
            certificate_id: digest(&(
                "oasis7.custody.native-active-certificate.v2",
                id,
                &root_birth,
                &claim,
            ))?,
            session_id: id.into(),
            approved_input_digest: s.input_digest()?,
            pending_certificate_digest: pending.digest().map_err(|_| invalid())?,
            native_registration_ack_digest: registration.digest().map_err(|_| invalid())?,
            opaque_registration_ref: registration.ack.opaque_registration_ref.clone(),
            root_birth_digest: root_birth,
            activation_transport: claim,
            issued_at: DecimalU64(now.get()),
            expires_at: s.header()?.0.cleanup_until,
        };
        certificate
            .validate_relative(pending)
            .map_err(|_| invalid())?;
        certificate
            .issuer
            .verify_pin(
                pin,
                floor,
                AccountMachinePurposeV2::ActiveThreeRoots,
                certificate.issued_at.0,
                certificate.expires_at.0,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let pending = pending.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.identity_transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(1)
            .map_err(|_| invalid())?;
        s.native_machine_registration
            .as_mut()
            .ok_or_else(invalid)?
            .active = Some(NativeMachineActiveRecordV2 {
            certificate: certificate.clone(),
            pin: pin.clone(),
            dispatch_until,
            signed: None,
            ready: None,
        });
        append_session_event_v2(s, SessionEventKindV2::NativeMachineActivationClaimed, now)?;
        super::v2_credential_http::close_if_control_exhausted(s, now);
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedNativeMachineActiveV2 {
            pending,
            session_id: id.into(),
            certificate,
            pin: pin.clone(),
        })
    }
    pub fn finish_test_session_v2_machine_active(
        &mut self,
        prepared: PreparedNativeMachineActiveV2,
        signature_hex: String,
        now: UnixMillis,
    ) -> Result<NativeMachineActivationDispatchV2> {
        let s = self.test_session_v2_record(&prepared.session_id)?;
        let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        let active = m.active.as_ref().ok_or_else(invalid)?;
        if self.frozen()
            || s.closing.is_some()
            || active.certificate != prepared.certificate
            || active.pin != prepared.pin
            || active.signed.is_some()
            || now.get() >= active.dispatch_until.get()
        {
            return Err(invalid());
        }
        let signed = SignedAccountActiveThreeRootsV2 {
            certificate: prepared.certificate,
            signature_hex,
        };
        signed
            .verify_issuer(
                m.signed.as_ref().ok_or_else(invalid)?,
                &prepared.pin,
                &AccountMachineIssuerFloorV2 {
                    issuer_epoch: prepared.pin.issuer_epoch,
                    config_generation: prepared.pin.config_generation,
                },
                now.get(),
            )
            .map_err(|_| invalid())?;
        let request = NativeActivateRequestV2 {
            api_version: 2,
            opaque_registration_ref: signed.certificate.opaque_registration_ref.clone(),
            active: signed.clone(),
        };
        let until = active.dispatch_until;
        let mut next = self.clone();
        next.test_session_v2_record_mut(&prepared.session_id)?
            .native_machine_registration
            .as_mut()
            .ok_or_else(invalid)?
            .active
            .as_mut()
            .ok_or_else(invalid)?
            .signed = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(NativeMachineActivationDispatchV2 { request, until })
    }
    pub fn admit_test_session_v2_native_activation(
        &mut self,
        id: &str,
        ack: SignedNativeActivationAckV2,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        for checks in self.activate_test_session_v2_checks(id, current, now)? {
            verifier.verify(&checks)?;
        }
        let s = self.test_session_v2_record(id)?;
        let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        let a = m.active.as_ref().ok_or_else(invalid)?;
        if self.frozen()
            || s.closing.is_some()
            || a.ready.is_some()
            || now.get() >= a.dispatch_until.get()
        {
            return Err(invalid());
        }
        ack.verify_native(
            m.signed.as_ref().ok_or_else(invalid)?,
            a.signed.as_ref().ok_or_else(invalid)?,
            &m.registered.as_ref().ok_or_else(invalid)?.ack,
            current.approval.native_scope.native_root_key_id,
            current.approval.native_scope.native_root_public_key,
        )
        .map_err(|_| invalid())?;
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .native_machine_registration
            .as_mut()
            .ok_or_else(invalid)?
            .active
            .as_mut()
            .ok_or_else(invalid)?
            .ready = Some(NativeMachineReadyFactV2 {
            ack: ack.clone(),
            admitted_at: now,
        });
        append_session_event_v2(
            next.test_session_v2_record_mut(id)?,
            SessionEventKindV2::NativeMachineActivationAdmitted,
            now,
        )?;
        next.record_test_session_v2_native_floor(
            id,
            NativeFloorEvidenceV2::Activation(Box::new(ack)),
            current.approval.native_scope,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    /// Stored Ready identity only. This neither proves current Native/Source
    /// state nor grants Execution or dispatch. The actual producer must verify
    /// externally pinned Native history, live_scope and its independent fee.
    pub(in crate::provisioning) fn test_session_v2_stored_native_ready_identity(
        &self,
        id: &str,
    ) -> Result<(&str, String)> {
        let s = self.test_session_v2_record(id)?;
        let machine = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        machine.validate_session(s)?;
        let active = machine.active.as_ref().ok_or_else(invalid)?;
        if active.ready.is_none() || machine.registered.is_none() {
            return Err(invalid());
        }
        let signed = active.signed.as_ref().ok_or_else(invalid)?;
        Ok((
            signed.certificate.opaque_registration_ref.as_str(),
            signed.digest().map_err(|_| invalid())?,
        ))
    }
}
