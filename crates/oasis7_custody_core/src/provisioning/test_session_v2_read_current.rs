//! Read-only first paid producer. It reserves the v3 graph, not Current.
//! No Execution/Control funding, Native dispatch or returned authority exists here.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
const ENTRY: u64 = 5;
const REMAINING: u64 = 54;
const PORTS: [&str; 55] = [
    "claim_source.callback.owner",
    "claim_source.callback.account",
    "claim_source.directory",
    "claim_source.rf.dispatcher",
    "claim_source.rf.causal_owner",
    "claim_source.rf.causal_account",
    "claim_source.rf.fences",
    "source_record_and_predecrypt.callback.owner",
    "source_record_and_predecrypt.callback.account",
    "source_record_and_predecrypt.directory",
    "source_record_and_predecrypt.rf.dispatcher",
    "source_record_and_predecrypt.rf.causal_owner",
    "source_record_and_predecrypt.rf.causal_account",
    "source_record_and_predecrypt.rf.fences",
    "postdecrypt_and_claim_lifetime.callback.owner",
    "postdecrypt_and_claim_lifetime.callback.account",
    "postdecrypt_and_claim_lifetime.directory",
    "postdecrypt_and_claim_lifetime.rf.dispatcher",
    "postdecrypt_and_claim_lifetime.rf.causal_owner",
    "postdecrypt_and_claim_lifetime.rf.causal_account",
    "postdecrypt_and_claim_lifetime.rf.fences",
    "final_admit.callback.owner",
    "final_admit.callback.account",
    "final_admit.directory",
    "final_admit.rf.dispatcher",
    "final_admit.rf.causal_owner",
    "final_admit.rf.causal_account",
    "final_admit.rf.fences",
    "before_decrypt.identity_rf.auth_owner",
    "before_decrypt.identity_rf.auth_account",
    "before_decrypt.identity_rf.dispatcher",
    "before_decrypt.identity_rf.causal_owner",
    "before_decrypt.identity_rf.causal_account",
    "before_decrypt.identity_rf.fences",
    "after_decrypt.identity_rf.auth_owner",
    "after_decrypt.identity_rf.auth_account",
    "after_decrypt.identity_rf.dispatcher",
    "after_decrypt.identity_rf.causal_owner",
    "after_decrypt.identity_rf.causal_account",
    "after_decrypt.identity_rf.fences",
    "before_native_lifetime.identity_rf.auth_owner",
    "before_native_lifetime.identity_rf.auth_account",
    "before_native_lifetime.identity_rf.dispatcher",
    "before_native_lifetime.identity_rf.causal_owner",
    "before_native_lifetime.identity_rf.causal_account",
    "before_native_lifetime.identity_rf.fences",
    "private.manifest_actor",
    "final.private_attestation.service",
    "final.private_attestation.manifest",
    "private.cipher.sid_before",
    "private.cipher.sid_after",
    "private.cipher.csrf_before",
    "private.cipher.csrf_after",
    "native.source",
    "native.lifetime",
];
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
pub struct ReadCurrentPreparationRecordV3 {
    pub(super) request: SessionActionRequestV2,
    #[serde(deserialize_with = "required")]
    pub(super) original_digest: Option<String>,
    pub(super) challenge_request_id: String,
    pub(super) consumed_challenge: bool,
    pub(super) settled_at: UnixMillis,
    pub(super) credential_generation: DecimalU64,
    pub(super) request_until: UnixMillis,
    pub(super) entry_routes: [String; 4],
    #[serde(deserialize_with = "required")]
    pub(super) reservation: Option<ReadCurrentReservationV3>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadCurrentReservationV3 {
    pub(super) source_id: String,
    nonce: String,
    pub(super) reserved_at: UnixMillis,
    pub(super) until: UnixMillis,
    graph_digest: String,
    tickets: u8,
    pub(super) claims: BTreeMap<ReadCurrentPortV3, ReadCurrentPortClaimV3>,
}
impl core::fmt::Debug for ReadCurrentPreparationRecordV3 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ReadCurrentPreparationRecordV3(<paid-read-only>)")
    }
}
/// First settlement only. Historical records cannot reconstruct this value.
pub struct ReadCurrentIngressPaidV3 {
    session: String,
    ingress: String,
    input: String,
    attempt: String,
    original: String,
}
/// Five-ticket Source reservation only, never an observed or full Current value.
pub struct ReadCurrentSourceV3 {
    session: String,
    ingress: String,
    input: String,
    source: String,
    graph: String,
    tickets: u8,
}
impl ReadCurrentSourceV3 {
    pub fn source_operation_id(&self) -> &str {
        &self.source
    }
}
fn record<'a>(
    s: &'a TestSessionAuthorityV2,
    ingress: &str,
) -> Result<&'a ReadCurrentPreparationRecordV3> {
    match &s
        .purpose_http_admissions
        .get(ingress)
        .ok_or_else(invalid)?
        .class
    {
        SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation } => Ok(preparation),
        _ => Err(invalid()),
    }
}
fn record_mut<'a>(
    s: &'a mut TestSessionAuthorityV2,
    ingress: &str,
) -> Result<&'a mut ReadCurrentPreparationRecordV3> {
    match &mut s
        .purpose_http_admissions
        .get_mut(ingress)
        .ok_or_else(invalid)?
        .class
    {
        SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation } => Ok(preparation),
        _ => Err(invalid()),
    }
}
fn known(
    state: &AccountProvisionApprovalState,
    id: &str,
    q: &SessionActionRequestV2,
) -> Result<Option<String>> {
    let SessionToolActionV1::ReconcileOperation { operation_id } = &q.action else {
        return Err(invalid());
    };
    if operation_id.is_empty()
        || operation_id.len() > 256
        || operation_id.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    if state
        .test_session_v2_record(id)?
        .execution_book
        .as_ref()
        .is_none_or(|b| b.prepare_original_recorded_at().is_none())
    {
        return Ok(None);
    }
    let Some(original) = state.test_session_v2_prepare_original_history(id)? else {
        return Ok(None);
    };
    if &original.request.original.request.client_operation_id != operation_id {
        return Ok(None);
    }
    original
        .commitment_digest()
        .map(Some)
        .map_err(|_| invalid())
}
fn source_id(id: &str, ingress: &str, input: &str) -> Result<String> {
    digest(&("oasis7.custody.read-current-source.v3", id, ingress, input))
}
fn graph(
    input: &str,
    ingress: &str,
    source: &str,
    nonce: &str,
    until: UnixMillis,
) -> Result<String> {
    digest(&(
        "oasis7.custody.read-current-paid-graph.v3",
        input,
        ingress,
        source,
        nonce,
        until,
        ENTRY,
        REMAINING + 1,
        PORTS.as_slice(),
    ))
}
impl AccountProvisionApprovalState {
    /// Settle the five actually observed ingress edges before any fallible
    /// post-entry reservation. Unknown scopes pay and permanently get no token.
    pub fn settle_test_session_v2_read_current_five(
        &mut self,
        id: &str,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<Option<ReadCurrentIngressPaidV3>> {
        let q = action_original_v2(&proof)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::ReconcileRead, now)?;
        authenticated_receipt_v2(s, SessionPurposeV1::ReconcileRead, proof.possession, now)?;
        execution_target_v2(self, s, SessionPurposeV1::ReconcileRead, proof.possession)?;
        if q.session_id != id || q.purpose() != SessionPurposeV1::ReconcileRead {
            return Err(invalid());
        }
        let ingress = proof.ingress_attempt_id;
        let attempt = proof.possession.transport_attempt_digest();
        if ingress.is_empty() || ingress.len() > 256 || ingress.chars().any(char::is_control) {
            return Err(invalid());
        }
        if let Some(old) = s.purpose_http_admissions.get(ingress) {
            let r = record(s, ingress)?;
            return if old.purpose == SessionPurposeV1::ReconcileRead
                && old.hop == SessionControlHopV1::Host
                && old.signed_attempt_digest == attempt
                && r.request == q
                && old.charged_at == r.settled_at
                && now.get() >= r.settled_at.get()
            {
                Ok(None)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_transport_id_available(id, ingress)?;
        if s.purpose_http_admissions.len() >= 700 {
            return Err(invalid());
        }
        let root = s
            .roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        let c = s
            .challenges
            .values()
            .find(|c| Some(c.nonce.as_str()) == proof.possession.nonce())
            .ok_or_else(invalid)?;
        if c.kind != SessionChallengeKindV1::ToolMetadata
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
        let entry_routes: [String; 4] = [
            "provider_abuse",
            "realm_abuse",
            "owner_service",
            "account_actor",
        ]
        .map(|route| digest(&("oasis7.custody.read-current-ingress.v3", id, ingress, route)))
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid())?;
        for route in &entry_routes {
            self.test_session_v2_transport_id_available(id, route)?;
        }
        let credential_generation = root.credential.as_ref().ok_or_else(invalid)?.generation;
        let request_until = proof
            .possession
            .expires()
            .checked_millis()
            .map_err(|_| invalid())?;
        if request_until.get() <= now.get() || request_until.get() > root.deadline.get() {
            return Err(invalid());
        }
        let original = known(self, id, &q)?;
        let input = s.input_digest()?;
        let challenge = c.request_id.clone();
        let consumed = c.consumed_by.is_none();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let r = s
            .roots
            .get_mut(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?;
        r.budgets.transport.charge(ENTRY).map_err(|_| invalid())?;
        r.budgets.requests.charge(1).map_err(|_| invalid())?;
        s.challenges
            .get_mut(&challenge)
            .ok_or_else(invalid)?
            .consumed_by = Some(q.request_id.clone());
        s.purpose_http_admissions.insert(
            ingress.into(),
            SessionPurposeHttpAdmissionV1 {
                class: SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 {
                    preparation: Box::new(ReadCurrentPreparationRecordV3 {
                        request: q,
                        original_digest: original.clone(),
                        challenge_request_id: challenge,
                        consumed_challenge: consumed,
                        settled_at: now,
                        credential_generation,
                        request_until,
                        entry_routes,
                        reservation: None,
                    }),
                },
                signed_attempt_digest: attempt.clone(),
                purpose: SessionPurposeV1::ReconcileRead,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        append_session_event_v2(
            s,
            SessionEventKindV2::PurposeHttpCharged {
                ingress_attempt_id: ingress.into(),
            },
            now,
        )?;
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadCurrentIngressSettled {
                ingress_attempt_id: ingress.into(),
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
        Ok(original.map(|original| ReadCurrentIngressPaidV3 {
            session: id.into(),
            ingress: ingress.into(),
            input,
            attempt,
            original,
        }))
    }
    /// One atomic 54-port prepayment plus the existing Read-funded Source1.
    /// This does not send a Native request or assert a Directory/Fence result.
    pub fn reserve_test_session_v2_read_current_source_five(
        &mut self,
        paid: ReadCurrentIngressPaidV3,
        nonce: &str,
        now: UnixMillis,
    ) -> Result<ReadCurrentSourceV3> {
        let id = &paid.session;
        let s = self.test_session_v2_local(id, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &paid.ingress)?;
        let admission = s
            .purpose_http_admissions
            .get(&paid.ingress)
            .ok_or_else(invalid)?;
        if s.input_digest()? != paid.input
            || r.reservation.is_some()
            || r.original_digest.as_ref() != Some(&paid.original)
            || known(self, id, &r.request)?.as_ref() != Some(&paid.original)
            || admission.signed_attempt_digest != paid.attempt
            || !hex64(nonce)
            || now.get() < r.settled_at.get()
        {
            return Err(invalid());
        }
        let source = source_id(id, &paid.ingress, &paid.input)?;
        let until = UnixMillis::new(
            now.get()
                .checked_add(30_000)
                .ok_or_else(invalid)?
                .min(r.request_until.get())
                .min(
                    s.roots
                        .get(&SessionPurposeV1::ReconcileRead)
                        .ok_or_else(invalid)?
                        .deadline
                        .get(),
                ),
        );
        if now.get() >= until.get() {
            return Err(invalid());
        }
        let hash = graph(&paid.input, &paid.ingress, &source, nonce, until)?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(REMAINING)
            .map_err(|_| invalid())?;
        record_mut(s, &paid.ingress)?.reservation = Some(ReadCurrentReservationV3 {
            source_id: source.clone(),
            nonce: nonce.into(),
            reserved_at: now,
            until,
            graph_digest: hash.clone(),
            tickets: 0,
            claims: BTreeMap::new(),
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadCurrentReserved {
                ingress_attempt_id: paid.ingress.clone(),
            },
            now,
        )?;
        next.reserve_test_session_v2_source_inner(
            id,
            SessionSourceReservation {
                operation_id: &source,
                nonce,
                funding: SessionSourceFundingV1::ReconcileRead,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(ReadCurrentSourceV3 {
            session: paid.session,
            ingress: paid.ingress,
            input: paid.input,
            source,
            graph: hash,
            tickets: 0,
        })
    }
    /// Consume a prepaid slot once before a receiver-specific producer can try
    /// I/O. This budget token is intentionally not accepted by any existing
    /// Execution RF/Identity/Native API and carries no observed authority.
    pub fn claim_test_session_v2_read_current_prepaid_port(
        &mut self,
        token: &mut ReadCurrentSourceV3,
        port: ReadCurrentPortV3,
        now: UnixMillis,
    ) -> Result<ReadCurrentPortPrepaidV3> {
        let s = self.test_session_v2_local(&token.session, SessionPurposeV1::ReconcileRead, now)?;
        let p = record(s, &token.ingress)?
            .reservation
            .as_ref()
            .ok_or_else(invalid)?;
        if s.input_digest()? != token.input
            || p.source_id != token.source
            || p.graph_digest != token.graph
            || now.get() < p.reserved_at.get()
            || now.get() >= p.until.get()
            || p.claims.contains_key(&port)
        {
            return Err(invalid());
        }
        let until = p.until;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&token.session)?;
        let event = SessionEventKindV2::ReadCurrentPortPrepaidClaimed {
            ingress_attempt_id: token.ingress.clone(),
            port,
        };
        let ordinal = next_session_ordinal_v2(s)?;
        record_mut(s, &token.ingress)?
            .reservation
            .as_mut()
            .ok_or_else(invalid)?
            .claims
            .insert(port, ReadCurrentPortClaimV3 { at: now, ordinal });
        append_session_event_v2(s, event, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(ReadCurrentPortPrepaidV3 {
            port,
            until,
            scope_digest: digest(&(
                "oasis7.read.prepaid-port.v3",
                &token.session,
                &token.input,
                &token.ingress,
                &token.source,
                &token.graph,
                port,
                until,
            ))?,
        })
    }
    /// Reserve only the next of five local observation tickets. A ticket is not
    /// an observed Source, LF, Current, dispatch proof or callback permission.
    pub fn reserve_test_session_v2_read_current_ticket(
        &mut self,
        token: &mut ReadCurrentSourceV3,
        now: UnixMillis,
    ) -> Result<u8> {
        let s = self.test_session_v2_local(&token.session, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &token.ingress)?
            .reservation
            .as_ref()
            .ok_or_else(invalid)?;
        if s.input_digest()? != token.input
            || r.source_id != token.source
            || r.graph_digest != token.graph
            || r.tickets != token.tickets
            || r.tickets >= 5
            || now.get() >= r.until.get()
        {
            return Err(invalid());
        }
        let wanted = r.tickets.checked_add(1).ok_or_else(invalid)?;
        let mut next = self.clone();
        record_mut(
            next.test_session_v2_record_mut(&token.session)?,
            &token.ingress,
        )?
        .reservation
        .as_mut()
        .ok_or_else(invalid)?
        .tickets = wanted;
        if next.reserve_test_session_v2_source_read(&token.session, &token.source, now)? != wanted {
            return Err(invalid());
        }
        next.validate_persistence()?;
        *self = next;
        token.tickets = wanted;
        Ok(wanted)
    }
}
#[path = "test_session_v2_read_current_restore.rs"]
mod restore;
pub(in crate::provisioning) use restore::*;
#[path = "test_session_v2_read_current_ports.rs"]
mod ports;
use ports::ReadCurrentPortClaimV3;
pub use ports::{ReadCurrentPortPrepaidV3, ReadCurrentPortV3};

pub(super) fn fee_counts(s: &TestSessionAuthorityV2) -> (u64, u64) {
    s.purpose_http_admissions.values().fold((0, 0), |mut n, a| {
        if let SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation: r } = &a.class {
            n.0 += 1;
            n.1 += u64::from(r.reservation.is_some());
        }
        n
    })
}
pub(super) fn event_count(s: &TestSessionAuthorityV2) -> usize {
    let (a, b) = fee_counts(s);
    (a + b) as usize
        + s.purpose_http_admissions
            .values()
            .map(|a| match &a.class {
                SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 { preparation: r } => {
                    r.reservation.as_ref().map_or(0, |p| p.claims.len())
                }
                _ => 0,
            })
            .sum::<usize>()
}

pub(super) fn source_owned(s: &TestSessionAuthorityV2, id: &str) -> bool {
    s.purpose_http_admissions.values().any(|a|matches!(&a.class,SessionPurposeHttpAdmissionClassV1::ReadCurrentFiveV3 {preparation:r} if r.reservation.as_ref().is_some_and(|p|p.source_id==id)))
}
