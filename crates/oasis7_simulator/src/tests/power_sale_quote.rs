use super::*;

#[test]
fn sell_power_quote_previews_remaining_runway_and_interrupt_risk() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "hub".to_string(),
        name: "hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        ResourceKind::Electricity,
        15,
    );

    let journal_len_before_quote = kernel.journal().len();
    let quote = kernel
        .quote_power_sale(
            &ResourceOwner::Agent {
                agent_id: "seller".to_string(),
            },
            &ResourceOwner::Agent {
                agent_id: "buyer".to_string(),
            },
            10,
            3,
        )
        .expect("sell power quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(quote.sale_amount, 10);
    assert_eq!(quote.expected_revenue, 30);
    assert_eq!(quote.current_power_level, 15);
    assert_eq!(quote.power_state_after_sale, "critical");
    assert_eq!(quote.remaining_runway_ticks, 5);
    assert_eq!(quote.next_action_affordability_after_sale, "limited");
    assert!(quote.production_interrupt_risk);
    assert_eq!(quote.recommended_sale_action, "defer_sale");
    assert!(
        quote
            .why_sale_is_safe_or_risky
            .contains("critical power runway")
    );
}

#[test]
fn buy_power_quote_previews_survival_recovery_without_mutating_journal() {
    let mut config = WorldConfig::default();
    config.power.market_base_price_per_pu = 2;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "hub".to_string(),
        name: "hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        ResourceKind::Electricity,
        50,
    );
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        ResourceKind::Electricity,
        4,
    );

    let journal_len_before_quote = kernel.journal().len();
    let quote = kernel
        .quote_power_survival(
            &ResourceOwner::Agent {
                agent_id: "buyer".to_string(),
            },
            &ResourceOwner::Agent {
                agent_id: "seller".to_string(),
            },
            20,
            0,
        )
        .expect("buy power survival quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(quote.recovery_action, "buy_power");
    assert_eq!(quote.recovery_amount, 20);
    assert_eq!(quote.power_gain_estimate, 20);
    assert_eq!(quote.price_per_pu, 2);
    assert_eq!(quote.price_or_time_cost, 40);
    assert_eq!(quote.current_power_level, 4);
    assert_eq!(quote.power_state_before, "critical");
    assert_eq!(quote.power_state_after_recovery, "normal");
    assert_eq!(quote.survival_runway_ticks, 24);
    assert_eq!(quote.next_action_affordability_after_recovery, "healthy");
    assert_eq!(quote.recommended_power_action, "buy_power");
    assert!(
        quote
            .shutdown_avoidance_reason
            .contains("restores 24 runway ticks")
    );
}

#[test]
fn buy_power_quote_rejects_buyer_stock_overflow_like_execution() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "hub".to_string(),
        name: "hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        ResourceKind::Electricity,
        i64::MAX - 5,
    );

    let journal_len_before_quote = kernel.journal().len();
    let reason = kernel
        .quote_power_survival(
            &ResourceOwner::Agent {
                agent_id: "buyer".to_string(),
            },
            &ResourceOwner::Agent {
                agent_id: "seller".to_string(),
            },
            10,
            1,
        )
        .expect_err("overflow should reject");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(reason, RejectReason::InvalidAmount { amount: 10 });
}
