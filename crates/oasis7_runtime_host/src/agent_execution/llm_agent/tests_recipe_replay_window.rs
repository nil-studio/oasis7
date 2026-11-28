use super::*;

#[test]
fn llm_agent_preserves_recent_replays_across_action_id_rollover() {
    const REPLAY_WINDOW: u64 = 64;
    let recipe_id = "recipe.assembler.control_chip";
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let high_start = u64::MAX - (REPLAY_WINDOW - 1);

    for offset in 0..REPLAY_WINDOW {
        behavior.on_event(&runtime_recipe_completed_event_with_identity(
            "agent-1", recipe_id, 20_000 + offset, high_start + offset,
        ));
    }
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_ids.len(),
        REPLAY_WINDOW as usize
    );

    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 30_000, 1,
    ));
    assert!(behavior.recipe_coverage.completion_receipt_ids.contains(&1));
    let memory_after_rollover = behavior.memory.short_term.len();
    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 30_001, 1,
    ));
    assert_eq!(behavior.memory.short_term.len(), memory_after_rollover);

    for low_job_id in 2..=REPLAY_WINDOW {
        behavior.on_event(&runtime_recipe_completed_event_with_identity(
            "agent-1", recipe_id, 30_001 + low_job_id, low_job_id,
        ));
    }
    assert!(!behavior
        .recipe_coverage
        .completion_receipt_ids
        .contains(&high_start));
    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 31_000, REPLAY_WINDOW + 1,
    ));
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_ids.len(),
        REPLAY_WINDOW as usize
    );
    assert!(!behavior.recipe_coverage.completion_receipt_ids.contains(&1));
    let aged_replay = runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 31_001, 1,
    );
    let memory_before_aged_replay = behavior.memory.short_term.clone();
    behavior.on_event(&aged_replay);
    assert_ne!(
        behavior.memory.short_term, memory_before_aged_replay,
        "an identity outside the bounded window may be observed once again"
    );
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_ids.len(),
        REPLAY_WINDOW as usize
    );
}

#[test]
fn llm_agent_completion_replay_window_tracks_sparse_insertion_order() {
    const REPLAY_WINDOW: usize = 64;
    let recipe_id = "recipe.assembler.control_chip";
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());

    for job_id in (100..=163).chain(std::iter::once(1)) {
        behavior.on_event(&runtime_recipe_completed_event_with_identity(
            "agent-1",
            recipe_id,
            40_000 + job_id,
            job_id,
        ));
    }

    assert_eq!(
        behavior.recipe_coverage.completion_receipt_order.len(),
        REPLAY_WINDOW
    );
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_order.front(),
        Some(&101)
    );
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_order.back(),
        Some(&1)
    );
    assert!(!behavior
        .recipe_coverage
        .completion_receipt_ids
        .contains(&100));
    assert!(behavior
        .recipe_coverage
        .completion_receipt_ids
        .contains(&1));

    let order_before_replay = behavior.recipe_coverage.completion_receipt_order.clone();
    let memory_before_replay = behavior.memory.short_term.clone();
    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 50_000, 130,
    ));
    assert_eq!(
        behavior.recipe_coverage.completion_receipt_order,
        order_before_replay,
        "replay must not reorder the durable insertion history"
    );
    assert_eq!(
        behavior.memory.short_term, memory_before_replay,
        "replay must not add a duplicate feedback memory"
    );

    let serialized = serde_json::to_value(&behavior.recipe_coverage).expect("serialize coverage");
    let restored: RecipeCoverageProgress =
        serde_json::from_value(serialized).expect("restore coverage");
    assert_eq!(restored, behavior.recipe_coverage);
}

#[test]
fn llm_agent_completion_replay_legacy_ids_restore_deterministic_bounded_order() {
    let serialized = serde_json::json!({
        "completed": [],
        "completion_receipt_ids": (1_u64..=70).collect::<Vec<_>>(),
    });
    let restored: RecipeCoverageProgress =
        serde_json::from_value(serialized).expect("restore legacy coverage");

    assert_eq!(restored.completion_receipt_ids.len(), 64);
    assert_eq!(restored.completion_receipt_order.len(), 64);
    assert_eq!(restored.completion_receipt_order.front(), Some(&7));
    assert_eq!(restored.completion_receipt_order.back(), Some(&70));
    assert!(!restored.completion_receipt_ids.contains(&6));
}
