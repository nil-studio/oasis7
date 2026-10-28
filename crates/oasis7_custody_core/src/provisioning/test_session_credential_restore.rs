use super::*;
pub(super) fn validate(
    s: &TestSessionAuthorityV1,
    root: &SessionRootV1,
    purpose: SessionPurposeV1,
    approved: UnixMillis,
    deadline: u64,
    id: &str,
) -> Result<()> {
    if root.credential_history.len() > 4
        || root.credential.as_ref() != root.credential_history.last()
        || (purpose == SessionPurposeV1::TerminationPlan && !root.credential_history.is_empty())
    {
        return Err(invalid());
    }
    let mut request_ids = BTreeSet::new();
    for (index, c) in root.credential_history.iter().enumerate() {
        let request = &c.claim_request;
        let expected_purpose = match purpose {
            SessionPurposeV1::Execution => TestSessionPurposeV1::Execute,
            SessionPurposeV1::ReconcileRead => TestSessionPurposeV1::Read,
            SessionPurposeV1::TerminationPlan => return Err(invalid()),
        };
        request.validate().map_err(|_| invalid())?;
        let claim_digest = request.test_session_digest()?;
        if !hex64(&c.digest)
            || c.generation.0 != index as u64 + 1
            || usize::from(c.replacements) != index
            || !request_ids.insert(&c.claim_request_id)
            || request.request_id != c.claim_request_id
            || request.test_session_id != id
            || request.enrollment_id != s.enrollment.registration.enrollment_id
            || !request.purposes.contains(&expected_purpose)
            || request.replace != (index > 0)
            || request.expected_generation != (index > 0).then_some(DecimalU64(index as u64))
            || s.control_logicals.get(&c.claim_request_id) != Some(&claim_digest)
            || !s.challenges.values().any(|challenge| {
                challenge.kind == SessionChallengeKindV1::CredentialClaim
                    && challenge.logical_request_id == c.claim_request_id
                    && challenge.consumed_by.as_ref() == Some(&c.claim_request_id)
                    && challenge.body_digest == claim_digest
                    && c.claimed_at.get() >= challenge.created_at.get()
                    && c.claimed_at.get() < challenge.expires_at.get()
            })
            || c.claimed_at.get() < approved.get()
            || c.claimed_at.get() >= deadline
            || (index > 0
                && c.claimed_at.get() < root.credential_history[index - 1].claimed_at.get())
        {
            return Err(invalid());
        }
        // The same logical ID may issue both siblings, only for one full original claim.
        for other in s.roots.values().flat_map(|r| &r.credential_history) {
            if !std::ptr::eq(c, other)
                && (c.digest == other.digest
                    || (c.claim_request_id == other.claim_request_id
                        && c.claim_request != other.claim_request))
            {
                return Err(invalid());
            }
        }
        // Every purpose named by an original allocation must have that same history fact.
        for sibling in &request.purposes {
            let sibling = match sibling {
                TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
                TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
                _ => return Err(invalid()),
            };
            if !s.roots.get(&sibling).is_some_and(|r| {
                r.credential_history.iter().any(|h| {
                    h.claim_request_id == c.claim_request_id && h.claim_request == *request
                })
            }) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
