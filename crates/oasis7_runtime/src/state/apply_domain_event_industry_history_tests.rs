use super::*;

fn failure_disposition(job_id: ActionId) -> FactoryProductionFailureDispositionV1 {
    FactoryProductionFailureDispositionV1 {
        action_id: job_id,
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.test".to_string(),
        recipe_id: "recipe.test".to_string(),
        blocker_kind: "product_validation".to_string(),
        blocker_detail: "test rejection".to_string(),
        disposition_kind: "consumed_lost".to_string(),
        consumed_inputs: Vec::new(),
        lost_inputs: Vec::new(),
        consumed_power: 0,
        lost_power: 0,
        next_action: "retry".to_string(),
        next_recheck: None,
    }
}

fn seed_settled_history(state: &mut WorldState, job_id: ActionId, order: u64) {
    state.settled_recipe_job_ids.insert(job_id);
    state.industry_settlement_orders.insert(job_id, order);
    state.product_validation_attempts.insert(
        job_id,
        vec![ProductValidationAttemptV1 {
            job_id,
            validation_index: Some(0),
            requester_agent_id: "builder-a".to_string(),
            module_id: "m4.product.test".to_string(),
            stack: MaterialStack::new("test_product", 1),
        }],
    );
    state.product_validation_receipts.insert(
        job_id,
        vec![ProductValidationReceiptV1 {
            job_id,
            validation_index: Some(0),
            requester_agent_id: "builder-a".to_string(),
            module_id: "m4.product.test".to_string(),
            stack: MaterialStack::new("test_product", 1),
            decision: ProductValidationDecision::accepted("test_product", 1, true, Vec::new()),
            failure_detail: None,
        }],
    );
    state.recipe_completion_receipts.insert(
        job_id,
        RecipeCompletionReceiptV1 {
            job_id,
            ..Default::default()
        },
    );
    state
        .factory_production_failure_dispositions
        .insert(job_id, failure_disposition(job_id));
}

#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn blocked_factory(action_id: ActionId) -> FactoryState {
    let mut production = FactoryProductionState::default();
    production.current_blocker_kind = Some("product_validation".to_string());
    production.current_blocker_detail = Some("test rejection".to_string());
    production.current_blocker_action_id = Some(action_id);
    FactoryState {
        factory_id: "factory.test".to_string(),
        site_id: "site.test".to_string(),
        builder_agent_id: "builder-a".to_string(),
        spec: FactoryModuleSpec {
            factory_id: "factory.test".to_string(),
            display_name: "Test Factory".to_string(),
            tier: 1,
            tags: Vec::new(),
            build_cost: Vec::new(),
            build_time_ticks: 1,
            base_power_draw: 0,
            recipe_slots: 1,
            throughput_bps: 1,
            maintenance_per_tick: 0,
        },
        input_ledger: MaterialLedgerId::world(),
        output_ledger: MaterialLedgerId::world(),
        durability_ppm: 1_000_000,
        production,
        site_authority_revision: None,
        site_location_id: None,
        location_anchor_revision: None,
        construction_power_profile_key: None,
        construction_power_profile_revision: None,
        built_at: 0,
    }
}

#[test]
fn legacy_factory_production_state_defaults_blocker_action_identity() {
    let production: FactoryProductionState =
        serde_json::from_str("{}").expect("legacy production state");
    assert_eq!(production.current_blocker_action_id, None);
}

