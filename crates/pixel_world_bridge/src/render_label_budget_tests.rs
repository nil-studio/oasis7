use super::*;

fn budget_app() -> App {
    let state: RenderState = serde_json::from_value(json!({
        "locations": [], "agents": [], "links": [],
        "selection": {"kind": "agent", "id": "selected"},
        "visual_hotspots": [
            {"id": "goal", "kind": "goal", "label": "goal",
             "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}},
            {"id": "blocker", "kind": "blocker", "label": "blocker",
             "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}}
        ]
    }))
    .expect("valid display state");
    let mut app = App::new();
    app.insert_resource(BevyRuntimeState {
        render_state: Some(state),
        ..default()
    });
    app.add_systems(Update, enforce_label_budget);
    app
}

fn visible_texts(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let mut query = world.query::<(&Text2d, &Visibility)>();
    let mut texts = query
        .iter(world)
        .filter(|(_, visibility)| **visibility != Visibility::Hidden)
        .map(|(text, _)| text.0.clone())
        .collect::<Vec<_>>();
    texts.sort();
    texts
}

#[test]
fn global_budget_preserves_priorities_markers_and_restores_hidden_labels() {
    let mut app = budget_app();
    let mut ordinary = Vec::new();
    // Reverse insertion proves that stable identity, not ECS insertion order,
    // decides which ambient labels survive the combined cross-producer limit.
    for index in (0..110).rev() {
        let id = format!("agent-{index:03}");
        ordinary.push(
            app.world_mut()
                .spawn((
                    Text2d::new(id.clone()),
                    agent_labels::PixelWorldAgentLabel { id },
                ))
                .id(),
        );
        app.world_mut().spawn(Sprite::default());
    }
    app.world_mut().spawn((
        Text2d::new("selected"),
        agent_labels::PixelWorldAgentLabel {
            id: "selected".into(),
        },
    ));
    for id in ["goal", "blocker"] {
        app.world_mut().spawn((
            Text2d::new(id),
            map_labels::PixelWorldMapLabel {
                id: format!("map:{id}"),
            },
        ));
    }
    app.world_mut().spawn(Text2d::new("decoration"));
    let producer_hidden = app
        .world_mut()
        .spawn((Text2d::new("producer-hidden"), Visibility::Hidden))
        .id();
    app.update();
    let texts = visible_texts(&mut app);
    assert_eq!(texts.len(), 100);
    for priority in ["selected", "goal", "blocker"] {
        assert!(texts.iter().any(|text| text == priority));
    }
    assert!(texts.contains(&"agent-096".to_string()));
    assert!(!texts.contains(&"agent-097".to_string()));
    assert!(!texts.contains(&"decoration".to_string()));
    let world = app.world_mut();
    assert_eq!(world.query::<&Sprite>().iter(world).count(), 110);
    // Marker and text entities still exist: budget decisions cannot alter hit
    // identities or feed truncation back into authoritative world selection.
    assert_eq!(
        world
            .query::<&agent_labels::PixelWorldAgentLabel>()
            .iter(world)
            .count(),
        111
    );
    for entity in ordinary.iter().skip(20) {
        app.world_mut().despawn(*entity);
    }
    app.update();
    let restored = visible_texts(&mut app);
    assert_eq!(restored.len(), 24);
    assert!(restored.contains(&"agent-109".to_string()));
    assert!(restored.contains(&"decoration".to_string()));
    assert_eq!(
        app.world().get::<Visibility>(producer_hidden),
        Some(&Visibility::Hidden)
    );
    assert_eq!(
        app.world_mut()
            .query::<&BudgetHidden>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn unicode_scalar_limit_is_global_and_preserves_valid_text() {
    let mut app = budget_app();
    app.world_mut().spawn(Text2d::new("𐐀🌍界".repeat(80)));
    app.update();
    let texts = visible_texts(&mut app);
    assert_eq!(texts[0].chars().count(), 128);
    assert!(texts[0].ends_with('…'));
    app.update();
    assert_eq!(texts, visible_texts(&mut app));
}

#[test]
fn many_goals_cannot_starve_the_primary_blocker_or_new_selection() {
    let mut app = budget_app();
    for index in 0..120 {
        let id = format!("goal-{index:03}");
        app.world_mut()
            .resource_mut::<BevyRuntimeState>()
            .render_state
            .as_mut()
            .expect("state")
            .visual_hotspots
            .push(VisualHotspot {
                id: id.clone(),
                label: id.clone(),
                kind: "goal".into(),
                pos: Position {
                    x_cm: 0.0,
                    y_cm: 0.0,
                    z_cm: 0.0,
                },
                emphasis: None,
                size_hint_px: None,
            });
        app.world_mut().spawn((
            Text2d::new(id.clone()),
            map_labels::PixelWorldMapLabel {
                id: format!("map:{id}"),
            },
        ));
    }
    app.world_mut().spawn((
        Text2d::new("blocker"),
        map_labels::PixelWorldMapLabel {
            id: "map:blocker".into(),
        },
    ));
    let chosen = app
        .world_mut()
        .spawn((
            Text2d::new("new-selection"),
            agent_labels::PixelWorldAgentLabel {
                id: "new-selection".into(),
            },
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(chosen),
        Some(&Visibility::Hidden)
    );
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("state")
        .selection = Some(Selection {
        kind: "agent".into(),
        id: "new-selection".into(),
    });
    app.update();
    let texts = visible_texts(&mut app);
    assert_eq!(texts.len(), 100);
    assert!(texts.contains(&"blocker".to_string()));
    assert!(texts.contains(&"new-selection".to_string()));
}

#[test]
fn scene_commands_are_budgeted_before_the_same_frame_becomes_visible() {
    fn spawn_labels(mut commands: Commands) {
        for index in 0..140 {
            commands.spawn(Text2d::new(format!("label-{index:03}")));
        }
    }
    let mut app = App::new();
    app.init_resource::<BevyRuntimeState>();
    app.add_systems(Update, (spawn_labels, enforce_label_budget).chain());
    app.update();
    assert_eq!(visible_texts(&mut app).len(), MAX_VISIBLE_LABELS);
}

#[test]
fn depot_label_backing_follows_its_text_budget_without_hiding_the_marker() {
    use micro_depot_facilities::{
        PixelWorldMicroDepotThroughputBackingVisual, PixelWorldMicroDepotThroughputTextVisual,
    };
    let mut app = budget_app();
    let mut ambient = Vec::new();
    for index in 0..100 {
        ambient.push(
            app.world_mut()
                .spawn((
                    Text2d::new(format!("ambient-{index:03}")),
                    agent_labels::PixelWorldAgentLabel {
                        id: format!("agent-{index:03}"),
                    },
                ))
                .id(),
        );
    }
    let label = app
        .world_mut()
        .spawn((
            Text2d::new("Throughput: 12"),
            PixelWorldMicroDepotThroughputTextVisual { id: "depot".into() },
        ))
        .id();
    let backing = app
        .world_mut()
        .spawn((
            Sprite::default(),
            PixelWorldMicroDepotThroughputBackingVisual { id: "depot".into() },
        ))
        .id();
    let marker = app
        .world_mut()
        .spawn((
            Sprite::default(),
            PixelWorldMicroDepotVisual { id: "depot".into() },
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(label),
        Some(&Visibility::Hidden)
    );
    assert_eq!(
        app.world().get::<Visibility>(backing),
        Some(&Visibility::Hidden)
    );
    assert_ne!(
        app.world().get::<Visibility>(marker),
        Some(&Visibility::Hidden)
    );
    app.world_mut().despawn(ambient[0]);
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(label),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world().get::<Visibility>(backing),
        Some(&Visibility::Inherited)
    );
    assert_ne!(
        app.world().get::<Visibility>(marker),
        Some(&Visibility::Hidden)
    );
    assert_eq!(
        app.world_mut()
            .query::<&PixelWorldMicroDepotVisual>()
            .iter(app.world())
            .count(),
        1
    );
}
