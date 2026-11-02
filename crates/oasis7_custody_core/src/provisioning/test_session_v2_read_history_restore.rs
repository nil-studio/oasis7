//! Strict paid/read chronology and exact original/fact integrity only. Restore
//! cannot recreate the first opaque token or grant fresh Current from history.
use super::*;
pub(in super::super) fn validate_read_history(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
) -> Result<()> {
    for (ingress, a) in &s.purpose_http_admissions {
        let SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { fee: r } = &a.class else {
            continue;
        };
        let root = s
            .roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        let c = s
            .challenges
            .get(&r.challenge_request_id)
            .ok_or_else(invalid)?;
        if a.purpose != SessionPurposeV1::ReconcileRead
            || a.hop != SessionControlHopV1::Host
            || r.request.api_version != 2
            || r.request.request_id.is_empty()
            || r.request.request_id.len() > 256
            || r.request.request_id.chars().any(char::is_control)
            || r.request.session_id != s.input.operation_id
            || matches!(&r.request.action,SessionToolActionV1::ReconcileOperation{operation_id}
                if operation_id.is_empty() || operation_id.len()>256 || operation_id.chars().any(char::is_control))
            || r.request.purpose() != SessionPurposeV1::ReconcileRead
            || r.owner_paid_at.get() < a.charged_at.get()
            || r.owner_paid_at.get() >= root.deadline.get()
            || r.original_digest.as_ref().is_some_and(|d| {
                original(state, &s.input.operation_id, &r.request)
                    .ok()
                    .as_ref()
                    != Some(d)
            })
            || r.original_digest.is_none() && r.outcome.is_some()
            || a.charged_at.get() < c.created_at.get()
            || c.purpose != SessionPurposeV1::ReconcileRead
            || c.kind != SessionChallengeKindV1::ToolMetadata
            || c.logical_request_id != r.request.request_id
            || c.body_digest != r.request.digest()?
            || c.consumed_by.as_deref() != Some(r.request.request_id.as_str())
            || r.consumed_challenge && r.owner_paid_at.get() >= c.expires_at.get()
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ReadPrepareHistoryOwnerPaid {
                    ingress_attempt_id: ingress.clone(),
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        let event = SessionEventKindV2::ReadPrepareHistoryReturned {
            ingress_attempt_id: ingress.clone(),
        };
        if r.outcome.is_some() != session_event_ordinal_v2(s, &event)?.is_some() {
            return Err(invalid());
        }
        if let Some(o) = &r.outcome {
            let source = s.sources.get(&o.source_operation_id).ok_or_else(invalid)?;
            let signed = source.source.verification.as_ref().ok_or_else(invalid)?;
            if o.returned_at.get() >= source.source.expires_at.0
                || signed.verification.observed_at.0 > o.returned_at.get()
                || o.returned_at.get() - signed.verification.observed_at.0 > 5000
                || s.source_funding.get(&o.source_operation_id)
                    != Some(&SessionSourceFundingV1::ReconcileRead)
                || o.returned_at.get() < r.owner_paid_at.get()
                || o.returned_at.get() >= root.deadline.get()
                || o.observed_at.get() > o.returned_at.get()
                || o.returned_at.get() - o.observed_at.get() > 5000
                || o.private_sid_generation != s.session.private_sid_generation
                || o.source_ticket == 0
                || s.source_oracle_reads
                    .get(&o.source_operation_id)
                    .is_none_or(|n| o.source_ticket > *n)
                || o.source_digest != digest(source)?
            {
                return Err(invalid());
            }
            let nonce = state
                .test_session_v2_lifetime_nonce(&s.input.operation_id, &o.source_operation_id)?;
            o.lifetime
                .lifetime
                .validate_for(
                    &s.native_lifetime
                        .as_ref()
                        .ok_or_else(invalid)?
                        .lifetime
                        .status
                        .receipt,
                    &nonce,
                    s.session.native_binding_generation,
                    s.input.request.context.service_epoch,
                    o.returned_at.get(),
                )
                .map_err(|_| invalid())?;
            o.lifetime
                .lifetime
                .covers(root.deadline)
                .map_err(|_| invalid())?;
            if let Some(d) = &o.fact_digest {
                let Some((expected, at)) = fact(s)? else {
                    return Err(invalid());
                };
                if d != &expected || at.get() > o.returned_at.get() {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

/// A metadata nonce may be consumed by exactly the first paid Read callback,
/// never by an unbound opaque string or by a later callback/restart.
pub(in crate::provisioning) fn read_nonce_consumed_exact(
    s: &TestSessionAuthorityV2,
    request: &str,
    challenge: &str,
) -> bool {
    let Some(c) = s.challenges.get(challenge) else {
        return false;
    };
    if c.kind != SessionChallengeKindV1::ToolMetadata
        || c.purpose != SessionPurposeV1::ReconcileRead
        || c.consumed_by.as_deref() != Some(request)
        || c.logical_request_id != request
    {
        return false;
    }
    s.purpose_http_admissions
        .values()
        .filter(|a| {
            let SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { fee: r } = &a.class
            else {
                return false;
            };
            a.purpose == SessionPurposeV1::ReconcileRead
                && a.hop == SessionControlHopV1::Host
                && r.consumed_challenge
                && r.challenge_request_id == challenge
                && r.request.api_version == 2
                && r.request.session_id == s.input.operation_id
                && r.request.request_id == request
                && r.request.purpose() == SessionPurposeV1::ReconcileRead
                && r.request.digest().is_ok_and(|d| d == c.body_digest)
                && a.charged_at.get() >= c.created_at.get()
                && r.owner_paid_at.get() >= a.charged_at.get()
                && r.owner_paid_at.get() < c.expires_at.get()
        })
        .count()
        == 1
}
