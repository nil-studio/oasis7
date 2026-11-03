//! Ref-only persistence consistency, never a reconstructed live admission.
use super::*;
fn external_id(s: &TestSessionAuthorityV1, value: &str) -> bool {
    s.sources.contains_key(value)
        || s.source_funding.contains_key(value)
        || s.purpose_http_admissions.contains_key(value)
        || s.control_transports.contains_key(value)
        || s.client_http_admissions.contains_key(value)
        || s.actions.contains_key(value)
        || s.actions
            .values()
            .any(|v| v.transport_attempts.iter().any(|a| a == value))
        || s.termination_slots
            .values()
            .any(|v| v.operation_id == value || v.transport_attempts.iter().any(|a| a == value))
        || s.tool_nonce_ingresses
            .values()
            .any(|v| v.owner_attempt == value || v.account_attempt == value)
        || s.context.as_ref().is_some_and(|c| {
            c.original.request.client_operation_id == value || c.attempts.contains_key(value)
        })
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn live_context_transport_keys_valid(
        s: &TestSessionAuthorityV1,
    ) -> bool {
        s.purpose_transports.iter().all(|(key, value)| match value {
            SessionPurposeTransportV1::LiveContextRead(marker) => {
                Self::live_context_transport_key_matches(s, key, marker).unwrap_or(false)
            }
            // Exactly the unchanged legacy ID rule, never a namespace reservation.
            _ => !key.is_empty() && key.len() <= 256 && !key.chars().any(char::is_control),
        })
    }
    pub(in crate::provisioning) fn live_context_transport_key_matches(
        s: &TestSessionAuthorityV1,
        key: &str,
        value: &LiveContextReadTransportV1,
    ) -> Result<bool> {
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let record = c.live_reads.get(&value.claim_id).ok_or_else(invalid)?;
        let reference = c
            .slots
            .get(&ContextBodySlotKindV1::Captured)
            .ok_or_else(invalid)?;
        Ok(key.len() == 275
            && key.is_ascii()
            && key == port_id(&value.claim_id, value.port, record, reference)?
            && s.purpose_transports.get(key)
                == Some(&marker(&value.claim_id, value.port, record, reference)?))
    }
    pub(super) fn validate_live_context_historical_reads(
        &self,
        id: &str,
        s: &TestSessionAuthorityV1,
        record: &LiveContextReadRecordV1,
    ) -> Result<()> {
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let r = c
            .slots
            .get(&ContextBodySlotKindV1::Captured)
            .ok_or_else(invalid)?;
        if record.historical_reads[0] == record.historical_reads[1]
            || r.scope != self.context_scope(id, ContextBodySlotKindV1::Captured)?
        {
            return Err(invalid());
        }
        let mut ports = Vec::new();
        for key in &record.historical_reads {
            let read = c.attempts.get(key).ok_or_else(invalid)?;
            let fact = read.observation.as_ref().ok_or_else(invalid)?;
            let stored = fact.stored.as_ref().ok_or_else(invalid)?;
            if read.slot != ContextBodySlotKindV1::Captured
                || read.purpose != SessionPurposeV1::Execution
                || read.source_operation_id != record.source_operation_id
                || read.claimed_at.get() > record.claimed_at.get()
                || fact.scope != r.scope
                || stored.envelope_digest != r.envelope_digest
                || stored.record_wire_bytes != r.record_wire_bytes
                || s.purpose_transports.get(key) != Some(&context_transport(c, read, r)?)
            {
                return Err(invalid());
            }
            ports.push(read.port);
        }
        if !matches!(
            ports.as_slice(),
            [ContextBodyPortV1::ReadBefore, ContextBodyPortV1::ReadAfter]
                | [
                    ContextBodyPortV1::RecoverBody,
                    ContextBodyPortV1::RecoverBody
                ]
        ) {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::provisioning) fn validate_live_context_persistence(
        &self,
        id: &str,
        s: &TestSessionAuthorityV1,
    ) -> Result<()> {
        let Some(c) = &s.context else {
            return if s
                .purpose_transports
                .values()
                .any(|v| matches!(v, SessionPurposeTransportV1::LiveContextRead(_)))
            {
                Err(invalid())
            } else {
                Ok(())
            };
        };
        if c.live_reads.len() > 400 / 3 {
            return Err(invalid());
        }
        let mut expected_ids = BTreeSet::new();
        let mut nonces = BTreeSet::new();
        for (attempt, record) in &c.live_reads {
            self.validate_live_context_historical_reads(id, s, record)?;
            let r = c
                .slots
                .get(&ContextBodySlotKindV1::Captured)
                .ok_or_else(invalid)?;
            let root = s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?;
            let source = s
                .sources
                .get(&record.source_operation_id)
                .ok_or_else(invalid)?;
            let signed = source.verification.as_ref().ok_or_else(invalid)?;
            let gateway = record
                .gateway_read
                .observation_nonce
                .as_ref()
                .ok_or_else(invalid)?;
            verify_live_context_read(&record.request).map_err(|_| invalid())?;
            let expected_gateway = ws::ReadWorldViewRequest {
                contract_version: 1,
                observation_nonce: Some(gateway.clone()),
                formal_anchor: None,
                world: ws::WorldIdentity {
                    world_id: r.scope.world.world_id.clone(),
                    genesis_digest: r.scope.world.genesis_digest.clone(),
                },
                scope_id: format!(
                    "agent:{}",
                    record.request.original.request.context.fence.agent_id
                ),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            };
            if !context_attempt_id(attempt)
                || external_id(s, attempt)
                || s.purpose_transports.contains_key(attempt)
                || record.request.original != *c.get.as_ref().ok_or_else(invalid)?
                || record.reference_digest != reference_digest(r)?
                || record.deadline != root.deadline
                || record.gateway_read != expected_gateway
                || !hex64(gateway)
                || !nonces.insert(record.request.observation_nonce.as_str())
                || !nonces.insert(gateway.as_str())
                || Self::live_context_legacy_nonce(id, s, &record.request.observation_nonce)?
                || Self::live_context_legacy_nonce(id, s, gateway)?
                || !hex64(&record.source_digest)
                || !hex64(&record.lifetime_digest)
                || record.source_digest != source_digest(s, &record.source_operation_id)?
                || s.source_funding.get(&record.source_operation_id)
                    != Some(&SessionSourceFundingV1::Execution)
                || record.source_ticket == 0
                || record.source_ticket
                    > *s.source_oracle_reads
                        .get(&record.source_operation_id)
                        .ok_or_else(invalid)?
                || record.claimed_at.get() < c.registered_at.get()
                || record.claimed_at.get() >= record.deadline.get()
                || record.claimed_at.get() < signed.verification.observed_at.0
                || record.claimed_at.get() - signed.verification.observed_at.0 > 5000
                || record.claimed_at.get() >= source.expires_at.0
            {
                return Err(invalid());
            }
            for port in PORTS {
                let key = port_id(attempt, port, record, r)?;
                if key == *attempt
                    || external_id(s, &key)
                    || c.live_reads.contains_key(&key)
                    || !expected_ids.insert(key.clone())
                    || s.purpose_transports.get(&key) != Some(&marker(attempt, port, record, r)?)
                {
                    return Err(invalid());
                }
            }
            if let Some(fact) = &record.archive {
                fact.read_commit.validate().map_err(|_| invalid())?;
                fact.capture_commit.validate().map_err(|_| invalid())?;
                let prefix = c
                    .prefix
                    .as_ref()
                    .ok_or_else(invalid)?
                    .decode()
                    .map_err(body_error)?;
                let ws::IntentOutcome::Committed { commit, .. } = prefix.response.payload.outcome
                else {
                    return Err(invalid());
                };
                if fact.request_digest
                    != ws::request_digest(TASK_CONTEXT_LIVE_READ_PATH, &record.request)
                        .map_err(|_| invalid())?
                    || fact.signature_hex.len() != 128
                    || !fact.signature_hex.bytes().all(|b| b.is_ascii_hexdigit())
                    || !hex64(&fact.payload_digest)
                    || fact.payload_bytes.0 == 0
                    || fact.payload_bytes.0 > CONTEXT_BODY_MAX_BYTES as u64
                    || fact.capture_commit != *commit
                    || fact.read_commit.world != fact.capture_commit.world
                    || fact.read_commit.binding.branch_id != r.scope.world.branch_id
                    || fact.read_commit.binding.reorg_generation != r.scope.world.reorg_epoch.0
                    || fact.read_commit.position < fact.capture_commit.position
                    || fact.archived_at.get() < record.claimed_at.get()
                {
                    return Err(invalid());
                }
            }
            match (
                record.admitted_at,
                record.admission_observed_at,
                record.admission_source_ticket,
                &record.archive,
            ) {
                (None, None, None, _) => {}
                (Some(at), Some(observed), Some(ticket), Some(fact)) => {
                    if observed.get() < fact.archived_at.get()
                        || at.get() < observed.get()
                        || at.get() - observed.get() > 5000
                        || at.get() >= record.deadline.get()
                        || at.get() >= source.expires_at.0
                        || at.get() < signed.verification.observed_at.0
                        || at.get() - signed.verification.observed_at.0 > 5000
                        || ticket <= record.source_ticket
                        || ticket
                            > *s.source_oracle_reads
                                .get(&record.source_operation_id)
                                .ok_or_else(invalid)?
                    {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            }
        }
        for (key, v) in &s.purpose_transports {
            if matches!(v, SessionPurposeTransportV1::LiveContextRead(_))
                && !expected_ids.contains(key)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
