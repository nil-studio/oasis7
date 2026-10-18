use super::super::{
    Action, ActionEnvelope, DomainEvent, GovernanceEvent, ProposalDecision, WorldEventBody,
};
use super::{World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::*;

#[path = "rollback_admission_freeze_test.rs"]
mod rollback_admission_freeze_regressions;

fn manifest(world: &mut World, version: &str) -> ModuleManifest {
    let bytes = format!("governed-atomic-{version}").into_bytes();
    let hash = crate::util::sha256_hex(&bytes);
    world
        .register_module_artifact(hash.clone(), &bytes)
        .unwrap();
    ModuleManifest {
        module_id: "m.governed-atomic".into(),
        name: "GovernedAtomic".into(),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".into(),
        exports: vec!["call".into()],
        subscriptions: vec![],
        required_caps: vec![],
        artifact_identity: Some(crate::tests::signed_test_artifact_identity(&hash)),
        abi_contract: ModuleAbiContract::default(),
        limits: ModuleLimits::default(),
    }
}

fn install(module: ModuleManifest) -> Action {
    Action::InstallModuleFromArtifact {
        installer_agent_id: "payer".into(),
        manifest: module,
        activate: true,
    }
}

fn upgrade(module: ModuleManifest) -> Action {
    Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "payer".into(),
        instance_id: "m.governed-atomic#1".into(),
        from_module_version: "1.0.0".into(),
        manifest: module,
        activate: true,
    }
}

fn dispatch(world: &mut World, action: Action) -> Result<bool, WorldError> {
    world.try_apply_runtime_module_action(&ActionEnvelope {
        committed_recipe_origin: None,
        id: 900,
        action,
    })
}

fn fixture(level: usize) -> (World, ModuleManifest) {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "payer".into(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().unwrap();
    world
        .set_agent_resource_balance("payer", ResourceKind::Electricity, 1000)
        .unwrap();
    let old = manifest(&mut world, "1.0.0");
    let next = manifest(&mut world, "2.0.0");
    if level > 0 {
        dispatch(&mut world, install(old.clone())).unwrap();
        assert_eq!(
            world.state.module_instances.len(),
            1,
            "fixture install must succeed"
        );
    }
    if level > 1 {
        dispatch(&mut world, upgrade(next.clone())).unwrap();
        assert_eq!(
            world.state.module_instances["m.governed-atomic#1"].module_version,
            "2.0.0"
        );
    }
    (world, if level == 0 { old } else { next })
}

fn assert_tail_failure_preserves_prelude(mut world: World, action: Action) {
    let mut prelude = world.clone();
    let proposal_id = world.next_proposal_id;
    let journal_len = world.journal().events.len();
    world.fail_append_after_publication_prepare_on_nth_for_test(5);
    let error = dispatch(&mut world, action).expect_err("lifecycle tail failpoint must propagate");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("injected"))
    );
    let proposed_manifest = world.proposals[&proposal_id].manifest.clone();
    assert_eq!(
        prelude
            .propose_manifest_update(proposed_manifest, "payer")
            .unwrap(),
        proposal_id
    );
    prelude.shadow_proposal(proposal_id).unwrap();
    prelude
        .approve_proposal(proposal_id, "payer", ProposalDecision::Approve)
        .unwrap();
    for event in &world.journal().events[journal_len..] {
        assert!(
            !matches!(
                &event.body,
                WorldEventBody::ModuleEvent(_)
                    | WorldEventBody::ManifestUpdated(_)
                    | WorldEventBody::Governance(GovernanceEvent::Applied { .. })
                    | WorldEventBody::Domain(
                        DomainEvent::ModuleInstalled { .. }
                            | DomainEvent::ModuleUpgraded { .. }
                            | DomainEvent::ModuleRollbackApplied { .. }
                    )
            ),
            "failed tail must not publish governance or lifecycle commit: {:?}",
            event.body
        );
    }
    assert_eq!(world.snapshot(), prelude.snapshot());
    assert_eq!(world.journal(), prelude.journal());
    assert_eq!(world.module_cache, prelude.module_cache);
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (prelude.next_event_id, prelude.next_event_id_era)
    );
    assert_eq!(
        world.tick_consensus_records(),
        prelude.tick_consensus_records()
    );
}

