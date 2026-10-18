use std::collections::BTreeMap;

use oasis7_agent_api::world_action::GeoPos;
use crate::major_world_event::project_major_world_events_with_canonical_state_indexed_for_test;
use crate::{
    CausedBy, CrisisState, CrisisStatus, DomainEvent, MajorWorldEventCategory,
    MajorWorldEventFreshness, MajorWorldEventLifecycle, MajorWorldEventProjectionContext,
    MajorWorldEventVisibilityPermission, WorldEvent, WorldEventBody, project_major_world_events,
    project_major_world_events_with_canonical_state,
};

fn crisis_spawned(id: u64, crisis_id: &str, severity: u32) -> WorldEvent {
    WorldEvent {
        id,
        time: id,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::CrisisSpawned {
            crisis_id: crisis_id.to_string(),
            kind: "power_shortage".to_string(),
            severity,
            expires_at: id + 20,
        }),
    }
}

fn crisis_resolved(id: u64, crisis_id: &str, caused_by: Option<CausedBy>) -> WorldEvent {
    WorldEvent {
        id,
        time: id,
        caused_by,
        body: WorldEventBody::Domain(DomainEvent::CrisisResolved {
            resolver_agent_id: "agent-resolver".to_string(),
            crisis_id: crisis_id.to_string(),
            strategy: "reroute_power".to_string(),
            success: true,
            impact: -2,
        }),
    }
}

fn crisis_timed_out(id: u64, crisis_id: &str) -> WorldEvent {
    WorldEvent {
        id,
        time: id,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::CrisisTimedOut {
            crisis_id: crisis_id.to_string(),
            penalty_impact: -4,
        }),
    }
}

fn canonical_crisis(
    crisis_id: &str,
    kind: &str,
    severity: u32,
    status: CrisisStatus,
    opened_at: u64,
    expires_at: u64,
    resolved_at: Option<u64>,
) -> CrisisState {
    CrisisState {
        crisis_id: crisis_id.to_string(),
        kind: kind.to_string(),
        severity,
        status,
        opened_at,
        expires_at,
        resolver_agent_id: (status == CrisisStatus::Resolved).then(|| "agent-resolver".to_string()),
        strategy: (status == CrisisStatus::Resolved).then(|| "reroute_power".to_string()),
        success: match status {
            CrisisStatus::Active => None,
            CrisisStatus::Resolved => Some(true),
            CrisisStatus::TimedOut => Some(false),
        },
        impact: match status {
            CrisisStatus::Active => 0,
            CrisisStatus::Resolved => -2,
            CrisisStatus::TimedOut => -4,
        },
        resolved_at,
    }
}

fn canonical_crisis_map(state: CrisisState) -> BTreeMap<String, CrisisState> {
    BTreeMap::from([(state.crisis_id.clone(), state)])
}

#[test]
fn projects_major_events_with_complete_identity_and_authority_metadata() {
    let events = vec![
        crisis_timed_out(4, "crisis-2"),
        crisis_resolved(3, "crisis-1", Some(CausedBy::Action(41))),
        crisis_spawned(1, "crisis-1", 4),
        crisis_spawned(2, "crisis-2", 5),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 7);

    let projections = project_major_world_events(&events, &context);

    assert_eq!(projections.len(), 4);
    assert_eq!(projections[0].identity.world_id, "world-a");
    assert_eq!(projections[0].identity.reorg_epoch, 7);
    assert_eq!(projections[0].identity.event_seq, 1);
    assert_eq!(projections[0].category, MajorWorldEventCategory::Crisis);
    assert_eq!(projections[0].subtype, "power_shortage");
    assert_eq!(projections[0].severity, 4);
    assert_eq!(projections[0].lifecycle, MajorWorldEventLifecycle::Active);
    assert_eq!(projections[0].freshness, MajorWorldEventFreshness::Current);
    assert_eq!(
        projections[0].visibility,
        MajorWorldEventVisibilityPermission::Public
    );
    let anchor = projections[0]
        .world_anchor
        .as_ref()
        .expect("world-scoped crisis anchor");
    assert_eq!(anchor.entity_id.as_deref(), Some("crisis-1"));
    assert!(anchor.position.is_none());
    assert!(projections[0].causal_reference.is_none());

    assert_eq!(projections[1].lifecycle, MajorWorldEventLifecycle::Active);
    assert_eq!(projections[1].severity, 5);
    assert_eq!(projections[2].lifecycle, MajorWorldEventLifecycle::Resolved);
    assert_eq!(projections[2].severity, 4);
    assert_eq!(
        projections[2].causal_reference,
        Some(crate::MajorWorldEventCausalReference::Action(41))
    );
    assert_eq!(projections[3].lifecycle, MajorWorldEventLifecycle::TimedOut);
    assert_eq!(projections[3].severity, 5);
    assert_eq!(projections[3].category, MajorWorldEventCategory::Crisis);
}

