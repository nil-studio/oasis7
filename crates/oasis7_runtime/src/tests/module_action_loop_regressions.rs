fn product_only_profile_changes(product_id: &str) -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: vec![ProductProfileV1 {
            product_id: product_id.to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 1)],
            tradable: true,
            unlock_stage: "scale_out".to_string(),
        }],
        recipe_profiles: Vec::new(),
        factory_profiles: Vec::new(),
    }
}

fn recipe_only_profile_changes(recipe_id: &str) -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: Vec::new(),
        recipe_profiles: vec![RecipeProfileV1 {
            recipe_id: recipe_id.to_string(),
            bottleneck_tags: vec!["control_chip".to_string()],
            stage_gate: "scale_out".to_string(),
            preferred_factory_tags: vec!["assembler".to_string()],
        }],
        factory_profiles: Vec::new(),
    }
}

fn factory_only_profile_changes(factory_id: &str) -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: Vec::new(),
        recipe_profiles: Vec::new(),
        factory_profiles: vec![FactoryProfileV1 {
            factory_id: factory_id.to_string(),
            tier: 2,
            recipe_slots: 4,
            tags: vec!["assembler".to_string()],
        }],
    }
}

fn submit_release_request(
    world: &mut World,
    requester_agent_id: &str,
    manifest: ModuleManifest,
    profile_changes: ModuleProfileChanges,
) -> u64 {
    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: requester_agent_id.to_string(),
        manifest,
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes,
    });
    world.step().expect("submit module release request");
    match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    }
}

fn shadow_approve_and_attest_request(
    world: &mut World,
    operator_agent_id: &str,
    approver_agent_id: &str,
    request_id: u64,
) {
    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: operator_agent_id.to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(world, operator_agent_id, approver_agent_id, &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: approver_agent_id.to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve module release role");
    set_module_release_attestation_epoch_snapshot(
        world,
        2,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );
    for (index, signer_node_id) in [LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2]
        .iter()
        .enumerate()
    {
        submit_test_module_release_attestation(
            world,
            operator_agent_id,
            request_id,
            signer_node_id,
            "linux-x86_64",
            format!("bafyshadowapply{request_id}{index:02}").as_str(),
        );
        world
            .step()
            .expect("submit module release attestation before apply");
    }
}

fn shadow_approve_and_apply_request(world: &mut World, operator_agent_id: &str, request_id: u64) {
    shadow_approve_and_attest_request(world, operator_agent_id, operator_agent_id, request_id);
    world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: operator_agent_id.to_string(),
        request_id,
    });
    world.step().expect("apply module release request");
}

fn approved_profile_release_fixture(suffix: &str) -> (World, u64, String) {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    register_agent(&mut world, "approver-1");
    register_agent(&mut world, "buyer-1");

    let wasm_bytes = format!("module-release-approved-profile-{suffix}").into_bytes();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(
            format!("m.loop.release.{suffix}").as_str(),
            "0.1.0",
            &wasm_hash,
        ),
        product_only_profile_changes(format!("product.{suffix}").as_str()),
    );
    shadow_approve_and_attest_request(&mut world, "operator-1", "approver-1", request_id);
    (world, request_id, wasm_hash)
}

#[test]
fn module_release_shadow_commitment_changes_with_profile_payload() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-profile-commitment".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let manifest = base_manifest("m.loop.release.profile.commitment", "0.1.0", &wasm_hash);
    let first_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        manifest.clone(),
        product_only_profile_changes("product.commitment.same"),
    );
    let mut changed_payload = product_only_profile_changes("product.commitment.same");
    changed_payload.product_profiles[0].tradable = false;
    let second_request_id =
        submit_release_request(&mut world, "publisher-1", manifest, changed_payload);

    let shadowed_hash = |world: &World, request_id| {
        world
            .journal()
            .events
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::ModuleReleaseShadowed {
                    request_id: shadowed_request_id,
                    manifest_hash,
                    ..
                }) if *shadowed_request_id == request_id => Some(manifest_hash.clone()),
                _ => None,
            })
            .expect("shadow commitment event")
    };

    for request_id in [first_request_id, second_request_id] {
        world.submit_action(Action::ModuleReleaseShadow {
            operator_agent_id: "operator-1".to_string(),
            request_id,
        });
        world.step().expect("shadow module release request");
    }

    assert_ne!(
        shadowed_hash(&world, first_request_id),
        shadowed_hash(&world, second_request_id),
        "the signed release commitment must bind the reviewed profile payload"
    );
}

