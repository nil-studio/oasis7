//! Closed metadata-only nonce issuance. No source, World, or action admission.
use super::*;
use crate::http_pop::{CanonicalHttpTarget, HttpPopRoute};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

const PREFIX: &str = "tool-nonce-internal-";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionToolNonceIngressV1 {
    pub request: SessionChallengeRequestV1,
    pub signed_attempt_digest: String,
    pub metadata_body_digest: String,
    pub credential_generation: DecimalU64,
    pub metadata_uri: String,
    pub action_uri: String,
    pub owner_attempt: String,
    pub account_attempt: String,
    pub charged_at: UnixMillis,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionToolNonceLinkV1 {
    pub first_ingress: String,
    pub request: SessionChallengeRequestV1,
    pub credential_generation: DecimalU64,
    pub signed_attempt_digest: String,
    pub metadata_body_digest: String,
    pub metadata_uri: String,
    pub action_uri: String,
    pub nonce: String,
    pub created_at: UnixMillis,
    pub expires_at: UnixMillis,
}
fn request_shape(q: &SessionChallengeRequestV1, session_id: &str) -> Result<()> {
    if q.api_version != 1
        || q.session_id != session_id
        || q.kind != SessionChallengeKindV1::Tool
        || q.purpose == SessionPurposeV1::TerminationPlan
        || !id(&q.request_id)
        || !id(&q.logical_request_id)
        || !hex64(&q.operation_request_digest)
    {
        return Err(invalid());
    }
    Ok(())
}
/// Only the registered Native profile can enter this new metadata lane.
/// This compares persisted enrollment and the independently signed approval;
/// it grants no Current, source or business authority.
pub(super) fn native_registration(s: &TestSessionAuthorityV1) -> Result<()> {
    let descriptor = &s.enrollment.registration.request.descriptor;
    let (_, pair, _, _, _) = admission::pairing(&s.input.request.intent)?;
    let CustodyIntentV1::TestSession { client_binding, .. } = &s.input.request.intent else {
        return Err(invalid());
    };
    if descriptor.transport_profile != HostTransportProfileV1::NativeBrokerCustodyV1
        || pair.connector_public_key.is_some()
        || pair.client_public_key != descriptor.client_public_key
        || !matches!(client_binding,
            crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key }
                if client_key == &pair.client_public_key)
    {
        return Err(invalid());
    }
    Ok(())
}
fn targets(draft: &AccountDraftV1, origin: &str) -> Result<(String, String)> {
    let target = |route| {
        CanonicalHttpTarget::new(
            origin,
            HttpPopRoute::ScopedClient {
                provision_id: draft.entry.provision_id.clone(),
                custody_account_id: draft.entry.custody_account_id.clone(),
                route: Box::new(route),
            },
        )
        .map(|t| t.uri().to_owned())
        .map_err(|_| invalid())
    };
    Ok((
        target(HttpPopRoute::ProofChallenges)?,
        target(HttpPopRoute::Actions)?,
    ))
}
fn attempts(session: &str, ingress: &str) -> Result<(String, String)> {
    let stem = digest(&("oasis7.custody.tool-nonce-internal.v1", session, ingress))?;
    Ok((
        format!("{PREFIX}owner-{stem}"),
        format!("{PREFIX}account-{stem}"),
    ))
}
impl AccountProvisionApprovalState {
    fn tool_nonce_input(
        &self,
        proof: &SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<SessionToolNonceIngressV1> {
        let q: SessionChallengeRequestV1 = lifecycle::original(proof)?;
        request_shape(&q, proof.possession.binding_id())?;
        // Exact paid callbacks authenticate the current key/token and absolute
        // deadline, but do not acquire a new write/lease permission.
        let s = self.test_sessions.get(&q.session_id).ok_or_else(invalid)?;
        native_registration(s)?;
        let root = s.roots.get(&q.purpose).ok_or_else(invalid)?;
        if self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || s.approved_at.is_none()
            || root.revoked
            || now.get() >= root.deadline.get()
            || s.input.request.context.security_version != self.provision.security_epoch()
        {
            return Err(invalid());
        }
        roots::authenticated_receipt(s, q.purpose, proof.possession, now)?;
        let generation = root.credential.as_ref().ok_or_else(invalid)?.generation;
        let (metadata_uri, action_uri) =
            targets(&self.draft, &s.enrollment.registration.public_origin)?;
        if proof.possession.profile() != HttpPopProfileV1::CustodyChallenge
            || proof.possession.method() != "POST"
            || proof.possession.target_uri() != metadata_uri
            || proof.possession.request_id() != q.request_id
            || proof.possession.nonce().is_some()
            || proof
                .possession
                .created()
                .checked_millis()
                .map_err(|_| invalid())?
                .get()
                > now.get()
            || proof
                .possession
                .expires()
                .checked_millis()
                .map_err(|_| invalid())?
                .get()
                > root.deadline.get()
            || !id(proof.ingress_attempt_id)
        {
            return Err(invalid());
        }
        let (owner_attempt, account_attempt) = attempts(&q.session_id, proof.ingress_attempt_id)?;
        Ok(SessionToolNonceIngressV1 {
            request: q,
            signed_attempt_digest: proof.possession.transport_attempt_digest(),
            metadata_body_digest: proof
                .possession
                .body_digest()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            credential_generation: generation,
            metadata_uri,
            action_uri,
            owner_attempt,
            account_attempt,
            charged_at: now,
        })
    }
    /// Fixed Host + Owner named + Account costs, committed independently of issuance.
    pub fn charge_test_session_tool_nonce_http_attempt(
        &mut self,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<SessionToolNonceIngressV1> {
        let mut record = self.tool_nonce_input(&proof, now)?;
        let ingress = proof.ingress_attempt_id;
        let s = self
            .test_sessions
            .get(&record.request.session_id)
            .ok_or_else(invalid)?;
        self.validate_session_tool_nonce_persistence(s)?;
        if let Some(old) = s.tool_nonce_ingresses.get(ingress) {
            if old.charged_at.get() > now.get() {
                return Err(invalid());
            }
            record.charged_at = old.charged_at;
            return if old == &record {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_local(&record.request.session_id, record.request.purpose, now)?;
        for attempt in [
            ingress,
            record.owner_attempt.as_str(),
            record.account_attempt.as_str(),
        ] {
            if Self::session_transport_attempt_occupied(s, attempt) {
                return Err(ProvisionApprovalError::Conflict);
            }
        }
        if s.purpose_http_admissions.len() >= 700 {
            return Err(invalid());
        }
        let mut next = self.clone();
        let session = next
            .test_sessions
            .get_mut(&record.request.session_id)
            .ok_or_else(invalid)?;
        session
            .roots
            .get_mut(&record.request.purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(3)
            .map_err(|_| invalid())?;
        session.purpose_http_admissions.insert(
            ingress.into(),
            SessionPurposeHttpAdmissionV1 {
                class: SessionPurposeHttpAdmissionClassV1::ToolNonceThree {
                    owner_attempt: record.owner_attempt.clone(),
                    account_attempt: record.account_attempt.clone(),
                },
                signed_attempt_digest: record.signed_attempt_digest.clone(),
                purpose: record.request.purpose,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        session
            .purpose_transports
            .insert(record.owner_attempt.clone(), record.request.purpose);
        session
            .purpose_transports
            .insert(record.account_attempt.clone(), record.request.purpose);
        session
            .tool_nonce_ingresses
            .insert(ingress.into(), record.clone());
        if session.closing.is_none()
            && session
                .roots
                .get(&record.request.purpose)
                .ok_or_else(invalid)?
                .budgets
                .transport
                .remaining()
                .map_err(|_| invalid())?
                == 0
        {
            session.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    /// Caller commits the fee transaction first. Failure here never refunds it.
    pub fn issue_test_session_tool_nonce(
        &mut self,
        proof: SessionProofRequest<'_>,
        service_nonce: &str,
        now: UnixMillis,
    ) -> Result<SessionChallengeV1> {
        let mut expected = self.tool_nonce_input(&proof, now)?;
        let open =
            self.test_session_local(&expected.request.session_id, expected.request.purpose, now)?;
        if open
            .roots
            .get(&expected.request.purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
        {
            return Err(invalid());
        }
        let s = self
            .test_sessions
            .get(&expected.request.session_id)
            .ok_or_else(invalid)?;
        let paid = s
            .tool_nonce_ingresses
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if paid.charged_at.get() > now.get() {
            return Err(invalid());
        }
        expected.charged_at = paid.charged_at;
        if &expected != paid {
            return Err(ProvisionApprovalError::Conflict);
        }
        let q = &expected.request;
        if let Some(old) = s.challenges.get(&q.request_id) {
            let link = s.tool_nonce_links.get(&q.request_id).ok_or_else(invalid)?;
            return if link.request == *q
                && link.metadata_body_digest == expected.metadata_body_digest
                && link.credential_generation == expected.credential_generation
                && old.kind == SessionChallengeKindV1::ToolMetadata
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(service_nonce)
            .map_err(|_| invalid())?;
        if bytes.len() != 32
            || URL_SAFE_NO_PAD.encode(bytes) != service_nonce
            || s.challenges.len() >= 1024
            || s.challenges.values().any(|c| c.nonce == service_nonce)
            || s.challenges
                .values()
                .filter(|c| c.consumed_by.is_none() && now.get() < c.expires_at.get())
                .count()
                >= 8
            || s.challenges
                .values()
                .filter(|c| c.created_at.get() / 1000 == now.get() / 1000)
                .count()
                >= 2
        {
            return Err(invalid());
        }
        let root = s.roots.get(&q.purpose).ok_or_else(invalid)?;
        let expires_at = UnixMillis::new(
            now.get()
                .checked_add(30_000)
                .ok_or_else(invalid)?
                .min(root.deadline.get()),
        );
        let challenge = SessionChallengeV1 {
            request_id: q.request_id.clone(),
            nonce: service_nonce.into(),
            purpose: q.purpose,
            kind: SessionChallengeKindV1::ToolMetadata,
            logical_request_id: q.logical_request_id.clone(),
            credential_generation: expected.credential_generation,
            body_digest: q.operation_request_digest.clone(),
            created_at: now,
            expires_at,
            consumed_by: None,
        };
        let link = SessionToolNonceLinkV1 {
            first_ingress: proof.ingress_attempt_id.into(),
            request: q.clone(),
            credential_generation: expected.credential_generation,
            signed_attempt_digest: expected.signed_attempt_digest,
            metadata_body_digest: expected.metadata_body_digest,
            metadata_uri: expected.metadata_uri,
            action_uri: expected.action_uri,
            nonce: service_nonce.into(),
            created_at: now,
            expires_at,
        };
        let mut next = self.clone();
        let session = next
            .test_sessions
            .get_mut(&q.session_id)
            .ok_or_else(invalid)?;
        session
            .challenges
            .insert(q.request_id.clone(), challenge.clone());
        session.tool_nonce_links.insert(q.request_id.clone(), link);
        next.validate_persistence()?;
        *self = next;
        Ok(challenge)
    }
}
#[path = "test_session_tool_nonce_restore.rs"]
mod restore;
