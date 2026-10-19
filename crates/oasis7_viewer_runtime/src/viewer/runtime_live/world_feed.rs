use super::*;
use crate::viewer::protocol;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const MODULE_VISUAL_ENTITY_UPSERTED_KIND: &str = "module_visual_entity_upserted";
const MODULE_VISUAL_ENTITY_REMOVED_KIND: &str = "module_visual_entity_removed";

#[derive(Debug)]
struct ModuleVisualEventProjection {
    kind: &'static str,
    entity_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    world_id: String,
    reorg_epoch: u64,
    last_event_seq: u64,
}

pub(super) fn build_world_feed(
    world_id: &str,
    reorg_epoch: u64,
    journal: &RuntimeJournal,
    canonical_crises: &BTreeMap<String, crate::runtime::CrisisState>,
    cursor: Option<&str>,
    requested_limit: usize,
    visibility: crate::runtime::MajorWorldEventVisibilityPermission,
) -> protocol::WorldFeedEnvelope {
    if has_conflicting_event_identity(journal) {
        return envelope(
            world_id,
            reorg_epoch,
            encode_cursor(world_id, reorg_epoch, latest_seq(journal)),
            Vec::new(),
            protocol::WorldFeedStatus::Gap,
            Some(protocol::WorldFeedGapReason::EventIdentityConflict),
            None,
        );
    }

    let decoded = cursor.map(decode_cursor);
    let gap_reason = match decoded.as_ref() {
        Some(Err(())) => Some(protocol::WorldFeedGapReason::CursorInvalid),
        Some(Ok(cursor)) if cursor.world_id != world_id => {
            Some(protocol::WorldFeedGapReason::CursorInvalid)
        }
        Some(Ok(cursor)) if cursor.reorg_epoch != reorg_epoch => {
            Some(protocol::WorldFeedGapReason::ReorgEpochChanged)
        }
        Some(Ok(cursor)) => cursor_gap_reason(journal, cursor.last_event_seq),
        None => None,
    };
    if let Some(reason) = gap_reason {
        return envelope(
            world_id,
            reorg_epoch,
            encode_cursor(world_id, reorg_epoch, latest_seq(journal)),
            Vec::new(),
            protocol::WorldFeedStatus::Gap,
            Some(reason),
            None,
        );
    }

    let after_seq = decoded
        .as_ref()
        .and_then(|cursor| cursor.as_ref().ok())
        .map_or(0, |cursor| cursor.last_event_seq);
    let limit = if requested_limit == 0 {
        DEFAULT_LIMIT
    } else {
        requested_limit.min(MAX_LIMIT)
    };
    let mut source_events: Vec<_> = journal
        .events
        .iter()
        .filter(|event| event.id > after_seq)
        .collect();
    source_events.sort_by_key(|event| event.id);
    source_events.dedup_by_key(|event| event.id);
    let major_event_context = if cursor.is_some() {
        crate::runtime::MajorWorldEventProjectionContext::new(
            world_id,
            reorg_epoch,
            crate::runtime::MajorWorldEventFreshness::LastKnown,
            visibility,
            crate::runtime::MajorWorldEventStreamState::Replay,
        )
    } else {
        crate::runtime::MajorWorldEventProjectionContext::new(
            world_id,
            reorg_epoch,
            crate::runtime::MajorWorldEventFreshness::Current,
            visibility,
            crate::runtime::MajorWorldEventStreamState::Current,
        )
    };
    let major_events_by_seq: BTreeMap<_, _> =
        crate::runtime::project_major_world_events_with_canonical_state(
            &journal.events,
            canonical_crises,
            &major_event_context,
        )
        .into_iter()
        .map(|projection| {
            let event_seq = projection.identity.event_seq;
            (event_seq, project_major_event(projection))
        })
        .collect();
    let page_events: Vec<_> = source_events.into_iter().take(limit).collect();
    let first_hidden_crisis_seq = page_events.iter().find_map(|event| {
        (is_crisis_event(event) && !major_events_by_seq.contains_key(&event.id)).then_some(event.id)
    });
    // Never advance past a crisis row that cannot be projected authoritatively.
    // In particular, a later permission grant must be able to replay the row.
    let last_seq = first_hidden_crisis_seq.map_or_else(
        || page_events.last().map_or(after_seq, |event| event.id),
        |event_seq| event_seq.saturating_sub(1),
    );
    // Unknown audience authority remains fail-closed, but is not proof of an
    // explicit denial. Keep that distinction in the wire reason so callers
    // can distinguish unavailable source metadata from permission rejection.
    let permission_denied = first_hidden_crisis_seq.is_some_and(|_| {
        matches!(
            visibility,
            crate::runtime::MajorWorldEventVisibilityPermission::Denied
        )
    });
    let authority_unavailable = first_hidden_crisis_seq.is_some() && !permission_denied;
    let events: Vec<_> = page_events
        .into_iter()
        .filter_map(|event| {
            let major_event = major_events_by_seq.get(&event.id);
            // Crisis rows are security-sensitive as a whole.  When their
            // explicit authority projection is unavailable (unknown/denied
            // permission, invalid severity, conflict, or missing history), do
            // not leak the legacy summary/detail representation either.
            if is_crisis_event(event) && major_event.is_none() {
                None
            } else {
                Some(project_event(event, major_event))
            }
        })
        .collect();
    let status = if permission_denied || authority_unavailable {
        protocol::WorldFeedStatus::Unavailable
    } else if events.is_empty() {
        protocol::WorldFeedStatus::Empty
    } else if cursor.is_some() {
        protocol::WorldFeedStatus::Replay
    } else {
        protocol::WorldFeedStatus::Ready
    };
    envelope(
        world_id,
        reorg_epoch,
        encode_cursor(world_id, reorg_epoch, last_seq),
        events,
        status,
        None,
        if permission_denied {
            Some(protocol::WorldFeedUnavailableReason::PermissionDenied)
        } else if authority_unavailable {
            Some(protocol::WorldFeedUnavailableReason::SourceUnavailable)
        } else {
            None
        },
    )
}

