//! A current ordinary DevLocal assertion beside the immutable Prefix receipt.
use super::*;
use oasis7_runtime::CapabilityAuthorizationEvent;
use oasis7_runtime::ProviderCapabilityAvailability;

pub(super) fn authority(
    service: &Service<'_>,
    current: &PinnedWorld,
    historical: Option<&PinnedWorld>,
    original: &WorldServicePayloadV1,
    correlation: &RequestCorrelation,
) -> Result<Option<TerminalRecoveryAuthorityV1>, String> {
    let WorldServicePayloadV1::Scheduler(signed) = original else {
        return Ok(None);
    };
    let SchedulerOperationV1::ProviderPrefix { request, .. } = &signed.request.operation else {
        return Ok(None);
    };
    // This is host-restored DevLocal authority, never a caller supplied mode.
    if service.guarded.is_some()
        || !service
            .restore_context
            .permits_local_provider_terminal_recovery()
    {
        return Ok(None);
    }
    let agent = &signed.request.agent_id;
    match current
        .world
        .local_provider_capability_availability(agent)
        .map_err(|e| format!("{e:?}"))?
    {
        ProviderCapabilityAvailability::Live => return Ok(None),
        ProviderCapabilityAvailability::ExpiredOnly { .. } => {}
    }
    request
        .validate_production_lane()
        .map_err(|e| e.to_string())?;
    if &request.agent_subject != agent {
        return Err("terminal original Agent mismatch".into());
    }
    let key = correlation::key_digest(&correlation.key)?;
    let source = historical.unwrap_or(current);
    let value = source
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
        .ok_or("terminal historical original result missing")?;
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if result.rejected.is_some()
        || result.request.correlation != *correlation
        || result.request.signed_payload != *original
        || historical.is_some_and(|pin| result.committed_height != pin.committed_height())
    {
        return Err("terminal historical original result differs".into());
    }
    let prefix_event = original_admission_event(source.world.journal(), &key, value)?;
    let admitted_at_tick = prefix_event.1;
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or("terminal original invocation missing")?;
    let grant = current
        .world
        .validate_provider_grant_for_terminal(agent, &invocation.grant_id, admitted_at_tick)
        .map_err(|e| format!("{e:?}"))?;
    if historical
        .map(|pin| {
            pin.world
                .validate_provider_grant_at_admission(agent, &invocation.grant_id, admitted_at_tick)
                .map_err(|e| format!("{e:?}"))
        })
        .transpose()?
        .is_some_and(|old| old != grant)
        || grant.subject != invocation.subject
        || grant.audience != invocation.audience
        || grant.scope.module_id != invocation.module_id
        || grant.scope.module_version != invocation.module_version
    {
        return Err("terminal original grant or source changed".into());
    }
    let current_binding = current
        .world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let saved = &request.runtime_binding;
    if saved.world_id != current_binding.world_id
        || saved.branch_id != current_binding.branch_id
        || saved.finality_epoch != current_binding.finality_epoch
        || saved.finality_block_hash != current_binding.finality_block_hash
        || saved.finality_status != current_binding.finality_status
        || saved.reorg_epoch != current_binding.reorg_epoch
        || saved.runtime_manifest_hash != current_binding.runtime_manifest_hash
    {
        return Err("terminal current Runtime binding changed".into());
    }
    oasis7_runtime::service_admission::agent_authority::validate_agent_signer(
        &current.world,
        agent,
        &signed.subject_public_key,
        signed.request.delegation_generation,
    )?;
    oasis7_runtime::service_admission::game_task::validate_cognition_fence_staged(
        &current.world,
        agent,
        None,
        &signed.subject_public_key,
        signed.request.delegation_generation,
        &[],
    )?;
    let identity = current
        .world
        .current_agent_identity(agent)
        .ok_or("terminal current Agent identity missing")?;
    let owner = current
        .world
        .current_agent_owner(agent)
        .ok_or("terminal current owner missing")?;
    if owner.player_id != identity.owner_binding
        || grant.subject
            != (oasis7_wasm_abi::CapabilitySubject::Agent {
                agent_id: agent.clone(),
                owner_binding: identity.owner_binding.clone(),
                generation: identity.generation,
            })
    {
        return Err("terminal current owner or generation changed".into());
    }
    let economy = current
        .world
        .cognition_economy()
        .map_err(|e| format!("{e:?}"))?;
    let leases = economy
        .leases
        .values()
        .filter(|lease| {
            lease.agent_id == *agent
                && lease.agent_session_id == request.agent_session_id
                && lease.agent_turn_id == request.agent_turn_id
                && lease.decision_request_id == request.decision_request_id
                && lease.request_digest == request.request_digest.to_string()
        })
        .collect::<Vec<_>>();
    if leases.len() != 1 {
        return Err("terminal original lease missing or ambiguous".into());
    }
    let lease = leases[0];
    let (old, current_authenticated_journal) = if let Some(historical) = historical {
        let original_economy = historical
            .world
            .cognition_economy()
            .map_err(|e| format!("{e:?}"))?;
        (
            original_economy
                .leases
                .get(&lease.lease_id)
                .ok_or("terminal historical lease missing")?
                .clone(),
            None,
        )
    } else {
        let (old, evidence) =
            current_journal_reserve(current, signed, &result, lease, prefix_event)?;
        (old, Some(evidence))
    };
    if lease.account_id != identity.owner_binding
        || lease.idempotency_key != request.provider_invocation_key().to_string()
        || lease.agent_id != old.agent_id
        || lease.agent_session_id != old.agent_session_id
        || lease.agent_turn_id != old.agent_turn_id
        || lease.decision_request_id != old.decision_request_id
        || lease.request_digest != old.request_digest
        || lease.account_id != old.account_id
        || lease.idempotency_key != old.idempotency_key
        || lease.reserved_at_tick != old.reserved_at_tick
        || lease.quote != old.quote
        || lease.reserved_amount != 1
        || old.reserved_amount != 1
        || lease.released_amount != 0
        || lease.refunded_amount != 0
        || lease.compensated_amount != 0
        || !matches!(
            lease.status,
            oasis7_runtime::CognitionLeaseStatusV1::Reserved
                | oasis7_runtime::CognitionLeaseStatusV1::Settled
        )
    {
        return Err("terminal canonical lease admission changed".into());
    }
    let task =
        oasis7_runtime::service_admission::game_task::current_control(&current.world, agent)?;
    let proof = TerminalRecoveryAuthorityV1 {
        schema_version: if historical.is_some() { 1 } else { 2 },
        role: TerminalRecoveryRoleV1::TerminalOnly,
        world: service.identity.clone(),
        agent_id: agent.clone(),
        owner_binding: identity.owner_binding,
        identity_generation: identity.generation,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        request_digest: request.request_digest.to_string(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        grant_id: grant.grant_id,
        grant_issued_at_tick: grant.issued_at_tick,
        grant_expires_at_tick: grant
            .expires_at_tick
            .ok_or("terminal original grant expiry missing")?,
        lease: TerminalLeaseIdentityV1 {
            lease_id: lease.lease_id.clone(),
            idempotency_key: lease.idempotency_key.clone(),
            account_id: lease.account_id.clone(),
            quote_digest: lease.quote.quote_digest.clone(),
            reserved_amount: lease.reserved_amount,
            reserved_at_tick: lease.reserved_at_tick,
        },
        prefix_correlation: correlation.clone(),
        prefix_result_digest: request_digest("terminal-prefix-result.v1", &result)?,
        prefix_action_id: result.action_id,
        prefix_deadline_unix_ms: result.request.deadline_unix_ms,
        prefix_committed_height: result.committed_height,
        prefix_commit: historical.map(|pin| pin.commit.clone()),
        admitted_at_tick,
        writer_public_key: signed.subject_public_key.clone(),
        delegation_generation: signed.request.delegation_generation,
        current_runtime_binding: current_binding,
        current_task_fence: task
            .as_ref()
            .map(oasis7_agent_api::game::GameTaskFenceV1::from_control),
        read_commit: current.commit.clone(),
        current_authenticated_journal,
    };
    proof.validate(correlation).map_err(|e| e.to_string())?;
    Ok(Some(proof))
}

fn current_journal_reserve(
    current: &PinnedWorld,
    prefix: &SignedReadRequest<SchedulerIntentV1>,
    prefix_result: &CanonicalIntentResultV1,
    lease: &oasis7_runtime::CognitionLeaseV1,
    prefix_event: (u64, u64),
) -> Result<
    (
        oasis7_runtime::CognitionLeaseV1,
        CurrentAuthenticatedJournalV1,
    ),
    String,
> {
    let SchedulerOperationV1::ProviderPrefix { request, .. } = &prefix.request.operation else {
        return Err("terminal current Journal requires original Prefix".into());
    };
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or("terminal original invocation missing")?;
    let mut candidates = Vec::new();
    for (key, value) in &current
        .world
        .capability_revocation_state()
        .world_service_results
    {
        let result: CanonicalIntentResultV1 =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
            continue;
        };
        let SchedulerOperationV1::ReserveLease(reserve) = &signed.request.operation else {
            continue;
        };
        if result.rejected.is_none()
            && reserve.idempotency_key == request.provider_invocation_key().to_string()
            && reserve.agent_id == request.agent_subject
            && reserve.agent_session_id == request.agent_session_id
            && reserve.agent_turn_id == request.agent_turn_id
            && reserve.decision_request_id == request.decision_request_id
            && reserve.request_digest == request.request_digest.to_string()
        {
            candidates.push((key, value, result.clone()));
        }
    }
    if candidates.len() != 1 {
        return Err("terminal current Journal original Reserve missing or ambiguous".into());
    }
    let (key, value, result) = candidates.remove(0);
    correlation::validate_result(key, value)?;
    if derive_correlation(current.commit.world.clone(), &result.request.signed_payload)?
        != result.request.correlation
    {
        return Err("terminal original Reserve signed correlation differs".into());
    }
    let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
        return Err("terminal original Reserve variant differs".into());
    };
    let SchedulerOperationV1::ReserveLease(reserve) = &signed.request.operation else {
        return Err("terminal original Reserve variant differs".into());
    };
    let reserve_event = original_admission_event(current.world.journal(), key, value)?;
    if reserve_event.0 >= prefix_event.0
        || reserve_event.1 > prefix_event.1
        || result.committed_height > prefix_result.committed_height
        || signed.subject_public_key != prefix.subject_public_key
        || signed.request.delegation_generation != prefix.request.delegation_generation
    {
        return Err("terminal current Journal Reserve event or signer differs".into());
    }
    current
        .world
        .validate_provider_grant_for_terminal(
            &request.agent_subject,
            &invocation.grant_id,
            reserve_event.1,
        )
        .map_err(|e| format!("{e:?}"))?;
    let original: oasis7_runtime::CognitionLeaseV1 =
        serde_json::from_value(result.receipt.clone()).map_err(|e| e.to_string())?;
    original.validate().map_err(|e| format!("{e:?}"))?;
    if serde_json::to_value(&original).map_err(|e| e.to_string())? != result.receipt {
        return Err("terminal original Reserve receipt is not exact typed material".into());
    }
    if original.status != oasis7_runtime::CognitionLeaseStatusV1::Reserved
        || original.lease_id != lease.lease_id
        || original.reserved_at_tick != reserve_event.1
        || original.account_id != reserve.account_id
        || original.agent_id != reserve.agent_id
        || original.agent_session_id != reserve.agent_session_id
        || original.agent_turn_id != reserve.agent_turn_id
        || original.decision_request_id != reserve.decision_request_id
        || original.request_digest != reserve.request_digest
        || original.idempotency_key != reserve.idempotency_key
        || original.quote != reserve.quote
        || original.reserved_amount != 1
        || original.settled_amount != 0
        || original.released_amount != 0
        || original.refunded_amount != 0
        || original.compensated_amount != 0
        || original.closed_at_tick.is_some()
    {
        return Err("terminal original Reserve receipt differs from admitted request".into());
    }
    let evidence = CurrentAuthenticatedJournalV1 {
        prefix_event_id: prefix_event.0,
        prefix_event_time: prefix_event.1,
        prefix_receipt: serde_json::from_value(prefix_result.receipt.clone())
            .map_err(|e| e.to_string())?,
        reserve_correlation: result.request.correlation.clone(),
        reserve_full_result_digest: request_digest("terminal-reserve-result.v1", &result)?,
        reserve_receipt_digest: request_digest("terminal-reserve-receipt.v1", &result.receipt)?,
        reserve_committed_height: result.committed_height,
        reserve_event_id: reserve_event.0,
        reserve_event_time: reserve_event.1,
    };
    Ok((original, evidence))
}

