//! Exhaustive borrowed Runtime field projection for IWS genesis preparation.
//! Not a new-format activation, snapshot importer, or ordinary commit exporter.
use super::World;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Namespace;
use oasis7_world_state::incremental_projection::*;
#[path = "incremental_projection_cognition.rs"]
mod cognition;

fn visit_registry<S: GenesisFieldVisitor>(
    registry: &super::super::ModuleRegistry,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let super::super::ModuleRegistry { records, active } = registry;
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "module_registry.records",
        destination: FieldDestination::State(Namespace::Artifacts),
        shape: FieldShape::Map,
    };
    oasis7_world_state::visit_iws_field!(sink, field, records, map)?;
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "module_registry.active",
        destination: FieldDestination::State(Namespace::Artifacts),
        shape: FieldShape::Map,
    };
    oasis7_world_state::visit_iws_field!(sink, field, active, map)
}
fn visit_policies<S: GenesisFieldVisitor>(
    policies: &super::super::PolicySet,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let super::super::PolicySet { rules } = policies;
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "policies.rules",
        destination: FieldDestination::State(Namespace::Authorization),
        shape: FieldShape::Ordered,
    };
    oasis7_world_state::visit_iws_field!(sink, field, rules, ordered)
}
fn visit_routing<S: GenesisFieldVisitor>(
    metrics: &super::module_tick_runtime::ModuleTickRoutingMetrics,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    use super::module_tick_runtime::ModuleTickRoutingMetrics;
    let ModuleTickRoutingMetrics {
        missing_invocation_count: _,
        routing_count: _,
        last_due_count: _,
        last_invoked_count: _,
        last_missing_invocation_count: _,
        oldest_overdue_ticks: _,
        last_route_duration_ms: _,
        max_route_duration_ms: _,
        cumulative_route_duration_ms: _,
        duration_buckets: _,
    } = metrics;
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "module_tick_routing_metrics",
        destination: FieldDestination::State(Namespace::Scheduler),
        shape: FieldShape::Value,
    };
    scalar(sink, field, &metrics.deterministic_snapshot())?;
    sink.process(FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "module_tick_routing_metrics.wall_clock",
        destination: FieldDestination::Process,
        shape: FieldShape::Process,
    })
    .map_err(ProjectionError::Sink)
}

fn visit_limits<S: GenesisFieldVisitor>(
    limits: &super::WorldRuntimeMemoryLimits,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let super::WorldRuntimeMemoryLimits {
        max_pending_actions,
        max_pending_effects,
        max_inflight_effects,
        max_journal_events: _,
    } = limits;
    for (name, value) in [
        (
            "runtime_memory_limits.max_pending_actions",
            max_pending_actions,
        ),
        (
            "runtime_memory_limits.max_pending_effects",
            max_pending_effects,
        ),
        (
            "runtime_memory_limits.max_inflight_effects",
            max_inflight_effects,
        ),
    ] {
        scalar(
            sink,
            FieldDescriptor {
                owner: FieldOwner::Runtime,
                name,
                destination: FieldDestination::State(Namespace::Scheduler),
                shape: FieldShape::Value,
            },
            value,
        )?;
    }
    sink.process(FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "runtime_memory_limits.max_journal_events",
        destination: FieldDestination::Process,
        shape: FieldShape::Process,
    })
    .map_err(ProjectionError::Sink)
}
fn visit_backpressure<S: GenesisFieldVisitor>(
    stats: &super::WorldRuntimeBackpressureStats,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    let super::WorldRuntimeBackpressureStats {
        pending_actions_evicted,
        pending_effects_evicted,
        inflight_effects_evicted,
        inflight_effect_dispatch_blocked: _,
        journal_events_evicted: _,
    } = stats;
    for (name, value) in [
        (
            "runtime_backpressure_stats.pending_actions_evicted",
            pending_actions_evicted,
        ),
        (
            "runtime_backpressure_stats.pending_effects_evicted",
            pending_effects_evicted,
        ),
        (
            "runtime_backpressure_stats.inflight_effects_evicted",
            inflight_effects_evicted,
        ),
    ] {
        scalar(
            sink,
            FieldDescriptor {
                owner: FieldOwner::Runtime,
                name,
                destination: FieldDestination::State(Namespace::Scheduler),
                shape: FieldShape::Value,
            },
            value,
        )?;
    }
    for name in [
        "runtime_backpressure_stats.inflight_effect_dispatch_blocked",
        "runtime_backpressure_stats.journal_events_evicted",
    ] {
        sink.process(FieldDescriptor {
            owner: FieldOwner::Runtime,
            name,
            destination: FieldDestination::Process,
            shape: FieldShape::Process,
        })
        .map_err(ProjectionError::Sink)?;
    }
    Ok(())
}