#[test]
fn governed_install_tail_failure_preserves_only_approved_prelude() {
    let (world, module) = fixture(0);
    assert_tail_failure_preserves_prelude(world, install(module));
}

#[test]
fn governed_upgrade_tail_failure_preserves_only_approved_prelude() {
    let (world, module) = fixture(1);
    assert_tail_failure_preserves_prelude(world, upgrade(module));
}

#[test]
fn governed_rollback_tail_failure_preserves_only_approved_prelude() {
    let (world, _) = fixture(2);
    assert_tail_failure_preserves_prelude(
        world,
        Action::RollbackModuleInstance {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            target_module_version: "1.0.0".into(),
        },
    );
}

#[test]
fn governed_lifecycle_keeps_governance_rejection_distinct_from_tail_error() {
    for (level, name) in [(0, "install"), (1, "upgrade"), (2, "rollback")] {
        for nth in [4, 5] {
            let (mut world, module) = fixture(level);
            let action = match level {
                0 => install(module),
                1 => upgrade(module),
                _ => Action::RollbackModuleInstance {
                    operator_agent_id: "payer".into(),
                    instance_id: "m.governed-atomic#1".into(),
                    target_module_version: "1.0.0".into(),
                },
            };
            if nth == 5 {
                assert_tail_failure_preserves_prelude(world, action);
                continue;
            }
            let before = world.clone();
            world.fail_append_after_publication_prepare_on_nth_for_test(nth);
            assert!(
                dispatch(&mut world, action).expect("governance failure is an action rejection")
            );
            let prefix = format!("apply module {name} rejected:");
            assert!(
                matches!(world.journal().events.last().map(|event| &event.body),
                    Some(WorldEventBody::Domain(DomainEvent::ActionRejected { reason: crate::RejectReason::RuleDenied { notes }, .. }))
                    if notes.iter().any(|note| note.starts_with(&prefix) && note.contains("injected"))
                )
            );
            assert_eq!(world.state, before.state);
            assert_eq!(world.manifest, before.manifest);
            assert_eq!(world.module_registry, before.module_registry);
            assert_eq!(world.module_cache, before.module_cache);
            assert_eq!(world.module_tick_schedule, before.module_tick_schedule);
            assert!(matches!(
                world.proposals[&before.next_proposal_id].status,
                crate::ProposalStatus::Approved { .. }
            ));
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
    }
}

fn action_for_level(level: usize, module: ModuleManifest) -> Action {
    match level {
        0 => install(module),
        1 => upgrade(module),
        _ => Action::RollbackModuleInstance {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            target_module_version: "1.0.0".into(),
        },
    }
}

#[test]
fn governed_success_keeps_event_order_roots_replay_and_rollback_schedule() {
    for level in 0..3 {
        let (mut world, module) = fixture(level);
        if level == 2 {
            world
                .module_tick_schedule
                .insert("m.governed-atomic#1".into(), 77);
        }
        let baseline = world.snapshot();
        let mailbox_len = world.state.agents["payer"].mailbox.len();
        let balance = world
            .agent_resource_balance("payer", ResourceKind::Electricity)
            .unwrap();
        assert!(dispatch(&mut world, action_for_level(level, module)).unwrap());
        let events = &world.journal().events[baseline.journal_len..];
        let manifest_index = events
            .iter()
            .position(|event| matches!(event.body, WorldEventBody::ManifestUpdated(_)))
            .unwrap();
        assert!(
            events[..manifest_index]
                .iter()
                .any(|event| matches!(event.body, WorldEventBody::ModuleEvent(_)))
        );
        assert!(matches!(
            events[manifest_index + 1].body,
            WorldEventBody::Governance(GovernanceEvent::Applied { .. })
        ));
        assert_eq!(manifest_index + 3, events.len());
        let tail_hash = match &events.last().unwrap().body {
            WorldEventBody::Domain(DomainEvent::ModuleInstalled { manifest_hash, .. })
                if level == 0 =>
            {
                manifest_hash
            }
            WorldEventBody::Domain(DomainEvent::ModuleUpgraded { manifest_hash, .. })
                if level == 1 =>
            {
                manifest_hash
            }
            WorldEventBody::Domain(DomainEvent::ModuleRollbackApplied {
                manifest_hash, ..
            }) if level == 2 => manifest_hash,
            other => panic!("wrong lifecycle tail: {other:?}"),
        };
        assert_eq!(tail_hash, &world.current_manifest_hash().unwrap());
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
        assert_eq!(
            world
                .agent_resource_balance("payer", ResourceKind::Electricity)
                .unwrap(),
            balance - 2
        );
        assert_eq!(world.state.agents["payer"].mailbox.len(), mailbox_len + 1);
        if level == 2 {
            assert_eq!(
                world.module_tick_schedule.get("m.governed-atomic#1"),
                Some(&77)
            );
            assert!(!world.module_tick_schedule.contains_key("m.governed-atomic"));
        }
        let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(
            replayed.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
        assert_eq!(replayed.module_tick_schedule, world.module_tick_schedule);
        assert_eq!(replayed.module_registry, world.module_registry);
    }
}

#[test]
fn governed_install_commit_preserves_rolling_era_and_bounded_journal() {
    let (mut world, module) = fixture(0);
    world.next_event_id = u64::MAX - 1;
    world.next_event_id_era = 9;
    world.runtime_memory_limits.max_journal_events = 5;
    let evicted = world.runtime_backpressure_stats.journal_events_evicted;
    assert!(dispatch(&mut world, install(module)).unwrap());
    assert_eq!(world.next_event_id_era, 10);
    assert_eq!(world.journal().events.len(), 5);
    assert!(world.runtime_backpressure_stats.journal_events_evicted > evicted);
    assert!(world.journal().events.iter().all(|event| event.id > 0));
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

#[test]
fn governed_install_preserves_local_policy_and_explicit_certificate_error_priority() {
    for explicit in [false, true] {
        let (mut world, module) = fixture(0);
        world.release_security_policy.allow_local_finality_signing = false;
        let before = world.clone();
        let action = if explicit {
            Action::InstallModuleFromArtifactWithFinality {
                installer_agent_id: "payer".into(),
                manifest: module,
                activate: true,
                finality_certificate: crate::GovernanceFinalityCertificate {
                    proposal_id: 999,
                    manifest_hash: "invalid".into(),
                    consensus_height: 0,
                    epoch_id: 0,
                    validator_set_hash: String::new(),
                    stake_root: String::new(),
                    threshold_bps: 0,
                    min_unique_signers: 0,
                    threshold: 0,
                    signatures: Default::default(),
                },
            }
        } else {
            install(module)
        };
        assert!(dispatch(&mut world, action).unwrap());
        let expected = if explicit {
            "proposal_id mismatch"
        } else {
            "local finality path is disabled"
        };
        assert!(
            matches!(world.journal().events.last().map(|event| &event.body),
                Some(WorldEventBody::Domain(DomainEvent::ActionRejected { reason: crate::RejectReason::RuleDenied { notes }, .. }))
                if notes.iter().any(|note| note.contains(expected))
            )
        );
        assert_eq!(world.state, before.state);
        assert_eq!(world.manifest, before.manifest);
        assert_eq!(world.module_registry, before.module_registry);
    }
}

#[test]
fn rollback_reducer_rejections_preserve_priority_and_all_observables() {
    for case in [
        "negative",
        "payer",
        "insufficient",
        "missing",
        "owner",
        "module",
        "version",
    ] {
        let (mut world, _) = fixture(2);
        let mut event = DomainEvent::ModuleRollbackApplied {
            operator_agent_id: "payer".into(),
            instance_id: "m.governed-atomic#1".into(),
            module_id: "m.governed-atomic".into(),
            from_module_version: "2.0.0".into(),
            to_module_version: "1.0.0".into(),
            wasm_hash: "old-hash".into(),
            from_wasm_hash: None,
            from_release_request_id: None,
            install_target: oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent,
            active: true,
            proposal_id: 0,
            manifest_hash: "not-used-by-raw-reducer".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 2,
        };
        if case == "negative" {
            let encoded = serde_json::to_value(&event).unwrap();
            assert!(encoded["data"].get("from_wasm_hash").is_none());
            assert!(encoded["data"].get("from_release_request_id").is_none());
            let decoded = serde_json::from_value::<DomainEvent>(encoded).unwrap();
            assert_eq!(decoded, event);
        }
        if let DomainEvent::ModuleRollbackApplied {
            operator_agent_id,
            instance_id,
            module_id,
            from_module_version,
            fee_amount,
            ..
        } = &mut event
        {
            match case {
                "negative" => {
                    *fee_amount = -1;
                    *operator_agent_id = "missing".into();
                    *instance_id = "missing".into();
                }
                "payer" => {
                    *operator_agent_id = "missing".into();
                    *instance_id = "missing".into();
                }
                "insufficient" => {
                    *fee_amount = i64::MAX;
                    *instance_id = "missing".into();
                }
                "missing" => *instance_id = "missing".into(),
                "owner" => {
                    world
                        .state
                        .module_instances
                        .get_mut(instance_id)
                        .unwrap()
                        .owner_agent_id = "other".into();
                    *module_id = "wrong".into();
                    *from_module_version = "wrong".into();
                }
                "module" => {
                    *module_id = "wrong".into();
                    *from_module_version = "wrong".into();
                }
                _ => *from_module_version = "wrong".into(),
            }
        }
        let before = world.clone();
        let error = world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap_err();
        if case == "payer" {
            assert!(
                matches!(error, WorldError::AgentNotFound { agent_id } if agent_id == "missing")
            );
        } else {
            let expected = match case {
                "negative" => "module action fee must be >= 0",
                "insufficient" => "module action fee debit failed",
                "missing" => "module instance missing for rollback",
                "owner" => "module instance owner mismatch for rollback",
                "module" => "module instance module_id mismatch for rollback",
                _ => "module instance from_version mismatch for rollback",
            };
            assert!(
                matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains(expected))
            );
        }
        assert_eq!(world.snapshot(), before.snapshot());
        assert_eq!(world.journal(), before.journal());
        assert_eq!(world.module_cache, before.module_cache);
        assert_eq!(
            (world.next_event_id, world.next_event_id_era),
            (before.next_event_id, before.next_event_id_era)
        );
    }
}

#[test]
fn governed_lifecycle_retry_retains_old_prelude_and_commits_business_once() {
    for level in 0..3 {
        let (mut world, module) = fixture(level);
        let baseline = world.snapshot();
        let old_proposal = world.next_proposal_id;
        let counter = world.state.next_module_instance_id;
        let balance = world
            .agent_resource_balance("payer", ResourceKind::Electricity)
            .unwrap();
        let mailbox_len = world.state.agents["payer"].mailbox.len();
        let action = action_for_level(level, module);
        world.fail_append_after_publication_prepare_on_nth_for_test(5);
        assert!(dispatch(&mut world, action.clone()).is_err());
        let retained_proposal = world.proposals[&old_proposal].clone();
        let retry_proposal = world.next_proposal_id;
        assert_ne!(retry_proposal, old_proposal);
        assert!(dispatch(&mut world, action).unwrap());
        assert_eq!(world.proposals[&old_proposal], retained_proposal);
        assert!(matches!(
            world.proposals[&retry_proposal].status,
            crate::ProposalStatus::Applied { .. }
        ));
        let events = &world.journal().events[baseline.journal_len..];
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event.body,
                    WorldEventBody::Governance(GovernanceEvent::Applied { .. })
                ))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.body, WorldEventBody::ManifestUpdated(_)))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event.body,
                    WorldEventBody::Domain(
                        DomainEvent::ModuleInstalled { .. }
                            | DomainEvent::ModuleUpgraded { .. }
                            | DomainEvent::ModuleRollbackApplied { .. }
                    )
                ))
                .count(),
            1
        );
        assert_eq!(
            world
                .agent_resource_balance("payer", ResourceKind::Electricity)
                .unwrap(),
            balance - 2
        );
        assert_eq!(world.state.agents["payer"].mailbox.len(), mailbox_len + 1);
        assert_eq!(
            world.state.next_module_instance_id,
            counter + u64::from(level == 0)
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
        let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(replayed.state, world.state);
        assert_eq!(replayed.manifest, world.manifest);
        assert_eq!(replayed.module_registry, world.module_registry);
        assert_eq!(replayed.module_tick_schedule, world.module_tick_schedule);
        assert_eq!(replayed.proposals, world.proposals);
        assert_eq!(
            replayed.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}
