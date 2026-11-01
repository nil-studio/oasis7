use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[path = "test_session_v2_journal_replay_events.rs"]
mod events;
#[path = "test_session_v2_journal_replay_execution.rs"]
mod execution;
#[path = "test_session_v2_journal_replay_machine.rs"]
mod machine;
#[path = "test_session_v2_journal_replay_model.rs"]
mod model;
#[path = "test_session_v2_journal_replay_purpose.rs"]
mod purpose;
#[path = "test_session_v2_journal_replay_world_preflight.rs"]
mod world_preflight;
pub(super) struct Replay<'a> {
    s: &'a TestSessionAuthorityV2,
    control: OperationBudgets,
    identity: BudgetLedger,
    connector: Option<BudgetLedger>,
    logicals: BTreeMap<String, String>,
    sources: BTreeSet<String>,
    reads: BTreeMap<String, u8>,
    source_claims: BTreeSet<String>,
    source_verified: BTreeSet<String>,
    fees: BTreeSet<String>,
    purpose_fees: BTreeSet<String>,
    model_fees: BTreeSet<String>,
    model_reservations: BTreeSet<String>,
    execution_requests: BudgetLedger,
    execution_started: bool,
    execution_prepare_paid: bool,
    execution_prepare_current_ports: bool,
    execution_prepare_admitted: bool,
    execution_prepare_permits_recorded: bool,
    execution_prepare_original_archived: bool,
    execution_native_prepare_claimed: bool,
    execution_prepared_public_archived: bool,
    execution_private_current_claimed: bool,
    execution_private_current_observed: bool,
    execution_lifetime_claimed: bool,
    execution_lifetime_archived: bool,
    execution_lifetime_admitted: bool,
    lifetimes: BTreeSet<(SessionJournalProfileV2, String)>,
    issued: BTreeSet<String>,
    consumed: BTreeSet<String>,
    claims: BTreeSet<String>,
    acks: BTreeSet<String>,
    generations: BTreeMap<SessionPurposeV1, u64>,
    root_transport: BTreeMap<SessionPurposeV1, BudgetLedger>,
    begun: bool,
    verify_claimed: bool,
    approval_consumed: bool,
    preflight_claimed: bool,
    preflight_reads: u8,
    steps: BTreeSet<SessionPreflightStepV1>,
    access: bool,
    lifetime: bool,
    published: bool,
    active: bool,
    machine_claimed: bool,
    machine_ready: bool,
    world_preflight_reserved: bool,
    world_preflight_admitted: bool,
    lease: Option<UnixMillis>,
    heartbeat: Option<UnixMillis>,
    closing: Option<(TestSessionClosingReasonV1, UnixMillis)>,
}
impl<'a> Replay<'a> {
    fn new(s: &'a TestSessionAuthorityV2) -> Result<Self> {
        let p = s.header()?.0;
        Ok(Self {
            s,
            control: OperationBudgets {
                requests: BudgetLedger::new(p.control_budget.requests.0),
                transport: BudgetLedger::new(p.control_budget.transport_per_hop.0),
                business: BudgetLedger::new(0),
            },
            identity: BudgetLedger::new(p.control_budget.identity_transport.0),
            connector: (p.control_budget.transport_hops == 2)
                .then(|| BudgetLedger::new(p.control_budget.transport_per_hop.0)),
            logicals: BTreeMap::new(),
            sources: BTreeSet::new(),
            reads: BTreeMap::new(),
            source_claims: BTreeSet::new(),
            source_verified: BTreeSet::new(),
            fees: BTreeSet::new(),
            purpose_fees: BTreeSet::new(),
            model_fees: BTreeSet::new(),
            model_reservations: BTreeSet::new(),
            execution_requests: BudgetLedger::new(p.request_limit.0),
            execution_started: false,
            execution_prepare_paid: false,
            execution_prepare_current_ports: false,
            execution_prepare_admitted: false,
            execution_prepare_permits_recorded: false,
            execution_prepare_original_archived: false,
            execution_native_prepare_claimed: false,
            execution_prepared_public_archived: false,
            execution_private_current_claimed: false,
            execution_private_current_observed: false,
            execution_lifetime_claimed: false,
            execution_lifetime_archived: false,
            execution_lifetime_admitted: false,
            lifetimes: BTreeSet::new(),
            issued: BTreeSet::new(),
            consumed: BTreeSet::new(),
            claims: BTreeSet::new(),
            acks: BTreeSet::new(),
            generations: BTreeMap::new(),
            root_transport: BTreeMap::new(),
            begun: false,
            verify_claimed: false,
            approval_consumed: false,
            preflight_claimed: false,
            preflight_reads: 0,
            steps: BTreeSet::new(),
            access: false,
            lifetime: false,
            published: false,
            active: false,
            machine_claimed: false,
            machine_ready: false,
            world_preflight_reserved: false,
            world_preflight_admitted: false,
            lease: None,
            heartbeat: None,
            closing: None,
        })
    }
    fn unique(set: &mut BTreeSet<String>, id: &str) -> Result<()> {
        if !set.insert(id.into()) {
            return Err(invalid());
        }
        Ok(())
    }
    fn live(&self, purpose: SessionPurposeV1, at: UnixMillis) -> Result<()> {
        if !self.active || self.s.native_machine_registration.is_some() && !self.machine_ready {
            return Err(invalid());
        }
        let p = self.s.header()?.0;
        let until = match purpose {
            SessionPurposeV1::Execution => p.write_deadline.0,
            SessionPurposeV1::ReconcileRead => p.read_until.0,
            SessionPurposeV1::TerminationPlan => p.cleanup_until.0,
        };
        if at.get() >= until
            || purpose == SessionPurposeV1::Execution
                && (self.closing.is_some() || self.lease.is_none_or(|v| at.get() >= v.get()))
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn maybe_close(&mut self, at: UnixMillis) -> Result<()> {
        if !self.active || self.closing.is_some() {
            return Ok(());
        }
        let empty = self.control.requests.remaining().map_err(|_| invalid())? == 0
            || self.control.transport.remaining().map_err(|_| invalid())? == 0
            || self.identity.remaining().map_err(|_| invalid())? == 0
            || self
                .connector
                .as_ref()
                .is_some_and(|v| v.remaining() == Ok(0))
            || [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead]
                .iter()
                .any(|p| {
                    self.root_transport
                        .get(p)
                        .is_some_and(|v| v.remaining() == Ok(0))
                });
        if empty {
            self.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, at));
        }
        Ok(())
    }
    /// One historical issuance pool for Credential, Control and metadata.
    /// Only earlier ordinal events can occupy or consume this capacity.
    pub(super) fn issue_challenge(&mut self, id: &str, at: UnixMillis) -> Result<()> {
        let c = self.s.challenges.get(id).ok_or_else(invalid)?;
        self.live(c.purpose, at)?;
        if c.created_at != at
            || self.generations.get(&c.purpose) != Some(&c.credential_generation.0)
            || self.issued.len() >= 1024
        {
            return Err(invalid());
        }
        let pending = self
            .issued
            .iter()
            .filter(|key| {
                !self.consumed.contains(*key)
                    && self
                        .s
                        .challenges
                        .get(*key)
                        .is_some_and(|old| at.get() < old.expires_at.get())
            })
            .count();
        let per_second = self
            .issued
            .iter()
            .filter(|key| {
                self.s
                    .challenges
                    .get(*key)
                    .is_some_and(|old| old.created_at.get() / 1000 == at.get() / 1000)
            })
            .count();
        if pending >= 8 || per_second >= 2 {
            return Err(invalid());
        }
        Self::unique(&mut self.issued, id)
    }
    fn reserve_logical(&mut self, id: &str, body: &str) -> Result<()> {
        if let Some(old) = self.logicals.get(id) {
            return if old == body { Ok(()) } else { Err(invalid()) };
        }
        if self.s.control_logicals.get(id).map(String::as_str) != Some(body) {
            return Err(invalid());
        }
        self.control
            .requests
            .reserve(id, 1)
            .map_err(|_| invalid())?;
        self.logicals.insert(id.into(), body.into());
        Ok(())
    }
    fn source_ready(&self, id: &str, at: UnixMillis) -> Result<()> {
        if !self.sources.contains(id) || !self.source_verified.contains(id) {
            return Err(invalid());
        }
        let source = &self.s.sources.get(id).ok_or_else(invalid)?.source;
        let v = &source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if source.verified_at.is_none_or(|v| v.0 > at.get())
            || at.get() >= source.expires_at.0
            || v.observed_at.0 > at.get()
            || at.get() - v.observed_at.0 > 5000
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn consume(
        &mut self,
        id: &str,
        request: &str,
        purpose: SessionPurposeV1,
        at: UnixMillis,
    ) -> Result<()> {
        let c = self.s.challenges.get(id).ok_or_else(invalid)?;
        if !self.issued.contains(id)
            || self.consumed.contains(id)
            || c.consumed_by.as_deref() != Some(request)
            || c.purpose != purpose
            || at.get() < c.created_at.get()
            || at.get() >= c.expires_at.get()
        {
            return Err(invalid());
        }
        self.consumed.insert(id.into());
        Ok(())
    }
    fn finish(self) -> Result<()> {
        let s = self.s;

        if let Some(book) = &s.execution_book {
            let b = book.journal_record();
            if self.execution_native_prepare_claimed != book.native_prepare_record().is_some()
                || self.execution_prepared_public_archived
                    != book
                        .native_prepare_record()
                        .is_some_and(|r| r.public_recorded_at.is_some())
                || self.execution_private_current_claimed != book.private_current_read(s).is_some()
                || self.execution_private_current_observed
                    != book
                        .private_current_read(s)
                        .is_some_and(|r| r.completed_at.is_some())
                || self.execution_prepare_original_archived
                    != book.prepare_original_recorded_at().is_some()
                || self.execution_prepare_permits_recorded
                    != book.prepare_service_record().is_some()
                || self.execution_prepare_current_ports
                    != book.prepare_current_read(self.s).is_some()
                || self.execution_prepare_paid != (book.prepare_paid_count() == 1)
                || self.execution_prepare_admitted != (book.prepare_stage_count() == 1)
                || !self.execution_started
                || !self.execution_lifetime_claimed
                || self.execution_lifetime_archived != b.lifetime_archived_at.is_some()
                || self.execution_lifetime_admitted != b.lifetime_admitted_at.is_some()
            {
                return Err(invalid());
            }
        } else if self.execution_started
            || self.execution_lifetime_claimed
            || self.execution_lifetime_archived
            || self.execution_lifetime_admitted
            || self.execution_prepare_current_ports
            || self.execution_prepare_paid
            || self.execution_prepare_admitted
            || self.execution_prepare_permits_recorded
            || self.execution_prepare_original_archived
        {
            return Err(invalid());
        }
        if self.world_preflight_reserved != s.preflight_world_access.is_some()
            || self.world_preflight_admitted
                != s.preflight_world_access
                    .as_ref()
                    .is_some_and(|w| w.admitted())
        {
            return Err(invalid());
        }
        if self.machine_claimed
            != s.native_machine_registration
                .as_ref()
                .is_some_and(|m| m.active.is_some())
            || self.machine_ready
                != s.native_machine_registration
                    .as_ref()
                    .is_some_and(|m| m.ready())
            || !self.begun
            || self.verify_claimed != s.approval_verification_claimed_at.is_some()
            || self.approval_consumed != s.consumed_at.is_some()
            || self.preflight_claimed != s.preflight_claimed_at.is_some()
            || self.preflight_reads != s.preflight_oracle_reads
            || self.steps.len() != s.preflight_steps.len()
            || self.access != s.preflight_access.is_some()
            || self.lifetime != s.preflight_lifetime.is_some()
            || self.published != s.preflight_read.is_some()
            || self.active != s.approved_at.is_some()
            || self.sources.len() != s.sources.len()
            || self.reads != s.source_oracle_reads
            || self.source_claims.len()
                != s.sources
                    .values()
                    .filter(|w| w.source.claimed_at.is_some())
                    .count()
            || self.source_verified.len()
                != s.sources
                    .values()
                    .filter(|w| w.source.verification.is_some())
                    .count()
            || self.fees.len() != s.client_http_admissions.len()
            || self.purpose_fees.len() != s.purpose_http_admissions.len()
            || self.model_fees.len()
                != s.native_model_budget
                    .as_ref()
                    .map_or(0, |m| m.http_admissions.len())
            || self.model_reservations.len()
                != s.native_model_budget
                    .as_ref()
                    .map_or(0, |m| m.reservations.len())
            || self.lifetimes.len()
                != s.control_transports
                    .values()
                    .filter(|h| **h == SessionControlHopV1::Identity)
                    .count()
            || self.issued.len() != s.challenges.len()
            || self.consumed.len()
                != s.challenges
                    .values()
                    .filter(|c| c.consumed_by.is_some())
                    .count()
            || self.claims.len() != s.credential_claims.len()
            || self.acks.len() != s.controls.len()
            || self.logicals != s.control_logicals
            || Some(&self.control) != s.control.as_ref()
            || Some(&self.identity) != s.identity_transport.as_ref()
            || self.connector != s.connector_transport
            || self.lease != s.lease_until
            || self.heartbeat != s.last_heartbeat
            || self.closing != s.closing
        {
            return Err(invalid());
        }
        if self.active {
            for purpose in [
                SessionPurposeV1::Execution,
                SessionPurposeV1::ReconcileRead,
                SessionPurposeV1::TerminationPlan,
            ] {
                let root = s.roots.get(&purpose).ok_or_else(invalid)?;
                if root.credential.as_ref().map_or(0, |c| c.generation.0)
                    != *self.generations.get(&purpose).ok_or_else(invalid)?
                {
                    return Err(invalid());
                }
                // Cleanup has its own independently restored Closing-only
                // allocator/paid original book. It cannot change live phase.
                if purpose == SessionPurposeV1::Execution
                    && root.budgets.requests != self.execution_requests
                {
                    return Err(invalid());
                }
                if purpose != SessionPurposeV1::TerminationPlan
                    && self.root_transport.get(&purpose) != Some(&root.budgets.transport)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
pub(super) fn validate(s: &TestSessionAuthorityV2) -> Result<()> {
    let mut replay = Replay::new(s)?;
    for (index, event) in s.events.iter().enumerate() {
        let ordinal = u64::try_from(index)
            .map_err(|_| invalid())?
            .checked_add(1)
            .ok_or_else(invalid)?;
        replay.apply(&event.event, event.at, ordinal)?;
        replay.maybe_close(event.at)?;
    }
    replay.finish()
}
