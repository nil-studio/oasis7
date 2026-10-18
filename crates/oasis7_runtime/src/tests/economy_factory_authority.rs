use super::economy_factory_lifecycle::{factory_spec, install_factory_authority, register_builder};
use crate::{
    Action, AgentLocationAuthorityV1, DomainEvent, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1,
    MaterialLedgerId, RejectReason, World, WorldError, WorldEventBody, WorldState,
};
use oasis7_agent_api::world_action::ResourceKind;

#[test]
fn legacy_snapshot_without_location_anchor_registry_remains_decodable() {
    let mut value = serde_json::to_value(World::new().state()).expect("serialize world state");
    value
        .as_object_mut()
        .expect("world state object")
        .remove("location_anchors");
    let restored: WorldState = serde_json::from_value(value).expect("decode legacy world state");
    assert!(restored.location_anchors.is_empty());
}

#[test]
fn legacy_location_anchor_without_effective_at_defaults_to_immediate() {
    let mut value = serde_json::to_value(World::new().state()).expect("serialize world state");
    value["location_anchors"] = serde_json::json!({
        "legacy-location": {
            "location_id": "legacy-location",
            "active": true,
            "authority_revision": 1
        }
    });
    let restored: WorldState = serde_json::from_value(value).expect("decode legacy anchor");
    assert_eq!(
        restored
            .location_anchors
            .get("legacy-location")
            .expect("legacy anchor")
            .effective_at,
        0
    );
}

#[test]
fn future_agent_assignment_is_rejected_at_admission_and_replay() {
    let mut world = World::new();
    register_builder(&mut world, "builder-future");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-future".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("seed active anchor");
    let result = world.set_agent_location_authority(AgentLocationAuthorityV1 {
        agent_id: "builder-future".to_string(),
        location_id: "location-future".to_string(),
        active: true,
        authority_revision: 1,
        effective_at: world.state().time.saturating_add(1),
    });
    assert!(matches!(
        result,
        Err(WorldError::ResourceBalanceInvalid { reason })
            if reason.contains("not yet effective")
    ));
    let future_event = DomainEvent::AgentLocationAuthorityUpdated {
        authority: AgentLocationAuthorityV1 {
            agent_id: "builder-future".to_string(),
            location_id: "location-future".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: world.state().time.saturating_add(1),
        },
    };
    let mut replay_state = world.state().clone();
    let replay_error = replay_state
        .apply_domain_event(&future_event, replay_state.time)
        .expect_err("future assignment must also fail during replay");
    assert!(matches!(
        replay_error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("not yet effective")
    ));
    let future_revocation = world.set_agent_location_authority(AgentLocationAuthorityV1 {
        agent_id: "builder-future".to_string(),
        location_id: "location-future".to_string(),
        active: false,
        authority_revision: 1,
        effective_at: world.state().time.saturating_add(1),
    });
    assert!(matches!(
        future_revocation,
        Err(WorldError::ResourceBalanceInvalid { reason })
            if reason.contains("not yet effective")
    ));
}

#[test]
fn inactive_anchor_allows_revocation_then_requires_anchor_for_reactivation() {
    let mut world = World::new();
    register_builder(&mut world, "builder-revoke");
    let location_id = "location-revoke".to_string();
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("seed active anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "builder-revoke".to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("activate assignment");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: false,
            authority_revision: 2,
            effective_at: 0,
        })
        .expect("deactivate anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "builder-revoke".to_string(),
            location_id: location_id.clone(),
            active: false,
            authority_revision: 2,
            effective_at: 0,
        })
        .expect("revoke assignment while anchor is inactive");
    let reactivate_before_anchor = world.set_agent_location_authority(AgentLocationAuthorityV1 {
        agent_id: "builder-revoke".to_string(),
        location_id: location_id.clone(),
        active: true,
        authority_revision: 3,
        effective_at: 0,
    });
    assert!(matches!(
        reactivate_before_anchor,
        Err(WorldError::ResourceBalanceInvalid { reason })
            if reason.contains("inactive_or_stale")
    ));
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 3,
            effective_at: 0,
        })
        .expect("reactivate anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "builder-revoke".to_string(),
            location_id,
            active: true,
            authority_revision: 3,
            effective_at: 0,
        })
        .expect("reactivate assignment after anchor");
}

