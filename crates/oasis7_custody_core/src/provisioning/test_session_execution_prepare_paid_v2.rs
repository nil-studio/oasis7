//! First durable SQL half of the fixed Prepare factory callback.
//! A paid allocation intent survives later Current/crypto failure; it is not a
//! signature/use grant. Account's second SQL must still rebuild and admit it.
use super::super::test_session::{
    SessionEventKindV2, append_session_event_v2, authenticated_receipt_v2,
};
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPreparePaidIntentV2 {
    pub allocation: ExecutionPrepareAllocationV2,
    pub allocation_sequence: DecimalU64,
    pub action_request_id: String,
    pub approved_input_digest: String,
    pub source_operation_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareFactoryKindV2 {
    IdentityToAccountPrepare,
    OwnerServiceAndAccountV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareFactoryTransportV2 {
    pub kind: ExecutionPrepareFactoryKindV2,
    pub intent: Box<ExecutionPreparePaidIntentV2>,
    pub signed_attempt_digest: String,
    pub claimed_at: UnixMillis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_read: Option<ExecutionPrepareCurrentReadV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_current: Option<ExecutionPrivateCurrentReadV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_ports: Option<[ExecutionPrepareFactoryPortV2; 2]>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareFactoryHopV2 {
    IdentityToOwnerService,
    OwnerServiceToAccountActor,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareFactoryPortV2 {
    pub hop: ExecutionPrepareFactoryHopV2,
    pub physical_id: String,
}
impl ExecutionPrepareFactoryKindV2 {
    pub(in crate::provisioning) fn units(self) -> u64 {
        match self {
            Self::IdentityToAccountPrepare => 1,
            Self::OwnerServiceAndAccountV2 => 2,
        }
    }
}
impl ExecutionPrepareFactoryTransportV2 {
    fn expected_factory_ports(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<Option<[ExecutionPrepareFactoryPortV2; 2]>> {
        if self.kind == ExecutionPrepareFactoryKindV2::IdentityToAccountPrepare {
            return Ok(None);
        }
        let parent = self.intent.ledger_key(s, &self.signed_attempt_digest)?;
        let make = |hop| -> Result<ExecutionPrepareFactoryPortV2> {
            Ok(ExecutionPrepareFactoryPortV2 {
                hop,
                physical_id: format!(
                    "execution-factory-hop-v2:{}",
                    digest(&(
                        "oasis7.custody.execution-factory-hop.v2",
                        self.kind,
                        &s.input.operation_id,
                        &self.intent.approved_input_digest,
                        &parent,
                        &self.signed_attempt_digest,
                        &self.intent,
                        hop,
                    ))?
                ),
            })
        };
        Ok(Some([
            make(ExecutionPrepareFactoryHopV2::IdentityToOwnerService)?,
            make(ExecutionPrepareFactoryHopV2::OwnerServiceToAccountActor)?,
        ]))
    }
    pub(in crate::provisioning) fn validate_factory_profile(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        if self.factory_ports != self.expected_factory_ports(s)? {
            return Err(invalid());
        }
        Ok(())
    }
}
impl core::fmt::Debug for ExecutionPreparePaidIntentV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPreparePaidIntentV2(<public allocation commitment>)")
    }
}
impl ExecutionPreparePaidIntentV2 {
    /// New class-only key in an old-unreachable length domain. Four actual
    /// semantic digests, no padding/control bytes; a Generic downgrade is
    /// therefore not a valid legacy <=256-byte purpose admission.
    pub(in crate::provisioning) fn ledger_key(
        &self,
        s: &TestSessionAuthorityV2,
        signed_attempt: &str,
    ) -> Result<String> {
        Ok(format!(
            "execution-prepare-v2:{}:{}:{}:{}",
            digest(&(
                "execution-prepare-book-v2",
                &s.input.operation_id,
                &self.action_request_id,
                &self.approved_input_digest
            ))?,
            digest(&(
                "execution-prepare-ingress-v2",
                &self.allocation.factory_ingress_id
            ))?,
            digest(&("execution-prepare-rfc-v2", signed_attempt))?,
            digest(&("execution-prepare-allocation-v2", self))?
        ))
    }
}
impl AccountProvisionApprovalState {
    pub fn charge_test_session_v2_execution_prepare_attempt(
        &mut self,
        session_id: &str,
        allocation: &ExecutionPrepareAllocationV2,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<String> {
        self.charge_execution_prepare_factory_inner(
            session_id,
            allocation,
            proof,
            now,
            ExecutionPrepareFactoryKindV2::IdentityToAccountPrepare,
        )
    }
    /// Actual fixed Owner named method plus its one Account Actor call.
    /// This does not upgrade or reuse a pre-existing legacy one-fee record.
    pub fn charge_test_session_v2_execution_prepare_owner_factory_attempt(
        &mut self,
        session_id: &str,
        allocation: &ExecutionPrepareAllocationV2,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<String> {
        self.charge_execution_prepare_factory_inner(
            session_id,
            allocation,
            proof,
            now,
            ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2,
        )
    }
    fn charge_execution_prepare_factory_inner(
        &mut self,
        session_id: &str,
        allocation: &ExecutionPrepareAllocationV2,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
        kind: ExecutionPrepareFactoryKindV2,
    ) -> Result<String> {
        let q = super::super::test_session::action_original_v2(&proof)?;
        let s = self.test_session_v2_record(session_id)?;
        authenticated_receipt_v2(s, SessionPurposeV1::Execution, proof.possession, now)?;
        super::super::test_session::execution_target_v2(
            self,
            s,
            SessionPurposeV1::Execution,
            proof.possession,
        )?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        if q.session_id != session_id
            || q.request_id != b.action_request_id
            || q.purpose() != SessionPurposeV1::Execution
            || q.action != SessionToolActionV1::Execute(s.header()?.0.actions[0].clone())
            || proof.ingress_attempt_id != allocation.factory_ingress_id
        {
            return Err(invalid());
        }
        let signed_attempt = proof.possession.transport_attempt_digest();
        // Original class is permanent. Same ingress can only read the exact
        // paid fact, without creating a replacement nonce or refreshing time.
        for (key, row) in &s.purpose_transports {
            if let SessionPurposeTransportV1::ExecutionFactoryCallbackV2(paid) = row {
                let intent = &paid.intent;
                if intent.allocation.factory_ingress_id == proof.ingress_attempt_id {
                    return if intent.allocation == *allocation
                        && intent.action_request_id == q.request_id
                        && intent.approved_input_digest == s.input_digest()?
                        && paid.signed_attempt_digest == signed_attempt
                        && paid.kind == kind
                        && intent.ledger_key(s, &signed_attempt)? == *key
                    {
                        Ok(key.clone())
                    } else {
                        Err(ProvisionApprovalError::Conflict)
                    };
                }
            }
        }
        self.test_session_v2_local(session_id, SessionPurposeV1::Execution, now)?;
        if b.lifetime_admitted_at.is_none()
            || !b.stages.is_empty()
            || b.last_allocation_sequence.0 != 0
            || s.source_oracle_reads
                .get(&b.source_operation_id)
                .is_none_or(|n| *n < 4 || *n > 8)
            || s.roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .credential
                .as_ref()
                .ok_or_else(invalid)?
                .generation
                != b.credential_generation
        {
            return Err(invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        for id in [
            &allocation.operation_id,
            &allocation.factory_ingress_id,
            &allocation.client_nonce,
            &allocation.observation_nonce,
            &allocation.gateway_nonce,
        ] {
            if id.is_empty()
                || id.len() > 256
                || id.chars().any(char::is_control)
                || !seen.insert(id)
            {
                return Err(invalid());
            }
            self.test_session_v2_transport_id_available(session_id, id)?;
            if s.native_lifetime
                .as_ref()
                .is_some_and(|v| v.lifetime.status.nonce == *id)
                || s.preflight_lifetime
                    .as_ref()
                    .is_some_and(|v| v.lifetime.status.nonce == *id)
            {
                return Err(ProvisionApprovalError::Conflict);
            }
        }
        for nonce in [
            &allocation.client_nonce,
            &allocation.observation_nonce,
            &allocation.gateway_nonce,
        ] {
            if nonce.len() != 64
                || !nonce
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid());
            }
        }
        let intent = ExecutionPreparePaidIntentV2 {
            allocation: allocation.clone(),
            allocation_sequence: DecimalU64(
                b.last_allocation_sequence
                    .0
                    .checked_add(1)
                    .ok_or_else(invalid)?,
            ),
            action_request_id: b.action_request_id.clone(),
            approved_input_digest: s.input_digest()?,
            source_operation_id: b.source_operation_id.clone(),
        };
        let key = intent.ledger_key(s, &signed_attempt)?;
        if s.purpose_transports.contains_key(&key) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut paid_record = ExecutionPrepareFactoryTransportV2 {
            kind,
            intent: Box::new(intent.clone()),
            signed_attempt_digest: signed_attempt.clone(),
            claimed_at: now,
            current_read: None,
            private_current: None,
            factory_ports: None,
        };
        paid_record.factory_ports = paid_record.expected_factory_ports(s)?;
        if let Some(ports) = &paid_record.factory_ports {
            if ports[0].physical_id == ports[1].physical_id {
                return Err(invalid());
            }
            for port in ports {
                self.test_session_v2_transport_id_available(session_id, &port.physical_id)?;
            }
        }
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(kind.units())
            .map_err(|_| invalid())?;
        saved
            .execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .last_allocation_sequence = intent.allocation_sequence;
        saved.purpose_transports.insert(
            key.clone(),
            SessionPurposeTransportV1::ExecutionFactoryCallbackV2(Box::new(paid_record)),
        );
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPrepareFactoryPaid {
                request_id: q.request_id,
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
        Ok(key)
    }
}
