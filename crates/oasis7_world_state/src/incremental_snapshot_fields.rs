//! Legacy Snapshot field ownership links only. No importer or snapshot
//! materialization is needed to compile-check this exhaustive list.
use crate::snapshot::Snapshot;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFieldLinkKind {
    Direct,
    DeterministicProjection,
    HistoryView,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotFieldLink {
    pub name: &'static str,
    pub runtime_field: &'static str,
    pub kind: SnapshotFieldLinkKind,
}
macro_rules! links {
    ($($name:ident => $source:literal, $kind:ident;)*) => {
        pub const SNAPSHOT_FIELDS: &[SnapshotFieldLink] = &[$(SnapshotFieldLink { name: stringify!($name), runtime_field: $source, kind: SnapshotFieldLinkKind::$kind },)*];
        const _: fn(&Snapshot) = |snapshot| { let Snapshot { $($name: _,)* } = snapshot; };
    }
}
links! {
    snapshot_catalog => "snapshot_catalog", Direct;
    manifest => "manifest", Direct;
    cognition => "cognition", Direct;
    chain_resource_manifest => "chain_resource_manifest", Direct;
    latest_chain_resource_delta => "latest_chain_resource_delta", Direct;
    module_registry => "module_registry", Direct;
    module_artifacts => "module_artifacts", Direct;
    module_artifact_bytes => "module_artifact_bytes", Direct;
    module_limits_max => "module_limits_max", Direct;
    state => "state", Direct;
    journal_len => "journal", HistoryView;
    last_event_id => "journal.last_event_id", HistoryView;
    journal_commitment => "journal", HistoryView;
    event_id_era => "next_event_id_era", Direct;
    next_action_id => "next_action_id", Direct;
    action_id_era => "next_action_id_era", Direct;
    next_intent_id => "next_intent_id", Direct;
    intent_id_era => "next_intent_id_era", Direct;
    next_proposal_id => "next_proposal_id", Direct;
    proposal_id_era => "next_proposal_id_era", Direct;
    pending_actions => "pending_actions", Direct;
    pending_effects => "pending_effects", Direct;
    inflight_effects => "inflight_effects", Direct;
    module_tick_schedule => "module_tick_schedule", Direct;
    module_tick_routing_metrics => "module_tick_routing_metrics", DeterministicProjection;
    capabilities => "capabilities", Direct;
    capability_grants_v2 => "capability_grants_v2", Direct;
    capability_revocation_state => "capability_revocation_state", Direct;
    capability_nonce_records => "capability_nonce_records", Direct;
    capability_authorization_receipts => "capability_authorization_receipts", Direct;
    capability_invocation_contexts => "capability_invocation_contexts", Direct;
    capability_authorization_root => "capability_authorization_root", Direct;
    capability_budget_accounts => "capability_budget_accounts", Direct;
    capability_effect_receipt_links => "capability_effect_receipt_links", Direct;
    policies => "policies", Direct;
    proposals => "proposals", Direct;
    scheduler_cursor => "scheduler_cursor", Direct;
    runtime_memory_limits => "runtime_memory_limits", DeterministicProjection;
    runtime_backpressure_stats => "runtime_backpressure_stats", DeterministicProjection;
    tick_consensus_records => "tick_consensus_records", Direct;
    tick_consensus_total_record_count => "tick_consensus_records", HistoryView;
    tick_consensus_archived_record_count => "formal_archived_tick_consensus_count", HistoryView;
    tick_consensus_hot_from_tick => "tick_consensus_records", HistoryView;
    tick_consensus_hot_to_tick => "tick_consensus_records", HistoryView;
    tick_consensus_authority_source => "tick_consensus_authority_source", Direct;
    tick_consensus_rejection_audit_events => "tick_consensus_rejection_audit_events", Direct;
    governance_execution_policy => "governance_execution_policy", Direct;
    governance_finality_epoch_snapshots => "governance_finality_epoch_snapshots", Direct;
    governance_emergency_brake_until_tick => "governance_emergency_brake_until_tick", Direct;
    governance_identity_penalties => "governance_identity_penalties", Direct;
    next_governance_identity_penalty_id => "next_governance_identity_penalty_id", Direct;
    rollback_authority_registry => "rollback_authority_registry", Direct;
    consumed_rollback_nonces => "consumed_rollback_nonces", Direct;
    rollback_nonce_outcomes => "rollback_nonce_outcomes", Direct;
}
