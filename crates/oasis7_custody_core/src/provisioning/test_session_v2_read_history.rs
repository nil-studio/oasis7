//! Fixed already-paid Read-only Account history consumer. No Native lookup,
//! private body, fresh Current return, mutation admission or usage settlement.
use super::*;
use crate::native_execution_prepare_v2::ExecutionPreparedPublicResultV2;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn required<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPrepareHistoryFeeV2 {
    pub(super) request: SessionActionRequestV2,
    #[serde(deserialize_with = "required")]
    pub(super) original_digest: Option<String>,
    pub(super) owner_paid_at: UnixMillis,
    pub(super) challenge_request_id: String,
    pub(super) consumed_challenge: bool,
    #[serde(deserialize_with = "required")]
    pub(super) outcome: Option<ReadPrepareHistoryOutcomeV2>,
}
impl core::fmt::Debug for ReadPrepareHistoryFeeV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ReadPrepareHistoryFeeV2(<historical-read-only>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadPrepareHistoryOutcomeV2 {
    pub(super) returned_at: UnixMillis,
    pub(super) source_operation_id: String,
    pub(super) source_ticket: u8,
    pub(super) source_digest: String,
    pub(super) observed_at: UnixMillis,
    pub(super) lifetime: SignedManagedNativeSessionLifetimeV1,
    pub(super) private_sid_generation: DecimalU64,
    #[serde(deserialize_with = "required")]
    pub(super) fact_digest: Option<String>,
}
/// First fee commit only. No Clone/serde/Debug or historical constructor.
pub struct ReadPrepareHistoryPaidV2 {
    approved_input_digest: String,
    signed_attempt_digest: String,
    ingress: String,
    request_digest: String,
    original_digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadPrepareHistoryReplyV2 {
    /// This attempted scope was not a known persisted Prepare original. No
    /// query/continuation is issued and the paid result never upgrades.
    UnknownOriginal,
    /// No Account public fact was observed; this is not World business state.
    Pending,
    HistoricalPublicObservation {
        observation: Box<ExecutionPreparedPublicResultV2>,
    },
}
fn fee<'a>(s: &'a TestSessionAuthorityV2, ingress: &str) -> Result<&'a ReadPrepareHistoryFeeV2> {
    match &s
        .purpose_http_admissions
        .get(ingress)
        .ok_or_else(invalid)?
        .class
    {
        SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { fee } => Ok(fee),
        _ => Err(invalid()),
    }
}
fn candidate(
    state: &AccountProvisionApprovalState,
    id: &str,
    q: &SessionActionRequestV2,
) -> Result<Option<String>> {
    if q.session_id != id {
        return Err(invalid());
    }
    let SessionToolActionV1::ReconcileOperation { operation_id } = &q.action else {
        return Ok(None);
    };
    if operation_id.is_empty()
        || operation_id.len() > 256
        || operation_id.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    let s = state.test_session_v2_record(id)?;
    if s.execution_book
        .as_ref()
        .is_none_or(|b| b.prepare_original_recorded_at().is_none())
    {
        return Ok(None);
    }
    let original = state
        .test_session_v2_prepare_original_history(id)?
        .ok_or_else(invalid)?;
    if operation_id != &original.request.original.request.client_operation_id {
        return Ok(None);
    }
    original
        .commitment_digest()
        .map(Some)
        .map_err(|_| invalid())
}
fn original(
    state: &AccountProvisionApprovalState,
    id: &str,
    q: &SessionActionRequestV2,
) -> Result<String> {
    candidate(state, id, q)?.ok_or_else(invalid)
}

