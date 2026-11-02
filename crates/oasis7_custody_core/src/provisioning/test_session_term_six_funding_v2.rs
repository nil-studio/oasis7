//! Local Term24 payer/once only. Reserved and claimed are not sent or Current.
use super::*;
use crate::native_cleanup_v2::NativeTermCurrentRequestV2;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermObservationRoundV2 {
    First,
    Fresh,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermObservationPortV2 {
    Directory,
    RfService,
    RfActor,
    IdentityService,
    NativeSessionsActor,
    Native,
}
impl TermObservationPortV2 {
    fn all() -> [Self; 6] {
        [
            Self::Directory,
            Self::RfService,
            Self::RfActor,
            Self::IdentityService,
            Self::NativeSessionsActor,
            Self::Native,
        ]
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Allocation {
    kind: TermObservationPortV2,
    attempt_id: String,
    paid_index: u32,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    round: TermObservationRoundV2,
    round_id: String,
    input_digest: String,
    term_digest: String,
    invocation_id: String,
    source_id: String,
    nonce: String,
    lifetime_nonce: String,
    origin_nonce: String,
    started_at: UnixMillis,
    until: UnixMillis,
    paid_before: u32,
    ports: [Allocation; 6],
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    at: UnixMillis,
    ordinal: u64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermSixFundingRecordV2 {
    header: Header,
    reserved_ordinal: u64,
    claims: BTreeMap<TermObservationPortV2, Claim>,
}
/// Only first reservation returns this. History cannot reconstruct a caller.
pub struct PreparedTermSixFundingV2 {
    session_id: String,
    header: Header,
    reserved_ordinal: u64,
}
/// Only first per-port SQL claim returns this. It is payment metadata, not permission.
pub struct PreparedTermObservationPortV2 {
    session_id: String,
    header: Header,
    port: TermObservationPortV2,
    claim: Claim,
}
impl PreparedTermSixFundingV2 {
    pub fn round_id(&self) -> &str {
        &self.header.round_id
    }
    pub fn started_at(&self) -> UnixMillis {
        self.header.started_at
    }
    pub fn until(&self) -> UnixMillis {
        self.header.until
    }
}
impl PreparedTermObservationPortV2 {
    pub fn kind(&self) -> TermObservationPortV2 {
        self.port
    }
    pub fn attempt_id(&self) -> &str {
        &self.header.ports[self.port_index()].attempt_id
    }
    pub fn paid_index(&self) -> u32 {
        self.header.ports[self.port_index()].paid_index
    }
    pub fn started_at(&self) -> UnixMillis {
        self.header.started_at
    }
    pub fn claimed_at(&self) -> UnixMillis {
        self.claim.at
    }
    pub fn until(&self) -> UnixMillis {
        self.header.until
    }
    pub fn graph_digest(&self) -> Result<String> {
        digest(&("oasis7.custody.term-six-header.v2", &self.header))
    }
    pub fn paid_record_digest(&self) -> Result<String> {
        digest(&(
            "oasis7.custody.term-six-paid-port.v2",
            &self.session_id,
            &self.header,
            self.port,
            &self.claim,
        ))
    }
    fn port_index(&self) -> usize {
        TermObservationPortV2::all()
            .iter()
            .position(|p| *p == self.port)
            .expect("closed port")
    }
    /// Typed canonical metadata only. It still lacks the configured issuer,
    /// authenticated prerequisite observations and actual Current/floor proof.
    pub fn native_request(
        &self,
        state: &AccountProvisionApprovalState,
    ) -> Result<NativeTermCurrentRequestV2> {
        if self.port != TermObservationPortV2::Native {
            return Err(invalid());
        }
        let s = state.test_session_v2_record(&self.session_id)?;
        let record = find(s, &self.header.round_id)?;
        if record.header != self.header || record.claims.get(&self.port) != Some(&self.claim) {
            return Err(invalid());
        }
        let term = &s.header()?.0.termination;
        Ok(NativeTermCurrentRequestV2 {
            api_version: 2,
            session_id: self.session_id.clone(),
            owned_invocation_id: self.header.invocation_id.clone(),
            source_operation_id: self.header.source_id.clone(),
            stop_operation_id: term.stop_operation_id.clone(),
            close_operation_id: term.close_operation_id.clone(),
            nonce: self.header.nonce.clone(),
            lifetime_nonce: self.header.lifetime_nonce.clone(),
            origin_nonce: self.header.origin_nonce.clone(),
        })
    }
}
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn find<'a>(s: &'a TestSessionAuthorityV2, id: &str) -> Result<&'a TermSixFundingRecordV2> {
    s.termination_state
        .term_six_funding
        .iter()
        .find(|r| r.header.round_id == id)
        .ok_or_else(invalid)
}
fn expected_header(
    s: &TestSessionAuthorityV2,
    round: TermObservationRoundV2,
    at: UnixMillis,
    paid_before: u32,
) -> Result<Header> {
    let term = &s.header()?.0.termination;
    let owned = s
        .termination_state
        .owned_invocation
        .as_ref()
        .ok_or_else(invalid)?;
    let input_digest = s.input_digest()?;
    let term_digest = term.digest().map_err(|_| invalid())?;
    let round_id = digest(&(
        "oasis7.custody.term-six-round.v2",
        &input_digest,
        &term_digest,
        owned.invocation_id(),
        round,
    ))?;
    let nonce = |kind: &str| digest(&("oasis7.custody.term-six-nonce.v2", &round_id, kind));
    let until = at
        .get()
        .checked_add(5000)
        .ok_or_else(invalid)?
        .min(term.cleanup_until.get())
        .min(
            s.roots
                .get(&SessionPurposeV1::TerminationPlan)
                .ok_or_else(invalid)?
                .deadline
                .get(),
        );
    let ports = TermObservationPortV2::all()
        .into_iter()
        .enumerate()
        .map(|(n, kind)| {
            Ok(Allocation {
                kind,
                attempt_id: format!(
                    "term-six-port-{}",
                    digest(&("oasis7.custody.term-six-port.v2", &round_id, kind))?
                ),
                paid_index: paid_before
                    .checked_add(u32::try_from(n).map_err(|_| invalid())?)
                    .ok_or_else(invalid)?,
            })
        })
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| invalid())?;
    Ok(Header {
        round,
        source_id: format!("term-six-source-{round_id}"),
        nonce: nonce("observation")?,
        lifetime_nonce: nonce("lifetime")?,
        origin_nonce: nonce("origin")?,
        round_id,
        input_digest,
        term_digest,
        invocation_id: owned.invocation_id().into(),
        started_at: at,
        until: UnixMillis::new(until),
        paid_before,
        ports,
    })
}
fn scope(s: &TestSessionAuthorityV2) -> Result<()> {
    let t = &s.header()?.0.termination;
    if s.closing.is_none()
        || t.stop_policy != WorldTaskStopPolicyV2::GuardedCommittedUtcStopV2
        || !matches!(
            t.cleanup_transport_policy,
            CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2
                | CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2
        )
        || s.native_machine_registration
            .as_ref()
            .is_none_or(|m| !m.ready())
        || s.source_funding
            .values()
            .any(|v| v == &SessionSourceFundingV1::TerminationPlan)
    {
        return Err(invalid());
    }
    s.termination_state.validate_input_binding(&s.input)
}
impl Header {
    fn ids(&self) -> impl Iterator<Item = &str> {
        [
            &self.round_id,
            &self.source_id,
            &self.nonce,
            &self.lifetime_nonce,
            &self.origin_nonce,
        ]
        .into_iter()
        .map(String::as_str)
        .chain(self.ports.iter().map(|p| p.attempt_id.as_str()))
    }
}
impl TermSixFundingRecordV2 {
    pub(in crate::provisioning) fn round_id(&self) -> &str {
        &self.header.round_id
    }
    pub(in crate::provisioning) fn replay_reserved(
        &self,
        s: &TestSessionAuthorityV2,
        at: UnixMillis,
        ordinal: u64,
    ) -> Result<()> {
        self.validate(s)?;
        if self.header.started_at != at || self.reserved_ordinal != ordinal {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::provisioning) fn replay_claim(
        &self,
        s: &TestSessionAuthorityV2,
        port: TermObservationPortV2,
        at: UnixMillis,
        ordinal: u64,
    ) -> Result<()> {
        self.validate(s)?;
        let c = self.claims.get(&port).ok_or_else(invalid)?;
        if c.at != at || c.ordinal != ordinal {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::provisioning) fn claim_count(&self) -> usize {
        self.claims.len()
    }
    pub(in crate::provisioning) fn id_in_use(&self, id: &str) -> bool {
        self.header.ids().any(|v| v == id)
    }
    pub(in crate::provisioning) fn event_count(&self) -> usize {
        1 + self.claims.len()
    }
    pub(in crate::provisioning) fn paid_indices(&self) -> impl Iterator<Item = u64> + '_ {
        self.header.ports.iter().map(|p| u64::from(p.paid_index))
    }
    pub(in crate::provisioning) fn physical_ids(&self) -> impl Iterator<Item = &str> {
        self.header.ids()
    }
    pub(in crate::provisioning) fn validate(&self, s: &TestSessionAuthorityV2) -> Result<()> {
        let h = &self.header;
        let owned = s
            .termination_state
            .owned_invocation
            .as_ref()
            .ok_or_else(invalid)?;
        let qualified = s
            .termination_state
            .qualification_read
            .as_ref()
            .ok_or_else(invalid)?;
        if *h != expected_header(s, h.round, h.started_at, h.paid_before)?
            || h.started_at.get() < qualified.paid_at().get()
            || h.started_at.get() < owned.acknowledged_at(s)?.get()
            || h.started_at.get() >= h.until.get()
            || h.paid_before.checked_add(6).is_none_or(|n| n > 24)
            || h.paid_before <= qualified.paid_index
            || self.reserved_ordinal <= owned.acknowledged_ordinal(s)?
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::TermSixReserved {
                    round_id: h.round_id.clone(),
                },
            )? != Some(self.reserved_ordinal)
        {
            return Err(invalid());
        }
        let mut last = self.reserved_ordinal;
        for (index, port) in TermObservationPortV2::all().into_iter().enumerate() {
            if index >= self.claims.len() {
                if self.claims.contains_key(&port) {
                    return Err(invalid());
                }
                continue;
            }
            let c = self.claims.get(&port).ok_or_else(invalid)?;
            if c.ordinal <= last
                || c.at.get() < h.started_at.get()
                || c.at.get() >= h.until.get()
                || session_event_ordinal_v2(
                    s,
                    &SessionEventKindV2::TermSixPortClaimed {
                        round_id: h.round_id.clone(),
                        port,
                    },
                )? != Some(c.ordinal)
            {
                return Err(invalid());
            }
            last = c.ordinal;
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// Six named reservations, never six sends. Only the real first owned
    /// continuation can return a per-round factory token; cold facts cannot.
    pub fn reserve_test_session_term_six_v2(
        &mut self,
        owned: &TermOwnedInvocationV2,
        round: TermObservationRoundV2,
        now: UnixMillis,
    ) -> Result<PreparedTermSixFundingV2> {
        owned.validate_local(self)?;
        let id = owned.session_id();
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        scope(s)?;
        let required_len = usize::from(round == TermObservationRoundV2::Fresh);
        if s.termination_state.qualification_read.is_none()
            || s.termination_state.term_six_funding.len() != required_len
            || required_len == 1 && s.termination_state.term_six_funding[0].claims.len() != 6
        {
            return Err(invalid());
        }
        let root = s
            .roots
            .get(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?;
        let paid = u32::try_from(
            24u64
                .checked_sub(root.budgets.transport.remaining().map_err(|_| invalid())?)
                .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        let header = expected_header(s, round, now, paid)?;
        let mut unique = BTreeSet::new();
        for value in header.ids() {
            if !unique.insert(value) {
                return Err(invalid());
            }
            self.test_session_v2_transport_id_available(id, value)?;
        }
        let ordinal = next_session_ordinal_v2(s)?;
        let record = TermSixFundingRecordV2 {
            header: header.clone(),
            reserved_ordinal: ordinal,
            claims: BTreeMap::new(),
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(6)
            .map_err(|_| invalid())?;
        s.termination_state.term_six_funding.push(record);
        append_session_event_v2(
            s,
            SessionEventKindV2::TermSixReserved {
                round_id: header.round_id.clone(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedTermSixFundingV2 {
            session_id: id.into(),
            header,
            reserved_ordinal: ordinal,
        })
    }
    /// Claim one already-paid physical port. There is no receiver permit here,
    /// no retry, and no Current/Source/Vault/World authorization from this token.
    pub fn claim_test_session_term_six_port_v2(
        &mut self,
        first: &PreparedTermSixFundingV2,
        port: TermObservationPortV2,
        now: UnixMillis,
    ) -> Result<PreparedTermObservationPortV2> {
        let s =
            self.test_session_v2_local(&first.session_id, SessionPurposeV1::TerminationPlan, now)?;
        scope(s)?;
        let record = find(s, &first.header.round_id)?;
        let index = TermObservationPortV2::all()
            .iter()
            .position(|p| *p == port)
            .ok_or_else(invalid)?;
        if record.header != first.header
            || record.reserved_ordinal != first.reserved_ordinal
            || record.claims.len() != index
            || record.claims.contains_key(&port)
            || now.get() < first.header.started_at.get()
            || now.get() >= first.header.until.get()
        {
            return Err(invalid());
        }
        let claim = Claim {
            at: now,
            ordinal: next_session_ordinal_v2(s)?,
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&first.session_id)?;
        s.termination_state
            .term_six_funding
            .iter_mut()
            .find(|r| r.header.round_id == first.header.round_id)
            .ok_or_else(invalid)?
            .claims
            .insert(port, claim.clone());
        append_session_event_v2(
            s,
            SessionEventKindV2::TermSixPortClaimed {
                round_id: first.header.round_id.clone(),
                port,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedTermObservationPortV2 {
            session_id: first.session_id.clone(),
            header: first.header.clone(),
            port,
            claim,
        })
    }
    pub(in crate::provisioning) fn validate_term_six_funding_v2(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        if s.termination_state.term_six_funding.is_empty() {
            return Ok(());
        }
        scope(s)?;
        if s.termination_state.term_six_funding.len() > 2 {
            return Err(invalid());
        }
        for (n, r) in s.termination_state.term_six_funding.iter().enumerate() {
            if r.header.round
                != if n == 0 {
                    TermObservationRoundV2::First
                } else {
                    TermObservationRoundV2::Fresh
                }
                || n == 1
                    && (s.termination_state.term_six_funding[0].claims.len() != 6
                        || s.termination_state.term_six_funding[0]
                            .claims
                            .values()
                            .any(|c| {
                                c.ordinal >= r.reserved_ordinal
                                    || c.at.get() > r.header.started_at.get()
                            })
                        || r.header.started_at.get()
                            < s.termination_state.term_six_funding[0]
                                .header
                                .started_at
                                .get())
            {
                return Err(invalid());
            }
            r.validate(s)?;
            for id in r.physical_ids() {
                if s.sources.contains_key(id)
                    || s.sources.values().any(|w| w.source.verify_nonce == id)
                    || s.source_funding.contains_key(id)
                    || s.purpose_transports.contains_key(id)
                    || s.purpose_http_admissions.contains_key(id)
                    || s.control_transports.contains_key(id)
                    || s.client_http_admissions.contains_key(id)
                    || s.tool_nonce_ingresses.contains_key(id)
                    || super::super::v2_credentials::credential_claim_id_in_use_v2(s, id)
                    || super::super::v2_control::id_in_use_v2(s, id)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