#[test]
fn projection_is_explicitly_fail_closed_for_non_major_or_incomplete_events() {
    let events = vec![
        WorldEvent {
            id: 1,
            time: 1,
            caused_by: None,
            body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
                agent_id: "agent-1".to_string(),
                pos: GeoPos::new(1, 2, 3),
            }),
        },
        crisis_resolved(2, "unknown-crisis", None),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);

    assert!(project_major_world_events(&events, &context).is_empty());
}

#[test]
fn severity_outside_authoritative_crisis_range_and_conflicting_ids_fail_closed() {
    let invalid_severity = crisis_spawned(1, "crisis-invalid", 6);
    let conflicting_identity = vec![
        crisis_spawned(2, "crisis-a", 2),
        crisis_spawned(2, "crisis-b", 3),
    ];
    let conflicting_spawn_history = vec![
        crisis_spawned(1, "crisis-c", 2),
        crisis_spawned(2, "crisis-c", 3),
        crisis_resolved(3, "crisis-c", None),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);

    assert!(project_major_world_events(&[invalid_severity], &context).is_empty());
    assert!(project_major_world_events(&conflicting_identity, &context).is_empty());
    assert!(project_major_world_events(&conflicting_spawn_history, &context).is_empty());
}

#[test]
fn replay_marks_freshness_and_deduplicates_event_identity_deterministically() {
    let event = crisis_spawned(9, "crisis-9", 2);
    let events = vec![event.clone(), event.clone(), event];
    let context = MajorWorldEventProjectionContext::replay_public("world-a", 3);

    let projections = project_major_world_events(&events, &context);

    assert_eq!(projections.len(), 1);
    assert_eq!(
        projections[0].freshness,
        MajorWorldEventFreshness::LastKnown
    );
    assert_eq!(projections[0].identity.event_seq, 9);
    assert_eq!(
        projections[0]
            .world_anchor
            .as_ref()
            .and_then(|anchor| anchor.entity_id.as_deref()),
        Some("crisis-9")
    );
}

#[test]
fn same_identity_with_conflicting_payload_fails_closed() {
    let events = vec![
        crisis_spawned(9, "crisis-9", 2),
        crisis_spawned(9, "other", 2),
    ];
    let context = MajorWorldEventProjectionContext::replay_public("world-a", 3);

    assert!(project_major_world_events(&events, &context).is_empty());
}

#[test]
fn gap_reorg_unknown_freshness_and_denied_visibility_never_emit_events() {
    let events = vec![crisis_spawned(1, "crisis-1", 2)];
    assert!(
        project_major_world_events(
            &events,
            &MajorWorldEventProjectionContext::current("world-a", 1)
        )
        .is_empty()
    );
    assert!(
        project_major_world_events(
            &events,
            &MajorWorldEventProjectionContext::gap("world-a", 1)
        )
        .is_empty()
    );
    assert!(
        project_major_world_events(
            &events,
            &MajorWorldEventProjectionContext::reorg("world-a", 2)
        )
        .is_empty()
    );
    assert!(
        project_major_world_events(
            &events,
            &MajorWorldEventProjectionContext::with_freshness(
                "world-a",
                1,
                MajorWorldEventFreshness::Unknown
            )
        )
        .is_empty()
    );
    assert!(
        project_major_world_events(
            &events,
            &MajorWorldEventProjectionContext::with_permission(
                "world-a",
                1,
                MajorWorldEventVisibilityPermission::Denied
            )
        )
        .is_empty()
    );
}