#[test]
fn factory_profile_duplicate_is_exact_replay_only_and_conflict_is_atomic() {
    let mut world = World::new();
    register_builder(&mut world, "profile-operator");
    let profile = FactoryProfileV1 {
        factory_id: "factory.immutable-profile".to_string(),
        tier: 1,
        recipe_slots: 2,
        tags: vec!["assembly".to_string()],
    };
    let event = DomainEvent::FactoryProfileGoverned {
        operator_agent_id: "profile-operator".to_string(),
        proposal_id: 1,
        profile: profile.clone(),
    };
    let mut state = world.state().clone();
    state
        .apply_domain_event(&event, state.time)
        .expect("initial immutable profile");
    let before_exact = serde_json::to_vec(&state).expect("serialize exact profile state");
    state
        .apply_domain_event(&event, state.time)
        .expect("exact profile replay");
    assert_eq!(
        serde_json::to_vec(&state).expect("serialize exact replay state"),
        before_exact
    );

    let mut conflict = profile;
    conflict.recipe_slots = 3;
    let conflicting_event = DomainEvent::FactoryProfileGoverned {
        operator_agent_id: "profile-operator".to_string(),
        proposal_id: 2,
        profile: conflict,
    };
    let before_conflict = serde_json::to_vec(&state).expect("serialize conflict state");
    let error = state
        .apply_domain_event(&conflicting_event, state.time)
        .expect_err("conflicting immutable profile must fail closed");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_eq!(
        serde_json::to_vec(&state).expect("serialize after conflict"),
        before_conflict
    );
}

#[test]
fn location_anchor_is_required_for_authority_and_build_admission() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");

    let authority = AgentLocationAuthorityV1 {
        agent_id: "builder-a".to_string(),
        location_id: "location-site-1".to_string(),
        active: true,
        authority_revision: 1,
        effective_at: 0,
    };
    let missing_anchor = world.set_agent_location_authority(authority.clone());
    assert!(matches!(
        missing_anchor,
        Err(WorldError::ResourceBalanceInvalid { reason }) if reason.contains("location anchor unknown")
    ));

    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-site-1".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("register exact location anchor");
    world
        .set_agent_location_authority(authority)
        .expect("active anchor permits agent assignment");

    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-site-1".to_string(),
            active: false,
            authority_revision: 2,
            effective_at: 0,
        })
        .expect("deactivate location anchor");
    let site_result = world.set_factory_site_authority(FactorySiteAuthorityV1 {
        site_id: "site-1".to_string(),
        location_id: "location-site-1".to_string(),
        owner_agent_id: "builder-a".to_string(),
        authorized_agent_ids: Vec::new(),
        chunk_ready: true,
        active: true,
        authority_revision: 1,
        registered_at: 0,
    });
    assert!(matches!(
        site_result,
        Err(WorldError::ResourceBalanceInvalid { reason })
            if reason.contains("location anchor inactive_or_stale")
    ));
}

#[test]
fn zero_revision_site_authority_is_rejected_at_build_admission() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut state = WorldState::default();
    state
        .apply_domain_event(
            &DomainEvent::AgentRegistered {
                agent_id: "builder-zero-revision".to_string(),
                pos: oasis7_agent_api::world_action::GeoPos {
                    x_cm: 0,
                    y_cm: 0,
                    z_cm: 0,
                },
            },
            0,
        )
        .expect("register builder");
    state.location_anchors.insert(
        "location-zero-revision".to_string(),
        LocationAnchorV1 {
            location_id: "location-zero-revision".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    state.agent_location_authorities.insert(
        "builder-zero-revision".to_string(),
        AgentLocationAuthorityV1 {
            agent_id: "builder-zero-revision".to_string(),
            location_id: "location-zero-revision".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    state.factory_site_authorities.insert(
        "site-zero-revision".to_string(),
        FactorySiteAuthorityV1 {
            site_id: "site-zero-revision".to_string(),
            location_id: "location-zero-revision".to_string(),
            owner_agent_id: "builder-zero-revision".to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 0,
            registered_at: 0,
        },
    );
    state.factory_construction_power_profiles.insert(
        "factory.zero-site-revision".to_string(),
        FactoryConstructionPowerProfileV1 {
            factory_id: "factory.zero-site-revision".to_string(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: CONSTRUCTION_POWER,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        },
    );
    state.factory_profiles.insert(
        "factory.zero-site-revision".to_string(),
        FactoryProfileV1 {
            factory_id: "factory.zero-site-revision".to_string(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["lifecycle".to_string()],
        },
    );
    state
        .resources
        .insert(ResourceKind::Electricity, CONSTRUCTION_POWER);
    state.material_ledgers.insert(
        MaterialLedgerId::agent("builder-zero-revision"),
        [
            ("steel_plate".to_string(), 10),
            ("circuit_board".to_string(), 2),
        ]
        .into_iter()
        .collect(),
    );
    let mut world = World::new_with_state(state);
    world
        .set_agent_resource_balance(
            "builder-zero-revision",
            ResourceKind::Electricity,
            CONSTRUCTION_POWER,
        )
        .expect("seed builder electricity");
    let spec = factory_spec("factory.zero-site-revision", 1, 1, 1);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-zero-revision".to_string(),
        site_id: "site-zero-revision".to_string(),
        spec,
    });

    world
        .step()
        .expect("zero-revision site authority should be a structured rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert_eq!(
        world
            .agent_resource_balance("builder-zero-revision", ResourceKind::Electricity)
            .expect("read builder electricity"),
        CONSTRUCTION_POWER,
        "zero-revision site authority must not sink construction power"
    );
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("site_access_or_location_blocked"))
        )
    }));
}

