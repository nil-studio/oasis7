use super::*;

#[test]
fn init_accepts_module_visual_entities_with_all_anchor_types() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents.count = 1;
    init.module_visual_entities = vec![
        ModuleVisualEntity {
            entity_id: "mv-agent".to_string(),
            module_id: "m.test.agent".to_string(),
            kind: "beacon".to_string(),
            label: Some("agent beacon".to_string()),
            anchor: ModuleVisualAnchor::Agent {
                agent_id: "agent-0".to_string(),
            },
        },
        ModuleVisualEntity {
            entity_id: "mv-location".to_string(),
            module_id: "m.test.location".to_string(),
            kind: "relay".to_string(),
            label: None,
            anchor: ModuleVisualAnchor::Location {
                location_id: "origin".to_string(),
            },
        },
        ModuleVisualEntity {
            entity_id: "mv-absolute".to_string(),
            module_id: "m.test.absolute".to_string(),
            kind: "artifact".to_string(),
            label: None,
            anchor: ModuleVisualAnchor::Absolute {
                pos: GeoPos::new(10, 10, 0),
            },
        },
    ];

    let (model, report) = build_world_model(&config, &init).expect("init world");

    assert_eq!(report.agents, 1);
    assert_eq!(model.module_visual_entities.len(), 3);
    assert!(model.module_visual_entities.contains_key("mv-agent"));
    assert!(model.module_visual_entities.contains_key("mv-location"));
    assert!(model.module_visual_entities.contains_key("mv-absolute"));
}

#[test]
fn init_rejects_module_visual_when_agent_anchor_missing() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.agents.count = 0;
    init.module_visual_entities = vec![ModuleVisualEntity {
        entity_id: "mv-missing-agent".to_string(),
        module_id: "m.test".to_string(),
        kind: "artifact".to_string(),
        label: None,
        anchor: ModuleVisualAnchor::Agent {
            agent_id: "ghost-agent".to_string(),
        },
    }];

    let err = build_world_model(&config, &init).unwrap_err();
    assert!(matches!(
        err,
        WorldInitError::ModuleVisualEntityAnchorNotFound {
            entity_id,
            anchor: ModuleVisualAnchor::Agent { .. },
        } if entity_id == "mv-missing-agent"
    ));
}

#[test]
fn init_rejects_module_visual_when_absolute_anchor_out_of_bounds() {
    let mut config = WorldConfig::default();
    config.space.width_cm = 100;
    config.space.depth_cm = 100;
    config.space.height_cm = 100;

    let mut init = WorldInitConfig::default();
    init.agents.count = 0;
    init.module_visual_entities = vec![ModuleVisualEntity {
        entity_id: "mv-oob".to_string(),
        module_id: "m.test".to_string(),
        kind: "artifact".to_string(),
        label: None,
        anchor: ModuleVisualAnchor::Absolute {
            pos: GeoPos::new(101, 0, 0),
        },
    }];

    let err = build_world_model(&config, &init).unwrap_err();
    assert!(matches!(
        err,
        WorldInitError::ModuleVisualEntityAnchorNotFound {
            entity_id,
            anchor: ModuleVisualAnchor::Absolute { .. },
        } if entity_id == "mv-oob"
    ));
}

#[test]
fn kernel_supports_module_visual_entity_upsert_and_remove() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let visual = ModuleVisualEntity {
        entity_id: "mv-1".to_string(),
        module_id: "m.runtime.test".to_string(),
        kind: "sensor".to_string(),
        label: Some("sensor-1".to_string()),
        anchor: ModuleVisualAnchor::Agent {
            agent_id: "agent-1".to_string(),
        },
    };

    kernel.submit_action(Action::UpsertModuleVisualEntity {
        entity: visual.clone(),
    });
    let upsert_event = kernel.step().expect("upsert event");
    assert!(matches!(
        upsert_event.kind,
        WorldEventKind::ModuleVisualEntityUpserted { ref entity }
            if entity.entity_id == "mv-1" && entity.module_id == "m.runtime.test"
    ));
    assert!(kernel.model().module_visual_entities.contains_key("mv-1"));

    kernel.submit_action(Action::RemoveModuleVisualEntity {
        entity_id: "mv-1".to_string(),
    });
    let remove_event = kernel.step().expect("remove event");
    assert!(matches!(
        remove_event.kind,
        WorldEventKind::ModuleVisualEntityRemoved { ref entity_id } if entity_id == "mv-1"
    ));
    assert!(!kernel.model().module_visual_entities.contains_key("mv-1"));
}

#[test]
fn kernel_rejects_module_visual_upsert_when_anchor_missing() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::UpsertModuleVisualEntity {
        entity: ModuleVisualEntity {
            entity_id: "mv-unknown".to_string(),
            module_id: "m.runtime.test".to_string(),
            kind: "sensor".to_string(),
            label: None,
            anchor: ModuleVisualAnchor::Agent {
                agent_id: "agent-missing".to_string(),
            },
        },
    });

    let event = kernel.step().expect("reject event");
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::AgentNotFound { .. },
        }
    ));
}

#[test]
fn replay_from_snapshot_applies_module_visual_upsert_event() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.kernel_snapshot();

    kernel.submit_action(Action::UpsertModuleVisualEntity {
        entity: ModuleVisualEntity {
            entity_id: "mv-replay".to_string(),
            module_id: "m.runtime.replay".to_string(),
            kind: "relay".to_string(),
            label: None,
            anchor: ModuleVisualAnchor::Location {
                location_id: "loc-1".to_string(),
            },
        },
    });
    kernel.step().expect("upsert event");

    let replayed =
        WorldKernel::replay_from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
            .expect("replay");

    assert!(
        replayed
            .model()
            .module_visual_entities
            .contains_key("mv-replay")
    );
}
