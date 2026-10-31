//! Shared historical capacity plus the actual local Execution Closing barrier.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
fn consumed_at(s: &TestSessionAuthorityV2, c: &SessionChallengeV1) -> Result<Option<u64>> {
    let Some(id) = &c.consumed_by else {
        return Ok(None);
    };
    let at = match c.kind {
        SessionChallengeKindV1::CredentialClaim => s
            .credential_claims
            .get(id)
            .ok_or_else(invalid)?
            .claimed_at
            .get(),
        SessionChallengeKindV1::Control(_) => s
            .controls
            .get(id)
            .ok_or_else(invalid)?
            .acknowledged_at
            .get(),
        _ => return Err(invalid()),
    };
    if at < c.created_at.get() || at >= c.expires_at.get() {
        return Err(invalid());
    }
    Ok(Some(at))
}
fn execution_may_precede_closing(s: &TestSessionAuthorityV2, time: u64) -> bool {
    s.closing.is_none_or(|(reason, at)| {
        time < at.get()
            || time == at.get()
                && matches!(
                    reason,
                    TestSessionClosingReasonV1::Finished | TestSessionClosingReasonV1::Aborted
                )
    })
}
pub(super) fn validate(s: &TestSessionAuthorityV2) -> Result<()> {
    let mut facts = BTreeMap::new();
    let mut times = BTreeSet::new();
    for (id, c) in &s.challenges {
        facts.insert(id, (c, consumed_at(s, c)?));
        times.insert(c.created_at.get());
    }
    // A credential claim containing Execute also needs a possible place before
    // Closing; readonly claims do not acquire execution by this timestamp rule.
    for claim in s.credential_claims.values() {
        if claim
            .request
            .purposes
            .iter()
            .any(|p| matches!(p, crate::client_enrollment::TestSessionPurposeV1::Execute))
            && !execution_may_precede_closing(s, claim.claimed_at.get())
        {
            return Err(invalid());
        }
    }
    for ack in s.controls.values() {
        times.insert(ack.acknowledged_at.get());
    }
    for time in times {
        let closing_now = s.closing.is_some_and(|(reason, at)| {
            at.get() == time
                && matches!(
                    reason,
                    TestSessionClosingReasonV1::Finished | TestSessionClosingReasonV1::Aborted
                )
        });
        let mut open = execution_may_precede_closing(s, time);
        let mut pending = facts
            .values()
            .filter(|(c, end)| {
                c.created_at.get() < time
                    && time < c.expires_at.get()
                    && match end {
                        None => true,
                        Some(at) => {
                            *at > time
                                || *at == time
                                    && matches!(c.kind, SessionChallengeKindV1::Control(_))
                        }
                    }
            })
            .count();
        if pending > 8 {
            return Err(invalid());
        }
        let mut read_instant_room = pending < 8;
        let mut execution_instant_room = open && pending < 8;
        let (mut execution_retained, mut read_retained) = (0usize, 0usize);
        let (mut execution_instant, mut read_instant) = (false, false);
        for (c, end) in facts.values() {
            if c.created_at.get() != time || c.kind != SessionChallengeKindV1::CredentialClaim {
                continue;
            }
            let execution = c.purpose == SessionPurposeV1::Execution;
            if execution && !open {
                return Err(invalid());
            }
            match (execution, end) {
                (true, Some(at)) if *at == time => execution_instant = true,
                (false, Some(at)) if *at == time => read_instant = true,
                (true, _) => {
                    execution_retained = execution_retained.checked_add(1).ok_or_else(invalid)?
                }
                (false, _) => read_retained = read_retained.checked_add(1).ok_or_else(invalid)?,
            }
        }
        let mut events = BTreeMap::new();
        for (id, r) in &s.control_records {
            let event = match r {
                SessionControlRecordV2::Challenge { sequence, .. } => {
                    let c = s.challenges.get(id).ok_or_else(invalid)?;
                    (*sequence, c.created_at.get(), true, c.purpose, false)
                }
                SessionControlRecordV2::Control {
                    sequence,
                    request,
                    challenge_request_id,
                    ..
                } => {
                    let c = s.challenges.get(challenge_request_id).ok_or_else(invalid)?;
                    (
                        *sequence,
                        s.controls
                            .get(id)
                            .ok_or_else(invalid)?
                            .acknowledged_at
                            .get(),
                        false,
                        c.purpose,
                        closing_kind(request.kind),
                    )
                }
            };
            if event.1 == time
                && events
                    .insert(event.0, (event.2, event.3, event.4))
                    .is_some()
            {
                return Err(invalid());
            }
        }
        let mut inserted_execution = false;
        for (issue, purpose, closes) in events.values() {
            if closing_now && *closes && open {
                // Unknown-order Execute credential issues cannot be placed
                // after Finish merely to make the pending-capacity sum fit.
                if execution_instant && !execution_instant_room {
                    return Err(invalid());
                }
                pending = pending
                    .checked_add(execution_retained)
                    .ok_or_else(invalid)?;
                if pending > 8 {
                    return Err(invalid());
                }
                inserted_execution = true;
                open = false;
            }
            if *purpose == SessionPurposeV1::Execution && !open && (*issue || !*closes) {
                return Err(invalid());
            }
            pending = if *issue {
                pending.checked_add(1)
            } else {
                pending.checked_sub(1)
            }
            .ok_or_else(invalid)?;
            if pending > 8 {
                return Err(invalid());
            }
            read_instant_room |= pending < 8;
            execution_instant_room |= open && pending < 8;
        }
        if closing_now && !inserted_execution && (execution_retained > 0 || execution_instant) {
            return Err(invalid());
        }
        if !inserted_execution {
            if execution_instant && !execution_instant_room {
                return Err(invalid());
            }
            pending = pending
                .checked_add(execution_retained)
                .ok_or_else(invalid)?;
        }
        if read_instant && !read_instant_room
            || pending.checked_add(read_retained).is_none_or(|n| n > 8)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
