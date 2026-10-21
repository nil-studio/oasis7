//! Session-owned, memory-only credentials and atomic physical-request admission.
//! A connection is never a World permission. Callbacks are trusted host boundaries.
use crate::preset::ProviderPreset;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

#[path = "connection_types.rs"]
mod types;
pub use types::*;
pub enum ProbeStart {
    Pending,
    Ready,
    Failed,
    Operation(ProbeOperationPermit),
}
pub struct ProbeOperationPermit {
    manager: ConnectionManager,
    principal: Principal,
    connection: String,
    version: u64,
    operation: String,
}
impl ProbeOperationPermit {
    pub fn reserve_phase(
        &self,
        phase: u8,
        max_tokens: u64,
    ) -> Result<RequestPermit, ConnectionError> {
        self.manager.reserve_probe_phase(self, phase, max_tokens)
    }
}
struct ProbeRecord {
    state: ProbeState,
    next_phase: u8,
    reserved: bool,
}

#[derive(Clone)]
pub struct ConnectionManager {
    shared: Arc<Shared>,
}
struct Shared {
    sessions: Arc<dyn SessionAuthority>,
    bindings: Arc<dyn BindingAuthority>,
    clock: Arc<dyn Clock>,
    limits: Limits,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    connections: HashMap<String, Connection>,
    budgets: HashMap<Principal, BudgetSnapshot>,
    tickets: HashMap<String, Ticket>,
    last_now: u64,
}
struct Connection {
    owner: Principal,
    key: Option<Zeroizing<Vec<u8>>>,
    preset: ProviderPreset,
    version: u64,
    generation: u64,
    active_version: Option<u64>,
    state: ConnectionState,
    binding: Option<BindingTarget>,
    expires: u64,
    absolute: u64,
    renewed: u64,
    probes: HashMap<String, ProbeRecord>,
    stops: HashMap<String, u64>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum TicketState {
    Reserved,
    Sent,
    Finished,
}
struct Ticket {
    connection: String,
    version: u64,
    generation: u64,
    owner: Principal,
    binding: Option<BindingTarget>,
    probe: Option<String>,
    phase: Option<u8>,
    tokens: u64,
    state: TicketState,
}
/// No Debug/Serialize/Clone. Dropping an unknown request never refunds a budget
/// or frees a possibly still-running physical request. Only finish does so.
pub struct RequestPermit {
    manager: ConnectionManager,
    ticket: String,
    principal: Principal,
    binding: Option<BindingTarget>,
    key: Zeroizing<Vec<u8>>,
    preset: ProviderPreset,
    tokens: u64,
}
impl RequestPermit {
    pub fn limits(&self) -> &Limits {
        &self.manager.shared.limits
    }
    pub fn reserved_tokens(&self) -> u64 {
        self.tokens
    }
    pub fn preset(&self) -> &ProviderPreset {
        &self.preset
    }
    pub fn before_send(&self) -> Result<(), ConnectionError> {
        self.manager.before_send(self)
    }
    /// Borrow only for request construction, after rechecking admission.
    pub fn with_key<R>(&self, f: impl FnOnce(&[u8]) -> R) -> Result<R, ConnectionError> {
        self.manager.check_permit(self, true)?;
        Ok(f(&self.key))
    }
    pub fn finish(&self, usage: Option<u64>, probe_verified: bool) -> Result<(), ConnectionError> {
        self.manager.finish(self, usage, probe_verified)
    }
}
fn id() -> Result<String, ConnectionError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| ConnectionError::Unavailable)?;
    Ok(hex::encode(bytes))
}
fn valid_text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256
}
impl ConnectionManager {
    pub fn new(
        sessions: Arc<dyn SessionAuthority>,
        bindings: Arc<dyn BindingAuthority>,
        clock: Arc<dyn Clock>,
        limits: Limits,
    ) -> Result<Self, ConnectionError> {
        if !limits.valid() {
            return Err(ConnectionError::Invalid);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                sessions,
                bindings,
                clock,
                limits,
                state: Mutex::new(State::default()),
            }),
        })
    }
    pub fn limits(&self) -> &Limits {
        &self.shared.limits
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, ConnectionError> {
        self.shared
            .state
            .lock()
            .map_err(|_| ConnectionError::Unavailable)
    }
    fn authenticate(&self, p: &Principal) -> Result<(), ConnectionError> {
        if !valid_text(&p.account_id)
            || !valid_text(&p.session_id)
            || !valid_text(&p.auth_ref)
            || !self.shared.sessions.is_current(p)
        {
            if let Ok(mut state) = self.lock() {
                for connection in state.connections.values_mut().filter(|c| c.owner == *p) {
                    Self::revoke(connection, ConnectionState::Revoked);
                }
            }
            return Err(ConnectionError::Unavailable);
        }
        Ok(())
    }
    fn invalidate_probes(c: &mut Connection) {
        for probe in c
            .probes
            .values_mut()
            .filter(|r| r.state == ProbeState::Pending)
        {
            probe.state = ProbeState::Failed;
        }
        if c.state == ConnectionState::Checking {
            c.state = ConnectionState::Blocked;
        }
    }
    fn revoke(c: &mut Connection, state: ConnectionState) {
        c.key.take();
        c.binding = None;
        c.active_version = None;
        c.generation = c.generation.saturating_add(1);
        c.state = state;
    }
    fn expire(&self, state: &mut State) -> u64 {
        let now = self.shared.clock.now_ms().max(state.last_now);
        state.last_now = now;
        for c in state.connections.values_mut() {
            if c.key.is_some() && (now >= c.expires || now >= c.absolute) {
                Self::revoke(c, ConnectionState::Expired)
            }
        }
        now
    }
    fn current<'a>(
        state: &'a mut State,
        p: &Principal,
        name: &str,
        version: Option<u64>,
    ) -> Result<&'a mut Connection, ConnectionError> {
        let c = state
            .connections
            .get_mut(name)
            .filter(|c| c.owner == *p)
            .ok_or(ConnectionError::Unavailable)?;
        if c.key.is_none() {
            return Err(ConnectionError::Unavailable);
        }
        if version.is_some_and(|v| v != c.version) {
            return Err(ConnectionError::Conflict);
        }
        Ok(c)
    }
    fn projection(name: &str, c: &Connection, budget: BudgetSnapshot) -> ConnectionSnapshot {
        ConnectionSnapshot {
            id: name.to_owned(),
            preset_id: c.preset.id.clone(),
            version: c.version,
            generation: c.generation,
            active_version: c.active_version,
            state: c.state,
            run_state: if c.binding.is_some() {
                RunState::Active
            } else {
                RunState::Stopped
            },
            expires_at_ms: c.expires,
            absolute_expires_at_ms: c.absolute,
            budget,
        }
    }
    pub fn create(
        &self,
        p: &Principal,
        preset: ProviderPreset,
        key: Zeroizing<Vec<u8>>,
        consent: bool,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        if !consent
            || key.is_empty()
            || key.len() > 4096
            || key.iter().any(|b| !b.is_ascii_graphic())
            || preset.validate().is_err()
        {
            return Err(ConnectionError::Invalid);
        }
        let name = id()?;
        let mut state = self.lock()?;
        let now = self.expire(&mut state);
        if state.connections.contains_key(&name) {
            return Err(ConnectionError::Unavailable);
        }
        if state.connections.len() >= 4096
            || state
                .connections
                .values()
                .filter(|c| c.owner == *p && c.key.is_some())
                .count()
                >= 8
        {
            return Err(ConnectionError::CapacityExceeded);
        }
        let c = Connection {
            owner: p.clone(),
            key: Some(key),
            preset,
            version: 1,
            generation: 0,
            active_version: None,
            state: ConnectionState::Draft,
            binding: None,
            expires: now.saturating_add(self.shared.limits.lease_ms),
            absolute: now.saturating_add(self.shared.limits.absolute_ms),
            renewed: now,
            probes: HashMap::new(),
            stops: HashMap::new(),
        };
        let budget = state.budgets.entry(p.clone()).or_default().clone();
        let result = Self::projection(&name, &c, budget);
        state.connections.insert(name, c);
        Ok(result)
    }
    pub fn snapshot(
        &self,
        p: &Principal,
        name: &str,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = state
            .connections
            .get(name)
            .filter(|c| c.owner == *p)
            .ok_or(ConnectionError::Unavailable)?;
        Ok(Self::projection(
            name,
            c,
            state.budgets.get(p).cloned().unwrap_or_default(),
        ))
    }
    pub fn list(&self, p: &Principal) -> Result<Vec<ConnectionSnapshot>, ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        Ok(state
            .connections
            .iter()
            .filter(|(_, c)| c.owner == *p)
            .map(|(id, c)| Self::projection(id, c, budget.clone()))
            .collect())
    }
    /// Read an original probe's terminal status without sending or reserving a call.
    pub fn probe_status(
        &self,
        p: &Principal,
        name: &str,
        operation: &str,
    ) -> Result<Option<ProbeState>, ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, p, name, None)?;
        Ok(c.probes.get(operation).map(|record| record.state))
    }
    /// Read whether an original Stop was committed, without changing its run.
    pub fn stop_recorded(
        &self,
        p: &Principal,
        name: &str,
        operation: &str,
    ) -> Result<bool, ConnectionError> {
        self.authenticate(p)?;
        let state = self.lock()?;
        let c = state
            .connections
            .get(name)
            .filter(|c| c.owner == *p)
            .ok_or(ConnectionError::Unavailable)?;
        Ok(c.stops.contains_key(operation))
    }
    pub fn renew(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        let now = self.expire(&mut state);
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        let c = Self::current(&mut state, p, name, Some(version))?;
        if now.saturating_sub(c.renewed) < self.shared.limits.renew_interval_ms {
            return Err(ConnectionError::Invalid);
        }
        c.expires = now
            .saturating_add(self.shared.limits.lease_ms)
            .min(c.absolute);
        c.renewed = now;
        c.version = c
            .version
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        Self::invalidate_probes(c);
        Ok(Self::projection(name, c, budget))
    }
    pub fn activate(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        target: BindingTarget,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        if !valid_text(&target.full_fence_digest)
            || !valid_text(&target.world_id)
            || !valid_text(&target.agent_id)
            || !valid_text(&target.task_id)
            || !valid_text(&target.executor_id)
            || target.goal_revision == 0
            || target.executor_epoch == 0
            || !self.shared.bindings.authorize(p, &target)
        {
            return Err(ConnectionError::BindingDenied);
        }
        let mut state = self.lock()?;
        self.expire(&mut state);
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        let c = Self::current(&mut state, p, name, Some(version))?;
        if c.state != ConnectionState::Ready {
            return Err(ConnectionError::NotReady);
        }
        c.generation = c
            .generation
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        c.version = c
            .version
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        Self::invalidate_probes(c);
        c.active_version = Some(c.version);
        c.binding = Some(target);
        Ok(Self::projection(name, c, budget))
    }
    pub fn stop(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        let c = Self::current(&mut state, p, name, Some(version))?;
        c.generation = c
            .generation
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        c.version = c
            .version
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        Self::invalidate_probes(c);
        c.binding = None;
        c.active_version = None;
        Ok(Self::projection(name, c, budget))
    }
    /// Idempotent stop is scoped to this connection and the original CAS
    /// version. Reusing its operation ID can never stop a newly activated run.
    pub fn stop_operation(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        operation: &str,
    ) -> Result<ConnectionSnapshot, ConnectionError> {
        self.authenticate(p)?;
        if !valid_text(operation) {
            return Err(ConnectionError::Invalid);
        }
        let mut state = self.lock()?;
        self.expire(&mut state);
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        let c = state
            .connections
            .get_mut(name)
            .filter(|c| c.owner == *p)
            .ok_or(ConnectionError::Unavailable)?;
        if let Some(original) = c.stops.get(operation) {
            if *original != version {
                return Err(ConnectionError::Conflict);
            }
            return Ok(Self::projection(name, c, budget));
        }
        if c.key.is_none() {
            return Err(ConnectionError::Unavailable);
        }
        if c.version != version {
            return Err(ConnectionError::Conflict);
        }
        if c.stops.len() >= 1024 {
            return Err(ConnectionError::BudgetExceeded);
        }
        c.generation = c
            .generation
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        c.version = c
            .version
            .checked_add(1)
            .ok_or(ConnectionError::Unavailable)?;
        Self::invalidate_probes(c);
        c.binding = None;
        c.active_version = None;
        c.stops.insert(operation.to_owned(), version);
        Ok(Self::projection(name, c, budget))
    }
    pub fn forget(&self, p: &Principal, name: &str, version: u64) -> Result<(), ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = state
            .connections
            .get_mut(name)
            .filter(|c| c.owner == *p)
            .ok_or(ConnectionError::Unavailable)?;
        if c.version != version {
            return Err(ConnectionError::Conflict);
        }
        if c.key.is_some() {
            Self::revoke(c, ConnectionState::Revoked)
        }
        Ok(())
    }
    pub fn logout(&self, p: &Principal) -> Result<(), ConnectionError> {
        self.authenticate(p)?;
        let mut state = self.lock()?;
        for c in state.connections.values_mut().filter(|c| c.owner == *p) {
            if c.key.is_some() {
                Self::revoke(c, ConnectionState::Revoked)
            }
        }
        Ok(())
    }
    /// Call from the host maintenance loop. Session callbacks never run under
    /// this manager's mutex, so DB lookup or host callbacks cannot deadlock it.
    pub fn sweep(&self) -> Result<(), ConnectionError> {
        let owners = {
            let mut state = self.lock()?;
            self.expire(&mut state);
            state
                .connections
                .values()
                .filter(|c| c.key.is_some())
                .map(|c| c.owner.clone())
                .collect::<std::collections::HashSet<_>>()
        };
        for p in owners {
            if !self.shared.sessions.is_current(&p) {
                let mut state = self.lock()?;
                for c in state.connections.values_mut().filter(|c| c.owner == p) {
                    if c.key.is_some() {
                        Self::revoke(c, ConnectionState::Revoked)
                    }
                }
            }
        }
        Ok(())
    }
    pub fn revoke_invalid_sessions(&self) -> Result<(), ConnectionError> {
        self.sweep()
    }
    pub fn begin_probe(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        operation: &str,
    ) -> Result<ProbeStart, ConnectionError> {
        self.authenticate(p)?;
        if !valid_text(operation) {
            return Err(ConnectionError::Invalid);
        }
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, p, name, Some(version))?;
        if let Some(record) = c.probes.get(operation) {
            return Ok(match record.state {
                ProbeState::Pending => ProbeStart::Pending,
                ProbeState::Ready => ProbeStart::Ready,
                ProbeState::Failed => ProbeStart::Failed,
            });
        }
        if c.binding.is_some() || c.probes.values().any(|r| r.state == ProbeState::Pending) {
            return Err(ConnectionError::Conflict);
        }
        if c.probes.len() >= self.shared.limits.max_calls as usize {
            return Err(ConnectionError::BudgetExceeded);
        }
        c.probes.insert(
            operation.to_owned(),
            ProbeRecord {
                state: ProbeState::Pending,
                next_phase: 0,
                reserved: false,
            },
        );
        c.state = ConnectionState::Checking;
        Ok(ProbeStart::Operation(ProbeOperationPermit {
            manager: self.clone(),
            principal: p.clone(),
            connection: name.to_owned(),
            version,
            operation: operation.to_owned(),
        }))
    }
    fn reserve_probe_phase(
        &self,
        op: &ProbeOperationPermit,
        phase: u8,
        max_tokens: u64,
    ) -> Result<RequestPermit, ConnectionError> {
        self.authenticate(&op.principal)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, &op.principal, &op.connection, Some(op.version))?;
        let record = c
            .probes
            .get(&op.operation)
            .ok_or(ConnectionError::Unavailable)?;
        if phase > 1
            || record.state != ProbeState::Pending
            || record.next_phase != phase
            || record.reserved
        {
            return Err(ConnectionError::Conflict);
        }
        let result = self.reserve_locked(
            &mut state,
            &op.principal,
            &op.connection,
            op.version,
            None,
            Some((op.operation.clone(), phase)),
            max_tokens,
        );
        if result.is_err() {
            let c = state
                .connections
                .get_mut(&op.connection)
                .ok_or(ConnectionError::Unavailable)?;
            if let Some(record) = c.probes.get_mut(&op.operation) {
                record.state = ProbeState::Failed;
            }
            c.state = ConnectionState::Blocked;
        }
        result
    }
    /// Validate an immutable run epoch independently of management CAS renewals.
    /// Authority callbacks run outside the manager mutex and the local epoch is
    /// checked again afterwards, so Stop cannot race a slow Runtime lookup.
    pub fn check_execution(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        generation: u64,
    ) -> Result<(), ConnectionError> {
        self.authenticate(p)?;
        let target = {
            let mut state = self.lock()?;
            self.expire(&mut state);
            let c = Self::current(&mut state, p, name, None)?;
            if c.active_version != Some(version) || c.generation != generation {
                return Err(ConnectionError::Conflict);
            }
            c.binding.clone().ok_or(ConnectionError::BindingDenied)?
        };
        if !self.shared.bindings.authorize(p, &target) {
            return Err(ConnectionError::BindingDenied);
        }
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, p, name, None)?;
        if c.active_version != Some(version)
            || c.generation != generation
            || c.binding.as_ref() != Some(&target)
        {
            return Err(ConnectionError::Conflict);
        }
        Ok(())
    }
    /// Read non-secret encoding policy only after validating the live run epoch.
    pub(crate) fn active_preset(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        generation: u64,
    ) -> Result<ProviderPreset, ConnectionError> {
        self.check_execution(p, name, version, generation)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, p, name, None)?;
        if c.active_version != Some(version) || c.generation != generation {
            return Err(ConnectionError::Conflict);
        }
        Ok(c.preset.clone())
    }
    pub fn reserve_run(
        &self,
        p: &Principal,
        name: &str,
        version: u64,
        generation: u64,
        max_tokens: u64,
    ) -> Result<RequestPermit, ConnectionError> {
        self.check_execution(p, name, version, generation)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let c = Self::current(&mut state, p, name, None)?;
        if c.active_version != Some(version) || c.generation != generation {
            return Err(ConnectionError::Conflict);
        }
        let target = c.binding.clone().ok_or(ConnectionError::BindingDenied)?;
        self.reserve_locked(&mut state, p, name, version, Some(target), None, max_tokens)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Atomic admission binds owner, connection, CAS version and fixed probe phase in one operation"
    )]
    fn reserve_locked(
        &self,
        state: &mut State,
        p: &Principal,
        name: &str,
        version: u64,
        binding: Option<BindingTarget>,
        probe: Option<(String, u8)>,
        tokens: u64,
    ) -> Result<RequestPermit, ConnectionError> {
        let limits = &self.shared.limits;
        if tokens == 0
            || tokens
                > limits
                    .max_input_tokens
                    .saturating_add(limits.max_output_tokens)
        {
            return Err(ConnectionError::Invalid);
        }
        let budget = state.budgets.get(p).cloned().unwrap_or_default();
        if state.tickets.len() >= 65_536
            || budget.calls >= limits.max_calls
            || budget.in_flight >= limits.max_concurrent
            || budget
                .tokens
                .checked_add(tokens)
                .is_none_or(|total| total > limits.max_tokens)
        {
            return Err(ConnectionError::BudgetExceeded);
        }
        let ticket = id()?;
        if state.tickets.contains_key(&ticket) {
            return Err(ConnectionError::Unavailable);
        }
        let c = Self::current(
            state,
            p,
            name,
            if binding.is_some() {
                None
            } else {
                Some(version)
            },
        )?;
        if binding.is_some() && c.active_version != Some(version) {
            return Err(ConnectionError::Conflict);
        }
        if binding.is_some() && c.state != ConnectionState::Ready {
            return Err(ConnectionError::NotReady);
        }
        let key = Zeroizing::new(c.key.as_ref().ok_or(ConnectionError::Unavailable)?.to_vec());
        let preset = c.preset.clone();
        let generation = c.generation;
        if let Some((operation, _)) = &probe {
            c.probes
                .get_mut(operation)
                .ok_or(ConnectionError::Unavailable)?
                .reserved = true;
        }
        state.budgets.insert(
            p.clone(),
            BudgetSnapshot {
                calls: budget.calls + 1,
                tokens: budget.tokens + tokens,
                in_flight: budget.in_flight + 1,
            },
        );
        state.tickets.insert(
            ticket.clone(),
            Ticket {
                connection: name.to_owned(),
                version,
                generation,
                owner: p.clone(),
                binding: binding.clone(),
                probe: probe.as_ref().map(|(op, _)| op.clone()),
                phase: probe.map(|(_, phase)| phase),
                tokens,
                state: TicketState::Reserved,
            },
        );
        Ok(RequestPermit {
            manager: self.clone(),
            ticket,
            principal: p.clone(),
            binding,
            key,
            preset,
            tokens,
        })
    }
    fn external_check(&self, permit: &RequestPermit) -> Result<(), ConnectionError> {
        if !Arc::ptr_eq(&self.shared, &permit.manager.shared) {
            return Err(ConnectionError::Unavailable);
        }
        self.authenticate(&permit.principal)?;
        if permit
            .binding
            .as_ref()
            .is_some_and(|target| !self.shared.bindings.authorize(&permit.principal, target))
        {
            return Err(ConnectionError::BindingDenied);
        }
        Ok(())
    }
    fn ticket_current(state: &State, ticket: &Ticket) -> bool {
        state.connections.get(&ticket.connection).is_some_and(|c| {
            c.key.is_some()
                && c.owner == ticket.owner
                && if ticket.probe.is_some() {
                    c.version == ticket.version
                } else {
                    c.active_version == Some(ticket.version)
                }
                && c.generation == ticket.generation
                && (ticket.probe.is_some() || c.binding == ticket.binding)
        })
    }
    fn check_permit(&self, permit: &RequestPermit, sent: bool) -> Result<(), ConnectionError> {
        self.external_check(permit)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let ticket = state
            .tickets
            .get(&permit.ticket)
            .ok_or(ConnectionError::Unavailable)?;
        if ticket.state
            != if sent {
                TicketState::Sent
            } else {
                TicketState::Reserved
            }
            || !Self::ticket_current(&state, ticket)
        {
            return Err(ConnectionError::Unavailable);
        }
        Ok(())
    }
    pub fn before_send(&self, permit: &RequestPermit) -> Result<(), ConnectionError> {
        self.external_check(permit)?;
        let mut state = self.lock()?;
        self.expire(&mut state);
        let ticket = state
            .tickets
            .get(&permit.ticket)
            .ok_or(ConnectionError::Unavailable)?;
        if ticket.state != TicketState::Reserved {
            return Err(ConnectionError::AlreadySent);
        }
        if !Self::ticket_current(&state, ticket) {
            return Err(ConnectionError::Unavailable);
        }
        state.tickets.get_mut(&permit.ticket).unwrap().state = TicketState::Sent;
        Ok(())
    }
    /// Settle only after physical completion; unknown/stale usage never refunds.
    pub fn finish(
        &self,
        permit: &RequestPermit,
        usage: Option<u64>,
        probe_verified: bool,
    ) -> Result<(), ConnectionError> {
        let external = self.external_check(permit);
        let mut state = self.lock()?;
        self.expire(&mut state);
        let ticket = state
            .tickets
            .get(&permit.ticket)
            .ok_or(ConnectionError::Unavailable)?;
        if ticket.state == TicketState::Finished {
            return Err(ConnectionError::AlreadyFinished);
        }
        if ticket.state == TicketState::Reserved && (probe_verified || usage.is_some()) {
            return Err(ConnectionError::Invalid);
        }
        let current = external.is_ok() && Self::ticket_current(&state, ticket);
        let owner = ticket.owner.clone();
        let name = ticket.connection.clone();
        let probe = ticket.probe.clone();
        let phase = ticket.phase;
        let tokens = ticket.tokens;
        state.tickets.get_mut(&permit.ticket).unwrap().state = TicketState::Finished;
        if let Some(budget) = state.budgets.get_mut(&owner) {
            budget.in_flight = budget.in_flight.saturating_sub(1);
            if current && let Some(actual) = usage {
                budget.tokens = budget.tokens.saturating_sub(tokens).saturating_add(actual);
            }
        }
        if let Some(c) = state.connections.get_mut(&name) {
            if let Some(operation) = probe {
                let verified = current && probe_verified && usage.is_none_or(|n| n <= tokens);
                if let Some(record) = c.probes.get_mut(&operation) {
                    record.reserved = false;
                    if verified && phase == Some(0) {
                        record.next_phase = 1;
                    } else {
                        record.state = if verified && phase == Some(1) {
                            ProbeState::Ready
                        } else {
                            ProbeState::Failed
                        };
                    }
                }
                if current {
                    c.state = if !verified {
                        ConnectionState::Blocked
                    } else if phase == Some(1) {
                        ConnectionState::Ready
                    } else {
                        ConnectionState::Checking
                    };
                }
            }
            if current && usage.is_some_and(|n| n > tokens) {
                c.state = ConnectionState::Blocked;
                c.binding = None;
                c.active_version = None;
                c.generation = c.generation.saturating_add(1)
            }
        }
        if current && usage.is_some_and(|n| n > tokens) {
            Err(ConnectionError::BudgetExceeded)
        } else if current {
            Ok(())
        } else {
            Err(external.err().unwrap_or(ConnectionError::Unavailable))
        }
    }
}
#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
