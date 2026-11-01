//! Historical invariants only. Every new grant/use still rebuilds live checks.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn unrelated_id(s: &TestSessionAuthorityV2, value: &str) -> bool {
    value == s.input.operation_id
        || s.sources.contains_key(value)
        || s.source_funding.contains_key(value)
        || s.sources
            .values()
            .any(|source| source.source.verify_nonce == value)
        || s.purpose_transports.contains_key(value)
        || s.purpose_http_admissions.contains_key(value)
        || s.control_transports.contains_key(value)
        || s.client_http_admissions.contains_key(value)
        || s.tool_nonce_ingresses.contains_key(value)
        || s.termination_state.transport_id_in_use(value)
        || s.execution_book
            .as_ref()
            .is_some_and(|book| book.transport_id_in_use(value))
}
pub(in crate::provisioning) fn validate(
    state: &AccountProvisionApprovalState,
    id_value: &str,
    s: &TestSessionAuthorityV2,
) -> Result<()> {
    if s.challenges.len() > 1024 || s.credential_claims.len() > 8 {
        return Err(invalid());
    }
    if !s.challenges.is_empty() || !s.credential_claims.is_empty() {
        super::v2_credential_http::native_registration(s)?;
    }
    let approved = s.approved_at;
    // Distinct categories cannot be collapsed by balancing ledger counters.
    // Multiple challenges may intentionally refer to the same logical request.
    let request_ids: BTreeSet<_> = s.challenges.keys().map(String::as_str).collect();
    let logical_ids: BTreeSet<_> = s
        .challenges
        .values()
        .map(|c| c.logical_request_id.as_str())
        .collect();
    let challenge_nonces: BTreeSet<_> = s.challenges.values().map(|c| c.nonce.as_str()).collect();
    if !request_ids.is_disjoint(&logical_ids)
        || !request_ids.is_disjoint(&challenge_nonces)
        || !logical_ids.is_disjoint(&challenge_nonces)
    {
        return Err(invalid());
    }
    let mut nonces = BTreeSet::new();
    let mut per_second = BTreeMap::new();
    for (key, challenge) in &s.challenges {
        let start = challenge.created_at.get();
        let root = s.roots.get(&challenge.purpose).ok_or_else(invalid)?;
        let nonce = URL_SAFE_NO_PAD
            .decode(&challenge.nonce)
            .map_err(|_| invalid())?;
        if key != &challenge.request_id
            || !id(key)
            || !id(&challenge.logical_request_id)
            || key == &challenge.logical_request_id
            || key == &challenge.nonce
            || challenge.logical_request_id == challenge.nonce
            || unrelated_id(s, key)
            || unrelated_id(s, &challenge.logical_request_id)
            || unrelated_id(s, &challenge.nonce)
            || nonce.len() != 32
            || URL_SAFE_NO_PAD.encode(nonce) != challenge.nonce
            || !nonces.insert(&challenge.nonce)
            || !hex64(&challenge.body_digest)
            || !matches!(
                challenge.kind,
                SessionChallengeKindV1::CredentialClaim
                    | SessionChallengeKindV1::Control(_)
                    | SessionChallengeKindV1::ToolMetadata
            )
            || challenge.purpose == SessionPurposeV1::TerminationPlan
            || approved.is_none_or(|t| start < t.get())
            || start >= root.deadline.get()
            || challenge.expires_at.get()
                != start
                    .checked_add(30_000)
                    .ok_or_else(invalid)?
                    .min(root.deadline.get())
            || (challenge.kind != SessionChallengeKindV1::ToolMetadata
                && s.control_logicals.get(&challenge.logical_request_id)
                    != Some(&challenge.body_digest))
            || challenge.credential_generation.0
                > root.credential.as_ref().map_or(0, |c| c.generation.0)
        {
            return Err(invalid());
        }
        *per_second.entry(start / 1000).or_insert(0usize) += 1;
        if per_second[&(start / 1000)] > 2 {
            return Err(invalid());
        }
        if challenge.kind == SessionChallengeKindV1::CredentialClaim {
            if let Some(consumed) = &challenge.consumed_by {
                let claim = s.credential_claims.get(consumed).ok_or_else(invalid)?;
                if consumed != &challenge.logical_request_id
                    || claim.challenge_request_id != *key
                    || claim.request.test_session_digest()? != challenge.body_digest
                {
                    return Err(invalid());
                }
            }
        }
    }
    let mut claim_sources = BTreeSet::new();
    let mut all_digests = BTreeSet::new();
    for (key, claim) in &s.credential_claims {
        claim.request.validate().map_err(|_| invalid())?;
        if !id(key)
            || key != &claim.request.request_id
            || claim.api_version != 2
            || claim.approved_input_digest != s.input_digest()?
            || claim.request.test_session_id != id_value
            || claim.request.enrollment_id != s.enrollment.registration.enrollment_id
            || claim.allocations.len() != claim.request.purposes.len()
            || claim.private_sid_generation.0 == 0
            || claim.private_sid_generation != s.session.private_sid_generation
            || approved.is_none_or(|t| claim.claimed_at.get() < t.get())
            || s.control_logicals.get(key) != Some(&claim.request.test_session_digest()?)
            || !claim_sources.insert(&claim.source_operation_id)
            || s.source_funding.get(&claim.source_operation_id)
                != Some(&SessionSourceFundingV1::Control)
        {
            return Err(invalid());
        }
        let paid = s
            .client_http_admissions
            .get(&claim.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if !hex64(&claim.signed_attempt_digest)
            || paid.signed_attempt_digest != claim.signed_attempt_digest
            || paid.hop != SessionControlHopV1::Host
            || paid.charged_at.get() > claim.claimed_at.get()
            || s.control_transports.get(&claim.ingress_attempt_id)
                != Some(&SessionControlHopV1::Host)
        {
            return Err(invalid());
        }
        let source = s
            .sources
            .get(&claim.source_operation_id)
            .ok_or_else(invalid)?;
        let verification = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if source.api_version != 2
            || source.approved_input_digest != s.input_digest()?
            || source.source.claimed_at.is_none()
            || source
                .source
                .verified_at
                .is_none_or(|t| t.0 > claim.claimed_at.get())
            || claim.source_ticket != 4
            || claim.source_ticket
                > *s.source_oracle_reads
                    .get(&claim.source_operation_id)
                    .ok_or_else(invalid)?
            || verification.observed_at.0 > claim.claimed_at.get()
            || claim.claimed_at.get() - verification.observed_at.0 > 5000
            || verification.verify_nonce != source.source.verify_nonce
            || verification.proof_digest != s.input.approval.digest().map_err(|_| invalid())?
            || verification.purpose != crate::approval::VerificationPurposeV1::InspectOrigin
            || verification.origin_state != crate::approval::OriginStateV1::Active
            || claim.claimed_at.get() >= verification.source_session_expires_at.0
            || claim.claimed_at.get() >= verification.source_session_idle_expires_at.0
            || claim.claimed_at.get() >= source.source.expires_at.0
        {
            return Err(invalid());
        }
        if s.control_transports
            .get(&super::v2_credential_source::lifetime_attempt_v2(
                s,
                &claim.source_operation_id,
            )?)
            != Some(&SessionControlHopV1::Identity)
        {
            return Err(invalid());
        }
        let challenge = s
            .challenges
            .get(&claim.challenge_request_id)
            .ok_or_else(invalid)?;
        if challenge.consumed_by.as_ref() != Some(key)
            || challenge.logical_request_id != *key
            || claim.claimed_at.get() < challenge.created_at.get()
            || claim.claimed_at.get() >= challenge.expires_at.get()
        {
            return Err(invalid());
        }
        claim.lifetime.validate().map_err(|_| invalid())?;
        claim
            .lifetime
            .lifetime
            .validate_for(
                &s.native_lifetime
                    .as_ref()
                    .ok_or_else(invalid)?
                    .lifetime
                    .status
                    .receipt,
                &state.test_session_v2_lifetime_nonce(id_value, &claim.source_operation_id)?,
                s.session.native_binding_generation,
                s.input.request.context.service_epoch,
                claim.claimed_at.get(),
            )
            .map_err(|_| invalid())?;
        if claim.lifetime.lifetime.status.session_ref != s.session.session_ref {
            return Err(invalid());
        }
        let mut purposes = BTreeSet::new();
        for allocation in &claim.allocations {
            let root = s.roots.get(&allocation.purpose).ok_or_else(invalid)?;
            if allocation.purpose == SessionPurposeV1::TerminationPlan
                || !purposes.insert(allocation.purpose)
                || !hex64(&allocation.digest)
                || !all_digests.insert(&allocation.digest)
                || !claim.request.purposes.iter().any(|p| {
                    super::v2_credentials::credential_purpose_v2(p).ok() == Some(allocation.purpose)
                })
                || root.approved_input_digest != s.input_digest()?
                || !root.credential_history.iter().any(|c| {
                    c.claim_request_id == *key
                        && c.claim_request == claim.request
                        && c.digest == allocation.digest
                        && c.generation == allocation.generation
                        && c.claimed_at == claim.claimed_at
                })
            {
                return Err(invalid());
            }
            claim
                .lifetime
                .lifetime
                .covers(root.deadline)
                .map_err(|_| invalid())?;
        }
    }
    for (purpose, root) in &s.roots {
        if root.credential_history.len() > 4
            || root.credential.as_ref() != root.credential_history.last()
            || (*purpose == SessionPurposeV1::TerminationPlan
                && !root.credential_history.is_empty())
        {
            return Err(invalid());
        }
        let mut ids = BTreeSet::new();
        for (index, credential) in root.credential_history.iter().enumerate() {
            let claim = s
                .credential_claims
                .get(&credential.claim_request_id)
                .ok_or_else(invalid)?;
            if !ids.insert(&credential.claim_request_id)
                || credential.generation.0 != index as u64 + 1
                || usize::from(credential.replacements) != index
                || credential.claim_request != claim.request
                || credential.claimed_at != claim.claimed_at
                || credential.claim_request.replace != (index > 0)
                || credential.claim_request.expected_generation
                    != (index > 0).then_some(DecimalU64(index as u64))
                || credential.claimed_at.get() >= root.deadline.get()
                || (index > 0
                    && credential.claimed_at.get()
                        < root.credential_history[index - 1].claimed_at.get())
                || !claim.allocations.iter().any(|a| {
                    a.purpose == *purpose
                        && a.digest == credential.digest
                        && a.generation == credential.generation
                })
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