#[test]
fn current_factory_blocker_protects_unresolved_failure_disposition() {
    let mut state = WorldState::default();
    state
        .factories
        .insert("factory.test".to_string(), blocked_factory(1));
    for job_id in 1..=65 {
        state.settled_recipe_job_ids.insert(job_id);
        state.industry_settlement_orders.insert(job_id, job_id);
        let mut disposition = failure_disposition(job_id);
        if job_id > 1 {
            disposition.factory_id = format!("factory.other-{job_id}");
        }
        state
            .factory_production_failure_dispositions
            .insert(job_id, disposition);
    }
    state.compact_settled_industry_history();
    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&1)
    );
    assert_eq!(state.factory_production_failure_dispositions.len(), 64);

    let production = &mut state.factories.get_mut("factory.test").unwrap().production;
    production.current_blocker_kind = None;
    production.current_blocker_detail = None;
    production.current_blocker_action_id = None;
    state.settled_recipe_job_ids.insert(66);
    state.industry_settlement_orders.insert(66, 66);
    let mut disposition = failure_disposition(66);
    disposition.factory_id = "factory.other-66".to_string();
    state
        .factory_production_failure_dispositions
        .insert(66, disposition);
    state.compact_settled_industry_history();
    assert!(
        !state
            .factory_production_failure_dispositions
            .contains_key(&1)
    );
    assert_eq!(state.factory_production_failure_dispositions.len(), 64);

    state
        .factories
        .insert("factory.test".to_string(), blocked_factory(1));
    state.factory_production_failure_dispositions.clear();
    state.settled_recipe_job_ids.clear();
    state.industry_settlement_orders.clear();
    for job_id in (u64::MAX - 63)..=u64::MAX {
        state.settled_recipe_job_ids.insert(job_id);
        state
            .industry_settlement_orders
            .insert(job_id, job_id - (u64::MAX - 64));
        state
            .factory_production_failure_dispositions
            .insert(job_id, failure_disposition(job_id));
    }
    state.settled_recipe_job_ids.insert(1);
    state.industry_settlement_orders.insert(1, 65);
    state
        .factory_production_failure_dispositions
        .insert(1, failure_disposition(1));
    state.compact_settled_industry_history();
    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&1)
    );
    assert_eq!(state.factory_production_failure_dispositions.len(), 64);
}

#[test]
fn settled_history_rollover_retains_newest_low_action_id() {
    let mut state = WorldState::default();
    for (order, job_id) in (1..=64).zip(100..=163) {
        seed_settled_history(&mut state, job_id, order);
    }
    seed_settled_history(&mut state, 1, 65);

    state.compact_settled_industry_history();

    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&1),
        "the post-rollover settlement must remain the newest history entry"
    );
    assert!(
        !state
            .factory_production_failure_dispositions
            .contains_key(&100),
        "the true oldest pre-rollover settlement must be evicted"
    );
    assert_eq!(state.product_validation_attempts.len(), 64);
    assert_eq!(state.product_validation_receipts.len(), 64);
    assert_eq!(state.recipe_completion_receipts.len(), 64);
    assert_eq!(state.factory_production_failure_dispositions.len(), 64);
    assert_eq!(state.industry_settlement_orders.len(), 64);
    assert_eq!(state.industry_settlement_orders.get(&1), Some(&65));
    assert!(!state.industry_settlement_orders.contains_key(&100));
}

#[test]
fn starter_milestone_survives_settled_history_compaction() {
    let mut state = WorldState::default();
    let milestone = StarterIndustrialMilestoneV1 {
        settlement_summary: Some(StarterIndustrialSettlementSummaryV1 {
            requester_agent_id: "starter-agent-0".into(),
            accepted_batches: 12,
            consume: vec![MaterialStack::new("iron_ore", 48)],
            power_required: 24,
            produce: vec![MaterialStack::new("iron_ingot", 36)],
        }),
        committed_recipe_origin: Some(crate::events::recipe_origin_tests::origin()),
        profile_id: STARTER_INDUSTRIAL_PROFILE_ID.to_string(),
        profile_revision: STARTER_INDUSTRIAL_PROFILE_REVISION,
        factory_id: STARTER_SMELTER_FACTORY_ID.to_string(),
        recipe_id: STARTER_SMELTER_RECIPE_ID.to_string(),
        output_ledger: MaterialLedgerId::site("starter-site"),
        settlement_job_id: 1,
        settled_at: 10,
    };
    state.industry_progress.starter_industrial_milestone = Some(milestone.clone());
    for job_id in 1..=65 {
        seed_settled_history(&mut state, job_id, job_id);
    }

    state.compact_settled_industry_history();

    assert_eq!(
        state.industry_progress.starter_industrial_milestone,
        Some(milestone),
        "bounded receipt history must not revoke durable progression"
    );
    assert!(
        !state.recipe_completion_receipts.contains_key(&1),
        "the receipt backing the milestone must remain independently compactable"
    );
}

