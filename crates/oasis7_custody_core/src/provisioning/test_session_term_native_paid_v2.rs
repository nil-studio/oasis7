//! First paid Native child of the fixed Term6 ledger, not Current or World authority.
use super::*;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    AccountMachinePurposeV2, NativeTermCurrentEnvelopeV2, NativeTermCurrentPaidEdgeV2,
    SignedNativeTermCurrentPaidEdgeV2,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TermNativePaidRecordV2 {
    envelope: NativeTermCurrentEnvelopeV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    recorded_at: UnixMillis,
    ordinal: u64,
}
/// Only the actual paid Native port token is consumed by this factory.
pub struct PreparedTermNativePaidV2 {
    session_id: String,
    round_id: String,
    body: NativeTermCurrentRequestV2,
    edge: NativeTermCurrentPaidEdgeV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
}
impl PreparedTermNativePaidV2 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.edge.issuer
    }
    pub fn issuer_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.edge.issued_at.0, self.edge.expires_at.0)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.edge.signing_bytes().map_err(|_| invalid())
    }
}
/// The first signed-original SQL commit alone returns the exact HTTP original.
/// Historical paid records cannot reconstruct this dispatch continuation.
pub struct TermNativeDispatchV2 {
    envelope: NativeTermCurrentEnvelopeV2,
}
impl TermNativeDispatchV2 {
    pub fn envelope(&self) -> &NativeTermCurrentEnvelopeV2 {
        &self.envelope
    }
}
fn body(
    s: &TestSessionAuthorityV2,
    r: &TermSixFundingRecordV2,
) -> Result<NativeTermCurrentRequestV2> {
    let term = &s.header()?.0.termination;
    Ok(NativeTermCurrentRequestV2 {
        api_version: 2,
        session_id: s.input.operation_id.clone(),
        owned_invocation_id: r.header.invocation_id.clone(),
        source_operation_id: r.header.source_id.clone(),
        stop_operation_id: term.stop_operation_id.clone(),
        close_operation_id: term.close_operation_id.clone(),
        nonce: r.header.nonce.clone(),
        lifetime_nonce: r.header.lifetime_nonce.clone(),
        origin_nonce: r.header.origin_nonce.clone(),
    })
}
fn expected(
    s: &TestSessionAuthorityV2,
    r: &TermSixFundingRecordV2,
    issuer: AccountMachineIssuerV2,
    pin: &AccountMachineIssuerPinV2,
    issued: u64,
) -> Result<(NativeTermCurrentRequestV2, NativeTermCurrentPaidEdgeV2)> {
    scope(s)?;
    let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
    m.validate_session(s)?;
    let pending = m.signed.as_ref().ok_or_else(invalid)?;
    let active = m.active.as_ref().ok_or_else(invalid)?;
    if active.ready.is_none() {
        return Err(invalid());
    }
    let certificate = active.signed.as_ref().ok_or_else(invalid)?;
    let ack = &m.registered.as_ref().ok_or_else(invalid)?.ack;
    let claim = r
        .claims
        .get(&TermObservationPortV2::Native)
        .ok_or_else(invalid)?;
    if issued < claim.at.get() {
        return Err(invalid());
    }
    let body = body(s, r)?;
    let until = r
        .header
        .until
        .get()
        .min(pin.not_after.0)
        .min(certificate.certificate.expires_at.0);
    let edge = NativeTermCurrentPaidEdgeV2 {
        api_version: 2,
        issuer,
        session_id: s.input.operation_id.clone(),
        approved_input_digest: s.input_digest()?,
        termination_plan_digest: r.header.term_digest.clone(),
        active_certificate_digest: certificate.digest().map_err(|_| invalid())?,
        opaque_registration_ref: ack.ack.opaque_registration_ref.clone(),
        native_key_id: ack.ack.native_key_id.clone(),
        identity_namespace: ack.ack.identity_namespace.clone(),
        identity_epoch: ack.ack.identity_epoch,
        native_body_digest: body.body_digest().map_err(|_| invalid())?,
        graph_reservation_digest: digest(&("oasis7.custody.term-six-header.v2", &r.header))?,
        paid_record_digest: digest(&(
            "oasis7.custody.term-six-paid-port.v2",
            &s.input.operation_id,
            &r.header,
            TermObservationPortV2::Native,
            claim,
        ))?,
        native_transport_attempt_id: r.header.ports[5].attempt_id.clone(),
        native_paid_ordinal: DecimalU64(
            u64::from(r.header.ports[5].paid_index)
                .checked_add(1)
                .ok_or_else(invalid)?,
        ),
        native_http_units: DecimalU64(1),
        observation_started_at: DecimalU64(r.header.started_at.get()),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(until),
        cleanup_until: DecimalU64(s.header()?.0.termination.cleanup_until.get()),
    };
    edge.validate_relative(&body, pending, certificate, ack)
        .map_err(|_| invalid())?;
    Ok((body, edge))
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_term_native_paid_v2(
        &self,
        first: PreparedTermObservationPortV2,
        config: NativeMachinePendingIssuerV2<'_>,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedTermNativePaidV2> {
        if first.port != TermObservationPortV2::Native {
            return Err(invalid());
        }
        for check in self.test_session_v2_native_restore_checks(&first.session_id, native)? {
            verifier.verify(&check)?;
        }
        let s =
            self.test_session_v2_local(&first.session_id, SessionPurposeV1::TerminationPlan, now)?;
        let r = find(s, &first.header.round_id)?;
        if r.header != first.header
            || r.claims.get(&first.port) != Some(&first.claim)
            || r.native_paid.is_some()
            || config.issuer.native_audience_origin != native.http_origin
        {
            return Err(invalid());
        }
        let (body, edge) = expected(s, r, config.issuer, config.pin, now.get())?;
        edge.issuer
            .verify_pin(
                config.pin,
                config.floor,
                AccountMachinePurposeV2::NativeTermCurrentAttempt,
                edge.issued_at.0,
                edge.expires_at.0,
                now.get(),
            )
            .map_err(|_| invalid())?;
        Ok(PreparedTermNativePaidV2 {
            session_id: first.session_id,
            round_id: first.header.round_id,
            body,
            edge,
            pin: config.pin.clone(),
            floor: config.floor.clone(),
        })
    }
    pub fn record_test_session_term_native_paid_v2(
        &mut self,
        first: PreparedTermNativePaidV2,
        signature_hex: String,
        now: UnixMillis,
    ) -> Result<TermNativeDispatchV2> {
        let s =
            self.test_session_v2_local(&first.session_id, SessionPurposeV1::TerminationPlan, now)?;
        let r = find(s, &first.round_id)?;
        if r.native_paid.is_some()
            || expected(
                s,
                r,
                first.edge.issuer.clone(),
                &first.pin,
                first.edge.issued_at.0,
            )? != (first.body.clone(), first.edge.clone())
        {
            return Err(invalid());
        }
        let envelope = NativeTermCurrentEnvelopeV2 {
            body: first.body,
            paid: SignedNativeTermCurrentPaidEdgeV2 {
                edge: first.edge,
                signature_hex,
            },
        };
        let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        envelope
            .verify_issuer(
                m.signed.as_ref().ok_or_else(invalid)?,
                m.active
                    .as_ref()
                    .and_then(|a| a.signed.as_ref())
                    .ok_or_else(invalid)?,
                &m.registered.as_ref().ok_or_else(invalid)?.ack,
                &first.pin,
                &first.floor,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let ordinal = next_session_ordinal_v2(s)?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&first.session_id)?;
        s.termination_state
            .term_six_funding
            .iter_mut()
            .find(|r| r.header.round_id == first.round_id)
            .ok_or_else(invalid)?
            .native_paid = Some(TermNativePaidRecordV2 {
            envelope: envelope.clone(),
            pin: first.pin,
            floor: first.floor,
            recorded_at: now,
            ordinal,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::TermSixNativePaidRecorded {
                round_id: first.round_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(TermNativeDispatchV2 { envelope })
    }
}
pub(super) fn validate_record(
    s: &TestSessionAuthorityV2,
    r: &TermSixFundingRecordV2,
) -> Result<()> {
    let event = session_event_ordinal_v2(
        s,
        &SessionEventKindV2::TermSixNativePaidRecorded {
            round_id: r.header.round_id.clone(),
        },
    )?;
    if event.is_some() != r.native_paid.is_some() {
        return Err(invalid());
    }
    let Some(record) = &r.native_paid else {
        return Ok(());
    };
    let (body, edge) = expected(
        s,
        r,
        record.envelope.paid.edge.issuer.clone(),
        &record.pin,
        record.envelope.paid.edge.issued_at.0,
    )?;
    let claim = r
        .claims
        .get(&TermObservationPortV2::Native)
        .ok_or_else(invalid)?;
    if record.envelope.body != body
        || record.envelope.paid.edge != edge
        || event != Some(record.ordinal)
        || record.ordinal <= claim.ordinal
        || record.recorded_at.get() < edge.issued_at.0
        || record.recorded_at.get() >= edge.expires_at.0
    {
        return Err(invalid());
    }
    let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
    record
        .envelope
        .verify_issuer(
            m.signed.as_ref().ok_or_else(invalid)?,
            m.active
                .as_ref()
                .and_then(|a| a.signed.as_ref())
                .ok_or_else(invalid)?,
            &m.registered.as_ref().ok_or_else(invalid)?.ack,
            &record.pin,
            &record.floor,
            edge.issued_at.0,
        )
        .map_err(|_| invalid())
}
pub(super) fn replay_record(
    r: &TermSixFundingRecordV2,
    at: UnixMillis,
    ordinal: u64,
) -> Result<()> {
    let p = r.native_paid.as_ref().ok_or_else(invalid)?;
    if p.recorded_at != at || p.ordinal != ordinal {
        return Err(invalid());
    }
    Ok(())
}
