use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionControlRequestV1 {
    pub api_version: u8,
    pub session_id: String,
    pub request_id: String,
    pub kind: SessionControlKindV1,
}
/// Private transport supplies exact octets alongside its verified receipt.
pub struct SessionProofRequest<'a> {
    pub ingress_attempt_id: &'a str,
    pub original_bytes: &'a [u8],
    pub possession: &'a HttpPossessionReceipt,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionActionAdmission {
    pub original: SessionActionOriginalV1,
    pub newly_admitted: bool,
}
pub(super) fn original<T: serde::de::DeserializeOwned>(
    proof: &SessionProofRequest<'_>,
) -> Result<T> {
    if proof.original_bytes.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
        || proof.possession.body_digest().as_slice()
            != Sha256::digest(proof.original_bytes).as_slice()
    {
        return Err(invalid());
    }
    serde_json::from_slice(proof.original_bytes).map_err(|_| invalid())
}
impl AccountProvisionApprovalState {
    pub fn issue_test_session_challenge(
        &mut self,
        proof: SessionProofRequest<'_>,
        nonce: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionChallengeV1> {
        let request: SessionChallengeRequestV1 = original(&proof)?;
        if request.api_version != 1
            || request.purpose == SessionPurposeV1::TerminationPlan
            || !id(&request.request_id)
            || !hex64(&request.operation_request_digest)
            || proof.possession.request_id() != request.request_id
            || !id(&request.logical_request_id)
            || !matches!(
                proof.possession.profile(),
                HttpPopProfileV1::CustodyChallenge
                    | HttpPopProfileV1::McpChallenge
                    | HttpPopProfileV1::ClaimChallenge
            )
        {
            return Err(invalid());
        }
        self.require_session_source_funding(
            &request.session_id,
            current.source_operation_id,
            if request.kind == SessionChallengeKindV1::Tool {
                request.purpose.source_funding()
            } else {
                SessionSourceFundingV1::Control
            },
        )?;
        self.require_session_http_paid(
            &request.session_id,
            proof.possession,
            proof.ingress_attempt_id,
            if request.kind == SessionChallengeKindV1::Tool {
                Some(request.purpose)
            } else {
                None
            },
        )?;
        let s = self.test_session_local(&request.session_id, request.purpose, now)?;
        if request.kind == SessionChallengeKindV1::CredentialClaim {
            if proof.possession.profile() != HttpPopProfileV1::ClaimChallenge {
                return Err(invalid());
            }
            roots::public_receipt(s, proof.possession, now)?;
        } else {
            if proof.possession.profile() == HttpPopProfileV1::ClaimChallenge {
                return Err(invalid());
            }
            roots::authenticated_receipt(s, request.purpose, proof.possession, now)?;
        }
        // PoP has already succeeded before any challenge slot, rate or budget mutation.
        verifier.verify(&self.test_session_live_checks(
            &request.session_id,
            request.purpose,
            current,
            now,
        )?)?;
        if let Some(old) = s.challenges.get(&request.request_id) {
            return if old.purpose == request.purpose
                && old.kind == request.kind
                && old.logical_request_id == request.logical_request_id
                && old.body_digest == request.operation_request_digest
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let bytes = URL_SAFE_NO_PAD.decode(nonce).map_err(|_| invalid())?;
        if bytes.len() != 32
            || URL_SAFE_NO_PAD.encode(bytes) != nonce
            || s.challenges.values().any(|v| v.nonce == nonce)
            || s.challenges.len() >= 1024
            || s.challenges
                .values()
                .filter(|v| v.consumed_by.is_none() && now.get() < v.expires_at.get())
                .count()
                >= 8
            || s.challenges
                .values()
                .filter(|v| v.created_at.get() / 1000 == now.get() / 1000)
                .count()
                >= 2
        {
            return Err(invalid());
        }
        let root = s.roots.get(&request.purpose).ok_or_else(invalid)?;
        let generation = if request.kind == SessionChallengeKindV1::CredentialClaim {
            root.credential
                .as_ref()
                .map_or(DecimalU64(0), |c| c.generation)
        } else {
            root.credential.as_ref().ok_or_else(invalid)?.generation
        };
        let challenge = SessionChallengeV1 {
            request_id: request.request_id,
            purpose: request.purpose,
            kind: request.kind,
            logical_request_id: request.logical_request_id.clone(),
            body_digest: request.operation_request_digest.clone(),
            nonce: nonce.into(),
            credential_generation: generation,
            created_at: now,
            expires_at: UnixMillis::new(
                now.get()
                    .checked_add(30_000)
                    .ok_or_else(invalid)?
                    .min(root.deadline.get()),
            ),
            consumed_by: None,
        };
        let mut next = self.clone();
        let s = next
            .test_sessions
            .get_mut(&request.session_id)
            .ok_or_else(invalid)?;
        if request.kind != SessionChallengeKindV1::Tool {
            reserve_control_logical(
                s,
                &request.logical_request_id,
                &request.operation_request_digest,
            )?;
        }
        s.challenges
            .insert(challenge.request_id.clone(), challenge.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(challenge)
    }
    pub fn admit_test_session_action(
        &mut self,
        proof: SessionProofRequest<'_>,
        cost: SessionActionCost,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionActionAdmission> {
        let request: SessionToolRequestV1 = original(&proof)?;
        let purpose = match &request.action {
            SessionToolActionV1::Execute(_) => SessionPurposeV1::Execution,
            SessionToolActionV1::ReconcileOperation { .. }
            | SessionToolActionV1::ReadCloseState => SessionPurposeV1::ReconcileRead,
        };
        self.require_session_source_funding(
            &request.session_id,
            current.source_operation_id,
            purpose.source_funding(),
        )?;
        self.require_session_http_paid(
            &request.session_id,
            proof.possession,
            proof.ingress_attempt_id,
            Some(purpose),
        )?;
        let s = self.test_session_local(&request.session_id, purpose, now)?;
        roots::authenticated_receipt(s, purpose, proof.possession, now)?;
        if request.api_version != 1
            || !id(&request.request_id)
            || proof.possession.request_id() != request.request_id
            || !matches!(
                proof.possession.profile(),
                HttpPopProfileV1::Custody | HttpPopProfileV1::Mcp
            )
        {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_live_checks(
            &request.session_id,
            purpose,
            current,
            now,
        )?)?;
        let body = digest(&("oasis7.custody.test-session-tool-request.v1", &request))?;
        let challenge = s
            .challenges
            .values()
            .find(|v| Some(v.nonce.as_str()) == proof.possession.nonce())
            .ok_or_else(invalid)?;
        let generation = s
            .roots
            .get(&purpose)
            .and_then(|r| r.credential.as_ref())
            .ok_or_else(invalid)?
            .generation;
        if challenge.purpose != purpose
            || challenge.kind != SessionChallengeKindV1::Tool
            || challenge.logical_request_id != request.request_id
            || challenge.credential_generation != generation
            || challenge.body_digest != body
            || now.get() >= challenge.expires_at.get()
            || challenge.consumed_by.is_some()
        {
            return Err(invalid());
        }
        match &request.action {
            SessionToolActionV1::Execute(action) => {
                let CustodyIntentV1::TestSession { actions, .. } = &s.input.request.intent else {
                    return Err(invalid());
                };
                if !actions.contains(action) {
                    return Err(invalid());
                }
            }
            SessionToolActionV1::ReconcileOperation { operation_id } => {
                if cost.business_units != 0
                    || s.actions
                        .get(operation_id)
                        .is_none_or(|a| a.purpose != SessionPurposeV1::Execution)
                {
                    return Err(invalid());
                }
            }
            SessionToolActionV1::ReadCloseState => {}
        }
        if matches!(request.action, SessionToolActionV1::ReadCloseState) && cost.business_units != 0
        {
            return Err(invalid());
        }
        let old = s.actions.get(&request.request_id);
        if old.is_none()
            && purpose == SessionPurposeV1::Execution
            && s.actions
                .values()
                .filter(|a| a.purpose == SessionPurposeV1::Execution)
                .count()
                >= 8
        {
            return Err(invalid());
        }
        if old.is_some_and(|o| {
            o.purpose != purpose || o.body_digest != body || o.business.0 != cost.business_units
        }) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let existing = old.cloned();
        let mut next = self.clone();
        let s = next
            .test_sessions
            .get_mut(&request.session_id)
            .ok_or_else(invalid)?;
        s.challenges
            .get_mut(&challenge.request_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(request.request_id.clone());
        let original = if let Some(old) = existing.clone() {
            old
        } else {
            let root = s.roots.get_mut(&purpose).ok_or_else(invalid)?;
            root.budgets.requests.charge(1).map_err(|_| invalid())?;
            root.budgets
                .business
                .reserve(&request.request_id, cost.business_units)
                .map_err(|_| invalid())?;
            let action = SessionActionOriginalV1 {
                request_id: request.request_id.clone(),
                request: request.clone(),
                purpose,
                body_digest: body,
                business: DecimalU64(cost.business_units),
                admitted_at: now,
                transport_attempts: Vec::new(),
            };
            s.actions.insert(request.request_id, action.clone());
            action
        };
        next.validate_persistence()?;
        *self = next;
        Ok(SessionActionAdmission {
            original,
            newly_admitted: existing.is_none(),
        })
    }
    /// Each actual outbound attempt has a separate once-only claim; unknown never refunds.
    pub fn claim_test_session_action_transport(
        &mut self,
        session_id: &str,
        request_id: &str,
        attempt_id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let action = self
            .test_sessions
            .get(session_id)
            .and_then(|s| s.actions.get(request_id))
            .ok_or_else(invalid)?;
        let purpose = action.purpose;
        self.require_session_source_funding(
            session_id,
            current.source_operation_id,
            purpose.source_funding(),
        )?;
        if !id(attempt_id)
            || self
                .test_sessions
                .get(session_id)
                .ok_or_else(invalid)?
                .purpose_http_admissions
                .contains_key(attempt_id)
            || action.transport_attempts.iter().any(|v| v == attempt_id)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        verifier.verify(&self.test_session_live_checks(session_id, purpose, current, now)?)?;
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(session_id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.actions
            .get_mut(request_id)
            .ok_or_else(invalid)?
            .transport_attempts
            .push(attempt_id.into());
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    fn charge_session_control_attempt(
        &mut self,
        id: &str,
        attempt_id: &str,
        hop: SessionControlHopV1,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if !id_nonempty(attempt_id)
            || s.consumed_at.is_none()
            || self.frozen()
            || s.control_transports.contains_key(attempt_id)
            || s.purpose_http_admissions.contains_key(attempt_id)
            || now.get()
                >= admission::pairing(&s.input.request.intent)?
                    .3
                    .max(admission::pairing(&s.input.request.intent)?.4)
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        let ledger = match hop {
            SessionControlHopV1::Host => &mut s.control.as_mut().ok_or_else(invalid)?.transport,
            SessionControlHopV1::Connector => s.connector_transport.as_mut().ok_or_else(invalid)?,
            SessionControlHopV1::Identity => s.identity_transport.as_mut().ok_or_else(invalid)?,
        };
        ledger.charge(1).map_err(|_| invalid())?;
        s.control_transports.insert(attempt_id.into(), hop);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
#[path = "test_session_controls.rs"]
mod controls;
pub(super) use controls::reserve_control_logical;

fn id_nonempty(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}

impl AccountProvisionApprovalState {
    /// Local reservation only. A separate exact source/current check must authorize I/O.
    pub fn reserve_test_session_purpose_transport(
        &mut self,
        id: &str,
        purpose: SessionPurposeV1,
        attempt_id: &str,
        now: UnixMillis,
    ) -> Result<()> {
        self.test_session_local(id, purpose, now)?;
        if self
            .test_sessions
            .get(id)
            .ok_or_else(invalid)?
            .purpose_http_admissions
            .contains_key(attempt_id)
            || !id_nonempty(attempt_id)
            || self
                .test_sessions
                .get(id)
                .ok_or_else(invalid)?
                .purpose_transports
                .contains_key(attempt_id)
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.purpose_transports.insert(attempt_id.into(), purpose);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