#[test]
fn zero_revision_agent_location_authority_is_rejected_at_build_admission() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut state = WorldState::default();
    state
        .apply_domain_event(
            &DomainEvent::AgentRegistered {
                agent_id: "builder-zero-location-revision".to_string(),
                pos: oasis7_agent_api::world_action::GeoPos {
                    x_cm: 0,
                    y_cm: 0,
                    z_cm: 0,
                },
            },
            0,
        )
        .expect("register builder");
    state.location_anchors.insert(
        "location-zero-location-revision".to_string(),
        LocationAnchorV1 {
            location_id: "location-zero-location-revision".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    state.agent_location_authorities.insert(
        "builder-zero-location-revision".to_string(),
        AgentLocationAuthorityV1 {
            agent_id: "builder-zero-location-revision".to_string(),
            location_id: "location-zero-location-revision".to_string(),
            active: true,
            authority_revision: 0,
            effective_at: 0,
        },
    );
    state.factory_site_authorities.insert(
        "site-zero-location-revision".to_string(),
        FactorySiteAuthorityV1 {
            site_id: "site-zero-location-revision".to_string(),
            location_id: "location-zero-location-revision".to_string(),
            owner_agent_id: "builder-zero-location-revision".to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        },
    );
    state.factory_construction_power_profiles.insert(
        "factory.zero-location-revision".to_string(),
        FactoryConstructionPowerProfileV1 {
            factory_id: "factory.zero-location-revision".to_string(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: CONSTRUCTION_POWER,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        },
    );
    state.factory_profiles.insert(
        "factory.zero-location-revision".to_string(),
        FactoryProfileV1 {
            factory_id: "factory.zero-location-revision".to_string(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["lifecycle".to_string()],
        },
    );
    state
        .resources
        .insert(ResourceKind::Electricity, CONSTRUCTION_POWER);
    state.material_ledgers.insert(
        MaterialLedgerId::agent("builder-zero-location-revision"),
        [
            ("steel_plate".to_string(), 10),
            ("circuit_board".to_string(), 2),
        ]
        .into_iter()
        .collect(),
    );
    let mut world = World::new_with_state(state);
    world
        .set_agent_resource_balance(
            "builder-zero-location-revision",
            ResourceKind::Electricity,
            CONSTRUCTION_POWER,
        )
        .expect("seed builder electricity");
    let spec = factory_spec("factory.zero-location-revision", 1, 1, 1);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-zero-location-revision".to_string(),
        site_id: "site-zero-location-revision".to_string(),
        spec,
    });

    world
        .step()
        .expect("zero-revision agent location authority should be a structured rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert_eq!(
        world
            .agent_resource_balance("builder-zero-location-revision", ResourceKind::Electricity)
            .expect("read builder electricity"),
        CONSTRUCTION_POWER,
        "zero-revision agent location authority must not sink construction power"
    );
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("site_access_or_location_blocked"))
        )
    }));
}

