use super::mapping_helpers::runtime_event_kind_label;
use crate::runtime::{
    MaterialStack as RuntimeMaterialStack, RejectReason as RuntimeRejectReason,
    WorldEvent as RuntimeWorldEvent,
};
use oasis7_simulator::{
    RejectReason as SimulatorRejectReason, ResourceOwner, WorldEventKind, WorldModel,
};

pub fn seed_location_id_for_agent_or_pos(
    seed_model: Option<&WorldModel>,
    agent_id: &str,
    pos: crate::geometry::GeoPos,
) -> Option<String> {
    let seed = seed_model?;
    seed.agents
        .get(agent_id)
        .and_then(|agent| seed.locations.get(&agent.location_id))
        .filter(|location| location.pos == pos)
        .map(|location| location.id.clone())
        .or_else(|| seed_location_id_for_pos(Some(seed), pos))
}

pub fn seed_location_id_for_pos(
    seed_model: Option<&WorldModel>,
    pos: crate::geometry::GeoPos,
) -> Option<String> {
    seed_model?
        .locations
        .values()
        .find(|location| location.pos == pos)
        .map(|location| location.id.clone())
}

pub fn runtime_reject_reason_to_simulator(reason: &RuntimeRejectReason) -> SimulatorRejectReason {
    match reason {
        RuntimeRejectReason::AgentAlreadyExists { agent_id } => {
            SimulatorRejectReason::AgentAlreadyExists {
                agent_id: agent_id.clone(),
            }
        }
        RuntimeRejectReason::AgentNotFound { agent_id } => SimulatorRejectReason::AgentNotFound {
            agent_id: agent_id.clone(),
        },
        RuntimeRejectReason::AgentsNotCoLocated {
            agent_id,
            other_agent_id,
        } => SimulatorRejectReason::AgentsNotCoLocated {
            agent_id: agent_id.clone(),
            other_agent_id: other_agent_id.clone(),
        },
        RuntimeRejectReason::InvalidAmount { amount } => {
            SimulatorRejectReason::InvalidAmount { amount: *amount }
        }
        RuntimeRejectReason::InsufficientResource {
            agent_id,
            kind,
            requested,
            available,
        } => SimulatorRejectReason::InsufficientResource {
            owner: ResourceOwner::Agent {
                agent_id: agent_id.clone(),
            },
            kind: *kind,
            requested: *requested,
            available: *available,
        },
        RuntimeRejectReason::FactoryNotFound { factory_id } => {
            SimulatorRejectReason::FacilityNotFound {
                facility_id: factory_id.clone(),
            }
        }
        RuntimeRejectReason::RuleDenied { notes } => SimulatorRejectReason::RuleDenied {
            notes: notes.clone(),
        },
        other => SimulatorRejectReason::RuleDenied {
            notes: vec![format!("runtime reject: {other:?}")],
        },
    }
}

pub fn runtime_fallback_event_kind(runtime_event: &RuntimeWorldEvent) -> WorldEventKind {
    let (kind, domain_kind) = runtime_event_kind_label(&runtime_event.body);
    WorldEventKind::RuntimeEvent { kind, domain_kind }
}

pub fn runtime_structured_event(kind: &str, domain_kind: String) -> WorldEventKind {
    WorldEventKind::RuntimeEvent {
        kind: kind.to_string(),
        domain_kind: Some(domain_kind),
    }
}

pub fn material_stack_summary(stacks: &[RuntimeMaterialStack]) -> String {
    if stacks.is_empty() {
        return "none".to_string();
    }

    stacks
        .iter()
        .map(|stack| {
            format!(
                "{}x{}",
                fallback_non_empty(&stack.kind, "unknown_material"),
                stack.amount
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn fallback_non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    // Summary fallbacks are display/observability labels only; the preserved
    // runtime payload remains the source of truth for empty or unknown fields.
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

pub fn action_accepted_player_feedback(notes: &[String]) -> &'static str {
    if notes.iter().any(|note| !note.trim().is_empty()) {
        "Action accepted — processing queued."
    } else {
        "Action accepted."
    }
}
