use super::super::{
    Action, ActionEnvelope, DomainEvent, GovernanceFinalityEpochSnapshot, ProposalDecision,
    WorldEventBody,
};
use super::{World, WorldError};
use crate::{FactoryProfileV1, ModuleProfileChanges, ProductProfileV1, RecipeProfileV1};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::*;

fn dispatch(world: &mut World, action: Action) -> Result<bool, WorldError> {
    world.try_apply_runtime_module_action(&ActionEnvelope {
        committed_recipe_origin: None,
        id: 950,
        action,
    })
}

fn profiles() -> ModuleProfileChanges {
    ModuleProfileChanges {
        product_profiles: vec![ProductProfileV1 {
            product_id: "release-product".into(),
            role_tag: "scale".into(),
            maintenance_sink: vec![],
            tradable: true,
            unlock_stage: "scale_out".into(),
        }],
        recipe_profiles: vec![RecipeProfileV1 {
            recipe_id: "release-recipe".into(),
            bottleneck_tags: vec![],
            stage_gate: "scale_out".into(),
            preferred_factory_tags: vec![],
        }],
        factory_profiles: vec![FactoryProfileV1 {
            factory_id: "release-factory".into(),
            tier: 2,
            recipe_slots: 4,
            tags: vec!["assembler".into()],
        }],
    }
}

fn ready_request(matching_module: bool) -> (World, u64) {
    ready_request_with_profiles(matching_module, profiles())
}

fn ready_request_with_profiles(
    matching_module: bool,
    profile_changes: ModuleProfileChanges,
) -> (World, u64) {
    let mut world = World::new();
    for agent_id in ["publisher", "operator"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: agent_id.into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().unwrap();
        world
            .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 1000)
            .unwrap();
    }
    let bytes = b"release-publication-atomicity";
    let hash = crate::util::sha256_hex(bytes);
    dispatch(
        &mut world,
        Action::DeployModuleArtifact {
            publisher_agent_id: "publisher".into(),
            wasm_hash: hash.clone(),
            wasm_bytes: bytes.to_vec(),
        },
    )
    .unwrap();
    let manifest = ModuleManifest {
        module_id: "m.release-atomic".into(),
        name: "ReleaseAtomic".into(),
        version: "1.0.0".into(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".into()],
        subscriptions: vec![],
        required_caps: vec![],
        artifact_identity: Some(crate::tests::signed_test_artifact_identity(&hash)),
        limits: ModuleLimits::default(),
    };
    if matching_module {
        world.module_registry.records.insert(
            ModuleRegistry::record_key(&manifest.module_id, &manifest.version),
            ModuleRecord {
                manifest: manifest.clone(),
                registered_at: 0,
                registered_by: "fixture".into(),
                audit_event_id: None,
            },
        );
        world
            .module_registry
            .active
            .insert(manifest.module_id.clone(), manifest.version.clone());
    }
    dispatch(
        &mut world,
        Action::ModuleReleaseSubmit {
            requester_agent_id: "publisher".into(),
            manifest: manifest.clone(),
            activate: true,
            install_target: ModuleInstallTarget::SelfAgent,
            required_roles: vec!["security".into()],
            profile_changes,
        },
    )
    .unwrap();
    let request_id = *world.state.module_release_requests.keys().next().unwrap();
    dispatch(
        &mut world,
        Action::ModuleReleaseShadow {
            operator_agent_id: "operator".into(),
            request_id,
        },
    )
    .unwrap();
    dispatch(
        &mut world,
        Action::ModuleReleaseBindRoles {
            operator_agent_id: "operator".into(),
            target_agent_id: "operator".into(),
            roles: vec!["security".into()],
        },
    )
    .unwrap();
    dispatch(
        &mut world,
        Action::ModuleReleaseApproveRole {
            approver_agent_id: "operator".into(),
            request_id,
            role: "security".into(),
        },
    )
    .unwrap();
    let signers = [
        "governance.local.finality.signer.1",
        "governance.local.finality.signer.2",
    ];
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 0,
            threshold: 2,
            signer_node_ids: signers.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .unwrap();
    let identity = manifest.artifact_identity.unwrap();
    for (index, signer) in signers.into_iter().enumerate() {
        dispatch(
            &mut world,
            Action::ModuleReleaseSubmitAttestation {
                operator_agent_id: "operator".into(),
                request_id,
                signer_node_id: signer.into(),
                platform: "linux-x86_64".into(),
                build_manifest_hash: identity.build_manifest_hash.clone(),
                source_hash: identity.source_hash.clone(),
                wasm_hash: hash.clone(),
                proof_cid: format!("bafyreleaseatomic{index}"),
                builder_image_digest: format!("sha256:{}", "1".repeat(64)),
                container_platform: "linux-x86_64".into(),
                canonicalizer_version: "strip-custom-sections-v1".into(),
            },
        )
        .unwrap();
    }
    assert_eq!(
        world.state.module_release_requests[&request_id]
            .attestations
            .len(),
        2
    );
    (world, request_id)
}