macro_rules! fields {
    ($($name:ident => $mode:ident, $shape:ident, $destination:expr;)*) => {
        pub const RUNTIME_FIELDS: &[FieldDescriptor] = &[$(FieldDescriptor { owner: FieldOwner::Runtime, name: stringify!($name), destination: $destination, shape: FieldShape::$shape },)*
            FieldDescriptor { owner: FieldOwner::Runtime, name: "bft_governance_parent", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "devlocal_participant_authority", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "formal_execution_tick", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "formal_history_reader", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "governance_replaying", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "module_cache", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "persistence_dir", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "prepared_subscription_cache", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "receipt_signer", destination: FieldDestination::Process, shape: FieldShape::Process },
            FieldDescriptor { owner: FieldOwner::Runtime, name: "source_compiler", destination: FieldDestination::Process, shape: FieldShape::Process },
        ];
        impl World {
            pub fn iws_runtime_fields() -> &'static [FieldDescriptor] { RUNTIME_FIELDS }
            pub fn visit_iws_genesis_fields<S: GenesisFieldVisitor>(&self, sink: &mut S) -> Result<(), ProjectionError<S::Error>> {
                // No `..`: ordinary and skipped process fields are both covered.
                let World { $($name: _,)*
                    bft_governance_parent: _,
                    devlocal_participant_authority: _,
                    formal_execution_tick: _,
                    formal_history_reader: _,
                    governance_replaying: _,
                    module_cache: _,
                    persistence_dir: _,
                    prepared_subscription_cache: _,
                    receipt_signer: _,
                    source_compiler: _,
                    #[cfg(test)] fail_append_after_publication_prepare_countdown: _,
                    #[cfg(test)] fail_next_append_after_publication_prepare: _,
                    #[cfg(test)] fail_next_append_after_reducer: _,
                } = self;
                $(let _field = FieldDescriptor { owner: FieldOwner::Runtime, name: stringify!($name), destination: $destination, shape: FieldShape::$shape };
                  field_visit!(self, sink, _field, $name, $mode)?;)*
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "bft_governance_parent", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "devlocal_participant_authority", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "formal_execution_tick", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "formal_history_reader", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "governance_replaying", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "module_cache", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "persistence_dir", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "prepared_subscription_cache", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "receipt_signer", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                sink.process(FieldDescriptor { owner: FieldOwner::Runtime, name: "source_compiler", destination: FieldDestination::Process, shape: FieldShape::Process }).map_err(ProjectionError::Sink)?;
                Ok(())
            }
        }
    }
}
macro_rules! field_visit {
    ($this:expr, $sink:expr, $field:expr, $name:ident, limits) => {
        visit_limits(&$this.runtime_memory_limits, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, backpressure) => {
        visit_backpressure(&$this.runtime_backpressure_stats, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, state) => {
        $this.state.visit_iws_genesis_fields($sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, cognition) => {
        cognition::visit(&$this.cognition, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, registry) => {
        visit_registry(&$this.module_registry, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, policies) => {
        visit_policies(&$this.policies, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, routing) => {
        visit_routing(&$this.module_tick_routing_metrics, $sink)
    };
    ($this:expr, $sink:expr, $field:expr, $name:ident, $mode:ident) => {
        oasis7_world_state::visit_iws_field!($sink, $field, &$this.$name, $mode)
    };
}
fields! {
    manifest => value, Value, FieldDestination::State(Namespace::Artifacts);
    cognition => cognition, Children, FieldDestination::Children;
    module_registry => registry, Children, FieldDestination::Children;
    module_artifacts => set, Set, FieldDestination::State(Namespace::Artifacts);
    module_artifact_bytes => bytes, ByteMap, FieldDestination::State(Namespace::Artifacts);
    module_limits_max => value, Value, FieldDestination::State(Namespace::Artifacts);
    snapshot_catalog => side, History, FieldDestination::History;
    chain_resource_manifest => value, Value, FieldDestination::State(Namespace::Artifacts);
    latest_chain_resource_delta => value, Value, FieldDestination::State(Namespace::Artifacts);
    state => state, Children, FieldDestination::Children;
    journal => side, History, FieldDestination::History;
    next_event_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_event_id_era => value, Value, FieldDestination::State(Namespace::Allocator);
    next_action_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_action_id_era => value, Value, FieldDestination::State(Namespace::Allocator);
    next_intent_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_intent_id_era => value, Value, FieldDestination::State(Namespace::Allocator);
    next_proposal_id => value, Value, FieldDestination::State(Namespace::Allocator);
    next_proposal_id_era => value, Value, FieldDestination::State(Namespace::Allocator);
    pending_actions => ordered, Ordered, FieldDestination::State(Namespace::Scheduler);
    pending_effects => ordered, Ordered, FieldDestination::State(Namespace::Effects);
    inflight_effects => map, Map, FieldDestination::State(Namespace::Effects);
    module_tick_schedule => map, Map, FieldDestination::State(Namespace::Scheduler);
    module_tick_routing_metrics => routing, Value, FieldDestination::State(Namespace::Scheduler);
    capabilities => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_grants_v2 => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_revocation_state => value, Value, FieldDestination::State(Namespace::Authorization);
    capability_nonce_records => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_authorization_receipts => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_invocation_contexts => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_authorization_root => value, Value, FieldDestination::State(Namespace::Authorization);
    capability_budget_accounts => map, Map, FieldDestination::State(Namespace::Authorization);
    capability_effect_receipt_links => map, Map, FieldDestination::State(Namespace::Effects);
    policies => policies, Children, FieldDestination::Children;
    proposals => map, Map, FieldDestination::State(Namespace::Authorization);
    scheduler_cursor => value, Value, FieldDestination::State(Namespace::Scheduler);
    runtime_memory_limits => limits, Children, FieldDestination::Children;
    runtime_backpressure_stats => backpressure, Children, FieldDestination::Children;
    logistics_sla_metrics => value, Value, FieldDestination::State(Namespace::Business);
    threat_heatmap => map, Map, FieldDestination::State(Namespace::Business);
    tick_consensus_records => side, History, FieldDestination::History;
    formal_archived_tick_consensus_count => side, History, FieldDestination::History;
    tick_consensus_authority_source => value, Value, FieldDestination::State(Namespace::Authorization);
    tick_consensus_rejection_audit_events => side, History, FieldDestination::History;
    governance_execution_policy => value, Value, FieldDestination::State(Namespace::Authorization);
    governance_finality_epoch_snapshots => map, Map, FieldDestination::State(Namespace::Authorization);
    governance_emergency_brake_until_tick => value, Value, FieldDestination::State(Namespace::Authorization);
    governance_identity_penalties => map, Map, FieldDestination::State(Namespace::Authorization);
    next_governance_identity_penalty_id => value, Value, FieldDestination::State(Namespace::Allocator);
    builtin_release_manifest => value, Value, FieldDestination::State(Namespace::Artifacts);
    release_security_policy => value, Value, FieldDestination::State(Namespace::Artifacts);
    rollback_authority_registry => value, Value, FieldDestination::State(Namespace::Authorization);
    consumed_rollback_nonces => set, Set, FieldDestination::State(Namespace::Authorization);
    rollback_nonce_outcomes => map, Map, FieldDestination::State(Namespace::Authorization);
}

#[cfg(test)]
#[path = "incremental_projection_tests.rs"]
mod tests;
