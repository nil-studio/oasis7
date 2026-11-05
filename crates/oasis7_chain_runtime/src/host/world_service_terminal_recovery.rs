//! A current ordinary DevLocal assertion beside the immutable Prefix receipt.
use super::*;
use oasis7_runtime::{CapabilityAuthorizationEvent, ProviderCapabilityAvailability};

pub(super) fn authority(
    service: &Service<'_>,
    current: &PinnedWorld,
    historical: &PinnedWorld,
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
    let value = historical
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
        || result.committed_height != historical.record.height
    {
        return Err("terminal historical original result differs".into());
    }
    let events = historical
        .world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                    key: recorded_key,
                    record,
                },
            ) if recorded_key == &key && record == value => Some(event.time),
            _ => None,
        })
        .collect::<Vec<_>>();
    if events.len() != 1 {
        return Err("terminal historical admission event missing or ambiguous".into());
    }
    let admitted_at_tick = events[0];
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or("terminal original invocation missing")?;
    let historical_grant = historical
        .world
        .validate_provider_grant_at_admission(agent, &invocation.grant_id, admitted_at_tick)
        .map_err(|e| format!("{e:?}"))?;
    let grant = current
        .world
        .validate_provider_grant_for_terminal(agent, &invocation.grant_id, admitted_at_tick)
        .map_err(|e| format!("{e:?}"))?;
    if historical_grant != grant
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
    let original_economy = historical
        .world
        .cognition_economy()
        .map_err(|e| format!("{e:?}"))?;
    let old = original_economy
        .leases
        .get(&lease.lease_id)
        .ok_or("terminal historical lease missing")?;
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
        schema_version: 1,
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
        prefix_commit: historical.commit.clone(),
        admitted_at_tick,
        writer_public_key: signed.subject_public_key.clone(),
        delegation_generation: signed.request.delegation_generation,
        current_runtime_binding: current_binding,
        current_task_fence: task
            .as_ref()
            .map(oasis7_agent_api::game::GameTaskFenceV1::from_control),
        read_commit: current.commit.clone(),
    };
    proof.validate(correlation).map_err(|e| e.to_string())?;
    Ok(Some(proof))
}
