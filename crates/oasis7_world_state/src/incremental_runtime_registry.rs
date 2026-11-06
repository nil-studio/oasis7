//! Closed typed Runtime descriptor catalogue, checked by the original Runtime
//! projection inventory. Catalogue membership grants no qualification.
use crate::incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
/// Event uses the existing event-sequence family; only action/intent/proposal
/// use this fixed tag family. Parsing carries no allocation authority.
pub const RUNTIME_SEQUENCE_KEY_DOMAIN_V1: &[u8] = b"oasis7.iws.runtime-sequence-key.v1\0";
pub fn parse_runtime_sequence_key_v1(bytes: &[u8]) -> Result<u8, &'static str> {
    let domain = RUNTIME_SEQUENCE_KEY_DOMAIN_V1;
    if bytes.len() != domain.len() + 1
        || !bytes.starts_with(domain)
        || !(1..=3).contains(&bytes[domain.len()])
    {
        return Err("runtime sequence key domain/tag/length");
    }
    Ok(bytes[domain.len()])
}
macro_rules! fields {($(($name:literal,$shape:ident,$namespace:ident)),+$(,)?)=>{pub const RUNTIME_TYPED_GENESIS_FIELDS_V1:&[FieldDescriptor]=&[$(FieldDescriptor{owner:FieldOwner::Runtime,name:$name,shape:FieldShape::$shape,destination:FieldDestination::State(Ns::$namespace)}),+];};}
fields! {
("manifest",Value,Artifacts),
("module_artifacts",Set,Artifacts),
("module_artifact_bytes",ByteMap,Artifacts),
("module_limits_max",Value,Artifacts),
("chain_resource_manifest",Value,Artifacts),
("latest_chain_resource_delta",Value,Artifacts),
("pending_actions",Ordered,Scheduler),
("pending_effects",Ordered,Effects),
("inflight_effects",Map,Effects),
("module_tick_routing_metrics",Value,Scheduler),
("capabilities",Map,Authorization),
("capability_grants_v2",Map,Authorization),
("capability_revocation_state",Value,Authorization),
("capability_nonce_records",Map,Authorization),
("capability_authorization_receipts",Map,Authorization),
("capability_invocation_contexts",Map,Authorization),
("capability_authorization_root",Value,Authorization),
("capability_budget_accounts",Map,Authorization),
("capability_effect_receipt_links",Map,Effects),
("proposals",Map,Authorization),
("scheduler_cursor",Value,Scheduler),
("logistics_sla_metrics",Value,Business),
("threat_heatmap",Map,Business),
("tick_consensus_authority_source",Value,Authorization),
("governance_execution_policy",Value,Authorization),
("governance_finality_epoch_snapshots",Map,Authorization),
("governance_emergency_brake_until_tick",Value,Authorization),
("governance_identity_penalties",Map,Authorization),
("next_governance_identity_penalty_id",Value,Allocator),
("builtin_release_manifest",Value,Artifacts),
("release_security_policy",Value,Artifacts),
("rollback_authority_registry",Value,Authorization),
("consumed_rollback_nonces",Set,Authorization),
("rollback_nonce_outcomes",Map,Authorization),
("module_registry.records.manifest",Map,Artifacts),
("module_registry.records.registered_at",Map,Artifacts),
("module_registry.records.registered_by",Map,Artifacts),
("module_registry.records.audit_event_id",Map,Artifacts),
("module_registry.active",Map,Artifacts),
("policies.rules",Ordered,Authorization),
("runtime_memory_limits.max_pending_actions",Value,Scheduler),
("runtime_memory_limits.max_pending_effects",Value,Scheduler),
("runtime_memory_limits.max_inflight_effects",Value,Scheduler),
("runtime_backpressure_stats.pending_actions_evicted",Value,Scheduler),
("runtime_backpressure_stats.pending_effects_evicted",Value,Scheduler),
("runtime_backpressure_stats.inflight_effects_evicted",Value,Scheduler)
}