#[test]
fn inactive_exact_anchor_blocks_build_without_resource_sink() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.anchor-gated", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    world
        .set_ledger_material_balance(builder_ledger.clone(), "steel_plate", 10)
        .expect("seed build steel");
    world
        .set_ledger_material_balance(builder_ledger.clone(), "circuit_board", 2)
        .expect("seed build circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed build power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-site-1".to_string(),
            active: false,
            authority_revision: 2,
            effective_at: 0,
        })
        .expect("deactivate exact location anchor");

    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("anchor-gated build rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&builder_ledger, "steel_plate"),
        10
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("read build power"),
        CONSTRUCTION_POWER
    );
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes
                .iter()
                .any(|note| note.contains("location anchor inactive_or_stale"))
        )
    }));
}

#[test]
fn future_exact_anchor_blocks_build_without_resource_sink() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.future-anchor", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    world
        .set_ledger_material_balance(builder_ledger.clone(), "steel_plate", 10)
        .expect("seed future-anchor steel");
    world
        .set_ledger_material_balance(builder_ledger.clone(), "circuit_board", 2)
        .expect("seed future-anchor circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed future-anchor power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-site-1".to_string(),
            active: true,
            authority_revision: 2,
            effective_at: world.state().time.saturating_add(2),
        })
        .expect("register future location anchor");

    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world
        .step()
        .expect("future anchor should become a structured rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&builder_ledger, "steel_plate"),
        10
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("read future-anchor power"),
        CONSTRUCTION_POWER
    );
    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("not yet effective"))
        )
    }));
}

#[test]
fn factory_build_pins_location_anchor_revision_and_replays_after_anchor_mutation() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.pinned-anchor", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed pinned-anchor construction material");
    }
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed pinned-anchor power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    let snapshot_before_build = world.snapshot();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start pinned-anchor build");

    let started = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
                location_anchor_revision,
                ..
            }) => Some(*location_anchor_revision),
            _ => None,
        })
        .expect("FactoryBuildStarted event");
    assert_eq!(started, Some(1));
    assert_eq!(
        world
            .state()
            .pending_factory_builds
            .values()
            .next()
            .and_then(|job| job.location_anchor_revision),
        Some(1)
    );

    let mut stripped_modern_journal = world.journal().clone();
    for event in &mut stripped_modern_journal.events {
        if let WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
            contract_version,
            location_anchor_revision,
            ..
        }) = &mut event.body
        {
            assert_eq!(*contract_version, Some(1));
            *location_anchor_revision = None;
        }
    }
    let modern_error = World::from_snapshot(snapshot_before_build.clone(), stripped_modern_journal)
        .expect_err("stripped modern construction facts must not be replayed as legacy");
    assert!(matches!(
        modern_error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("modern factory build event is missing")
    ));

    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-site-1".to_string(),
            active: true,
            authority_revision: 2,
            effective_at: 0,
        })
        .expect("rotate location anchor after acceptance");
    world.step().expect("complete pinned-anchor build");
    assert!(world.has_factory("factory.pinned-anchor"));
    assert_eq!(
        world
            .state()
            .factories
            .get("factory.pinned-anchor")
            .and_then(|factory| factory.location_anchor_revision),
        Some(1),
        "completed factory must retain the accepted location anchor revision"
    );

    let restored = World::from_snapshot(snapshot_before_build.clone(), world.journal().clone())
        .expect("replay must preserve accepted anchor pin");
    assert!(restored.has_factory("factory.pinned-anchor"));
    assert_eq!(restored.state(), world.state());

    let mut legacy_journal = world.journal().clone();
    for event in &mut legacy_journal.events {
        if let WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
            contract_version,
            location_anchor_revision,
            ..
        }) = &mut event.body
        {
            *contract_version = Some(0);
            *location_anchor_revision = None;
        }
    }
    let legacy_restored = World::from_snapshot(snapshot_before_build, legacy_journal)
        .expect("historical build event without location pin must replay");
    assert!(legacy_restored.has_factory("factory.pinned-anchor"));
}

#[test]
fn stripped_factory_build_contract_version_cannot_downgrade_modern_event() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.stripped-version", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed stripped-version construction material");
    }
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed stripped-version power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    let snapshot_before_build = world.snapshot();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start stripped-version build");

    let mut journal_json = serde_json::to_value(world.journal()).expect("serialize journal");
    for event in journal_json["events"]
        .as_array_mut()
        .expect("journal events")
    {
        if event["body"]["payload"]["type"] == "FactoryBuildStarted" {
            event["body"]["payload"]["data"]
                .as_object_mut()
                .expect("factory build event")
                .remove("contract_version");
        }
    }
    let stripped_journal: crate::Journal =
        serde_json::from_value(journal_json).expect("decode stripped journal");
    let error = World::from_snapshot(snapshot_before_build, stripped_journal)
        .expect_err("omitting the modern discriminator must not downgrade the event");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("modern factory build event")
    ));
}