#[test]
fn projection_roundtrips_and_keeps_schema_additive() {
    let events = vec![crisis_spawned(1, "crisis-1", 2)];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);
    let projection = &project_major_world_events(&events, &context)[0];

    let encoded = serde_json::to_string(projection).expect("serialize projection");
    let decoded: crate::MajorWorldEventProjection =
        serde_json::from_str(&encoded).expect("deserialize projection");
    assert_eq!(&decoded, projection);
    assert_eq!(
        decoded.schema_version,
        crate::MAJOR_WORLD_EVENT_PROJECTION_SCHEMA_VERSION
    );
}

#[test]
fn terminal_events_require_a_canonical_crisis_state_join() {
    let events = vec![
        crisis_spawned(1, "crisis-1", 4),
        crisis_resolved(2, "crisis-1", None),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);
    let canonical = canonical_crisis_map(canonical_crisis(
        "crisis-1",
        "power_shortage",
        4,
        CrisisStatus::Resolved,
        1,
        21,
        Some(2),
    ));

    let projections =
        project_major_world_events_with_canonical_state(&events, &canonical, &context);
    assert_eq!(projections.len(), 2);
    assert_eq!(projections[1].lifecycle, MajorWorldEventLifecycle::Resolved);

    // A terminal journal row without the authoritative persisted state is not
    // safe to disclose, even when its preceding spawn is present.
    assert!(
        project_major_world_events_with_canonical_state(&events, &BTreeMap::new(), &context)
            .is_empty()
    );
}

#[test]
fn timed_out_event_recovers_canonical_severity_kind_and_terminal_lifecycle() {
    let events = vec![
        crisis_spawned(1, "crisis-timeout", 5),
        crisis_timed_out(2, "crisis-timeout"),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);
    let canonical = canonical_crisis_map(canonical_crisis(
        "crisis-timeout",
        "power_shortage",
        5,
        CrisisStatus::TimedOut,
        1,
        21,
        Some(2),
    ));

    let projections =
        project_major_world_events_with_canonical_state(&events, &canonical, &context);
    assert_eq!(projections.len(), 2);
    assert_eq!(projections[1].lifecycle, MajorWorldEventLifecycle::TimedOut);
    assert_eq!(projections[1].severity, 5);
    assert_eq!(projections[1].subtype, "power_shortage");
    assert_eq!(projections[1].freshness, MajorWorldEventFreshness::Current);
}

#[test]
fn canonical_crisis_join_fails_closed_on_missing_or_conflicting_fields() {
    let events = vec![
        crisis_spawned(1, "crisis-1", 4),
        crisis_resolved(2, "crisis-1", None),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);
    let valid = canonical_crisis(
        "crisis-1",
        "power_shortage",
        4,
        CrisisStatus::Resolved,
        1,
        21,
        Some(2),
    );

    let mut conflicting_severity = valid.clone();
    conflicting_severity.severity = 3;
    let mut conflicting_status = valid.clone();
    conflicting_status.status = CrisisStatus::Active;
    conflicting_status.resolved_at = None;
    let mut conflicting_kind = valid.clone();
    conflicting_kind.kind = "water_shortage".to_string();
    let mut conflicting_opened_at = valid.clone();
    conflicting_opened_at.opened_at = 0;
    let mut conflicting_expires_at = valid.clone();
    conflicting_expires_at.expires_at = 99;
    let mut conflicting_resolved_at = valid.clone();
    conflicting_resolved_at.resolved_at = Some(3);
    let mut empty_id = valid.clone();
    empty_id.crisis_id.clear();
    let mut empty_kind = valid.clone();
    empty_kind.kind.clear();

    for conflicting in [
        conflicting_severity,
        conflicting_status,
        conflicting_kind,
        conflicting_opened_at,
        conflicting_expires_at,
        conflicting_resolved_at,
        empty_id,
        empty_kind,
    ] {
        assert!(
            project_major_world_events_with_canonical_state(
                &events,
                &canonical_crisis_map(conflicting),
                &context
            )
            .is_empty()
        );
    }
}