fn has_conflicting_event_identity(journal: &RuntimeJournal) -> bool {
    let mut events_by_id = BTreeMap::new();
    for event in &journal.events {
        if let Some(previous) = events_by_id.insert(event.id, event)
            && previous != event
        {
            return true;
        }
    }
    false
}

fn is_crisis_event(event: &crate::runtime::WorldEvent) -> bool {
    matches!(
        &event.body,
        crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::CrisisSpawned { .. }
                | crate::runtime::DomainEvent::CrisisResolved { .. }
                | crate::runtime::DomainEvent::CrisisTimedOut { .. }
        )
    )
}

fn envelope(
    world_id: &str,
    reorg_epoch: u64,
    cursor: String,
    events: Vec<protocol::WorldFeedEvent>,
    status: protocol::WorldFeedStatus,
    gap_reason: Option<protocol::WorldFeedGapReason>,
    unavailable_reason: Option<protocol::WorldFeedUnavailableReason>,
) -> protocol::WorldFeedEnvelope {
    protocol::WorldFeedEnvelope {
        schema_version: protocol::WORLD_FEED_SCHEMA_VERSION.to_string(),
        world_id: world_id.to_string(),
        reorg_epoch,
        cursor,
        events,
        status,
        gap_reason,
        unavailable_reason,
        snapshot_reload_required: status == protocol::WorldFeedStatus::Gap,
    }
}

