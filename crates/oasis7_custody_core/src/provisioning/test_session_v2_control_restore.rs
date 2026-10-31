//! Replay only typed V2 control facts and the original lease; never issue authority.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
fn paid(s: &TestSessionAuthorityV2, ingress: &str, signature: &str, at: UnixMillis) -> Result<()> {
    let p = s.client_http_admissions.get(ingress).ok_or_else(invalid)?;
    if !hex64(signature)
        || p.signed_attempt_digest != signature
        || p.hop != SessionControlHopV1::Host
        || s.control_transports.get(ingress) != Some(&SessionControlHopV1::Host)
        || p.charged_at.get() > at.get()
    {
        return Err(invalid());
    }
    Ok(())
}
pub(in crate::provisioning) fn validate(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
) -> Result<()> {
    for (id, body) in &s.control_logicals {
        if !s
            .challenges
            .values()
            .any(|c| &c.logical_request_id == id && &c.body_digest == body)
        {
            return Err(invalid());
        }
    }
    if s.control_records.len() > 1104 || s.controls.len() > 80 {
        return Err(invalid());
    }
    // An unknown LF dispatch retains its exact derived key and charge. It never
    // becomes a second permit, and a Source cannot fund both credential and Control LF.
    for (id, hop) in &s.control_transports {
        if *hop != SessionControlHopV1::Identity {
            continue;
        }
        let mut found = false;
        for (source, funding) in &s.source_funding {
            if *funding != SessionSourceFundingV1::Control {
                continue;
            }
            if id == &source::lifetime_attempt(s, source)?
                || id == &super::super::v2_credential_source::lifetime_attempt_v2(s, source)?
            {
                found = true;
                break;
            }
        }
        if !found {
            return Err(invalid());
        }
    }
    for source in s.source_funding.keys() {
        if source::control_lifetime_claimed_v2(s, source)?
            && s.control_transports.contains_key(
                &super::super::v2_credential_source::lifetime_attempt_v2(s, source)?,
            )
        {
            return Err(invalid());
        }
    }
    if s.approved_at.is_none() {
        return if s.control_records.is_empty()
            && s.controls.is_empty()
            && s.last_heartbeat.is_none()
        {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    let approved = s.approved_at.ok_or_else(invalid)?;
    let write = s.header()?.0.write_deadline.0;
    let mut ordered = BTreeMap::new();
    for (key, record) in &s.control_records {
        let sequence = match record {
            SessionControlRecordV2::Challenge { sequence, .. }
            | SessionControlRecordV2::Control { sequence, .. } => *sequence,
        };
        if !valid_id(key) || sequence == 0 || ordered.insert(sequence, (key, record)).is_some() {
            return Err(invalid());
        }
    }
    let mut lease = UnixMillis::new(
        approved
            .get()
            .checked_add(90_000)
            .ok_or_else(invalid)?
            .min(write),
    );
    let mut heartbeat = None;
    let mut previous = approved;
    let mut used_sources = BTreeSet::new();
    let mut first_requested_close = None;
    for (index, (sequence, (key, record))) in ordered.iter().enumerate() {
        if *sequence != u64::try_from(index + 1).map_err(|_| invalid())? {
            return Err(invalid());
        }
        let at = match record {
            SessionControlRecordV2::Challenge {
                approved_input_digest,
                request,
                ingress_attempt_id,
                signed_attempt_digest,
                observation,
                ..
            } => {
                let c = s.challenges.get(*key).ok_or_else(invalid)?;
                if request.api_version != 1
                    || &request.request_id != *key
                    || request.session_id != s.input.operation_id
                    || approved_input_digest != &s.input_digest()?
                    || request.kind != c.kind
                    || !matches!(request.kind, SessionChallengeKindV1::Control(_))
                    || request.purpose != c.purpose
                    || request.logical_request_id != c.logical_request_id
                    || request.operation_request_digest != c.body_digest
                    || s.controls.contains_key(*key)
                {
                    return Err(invalid());
                }
                paid(s, ingress_attempt_id, signed_attempt_digest, c.created_at)?;
                source::validate_observation(state, s, observation, c.purpose, c.created_at)?;
                if !used_sources.insert(&observation.source_operation_id) {
                    return Err(invalid());
                }
                if matches!(
                    c.kind,
                    SessionChallengeKindV1::Control(SessionControlKindV1::Heartbeat)
                ) && c.purpose != SessionPurposeV1::Execution
                {
                    return Err(invalid());
                }
                c.created_at
            }
            SessionControlRecordV2::Control {
                approved_input_digest,
                request,
                ingress_attempt_id,
                signed_attempt_digest,
                challenge_request_id,
                observation,
                ..
            } => {
                let ack = s.controls.get(*key).ok_or_else(invalid)?;
                let c = s.challenges.get(challenge_request_id).ok_or_else(invalid)?;
                if approved_input_digest != &s.input_digest()?
                    || request.api_version != 1
                    || &request.request_id != *key
                    || request.session_id != s.input.operation_id
                    || ack.request_id != **key
                    || ack.kind != request.kind
                    || c.kind != SessionChallengeKindV1::Control(request.kind)
                    || c.logical_request_id != **key
                    || c.body_digest != request.digest()?
                    || c.consumed_by.as_ref() != Some(*key)
                    || ack.acknowledged_at.get() < c.created_at.get()
                    || ack.acknowledged_at.get() >= c.expires_at.get()
                    || s.control_logicals.get(*key) != Some(&request.digest()?)
                {
                    return Err(invalid());
                }
                paid(
                    s,
                    ingress_attempt_id,
                    signed_attempt_digest,
                    ack.acknowledged_at,
                )?;
                if closing_kind(request.kind) {
                    if observation.is_some() {
                        return Err(invalid());
                    }
                    if first_requested_close.is_none() {
                        first_requested_close = Some((
                            if request.kind == SessionControlKindV1::Finish {
                                TestSessionClosingReasonV1::Finished
                            } else {
                                TestSessionClosingReasonV1::Aborted
                            },
                            ack.acknowledged_at,
                        ));
                    }
                } else {
                    let o = observation.as_ref().ok_or_else(invalid)?;
                    source::validate_observation(state, s, o, c.purpose, ack.acknowledged_at)?;
                    if !used_sources.insert(&o.source_operation_id) {
                        return Err(invalid());
                    }
                }
                if request.kind == SessionControlKindV1::Heartbeat {
                    if c.purpose != SessionPurposeV1::Execution
                        || heartbeat.is_some_and(|v: UnixMillis| {
                            ack.acknowledged_at.get() < v.get().saturating_add(30_000)
                        })
                    {
                        return Err(invalid());
                    }
                    heartbeat = Some(ack.acknowledged_at);
                    lease = UnixMillis::new(
                        ack.acknowledged_at
                            .get()
                            .checked_add(90_000)
                            .ok_or_else(invalid)?
                            .min(write),
                    );
                }
                if ack.lease_until != lease {
                    return Err(invalid());
                }
                ack.acknowledged_at
            }
        };
        if at.get() < previous.get() {
            return Err(invalid());
        }
        previous = at;
    }
    if s.last_heartbeat != heartbeat || s.lease_until != Some(lease) {
        return Err(invalid());
    }
    for (key, c) in &s.challenges {
        if matches!(c.kind, SessionChallengeKindV1::Control(_)) {
            if !matches!(
                s.control_records.get(key),
                Some(SessionControlRecordV2::Challenge { .. })
            ) {
                return Err(invalid());
            }
            if let Some(id) = &c.consumed_by {
                if !matches!(s.control_records.get(id),Some(SessionControlRecordV2::Control{challenge_request_id,..}) if challenge_request_id==key)
                {
                    return Err(invalid());
                }
            }
        }
    }
    if s.controls.keys().any(|key| {
        !matches!(
            s.control_records.get(key),
            Some(SessionControlRecordV2::Control { .. })
        )
    }) {
        return Err(invalid());
    }
    if s.credential_claims
        .values()
        .any(|c| used_sources.contains(&c.source_operation_id))
    {
        return Err(invalid());
    }
    if let Some((reason, at)) = first_requested_close {
        let actual = s.closing.ok_or_else(invalid)?;
        if matches!(
            actual.0,
            TestSessionClosingReasonV1::Finished | TestSessionClosingReasonV1::Aborted
        ) {
            if actual != (reason, at) {
                return Err(invalid());
            }
        } else if actual.1.get() > at.get() {
            return Err(invalid());
        }
    } else if s.closing.is_some_and(|v| {
        matches!(
            v.0,
            TestSessionClosingReasonV1::Finished | TestSessionClosingReasonV1::Aborted
        )
    }) {
        return Err(invalid());
    }
    Ok(())
}