#[test]
fn module_release_shadow_commitment_canonicalizes_profile_vector_order() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-profile-order-commitment".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let manifest = base_manifest("m.loop.release.profile.order", "0.1.0", &wasm_hash);
    let mut ordered_profiles = product_only_profile_changes("product.order.a");
    ordered_profiles.product_profiles.push(
        product_only_profile_changes("product.order.b")
            .product_profiles
            .remove(0),
    );
    let mut reverse_profiles = ordered_profiles.clone();
    reverse_profiles.product_profiles.reverse();
    let first_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        manifest.clone(),
        ordered_profiles,
    );
    let second_request_id =
        submit_release_request(&mut world, "publisher-1", manifest, reverse_profiles);
    for request_id in [first_request_id, second_request_id] {
        world.submit_action(Action::ModuleReleaseShadow {
            operator_agent_id: "operator-1".to_string(),
            request_id,
        });
        world.step().expect("shadow module release request");
    }
    let shadow_hash = |request_id| {
        world
            .journal()
            .events
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::ModuleReleaseShadowed {
                    request_id: shadowed_request_id,
                    manifest_hash,
                    ..
                }) if *shadowed_request_id == request_id => Some(manifest_hash.clone()),
                _ => None,
            })
            .expect("shadow commitment event")
    };
    assert_eq!(
        shadow_hash(first_request_id),
        shadow_hash(second_request_id),
        "profile records are committed in canonical ID order"
    );
}

#[test]
fn module_release_finality_hash_matches_reviewed_profile_shadow_commitment() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-profile-finality-commitment".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest("m.loop.release.profile.finality", "0.1.0", &wasm_hash),
        product_only_profile_changes("product.profile.finality"),
    );
    shadow_approve_and_apply_request(&mut world, "operator-1", request_id);

    let shadow_hash = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleReleaseShadowed {
                request_id: shadowed_request_id,
                manifest_hash,
                ..
            }) if *shadowed_request_id == request_id => Some(manifest_hash.clone()),
            _ => None,
        })
        .expect("shadow commitment event");
    let applied_proposal_id = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied {
                request_id: applied_request_id,
                proposal_id,
                ..
            }) if *applied_request_id == request_id => Some(*proposal_id),
            _ => None,
        })
        .expect("applied proposal id");
    let proposed_manifest = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Governance(GovernanceEvent::Proposed {
                proposal_id,
                manifest,
                ..
            }) if *proposal_id == applied_proposal_id => Some(manifest),
            _ => None,
        })
        .expect("reviewed governance proposal manifest");
    let proposed_manifest_hash =
        util::hash_json(proposed_manifest).expect("hash proposal manifest");
    assert_eq!(
        shadow_hash, proposed_manifest_hash,
        "the governance proposal covered by finality must match the reviewed Shadow commitment"
    );
    let committed_profiles = proposed_manifest
        .content
        .get("module_profile_commitment")
        .expect("proposal manifest profile commitment");
    let applied_manifest = world.snapshot().manifest;
    assert_eq!(
        applied_manifest.content.get("module_profile_commitment"),
        Some(committed_profiles),
        "the applied world manifest must retain the profile commitment"
    );
}

#[test]
fn module_release_apply_rejects_when_approver_loses_current_role_binding() {
    let (mut world, request_id, _) = approved_profile_release_fixture("role.recheck");

    bind_release_roles(&mut world, "operator-1", "approver-1", &[]);
    let apply_action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply module release request");

    assert_rule_denied_note_for_action(&world, apply_action_id, "current role binding");
    assert_eq!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .expect("release request remains available")
            .status,
        ModuleReleaseRequestStatus::Approved,
        "a rejected apply must preserve the approved request"
    );
    assert!(
        !world
            .state()
            .product_profiles
            .contains_key("product.role.recheck"),
        "rejected apply must not publish the reviewed profile"
    );
}