fn apply(world: &mut World, request_id: u64) -> Result<bool, WorldError> {
    dispatch(
        world,
        Action::ModuleReleaseApply {
            operator_agent_id: "operator".into(),
            request_id,
        },
    )
}

fn assert_world_matches(world: &World, expected: &World) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(world.module_cache, expected.module_cache);
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
}

fn prelude_oracle(before: &World, failed: &World) -> World {
    let mut expected = before.clone();
    let id = before.next_proposal_id;
    let manifest = failed.proposals[&id].manifest.clone();
    assert_eq!(
        expected
            .propose_manifest_update(manifest, "publisher")
            .unwrap(),
        id
    );
    expected.shadow_proposal(id).unwrap();
    expected
        .approve_proposal(id, "publisher", ProposalDecision::Approve)
        .unwrap();
    expected
}

#[test]
fn profile_only_release_for_matching_module_uses_governed_proposal() {
    let (mut world, id) = ready_request(true);
    let baseline = world.snapshot();
    assert!(apply(&mut world, id).expect("profile-only release has a governed proposal"));
    let applied_proposal_id = world.state.module_release_requests[&id]
        .applied_proposal_id
        .expect("profile-only release proposal id");
    assert!(applied_proposal_id > 0);
    assert!(world.state.product_profiles.contains_key("release-product"));
    assert!(world.state.recipe_profiles.contains_key("release-recipe"));
    assert!(world.state.factory_profiles.contains_key("release-factory"));
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied {
            proposal_id,
            ..
        })) if *proposal_id == applied_proposal_id
    ));
    assert_root_and_replay(&world, baseline);
}

#[test]
fn missing_release_mapping_rejects_without_install_profiles_or_request_mutation() {
    let (mut world, id) = ready_request(false);
    world.state.module_release_manifest_mappings.remove(&id);
    let before = world.clone();
    let error = apply(&mut world, id).expect_err("missing mapping rejects release business batch");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("mapping"))
    );
    assert_world_matches(&world, &prelude_oracle(&before, &world));
}

#[test]
fn governed_release_final_tail_failure_preserves_only_proposal_prelude() {
    let (mut world, id) = ready_request(false);
    let before = world.clone();
    world.fail_append_after_publication_prepare_on_nth_for_test(9);
    let error =
        apply(&mut world, id).expect_err("release final tail must honor ninth prepare failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("injected"))
    );
    assert_world_matches(&world, &prelude_oracle(&before, &world));
}

#[test]
fn release_applied_missing_mapping_preserves_request_and_legacy_materials() {
    let (mut world, id) = ready_request(false);
    world.state.module_release_manifest_mappings.remove(&id);
    world.state.material_ledgers.clear();
    world.state.materials.insert("iron_ingot".into(), 7);
    let before = world.state.clone();
    let error = world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleReleaseApplied {
                request_id: id,
                operator_agent_id: "operator".into(),
                installer_agent_id: "publisher".into(),
                instance_id: "m.release-atomic#1".into(),
                module_id: "m.release-atomic".into(),
                module_version: "1.0.0".into(),
                proposal_id: 1,
                manifest_hash: "applied-hash".into(),
            },
            99,
        )
        .unwrap_err();
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("mapping"))
    );
    assert_eq!(world.state, before);
}

