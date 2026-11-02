//! Actual trusted ingress charge, never a context/use/write permission.
use super::*;
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
pub(in crate::provisioning) fn purpose_http_occupied_except_self_v2(
    s: &TestSessionAuthorityV2,
    key: &str,
    nonce_self: bool,
) -> bool {
    super::super::execution_book_v2::prepare_paid_id_in_use_v2(s, key)
        || s.sources.contains_key(key)
        || s.source_funding.contains_key(key)
        || s.purpose_transports.contains_key(key)
        || s.control_transports.contains_key(key)
        || s.client_http_admissions.contains_key(key)
        || super::v2_credentials::credential_claim_id_in_use_v2(s, key)
        || super::v2_control::id_in_use_v2(s, key)
        || AccountProvisionApprovalState::tool_nonce_v2_internal_attempt(s, key)
        || s.sources.values().any(|v| v.source.verify_nonce == key)
        || (s.tool_nonce_ingresses.contains_key(key) && !nonce_self)
        || s.termination_state.transport_id_in_use(key)
        || s.execution_book
            .as_ref()
            .is_some_and(|v| v.transport_id_in_use(key))
}

pub(in crate::provisioning) fn execution_target_v2(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
    purpose: SessionPurposeV1,
    proof: &HttpPossessionReceipt,
) -> Result<()> {
    use crate::http_pop::{CanonicalHttpTarget, HttpPopRoute};
    // Both purposes use the actual fixed Actions POST; the closed body derives
    // Execute versus ReconcileRead. No arbitrary GET or Control target is accepted.
    if purpose == SessionPurposeV1::TerminationPlan || proof.profile() != HttpPopProfileV1::Custody
    {
        return Err(invalid());
    }
    let target = CanonicalHttpTarget::new(
        &s.enrollment.registration.public_origin,
        HttpPopRoute::ScopedClient {
            provision_id: state.draft.entry.provision_id.clone(),
            custody_account_id: state.draft.entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::Actions),
        },
    )
    .map_err(|_| invalid())?;
    if proof.target_uri() != target.uri() || proof.method() != target.method() {
        return Err(invalid());
    }
    Ok(())
}

impl AccountProvisionApprovalState {
    pub fn charge_test_session_v2_purpose_http_attempt(
        &mut self,
        session_id: &str,
        purpose: SessionPurposeV1,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        if purpose == SessionPurposeV1::TerminationPlan {
            return Err(invalid());
        }
        let request = super::v2_action_http::action_original_v2(&proof)?;
        if request.session_id != session_id || request.purpose() != purpose {
            return Err(invalid());
        }
        let s = self.test_session_v2_record(session_id)?;
        super::authenticated_receipt_v2(s, purpose, proof.possession, now)?;
        execution_target_v2(self, s, purpose, proof.possession)?;
        let ingress = proof.ingress_attempt_id;
        let signature = proof.possession.transport_attempt_digest();
        if !id(ingress) || purpose_http_occupied_except_self_v2(s, ingress, false) {
            return Err(ProvisionApprovalError::Conflict);
        }
        if let Some(old) = s.purpose_http_admissions.get(ingress) {
            return if old.class == SessionPurposeHttpAdmissionClassV1::GenericOne
                && old.purpose == purpose
                && old.hop == SessionControlHopV1::Host
                && old.signed_attempt_digest == signature
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_local(session_id, purpose, now)?;
        self.test_session_v2_transport_id_available(session_id, ingress)?;
        let cap = if purpose == SessionPurposeV1::Execution {
            400
        } else {
            300
        };
        if s.purpose_http_admissions.len() >= 700
            || s.purpose_http_admissions
                .values()
                .filter(|a| a.purpose == purpose)
                .count()
                >= cap
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        let root = saved.roots.get_mut(&purpose).ok_or_else(invalid)?;
        root.budgets.transport.charge(1).map_err(|_| invalid())?;
        let exhausted = root.budgets.transport.remaining().map_err(|_| invalid())? == 0;
        saved.purpose_http_admissions.insert(
            ingress.into(),
            SessionPurposeHttpAdmissionV1 {
                class: SessionPurposeHttpAdmissionClassV1::GenericOne,
                signed_attempt_digest: signature,
                purpose,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        append_session_event_v2(
            saved,
            SessionEventKindV2::PurposeHttpCharged {
                ingress_attempt_id: ingress.into(),
            },
            now,
        )?;
        if exhausted && saved.closing.is_none() {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
pub(in crate::provisioning) fn validate_purpose_http_v2(s: &TestSessionAuthorityV2) -> Result<()> {
    if s.purpose_http_admissions.len() > 700 {
        return Err(invalid());
    }
    for (key, paid) in &s.purpose_http_admissions {
        let cap = match paid.purpose {
            SessionPurposeV1::Execution => 400,
            SessionPurposeV1::ReconcileRead => 300,
            _ => return Err(invalid()),
        };
        let root = s.roots.get(&paid.purpose).ok_or_else(invalid)?;
        if !id(key)
            || !hex64(&paid.signed_attempt_digest)
            || (!matches!(
                paid.class,
                SessionPurposeHttpAdmissionClassV1::GenericOne
                    | SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. }
                    | SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 { .. }
            ))
            || paid.hop != SessionControlHopV1::Host
            || paid.charged_at.get() < s.approved_at.ok_or_else(invalid)?.get()
            || paid.charged_at.get() >= root.deadline.get()
            || s.purpose_http_admissions
                .values()
                .filter(|a| a.purpose == paid.purpose)
                .count()
                > cap
            || purpose_http_occupied_except_self_v2(
                s,
                key,
                matches!(
                    paid.class,
                    SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. }
                        | SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 { .. }
                ),
            )
        {
            return Err(invalid());
        }
    }
    Ok(())
}