fn fact(s: &TestSessionAuthorityV2) -> Result<Option<(String, UnixMillis)>> {
    let Some(record) = s
        .execution_book
        .as_ref()
        .and_then(|b| b.native_prepare_record())
    else {
        return Ok(None);
    };
    match (&record.public_original_cbor_hex, record.public_recorded_at) {
        (Some(raw), Some(at)) => Ok(Some((
            digest(&("oasis7.read.prepared-public-history.v2", raw))?,
            at,
        ))),
        (None, None) => Ok(None),
        _ => Err(invalid()),
    }
}
pub(super) fn read_fee_count(s: &TestSessionAuthorityV2) -> u64 {
    s.purpose_http_admissions
        .values()
        .filter(|a| {
            matches!(
                a.class,
                SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { .. }
            )
        })
        .count() as u64
}
impl AccountProvisionApprovalState {
    /// Host1 was separately authenticated/charged. These actual Owner-service
    /// and Account ingress2 are committed before any new Current producer I/O.
    /// Every new trusted ingress spends Read requests1 and transport2; same
    /// ingress callback cannot refund, reissue an opaque token or retry work.
    pub fn charge_test_session_v2_prepare_history_owner_attempt(
        &mut self,
        id: &str,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<Option<ReadPrepareHistoryPaidV2>> {
        let q = action_original_v2(&proof)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::ReconcileRead, now)?;
        authenticated_receipt_v2(s, SessionPurposeV1::ReconcileRead, proof.possession, now)?;
        execution_target_v2(self, s, SessionPurposeV1::ReconcileRead, proof.possession)?;
        if q.purpose() != SessionPurposeV1::ReconcileRead {
            return Err(invalid());
        }
        let original_digest = candidate(self, id, &q)?;
        let paid = s
            .purpose_http_admissions
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if paid.purpose != SessionPurposeV1::ReconcileRead
            || paid.hop != SessionControlHopV1::Host
            || paid.signed_attempt_digest != proof.possession.transport_attempt_digest()
            || paid.charged_at.get() > now.get()
        {
            return Err(invalid());
        }
        if let SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { fee } = &paid.class {
            return if fee.request == q
                && fee
                    .original_digest
                    .as_ref()
                    .is_none_or(|d| original_digest.as_ref() == Some(d))
            {
                Ok(None)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if paid.class != SessionPurposeHttpAdmissionClassV1::GenericOne {
            return Err(invalid());
        }
        let c = s
            .challenges
            .values()
            .find(|c| Some(c.nonce.as_str()) == proof.possession.nonce())
            .ok_or_else(invalid)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        if q.purpose() != SessionPurposeV1::ReconcileRead
            || c.kind != SessionChallengeKindV1::ToolMetadata
            || c.purpose != SessionPurposeV1::ReconcileRead
            || c.logical_request_id != q.request_id
            || c.body_digest != q.digest()?
            || c.consumed_by.as_ref().is_some_and(|r| r != &q.request_id)
            || c.consumed_by.is_none()
                && (now.get() >= c.expires_at.get()
                    || c.credential_generation
                        != root.credential.as_ref().ok_or_else(invalid)?.generation)
        {
            return Err(invalid());
        }
        let challenge_request_id = c.request_id.clone();
        let consumed_challenge = c.consumed_by.is_none();
        let request_digest = q.digest()?;
        let approved_input_digest = s.input_digest()?;
        let signed_attempt_digest = proof.possession.transport_attempt_digest();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let root = s
            .roots
            .get_mut(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        root.budgets.requests.charge(1).map_err(|_| invalid())?;
        root.budgets.transport.charge(2).map_err(|_| invalid())?;
        s.challenges
            .get_mut(&challenge_request_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(q.request_id.clone());
        s.purpose_http_admissions
            .get_mut(proof.ingress_attempt_id)
            .ok_or_else(invalid)?
            .class = SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 {
            fee: Box::new(ReadPrepareHistoryFeeV2 {
                request: q,
                original_digest: original_digest.clone(),
                owner_paid_at: now,
                challenge_request_id,
                consumed_challenge,
                outcome: None,
            }),
        };
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadPrepareHistoryOwnerPaid {
                ingress_attempt_id: proof.ingress_attempt_id.into(),
            },
            now,
        )?;
        if s.roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && s.closing.is_none()
        {
            s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(
            original_digest.map(|original_digest| ReadPrepareHistoryPaidV2 {
                approved_input_digest,
                signed_attempt_digest,
                ingress: proof.ingress_attempt_id.into(),
                request_digest,
                original_digest,
            }),
        )
    }
    pub fn test_session_v2_prepare_history_checks(
        &self,
        id: &str,
        ingress: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let paid = fee(s, ingress)?;
        if paid.original_digest.is_none()
            || paid.outcome.is_some()
            || paid.owner_paid_at.get() > now.get()
            || s.source_funding.get(current.source_operation_id)
                != Some(&SessionSourceFundingV1::ReconcileRead)
        {
            return Err(invalid());
        }
        self.test_session_v2_live_scope(id, SessionPurposeV1::ReconcileRead, current, now)
    }
    /// Token consumes the original first attempt. Unknown cannot be resumed by
    /// restoring the paid row, and a fresh observation never grants a new action.
    pub fn finish_test_session_v2_prepare_history(
        &mut self,
        id: &str,
        paid: ReadPrepareHistoryPaidV2,
        proof: SessionProofRequest<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ReadPrepareHistoryReplyV2> {
        let q = action_original_v2(&proof)?;
        let s = self.test_session_v2_record(id)?;
        let old = fee(s, &paid.ingress)?;
        if s.input_digest()? != paid.approved_input_digest
            || proof.possession.transport_attempt_digest() != paid.signed_attempt_digest
            || proof.ingress_attempt_id != paid.ingress
            || old.outcome.is_some()
            || old.request != q
            || q.digest()? != paid.request_digest
            || original(self, id, &q)? != paid.original_digest
            || old.original_digest.as_deref() != Some(paid.original_digest.as_str())
            || s.purpose_http_admissions
                .get(&paid.ingress)
                .ok_or_else(invalid)?
                .signed_attempt_digest
                != proof.possession.transport_attempt_digest()
        {
            return Err(invalid());
        }
        authenticated_receipt_v2(s, SessionPurposeV1::ReconcileRead, proof.possession, now)?;
        execution_target_v2(self, s, SessionPurposeV1::ReconcileRead, proof.possession)?;
        verifier.verify(&self.test_session_v2_prepare_history_checks(
            id,
            &paid.ingress,
            current,
            now,
        )?)?;
        let observation = self.read_prepared_public_history_v2(id, current.native_scope)?;
        let fact_digest = fact(s)?
            .map(|(d, at)| {
                if at.get() <= now.get() {
                    Ok(d)
                } else {
                    Err(invalid())
                }
            })
            .transpose()?;
        if observation.is_some() != fact_digest.is_some() {
            return Err(invalid());
        }
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        let outcome = ReadPrepareHistoryOutcomeV2 {
            returned_at: now,
            source_operation_id: current.source_operation_id.into(),
            source_ticket: current.source_ticket,
            source_digest: digest(source)?,
            observed_at: current.observed_at,
            lifetime: current.lifetime.clone(),
            private_sid_generation: current.private_sid_generation,
            fact_digest,
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let SessionPurposeHttpAdmissionClassV1::ReadPrepareHistoryV2 { fee } = &mut s
            .purpose_http_admissions
            .get_mut(&paid.ingress)
            .ok_or_else(invalid)?
            .class
        else {
            return Err(invalid());
        };
        fee.outcome = Some(outcome);
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadPrepareHistoryReturned {
                ingress_attempt_id: paid.ingress,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(match observation {
            Some(observation) => ReadPrepareHistoryReplyV2::HistoricalPublicObservation {
                observation: Box::new(observation),
            },
            None => ReadPrepareHistoryReplyV2::Pending,
        })
    }
}
#[path = "test_session_v2_read_history_restore.rs"]
mod restore;
pub(super) use restore::validate_read_history;

#[path = "test_session_v2_read_history_return.rs"]
mod returned;

pub(in crate::provisioning) use restore::read_nonce_consumed_exact;
