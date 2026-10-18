use super::*;

#[test]
fn runtime_gameplay_actions_enable_assembler_motor_at_runtime_power_boundary() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(52);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        52,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        53,
    );
    build_first_assembler_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        62,
    );
    seed_assembler_site_materials(
        &mut server,
        &[("gear", 4), ("copper_wire", 6), ("hardware_part", 1)],
    );

    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 14)
        .expect("seed exact motor runtime power");
    let gameplay = expect_player_gameplay(&mut server, "motor runtime power boundary");
    let action = smelter_schedule_action(
        &gameplay,
        crate::viewer::ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1,
    );
    assert_eq!(
        action.disabled_reason, None,
        "runtime-valid motor power must keep the published action enabled"
    );

    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 13)
        .expect("drain one unit below motor runtime power");
    let gameplay = expect_player_gameplay(&mut server, "motor below power boundary");
    let action = smelter_schedule_action(
        &gameplay,
        crate::viewer::ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1,
    );
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("one below runtime motor power must disable scheduling");
    assert!(disabled_reason.contains("need 14"));
    assert!(disabled_reason.contains("replenish electricity"));
}

#[test]
fn runtime_gameplay_actions_enable_assembler_drone_at_runtime_power_boundary() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        setup_runtime_industrial_gameplay_session(53);
    build_first_smelter_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        53,
    );
    settle_one_smelter_iron_ingot_job(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        54,
    );
    build_first_assembler_via_gameplay_action(
        &mut server,
        agent_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
        63,
    );
    seed_assembler_site_materials(
        &mut server,
        &[
            ("motor_mk1", 2),
            ("control_chip", 1),
            ("iron_ingot", 2),
            ("hardware_part", 2),
        ],
    );

    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 12)
        .expect("seed exact drone runtime power");
    let gameplay = expect_player_gameplay(&mut server, "drone runtime power boundary");
    let action = smelter_schedule_action(
        &gameplay,
        crate::viewer::ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE,
    );
    assert_eq!(
        action.disabled_reason, None,
        "runtime-valid drone power must keep the published action enabled"
    );

    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, 11)
        .expect("drain one unit below drone runtime power");
    let gameplay = expect_player_gameplay(&mut server, "drone below power boundary");
    let action = smelter_schedule_action(
        &gameplay,
        crate::viewer::ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE,
    );
    let disabled_reason = action
        .disabled_reason
        .as_deref()
        .expect("one below runtime drone power must disable scheduling");
    assert!(disabled_reason.contains("need 12"));
    assert!(disabled_reason.contains("replenish electricity"));
}
