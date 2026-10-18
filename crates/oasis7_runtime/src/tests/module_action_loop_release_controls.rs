use crate::state::ModuleReleaseRequestStatus;

#[path = "module_action_loop_release_controls_tests.rs"]
mod release_controls_tests;

#[path = "module_release_raw_publication_transaction_regressions.rs"]
mod raw_publication_transaction_regressions;

const LOCAL_FINALITY_SIGNER_1: &str = "governance.local.finality.signer.1";
const LOCAL_FINALITY_SIGNER_2: &str = "governance.local.finality.signer.2";
const TEST_FINALITY_SIGNER_3: &str = "governance.test.finality.signer.3";
const TEST_RELEASE_BUILDER_IMAGE_DIGEST: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const TEST_RELEASE_CONTAINER_PLATFORM: &str = "linux-x86_64";
const TEST_RELEASE_CANONICALIZER_VERSION: &str = "strip-custom-sections-v1";

fn bind_release_roles(
    world: &mut World,
    operator_agent_id: &str,
    target_agent_id: &str,
    roles: &[&str],
) {
    world.submit_action(Action::ModuleReleaseBindRoles {
        operator_agent_id: operator_agent_id.to_string(),
        target_agent_id: target_agent_id.to_string(),
        roles: roles.iter().map(|role| role.to_string()).collect(),
    });
    world.step().expect("bind module release roles");
}

fn assert_rule_denied_note_for_action(world: &World, action_id: ActionId, expected: &str) {
    let notes = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id: rejected_action_id,
                reason: RejectReason::RuleDenied { notes },
            }) if *rejected_action_id == action_id => Some(notes.clone()),
            _ => None,
        })
        .expect("action rejected rule denied event");
    assert!(
        notes.iter().any(|note| note.contains(expected)),
        "missing expected note `{expected}` in {notes:?}"
    );
}

fn request_manifest_identity(world: &World, request_id: u64) -> ModuleArtifactIdentity {
    world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state")
        .manifest
        .artifact_identity
        .clone()
        .expect("module release request artifact identity")
}

fn submit_test_module_release_attestation(
    world: &mut World,
    operator_agent_id: &str,
    request_id: u64,
    signer_node_id: &str,
    platform: &str,
    proof_cid: &str,
) {
    let request = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state")
        .clone();
    let identity = request
        .manifest
        .artifact_identity
        .clone()
        .expect("module release request artifact identity");
    world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: operator_agent_id.to_string(),
        request_id,
        signer_node_id: signer_node_id.to_string(),
        platform: platform.to_string(),
        build_manifest_hash: identity.build_manifest_hash,
        source_hash: identity.source_hash,
        wasm_hash: request.manifest.wasm_hash,
        proof_cid: proof_cid.to_string(),
        builder_image_digest: TEST_RELEASE_BUILDER_IMAGE_DIGEST.to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
}

fn sample_profile_changes() -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: vec![ProductProfileV1 {
            product_id: "module_rack".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 1)],
            tradable: true,
            unlock_stage: "scale_out".to_string(),
        }],
        recipe_profiles: vec![RecipeProfileV1 {
            recipe_id: "recipe.assembler.module_rack".to_string(),
            bottleneck_tags: vec!["control_chip".to_string()],
            stage_gate: "scale_out".to_string(),
            preferred_factory_tags: vec!["assembler".to_string()],
        }],
        factory_profiles: vec![FactoryProfileV1 {
            factory_id: "factory.assembler.mk1".to_string(),
            tier: 2,
            recipe_slots: 4,
            tags: vec!["assembler".to_string()],
        }],
    }
}

fn duplicate_profile_changes() -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: vec![
            ProductProfileV1 {
                product_id: "dup_product".to_string(),
                role_tag: "scale".to_string(),
                maintenance_sink: Vec::new(),
                tradable: true,
                unlock_stage: "scale_out".to_string(),
            },
            ProductProfileV1 {
                product_id: "dup_product".to_string(),
                role_tag: "energy".to_string(),
                maintenance_sink: Vec::new(),
                tradable: true,
                unlock_stage: "scale_out".to_string(),
            },
        ],
        recipe_profiles: Vec::new(),
        factory_profiles: Vec::new(),
    }
}

