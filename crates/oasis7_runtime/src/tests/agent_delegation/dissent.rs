use super::*;

#[test]
fn authorized_dissent_is_evidence_and_commits_once_without_owner_override() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world.install_agent_delegation_grant(allowance).unwrap();
    let intent = accepted(&mut world);
    let prepared = prepare(
        &mut world,
        "dissent-authorized",
        recipe(),
        8,
        AgentDecisionCausalContextV1 {
            intent_id: Some(intent.intent_id.clone()),
            dissent: Some("prefer to preserve electricity".into()),
            ..Default::default()
        },
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts.len(), 1);
    let receipt = &receipts[0];
    assert_eq!(
        receipt.intent_id.as_deref(),
        Some(intent.intent_id.as_str())
    );
    assert_eq!(receipt.disposition, "applied");
    assert_eq!(
        receipt.dissent.as_deref(),
        Some("prefer to preserve electricity")
    );
    assert!(receipt.override_actor.is_none());
    assert!(receipt.owner_control_refs.is_empty());
    assert!(!receipt.domain_event_refs.is_empty());
    assert_eq!(receipt.authorization.as_ref().unwrap().spent_units, 8);
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].remaining_units,
        4
    );
    let root = world.current_state_root_hash().unwrap();
    let journal_len = world.journal().events.len();
    let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    restored
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(restored.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
    assert_eq!(restored.current_state_root_hash().unwrap(), root);
    assert_eq!(restored.journal().events.len(), journal_len);
    assert_eq!(
        restored.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        8
    );
}

#[test]
fn authorized_dissent_still_obeys_hard_boundary_without_owner_override() {
    let mut world = fixture();
    let mut snapshot = world.snapshot();
    snapshot.state.gameplay_policy.forbidden_location_ids = vec!["10:0:0".into()];
    world = World::new_with_state(snapshot.state);
    world.install_agent_delegation_grant(grant()).unwrap();
    let intent = accepted(&mut world);
    let position = world.state().agents[AGENT_ID].state.pos;
    let prepared = prepare(
        &mut world,
        "dissent-hard-boundary",
        movement(10),
        0,
        AgentDecisionCausalContextV1 {
            intent_id: Some(intent.intent_id.clone()),
            dissent: Some("location is unsafe".into()),
            ..Default::default()
        },
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].disposition, "not_applied");
    assert!(receipts[0].hard_boundary.is_some());
    assert_eq!(receipts[0].dissent.as_deref(), Some("location is unsafe"));
    assert!(receipts[0].override_actor.is_none());
    assert_eq!(world.state().agents[AGENT_ID].state.pos, position);
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        0
    );
}

#[test]
fn dissent_does_not_preserve_execution_authority_after_revocation() {
    let mut world = recipe_fixture();
    let mut allowance = grant();
    allowance.object_id = "factory".into();
    allowance.action_kinds = vec!["ScheduleRecipe".into()];
    allowance.limit_units = 12;
    world
        .install_agent_delegation_grant(allowance.clone())
        .unwrap();
    let intent = accepted(&mut world);
    let prepared = prepare(
        &mut world,
        "dissent-revoked",
        recipe(),
        8,
        AgentDecisionCausalContextV1 {
            intent_id: Some(intent.intent_id.clone()),
            dissent: Some("prefer to wait".into()),
            ..Default::default()
        },
    );
    allowance.revoked = true;
    allowance.revision = 2;
    world.install_agent_delegation_grant(allowance).unwrap();
    let before = world.state().clone();
    let journal_len = world.journal().events.len();
    let error = world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap_err();
    assert!(format!("{error:?}").contains("grant_revoked"));
    assert_eq!(world.state().resources, before.resources);
    assert_eq!(world.state().material_ledgers, before.material_ledgers);
    assert_eq!(
        world.state().agents[AGENT_ID].state,
        before.agents[AGENT_ID].state
    );
    assert_eq!(
        world.state().agent_intent_ledger[&intent.intent_id].status,
        "rejected"
    );
    assert!(
        world
            .journal()
            .events
            .iter()
            .skip(journal_len)
            .all(|event| {
                matches!(
                    event.body,
                    WorldEventBody::Domain(DomainEvent::AgentIntentTransitioned { .. })
                )
            })
    );
    assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        0
    );
}
