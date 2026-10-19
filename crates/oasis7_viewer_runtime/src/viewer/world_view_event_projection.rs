//! Explicit historical positions from the already-authorized typed event stream.
use crate::runtime::{DomainEvent, WorldEvent, WorldEventBody};
use oasis7_agent_api::module_visual::ModuleVisualMutation;
use oasis7_agent_api::world_entity::ModuleVisualAnchor;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn project(events: &[WorldEvent], world: &str, epoch: u64) -> Vec<Value> {
    let mut unique = BTreeMap::new();
    for event in events {
        if unique
            .insert(event.id, event)
            .is_some_and(|old| old != event)
        {
            return Vec::new();
        }
    }
    unique
        .values()
        .rev()
        .filter_map(|event| {
            let (kind, id, pos, visibility_kind) = match &event.body {
                WorldEventBody::Domain(DomainEvent::AgentRegistered { agent_id, pos }) => {
                    ("agent_registered", agent_id.as_str(), pos, "agent")
                }
                WorldEventBody::ModuleEmitted(emit) => {
                    let ModuleVisualMutation::Upsert(entity) =
                        oasis7_agent_api::module_visual::parse_module_visual_emit(emit).ok()??
                    else {
                        return None;
                    };
                    let ModuleVisualAnchor::Absolute { pos } = entity.anchor else {
                        return None;
                    };
                    return Some(record(
                        world,
                        epoch,
                        event,
                        "module_visual_entity_upserted",
                        &entity.entity_id,
                        &pos,
                        "module_visual",
                    ));
                }
                _ => return None,
            };
            Some(record(world, epoch, event, kind, id, pos, visibility_kind))
        })
        .take(200)
        .collect()
}

fn record(
    world: &str,
    epoch: u64,
    event: &WorldEvent,
    kind: &str,
    id: &str,
    pos: &crate::geometry::GeoPos,
    visibility_kind: &str,
) -> Value {
    let identity =
        json!({"world_id":world,"reorg_epoch":epoch.to_string(),"event_seq":event.id.to_string()});
    let label = if kind == "agent_registered" {
        "Agent joined"
    } else {
        "Object updated"
    };
    json!({"event_id":format!("{world}:{epoch}:{}",event.id),"identity":identity,"feed_ref":identity,
        "kind":kind,"label":format!("{label}: {id}"),"logical_time":event.time.to_string(),
        "position":{"x_cm":pos.x_cm.to_string(),"y_cm":pos.y_cm.to_string(),"z_cm":pos.z_cm.to_string()},
        "source_class":"runtime_journal_payload","freshness":"last_known",
        "visibility_ref":{"kind":visibility_kind,"id":id}})
}