fn assert_root_and_replay(world: &World, baseline: crate::Snapshot) {
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replayed.state, world.state);
    assert_eq!(replayed.manifest, world.manifest);
    assert_eq!(replayed.module_registry, world.module_registry);
    assert_eq!(replayed.module_tick_schedule, world.module_tick_schedule);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

fn raw_events(id: u64) -> Vec<DomainEvent> {
    let changes = profiles();
    vec![
        DomainEvent::ProductProfileGoverned {
            operator_agent_id: "operator".into(),
            proposal_id: 1,
            profile: changes.product_profiles[0].clone(),
        },
        DomainEvent::RecipeProfileGoverned {
            operator_agent_id: "operator".into(),
            proposal_id: 1,
            profile: changes.recipe_profiles[0].clone(),
        },
        DomainEvent::FactoryProfileGoverned {
            operator_agent_id: "operator".into(),
            proposal_id: 1,
            profile: changes.factory_profiles[0].clone(),
        },
        DomainEvent::ModuleReleaseApplied {
            request_id: id,
            operator_agent_id: "operator".into(),
            installer_agent_id: "publisher".into(),
            instance_id: "raw-instance".into(),
            module_id: "m.release-atomic".into(),
            module_version: "1.0.0".into(),
            proposal_id: 0,
            manifest_hash: "raw-hash".into(),
        },
    ]
}

#[test]
fn release_success_merges_same_or_distinct_actor_fees_mailboxes_and_roots() {
    for operator in ["operator", "publisher"] {
        let (mut world, id) = ready_request(false);
        world.state.time += 1;
        let baseline = world.snapshot();
        let payer_before = world
            .agent_resource_balance("publisher", ResourceKind::Electricity)
            .unwrap();
        let payer_mailbox = world.state.agents["publisher"].mailbox.len();
        let operator_mailbox = world.state.agents[operator].mailbox.len();
        assert!(
            dispatch(
                &mut world,
                Action::ModuleReleaseApply {
                    operator_agent_id: operator.into(),
                    request_id: id
                }
            )
            .unwrap()
        );
        assert_eq!(
            world
                .agent_resource_balance("publisher", ResourceKind::Electricity)
                .unwrap(),
            payer_before - 2
        );
        assert_eq!(
            world.state.agents["publisher"].mailbox.len(),
            payer_mailbox + if operator == "publisher" { 5 } else { 1 }
        );
        assert_eq!(
            world.state.agents[operator].mailbox.len(),
            operator_mailbox + if operator == "publisher" { 5 } else { 4 }
        );
        assert_eq!(
            world.state.agents["publisher"].last_active,
            world.state.time
        );
        assert_eq!(world.state.agents[operator].last_active, world.state.time);
        assert!(matches!(
            world.state.module_release_requests[&id].status,
            crate::state::ModuleReleaseRequestStatus::Applied
        ));
        assert_root_and_replay(&world, baseline);
    }
}

#[test]
fn proposal_zero_empty_profiles_release_remains_successful() {
    let (mut world, id) = ready_request_with_profiles(true, ModuleProfileChanges::default());
    let baseline = world.snapshot();
    let next_proposal = world.next_proposal_id;
    assert!(apply(&mut world, id).unwrap());
    assert_eq!(world.next_proposal_id, next_proposal);
    assert_eq!(
        world.state.module_release_requests[&id].applied_proposal_id,
        None
    );
    assert_eq!(world.journal().events.len(), baseline.journal_len + 2);
    assert!(matches!(
        world.journal().events.last().unwrap().body,
        WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied { proposal_id: 0, .. })
    ));
    assert_root_and_replay(&world, baseline);
}