fn pending_recipe_job(job_id: ActionId) -> RecipeJobState {
    RecipeJobState {
        committed_recipe_origin: None,
        job_id,
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.test".to_string(),
        recipe_id: "recipe.test".to_string(),
        accepted_batches: 1,
        consume: Vec::new(),
        produce: Vec::new(),
        byproducts: Vec::new(),
        power_required: 0,
        power_owner_agent_id: Some("builder-a".to_string()),
        duration_ticks: 1,
        consume_ledger: MaterialLedgerId::world(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
        ready_at: 0,
    }
}

#[test]
fn first_terminal_industry_events_allocate_order_once_and_replay_is_stable() {
    let mut state = WorldState::default();
    state.pending_recipe_jobs.insert(10, pending_recipe_job(10));
    state
        .apply_domain_event(
            &DomainEvent::RecipeCompleted {
                committed_recipe_origin: None,
                job_id: 10,
                requester_agent_id: "builder-a".to_string(),
                factory_id: "factory.test".to_string(),
                recipe_id: "recipe.test".to_string(),
                accepted_batches: 1,
                produce: Vec::new(),
                byproducts: Vec::new(),
                output_ledger: MaterialLedgerId::world(),
                bottleneck_tags: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            },
            0,
        )
        .expect("first completion");
    assert_eq!(state.industry_settlement_orders.get(&10), Some(&1));
    assert_eq!(state.next_industry_settlement_order, 2);
    state
        .apply_domain_event(
            &DomainEvent::RecipeCompleted {
                committed_recipe_origin: None,
                job_id: 10,
                requester_agent_id: "builder-a".to_string(),
                factory_id: "factory.test".to_string(),
                recipe_id: "recipe.test".to_string(),
                accepted_batches: 1,
                produce: Vec::new(),
                byproducts: Vec::new(),
                output_ledger: MaterialLedgerId::world(),
                bottleneck_tags: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            },
            0,
        )
        .expect("completion replay");
    assert_eq!(state.next_industry_settlement_order, 2);

    state.pending_recipe_jobs.insert(11, pending_recipe_job(11));
    let blocked = DomainEvent::FactoryProductionBlocked {
        action_id: 11,
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.test".to_string(),
        recipe_id: "recipe.test".to_string(),
        blocker_kind: "product_validation".to_string(),
        blocker_detail: "test rejection".to_string(),
    };
    state
        .apply_domain_event(&blocked, 0)
        .expect("first validation failure");
    assert_eq!(state.industry_settlement_orders.get(&11), Some(&2));
    assert_eq!(state.next_industry_settlement_order, 3);
    state
        .apply_domain_event(&blocked, 0)
        .expect("validation failure replay");
    assert_eq!(state.next_industry_settlement_order, 3);
}

#[test]
fn legacy_industry_history_without_settlement_order_is_decodable_and_retained() {
    let mut state = WorldState::default();
    state
        .factory_production_failure_dispositions
        .insert(7, failure_disposition(7));
    state.settled_recipe_job_ids.insert(7);
    let mut value = serde_json::to_value(&state).expect("serialize legacy fixture");
    value
        .as_object_mut()
        .expect("world state object")
        .remove("next_industry_settlement_order");
    value
        .as_object_mut()
        .expect("world state object")
        .remove("industry_settlement_orders");
    let mut restored: WorldState = serde_json::from_value(value).expect("decode legacy state");
    assert_eq!(restored.next_industry_settlement_order, 1);
    assert!(restored.industry_settlement_orders.is_empty());
    restored.compact_settled_industry_history();
    assert!(
        restored
            .factory_production_failure_dispositions
            .contains_key(&7)
    );
}

#[test]
fn committed_recipe_origin_completion_rejects_tampering_and_replays_without_credit() {
    let origin = crate::events::recipe_origin_tests::origin();
    let mut state = WorldState::default();
    let mut pending = pending_recipe_job(5);
    pending.committed_recipe_origin = Some(origin.clone());
    pending.produce = vec![MaterialStack::new("iron_ingot", 2)];
    state.pending_recipe_jobs.insert(5, pending);
    let event = DomainEvent::RecipeCompleted {
        committed_recipe_origin: Some(origin.clone()),
        job_id: 5,
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
        accepted_batches: 1,
        produce: vec![MaterialStack::new("iron_ingot", 2)],
        byproducts: vec![],
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: vec![],
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let mut tampered = event.clone();
    if let DomainEvent::RecipeCompleted {
        committed_recipe_origin: Some(origin),
        ..
    } = &mut tampered
    {
        origin.action_payload_hash = "d".repeat(64);
    }
    let before = serde_json::to_vec(&state).unwrap();
    assert!(state.apply_domain_event(&tampered, 0).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    state.apply_domain_event(&event, 0).expect("settle origin");
    assert_eq!(
        state.recipe_completion_receipts[&5].committed_recipe_origin,
        Some(origin)
    );
    let settled = serde_json::to_vec(&state).unwrap();
    state
        .apply_domain_event(&event, 0)
        .expect("same origin replay");
    assert_eq!(serde_json::to_vec(&state).unwrap(), settled);
    assert!(state.apply_domain_event(&tampered, 0).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), settled);
}
