//! Local scheduling metadata only; no permission, remote I/O or cleanup completion.
use super::*;
impl AccountProvisionApprovalState {
    /// Returns an already-due timestamp unchanged if expire has not yet run.
    /// After local expire, all closing sessions are excluded to prevent past-timer loops.
    pub fn next_test_session_expiry_at(&self, now: UnixMillis) -> Result<Option<UnixMillis>> {
        let deadlines = self
            .test_sessions
            .values()
            .filter(|s| s.closing.is_none())
            .map(|s| {
                let (_, _, write, _, _) = admission::pairing(&s.input.request.intent)?;
                if session_control_exhausted(s) {
                    return Ok(now);
                }
                let deadline = if s.approved_at.is_some() {
                    s.roots
                        .get(&SessionPurposeV1::Execution)
                        .ok_or_else(invalid)?
                        .deadline
                        .get()
                        .min(s.lease_until.ok_or_else(invalid)?.get())
                        .min(write)
                } else if s.consumed_at.is_none() {
                    write.min(s.input.approval.proof.expires_at.0)
                } else {
                    write
                };
                Ok(UnixMillis::new(deadline))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(earliest(deadlines))
    }
}
fn earliest(deadlines: impl IntoIterator<Item = UnixMillis>) -> Option<UnixMillis> {
    deadlines.into_iter().min_by_key(|v| v.get())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduling_uses_minimum_without_renewing_past_due_or_multiple_session_deadlines() {
        assert_eq!(earliest([]), None);
        assert_eq!(
            earliest([
                UnixMillis::new(50),
                UnixMillis::new(10),
                UnixMillis::new(30)
            ]),
            Some(UnixMillis::new(10))
        );
        assert_eq!(
            earliest([
                UnixMillis::new(50),
                UnixMillis::new(10),
                UnixMillis::new(30)
            ]),
            earliest([
                UnixMillis::new(30),
                UnixMillis::new(50),
                UnixMillis::new(10)
            ])
        );
    }
}