fn cursor_gap_reason(
    journal: &RuntimeJournal,
    last_event_seq: u64,
) -> Option<protocol::WorldFeedGapReason> {
    let mut event_ids: Vec<_> = journal.events.iter().map(|event| event.id).collect();
    event_ids.sort_unstable();
    event_ids.dedup();
    let latest = event_ids.last().copied().unwrap_or(0);
    if last_event_seq > latest {
        return Some(protocol::WorldFeedGapReason::CursorInvalid);
    }

    let mut expected = last_event_seq.saturating_add(1);
    for event_id in event_ids
        .into_iter()
        .filter(|event_id| *event_id > last_event_seq)
    {
        if event_id != expected {
            return Some(protocol::WorldFeedGapReason::CursorGap);
        }
        if event_id == u64::MAX {
            break;
        }
        expected = event_id + 1;
    }
    None
}

fn latest_seq(journal: &RuntimeJournal) -> u64 {
    journal
        .events
        .iter()
        .map(|event| event.id)
        .max()
        .unwrap_or(0)
}

fn project_event(
    event: &crate::runtime::WorldEvent,
    major_event: Option<&protocol::WorldFeedMajorEvent>,
) -> protocol::WorldFeedEvent {
    let body = serde_json::to_value(&event.body).unwrap_or(serde_json::Value::Null);
    let source_kind = body
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("RuntimeEvent");
    let module_visual_projection = project_module_visual_event(&event.body);
    let kind = module_visual_projection.as_ref().map_or_else(
        || snake_case(source_kind),
        |projection| projection.kind.to_string(),
    );
    protocol::WorldFeedEvent {
        event_seq: event.id,
        summary: format!("{} event", kind.replace('_', " ")),
        kind,
        detail: serde_json::to_string(&event.body).unwrap_or_else(|_| "null".to_string()),
        // No durable runtime receipt identity is available yet; never infer one.
        receipt_ref: None,
        module_visual_entity_id: module_visual_projection.map(|projection| projection.entity_id),
        major_event: major_event.cloned(),
    }
}

/// Project the explicit module visual mutation contract carried by a durable
/// `ModuleEmitted` event.  The payload is decoded as typed data; `detail` is
/// deliberately not inspected because it is only a legacy display string.
fn project_module_visual_event(
    body: &crate::runtime::WorldEventBody,
) -> Option<ModuleVisualEventProjection> {
    let crate::runtime::WorldEventBody::ModuleEmitted(module_emit) = body else {
        return None;
    };

    match oasis7_agent_api::module_visual::parse_module_visual_emit(module_emit).ok()?? {
        oasis7_agent_api::module_visual::ModuleVisualMutation::Upsert(entity) => {
            let entity_id = non_empty_identity(&entity.entity_id)?;
            Some(ModuleVisualEventProjection {
                kind: MODULE_VISUAL_ENTITY_UPSERTED_KIND,
                entity_id,
            })
        }
        oasis7_agent_api::module_visual::ModuleVisualMutation::Remove(entity_id) => {
            Some(ModuleVisualEventProjection {
                kind: MODULE_VISUAL_ENTITY_REMOVED_KIND,
                entity_id: non_empty_identity(&entity_id)?,
            })
        }
    }
}