#[test]
fn release_profiles_publish_sorted_with_category_order() {
    let mut changes = profiles();
    let mut product = changes.product_profiles[0].clone();
    product.product_id = "aaa-product".into();
    changes.product_profiles.push(product);
    let mut recipe = changes.recipe_profiles[0].clone();
    recipe.recipe_id = "aaa-recipe".into();
    changes.recipe_profiles.push(recipe);
    let mut factory = changes.factory_profiles[0].clone();
    factory.factory_id = "aaa-factory".into();
    changes.factory_profiles.push(factory);
    let (mut world, id) = ready_request_with_profiles(false, changes);
    let baseline = world.snapshot();
    assert!(apply(&mut world, id).unwrap());
    let ordered: Vec<_> = world.journal().events[baseline.journal_len..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ProductProfileGoverned { profile, .. }) => {
                Some(format!("product:{}", profile.product_id))
            }
            WorldEventBody::Domain(DomainEvent::RecipeProfileGoverned { profile, .. }) => {
                Some(format!("recipe:{}", profile.recipe_id))
            }
            WorldEventBody::Domain(DomainEvent::FactoryProfileGoverned { profile, .. }) => {
                Some(format!("factory:{}", profile.factory_id))
            }
            WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied { .. }) => {
                Some("applied".into())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        ordered,
        [
            "product:aaa-product",
            "product:release-product",
            "recipe:aaa-recipe",
            "recipe:release-recipe",
            "factory:aaa-factory",
            "factory:release-factory",
            "applied"
        ]
    );
    assert_root_and_replay(&world, baseline);
}

#[test]
fn release_profile_validation_preserves_input_priority_and_forbids_overwrite() {
    for duplicate_first in [false, true] {
        let (mut world, id) = ready_request(false);
        let original = profiles().product_profiles[0].clone();
        let mut invalid = original.clone();
        invalid.product_id.clear();
        world
            .state
            .module_release_requests
            .get_mut(&id)
            .unwrap()
            .profile_changes
            .product_profiles = if duplicate_first {
            vec![original.clone(), original, invalid]
        } else {
            vec![invalid, original.clone(), original]
        };
        let before = world.snapshot();
        assert!(apply(&mut world, id).unwrap());
        let expected = if duplicate_first {
            "duplicate product"
        } else {
            "product_id cannot be empty"
        };
        assert!(
            matches!(&world.journal().events.last().unwrap().body, WorldEventBody::Domain(DomainEvent::ActionRejected { reason: crate::RejectReason::RuleDenied { notes }, .. }) if notes.iter().any(|note| note.contains(expected)))
        );
        assert_eq!(world.state, before.state);
        assert_eq!(world.next_proposal_id, 1);
    }
    let (mut world, id) = ready_request(false);
    let profile = profiles().product_profiles[0].clone();
    world
        .state
        .product_profiles
        .insert(profile.product_id.clone(), profile);
    let before = world.snapshot();
    assert!(apply(&mut world, id).unwrap());
    assert!(
        matches!(&world.journal().events.last().unwrap().body, WorldEventBody::Domain(DomainEvent::ActionRejected { reason: crate::RejectReason::RuleDenied { notes }, .. }) if notes.iter().any(|note| note.contains("overwrite is forbidden")))
    );
    assert_eq!(world.state, before.state);
}

#[test]
fn raw_profile_and_final_status_failpoints_publish_nothing() {
    let (world, id) = ready_request(false);
    for event in raw_events(id) {
        let mut attempt = world.clone();
        attempt.fail_next_append_after_publication_prepare_for_test();
        let error = attempt
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap_err();
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("injected"))
        );
        assert_world_matches(&attempt, &world);
    }
}

