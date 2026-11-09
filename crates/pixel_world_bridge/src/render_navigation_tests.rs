use super::*;

#[test]
fn module_visual_selection_focuses_the_published_marker() {
    let mut render_state = sample_render_state(12_000.0);
    render_state.module_visual_entities = vec![ModuleVisualEntity {
        factory_kind: None,
        id: "module-relay".to_string(),
        module_id: "module-7".to_string(),
        kind: "relay".to_string(),
        label: Some("Relay Seven".to_string()),
        pos: Position {
            x_cm: 2_700_000.0,
            y_cm: 1_600_000.0,
            z_cm: 80.0,
        },
    }];
    let focus_target = FocusTarget {
        kind: "module_visual".to_string(),
        id: "module-relay".to_string(),
    };

    assert_eq!(
        selection_focus_position(&render_state, &focus_target)
            .expect("module selection must resolve to its published marker position")
            .x_cm,
        2_700_000.0
    );
}
