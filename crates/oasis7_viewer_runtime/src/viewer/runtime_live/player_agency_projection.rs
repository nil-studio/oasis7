use crate::runtime::{
    AgentCausalReceiptV1, AgentDelegationAuthorizationV1, World, WorldEvent as RuntimeWorldEvent,
    WorldEventBody as RuntimeWorldEventBody,
};
use crate::runtime_host::archive::{
    PlayerGameplayAgencyReadModel, PlayerGameplayCausalReceiptSnapshot,
    PlayerGameplayCausalityKind, PlayerGameplayDelegationGrantSnapshot,
    PlayerGameplayDelegationUsageSnapshot,
};

pub(super) fn canonical_accepted_runtime_intent_id(
    status: &str,
    source_class: Option<&str>,
    freshness: Option<&str>,
    intent_id: Option<&str>,
) -> Option<String> {
    (source_class == Some("runtime_projection")
        && freshness == Some("current")
        && matches!(status, "accepted" | "blocked" | "completed"))
    .then_some(intent_id)
    .flatten()
    .map(str::trim)
    .filter(|intent_id| !intent_id.is_empty())
    .map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlayerGameplayCausalitySignal {
    pub kind: PlayerGameplayCausalityKind,
    pub detail: String,
}

pub(super) fn player_gameplay_causality_from_runtime_events(
    new_events: &[RuntimeWorldEvent],
) -> Option<PlayerGameplayCausalitySignal> {
    let mut override_detail = None;
    let mut override_fallback = None;
    for runtime_event in new_events {
        match &runtime_event.body {
            RuntimeWorldEventBody::RuleDecisionRecorded(record)
                if record.override_action.is_some() =>
            {
                let notes = if record.notes.is_empty() {
                    "no rule note supplied".to_string()
                } else {
                    record.notes.join("; ")
                };
                override_detail = Some(format!(
                    "rule module {} redirected the accepted action before execution: {}",
                    record.module_id, notes
                ));
            }
            RuntimeWorldEventBody::ActionOverridden(record) => {
                override_fallback = Some(format!(
                    "the acting agent followed an overridden plan instead of the original action: {:?} -> {:?}",
                    record.original_action, record.override_action
                ));
            }
            _ => {}
        }
    }
    override_fallback.map(|fallback| PlayerGameplayCausalitySignal {
        kind: PlayerGameplayCausalityKind::AgentOverride,
        detail: override_detail.unwrap_or(fallback),
    })
}

fn grant_snapshot(
    grant: &crate::runtime::AgentDelegationGrantV1,
) -> PlayerGameplayDelegationGrantSnapshot {
    PlayerGameplayDelegationGrantSnapshot {
        grant_id: grant.grant_id.clone(),
        source_id: grant.source_id.clone(),
        issuer_id: grant.issuer_id.clone(),
        owner_id: grant.owner_id.clone(),
        organization_id: grant.organization_id.clone(),
        agent_id: grant.agent_id.clone(),
        object_id: grant.object_id.clone(),
        action_kinds: grant.action_kinds.clone(),
        revision: grant.revision,
        period_id: grant.period_id.clone(),
        valid_from_tick: grant.valid_from_tick,
        valid_until_tick: grant.valid_until_tick,
        limit_units: grant.limit_units,
        resource_kind: grant.resource_kind.clone(),
        revoked: grant.revoked,
    }
}

fn authorization_snapshot(
    authorization: &AgentDelegationAuthorizationV1,
) -> PlayerGameplayDelegationUsageSnapshot {
    PlayerGameplayDelegationUsageSnapshot {
        grant: grant_snapshot(&authorization.grant),
        cost_units: authorization.cost_units,
        spent_units: authorization.spent_units,
        remaining_units: authorization.remaining_units,
    }
}

fn receipt_snapshot(receipt: &AgentCausalReceiptV1) -> PlayerGameplayCausalReceiptSnapshot {
    PlayerGameplayCausalReceiptSnapshot {
        receipt_id: receipt.receipt_id.clone(),
        commit_id: receipt.commit_id.clone(),
        intent_id: receipt.intent_id.clone(),
        effect_intent_id: receipt.effect_intent_id.clone(),
        action_id: receipt.action_id,
        action_kind: receipt.action_kind.clone(),
        domain_event_refs: receipt.domain_event_refs.clone(),
        disposition: receipt.disposition.clone(),
        primary_reason: receipt.primary_reason.clone(),
        next_step: receipt.next_step.clone(),
        expected_consequence: receipt.expected_consequence.clone(),
        actual_consequence: receipt.actual_consequence.clone(),
        alternative: receipt.alternative.clone(),
        stakes: receipt.stakes.clone(),
        evidence_refs: receipt.evidence_refs.clone(),
        correction_refs: receipt.correction_refs.clone(),
        interruption_refs: receipt.interruption_refs.clone(),
        owner_control_refs: receipt.owner_control_refs.clone(),
        authorization: receipt.authorization.as_ref().map(authorization_snapshot),
        dissent: receipt.dissent.clone(),
        override_actor: receipt.override_actor.clone(),
        hard_boundary: receipt.hard_boundary.clone(),
    }
}

/// Build one owner-filtered Viewer read model from the same committed Runtime
/// receipts and private-memory retrieval context used by the player snapshot.
pub(super) fn project_player_agency_read_model(
    world: &World,
    sidecar: &super::RuntimeLlmSidecar,
    agent_id: &str,
    canonical_intent_id: Option<&str>,
) -> PlayerGameplayAgencyReadModel {
    let receipts = world.agent_causal_receipts(agent_id);
    let authorizations = world.agent_delegation_authorizations(agent_id);
    let referenced_memory_context = sidecar.agent_referenced_memory_context(agent_id);
    let memory_corrections = sidecar.agent_memory_corrections(agent_id);
    let runtime_error = receipts
        .as_ref()
        .err()
        .or_else(|| authorizations.as_ref().err())
        .map(|error| format!("runtime_agency_read_unavailable:{error:?}"));
    let causal_receipt = receipts.as_ref().ok().and_then(|rows| {
        canonical_intent_id.and_then(|intent_id| {
            rows.iter()
                .rev()
                .find(|receipt| receipt.intent_id.as_deref() == Some(intent_id))
                .map(receipt_snapshot)
        })
    });
    let projected_authorizations = authorizations
        .as_ref()
        .map(|rows| rows.iter().map(authorization_snapshot).collect())
        .unwrap_or_default();
    let causal_receipt_status = if runtime_error.is_some() {
        "unavailable"
    } else if causal_receipt.is_some() {
        "committed_receipt_available"
    } else {
        "waiting_for_committed_decision"
    };
    let memory_context_status = if referenced_memory_context.is_some() {
        "available"
    } else {
        "unavailable"
    };
    let status = if runtime_error.is_some() {
        "unavailable"
    } else if referenced_memory_context.is_some() {
        "available"
    } else {
        "partial"
    };
    let unavailable_reason = runtime_error.or_else(|| {
        referenced_memory_context
            .is_none()
            .then(|| "referenced_private_memory_context_unavailable".to_string())
    });
    // Keep explicit substatus alongside nullable payloads; an empty receipt is
    // not evidence that a decision was rejected or had no world effect.
    PlayerGameplayAgencyReadModel {
        status: status.to_string(),
        source: "runtime_receipts_and_owner_private_memory_context".to_string(),
        unavailable_reason,
        causal_receipt_status: causal_receipt_status.to_string(),
        memory_context_status: memory_context_status.to_string(),
        causal_receipt,
        delegation_authorizations: projected_authorizations,
        referenced_memory_context,
        memory_corrections,
    }
}
