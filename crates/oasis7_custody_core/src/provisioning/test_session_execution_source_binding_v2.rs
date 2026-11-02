//! Paid-original provenance is carried by the actual Native-signed Source nonce.
//! This producer does not grant an SDK stage, refresh a Source, or move a deadline.
use super::super::test_session::{action_original_v2, authenticated_receipt_v2};
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn nonce(
    s: &TestSessionAuthorityV2,
    source_id: &str,
    ingress: &str,
    signed_attempt_digest: &str,
    action_digest: &str,
    nonce_salt: &[u8; 32],
) -> Result<String> {
    digest(&(
        "oasis7.custody.execution-paid-source-nonce.v2",
        &s.input.operation_id,
        s.input_digest()?,
        source_id,
        ingress,
        signed_attempt_digest,
        action_digest,
        nonce_salt,
    ))
}
pub(super) fn check(
    s: &TestSessionAuthorityV2,
    source_id: &str,
    ingress: &str,
    signed_attempt_digest: &str,
    action_digest: &str,
) -> Result<()> {
    let Some(SessionSourceFundingV1::ExecutionFactoryV2 { nonce_salt }) =
        s.source_funding.get(source_id)
    else {
        return Err(invalid());
    };
    let expected = nonce(
        s,
        source_id,
        ingress,
        signed_attempt_digest,
        action_digest,
        nonce_salt,
    )?;
    let source = &s.sources.get(source_id).ok_or_else(invalid)?.source;
    if source.verify_nonce != expected
        || source
            .verification
            .as_ref()
            .is_none_or(|signed| signed.verification.verify_nonce != expected)
    {
        return Err(invalid());
    }
    Ok(())
}
impl AccountProvisionApprovalState {
    /// The Rust service supplies independent random salt. The existing Native
    /// InspectOrigin request signs the resulting exact nonce in its normal
    /// verification response. No caller-provided digest is accepted instead.
    pub fn reserve_test_session_v2_execution_source(
        &mut self,
        session_id: &str,
        source_id: &str,
        nonce_salt: [u8; 32],
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionSourceV2> {
        let q = action_original_v2(&proof)?;
        let s = self.test_session_v2_local(session_id, SessionPurposeV1::Execution, now)?;
        authenticated_receipt_v2(s, SessionPurposeV1::Execution, proof.possession, now)?;
        let (plan, _) = s.header()?;
        let [approved] = plan.actions.as_slice() else {
            return Err(invalid());
        };
        let SessionToolActionV1::Execute(action) = &q.action else {
            return Err(invalid());
        };
        let paid = s
            .purpose_http_admissions
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if q.session_id != session_id
            || action != approved
            || !plan.selection.is_exact_move0()
            || plan.business_limit.0 != 0
            || !plan.allowed_resource_refs.is_empty()
            || s.execution_book.is_some()
            || paid.class != SessionPurposeHttpAdmissionClassV1::GenericOne
            || paid.purpose != SessionPurposeV1::Execution
            || paid.hop != SessionControlHopV1::Host
            || paid.signed_attempt_digest != proof.possession.transport_attempt_digest()
            || paid.charged_at.get() > now.get()
        {
            return Err(invalid());
        }
        let challenge = s
            .challenges
            .values()
            .find(|c| Some(c.nonce.as_str()) == proof.possession.nonce())
            .ok_or_else(invalid)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        if challenge.kind != SessionChallengeKindV1::ToolMetadata
            || challenge.logical_request_id != q.request_id
            || challenge.purpose != SessionPurposeV1::Execution
            || challenge.body_digest != q.digest()?
            || challenge.consumed_by.is_some()
            || challenge.credential_generation
                != root.credential.as_ref().ok_or_else(invalid)?.generation
            || now.get() >= challenge.expires_at.get()
        {
            return Err(invalid());
        }
        let verify_nonce = nonce(
            s,
            source_id,
            proof.ingress_attempt_id,
            &proof.possession.transport_attempt_digest(),
            &q.digest()?,
            &nonce_salt,
        )?;
        self.reserve_test_session_v2_source_inner(
            session_id,
            SessionSourceReservation {
                operation_id: source_id,
                nonce: &verify_nonce,
                funding: SessionSourceFundingV1::ExecutionFactoryV2 { nonce_salt },
            },
            now,
        )
    }
}
