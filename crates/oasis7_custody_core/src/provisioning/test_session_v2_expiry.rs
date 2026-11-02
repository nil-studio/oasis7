//! Local V2 expiry metadata; this never dispatches cleanup or claims Closed.
use super::*;
impl AccountProvisionApprovalState {
    fn test_session_v2_expiry(s: &TestSessionAuthorityV2) -> Result<UnixMillis> {
        let (plan, _) = s.header()?;
        let write = plan.write_deadline.0;
        let expiry = if s.approved_at.is_some() {
            write.min(s.lease_until.ok_or_else(invalid)?.get())
        } else if s.consumed_at.is_none() {
            write.min(s.input.approval.proof.expires_at.0)
        } else {
            write
        };
        Ok(UnixMillis::new(expiry))
    }
    pub fn next_test_session_v2_expiry_at(&self) -> Result<Option<UnixMillis>> {
        self.test_sessions_v2
            .values()
            .filter(|s| s.closing.is_none())
            .map(Self::test_session_v2_expiry)
            .collect::<Result<Vec<_>>>()
            .map(|v| v.into_iter().min_by_key(|t| t.get()))
    }
    pub fn expire_test_sessions_v2(&mut self, now: UnixMillis) -> Result<Vec<String>> {
        let mut next = self.clone();
        let mut closed = Vec::new();
        for (id, session) in &mut next.test_sessions_v2 {
            if session.closing.is_none()
                && now.get() >= Self::test_session_v2_expiry(session)?.get()
            {
                let (plan, _) = session.header()?;
                let reason = if session.approved_at.is_some() && now.get() < plan.write_deadline.0 {
                    TestSessionClosingReasonV1::LeaseExpired
                } else {
                    TestSessionClosingReasonV1::Deadline
                };
                session.closing = Some((reason, now));
                append_session_event_v2(session, SessionEventKindV2::Expired { reason }, now)?;
                closed.push(id.clone());
            }
        }
        next.validate_persistence()?;
        *self = next;
        Ok(closed)
    }
}