fn duplicate_factory_profile_changes() -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: Vec::new(),
        recipe_profiles: Vec::new(),
        factory_profiles: vec![
            FactoryProfileV1 {
                factory_id: "dup_factory".to_string(),
                tier: 1,
                recipe_slots: 2,
                tags: vec!["assembly".to_string()],
            },
            FactoryProfileV1 {
                factory_id: "dup_factory".to_string(),
                tier: 2,
                recipe_slots: 3,
                tags: vec!["assembly".to_string()],
            },
        ],
    }
}

fn duplicate_recipe_profile_changes() -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: Vec::new(),
        recipe_profiles: vec![
            RecipeProfileV1 {
                recipe_id: "dup_recipe".to_string(),
                bottleneck_tags: vec!["control_chip".to_string()],
                stage_gate: "scale_out".to_string(),
                preferred_factory_tags: vec!["assembler".to_string()],
            },
            RecipeProfileV1 {
                recipe_id: "dup_recipe".to_string(),
                bottleneck_tags: vec!["maintenance".to_string()],
                stage_gate: "scale_out".to_string(),
                preferred_factory_tags: vec!["assembler".to_string()],
            },
        ],
        factory_profiles: Vec::new(),
    }
}

fn bind_attestor_node_identity(world: &mut World, node_id: &str) {
    let public_key_hex = util::sha256_hex(node_id.as_bytes());
    world
        .bind_node_identity(node_id, public_key_hex.as_str())
        .expect("bind attestor node identity");
}

fn set_module_release_attestation_epoch_snapshot(
    world: &mut World,
    threshold: u16,
    signer_node_ids: &[&str],
) {
    set_module_release_attestation_epoch_snapshot_with_stakes(world, threshold, signer_node_ids, &[])
}

fn set_module_release_attestation_epoch_snapshot_with_stakes(
    world: &mut World,
    threshold: u16,
    signer_node_ids: &[&str],
    validator_stakes: &[(&str, u64)],
) {
    let epoch_len = world
        .governance_execution_policy()
        .epoch_length_ticks
        .max(1);
    let epoch_id = world.state().time / epoch_len;
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id,
            threshold,
            signer_node_ids: signer_node_ids
                .iter()
                .map(|signer| signer.to_string())
                .collect(),
            validator_stakes: validator_stakes
                .iter()
                .map(|(node_id, stake)| ((*node_id).to_string(), *stake))
                .collect(),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set module release attestation epoch snapshot");
}

fn prepare_module_release_apply_ready_request(
    world: &mut World,
    requester_agent_id: &str,
    operator_agent_id: &str,
    module_id: &str,
) -> u64 {
    let wasm_bytes = format!("module-release-ready-{module_id}").into_bytes();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: requester_agent_id.to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: requester_agent_id.to_string(),
        manifest: base_manifest(module_id, "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: operator_agent_id.to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(world, operator_agent_id, operator_agent_id, &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: operator_agent_id.to_string(),
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
            format!("bafyreadyapply{request_id}{index:02}").as_str(),
        );
        world
            .step()
            .expect("submit module release attestation before apply");
    }
    request_id
}

