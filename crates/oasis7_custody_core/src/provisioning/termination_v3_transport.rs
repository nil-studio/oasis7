//! Independent Term3 physical-port prepayment. One entry is one listed edge,
//! not one opaque batch. Failure/unknown never refunds a paid entry.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationPaidPortV3 {
    SourceBeforeDirectoryActor,
    SourceBeforeFenceService,
    SourceBeforeFenceActor,
    SourceNativeVerifyService,
    SourceNativeVerifyHttp,
    SourceAfterDirectoryActor,
    SourceAfterFenceService,
    SourceAfterFenceActor,
    PermitService,
    PermitAccountActor,
    PermitDirectoryActor,
    PermitFenceService,
    PermitFenceActor,
    RoleFenceService,
    RoleFenceActor,
    ProfileFactoryService,
    ProfileVaultActor,
    PrivateCipherService,
    PrivateCipherActor,
    NativeSessionHttp,
    NativeSubmitHttp,
    NativeLookupHttp,
    WorldSubmitHttp,
    WorldLookupHttp,
    WorldCurrentHttp,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationPaidEdgeV3 {
    pub trusted_attempt_id: String,
    pub port: TerminationPaidPortV3,
    pub charged_at: UnixMillis,
    pub dispatched_at: Option<UnixMillis>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationTransportBookV3 {
    pub limit: DecimalU64,
    pub spent: DecimalU64,
    pub edges: BTreeMap<String, TerminationPaidEdgeV3>,
}
impl TerminationTransportBookV3 {
    pub(super) fn new(limit: DecimalU64) -> Self {
        Self {
            limit,
            spent: DecimalU64(0),
            edges: BTreeMap::new(),
        }
    }
    pub fn remaining(&self) -> Result<u64> {
        self.limit.0.checked_sub(self.spent.0).ok_or_else(invalid)
    }
    pub(super) fn validate(&self, plan: &TerminationPlanV3) -> Result<()> {
        if self.limit != plan.transport_limit
            || self.spent.0 != self.edges.len() as u64
            || self.spent.0 > self.limit.0
        {
            return Err(invalid());
        }
        for (key, edge) in &self.edges {
            if !id(&edge.trusted_attempt_id)
                || key != &edge_key(&edge.trusted_attempt_id, edge.port)?
                || edge.charged_at.get() >= plan.cleanup_until.get()
                || edge.dispatched_at.is_some_and(|t| {
                    t.get() < edge.charged_at.get() || t.get() >= plan.cleanup_until.get()
                })
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn prepay(
        &mut self,
        attempt: &str,
        ports: &[TerminationPaidPortV3],
        now: UnixMillis,
        plan: &TerminationPlanV3,
    ) -> Result<()> {
        if !id(attempt) || now.get() >= plan.cleanup_until.get() {
            return Err(invalid());
        }
        let unique: std::collections::BTreeSet<_> = ports.iter().copied().collect();
        if unique.len() != ports.len() {
            return Err(invalid());
        }
        // Same service attempt callback can repeat; a second actual dispatch
        // must receive a new trusted service attempt id. Caller request/proof ids
        // are never used by the fixed adapter as the service attempt identity.
        let existing = self
            .edges
            .values()
            .filter(|e| e.trusted_attempt_id == attempt)
            .map(|e| e.port)
            .collect::<std::collections::BTreeSet<_>>();
        if !existing.is_empty() {
            return if existing == unique {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if self.remaining()? < ports.len() as u64 {
            return Err(invalid());
        }
        for port in ports {
            self.edges.insert(
                edge_key(attempt, *port)?,
                TerminationPaidEdgeV3 {
                    trusted_attempt_id: attempt.into(),
                    port: *port,
                    charged_at: now,
                    dispatched_at: None,
                },
            );
        }
        self.spent.0 = self
            .spent
            .0
            .checked_add(ports.len() as u64)
            .ok_or_else(invalid)?;
        self.validate(plan)
    }
    pub fn require_paid(&self, attempt: &str, port: TerminationPaidPortV3) -> Result<()> {
        self.edges
            .get(&edge_key(attempt, port)?)
            .filter(|e| e.trusted_attempt_id == attempt && e.port == port)
            .map(|_| ())
            .ok_or_else(invalid)
    }
}
fn edge_key(attempt: &str, port: TerminationPaidPortV3) -> Result<String> {
    digest(&("oasis7.custody.termination-physical-edge.v3", attempt, port))
}
/// An in-process once capability, never JSON or a borrowed request proof.
/// A crash after claim loses this dispatch permission but never refunds its fee.
pub struct TerminationPaidDispatchV3 {
    _attempt: String,
    _port: TerminationPaidPortV3,
}
impl AccountProvisionApprovalState {
    fn prepay_termination_ports_v3(
        &mut self,
        subject: &str,
        attempt: &str,
        ports: &[TerminationPaidPortV3],
        now: UnixMillis,
    ) -> Result<()> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        // Even pending install reserves from this newly signed independent cap,
        // never the legacy TerminationPlan root budget or Execution/Read ledgers.
        saved.plan.validate_at(now.get()).map_err(|_| invalid())?;
        let mut next = self.clone();
        let saved = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        saved.transport.prepay(attempt, ports, now, &saved.plan)?;
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn claim_termination_paid_dispatch_v3(
        &mut self,
        subject: &str,
        attempt: &str,
        port: TerminationPaidPortV3,
        now: UnixMillis,
    ) -> Result<TerminationPaidDispatchV3> {
        let mut next = self.clone();
        let saved = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        if now.get() >= saved.plan.cleanup_until.get() {
            return Err(invalid());
        }
        let edge = saved
            .transport
            .edges
            .get_mut(&edge_key(attempt, port)?)
            .ok_or_else(invalid)?;
        if edge.dispatched_at.is_some() || now.get() < edge.charged_at.get() {
            return Err(ProvisionApprovalError::Conflict);
        }
        edge.dispatched_at = Some(now);
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(TerminationPaidDispatchV3 {
            _attempt: attempt.into(),
            _port: port,
        })
    }
    pub fn prepay_termination_source_v3(
        &mut self,
        subject: &str,
        attempt: &str,
        now: UnixMillis,
    ) -> Result<()> {
        use TerminationPaidPortV3::*;
        self.prepay_termination_ports_v3(
            subject,
            attempt,
            &[
                SourceBeforeDirectoryActor,
                SourceBeforeFenceService,
                SourceBeforeFenceActor,
                SourceNativeVerifyService,
                SourceNativeVerifyHttp,
                SourceAfterDirectoryActor,
                SourceAfterFenceService,
                SourceAfterFenceActor,
            ],
            now,
        )
    }
    pub fn prepay_termination_owner_observation_v3(
        &mut self,
        subject: &str,
        attempt: &str,
        now: UnixMillis,
    ) -> Result<()> {
        use TerminationPaidPortV3::*;
        self.prepay_termination_ports_v3(
            subject,
            attempt,
            &[
                PermitService,
                PermitAccountActor,
                PermitDirectoryActor,
                PermitFenceService,
                PermitFenceActor,
                RoleFenceService,
                RoleFenceActor,
            ],
            now,
        )
    }
    pub fn prepay_termination_profile_dispatch_v3(
        &mut self,
        subject: &str,
        attempt: &str,
        now: UnixMillis,
    ) -> Result<()> {
        self.prepay_termination_ports_v3(
            subject,
            attempt,
            &[
                TerminationPaidPortV3::ProfileFactoryService,
                TerminationPaidPortV3::ProfileVaultActor,
            ],
            now,
        )
    }
}
