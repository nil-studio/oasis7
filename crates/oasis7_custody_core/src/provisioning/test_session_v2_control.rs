//! Real V2 Control authority. No V1 record projection and no World mutation.
use super::*;
#[path = "test_session_v2_control_http.rs"]
mod http;
#[path = "test_session_v2_control_ops.rs"]
mod ops;
#[path = "test_session_v2_control_restore.rs"]
mod restore;
#[path = "test_session_v2_control_source.rs"]
mod source;
pub(in crate::provisioning) use restore::validate as validate_control_records_v2;
pub(in crate::provisioning) use source::control_lifetime_claimed_v2;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct ControlObservationV2 {
    approved_input_digest: String,
    source_operation_id: String,
    source_ticket: u8,
    private_sid_generation: DecimalU64,
    lifetime: SignedManagedNativeSessionLifetimeV1,
    checked_at: UnixMillis,
}
impl ControlObservationV2 {
    pub(in crate::provisioning) fn source_operation_id(&self) -> &str {
        &self.source_operation_id
    }
}
impl core::fmt::Debug for ControlObservationV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ControlObservationV2(<private>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::provisioning) enum SessionControlRecordV2 {
    Challenge {
        sequence: u64,
        approved_input_digest: String,
        request: SessionChallengeRequestV1,
        ingress_attempt_id: String,
        signed_attempt_digest: String,
        observation: ControlObservationV2,
    },
    Control {
        sequence: u64,
        approved_input_digest: String,
        request: SessionControlRequestV1,
        ingress_attempt_id: String,
        signed_attempt_digest: String,
        challenge_request_id: String,
        #[serde(deserialize_with = "required_observation")]
        observation: Option<ControlObservationV2>,
    },
}
fn required_observation<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> core::result::Result<Option<ControlObservationV2>, D::Error> {
    Option::deserialize(d)
}
impl core::fmt::Debug for SessionControlRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionControlRecordV2(<private>)")
    }
}
pub(in crate::provisioning) fn stored_lifetimes_v2(
    s: &TestSessionAuthorityV2,
) -> impl Iterator<Item = &SignedManagedNativeSessionLifetimeV1> {
    s.control_records.values().filter_map(|r| match r {
        SessionControlRecordV2::Challenge { observation, .. } => Some(&observation.lifetime),
        SessionControlRecordV2::Control { observation, .. } => {
            observation.as_ref().map(|v| &v.lifetime)
        }
    })
}
fn valid_id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn closing_kind(k: SessionControlKindV1) -> bool {
    matches!(
        k,
        SessionControlKindV1::Finish | SessionControlKindV1::Abort
    )
}
fn reserve_logical(s: &mut TestSessionAuthorityV2, id: &str, body: &str) -> Result<()> {
    if !valid_id(id) || !hex64(body) {
        return Err(invalid());
    }
    if let Some(old) = s.control_logicals.get(id) {
        return if old == body {
            Ok(())
        } else {
            Err(ProvisionApprovalError::Conflict)
        };
    }
    s.control
        .as_mut()
        .ok_or_else(invalid)?
        .requests
        .reserve(id, 1)
        .map_err(|_| invalid())?;
    s.control_logicals.insert(id.into(), body.into());
    Ok(())
}
fn observation(
    s: &TestSessionAuthorityV2,
    current: &TestSessionLiveCurrent<'_, '_>,
    now: UnixMillis,
) -> Result<ControlObservationV2> {
    if s.source_funding.get(current.source_operation_id) != Some(&SessionSourceFundingV1::Control)
        || current.source_ticket != 4
    {
        return Err(invalid());
    }
    Ok(ControlObservationV2 {
        approved_input_digest: s.input_digest()?,
        source_operation_id: current.source_operation_id.into(),
        source_ticket: current.source_ticket,
        private_sid_generation: current.private_sid_generation,
        lifetime: current.lifetime.clone(),
        checked_at: now,
    })
}

fn next_sequence(s: &TestSessionAuthorityV2, _now: UnixMillis) -> Result<u64> {
    next_session_ordinal_v2(s)
}

pub(in crate::provisioning) fn id_in_use_v2(s: &TestSessionAuthorityV2, id: &str) -> bool {
    s.control_records.contains_key(id)
        || stored_lifetimes_v2(s).any(|v| v.lifetime.status.nonce == id)
}

/// Actual V2 Native-root token PoP only. This does not admit Source/current,
/// lease, Closing, purpose budgets or an execution Book dispatch.
pub(in crate::provisioning) fn authenticated_receipt_v2<'a>(
    s: &'a TestSessionAuthorityV2,
    purpose: SessionPurposeV1,
    proof: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<&'a SessionCredentialV1> {
    http::authenticated(s, purpose, proof, now)
}