#[test]
fn module_release_apply_rejects_tampered_approved_profile_payload_atomically() {
    let (world, request_id, _) = approved_profile_release_fixture("profile-tamper");
    let mut tampered = world.clone();
    tampered
        .mutate_state_and_refresh_tick_consensus_for_test(|state| {
            state
                .module_release_requests
                .get_mut(&request_id)
                .expect("approved release request")
                .profile_changes
                .product_profiles[0]
                .tradable = false;
        })
        .expect("keep edited request state root coherent");

    let apply_action_id = tampered.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    tampered.step().expect("apply tampered release request");

    assert_rule_denied_note_for_action(
        &tampered,
        apply_action_id,
        "reviewed manifest/profile commitment changed",
    );
    assert_eq!(
        tampered
            .state()
            .module_release_requests
            .get(&request_id)
            .expect("release request remains available")
            .status,
        ModuleReleaseRequestStatus::Approved,
        "tampered apply must leave approval state unchanged"
    );
    assert!(
        !tampered
            .state()
            .product_profiles
            .contains_key("product.profile-tamper"),
        "tampered apply must not publish a profile"
    );
}

#[test]
fn module_release_apply_rejects_when_requester_loses_artifact_ownership() {
    let (world, request_id, wasm_hash) = approved_profile_release_fixture("owner-recheck");
    let mut owner_changed = world.clone();
    owner_changed
        .mutate_state_and_refresh_tick_consensus_for_test(|state| {
            state
                .module_artifact_owners
                .insert(wasm_hash, "buyer-1".to_string());
        })
        .expect("keep transferred-owner state root coherent");

    let apply_action_id = owner_changed.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    owner_changed
        .step()
        .expect("apply after artifact ownership transfer");

    assert_rule_denied_note_for_action(&owner_changed, apply_action_id, "no longer current owner");
    assert_eq!(
        owner_changed
            .state()
            .module_release_requests
            .get(&request_id)
            .expect("release request remains available")
            .status,
        ModuleReleaseRequestStatus::Approved,
        "owner drift rejection must leave approval state unchanged"
    );
    assert!(
        !owner_changed
            .state()
            .product_profiles
            .contains_key("product.owner-recheck"),
        "owner drift rejection must not publish a profile"
    );
}

#[test]
fn module_release_apply_rejects_legacy_profile_request_without_reviewed_commitment() {
    let (world, request_id, _) = approved_profile_release_fixture("legacy-profile");
    let mut legacy = world.clone();
    legacy
        .mutate_state_and_refresh_tick_consensus_for_test(|state| {
            state
                .module_release_requests
                .get_mut(&request_id)
                .expect("approved release request")
                .shadow_manifest_hash = None;
        })
        .expect("keep legacy request state root coherent");

    let apply_action_id = legacy.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    legacy.step().expect("apply legacy profile request");

    assert_rule_denied_note_for_action(&legacy, apply_action_id, "has no reviewed commitment");
    assert!(
        !legacy
            .state()
            .product_profiles
            .contains_key("product.legacy-profile"),
        "legacy pending profile request must not publish without a commitment"
    );
}

#[test]
fn module_release_freeze_blocks_new_submit_and_old_approved_apply() {
    let (world, request_id, wasm_hash) = approved_profile_release_fixture("frozen-release");
    let mut frozen = world.clone();
    let freeze = crate::state::ModuleAdmissionFreeze {
        module_id: "m.loop.release.frozen-release".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        rollback_proposal_id: 77,
        source_release_request_id: Some(request_id),
        reason: "rollback freeze test".to_string(),
    };
    frozen
        .mutate_state_and_refresh_tick_consensus_for_test(|state| {
            state.module_admission_freezes.insert(
                crate::state::ModuleAdmissionFreeze::key(
                    &freeze.module_id,
                    &freeze.module_version,
                    &freeze.wasm_hash,
                ),
                freeze,
            );
        })
        .expect("keep frozen release state root coherent");

    let submit_action_id = frozen.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.frozen-release", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: product_only_profile_changes("product.frozen-new"),
    });
    frozen.step().expect("submit release for frozen artifact");
    assert_rule_denied_note_for_action(&frozen, submit_action_id, "rollback freeze");

    let apply_action_id = frozen.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    frozen
        .step()
        .expect("apply previously approved frozen release");
    assert_rule_denied_note_for_action(&frozen, apply_action_id, "rollback freeze");
    assert_eq!(
        frozen
            .state()
            .module_release_requests
            .get(&request_id)
            .expect("old request remains available")
            .status,
        ModuleReleaseRequestStatus::Approved,
        "old approval must not clear a rollback freeze"
    );
    assert!(
        !frozen
            .state()
            .product_profiles
            .contains_key("product.frozen-release")
    );
    assert!(
        !frozen
            .state()
            .product_profiles
            .contains_key("product.frozen-new")
    );
}