#[test]
fn module_release_state_machine_runs_submit_shadow_approve_apply() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    set_module_release_attestation_epoch_snapshot(
        &mut world,
        2,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );

    let wasm_bytes = b"module-release-state-machine".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let manifest = base_manifest("m.loop.release", "0.1.0", &wasm_hash);
    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: manifest.clone(),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["runtime".to_string(), "security".to_string()],
        profile_changes: sample_profile_changes(),
    });
    world.step().expect("submit module release request");

    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested {
            request_id,
            requester_agent_id,
            required_roles,
            ..
        }) => {
            assert_eq!(requester_agent_id, "publisher-1");
            assert_eq!(
                required_roles,
                &vec!["runtime".to_string(), "security".to_string()]
            );
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };
    assert!(request_id > 0);
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Requested)
    ));
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release manifest mapping state");
    assert_eq!(mapping.status, ModuleReleaseRequestStatus::Requested);
    assert_eq!(mapping.module_id, "m.loop.release");
    assert_eq!(mapping.release_id, format!("release-{request_id}"));
    assert_eq!(mapping.attestation_count, 0);
    assert!(mapping.release_wasm_hash.is_none());
    assert!(mapping.attestation_platforms.is_empty());
    assert!(!mapping.receipt_evidence_conflict);

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    let shadow_manifest_hash = match &world.journal().events.last().expect("shadow event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseShadowed {
            request_id: event_request_id,
            operator_agent_id,
            manifest_hash,
        }) => {
            assert_eq!(*event_request_id, request_id);
            assert_eq!(operator_agent_id, "operator-1");
            manifest_hash.clone()
        }
        other => panic!("expected module release shadowed event: {other:?}"),
    };
    assert!(!shadow_manifest_hash.is_empty());
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Shadowed)
    ));
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release manifest mapping after shadow");
    assert_eq!(mapping.status, ModuleReleaseRequestStatus::Shadowed);
    assert_eq!(
        mapping.shadow_manifest_hash.as_deref(),
        Some(shadow_manifest_hash.as_str())
    );
    bind_release_roles(&mut world, "operator-1", "operator-1", &["security"]);
    bind_release_roles(&mut world, "operator-1", "publisher-1", &["runtime"]);

    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve module release security role");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::PartiallyApproved)
    ));

    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "publisher-1".to_string(),
        request_id,
        role: "runtime".to_string(),
    });
    world.step().expect("approve module release runtime role");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Approved)
    ));
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_1,
        "linux-x86_64",
        "bafyreleaseattestsm001",
    );
    world.step().expect("submit attestation signer1");
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_2,
        "linux-x86_64",
        "bafyreleaseattestsm002",
    );
    world.step().expect("submit attestation signer2");
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release manifest mapping state");
    assert_eq!(mapping.attestation_count, 2);
    let identity = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state")
        .manifest
        .artifact_identity
        .as_ref()
        .expect("artifact identity");
    assert_eq!(
        mapping.release_wasm_hash.as_deref(),
        Some(wasm_hash.as_str())
    );
    assert_eq!(
        mapping.release_source_hash.as_deref(),
        Some(identity.source_hash.as_str())
    );
    assert_eq!(
        mapping.release_build_manifest_hash.as_deref(),
        Some(identity.build_manifest_hash.as_str())
    );
    assert_eq!(
        mapping.release_builder_image_digest.as_deref(),
        Some(TEST_RELEASE_BUILDER_IMAGE_DIGEST)
    );
    assert_eq!(
        mapping.release_container_platform.as_deref(),
        Some(TEST_RELEASE_CONTAINER_PLATFORM)
    );
    assert_eq!(
        mapping.release_canonicalizer_version.as_deref(),
        Some(TEST_RELEASE_CANONICALIZER_VERSION)
    );
    assert_eq!(
        mapping.attestation_platforms,
        vec!["linux-x86_64".to_string()]
    );
    assert_eq!(mapping.attestation_proof_cids.len(), 2);
    assert!(!mapping.receipt_evidence_conflict);

    world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply module release request");

    let apply_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied {
                request_id: event_request_id,
                operator_agent_id,
                installer_agent_id,
                module_id,
                module_version,
                proposal_id,
                manifest_hash,
                ..
            }) if *event_request_id == request_id => Some((
                operator_agent_id.clone(),
                installer_agent_id.clone(),
                module_id.clone(),
                module_version.clone(),
                *proposal_id,
                manifest_hash.clone(),
            )),
            _ => None,
        })
        .expect("module release applied event");
    let (
        operator_agent_id,
        installer_agent_id,
        module_id,
        module_version,
        proposal_id,
        applied_manifest_hash,
    ) = apply_event;
    assert_eq!(operator_agent_id, "operator-1");
    assert_eq!(installer_agent_id, "publisher-1");
    assert_eq!(module_id, "m.loop.release");
    assert_eq!(module_version, "0.1.0");
    assert!(proposal_id > 0);
    assert!(!applied_manifest_hash.is_empty());

    let release_state = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state");
    assert_eq!(release_state.status, ModuleReleaseRequestStatus::Applied);
    assert_eq!(
        release_state.applied_manifest_hash.as_deref(),
        Some(applied_manifest_hash.as_str())
    );
    assert_eq!(release_state.applied_proposal_id, Some(proposal_id));
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release manifest mapping state after apply");
    assert_eq!(mapping.status, ModuleReleaseRequestStatus::Applied);
    assert_eq!(
        mapping.applied_manifest_hash.as_deref(),
        Some(applied_manifest_hash.as_str())
    );
    assert_eq!(mapping.applied_proposal_id, Some(proposal_id));
    assert_eq!(
        world.module_registry().active.get("m.loop.release"),
        Some(&"0.1.0".to_string())
    );

    let product = world
        .product_profile("module_rack")
        .expect("product profile applied");
    assert_eq!(product.role_tag, "scale");
    let recipe = world
        .recipe_profile("recipe.assembler.module_rack")
        .expect("recipe profile applied");
    assert_eq!(recipe.stage_gate, "scale_out");
    let factory = world
        .factory_profile("factory.assembler.mk1")
        .expect("factory profile applied");
    assert_eq!(factory.recipe_slots, 4);

    let snapshot = world.snapshot();
    let restored =
        World::from_snapshot(snapshot, world.journal().clone()).expect("restore from snapshot");
    assert!(restored.product_profile("module_rack").is_some());
    assert!(restored
        .recipe_profile("recipe.assembler.module_rack")
        .is_some());
    assert!(restored.factory_profile("factory.assembler.mk1").is_some());
}