#[test]
fn raw_profile_and_final_status_rejections_keep_priority_and_state() {
    let (world, id) = ready_request(false);
    for event in raw_events(id).into_iter().take(3) {
        for missing_actor in [true, false] {
            let mut attempt = world.clone();
            let mut invalid = event.clone();
            match &mut invalid {
                DomainEvent::ProductProfileGoverned {
                    operator_agent_id,
                    proposal_id,
                    profile,
                } => {
                    *proposal_id = 0;
                    profile.product_id.clear();
                    if missing_actor {
                        *operator_agent_id = "missing".into();
                    }
                }
                DomainEvent::RecipeProfileGoverned {
                    operator_agent_id,
                    proposal_id,
                    profile,
                } => {
                    *proposal_id = 0;
                    profile.recipe_id.clear();
                    if missing_actor {
                        *operator_agent_id = "missing".into();
                    }
                }
                DomainEvent::FactoryProfileGoverned {
                    operator_agent_id,
                    proposal_id,
                    profile,
                } => {
                    *proposal_id = 0;
                    profile.factory_id.clear();
                    if missing_actor {
                        *operator_agent_id = "missing".into();
                    }
                }
                _ => unreachable!(),
            }
            let error = attempt
                .append_event(WorldEventBody::Domain(invalid), None)
                .unwrap_err();
            if missing_actor {
                assert!(matches!(error, WorldError::AgentNotFound { .. }));
            } else {
                assert!(
                    matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("proposal_id must be > 0"))
                );
            }
            assert_world_matches(&attempt, &world);
        }
    }
    for absent_request in [true, false] {
        let mut attempt = world.clone();
        attempt.state.module_release_manifest_mappings.remove(&id);
        if absent_request {
            attempt.state.module_release_requests.remove(&id);
        } else {
            attempt
                .state
                .module_release_requests
                .get_mut(&id)
                .unwrap()
                .status = crate::state::ModuleReleaseRequestStatus::Rejected;
        }
        let before = attempt.clone();
        let error = attempt
            .append_event(WorldEventBody::Domain(raw_events(id).pop().unwrap()), None)
            .unwrap_err();
        let expected = if absent_request {
            "request not found"
        } else {
            "invalid status"
        };
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains(expected))
        );
        assert_world_matches(&attempt, &before);
    }
}

#[test]
fn release_tail_retry_commits_once_and_replays_with_retained_prelude() {
    let (mut world, id) = ready_request(false);
    let baseline = world.snapshot();
    let payer = world
        .agent_resource_balance("publisher", ResourceKind::Electricity)
        .unwrap();
    world.fail_append_after_publication_prepare_on_nth_for_test(9);
    assert!(apply(&mut world, id).is_err());
    let old_proposal = world.proposals[&1].clone();
    assert!(apply(&mut world, id).unwrap());
    assert_eq!(world.proposals[&1], old_proposal);
    assert_eq!(
        world
            .agent_resource_balance("publisher", ResourceKind::Electricity)
            .unwrap(),
        payer - 2
    );
    assert_eq!(world.state.module_instances.len(), 1);
    for expected in ["install", "product", "recipe", "factory", "applied"] {
        let count = world.journal().events[baseline.journal_len..]
            .iter()
            .filter(|event| {
                matches!(
                    (&event.body, expected),
                    (
                        WorldEventBody::Domain(DomainEvent::ModuleInstalled { .. }),
                        "install"
                    ) | (
                        WorldEventBody::Domain(DomainEvent::ProductProfileGoverned { .. }),
                        "product"
                    ) | (
                        WorldEventBody::Domain(DomainEvent::RecipeProfileGoverned { .. }),
                        "recipe"
                    ) | (
                        WorldEventBody::Domain(DomainEvent::FactoryProfileGoverned { .. }),
                        "factory"
                    ) | (
                        WorldEventBody::Domain(DomainEvent::ModuleReleaseApplied { .. }),
                        "applied"
                    )
                )
            })
            .count();
        assert_eq!(count, 1);
    }
    assert_root_and_replay(&world, baseline);
}

#[test]
fn release_commit_keeps_rolling_era_and_retention_accounting() {
    let (mut world, id) = ready_request(false);
    world.next_event_id = u64::MAX - 1;
    world.next_event_id_era = 7;
    world.runtime_memory_limits.max_journal_events = 5;
    let evicted = world.runtime_backpressure_stats.journal_events_evicted;
    assert!(apply(&mut world, id).unwrap());
    assert_eq!(world.next_event_id_era, 8);
    assert_eq!(world.journal().events.len(), 5);
    assert!(world.runtime_backpressure_stats.journal_events_evicted > evicted);
    assert_eq!(
        world.journal().events.last().unwrap().id + 1,
        world.next_event_id
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
}
