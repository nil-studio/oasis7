//! Exact first Actor+Native paid pair, never a Current or World capability.
use super::*;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    AccountMachinePurposeV2, NativeTermDirectBodyV3, NativeTermDirectFrameV3,
    SignedTermDirectActorV3, SignedTermDirectNativeV3, TermDirectNativeClaimV3,
    TermDirectPaidBindingV3,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TermDirectPaidRecordV3 {
    frame: NativeTermDirectFrameV3,
    actor_pin: AccountMachineIssuerPinV2,
    native_pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    recorded_at: UnixMillis,
    ordinal: u64,
    native_root_key_id: String,
    native_root_public_key: String,
    #[serde(deserialize_with = "required_result")]
    result: Option<TermDirectResultRecordV3>,
}
impl TermDirectPaidRecordV3 {
    pub(super) fn has_result(&self) -> bool {
        self.result.is_some()
    }
    pub(super) fn replay_result(&self, at: UnixMillis, ordinal: u64) -> Result<()> {
        let r = self.result.as_ref().ok_or_else(invalid)?;
        if r.recorded_at != at || r.ordinal != ordinal {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn at(&self) -> UnixMillis {
        self.recorded_at
    }
    pub(super) fn ordinal(&self) -> u64 {
        self.ordinal
    }
}
/// Borrowed fixed configuration; these fields confer no authority by themselves.
pub struct TermDirectIssuerV3<'a> {
    pub issuer: AccountMachineIssuerV2,
    pub actor_pin: &'a AccountMachineIssuerPinV2,
    pub native_pin: &'a AccountMachineIssuerPinV2,
    pub floor: &'a AccountMachineIssuerFloorV2,
    pub actor_object_id: &'a str,
}
pub struct PreparedTermDirectNativeV3 {
    id: String,
    round_id: String,
    body: NativeTermDirectBodyV3,
    binding: TermDirectPaidBindingV3,
    actor_pin: AccountMachineIssuerPinV2,
    native_pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    native_root_key_id: String,
    native_root_public_key: String,
}
impl PreparedTermDirectNativeV3 {
    pub fn issuer(&self) -> &AccountMachineIssuerV2 {
        &self.binding.issuer
    }
    pub fn actor_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.actor_pin
    }
    pub fn native_pin(&self) -> &AccountMachineIssuerPinV2 {
        &self.native_pin
    }
    pub fn validity(&self) -> (u64, u64) {
        (self.binding.issued_at.0, self.binding.expires_at.0)
    }
    pub fn actor_signing_bytes(&self) -> Result<Vec<u8>> {
        self.binding.actor_signing_bytes().map_err(|_| invalid())
    }
    pub fn native_signing_bytes(&self, actor_signature: &str) -> Result<Vec<u8>> {
        let actor = SignedTermDirectActorV3 {
            binding: self.binding.clone(),
            signature_hex: actor_signature.into(),
        };
        actor
            .verify(
                &self.body,
                &self.actor_pin,
                &self.floor,
                &self.binding.actor_object_id,
                self.binding.issued_at.0,
            )
            .map_err(|_| invalid())?;
        TermDirectNativeClaimV3 {
            binding: self.binding.clone(),
            actor_proof_digest: actor.digest().map_err(|_| invalid())?,
        }
        .signing_bytes()
        .map_err(|_| invalid())
    }
}
/// Created only after the exact signed-frame Account SQL commit succeeds.
pub struct TermDirectNativeDispatchV3 {
    frame: NativeTermDirectFrameV3,
}
impl TermDirectNativeDispatchV3 {
    pub fn frame(&self) -> &NativeTermDirectFrameV3 {
        &self.frame
    }
}
fn expected(
    s: &TestSessionAuthorityV2,
    r: &TermFiveFundingRecordV3,
    issuer: AccountMachineIssuerV2,
    actor_pin: &AccountMachineIssuerPinV2,
    native_pin: &AccountMachineIssuerPinV2,
    object: &str,
    issued: u64,
) -> Result<(NativeTermDirectBodyV3, TermDirectPaidBindingV3)> {
    scope(s)?;
    let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
    m.validate_session(s)?;
    let pending = m.signed.as_ref().ok_or_else(invalid)?;
    let active_record = m
        .active
        .as_ref()
        .filter(|a| a.ready.is_some())
        .ok_or_else(invalid)?;
    let active = active_record.signed.as_ref().ok_or_else(invalid)?;
    let registered = &m.registered.as_ref().ok_or_else(invalid)?.ack;
    let actor = r
        .claims
        .get(&TermObservationPortV3::NativeSessionsActor)
        .ok_or_else(invalid)?;
    let native = r
        .claims
        .get(&TermObservationPortV3::NativeHttp)
        .ok_or_else(invalid)?;
    if issued < actor.at.get()
        || issued < native.at.get()
        || actor.ordinal >= native.ordinal
        || actor_pin.public_key != native_pin.public_key
    {
        return Err(invalid());
    }
    let c = &pending.certificate;
    let term = &s.header()?.0.termination;
    let body = NativeTermDirectBodyV3 {
        api_version: 3,
        session_id: s.input.operation_id.clone(),
        owned_invocation_id: r.header.invocation_id.clone(),
        source_operation_id: r.header.source_id.clone(),
        stop_operation_id: term.stop_operation_id.clone(),
        close_operation_id: term.close_operation_id.clone(),
        nonce: r.header.nonce.clone(),
        lifetime_nonce: r.header.lifetime_nonce.clone(),
        origin_nonce: r.header.origin_nonce.clone(),
        metadata_session: c.participant_session.clone(),
        metadata_lifetime: c.native_lifetime.clone(),
    };
    let paid = |port, claim: &Claim| {
        digest(&(
            "oasis7.custody.term-five-paid-port.v3",
            &s.input.operation_id,
            &r.header,
            port,
            claim,
        ))
    };
    let binding = TermDirectPaidBindingV3 {
        api_version: 3,
        issuer,
        session_id: s.input.operation_id.clone(),
        approved_input_digest: s.input_digest()?,
        termination_plan_digest: r.header.term_digest.clone(),
        active_certificate_digest: active.digest().map_err(|_| invalid())?,
        opaque_registration_ref: registered.ack.opaque_registration_ref.clone(),
        native_key_id: registered.ack.native_key_id.clone(),
        identity_namespace: registered.ack.identity_namespace.clone(),
        identity_epoch: registered.ack.identity_epoch,
        observation_round: r.header.round,
        round_id: r.header.round_id.clone(),
        actor_object_id: object.into(),
        actor_attempt_id: r.header.ports[3].attempt_id.clone(),
        native_attempt_id: r.header.ports[4].attempt_id.clone(),
        actor_paid_ordinal: DecimalU64(u64::from(r.header.ports[3].paid_index) + 1),
        native_paid_ordinal: DecimalU64(u64::from(r.header.ports[4].paid_index) + 1),
        graph_digest: digest(&("oasis7.custody.term-five-header.v3", &r.header))?,
        actor_paid_digest: paid(TermObservationPortV3::NativeSessionsActor, actor)?,
        native_paid_digest: paid(TermObservationPortV3::NativeHttp, native)?,
        body_digest: body.digest().map_err(|_| invalid())?,
        observation_started_at: DecimalU64(r.header.started_at.get()),
        issued_at: DecimalU64(issued),
        expires_at: DecimalU64(
            r.header
                .until
                .get()
                .min(actor_pin.not_after.0)
                .min(native_pin.not_after.0)
                .min(active.certificate.expires_at.0),
        ),
        cleanup_until: DecimalU64(term.cleanup_until.get()),
    };
    binding
        .validate_relative(&body, pending, active, registered)
        .map_err(|_| invalid())?;
    Ok((body, binding))
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_term_direct_native_v3(
        &self,
        actor: PreparedTermObservationPortV3,
        native: PreparedTermObservationPortV3,
        config: TermDirectIssuerV3<'_>,
        actual_native: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedTermDirectNativeV3> {
        if actor.port != TermObservationPortV3::NativeSessionsActor
            || native.port != TermObservationPortV3::NativeHttp
            || actor.session_id != native.session_id
            || actor.header != native.header
        {
            return Err(invalid());
        }
        for job in self.test_session_v2_native_restore_checks(&actor.session_id, actual_native)? {
            verifier.verify(&job)?;
        }
        let s =
            self.test_session_v2_local(&actor.session_id, SessionPurposeV1::TerminationPlan, now)?;
        let r = find(s, &actor.header.round_id)?;
        if r.native_paid.is_some()
            || r.claims.get(&actor.port) != Some(&actor.claim)
            || r.claims.get(&native.port) != Some(&native.claim)
            || r.header != actor.header
        {
            return Err(invalid());
        }
        if config.issuer.native_audience_origin != actual_native.http_origin {
            return Err(invalid());
        }
        let (body, binding) = expected(
            s,
            r,
            config.issuer,
            config.actor_pin,
            config.native_pin,
            config.actor_object_id,
            now.get(),
        )?;
        for (pin, purpose) in [
            (
                config.actor_pin,
                AccountMachinePurposeV2::TermNativeSessionsCurrentV3,
            ),
            (
                config.native_pin,
                AccountMachinePurposeV2::NativeTermCurrentAttemptV3,
            ),
        ] {
            binding
                .issuer
                .verify_pin(
                    pin,
                    config.floor,
                    purpose,
                    binding.issued_at.0,
                    binding.expires_at.0,
                    now.get(),
                )
                .map_err(|_| invalid())?;
        }
        Ok(PreparedTermDirectNativeV3 {
            id: actor.session_id,
            round_id: r.header.round_id.clone(),
            body,
            binding,
            actor_pin: config.actor_pin.clone(),
            native_pin: config.native_pin.clone(),
            floor: config.floor.clone(),
            native_root_key_id: actual_native.native_root_key_id.into(),
            native_root_public_key: actual_native.native_root_public_key.into(),
        })
    }
    pub fn record_test_session_term_direct_native_v3(
        &mut self,
        prepared: PreparedTermDirectNativeV3,
        actor_signature: String,
        native_signature: String,
        now: UnixMillis,
    ) -> Result<TermDirectNativeDispatchV3> {
        let actor = SignedTermDirectActorV3 {
            binding: prepared.binding.clone(),
            signature_hex: actor_signature,
        };
        let claim = TermDirectNativeClaimV3 {
            binding: prepared.binding.clone(),
            actor_proof_digest: actor.digest().map_err(|_| invalid())?,
        };
        let frame = NativeTermDirectFrameV3 {
            body: prepared.body.clone(),
            actor,
            native: SignedTermDirectNativeV3 {
                claim,
                signature_hex: native_signature,
            },
        };
        frame
            .verify_issuer(
                &prepared.actor_pin,
                &prepared.native_pin,
                &prepared.floor,
                &prepared.binding.actor_object_id,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let s = self.test_session_v2_local(&prepared.id, SessionPurposeV1::TerminationPlan, now)?;
        let r = find(s, &prepared.round_id)?;
        if r.native_paid.is_some()
            || expected(
                s,
                r,
                prepared.binding.issuer.clone(),
                &prepared.actor_pin,
                &prepared.native_pin,
                &prepared.binding.actor_object_id,
                prepared.binding.issued_at.0,
            )? != (prepared.body, prepared.binding)
        {
            return Err(invalid());
        }
        let record = TermDirectPaidRecordV3 {
            frame: frame.clone(),
            actor_pin: prepared.actor_pin,
            native_pin: prepared.native_pin,
            floor: prepared.floor,
            recorded_at: now,
            ordinal: next_session_ordinal_v2(s)?,
            native_root_key_id: prepared.native_root_key_id,
            native_root_public_key: prepared.native_root_public_key,
            result: None,
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&prepared.id)?;
        s.termination_state
            .term_five_funding
            .as_mut()
            .ok_or_else(invalid)?
            .iter_mut()
            .find(|r| r.header.round_id == prepared.round_id)
            .ok_or_else(invalid)?
            .native_paid = Some(record);
        append_session_event_v2(
            s,
            SessionEventKindV2::TermFiveNativePaidRecorded {
                round_id: prepared.round_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(TermDirectNativeDispatchV3 { frame })
    }
}
pub(super) fn validate_record(
    s: &TestSessionAuthorityV2,
    r: &TermFiveFundingRecordV3,
) -> Result<()> {
    let Some(p) = &r.native_paid else {
        return Ok(());
    };
    let b = &p.frame.actor.binding;
    let expected = expected(
        s,
        r,
        b.issuer.clone(),
        &p.actor_pin,
        &p.native_pin,
        &b.actor_object_id,
        b.issued_at.0,
    )?;
    if expected != (p.frame.body.clone(), b.clone())
        || p.recorded_at.get() < b.issued_at.0
        || p.recorded_at.get() >= b.expires_at.0
        || p.ordinal
            <= r.claims
                .get(&TermObservationPortV3::NativeHttp)
                .ok_or_else(invalid)?
                .ordinal
        || session_event_ordinal_v2(
            s,
            &SessionEventKindV2::TermFiveNativePaidRecorded {
                round_id: r.header.round_id.clone(),
            },
        )? != Some(p.ordinal)
    {
        return Err(invalid());
    }
    let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
    let pending = m.signed.as_ref().ok_or_else(invalid)?;
    // The captured fixed Root must verify the original Account-signed LF; a
    // result cannot select a new key and re-sign itself into this archive.
    pending
        .certificate
        .native_lifetime
        .verify_root(&p.native_root_key_id, &p.native_root_public_key)
        .map_err(|_| invalid())?;
    if let Some(result) = &p.result {
        validate_result(s, p, result)?;
    }
    p.frame
        .verify_issuer(
            &p.actor_pin,
            &p.native_pin,
            &p.floor,
            &b.actor_object_id,
            p.recorded_at.get(),
        )
        .map_err(|_| invalid())
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TermDirectResultRecordV3 {
    reply: crate::native_cleanup_v2::TermDirectPrivateCurrentReplyV3,
    recorded_at: UnixMillis,
    ordinal: u64,
}
fn required_result<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<TermDirectResultRecordV3>, D::Error> {
    Option::deserialize(d)
}

fn validate_result(
    s: &TestSessionAuthorityV2,
    p: &TermDirectPaidRecordV3,
    r: &TermDirectResultRecordV3,
) -> Result<()> {
    let m = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
    let pending = m.signed.as_ref().ok_or_else(invalid)?;
    let active = m
        .active
        .as_ref()
        .and_then(|a| a.signed.as_ref())
        .ok_or_else(invalid)?;
    let registration = &m.registered.as_ref().ok_or_else(invalid)?.ack;
    if r.recorded_at.get() < p.recorded_at.get()
        || r.ordinal <= p.ordinal
        || r.recorded_at.get() >= p.frame.actor.binding.expires_at.0
        || session_event_ordinal_v2(
            s,
            &SessionEventKindV2::TermFiveNativeResultRecorded {
                round_id: p.frame.actor.binding.round_id.clone(),
            },
        )? != Some(r.ordinal)
    {
        return Err(invalid());
    }
    r.reply
        .verify_for(
            &p.frame,
            crate::native_cleanup_v2::TermDirectFactContextV3 {
                pending,
                active,
                registration,
                native_key_id: &p.native_root_key_id,
                native_public_key: &p.native_root_public_key,
                now: r.recorded_at.get(),
            },
        )
        .map_err(|_| invalid())
}
impl AccountProvisionApprovalState {
    /// Records only the exact result of the original committed dispatch. This
    /// produces no Current/driver token, retry permit or new physical budget.
    pub fn record_test_session_term_direct_result_v3(
        &mut self,
        dispatch: TermDirectNativeDispatchV3,
        reply: crate::native_cleanup_v2::TermDirectPrivateCurrentReplyV3,
        now: UnixMillis,
    ) -> Result<()> {
        let id = &dispatch.frame.body.session_id;
        let round = &dispatch.frame.actor.binding.round_id;
        let s = self.test_session_v2_record(id)?;
        let p = find(s, round)?.native_paid.as_ref().ok_or_else(invalid)?;
        if p.frame != dispatch.frame || p.result.is_some() {
            return Err(invalid());
        }
        let result = TermDirectResultRecordV3 {
            reply,
            recorded_at: now,
            ordinal: next_session_ordinal_v2(s)?,
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.termination_state
            .term_five_funding
            .as_mut()
            .ok_or_else(invalid)?
            .iter_mut()
            .find(|r| r.header.round_id == *round)
            .ok_or_else(invalid)?
            .native_paid
            .as_mut()
            .ok_or_else(invalid)?
            .result = Some(result);
        append_session_event_v2(
            s,
            SessionEventKindV2::TermFiveNativeResultRecorded {
                round_id: round.clone(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