#[test]
fn module_release_submit_attestation_persists_audit_evidence() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    bind_attestor_node_identity(&mut world, "attestor-node-1");

    let wasm_bytes = b"module-release-attestation-audit".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.attest", "0.1.0", &wasm_hash),
        activate: false,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["runtime".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    let identity = request_manifest_identity(&world, request_id);
    world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        signer_node_id: "attestor-node-1".to_string(),
        platform: "linux-x86_64".to_string(),
        build_manifest_hash: identity.build_manifest_hash.clone(),
        source_hash: identity.source_hash.clone(),
        wasm_hash: wasm_hash.clone(),
        proof_cid: "bafyreleaseattest0001".to_string(),
        builder_image_digest: TEST_RELEASE_BUILDER_IMAGE_DIGEST.to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
    world.step().expect("submit module release attestation");

    match &world
        .journal()
        .events
        .last()
        .expect("attestation event")
        .body
    {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseAttested {
            request_id: event_request_id,
            operator_agent_id,
            signer_node_id,
            platform,
            build_manifest_hash: event_build_manifest_hash,
            source_hash: event_source_hash,
            wasm_hash: event_wasm_hash,
            proof_cid,
            builder_image_digest,
            container_platform,
            canonicalizer_version,
        }) => {
            assert_eq!(*event_request_id, request_id);
            assert_eq!(operator_agent_id, "operator-1");
            assert_eq!(signer_node_id, "attestor-node-1");
            assert_eq!(platform, "linux-x86_64");
            assert_eq!(event_build_manifest_hash, &identity.build_manifest_hash);
            assert_eq!(event_source_hash, &identity.source_hash);
            assert_eq!(event_wasm_hash, &wasm_hash);
            assert_eq!(proof_cid, "bafyreleaseattest0001");
            assert_eq!(builder_image_digest, TEST_RELEASE_BUILDER_IMAGE_DIGEST);
            assert_eq!(container_platform, TEST_RELEASE_CONTAINER_PLATFORM);
            assert_eq!(canonicalizer_version, TEST_RELEASE_CANONICALIZER_VERSION);
        }
        other => panic!("expected module release attested event: {other:?}"),
    }

    let request = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state");
    let attestation = request
        .attestations
        .get("attestor-node-1|linux-x86_64")
        .expect("attestation state");
    assert_eq!(attestation.proof_cid, "bafyreleaseattest0001");
    assert_eq!(attestation.wasm_hash, wasm_hash);
    assert_eq!(
        attestation.builder_image_digest,
        TEST_RELEASE_BUILDER_IMAGE_DIGEST
    );
    assert_eq!(
        attestation.container_platform,
        TEST_RELEASE_CONTAINER_PLATFORM
    );
    assert_eq!(
        attestation.canonicalizer_version,
        TEST_RELEASE_CANONICALIZER_VERSION
    );
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release mapping state");
    assert_eq!(mapping.attestation_count, 1);
    assert_eq!(
        mapping.attestation_platforms,
        vec!["linux-x86_64".to_string()]
    );
    assert_eq!(
        mapping.attestation_proof_cids,
        vec!["bafyreleaseattest0001".to_string()]
    );
    assert!(!mapping.receipt_evidence_conflict);
}

