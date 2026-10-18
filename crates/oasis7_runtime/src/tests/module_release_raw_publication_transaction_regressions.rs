use super::*;

fn event_from_step(
    base: &World,
    submit: impl FnOnce(&mut World),
    pick: impl Fn(&DomainEvent) -> bool,
) -> DomainEvent {
    let mut probe = base.clone();
    let start = probe.journal().events.len();
    submit(&mut probe);
    probe.step().expect("capture valid module-release action");
    probe.journal().events[start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event) if pick(event) => Some(event.clone()),
            _ => None,
        })
        .expect("captured module-release event")
}

fn request_base() -> (World, String) {
    let mut world = World::new();
    register_agent(&mut world, "publisher-39");
    register_agent(&mut world, "operator-39");
    let bytes = b"module-release-raw-publication".to_vec();
    let hash = util::sha256_hex(&bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-39".into(),
        wasm_hash: hash.clone(),
        wasm_bytes: bytes,
    });
    world.step().expect("deploy release artifact");
    (world, hash)
}

fn requested_base() -> (World, u64) {
    let (mut world, hash) = request_base();
    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-39".into(),
        manifest: base_manifest("module.raw.release", "0.1.0", &hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".into()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit release request");
    let request_id = world
        .state()
        .module_release_requests
        .keys()
        .next_back()
        .copied()
        .expect("request");
    (world, request_id)
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let (request_world, hash) = request_base();
    let requested = event_from_step(
        &request_world,
        |probe| {
            probe.submit_action(Action::ModuleReleaseSubmit {
                requester_agent_id: "publisher-39".into(),
                manifest: base_manifest("module.raw.requested", "0.1.0", &hash),
                activate: true,
                install_target: ModuleInstallTarget::SelfAgent,
                required_roles: vec![" security ".into(), "security".into()],
                profile_changes: ModuleProfileChanges::default(),
            });
        },
        |event| matches!(event, DomainEvent::ModuleReleaseRequested { .. }),
    );
    let (roles_world, request_id) = requested_base();
    let roles = event_from_step(
        &roles_world,
        |probe| {
            probe.submit_action(Action::ModuleReleaseBindRoles {
                operator_agent_id: "operator-39".into(),
                target_agent_id: "operator-39".into(),
                roles: vec![" security ".into(), "security".into()],
            });
        },
        |event| matches!(event, DomainEvent::ModuleReleaseRolesBound { .. }),
    );
    let mut attested_world = roles_world.clone();
    set_module_release_attestation_epoch_snapshot(
        &mut attested_world,
        1,
        &[LOCAL_FINALITY_SIGNER_1],
    );
    let attested = event_from_step(
        &attested_world,
        |probe| {
            submit_test_module_release_attestation(
                probe,
                "operator-39",
                request_id,
                LOCAL_FINALITY_SIGNER_1,
                "linux-x86_64",
                "bafyrawrelease39",
            )
        },
        |event| matches!(event, DomainEvent::ModuleReleaseAttested { .. }),
    );
    vec![
        (request_world, requested),
        (roles_world, roles),
        (attested_world, attested),
    ]
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(
        world.state().module_release_requests,
        before.state().module_release_requests
    );
    assert_eq!(
        world.state().module_release_role_bindings,
        before.state().module_release_role_bindings
    );
    assert_eq!(
        world.state().material_ledgers,
        before.state().material_ledgers
    );
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.state().time, before.state().time);
    assert_eq!(world.current_state_root_hash().expect("root"), root);
}

fn assert_raw_failure(index: usize) {
    let (mut world, event) = fixtures().swap_remove(index);
    let before = world.clone();
    let baseline = world.snapshot();
    let root = world.current_state_root_hash().expect("root before");
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(390)),
        )
        .expect_err("raw release event honors failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);
    let retry_cause = Some(CausedBy::Action(392));
    world
        .append_event_for_test(WorldEventBody::Domain(event), retry_cause.clone())
        .expect("same-world raw release retry");
    assert_eq!(
        world
            .journal()
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        retry_cause
    );
    let replay = World::from_snapshot(baseline, world.journal().clone())
        .expect("replay same-world release retry");
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}

#[test]
fn raw_release_requested_fails_atomically_after_prepare() {
    assert_raw_failure(0);
}
#[test]
fn raw_release_roles_bound_fails_atomically_after_prepare() {
    assert_raw_failure(1);
}
#[test]
fn raw_release_attested_fails_atomically_after_prepare() {
    assert_raw_failure(2);
}

#[test]
fn raw_module_release_event_retries_and_replays() {
    for (mut world, event) in fixtures() {
        let baseline = world.snapshot();
        let cause = Some(CausedBy::Action(391));
        world
            .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
            .expect("raw release success");
        assert_eq!(
            world
                .journal()
                .events
                .last()
                .and_then(|event| event.caused_by.clone()),
            cause
        );
        let replay =
            World::from_snapshot(baseline, world.journal().clone()).expect("replay release event");
        assert_eq!(
            replay.state().module_release_requests,
            world.state().module_release_requests
        );
        assert_eq!(
            replay.state().module_release_role_bindings,
            world.state().module_release_role_bindings
        );
        assert_eq!(
            replay.current_state_root_hash().expect("replay root"),
            world.current_state_root_hash().expect("live root")
        );
    }
}

#[test]
fn raw_attestation_missing_mapping_rejects_without_late_partial_write() {
    let (mut world, event) = fixtures().swap_remove(2);
    let DomainEvent::ModuleReleaseAttested { request_id, .. } = &event else {
        unreachable!()
    };
    let request_id = *request_id;
    world.remove_module_release_mapping_for_test(request_id);
    let before = world.clone();
    let root = world.current_state_root_hash().expect("root before");
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("missing attestation mapping");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason == &format!("module release mapping missing for attestation request_id={request_id}"))
    );
    assert_unchanged(&world, &before, &root);
}

#[test]
fn raw_roles_bound_routes_operator_only_updates_target_and_empty_removes() {
    let mut world = World::new();
    register_agent(&mut world, "operator-route-39");
    register_agent(&mut world, "target-route-39");
    let operator_mailbox = world.state().agents["operator-route-39"].mailbox.len();
    let target_mailbox = world.state().agents["target-route-39"].mailbox.len();
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id: "operator-route-39".into(),
                target_agent_id: "target-route-39".into(),
                roles: vec![" Security ".into(), "security".into()],
            }),
            None,
        )
        .expect("bind raw roles");
    assert_eq!(
        world.state().agents["operator-route-39"].mailbox.len(),
        operator_mailbox + 1
    );
    assert_eq!(
        world.state().agents["target-route-39"].mailbox.len(),
        target_mailbox
    );
    assert_eq!(
        world.state().agents["target-route-39"].last_active,
        world.state().time
    );
    assert_eq!(
        world.state().module_release_role_bindings["target-route-39"].len(),
        1
    );
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::ModuleReleaseRolesBound {
                operator_agent_id: "operator-route-39".into(),
                target_agent_id: "target-route-39".into(),
                roles: vec!["  ".into()],
            }),
            None,
        )
        .expect("empty normalized roles remove binding");
    assert!(
        !world
            .state()
            .module_release_role_bindings
            .contains_key("target-route-39")
    );
    assert_eq!(
        world.state().agents["target-route-39"].last_active,
        world.state().time
    );
}
