//! Reverse checks for both paid-pending and Current-admitted initial stages.
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
pub(super) fn intent_ids(intent: &ExecutionPreparePaidIntentV2) -> [&str; 5] {
    let a = &intent.allocation;
    [
        &a.operation_id,
        &a.factory_ingress_id,
        &a.client_nonce,
        &a.observation_nonce,
        &a.gateway_nonce,
    ]
}
pub(in crate::provisioning) fn prepare_paid_id_in_use_v2(
    s: &TestSessionAuthorityV2,
    id: &str,
) -> bool {
    s.purpose_transports.values().any(|row| match row {
        SessionPurposeTransportV1::ExecutionFactoryCallbackV2(paid) => {
            intent_ids(&paid.intent).contains(&id)
        }
        _ => false,
    })
}
fn old_namespace_in_use(s: &TestSessionAuthorityV2, id: &str) -> bool {
    s.sources.contains_key(id)
        || s.source_funding.contains_key(id)
        || s.purpose_transports.contains_key(id)
        || s.purpose_http_admissions.contains_key(id)
        || s.control_transports.contains_key(id)
        || s.client_http_admissions.contains_key(id)
        || super::super::test_session::execution_old_claim_id_in_use_v2(s, id)
        || super::super::test_session::execution_old_control_id_in_use_v2(s, id)
        || s.sources.values().any(|v| v.source.verify_nonce == id)
        || s.tool_nonce_ingresses.contains_key(id)
        || s.termination_state.transport_id_in_use(id)
}
impl AccountProvisionApprovalState {
    pub(super) fn validate_execution_prepare_v2(
        &self,
        s: &TestSessionAuthorityV2,
        b: &SessionExecutionBookV2,
    ) -> Result<()> {
        for (key, row) in &s.purpose_transports {
            if let SessionPurposeTransportV1::ExecutionPrepareRkvV2(port) = row {
                super::rkv_ports_v2::parent_valid(s, key, port)?;
            }
            if let SessionPurposeTransportV1::ExecutionPrepareCurrentV2(port) = row {
                super::execution_current_ports_v2::current_port_parent_valid(s, key, port)?;
            }
        }
        let rows: Vec<_> = s
            .purpose_transports
            .iter()
            .filter_map(|(key, row)| match row {
                SessionPurposeTransportV1::ExecutionFactoryCallbackV2(paid) => {
                    Some((key, paid.as_ref(), paid.intent.as_ref()))
                }
                _ => None,
            })
            .collect();
        if rows.is_empty() {
            return if b.last_allocation_sequence.0 == 0 && b.stages.is_empty() {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let [(key, paid, intent)] = rows.as_slice() else {
            return Err(invalid());
        };
        if intent.ledger_key(s, &paid.signed_attempt_digest)? != **key
            || key.len() <= 256
            || paid.kind != ExecutionPrepareFactoryKindV2::IdentityToAccountPrepare
            || !hex64(&paid.signed_attempt_digest)
            || intent.action_request_id != b.action_request_id
            || intent.approved_input_digest != b.approved_input_digest
            || intent.source_operation_id != b.source_operation_id
            || intent.allocation_sequence.0 != 1
            || b.last_allocation_sequence != intent.allocation_sequence
            || b.lifetime_admitted_at
                .is_none_or(|at| at.get() > paid.claimed_at.get())
            || paid.claimed_at.get() >= b.execution_deadline.get()
            || s.source_oracle_reads
                .get(&b.source_operation_id)
                .is_none_or(|n| *n < 4)
        {
            return Err(invalid());
        }
        super::execution_current_ports_v2::validate_current_ports(s, key, paid)?;
        let mut ids = std::collections::BTreeSet::new();
        for id in intent_ids(intent) {
            if id.is_empty()
                || id.len() > 256
                || id.chars().any(char::is_control)
                || !ids.insert(id)
                || old_namespace_in_use(s, id)
                || id == b.session_id
                || id == b.source_operation_id
                || id == b.lifetime_transport_id
                || b.archived_lifetime
                    .as_ref()
                    .is_some_and(|v| v.lifetime.status.nonce == id)
                || s.native_lifetime
                    .as_ref()
                    .is_some_and(|v| v.lifetime.status.nonce == id)
                || s.preflight_lifetime
                    .as_ref()
                    .is_some_and(|v| v.lifetime.status.nonce == id)
            {
                return Err(invalid());
            }
        }
        for nonce in [
            &intent.allocation.client_nonce,
            &intent.allocation.observation_nonce,
            &intent.allocation.gateway_nonce,
        ] {
            if !hex64(nonce) {
                return Err(invalid());
            }
        }
        // A paid pending record is valid without another signature or a grant.
        // Unknown second-SQL outcome never erases its operation/nonces.
        if b.stages.is_empty() {
            return Ok(());
        }
        if b.stages.len() != 1 {
            return Err(invalid());
        }
        let stage = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        let a = &stage.allocation;
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let source_fact = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        let observed = source_fact.observed_at.0;
        if a.operation_id != intent.allocation.operation_id
            || a.factory_ingress_id != **key
            || a.allocation_sequence != intent.allocation_sequence
            || a.source_operation_id != intent.source_operation_id
            || a.source_digest != digest(source)?
            || a.source_observed_at.get() != observed
            || a.source_expires_at.get() != source.source.expires_at.0
            || a.allocated_at != paid.claimed_at
            || a.client_nonce.as_ref() != Some(&intent.allocation.client_nonce)
            || a.observation_nonce.as_ref() != Some(&intent.allocation.observation_nonce)
            || a.gateway_nonce != intent.allocation.gateway_nonce
            || a.predecessor_digest.is_some()
            || stage.signing_admitted_at.get() < a.allocated_at.get()
            || stage.signing_admitted_at.get() < observed
            || stage.signing_admitted_at.get() - observed >= 5000
            || stage.signing_admitted_at.get() >= source_fact.source_session_expires_at.0
            || stage.signing_admitted_at.get() >= source_fact.source_session_idle_expires_at.0
            || stage.signing_admitted_at.get() >= source.source.expires_at.0
            || stage.signing_admitted_at.get() >= b.execution_deadline.get()
            || s.source_oracle_reads
                .get(&b.source_operation_id)
                .is_none_or(|n| *n < 5)
            || stage.original.is_some()
            || stage.send_claim.is_some()
            || !stage.lookup_claims.is_empty()
            || stage.archived_fact_cbor_hex.is_some()
            || stage.admitted_at.is_some()
        {
            return Err(invalid());
        }
        // This branch is an admitted signing stage, not the earlier durable
        // paid intent. Reuse the same strong LF semantic validator at its exact
        // historical admission time; late paid facts themselves stay archivable.
        b.archived_lifetime
            .as_ref()
            .ok_or_else(invalid)?
            .lifetime
            .validate_for(
                &s.native_lifetime
                    .as_ref()
                    .ok_or_else(invalid)?
                    .lifetime
                    .status
                    .receipt,
                &self.test_session_v2_lifetime_nonce(&b.session_id, &b.source_operation_id)?,
                s.session.native_binding_generation,
                s.input.request.context.service_epoch,
                stage.signing_admitted_at.get(),
            )
            .map_err(|_| invalid())?;
        let expected = b.prepare_bundle(s, a, self.test_session_v2_world_read(&s.input)?)?;
        if stage.candidate_digest != expected.digest()? {
            return Err(invalid());
        }
        if let Some(record) = &stage.service_record {
            super::rkv_ports_v2::validate_ports(s, record)?;
            if paid.current_read.is_none() {
                return Err(invalid());
            }
            let scope = crate::role_key_slot::RoleKeySlotScopeV1::from_task(
                self.role_key_tasks.get("executor").ok_or_else(invalid)?,
            )
            .map_err(|_| invalid())?;
            let public = self
                .provision
                .role_key(Role::Executor)
                .ok_or_else(invalid)?;
            super::prepare_service_v2::service_shape(
                record,
                &expected,
                &scope,
                public,
                stage.signing_admitted_at,
            )?;
        }
        Ok(())
    }
}
