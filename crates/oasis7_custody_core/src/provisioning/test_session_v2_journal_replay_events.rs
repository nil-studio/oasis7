use super::*;
impl Replay<'_> {
    pub(super) fn apply(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
        ordinal: u64,
    ) -> Result<()> {
        use SessionEventKindV2 as E;
        match event {
            E::PreflightReservation => {
                if self.begun
                    || ordinal != 1
                    || at != self.s.budget_authorized_at
                    || self
                        .s
                        .book
                        .original(&self.s.input.operation_id)
                        .is_none_or(|r| r.input() != &self.s.input)
                {
                    return Err(invalid());
                }
                let id = digest(&(
                    "oasis7.custody.test-session-preflight-control-id.v2",
                    &self.s.input.operation_id,
                    self.s
                        .input
                        .request
                        .intent_digest()
                        .map_err(|_| invalid())?,
                ))?;
                let body = digest(&(
                    "oasis7.custody.test-session-preflight-control-body.v2",
                    &self.s.input,
                ))?;
                self.reserve_logical(&id, &body)?;
                self.begun = true;
            }
            E::ApprovalVerificationClaim => {
                if !self.begun
                    || self.verify_claimed
                    || self.approval_consumed
                    || self.closing.is_some()
                    || self.s.approval_verification_claimed_at != Some(at)
                {
                    return Err(invalid());
                }
                self.identity.charge(1).map_err(|_| invalid())?;
                self.verify_claimed = true;
            }
            E::ApprovalConsumed => {
                if !self.verify_claimed
                    || self.approval_consumed
                    || self.closing.is_some()
                    || self.s.consumed_at != Some(at)
                {
                    return Err(invalid());
                }
                self.approval_consumed = true;
            }
            E::SourceReserved {
                source_operation_id: id,
            } => {
                if !self.approval_consumed {
                    return Err(invalid());
                }
                let source = &self.s.sources.get(id).ok_or_else(invalid)?.source;
                if source.created_at.0 != at.get() {
                    return Err(invalid());
                }
                Self::unique(&mut self.sources, id)?;
                self.reads.insert(id.clone(), 0);
                let funding = self.s.source_funding.get(id).ok_or_else(invalid)?;
                if let Some(p) = funding.purpose() {
                    self.live(p, at)?;
                    self.root_transport
                        .get_mut(&p)
                        .ok_or_else(invalid)?
                        .charge(1)
                        .map_err(|_| invalid())?;
                } else {
                    self.identity.charge(1).map_err(|_| invalid())?;
                }
            }
            E::SourceReadReserved {
                source_operation_id: id,
                ticket,
            } => {
                let source = &self.s.sources.get(id).ok_or_else(invalid)?.source;
                if at.get() >= source.expires_at.0 {
                    return Err(invalid());
                }
                let count = self.reads.get_mut(id).ok_or_else(invalid)?;
                if *ticket != count.checked_add(1).ok_or_else(invalid)? || *ticket > 8 {
                    return Err(invalid());
                }
                *count = *ticket;
            }
            E::SourceVerificationClaimed {
                source_operation_id: id,
            } => {
                let source = &self.s.sources.get(id).ok_or_else(invalid)?.source;
                if !self.sources.contains(id)
                    || self.reads.get(id) != Some(&1)
                    || source.claimed_at.map(|v| v.0) != Some(at.get())
                    || source.request_bytes.is_none()
                    || at.get() >= source.expires_at.0
                {
                    return Err(invalid());
                }
                Self::unique(&mut self.source_claims, id)?;
            }
            E::SourceVerified {
                source_operation_id: id,
            } => {
                let source = &self.s.sources.get(id).ok_or_else(invalid)?.source;
                if !self.source_claims.contains(id)
                    || source.verified_at.map(|v| v.0) != Some(at.get())
                    || source.verification.is_none()
                    || at.get() >= source.expires_at.0
                {
                    return Err(invalid());
                }
                Self::unique(&mut self.source_verified, id)?;
                self.source_ready(id, at)?;
            }
            E::PreflightClaimed => {
                if !self.approval_consumed
                    || self.preflight_claimed
                    || self.active
                    || self.closing.is_some()
                    || self.s.preflight_claimed_at != Some(at)
                {
                    return Err(invalid());
                }
                let nonce = self.s.preflight_source_nonce.as_ref().ok_or_else(invalid)?;
                let source = self
                    .s
                    .sources
                    .iter()
                    .find(|(_, w)| &w.source.verify_nonce == nonce)
                    .map(|(id, _)| id)
                    .ok_or_else(invalid)?;
                if !self.sources.contains(source) {
                    return Err(invalid());
                }
                self.preflight_claimed = true;
            }
            E::PreflightReadReserved { ticket } => {
                if !self.preflight_claimed
                    || self.active
                    || self.closing.is_some()
                    || *ticket != self.preflight_reads.checked_add(1).ok_or_else(invalid)?
                    || *ticket > 5
                {
                    return Err(invalid());
                }
                self.preflight_reads = *ticket;
            }
            E::PreflightStep { step } => {
                if !self.preflight_claimed
                    || self.active
                    || self.closing.is_some()
                    || self.s.preflight_steps.get(step) != Some(&at)
                    || !self.steps.insert(*step)
                {
                    return Err(invalid());
                }
                match step {
                    SessionPreflightStepV1::FreshAccessPair => {
                        if self.preflight_reads < 2 {
                            return Err(invalid());
                        }
                    }
                    SessionPreflightStepV1::Lifetime => {
                        if !self.access || !self.published {
                            return Err(invalid());
                        }
                    }
                    SessionPreflightStepV1::OwnerRead => {
                        if !self.lifetime {
                            return Err(invalid());
                        }
                    }
                }
                self.identity
                    .charge(if *step == SessionPreflightStepV1::FreshAccessPair {
                        4
                    } else {
                        1
                    })
                    .map_err(|_| invalid())?;
            }
            E::PreflightAccessRecorded => {
                if self.access
                    || !self
                        .steps
                        .contains(&SessionPreflightStepV1::FreshAccessPair)
                    || self.preflight_reads < 3
                    || self.s.preflight_access.is_none()
                {
                    return Err(invalid());
                }
                self.access = true;
            }
            E::PreflightReadPublished => {
                if self.active
                    || self.closing.is_some()
                    || !self.preflight_claimed
                    || self.published
                    || !self.access
                    || self.preflight_reads != 5
                    || self.s.preflight_read.is_none()
                {
                    return Err(invalid());
                }
                self.published = true;
            }
            E::PreflightLifetimeRecorded => {
                if self.active
                    || self.closing.is_some()
                    || !self.preflight_claimed
                    || self.lifetime
                    || !self.steps.contains(&SessionPreflightStepV1::Lifetime)
                    || self.s.preflight_lifetime.is_none()
                {
                    return Err(invalid());
                }
                self.lifetime = true;
            }
            E::Activated => {
                if self.active
                    || !self.approval_consumed
                    || self.closing.is_some()
                    || self.steps.len() != 3
                    || !self.access
                    || !self.lifetime
                    || !self.published
                    || self.s.approved_at != Some(at)
                {
                    return Err(invalid());
                }
                let p = self.s.header()?.0;
                if at.get() >= p.write_deadline.0 {
                    return Err(invalid());
                }
                self.active = true;
                self.lease = Some(UnixMillis::new(
                    at.get()
                        .checked_add(90_000)
                        .ok_or_else(invalid)?
                        .min(p.write_deadline.0),
                ));
                for (purpose, limit) in [
                    (SessionPurposeV1::Execution, p.transport_limit.0),
                    (SessionPurposeV1::ReconcileRead, p.read_budget.transport.0),
                    (
                        SessionPurposeV1::TerminationPlan,
                        p.cleanup_budget.transport.0,
                    ),
                ] {
                    self.root_transport
                        .insert(purpose, BudgetLedger::new(limit));
                    self.generations.insert(purpose, 0);
                }
            }
            E::ClientHttpCharged {
                ingress_attempt_id: id,
            } => {
                if !self.active {
                    return Err(invalid());
                }
                let paid = self.s.client_http_admissions.get(id).ok_or_else(invalid)?;
                if paid.charged_at != at
                    || paid.hop != SessionControlHopV1::Host
                    || self.s.control_transports.get(id) != Some(&SessionControlHopV1::Host)
                {
                    return Err(invalid());
                }
                Self::unique(&mut self.fees, id)?;
                self.control.transport.charge(1).map_err(|_| invalid())?;
            }
            E::LifetimeClaimed {
                source_operation_id: id,
                ingress_attempt_id: ingress,
                profile,
            } => {
                if !self.active
                    || !self.fees.contains(ingress)
                    || self.reads.get(id) != Some(&3)
                    || self.s.source_funding.get(id) != Some(&SessionSourceFundingV1::Control)
                    || !self.lifetimes.insert((*profile, id.clone()))
                {
                    return Err(invalid());
                }
                self.source_ready(id, at)?;
                let present = match profile {
                    SessionJournalProfileV2::Credential => {
                        self.s.control_transports.get(
                            &super::super::super::v2_credential_source::lifetime_attempt_v2(
                                self.s, id,
                            )?,
                        ) == Some(&SessionControlHopV1::Identity)
                    }
                    SessionJournalProfileV2::Control => {
                        super::super::super::v2_control::control_lifetime_claimed_v2(self.s, id)?
                    }
                };
                if !present
                    || self
                        .lifetimes
                        .iter()
                        .filter(|(_, source)| source == id)
                        .count()
                        != 1
                {
                    return Err(invalid());
                }
                self.identity.charge(1).map_err(|_| invalid())?;
            }
            E::ChallengeIssued {
                request_id: id,
                source_operation_id: source,
                ingress_attempt_id: ingress,
                profile,
            } => {
                let c = self.s.challenges.get(id).ok_or_else(invalid)?;
                self.source_ready(source, at)?;
                if !self.fees.contains(ingress)
                    || self.reads.get(source) != Some(&4)
                    || !self.lifetimes.contains(&(*profile, source.clone()))
                {
                    return Err(invalid());
                }
                match profile {
                    SessionJournalProfileV2::Credential => {
                        if c.kind != SessionChallengeKindV1::CredentialClaim {
                            return Err(invalid());
                        }
                    }
                    SessionJournalProfileV2::Control => match self.s.control_records.get(id) {
                        Some(SessionControlRecordV2::Challenge {
                            sequence,
                            ingress_attempt_id,
                            observation,
                            ..
                        }) if *sequence == ordinal
                            && ingress_attempt_id == ingress
                            && observation.source_operation_id() == source => {}
                        _ => return Err(invalid()),
                    },
                }
                let logical = c.logical_request_id.clone();
                let body = c.body_digest.clone();
                self.issue_challenge(id, at)?;
                self.reserve_logical(&logical, &body)?;
            }
            E::CredentialClaimed { request_id: id } => {
                let record = self.s.credential_claims.get(id).ok_or_else(invalid)?;
                if record.claimed_at != at
                    || !self.fees.contains(&record.ingress_attempt_id)
                    || self.reads.get(&record.source_operation_id) != Some(&4)
                    || !self.lifetimes.contains(&(
                        SessionJournalProfileV2::Credential,
                        record.source_operation_id.clone(),
                    ))
                {
                    return Err(invalid());
                }
                self.source_ready(&record.source_operation_id, at)?;
                let c = self
                    .s
                    .challenges
                    .get(&record.challenge_request_id)
                    .ok_or_else(invalid)?;
                if c.kind != SessionChallengeKindV1::CredentialClaim
                    || c.credential_generation.0
                        != *self.generations.get(&c.purpose).ok_or_else(invalid)?
                {
                    return Err(invalid());
                }
                for allocation in &record.allocations {
                    self.live(allocation.purpose, at)?;
                    let old = self
                        .generations
                        .get_mut(&allocation.purpose)
                        .ok_or_else(invalid)?;
                    if allocation.generation.0 != old.checked_add(1).ok_or_else(invalid)? {
                        return Err(invalid());
                    }
                    *old = allocation.generation.0;
                }
                self.consume(&record.challenge_request_id, id, c.purpose, at)?;
                Self::unique(&mut self.claims, id)?;
            }
            E::ControlAcknowledged { request_id: id } => {
                let ack = self.s.controls.get(id).ok_or_else(invalid)?;
                let (q, challenge, observation) = match self.s.control_records.get(id) {
                    Some(SessionControlRecordV2::Control {
                        sequence,
                        request,
                        challenge_request_id,
                        observation,
                        ..
                    }) if *sequence == ordinal => (request, challenge_request_id, observation),
                    _ => return Err(invalid()),
                };
                let c = self.s.challenges.get(challenge).ok_or_else(invalid)?;
                if ack.acknowledged_at != at
                    || ack.kind != q.kind
                    || c.credential_generation.0
                        != *self.generations.get(&c.purpose).ok_or_else(invalid)?
                {
                    return Err(invalid());
                }
                if matches!(
                    q.kind,
                    SessionControlKindV1::Finish | SessionControlKindV1::Abort
                ) {
                    if !self.active || observation.is_some() {
                        return Err(invalid());
                    }
                    if self.closing.is_none() {
                        self.closing = Some((
                            if q.kind == SessionControlKindV1::Finish {
                                TestSessionClosingReasonV1::Finished
                            } else {
                                TestSessionClosingReasonV1::Aborted
                            },
                            at,
                        ));
                    }
                } else {
                    self.live(c.purpose, at)?;
                    let o = observation.as_ref().ok_or_else(invalid)?;
                    self.source_ready(o.source_operation_id(), at)?;
                    if self.reads.get(o.source_operation_id()) != Some(&4)
                        || !self.lifetimes.contains(&(
                            SessionJournalProfileV2::Control,
                            o.source_operation_id().into(),
                        ))
                    {
                        return Err(invalid());
                    }
                    if q.kind == SessionControlKindV1::Heartbeat {
                        if c.purpose != SessionPurposeV1::Execution
                            || self
                                .heartbeat
                                .is_some_and(|old| at.get() < old.get().saturating_add(30_000))
                        {
                            return Err(invalid());
                        }
                        self.heartbeat = Some(at);
                        self.lease = Some(UnixMillis::new(
                            at.get()
                                .checked_add(90_000)
                                .ok_or_else(invalid)?
                                .min(self.s.header()?.0.write_deadline.0),
                        ));
                    }
                }
                if self.lease != Some(ack.lease_until) {
                    return Err(invalid());
                }
                self.consume(challenge, id, c.purpose, at)?;
                Self::unique(&mut self.acks, id)?;
            }
            E::Expired { reason } => {
                if self.closing.is_some() {
                    return Err(invalid());
                }
                let p = self.s.header()?.0;
                let until = if self.active {
                    p.write_deadline
                        .0
                        .min(self.lease.ok_or_else(invalid)?.get())
                } else if !self.approval_consumed {
                    p.write_deadline
                        .0
                        .min(self.s.input.approval.proof.expires_at.0)
                } else {
                    p.write_deadline.0
                };
                let expected = if self.active && at.get() < p.write_deadline.0 {
                    TestSessionClosingReasonV1::LeaseExpired
                } else {
                    TestSessionClosingReasonV1::Deadline
                };
                if at.get() < until || *reason != expected {
                    return Err(invalid());
                }
                self.closing = Some((*reason, at));
            }
            // Their actual independent consumers must extend this closed replay
            // before any future field or Book can carry an admissible event.
            _ => return self.apply_purpose(event, at),
        }
        Ok(())
    }
}