#[test]
fn factory_build_apply_rejects_mismatched_location_anchor_revision() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.stale-anchor-event", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed stale-anchor construction material");
    }
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed stale-anchor power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    let snapshot_before_build = world.snapshot();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start stale-anchor build");

    let mut journal = world.journal().clone();
    for event in &mut journal.events {
        if let WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
            location_anchor_revision,
            ..
        }) = &mut event.body
        {
            *location_anchor_revision = Some(99);
        }
    }
    let error = World::from_snapshot(snapshot_before_build, journal)
        .expect_err("mismatched anchor revision must fail replay");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("factory build authority changed or is unavailable")
    ));
}

#[test]
fn build_factory_requires_canonical_profile_capabilities() {
    for (label, mismatch) in [("tier", 0_u8), ("recipe_slots", 1_u8), ("tags", 2_u8)] {
        let mut world = World::new();
        register_builder(&mut world, "builder-a");
        let mut spec = factory_spec("factory.profile-gated", 1, 1, 1);
        match mismatch {
            0 => spec.tier = 2,
            1 => spec.recipe_slots = 2,
            _ => spec.tags = vec!["forged".to_string()],
        }
        let builder_ledger = MaterialLedgerId::agent("builder-a");
        world
            .set_ledger_material_balance(builder_ledger.clone(), "steel_plate", 10)
            .expect("seed profile test steel");
        world
            .set_ledger_material_balance(builder_ledger.clone(), "circuit_board", 2)
            .expect("seed profile test circuits");
        world
            .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
            .expect("seed profile test power");
        install_factory_authority(
            &mut world,
            "builder-a",
            "site-1",
            spec.factory_id.as_str(),
            10,
        );
        world.submit_action(Action::BuildFactory {
            builder_agent_id: "builder-a".to_string(),
            site_id: "site-1".to_string(),
            spec,
        });
        world
            .step()
            .unwrap_or_else(|error| panic!("{label} mismatch should be rejected: {error:?}"));
        assert_eq!(world.pending_factory_builds_len(), 0, "{label} mismatch");
        assert_eq!(
            world.ledger_material_balance(&builder_ledger, "steel_plate"),
            10,
            "{label} mismatch must not consume materials"
        );
    }
}

#[test]
fn build_factory_accepts_profile_tags_after_normalization() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.profile-normalized", 1, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    world
        .set_ledger_material_balance(builder_ledger, "steel_plate", 10)
        .expect("seed normalized profile steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 2)
        .expect("seed normalized profile circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
        .expect("seed normalized profile power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        10,
    );
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: vec![" LIFECYCLE ".to_string(), "lifecycle".to_string()],
        })
        .expect("install normalized factory profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("normalized profile permits build");
    assert_eq!(world.pending_factory_builds_len(), 1);
}

#[test]
fn modern_factory_build_replay_rejects_forged_profile_capabilities() {
    let mut world = World::new();
    register_builder(&mut world, "builder-forged");
    let spec = factory_spec("factory.forged-replay", 1, 1, 1);
    let ledger = MaterialLedgerId::agent("builder-forged");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed forged replay material");
    }
    world
        .set_agent_resource_balance("builder-forged", ResourceKind::Electricity, 10)
        .expect("seed forged replay power");
    install_factory_authority(
        &mut world,
        "builder-forged",
        "site-1",
        spec.factory_id.as_str(),
        10,
    );
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .expect("install canonical replay profile");
    let snapshot = world.snapshot();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-forged".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("accept canonical build");
    let mut journal = world.journal().clone();
    for event in &mut journal.events {
        if let WorldEventBody::Domain(DomainEvent::FactoryBuildStarted { spec, .. }) =
            &mut event.body
        {
            spec.tier = spec.tier.saturating_add(1);
        }
    }
    let error = World::from_snapshot(snapshot, journal)
        .expect_err("forged modern build profile must fail replay");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("canonical profile tier mismatch")
    ));
}

