//! Three demonstrated Current edges, separate from the paid factory ingress.
//! This is not a full execution graph, a signer permission or Native160 charge.
use super::super::test_session::{SessionEventKindV2, append_session_event_v2};
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareCurrentPortV2 {
    ManagerDirectory,
    RecoveryFenceReader,
    RecoveryFenceObject,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareCurrentTransportKindV2 {
    ExecutionPrepareCurrentV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareCurrentTransportV2 {
    pub kind: ExecutionPrepareCurrentTransportKindV2,
    pub port: ExecutionPrepareCurrentPortV2,
    pub factory_key: String,
    pub source_operation_id: String,
    pub approved_input_digest: String,
    pub source_ticket: u8,
    pub claimed_at: UnixMillis,
}
impl core::fmt::Debug for ExecutionPrepareCurrentTransportV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPrepareCurrentTransportV2(<paid fixed port>)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareCurrentReadV2 {
    pub source_operation_id: String,
    pub source_ticket: u8,
    pub claimed_at: UnixMillis,
    pub port_ids: [String; 3],
}
/// Returned only on the first actual durable reservation. No wire/Clone/Debug.
/// Actual Worker code consumes it to perform exactly Directory1 + RF2.
pub struct ExecutionPrepareCurrentPortsV2 {
    read: ExecutionPrepareCurrentReadV2,
    session_id: String,
    approved_input_digest: String,
    factory_key: String,
    manager: ManagerPrincipal,
    environment: String,
    expires_at: UnixMillis,
    source_nonce: String,
}
impl ExecutionPrepareCurrentPortsV2 {
    pub fn read(&self) -> &ExecutionPrepareCurrentReadV2 {
        &self.read
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn approved_input_digest(&self) -> &str {
        &self.approved_input_digest
    }
    pub fn factory_key(&self) -> &str {
        &self.factory_key
    }
    pub fn manager(&self) -> &ManagerPrincipal {
        &self.manager
    }
    pub fn environment(&self) -> &str {
        &self.environment
    }
    pub fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
    pub fn source_nonce(&self) -> &str {
        &self.source_nonce
    }
}
const PORTS: [ExecutionPrepareCurrentPortV2; 3] = [
    ExecutionPrepareCurrentPortV2::ManagerDirectory,
    ExecutionPrepareCurrentPortV2::RecoveryFenceReader,
    ExecutionPrepareCurrentPortV2::RecoveryFenceObject,
];
pub(super) fn port_id(
    s: &TestSessionAuthorityV2,
    factory: &str,
    port: ExecutionPrepareCurrentPortV2,
) -> Result<String> {
    Ok(format!(
        "execution-current-v2:{}:{}:{}:{}",
        digest(&(
            "execution-current-session-v2",
            &s.input.operation_id,
            s.input_digest()?
        ))?,
        digest(&("execution-current-factory-v2", factory))?,
        digest(&("execution-current-port-v2", port))?,
        digest(&(
            "execution-current-source-v2",
            s.execution_book
                .as_ref()
                .ok_or_else(invalid)?
                .source_operation_id
                .as_str(),
            5u8
        ))?
    ))
}
impl AccountProvisionApprovalState {
    /// Same Account RPC first SQL: ingress1 is separately committed by the
    /// existing fee method; if this additional reservation fails, preserve that
    /// fee state. Commit all three claims before the fixed reads. A repeat only
    /// reads paid metadata and returns None, never another outbound token.
    pub fn claim_test_session_v2_prepare_current_ports(
        &mut self,
        session_id: &str,
        factory_key: &str,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPrepareCurrentPortsV2>> {
        let s = self.test_session_v2_record(session_id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory) =
            s.purpose_transports.get(factory_key).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        if factory.intent.action_request_id != b.action_request_id
            || factory.intent.approved_input_digest != s.input_digest()?
            || factory.intent.source_operation_id != b.source_operation_id
            || factory
                .intent
                .ledger_key(s, &factory.signed_attempt_digest)?
                != factory_key
        {
            return Err(invalid());
        }
        if factory.current_read.is_some() {
            return Ok(None);
        }
        self.test_session_v2_local(session_id, SessionPurposeV1::Execution, now)?;
        if !b.stages.is_empty()
            || b.lifetime_admitted_at.is_none()
            || b.credential_generation
                != s.roots
                    .get(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .credential
                    .as_ref()
                    .ok_or_else(invalid)?
                    .generation
            || s.source_oracle_reads.get(&b.source_operation_id) != Some(&4)
        {
            return Err(invalid());
        }
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let v = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        let lf = &b
            .archived_lifetime
            .as_ref()
            .ok_or_else(invalid)?
            .lifetime
            .status;
        let until = [
            source.source.expires_at.0,
            v.observed_at.0.checked_add(5000).ok_or_else(invalid)?,
            v.source_session_expires_at.0,
            v.source_session_idle_expires_at.0,
            lf.expires_at.0,
            b.execution_deadline.get(),
            s.lease_until.ok_or_else(invalid)?.get(),
        ]
        .into_iter()
        .min()
        .ok_or_else(invalid)?;
        if now.get() >= until {
            return Err(invalid());
        }
        let ids = [
            port_id(s, factory_key, PORTS[0])?,
            port_id(s, factory_key, PORTS[1])?,
            port_id(s, factory_key, PORTS[2])?,
        ];
        let mut unique = std::collections::BTreeSet::new();
        for id in &ids {
            if !unique.insert(id)
                || s.purpose_transports.contains_key(id)
                || s.purpose_http_admissions.contains_key(id)
                || s.sources.contains_key(id)
                || s.source_funding.contains_key(id)
                || s.control_transports.contains_key(id)
                || s.client_http_admissions.contains_key(id)
                || s.termination_state.transport_id_in_use(id)
                || b.transport_id_in_use(id)
            {
                return Err(ProvisionApprovalError::Conflict);
            }
        }
        let record = ExecutionPrepareCurrentReadV2 {
            source_operation_id: b.source_operation_id.clone(),
            source_ticket: 5,
            claimed_at: now,
            port_ids: ids,
        };
        let source_nonce = source.source.verify_nonce.clone();
        let approved_input_digest = s.input_digest()?;
        let manager = self.draft.entry.manager.clone();
        let environment = self.draft.entry.environment.clone();
        let mut next = self.clone();
        // Ticket is only an ordinal/once reservation. These three real port
        // claims supply the separate transport funding; no Native160 is used.
        if next.reserve_test_session_v2_source_read(session_id, &record.source_operation_id, now)?
            != 5
        {
            return Err(invalid());
        }
        let saved = next.test_session_v2_record_mut(session_id)?;
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(3)
            .map_err(|_| invalid())?;
        for (port, id) in PORTS.into_iter().zip(&record.port_ids) {
            saved.purpose_transports.insert(
                id.clone(),
                SessionPurposeTransportV1::ExecutionPrepareCurrentV2(Box::new(
                    ExecutionPrepareCurrentTransportV2 {
                        kind: ExecutionPrepareCurrentTransportKindV2::ExecutionPrepareCurrentV2,
                        port,
                        factory_key: factory_key.into(),
                        source_operation_id: record.source_operation_id.clone(),
                        approved_input_digest: approved_input_digest.clone(),
                        source_ticket: 5,
                        claimed_at: now,
                    },
                )),
            );
        }
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
            saved.purpose_transports.get_mut(factory_key)
        else {
            return Err(invalid());
        };
        factory.current_read = Some(record.clone());
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPrepareCurrentPortsClaimed {
                request_id: b.action_request_id.clone(),
            },
            now,
        )?;
        if saved
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && saved.closing.is_none()
        {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(Some(ExecutionPrepareCurrentPortsV2 {
            read: record,
            session_id: session_id.into(),
            approved_input_digest,
            factory_key: factory_key.into(),
            manager,
            environment,
            expires_at: UnixMillis::new(until),
            source_nonce,
        }))
    }
}

/// Closed class reverse checks. No record can be silently downgraded to a
/// generic paid port, and an orphan port never creates a Current observation.
pub(super) fn validate_current_ports(
    s: &TestSessionAuthorityV2,
    factory_key: &str,
    factory: &ExecutionPrepareFactoryTransportV2,
) -> Result<()> {
    let Some(read) = &factory.current_read else {
        return if s.purpose_transports.values().any(|row| {
            matches!(row, SessionPurposeTransportV1::ExecutionPrepareCurrentV2(p)
                if p.factory_key == factory_key)
        }) {
            Err(invalid())
        } else {
            Ok(())
        };
    };
    let b = s.execution_book.as_ref().ok_or_else(invalid)?;
    if read.source_operation_id != b.source_operation_id
        || read.source_ticket != 5
        || read.claimed_at.get() < factory.claimed_at.get()
        || read.claimed_at.get() >= b.execution_deadline.get()
        || s.source_oracle_reads
            .get(&b.source_operation_id)
            .is_none_or(|n| *n < 5)
    {
        return Err(invalid());
    }
    // The complete Book validator has already reconstructed and strongly
    // verified this exact archived LF (including its deterministic nonce).
    // Here replay the same semantic lifetime contract at the actual port claim,
    // not merely at the earlier LF-admitted or later signing-stage timestamps.
    let lifetime = &b.archived_lifetime.as_ref().ok_or_else(invalid)?.lifetime;
    lifetime
        .validate_for(
            &s.native_lifetime
                .as_ref()
                .ok_or_else(invalid)?
                .lifetime
                .status
                .receipt,
            &lifetime.status.nonce,
            s.session.native_binding_generation,
            s.input.request.context.service_epoch,
            read.claimed_at.get(),
        )
        .map_err(|_| invalid())?;
    let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
    let fact = &source
        .source
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .verification;
    if read.claimed_at.get() < fact.observed_at.0
        || read.claimed_at.get() - fact.observed_at.0 >= 5000
        || read.claimed_at.get() >= source.source.expires_at.0
        || read.claimed_at.get() >= fact.source_session_expires_at.0
        || read.claimed_at.get() >= fact.source_session_idle_expires_at.0
    {
        return Err(invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for (port, key) in PORTS.into_iter().zip(&read.port_ids) {
        if key != &port_id(s, factory_key, port)? || key.len() <= 256 || !seen.insert(key) {
            return Err(invalid());
        }
        let Some(SessionPurposeTransportV1::ExecutionPrepareCurrentV2(p)) =
            s.purpose_transports.get(key)
        else {
            return Err(invalid());
        };
        if p.kind != ExecutionPrepareCurrentTransportKindV2::ExecutionPrepareCurrentV2
            || p.port != port
            || p.factory_key != factory_key
            || p.source_operation_id != read.source_operation_id
            || p.approved_input_digest != s.input_digest()?
            || p.source_ticket != read.source_ticket
            || p.claimed_at != read.claimed_at
        {
            return Err(invalid());
        }
    }
    // Exact inventory, not merely presence of the three expected IDs.
    if s.purpose_transports
        .iter()
        .filter(|(_, row)| {
            matches!(row, SessionPurposeTransportV1::ExecutionPrepareCurrentV2(p)
            if p.factory_key == factory_key)
        })
        .count()
        != PORTS.len()
    {
        return Err(invalid());
    }
    Ok(())
}
/// Called for every typed leaf, including when the parent map was stripped.
pub(super) fn current_port_parent_valid(
    s: &TestSessionAuthorityV2,
    key: &str,
    port: &ExecutionPrepareCurrentTransportV2,
) -> Result<()> {
    let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
        s.purpose_transports.get(&port.factory_key)
    else {
        return Err(invalid());
    };
    validate_current_ports(s, &port.factory_key, factory)?;
    if !factory
        .current_read
        .as_ref()
        .is_some_and(|r| r.port_ids.iter().any(|id| id == key))
    {
        return Err(invalid());
    }
    Ok(())
}
