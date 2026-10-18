//! Durable base-head capture for borrowed command staging.
//!
//! The process-local module cache, subscription cache, receipt signer, and
//! wall-clock telemetry are intentionally outside this head.  Everything that
//! can affect a serialized world or a deterministic execution decision is
//! represented by one of the grouped digests below.

use super::super::util::{hash_json, sha256_hex};
use super::{World, WorldError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorldPreparedBaseHead {
    state_and_configuration: String,
    journal_and_allocators: String,
    authorization: String,
    policy: String,
    scheduling: String,
    artifacts: String,
    consensus: String,
    governance: String,
}

impl WorldPreparedBaseHead {
    pub(super) fn capture(world: &World) -> Result<Self, WorldError> {
        let artifact_bytes: Vec<(&String, String)> = world
            .module_artifact_bytes
            .iter()
            .map(|(hash, bytes)| (hash, sha256_hex(bytes.as_ref())))
            .collect();

        Ok(Self {
            state_and_configuration: hash_json(&(
                &world.manifest,
                &world.module_registry,
                &world.module_limits_max,
                &world.snapshot_catalog,
                &world.chain_resource_manifest,
                &world.latest_chain_resource_delta,
                &world.state,
            ))?,
            journal_and_allocators: hash_json(&(
                world.journal.commitment()?,
                world.journal.len(),
                world.next_event_id,
                world.next_event_id_era,
                world.next_action_id,
                world.next_action_id_era,
                world.next_intent_id,
                world.next_intent_id_era,
                world.next_proposal_id,
                world.next_proposal_id_era,
            ))?,
            authorization: hash_json(&(
                &world.capabilities,
                &world.capability_grants_v2,
                &world.capability_revocation_state,
                &world.capability_nonce_records,
                &world.capability_authorization_receipts,
                &world.capability_invocation_contexts,
                &world.capability_authorization_root,
                &world.capability_budget_accounts,
                &world.capability_effect_receipt_links,
            ))?,
            policy: hash_json(&(&world.policies, &world.proposals, &world.scheduler_cursor))?,
            scheduling: hash_json(&(
                &world.pending_actions,
                &world.pending_effects,
                &world.inflight_effects,
                &world.module_tick_schedule,
                &world.module_tick_routing_metrics.deterministic_snapshot(),
                &world.runtime_memory_limits,
                &world.runtime_backpressure_stats,
                &world.logistics_sla_metrics,
                &world.threat_heatmap,
            ))?,
            artifacts: hash_json(&(
                &world.module_artifacts,
                &artifact_bytes,
                &world.builtin_release_manifest,
                &world.release_security_policy,
            ))?,
            consensus: if world.formal_archived_tick_consensus_count == 0 {
                hash_json(&(
                    &world.tick_consensus_records,
                    &world.tick_consensus_authority_source,
                    &world.tick_consensus_rejection_audit_events,
                ))?
            } else {
                hash_json(&(
                    world.formal_archived_tick_consensus_count,
                    &world.tick_consensus_records,
                    &world.tick_consensus_authority_source,
                    &world.tick_consensus_rejection_audit_events,
                ))?
            },
            governance: hash_json(&(
                &world.governance_execution_policy,
                &world.governance_finality_epoch_snapshots,
                &world.governance_emergency_brake_until_tick,
                &world.governance_identity_penalties,
                world.next_governance_identity_penalty_id,
                &world.rollback_authority_registry,
                &world.consumed_rollback_nonces,
                &world.rollback_nonce_outcomes,
            ))?,
        })
    }

    pub(super) fn stale_error() -> WorldError {
        WorldError::DistributedValidationFailed {
            reason: "prepared trusted command base head is stale".to_string(),
        }
    }
}