#[test]
fn module_release_submit_attestation_rejects_conflicting_duplicate() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    bind_attestor_node_identity(&mut world, "attestor-node-1");

    let wasm_bytes = b"module-release-attestation-conflict".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.attest.dup", "0.1.0", &wasm_hash),
        activate: false,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["runtime".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    let identity = request_manifest_identity(&world, request_id);
    world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        signer_node_id: "attestor-node-1".to_string(),
        platform: "linux-x86_64".to_string(),
        build_manifest_hash: identity.build_manifest_hash.clone(),
        source_hash: identity.source_hash.clone(),
        wasm_hash: wasm_hash.clone(),
        proof_cid: "bafyreleaseattestdup0001".to_string(),
        builder_image_digest: TEST_RELEASE_BUILDER_IMAGE_DIGEST.to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
    world.step().expect("submit first attestation");

    let action_id = world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        signer_node_id: "attestor-node-1".to_string(),
        platform: "linux-x86_64".to_string(),
        build_manifest_hash: identity.build_manifest_hash,
        source_hash: identity.source_hash,
        wasm_hash,
        proof_cid: "bafyreleaseattestdup0002".to_string(),
        builder_image_digest: TEST_RELEASE_BUILDER_IMAGE_DIGEST.to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
    world.step().expect("submit conflicting attestation");

    assert_rule_denied_note_for_action(&world, action_id, "conflicting attestation already exists");
    let request = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state");
    assert_eq!(request.attestations.len(), 1);
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release mapping state");
    assert_eq!(mapping.attestation_count, 1);
    assert_eq!(
        mapping.attestation_platforms,
        vec!["linux-x86_64".to_string()]
    );
    assert_eq!(
        mapping.attestation_proof_cids,
        vec!["bafyreleaseattestdup0001".to_string()]
    );
    assert!(!mapping.receipt_evidence_conflict);
}

#[test]
fn module_release_apply_rejects_when_attestation_threshold_not_met() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    set_module_release_attestation_epoch_snapshot(
        &mut world,
        2,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );

    let wasm_bytes = b"module-release-threshold-not-met".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.threshold", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(&mut world, "operator-1", "operator-1", &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve required role");
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_1,
        "linux-x86_64",
        "bafyreleaseattestthreshold001",
    );
    world.step().expect("submit single attestation");

    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply module release request");

    assert_rule_denied_note_for_action(&world, action_id, "attestation threshold not met");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Approved)
    ));
}

