//! Trusted actual-ingress accounting, separate from internal outbound attempts.
use super::*;
impl AccountProvisionApprovalState {
    pub fn charge_test_session_purpose_http_attempt(
        &mut self,
        id: &str,
        purpose: SessionPurposeV1,
        admission: SessionHttpAdmission<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        if purpose == SessionPurposeV1::TerminationPlan {
            return Err(invalid());
        }
        let s = self.test_session_local(id, purpose, now)?;
        roots::authenticated_receipt(s, purpose, admission.possession, now)?;
        let ingress = admission.ingress_attempt_id;
        if !id_nonempty(ingress)
            || s.control_transports.contains_key(ingress)
            || s.client_http_admissions.contains_key(ingress)
            || s.purpose_transports.contains_key(ingress)
            || s.sources.contains_key(ingress)
            || s.source_funding.contains_key(ingress)
            || s.actions
                .values()
                .any(|a| a.transport_attempts.iter().any(|id| id == ingress))
        {
            return Err(invalid());
        }
        let signed_attempt_digest = admission.possession.transport_attempt_digest();
        if let Some(old) = s.purpose_http_admissions.get(ingress) {
            return if old.signed_attempt_digest == signed_attempt_digest
                && old.purpose == purpose
                && old.hop == SessionControlHopV1::Host
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let limit = match purpose {
            SessionPurposeV1::Execution => 400,
            SessionPurposeV1::ReconcileRead => 300,
            SessionPurposeV1::TerminationPlan => return Err(invalid()),
        };
        if s.purpose_http_admissions.len() >= 700
            || s.purpose_http_admissions
                .values()
                .filter(|a| a.purpose == purpose)
                .count()
                >= limit
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        let session = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        session
            .roots
            .get_mut(&purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        session.purpose_http_admissions.insert(
            ingress.into(),
            SessionPurposeHttpAdmissionV1 {
                signed_attempt_digest,
                purpose,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn validate_session_purpose_http_persistence(
        &self,
        s: &TestSessionAuthorityV1,
    ) -> Result<()> {
        if s.purpose_http_admissions.len() > 700 {
            return Err(invalid());
        }
        for purpose in [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead] {
            let cap = if purpose == SessionPurposeV1::Execution {
                400
            } else {
                300
            };
            if s.purpose_http_admissions
                .values()
                .filter(|a| a.purpose == purpose)
                .count()
                > cap
            {
                return Err(invalid());
            }
        }
        for (id, a) in &s.purpose_http_admissions {
            let root = s.roots.get(&a.purpose).ok_or_else(invalid)?;
            if !id_nonempty(id)
                || !hex64(&a.signed_attempt_digest)
                || a.purpose == SessionPurposeV1::TerminationPlan
                || a.hop != SessionControlHopV1::Host
                || a.charged_at.get() < s.approved_at.ok_or_else(invalid)?.get()
                || a.charged_at.get() >= root.deadline.get()
                || s.control_transports.contains_key(id)
                || s.client_http_admissions.contains_key(id)
                || s.purpose_transports.contains_key(id)
                || s.sources.contains_key(id)
                || s.source_funding.contains_key(id)
                || s.actions
                    .values()
                    .any(|action| action.transport_attempts.contains(id))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

// Same bounded opaque-attempt grammar as the existing session transport lanes.
fn id_nonempty(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
