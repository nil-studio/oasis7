use super::*;

// Pointer capture can outlive an automatic camera update. Preserve the entity
// pressed, then revalidate its presence against current authorized render input.
pub(super) fn click_selection(
    runtime: &mut BevyRuntimeState,
    x: f64,
    y: f64,
) -> Option<(String, String)> {
    let hit = match runtime.pointer_click_target.take() {
        Some(hit) => hit,
        None => hit_test(&runtime.hit_regions, x, y),
    };
    click_selection_from_hit(hit).filter(|(kind, id)| {
        runtime
            .hit_regions
            .iter()
            .any(|region| region.kind == kind && region.id == *id)
            && runtime
                .render_state
                .as_ref()
                .is_some_and(|state| match kind.as_str() {
                    "agent" => state.agents.iter().any(|entity| entity.id == *id),
                    "location" => state.locations.iter().any(|entity| entity.id == *id),
                    "module_visual" => state
                        .module_visual_entities
                        .iter()
                        .any(|entity| entity.id == *id),
                    "industry_aggregate" => state
                        .industry_aggregates
                        .iter()
                        .any(|entity| entity.id == *id),
                    "world_event" => state.world_events.iter().any(|entity| entity.id == *id),
                    _ => false,
                })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn region(id: &str, x: f64) -> HitRegion {
        HitRegion {
            kind: "agent",
            id: id.into(),
            left: x - 8.0,
            right: x + 8.0,
            top: 0.0,
            bottom: 16.0,
        }
    }
    fn sample_runtime() -> BevyRuntimeState {
        let mut runtime = BevyRuntimeState::default();
        runtime.render_state = Some(
            serde_json::from_value(json!({
                "locations":[],"links":[],"visual_hotspots":[],
                "agents":[{ "id":"agent-0","label":"A","resource_summary":"-","status_badges":[]},
                           { "id":"agent-1","label":"B","resource_summary":"-","status_badges":[]}]
            }))
            .unwrap(),
        );
        runtime.hit_regions = vec![region("agent-0", 20.0), region("agent-1", 40.0)];
        process_input_event(
            &mut runtime,
            InputEvent::PointerDown {
                x: 20.0,
                y: 8.0,
                pointer_id: 1,
            },
        );
        runtime
    }
    #[test]
    fn camera_motion_between_down_and_click_preserves_pressed_identity() {
        let mut runtime = sample_runtime();
        runtime.hit_regions = vec![region("agent-0", 40.0), region("agent-1", 20.0)];
        process_input_event(&mut runtime, InputEvent::PointerUp { pointer_id: 1 });
        assert_eq!(
            click_selection(&mut runtime, 20.0, 8.0),
            Some(("agent".into(), "agent-0".into()))
        );
    }
    #[test]
    fn removed_or_revoked_pressed_entity_cannot_select_a_replacement() {
        let mut runtime = sample_runtime();
        runtime
            .render_state
            .as_mut()
            .unwrap()
            .agents
            .retain(|agent| agent.id != "agent-0");
        assert_eq!(click_selection(&mut runtime, 20.0, 8.0), None);
        let mut runtime = sample_runtime();
        runtime.hit_regions = vec![region("agent-1", 20.0)];
        assert_eq!(click_selection(&mut runtime, 20.0, 8.0), None);
    }
    #[test]
    fn blank_press_and_drag_do_not_acquire_a_moving_target() {
        let mut blank = sample_runtime();
        process_input_event(
            &mut blank,
            InputEvent::PointerDown {
                x: 60.0,
                y: 8.0,
                pointer_id: 1,
            },
        );
        blank.hit_regions = vec![region("agent-1", 60.0)];
        assert_eq!(click_selection(&mut blank, 60.0, 8.0), None);
        let mut drag = sample_runtime();
        process_input_event(
            &mut drag,
            InputEvent::PointerMove {
                x: 30.0,
                y: 8.0,
                pointer_id: 1,
                is_leave: false,
            },
        );
        assert_eq!(click_selection(&mut drag, 20.0, 8.0), None);
    }
    #[test]
    fn spatial_overlay_pointer_selection_is_preserved_and_removed_input_rejected() {
        for kind in ["industry_aggregate", "world_event"] {
            let mut runtime = sample_runtime();
            let visual: spatial_display::SpatialVisual = serde_json::from_value(json!({
                "id":"spatial","label":"Spatial","pos":{"x_cm":1,"y_cm":2,"z_cm":0}
            }))
            .unwrap();
            let state = runtime.render_state.as_mut().unwrap();
            if kind == "industry_aggregate" {
                state.industry_aggregates.push(visual);
            } else {
                state.world_events.push(visual);
            }
            let mut hit = region("spatial", 20.0);
            hit.kind = kind;
            runtime.hit_regions = vec![hit];
            process_input_event(
                &mut runtime,
                InputEvent::PointerDown {
                    x: 20.0,
                    y: 8.0,
                    pointer_id: 1,
                },
            );
            assert_eq!(
                click_selection(&mut runtime, 20.0, 8.0),
                Some((kind.into(), "spatial".into()))
            );
            process_input_event(
                &mut runtime,
                InputEvent::PointerDown {
                    x: 20.0,
                    y: 8.0,
                    pointer_id: 1,
                },
            );
            let state = runtime.render_state.as_mut().unwrap();
            state.industry_aggregates.clear();
            state.world_events.clear();
            assert_eq!(click_selection(&mut runtime, 20.0, 8.0), None);
            // No pointer capture: direct-click compatibility still checks current input.
            assert_eq!(click_selection(&mut runtime, 20.0, 8.0), None);
        }
    }
}