#[test]
fn module_release_shadow_rejects_existing_product_profile_even_when_payload_matches() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-existing-product-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let module_id = "m.loop.release.product.existing";
    let first_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        product_only_profile_changes("product.non_overwrite"),
    );
    shadow_approve_and_apply_request(&mut world, "operator-1", first_request_id);

    let second_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        product_only_profile_changes("product.non_overwrite"),
    );
    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id: second_request_id,
    });
    world
        .step()
        .expect("shadow module release request with existing product profile");
    assert_rule_denied_note_for_action(
        &world,
        action_id,
        "product profile_id already exists in state product.non_overwrite",
    );
}

#[test]
fn module_release_shadow_rejects_existing_recipe_profile_even_when_payload_matches() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-existing-recipe-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let module_id = "m.loop.release.recipe.existing";
    let first_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        recipe_only_profile_changes("recipe.non_overwrite"),
    );
    shadow_approve_and_apply_request(&mut world, "operator-1", first_request_id);

    let second_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        recipe_only_profile_changes("recipe.non_overwrite"),
    );
    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id: second_request_id,
    });
    world
        .step()
        .expect("shadow module release request with existing recipe profile");
    assert_rule_denied_note_for_action(
        &world,
        action_id,
        "recipe profile_id already exists in state recipe.non_overwrite",
    );
}

#[test]
fn module_release_shadow_rejects_existing_factory_profile_even_when_payload_matches() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-existing-factory-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let module_id = "m.loop.release.factory.existing";
    let first_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        factory_only_profile_changes("factory.non_overwrite"),
    );
    shadow_approve_and_apply_request(&mut world, "operator-1", first_request_id);

    let second_request_id = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        factory_only_profile_changes("factory.non_overwrite"),
    );
    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id: second_request_id,
    });
    world
        .step()
        .expect("shadow module release request with existing factory profile");
    assert_rule_denied_note_for_action(
        &world,
        action_id,
        "factory profile_id already exists in state factory.non_overwrite",
    );
}

#[test]
fn module_release_apply_rechecks_and_rejects_existing_profile_overwrite() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-apply-recheck-overwrite".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let module_id = "m.loop.release.apply.recheck";
    let changes = product_only_profile_changes("product.apply.non_overwrite");
    let request_a = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        changes.clone(),
    );
    let request_b = submit_release_request(
        &mut world,
        "publisher-1",
        base_manifest(module_id, "0.1.0", &wasm_hash),
        changes,
    );

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id: request_a,
    });
    world.step().expect("shadow request a");
    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id: request_b,
    });
    world.step().expect("shadow request b");

    bind_release_roles(&mut world, "operator-1", "operator-1", &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id: request_a,
        role: "security".to_string(),
    });
    world.step().expect("approve request a");
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id: request_b,
        role: "security".to_string(),
    });
    world.step().expect("approve request b");
    set_module_release_attestation_epoch_snapshot(
        &mut world,
        2,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );
    for request_id in [request_a, request_b] {
        for (index, signer_node_id) in [LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2]
            .iter()
            .enumerate()
        {
            submit_test_module_release_attestation(
                &mut world,
                "operator-1",
                request_id,
                signer_node_id,
                "linux-x86_64",
                format!("bafyapplyrecheck{request_id}{index:02}").as_str(),
            );
            world
                .step()
                .expect("submit module release attestation for apply recheck");
        }
    }

    world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id: request_a,
    });
    world.step().expect("apply request a");

    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id: request_b,
    });
    world.step().expect("apply request b should reject");
    assert_rule_denied_note_for_action(
        &world,
        action_id,
        "product profile_id already exists in state product.apply.non_overwrite",
    );
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_b)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Approved)
    ));
}
