//! Streaming identity of authoritative observer fields; no snapshot copies.
use super::*;
use serde::ser::SerializeMap;
use std::io::Write;

struct AuthorityView<'a>(&'a World);
impl Serialize for AuthorityView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let world = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("manifest", &world.manifest)?;
        map.serialize_entry("cognition", &world.cognition)?;
        map.serialize_entry("module_registry", &world.module_registry)?;
        map.serialize_entry("module_artifacts", &world.module_artifacts)?;
        map.serialize_entry("module_limits_max", &world.module_limits_max)?;
        map.serialize_entry("snapshot_catalog", &world.snapshot_catalog)?;
        map.serialize_entry("chain_resource_manifest", &world.chain_resource_manifest)?;
        map.serialize_entry(
            "latest_chain_resource_delta",
            &world.latest_chain_resource_delta,
        )?;
        map.serialize_entry("state", &world.state)?;
        map.serialize_entry("journal", &world.journal)?;
        map.serialize_entry("next_event_id", &world.next_event_id)?;
        map.serialize_entry("next_event_id_era", &world.next_event_id_era)?;
        map.serialize_entry("next_action_id", &world.next_action_id)?;
        map.serialize_entry("next_action_id_era", &world.next_action_id_era)?;
        map.serialize_entry("next_intent_id", &world.next_intent_id)?;
        map.serialize_entry("next_intent_id_era", &world.next_intent_id_era)?;
        map.serialize_entry("next_proposal_id", &world.next_proposal_id)?;
        map.serialize_entry("next_proposal_id_era", &world.next_proposal_id_era)?;
        map.serialize_entry("pending_actions", &world.pending_actions)?;
        map.serialize_entry("pending_effects", &world.pending_effects)?;
        map.serialize_entry("inflight_effects", &world.inflight_effects)?;
        map.serialize_entry("module_tick_schedule", &world.module_tick_schedule)?;
        map.serialize_entry("capabilities", &world.capabilities)?;
        map.serialize_entry("capability_grants_v2", &world.capability_grants_v2)?;
        map.serialize_entry(
            "capability_revocation_state",
            &world.capability_revocation_state,
        )?;
        map.serialize_entry("capability_nonce_records", &world.capability_nonce_records)?;
        map.serialize_entry(
            "capability_authorization_receipts",
            &world.capability_authorization_receipts,
        )?;
        map.serialize_entry(
            "capability_invocation_contexts",
            &world.capability_invocation_contexts,
        )?;
        map.serialize_entry(
            "capability_authorization_root",
            &world.capability_authorization_root,
        )?;
        map.serialize_entry(
            "capability_budget_accounts",
            &world.capability_budget_accounts,
        )?;
        map.serialize_entry(
            "capability_effect_receipt_links",
            &world.capability_effect_receipt_links,
        )?;
        map.serialize_entry("policies", &world.policies)?;
        map.serialize_entry("proposals", &world.proposals)?;
        map.serialize_entry("scheduler_cursor", &world.scheduler_cursor)?;
        map.serialize_entry("runtime_memory_limits", &world.runtime_memory_limits)?;
        map.serialize_entry(
            "runtime_backpressure_stats",
            &world.runtime_backpressure_stats,
        )?;
        map.serialize_entry("tick_consensus_records", &world.tick_consensus_records)?;
        if world.formal_archived_tick_consensus_count > 0 {
            map.serialize_entry(
                "formal_archived_tick_consensus_count",
                &world.formal_archived_tick_consensus_count,
            )?;
        }
        map.serialize_entry(
            "tick_consensus_authority_source",
            &world.tick_consensus_authority_source,
        )?;
        map.serialize_entry(
            "tick_consensus_rejection_audit_events",
            &world.tick_consensus_rejection_audit_events,
        )?;
        map.serialize_entry(
            "governance_execution_policy",
            &world.governance_execution_policy,
        )?;
        map.serialize_entry(
            "governance_finality_epoch_snapshots",
            &world.governance_finality_epoch_snapshots,
        )?;
        map.serialize_entry(
            "governance_emergency_brake_until_tick",
            &world.governance_emergency_brake_until_tick,
        )?;
        map.serialize_entry(
            "governance_identity_penalties",
            &world.governance_identity_penalties,
        )?;
        map.serialize_entry(
            "next_governance_identity_penalty_id",
            &world.next_governance_identity_penalty_id,
        )?;
        map.serialize_entry(
            "rollback_authority_registry",
            &world.rollback_authority_registry,
        )?;
        map.serialize_entry("consumed_rollback_nonces", &world.consumed_rollback_nonces)?;
        map.serialize_entry("rollback_nonce_outcomes", &world.rollback_nonce_outcomes)?;
        map.serialize_entry(
            "module_tick_routing_metrics",
            &world.module_tick_routing_metrics.deterministic_snapshot(),
        )?;
        map.end()
    }
}
struct HashWriter(blake3::Hasher);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Stable observer comparison over the original authoritative serialized view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverAuthorityIdentity {
    pub authority_digest: String,
    pub position: super::JournalEventPosition,
}
impl World {
    pub fn observer_authority_identity(&self) -> Result<ObserverAuthorityIdentity, WorldError> {
        Ok(ObserverAuthorityIdentity { authority_digest: self.observer_authority_digest()?, position: self.last_journal_event_position() })
    }
    pub(crate) fn observer_authority_digest(&self) -> Result<String, WorldError> {
        let mut output = HashWriter(blake3::Hasher::new());
        serde_json::to_writer(&mut output, &AuthorityView(self))?;
        Ok(output.0.finalize().to_hex().to_string())
    }
    pub(crate) fn observer_last_event_era(&self) -> u64 {
        let last = self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or_else(|| self.next_event_id.saturating_sub(1));
        if last == 0 {
            self.next_event_id_era.saturating_sub(1)
        } else {
            self.next_event_id_era
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_digest_tracks_state_and_ignores_local_failure_hooks() {
        let mut world = World::new();
        let original = world.observer_authority_digest().unwrap();
        world.fail_next_append_after_reducer = true;
        assert_eq!(world.observer_authority_digest().unwrap(), original);
        world.state.time += 1;
        assert_ne!(world.observer_authority_digest().unwrap(), original);
    }
}