#[test]
fn location_anchor_revision_exhaustion_rejects_without_mutation() {
    let mut state = WorldState::default();
    state.location_anchors.insert(
        "location-exhausted".to_string(),
        LocationAnchorV1 {
            location_id: "location-exhausted".to_string(),
            active: true,
            authority_revision: u64::MAX,
            effective_at: 0,
        },
    );
    let mut world = World::new_with_state(state);
    let before_state = serde_json::to_vec(world.state()).expect("serialize exhausted anchor");
    let before_journal_len = world.journal().len();

    let error = world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location-exhausted".to_string(),
            active: false,
            authority_revision: u64::MAX,
            effective_at: 0,
        })
        .expect_err("exhausted location anchor must fail closed");

    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("location anchor authority revision exhausted at")
    ));
    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize anchor after rejection"),
        before_state
    );
    assert_eq!(world.journal().len(), before_journal_len);
}

#[test]
fn agent_location_revision_exhaustion_rejects_without_mutation() {
    let mut state = WorldState::default();
    state
        .apply_domain_event(
            &DomainEvent::AgentRegistered {
                agent_id: "agent-exhausted".to_string(),
                pos: oasis7_agent_api::world_action::GeoPos {
                    x_cm: 0,
                    y_cm: 0,
                    z_cm: 0,
                },
            },
            0,
        )
        .expect("register exhausted agent");
    state.agent_location_authorities.insert(
        "agent-exhausted".to_string(),
        AgentLocationAuthorityV1 {
            agent_id: "agent-exhausted".to_string(),
            location_id: "location-exhausted".to_string(),
            active: true,
            authority_revision: u64::MAX,
            effective_at: 0,
        },
    );
    let mut world = World::new_with_state(state);
    let before_state = serde_json::to_vec(world.state()).expect("serialize exhausted assignment");
    let before_journal_len = world.journal().len();

    let error = world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "agent-exhausted".to_string(),
            location_id: "location-exhausted".to_string(),
            active: false,
            authority_revision: u64::MAX,
            effective_at: 0,
        })
        .expect_err("exhausted agent location authority must fail closed");

    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("agent location authority revision exhausted at")
    ));
    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize assignment after rejection"),
        before_state
    );
    assert_eq!(world.journal().len(), before_journal_len);
}

#[test]
fn factory_site_revision_exhaustion_rejects_without_mutation() {
    let mut state = WorldState::default();
    state.location_anchors.insert(
        "location-exhausted".to_string(),
        LocationAnchorV1 {
            location_id: "location-exhausted".to_string(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    state.factory_site_authorities.insert(
        "site-exhausted".to_string(),
        FactorySiteAuthorityV1 {
            site_id: "site-exhausted".to_string(),
            location_id: "location-exhausted".to_string(),
            owner_agent_id: "owner-exhausted".to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: u64::MAX,
            registered_at: 0,
        },
    );
    let mut world = World::new_with_state(state);
    let before_state = serde_json::to_vec(world.state()).expect("serialize exhausted site");
    let before_journal_len = world.journal().len();

    let error = world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: "site-exhausted".to_string(),
            location_id: "location-exhausted".to_string(),
            owner_agent_id: "owner-exhausted".to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: false,
            active: true,
            authority_revision: u64::MAX,
            registered_at: 0,
        })
        .expect_err("exhausted factory site authority must fail closed");

    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("factory site authority revision exhausted at")
    ));
    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize site after rejection"),
        before_state
    );
    assert_eq!(world.journal().len(), before_journal_len);
}

#[test]
fn construction_power_profile_revision_exhaustion_rejects_without_mutation() {
    let mut state = WorldState::default();
    state.factory_construction_power_profiles.insert(
        "factory-exhausted".to_string(),
        FactoryConstructionPowerProfileV1 {
            factory_id: "factory-exhausted".to_string(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: u64::MAX,
            active: true,
        },
    );
    let mut world = World::new_with_state(state);
    let before_state = serde_json::to_vec(world.state()).expect("serialize exhausted profile");
    let before_journal_len = world.journal().len();

    let error = world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: "factory-exhausted".to_string(),
            factory_kind: "test".to_string(),
            source_module_id: Some("module-next".to_string()),
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: u64::MAX,
            active: true,
        })
        .expect_err("exhausted construction profile must fail closed");

    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("construction power profile authority revision exhausted at")
    ));
    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize profile after rejection"),
        before_state
    );
    assert_eq!(world.journal().len(), before_journal_len);
}
