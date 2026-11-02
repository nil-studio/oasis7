//! Private wire consumers. A historical claim contains no recoverable token.
use super::state::*;
use oasis7_custody_core::{
    DecimalU64,
    provisioning::{
        SessionChallengeRequestV1, SessionChallengeV1, SessionControlAckV1, SessionControlKindV1,
        SessionPurposeV1,
    },
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ClaimReply {
    Claimed {
        api_version: u8,
        session_id: String,
        request_id: String,
        credentials: Vec<WireCredential>,
    },
    Historical {
        api_version: u8,
        session_id: String,
        request_id: String,
        credentials: Vec<Historical>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireCredential {
    pub purpose: SessionPurposeV1,
    pub generation: DecimalU64,
    pub token: String,
}
impl Drop for WireCredential {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.token.zeroize();
    }
}
impl WireCredential {
    fn validate(&self) -> Result<(), String> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let b = zeroize::Zeroizing::new(URL_SAFE_NO_PAD.decode(&self.token).map_err(|_| denied())?);
        if b.len() != 32
            || self.token.len() != 43
            || URL_SAFE_NO_PAD.encode(&*b) != self.token
            || !(1..=4).contains(&self.generation.0)
            || self.purpose == SessionPurposeV1::TerminationPlan
        {
            return Err(denied());
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Historical {
    pub purpose: SessionPurposeV1,
    pub generation: DecimalU64,
}
impl ClaimReply {
    pub fn install(
        self,
        state: &mut State,
        request: &oasis7_custody_core::client_enrollment::ClientCredentialClaimRequestV1,
    ) -> Result<bool, String> {
        let wanted: Vec<_> = request
            .purposes
            .iter()
            .map(|p| match p {
                oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Execute => {
                    Ok(SessionPurposeV1::Execution)
                }
                oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Read => {
                    Ok(SessionPurposeV1::ReconcileRead)
                }
                _ => Err(denied()),
            })
            .collect::<Result<_, _>>()?;
        let session = state.session.as_mut().ok_or_else(denied)?;
        let (version, sid, rid, mut credentials, historical) = match self {
            Self::Claimed {
                api_version,
                session_id,
                request_id,
                credentials,
            } => (api_version, session_id, request_id, credentials, None),
            Self::Historical {
                api_version,
                session_id,
                request_id,
                credentials,
            } => (
                api_version,
                session_id,
                request_id,
                Vec::new(),
                Some(credentials),
            ),
        };
        if version != 1 || sid != session.binding.session_id || rid != request.request_id {
            return Err(denied());
        }
        if let Some(values) = historical {
            if values.len() != wanted.len() {
                return Err(denied());
            }
            let mut seen = std::collections::BTreeSet::new();
            for c in values {
                if !wanted.contains(&c.purpose)
                    || !seen.insert(c.purpose)
                    || !(1..=4).contains(&c.generation.0)
                {
                    return Err(denied());
                }
            }
            // Never fabricate a secret from historical metadata or clear an
            // unknown. A trusted operator must explicitly replace generation.
            return Ok(false);
        }
        if credentials.len() != wanted.len() {
            return Err(denied());
        }
        let expected = request
            .expected_generation
            .map_or(Ok(1), |g| g.0.checked_add(1).ok_or_else(denied))?;
        let mut seen = std::collections::BTreeSet::new();
        for c in &credentials {
            c.validate()?;
            if !wanted.contains(&c.purpose)
                || !seen.insert(c.purpose)
                || c.generation.0 != expected
                || c.generation.0 <= session.floor(c.purpose)?
                || super::vault::now()? >= session.binding.deadline(c.purpose)?
            {
                return Err(denied());
            }
        }
        for mut c in credentials.drain(..) {
            let p = c.purpose;
            if let Some(old) = session.credentials.iter().find(|old| old.purpose == p)
                && old.generation.0 >= c.generation.0
            {
                return Err(denied());
            }
            session.credentials.retain(|old| old.purpose != p);
            let retired = c.generation.0 - 1;
            match p {
                SessionPurposeV1::Execution => {
                    session.retired_execution = session.retired_execution.max(retired)
                }
                SessionPurposeV1::ReconcileRead => {
                    session.retired_read = session.retired_read.max(retired)
                }
                _ => return Err(denied()),
            }
            session.credentials.push(Credential {
                purpose: c.purpose,
                generation: c.generation,
                token: std::mem::take(&mut c.token),
                claim_request_id: request.request_id.clone(),
            });
        }
        Ok(true)
    }
}
pub(super) fn challenge(
    body: &[u8],
    request: &SessionChallengeRequestV1,
    generation: Option<DecimalU64>,
    deadline: u64,
) -> Result<SessionChallengeV1, String> {
    let c: SessionChallengeV1 = serde_json::from_slice(body).map_err(|_| denied())?;
    let now = super::vault::now()?;
    if c.request_id != request.request_id
        || c.purpose != request.purpose
        || c.kind != request.kind
        || c.logical_request_id != request.logical_request_id
        || c.body_digest != request.operation_request_digest
        || c.consumed_by.is_some()
        || c.created_at.get() > now
        || c.expires_at.get() <= now
        || c.expires_at.get() > deadline
        || generation.is_some_and(|g| g != c.credential_generation)
    {
        return Err(denied());
    }
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let n = URL_SAFE_NO_PAD.decode(&c.nonce).map_err(|_| denied())?;
    if n.len() != 32 || URL_SAFE_NO_PAD.encode(n) != c.nonce {
        return Err(denied());
    }
    Ok(c)
}
pub(super) fn ack(
    body: &[u8],
    id: &str,
    kind: SessionControlKindV1,
    write_deadline: u64,
) -> Result<SessionControlAckV1, String> {
    let a: SessionControlAckV1 = serde_json::from_slice(body).map_err(|_| denied())?;
    if a.request_id != id
        || a.kind != kind
        || a.acknowledged_at.get() == 0
        || a.lease_until.get() > write_deadline
    {
        return Err(denied());
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_claim_shape_has_no_token_recovery_channel() {
        let value = serde_json::json!({"status":"historical","api_version":1,"session_id":"session","request_id":"claim",
            "credentials":[{"purpose":"execution","generation":"1"}]});
        assert!(serde_json::from_value::<ClaimReply>(value.clone()).is_ok());
        let mut injected = value;
        injected["credentials"][0]["token"] = "secret".into();
        assert!(serde_json::from_value::<ClaimReply>(injected).is_err());
    }
    #[test]
    fn credential_requires_canonical_32_bytes_and_bounded_generation() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let mut c = WireCredential {
            purpose: SessionPurposeV1::Execution,
            generation: DecimalU64(1),
            token: URL_SAFE_NO_PAD.encode([7; 32]),
        };
        assert!(c.validate().is_ok());
        c.token.push('=');
        assert!(c.validate().is_err());
        c.token = URL_SAFE_NO_PAD.encode([7; 32]);
        c.generation = DecimalU64(5);
        assert!(c.validate().is_err());
        c.generation = DecimalU64(1);
        c.purpose = SessionPurposeV1::TerminationPlan;
        assert!(c.validate().is_err());
    }
}