#[test]
fn current_feed_marks_only_the_canonical_latest_lifecycle_current() {
    let events = vec![
        crisis_spawned(1, "crisis-1", 4),
        crisis_resolved(2, "crisis-1", None),
    ];
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);
    let canonical = canonical_crisis_map(canonical_crisis(
        "crisis-1",
        "power_shortage",
        4,
        CrisisStatus::Resolved,
        1,
        21,
        Some(2),
    ));

    let projections =
        project_major_world_events_with_canonical_state(&events, &canonical, &context);
    assert_eq!(projections.len(), 2);
    assert_ne!(projections[0].freshness, MajorWorldEventFreshness::Current);
    assert_eq!(projections[1].freshness, MajorWorldEventFreshness::Current);
}

#[test]
fn cursor_replay_makes_every_canonical_transition_historical() {
    let events = vec![
        crisis_spawned(1, "crisis-1", 4),
        crisis_resolved(2, "crisis-1", None),
    ];
    let context = MajorWorldEventProjectionContext::replay_public("world-a", 1);
    let canonical = canonical_crisis_map(canonical_crisis(
        "crisis-1",
        "power_shortage",
        4,
        CrisisStatus::Resolved,
        1,
        21,
        Some(2),
    ));

    let projections =
        project_major_world_events_with_canonical_state(&events, &canonical, &context);
    assert_eq!(projections.len(), 2);
    assert!(
        projections
            .iter()
            .all(|projection| projection.freshness == MajorWorldEventFreshness::LastKnown)
    );
}

#[test]
fn bounded_journal_projection_builds_one_linear_history_index() {
    const JOURNAL_SIZE: u64 = 65_536;
    const CRISIS_STRIDE: u64 = 32;
    let mut events = Vec::with_capacity(JOURNAL_SIZE as usize);
    let mut canonical = BTreeMap::new();
    for id in 1..=JOURNAL_SIZE {
        if id % CRISIS_STRIDE == 0 {
            let crisis_id = format!("crisis-{id}");
            events.push(crisis_spawned(id, &crisis_id, (id % 5 + 1) as u32));
            canonical.insert(
                crisis_id.clone(),
                canonical_crisis(
                    &crisis_id,
                    "power_shortage",
                    (id % 5 + 1) as u32,
                    CrisisStatus::Active,
                    id,
                    id + 20,
                    None,
                ),
            );
        } else {
            events.push(WorldEvent {
                id,
                time: id,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
                    agent_id: format!("ambient-{id}"),
                    pos: GeoPos::new(id as i64, 0, 0),
                }),
            });
        }
    }
    let context = MajorWorldEventProjectionContext::current_public("world-a", 1);

    let (projections, indexed_event_count) =
        project_major_world_events_with_canonical_state_indexed_for_test(
            &events, &canonical, &context,
        );

    assert_eq!(indexed_event_count, JOURNAL_SIZE as usize);
    assert_eq!(projections.len(), (JOURNAL_SIZE / CRISIS_STRIDE) as usize);
    assert_eq!(projections[0].identity.event_seq, CRISIS_STRIDE);
    assert_eq!(
        projections
            .last()
            .expect("last crisis projection")
            .identity
            .event_seq,
        JOURNAL_SIZE
    );
}