fn original_admission_event(
    journal: &oasis7_runtime::Journal,
    key: &str,
    value: &serde_json::Value,
) -> Result<(u64, u64), String> {
    let mut events = journal.events.iter().filter_map(|event| match &event.body {
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                key: recorded_key,
                record,
            },
        ) if recorded_key == key => Some((event.id, event.time, record)),
        _ => None,
    });
    let (id, time, record) = events
        .next()
        .ok_or("terminal original admission event missing")?;
    if record != value || events.next().is_some() {
        return Err("terminal original admission event differs or is ambiguous".into());
    }
    Ok((id, time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_journal_admission_requires_one_exact_full_record() {
        // This isolates event selection after authenticated pin/restore. The
        // actual-WASM HTTP case separately exercises that production boundary.
        let value =
            serde_json::json!({"request":{"identity":"original"},"receipt":{"dispatched":true}});
        let event = oasis7_runtime::WorldEvent {
            id: 71,
            time: 224,
            caused_by: None,
            body: WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                    key: "original".into(),
                    record: value.clone(),
                },
            ),
        };
        let mut journal = oasis7_runtime::Journal::default();
        assert!(original_admission_event(&journal, "original", &value).is_err());
        journal.events.push(event.clone());
        assert_eq!(
            original_admission_event(&journal, "original", &value).unwrap(),
            (71, 224)
        );
        let changed =
            serde_json::json!({"request":{"identity":"other"},"receipt":{"dispatched":true}});
        assert!(original_admission_event(&journal, "original", &changed).is_err());
        journal.events.push(event);
        assert!(original_admission_event(&journal, "original", &value).is_err());
        journal.events[1].body = WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                key: "original".into(),
                record: changed,
            },
        );
        assert!(
            original_admission_event(&journal, "original", &value).is_err(),
            "a conflicting event for the same key is also ambiguous"
        );
    }
}