#[test]
fn module_release_apply_rejects_when_attestor_not_in_epoch_snapshot() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    set_module_release_attestation_epoch_snapshot(&mut world, 1, &[LOCAL_FINALITY_SIGNER_1]);

    let wasm_bytes = b"module-release-attestor-outside-snapshot".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.snapshot-filter", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(&mut world, "operator-1", "operator-1", &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve required role");
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_2,
        "linux-x86_64",
        "bafyreleaseattestsnapshot001",
    );
    world.step().expect("submit out-of-snapshot attestation");

    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply module release request");

    assert_rule_denied_note_for_action(&world, action_id, "attestation threshold not met");
}

#[test]
fn module_release_apply_rejects_when_attestation_receipt_evidence_mismatches() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    set_module_release_attestation_epoch_snapshot(
        &mut world,
        2,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );

    let wasm_bytes = b"module-release-attestation-receipt-mismatch".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.receipt.mismatch", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: ModuleProfileChanges::default(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(&mut world, "operator-1", "operator-1", &["security"]);
    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve required role");

    let identity = request_manifest_identity(&world, request_id);
    world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        signer_node_id: LOCAL_FINALITY_SIGNER_1.to_string(),
        platform: "darwin-arm64".to_string(),
        build_manifest_hash: identity.build_manifest_hash.clone(),
        source_hash: identity.source_hash.clone(),
        wasm_hash: wasm_hash.clone(),
        proof_cid: "bafyreleaseattestreceiptmismatch001".to_string(),
        builder_image_digest: TEST_RELEASE_BUILDER_IMAGE_DIGEST.to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
    world.step().expect("submit first attestation");
    world.submit_action(Action::ModuleReleaseSubmitAttestation {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        signer_node_id: LOCAL_FINALITY_SIGNER_2.to_string(),
        platform: "linux-x86_64".to_string(),
        build_manifest_hash: identity.build_manifest_hash,
        source_hash: identity.source_hash,
        wasm_hash,
        proof_cid: "bafyreleaseattestreceiptmismatch002".to_string(),
        builder_image_digest:
            "sha256:2222222222222222222222222222222222222222222222222222222222222222".to_string(),
        container_platform: TEST_RELEASE_CONTAINER_PLATFORM.to_string(),
        canonicalizer_version: TEST_RELEASE_CANONICALIZER_VERSION.to_string(),
    });
    world.step().expect("submit second attestation");

    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply module release request");

    assert_rule_denied_note_for_action(&world, action_id, "attestation receipt evidence mismatch");
    let mapping = world
        .state()
        .module_release_manifest_mappings
        .get(&request_id)
        .expect("module release mapping state after receipt mismatch");
    assert_eq!(mapping.attestation_count, 2);
    assert_eq!(
        mapping.attestation_platforms,
        vec!["darwin-arm64".to_string(), "linux-x86_64".to_string()]
    );
    assert_eq!(mapping.attestation_proof_cids.len(), 2);
    assert!(mapping.receipt_evidence_conflict);
}

#[test]
fn module_release_shadow_rejects_duplicate_profile_changes() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-dup-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.dup-profile", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: duplicate_profile_changes(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("shadow module release request with dup profiles");
    assert_rule_denied_note_for_action(&world, action_id, "duplicate product profile_id");
}

#[test]
fn module_release_shadow_rejects_duplicate_factory_profile_changes() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-dup-factory-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.dup-factory", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: duplicate_factory_profile_changes(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("shadow module release request with dup factory profiles");
    assert_rule_denied_note_for_action(&world, action_id, "duplicate factory profile_id");
}

#[test]
fn module_release_shadow_rejects_duplicate_recipe_profile_changes() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-dup-recipe-profile".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.dup-recipe", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string()],
        profile_changes: duplicate_recipe_profile_changes(),
    });
    world.step().expect("submit module release request");
    let request_id = match &world.journal().events.last().expect("submit event").body {
        WorldEventBody::Domain(DomainEvent::ModuleReleaseRequested { request_id, .. }) => {
            *request_id
        }
        other => panic!("expected module release requested event: {other:?}"),
    };

    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("shadow module release request with dup recipe profiles");
    assert_rule_denied_note_for_action(&world, action_id, "duplicate recipe profile_id");
}