fn non_empty_identity(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn project_major_event(
    projection: crate::runtime::MajorWorldEventProjection,
) -> protocol::WorldFeedMajorEvent {
    protocol::WorldFeedMajorEvent {
        schema_version: projection.schema_version,
        identity: protocol::WorldFeedMajorEventIdentity {
            world_id: projection.identity.world_id,
            reorg_epoch: projection.identity.reorg_epoch.to_string(),
            event_seq: projection.identity.event_seq.to_string(),
        },
        category: "crisis".to_string(),
        subtype: projection.subtype,
        severity: projection.severity,
        lifecycle: match projection.lifecycle {
            crate::runtime::MajorWorldEventLifecycle::Active => "active",
            crate::runtime::MajorWorldEventLifecycle::Resolved => "resolved",
            crate::runtime::MajorWorldEventLifecycle::TimedOut => "timed_out",
        }
        .to_string(),
        source: protocol::WorldFeedMajorEventSource {
            authority: "runtime_journal".to_string(),
            event_kind: projection.source.event_kind,
        },
        freshness: match projection.freshness {
            crate::runtime::MajorWorldEventFreshness::Current => "current",
            // Freshness is event authority, while Replay is the independent
            // feed transport state. Preserve both dimensions on the wire.
            crate::runtime::MajorWorldEventFreshness::LastKnown => "last_known",
            crate::runtime::MajorWorldEventFreshness::Stale => "stale",
            crate::runtime::MajorWorldEventFreshness::Unknown => "unknown",
            crate::runtime::MajorWorldEventFreshness::Conflict => "conflict",
            crate::runtime::MajorWorldEventFreshness::Reconnecting => "reconnecting",
            crate::runtime::MajorWorldEventFreshness::Unavailable => "unavailable",
        }
        .to_string(),
        visibility: match projection.visibility {
            crate::runtime::MajorWorldEventVisibilityPermission::Public => "public",
            crate::runtime::MajorWorldEventVisibilityPermission::Restricted => "restricted",
            crate::runtime::MajorWorldEventVisibilityPermission::Denied => "denied",
            crate::runtime::MajorWorldEventVisibilityPermission::Unknown => "unknown",
        }
        .to_string(),
        logical_time: projection.logical_time,
        causal_reference: projection
            .causal_reference
            .map(|reference| match reference {
                crate::runtime::MajorWorldEventCausalReference::Action(action_id) => {
                    protocol::WorldFeedMajorEventCausalReference::Action(action_id.to_string())
                }
                crate::runtime::MajorWorldEventCausalReference::Effect { intent_id } => {
                    protocol::WorldFeedMajorEventCausalReference::Effect { intent_id }
                }
            }),
        world_anchor: projection
            .world_anchor
            .map(|anchor| protocol::WorldFeedMajorEventAnchor {
                scope: "world".to_string(),
                entity_id: anchor.entity_id,
                position: anchor
                    .position
                    .map(|position| protocol::WorldFeedMajorEventPosition {
                        x_cm: position.x_cm,
                        y_cm: position.y_cm,
                        z_cm: position.z_cm,
                    }),
            }),
    }
}

fn snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn encode_cursor(world_id: &str, reorg_epoch: u64, last_event_seq: u64) -> String {
    let cursor = Cursor {
        world_id: world_id.to_string(),
        reorg_epoch,
        last_event_seq,
    };
    let bytes = serde_json::to_vec(&cursor).expect("serialize world feed cursor");
    format!("wf1.{}", hex::encode(bytes))
}

fn decode_cursor(value: &str) -> Result<Cursor, ()> {
    let bytes = hex::decode(value.strip_prefix("wf1.").ok_or(())?).map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        Journal, MajorWorldEventVisibilityPermission, SnapshotMeta, WorldEvent, WorldEventBody,
    };

    fn module_emit_event(id: u64, kind: &str, payload: serde_json::Value) -> WorldEvent {
        WorldEvent {
            id,
            time: id,
            caused_by: None,
            body: WorldEventBody::ModuleEmitted(oasis7_wasm_abi::ModuleEmitEvent {
                module_id: "fixture.visual-module".to_string(),
                trace_id: format!("trace-{id}"),
                kind: kind.to_string(),
                payload,
            }),
        }
    }

    fn event(id: u64) -> WorldEvent {
        WorldEvent {
            id,
            time: id,
            caused_by: None,
            body: WorldEventBody::SnapshotCreated(SnapshotMeta {
                journal_len: id as usize,
            }),
        }
    }

    fn crisis_event(id: u64, body: crate::runtime::DomainEvent) -> WorldEvent {
        WorldEvent {
            id,
            time: id,
            caused_by: None,
            body: WorldEventBody::Domain(body),
        }
    }

    #[test]
    fn projection_is_ascending_deduplicated_and_receipt_free() {
        let journal = Journal {
            events: vec![event(3), event(1), event(3), event(2)],
        };
        let feed = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(feed.status, protocol::WorldFeedStatus::Ready);
        assert_eq!(
            feed.events
                .iter()
                .map(|event| event.event_seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(feed.events.iter().all(|event| event.receipt_ref.is_none()));
        assert!(!feed.snapshot_reload_required);
    }

    #[test]
    fn module_visual_emit_events_project_authoritative_ids_for_upsert_and_remove() {
        let entity = oasis7_agent_api::world_entity::ModuleVisualEntity {
            entity_id: "module-relay".to_string(),
            module_id: "fixture.visual-module".to_string(),
            kind: "relay".to_string(),
            label: Some("Relay".to_string()),
            anchor: oasis7_agent_api::world_entity::ModuleVisualAnchor::Absolute {
                pos: crate::geometry::GeoPos::new(100, 200, 0),
            },
        };
        let upsert_payload = serde_json::json!({
            "entity": serde_json::to_value(entity).expect("encode visual entity")
        });
        let journal = Journal {
            events: vec![
                module_emit_event(1, "ModuleVisualEntityUpserted", upsert_payload),
                module_emit_event(
                    2,
                    "ModuleVisualEntityRemoved",
                    serde_json::json!({ "entity_id": "module-relay" }),
                ),
                event(3),
            ],
        };
        let replayed_journal =
            Journal::from_json(&journal.to_json().expect("encode runtime journal"))
                .expect("decode runtime journal");

        let feed = build_world_feed(
            "world-a",
            4,
            &replayed_journal,
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Public,
        );

        assert_eq!(feed.status, protocol::WorldFeedStatus::Ready);
        assert_eq!(feed.events[0].kind, "module_visual_entity_upserted");
        assert_eq!(
            feed.events[0].module_visual_entity_id.as_deref(),
            Some("module-relay")
        );
        assert_eq!(feed.events[1].kind, "module_visual_entity_removed");
        assert_eq!(
            feed.events[1].module_visual_entity_id.as_deref(),
            Some("module-relay")
        );
        assert_eq!(feed.events[2].kind, "snapshot_created");
        assert_eq!(feed.events[2].module_visual_entity_id, None);
    }

    #[test]
    fn module_visual_events_do_not_require_major_event_visibility_authority() {
        let entity = oasis7_agent_api::world_entity::ModuleVisualEntity {
            entity_id: "runtime-entity".to_string(),
            module_id: "fixture.visual-module".to_string(),
            kind: "runtime_driver".to_string(),
            label: None,
            anchor: oasis7_agent_api::world_entity::ModuleVisualAnchor::Absolute {
                pos: crate::geometry::GeoPos::new(1, 2, 3),
            },
        };
        let feed = build_world_feed(
            "world-a",
            4,
            &Journal {
                events: vec![module_emit_event(
                    1,
                    MODULE_VISUAL_ENTITY_UPSERTED_KIND,
                    serde_json::json!({
                        "entity": serde_json::to_value(entity).expect("encode visual entity")
                    }),
                )],
            },
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Unknown,
        );

        assert_eq!(feed.status, protocol::WorldFeedStatus::Ready);
        assert_eq!(feed.unavailable_reason, None);
        assert_eq!(feed.events.len(), 1);
        assert_eq!(
            feed.events[0].module_visual_entity_id.as_deref(),
            Some("runtime-entity")
        );
    }

    #[test]
    fn unrelated_module_emits_do_not_infer_visual_entity_identity() {
        let journal = Journal {
            events: vec![module_emit_event(
                1,
                "telemetry",
                serde_json::json!({ "entity_id": "module-relay" }),
            )],
        };
        let feed = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Public,
        );

        assert_eq!(feed.events.len(), 1);
        assert_eq!(feed.events[0].kind, "module_emitted");
        assert_eq!(feed.events[0].module_visual_entity_id, None);
    }

    #[test]
    fn malformed_visual_emit_does_not_infer_identity_from_detail() {
        let journal = Journal {
            events: vec![module_emit_event(
                1,
                "ModuleVisualEntityUpserted",
                serde_json::json!({ "entity_id": "module-relay" }),
            )],
        };
        let feed = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Public,
        );

        assert_eq!(feed.events.len(), 1);
        assert_eq!(feed.events[0].kind, "module_emitted");
        assert_eq!(feed.events[0].module_visual_entity_id, None);
        assert!(feed.events[0].detail.contains("module-relay"));
    }

    #[test]
    fn malformed_visual_emit_missing_module_or_anchor_stays_generic() {
        for field in ["module_id", "anchor"] {
            let mut entity = serde_json::json!({
                "entity_id": "module-relay",
                "module_id": "fixture.visual-module",
                "kind": "relay",
                "anchor": {
                    "type": "absolute",
                    "data": { "pos": { "x_cm": 1, "y_cm": 2, "z_cm": 0 } }
                }
            });
            entity.as_object_mut().unwrap().remove(field);
            let feed = build_world_feed(
                "world-a",
                4,
                &Journal {
                    events: vec![module_emit_event(
                        1,
                        "ModuleVisualEntityUpserted",
                        serde_json::json!({ "entity": entity }),
                    )],
                },
                &BTreeMap::new(),
                None,
                50,
                MajorWorldEventVisibilityPermission::Public,
            );

            assert_eq!(feed.events[0].kind, "module_emitted");
            assert_eq!(feed.events[0].module_visual_entity_id, None);
        }
    }

    #[test]
    fn conflicting_duplicate_event_identity_requires_snapshot_reload() {
        let mut conflicting = event(2);
        conflicting.time = 99;
        let journals = [
            Journal {
                events: vec![event(1), event(2), conflicting, event(3)],
            },
            Journal {
                events: vec![
                    crisis_event(
                        1,
                        crate::runtime::DomainEvent::CrisisSpawned {
                            crisis_id: "conflict-a".to_string(),
                            kind: "power_shortage".to_string(),
                            severity: 2,
                            expires_at: 10,
                        },
                    ),
                    crisis_event(
                        1,
                        crate::runtime::DomainEvent::CrisisSpawned {
                            crisis_id: "conflict-b".to_string(),
                            kind: "power_shortage".to_string(),
                            severity: 3,
                            expires_at: 10,
                        },
                    ),
                    event(2),
                ],
            },
        ];

        for journal in journals {
            let feed = build_world_feed(
                "world-a",
                4,
                &journal,
                &BTreeMap::new(),
                None,
                50,
                MajorWorldEventVisibilityPermission::Public,
            );

            assert_eq!(feed.status, protocol::WorldFeedStatus::Gap);
            assert_eq!(
                feed.gap_reason,
                Some(protocol::WorldFeedGapReason::EventIdentityConflict)
            );
            assert!(feed.events.is_empty());
            assert!(feed.snapshot_reload_required);
        }
    }

    #[test]
    fn cursor_replays_only_newer_events() {
        let journal = Journal {
            events: vec![event(1), event(2), event(3)],
        };
        let first = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            None,
            2,
            MajorWorldEventVisibilityPermission::Public,
        );
        let replay = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            Some(&first.cursor),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(replay.status, protocol::WorldFeedStatus::Replay);
        assert_eq!(
            replay
                .events
                .iter()
                .map(|event| event.event_seq)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }

    #[test]
    fn reorg_and_retention_gap_require_snapshot_reload() {
        let journal = Journal {
            events: vec![event(5), event(6)],
        };
        let stale = encode_cursor("world-a", 3, 4);
        let reorg = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            Some(&stale),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(
            reorg.gap_reason,
            Some(protocol::WorldFeedGapReason::ReorgEpochChanged)
        );
        assert!(reorg.snapshot_reload_required);

        let evicted = encode_cursor("world-a", 4, 2);
        let gap = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            Some(&evicted),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(
            gap.gap_reason,
            Some(protocol::WorldFeedGapReason::CursorGap)
        );
        assert!(gap.snapshot_reload_required);
    }

    #[test]
    fn invalid_cursor_fails_closed_as_gap() {
        let feed = build_world_feed(
            "world-a",
            4,
            &Journal::new(),
            &BTreeMap::new(),
            Some("invalid"),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(feed.status, protocol::WorldFeedStatus::Gap);
        assert_eq!(
            feed.gap_reason,
            Some(protocol::WorldFeedGapReason::CursorInvalid)
        );
        assert!(feed.snapshot_reload_required);
    }

    #[test]
    fn crisis_events_carry_additive_authority_and_replay_freshness() {
        let journal = Journal {
            events: vec![
                crisis_event(
                    1,
                    crate::runtime::DomainEvent::CrisisSpawned {
                        crisis_id: "crisis-1".to_string(),
                        kind: "power_shortage".to_string(),
                        severity: 4,
                        expires_at: 10,
                    },
                ),
                event(2),
            ],
        };
        let canonical_crises = BTreeMap::from([(
            "crisis-1".to_string(),
            crate::runtime::CrisisState {
                crisis_id: "crisis-1".to_string(),
                kind: "power_shortage".to_string(),
                severity: 4,
                status: crate::runtime::CrisisStatus::Active,
                opened_at: 1,
                expires_at: 10,
                resolver_agent_id: None,
                strategy: None,
                success: None,
                impact: 0,
                resolved_at: None,
            },
        )]);
        let unknown_permission = build_world_feed(
            "world-a",
            4,
            &journal,
            &canonical_crises,
            None,
            50,
            MajorWorldEventVisibilityPermission::Unknown,
        );
        assert_eq!(
            unknown_permission.status,
            protocol::WorldFeedStatus::Unavailable
        );
        assert_eq!(
            unknown_permission.unavailable_reason,
            Some(protocol::WorldFeedUnavailableReason::SourceUnavailable)
        );
        assert!(!unknown_permission.snapshot_reload_required);
        assert_eq!(unknown_permission.cursor, encode_cursor("world-a", 4, 0));
        assert_eq!(unknown_permission.events.len(), 1);
        assert_eq!(unknown_permission.events[0].event_seq, 2);
        assert!(unknown_permission.events[0].major_event.is_none());
        assert!(
            unknown_permission
                .events
                .iter()
                .all(|event| !event.detail.contains("Crisis"))
        );
        let replay_after_permission = build_world_feed(
            "world-a",
            4,
            &journal,
            &canonical_crises,
            Some(&unknown_permission.cursor),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(
            replay_after_permission.status,
            protocol::WorldFeedStatus::Replay
        );
        assert_eq!(replay_after_permission.events[0].event_seq, 1);
        assert!(replay_after_permission.events[0].major_event.is_some());

        let denied_cursor = encode_cursor("world-a", 4, 0);
        let denied_permission = build_world_feed(
            "world-a",
            4,
            &journal,
            &canonical_crises,
            Some(&denied_cursor),
            50,
            MajorWorldEventVisibilityPermission::Denied,
        );
        assert_eq!(
            denied_permission.status,
            protocol::WorldFeedStatus::Unavailable
        );
        assert_eq!(
            denied_permission.unavailable_reason,
            Some(protocol::WorldFeedUnavailableReason::PermissionDenied)
        );
        assert_eq!(denied_permission.cursor, denied_cursor);
        assert_eq!(denied_permission.events.len(), 1);
        assert_eq!(denied_permission.events[0].event_seq, 2);

        let current = build_world_feed(
            "world-a",
            4,
            &journal,
            &canonical_crises,
            None,
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        let current_major = current.events[0]
            .major_event
            .as_ref()
            .expect("current crisis authority");
        assert_eq!(current_major.category, "crisis");
        assert_eq!(current_major.subtype, "power_shortage");
        assert_eq!(current_major.severity, 4);
        assert_eq!(current_major.lifecycle, "active");
        assert_eq!(current_major.freshness, "current");
        assert_eq!(current_major.visibility, "public");
        assert_eq!(current_major.identity.reorg_epoch, "4");
        assert_eq!(current_major.identity.event_seq, "1");
        assert!(
            current
                .events
                .iter()
                .all(|event| event.receipt_ref.is_none())
        );

        let replay_cursor = encode_cursor("world-a", 4, 0);
        let replay = build_world_feed(
            "world-a",
            4,
            &journal,
            &canonical_crises,
            Some(&replay_cursor),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(replay.status, protocol::WorldFeedStatus::Replay);
        assert_eq!(
            replay.events[0].major_event.as_ref().unwrap().freshness,
            "last_known"
        );

        let gap = build_world_feed(
            "world-a",
            5,
            &journal,
            &canonical_crises,
            Some(&encode_cursor("world-a", 4, 0)),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(gap.status, protocol::WorldFeedStatus::Gap);
        assert!(gap.events.is_empty());
    }

    #[test]
    fn internal_cursor_hole_requires_snapshot_reload() {
        let journal = Journal {
            events: vec![event(1), event(3), event(4)],
        };
        let cursor = encode_cursor("world-a", 4, 1);
        let feed = build_world_feed(
            "world-a",
            4,
            &journal,
            &BTreeMap::new(),
            Some(&cursor),
            50,
            MajorWorldEventVisibilityPermission::Public,
        );
        assert_eq!(feed.status, protocol::WorldFeedStatus::Gap);
        assert_eq!(
            feed.gap_reason,
            Some(protocol::WorldFeedGapReason::CursorGap)
        );
        assert!(feed.events.is_empty());
        assert!(feed.snapshot_reload_required);
    }

    #[test]
    fn public_crisis_without_canonical_authority_is_explicitly_unavailable() {
        let cases = [
            Journal {
                events: vec![
                    crisis_event(
                        1,
                        crate::runtime::DomainEvent::CrisisSpawned {
                            crisis_id: "invalid".to_string(),
                            kind: "power_shortage".to_string(),
                            severity: 0,
                            expires_at: 10,
                        },
                    ),
                    event(2),
                ],
            },
            Journal {
                events: vec![
                    crisis_event(
                        1,
                        crate::runtime::DomainEvent::CrisisResolved {
                            crisis_id: "missing".to_string(),
                            resolver_agent_id: "agent-1".to_string(),
                            strategy: "repair".to_string(),
                            success: true,
                            impact: 1,
                        },
                    ),
                    event(2),
                ],
            },
        ];

        for journal in cases {
            let feed = build_world_feed(
                "world-a",
                4,
                &journal,
                &BTreeMap::new(),
                None,
                50,
                MajorWorldEventVisibilityPermission::Public,
            );
            assert_eq!(feed.status, protocol::WorldFeedStatus::Unavailable);
            assert_eq!(
                feed.unavailable_reason,
                Some(protocol::WorldFeedUnavailableReason::SourceUnavailable)
            );
            assert_eq!(feed.cursor, encode_cursor("world-a", 4, 0));
            assert_eq!(
                feed.events.len(),
                1,
                "ordinary ambient rows remain compatible"
            );
            assert_eq!(feed.events[0].event_seq, 2);
            assert!(feed.events[0].major_event.is_none());
            assert!(!feed.snapshot_reload_required);
        }

        let restricted = build_world_feed(
            "world-a",
            4,
            &Journal {
                events: vec![crisis_event(
                    1,
                    crate::runtime::DomainEvent::CrisisSpawned {
                        crisis_id: "restricted-invalid".to_string(),
                        kind: "power_shortage".to_string(),
                        severity: 0,
                        expires_at: 10,
                    },
                )],
            },
            &BTreeMap::new(),
            None,
            50,
            MajorWorldEventVisibilityPermission::Restricted,
        );
        assert_eq!(restricted.status, protocol::WorldFeedStatus::Unavailable);
        assert_eq!(
            restricted.unavailable_reason,
            Some(protocol::WorldFeedUnavailableReason::SourceUnavailable)
        );
        assert_eq!(restricted.cursor, encode_cursor("world-a", 4, 0));
    }
}
