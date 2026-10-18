use super::super::{Action, ActionEnvelope, DomainEvent, ModuleProfileChanges, WorldEventBody};
use super::{World, WorldError};
use crate::state::ModuleReleaseRequestStatus;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_wasm_abi::*;

fn dispatch(world: &mut World, action: Action) -> Result<bool, WorldError> {
    world.try_apply_runtime_module_action(&ActionEnvelope {
        committed_recipe_origin: None,
        id: 980,
        action,
    })
}

fn fixture() -> (World, u64) {
    let mut world = World::new();
    for id in ["publisher", "operator", "approver"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: id.into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().unwrap();
    }
    let bytes = b"release-review-publication";
    let hash = crate::util::sha256_hex(bytes);
    world.register_module_artifact(hash.clone(), bytes).unwrap();
    world
        .state
        .module_artifact_owners
        .insert(hash.clone(), "publisher".into());
    let manifest = ModuleManifest {
        module_id: "m.review-atomic".into(),
        name: "ReviewAtomic".into(),
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
    dispatch(
        &mut world,
        Action::ModuleReleaseSubmit {
            requester_agent_id: "publisher".into(),
            manifest,
            activate: true,
            install_target: ModuleInstallTarget::SelfAgent,
            required_roles: vec!["security".into(), "operations".into()],
            profile_changes: ModuleProfileChanges::default(),
        },
    )
    .unwrap();
    let id = *world.state.module_release_requests.keys().next().unwrap();
    (world, id)
}

fn shadow(id: u64) -> DomainEvent {
    DomainEvent::ModuleReleaseShadowed {
        request_id: id,
        operator_agent_id: "operator".into(),
        manifest_hash: "shadow-hash".into(),
    }
}

fn approval(id: u64) -> DomainEvent {
    DomainEvent::ModuleReleaseRoleApproved {
        request_id: id,
        approver_agent_id: "approver".into(),
        role: "security".into(),
    }
}

fn rejection(id: u64) -> DomainEvent {
    DomainEvent::ModuleReleaseRejected {
        request_id: id,
        rejector_agent_id: "operator".into(),
        reason: "review rejected".into(),
    }
}

fn assert_world_unchanged(world: &World, expected: &World) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
}

#[test]
fn direct_shadow_missing_mapping_preserves_request() {
    let (mut world, id) = fixture();
    world.state.module_release_manifest_mappings.remove(&id);
    let before = world.state.clone();
    let error = world.state.apply_domain_event(&shadow(id), 99).unwrap_err();
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("mapping"))
    );
    assert_eq!(world.state, before);
}

#[test]
fn raw_shadow_postprepare_failure_preserves_world() {
    assert_raw_postprepare_failure(shadow, ModuleReleaseRequestStatus::Requested);
}

#[test]
fn raw_role_approved_postprepare_failure_preserves_world() {
    assert_raw_postprepare_failure(approval, ModuleReleaseRequestStatus::Shadowed);
}

#[test]
fn raw_rejected_postprepare_failure_preserves_world() {
    assert_raw_postprepare_failure(rejection, ModuleReleaseRequestStatus::Requested);
}

fn assert_raw_postprepare_failure(
    event: impl FnOnce(u64) -> DomainEvent,
    status: ModuleReleaseRequestStatus,
) {
    let (mut world, id) = fixture();
    world
        .state
        .module_release_requests
        .get_mut(&id)
        .unwrap()
        .status = status;
    world.state.material_ledgers.clear();
    world
        .state
        .materials
        .insert("legacy-review-material".into(), 7);
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(WorldEventBody::Domain(event(id)), None)
        .expect_err("release review publication must honor the postprepare failpoint");
    assert!(format!("{error:?}").contains("injected"));
    assert_world_unchanged(&world, &before);
}

#[test]
fn live_shadow_missing_mapping_propagates_and_preserves_world() {
    let (mut world, id) = fixture();
    world.state.module_release_manifest_mappings.remove(&id);
    let before = world.clone();
    let error = dispatch(
        &mut world,
        Action::ModuleReleaseShadow {
            operator_agent_id: "operator".into(),
            request_id: id,
        },
    )
    .expect_err("valid action preparation with missing reducer mapping remains propagated");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("mapping"))
    );
    assert_world_unchanged(&world, &before);
}

fn set_status(world: &mut World, id: u64, status: ModuleReleaseRequestStatus) {
    world
        .state
        .module_release_requests
        .get_mut(&id)
        .unwrap()
        .status = status;
    if let Some(mapping) = world.state.module_release_manifest_mappings.get_mut(&id) {
        mapping.status = status;
    }
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
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn all_review_events_publish_success_with_optional_mapping_and_replay() {
    for kind in 0..3 {
        let (mut world, id) = fixture();
        if kind == 1 {
            set_status(&mut world, id, ModuleReleaseRequestStatus::Shadowed);
        }
        if kind > 0 {
            world.state.module_release_manifest_mappings.remove(&id);
        }
        world
            .state
            .materials
            .insert("legacy-review-success".into(), 9);
        world.state.material_ledgers.clear();
        let baseline = world.snapshot();
        let event = match kind {
            0 => shadow(id),
            1 => approval(id),
            _ => rejection(id),
        };
        world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap();
        let request = &world.state.module_release_requests[&id];
        assert_eq!(
            request.status,
            match kind {
                0 => ModuleReleaseRequestStatus::Shadowed,
                1 => ModuleReleaseRequestStatus::PartiallyApproved,
                _ => ModuleReleaseRequestStatus::Rejected,
            }
        );
        assert_eq!(world.state.materials["legacy-review-success"], 9);
        assert_eq!(
            world.state.material_ledgers[&crate::MaterialLedgerId::world()]["legacy-review-success"],
            9
        );
        assert_root_and_replay(&world, baseline);
    }
}

#[test]
fn approval_normalizes_role_and_duplicate_same_approver_remains_successful() {
    let (mut world, id) = fixture();
    set_status(&mut world, id, ModuleReleaseRequestStatus::Shadowed);
    let baseline = world.snapshot();
    let mut event = approval(id);
    if let DomainEvent::ModuleReleaseRoleApproved { role, .. } = &mut event {
        *role = "  SeCuRiTy ".into();
    }
    world
        .append_event(WorldEventBody::Domain(event), None)
        .unwrap();
    world
        .append_event(WorldEventBody::Domain(approval(id)), None)
        .unwrap();
    assert_eq!(
        world.state.module_release_requests[&id].role_approvals["security"],
        "approver"
    );
    assert_eq!(
        world.state.module_release_requests[&id].status,
        ModuleReleaseRequestStatus::PartiallyApproved
    );
    assert_root_and_replay(&world, baseline);

    let before = world.state.clone();
    let mut different = approval(id);
    if let DomainEvent::ModuleReleaseRoleApproved {
        approver_agent_id, ..
    } = &mut different
    {
        *approver_agent_id = "operator".into();
    }
    assert!(
        format!(
            "{:?}",
            world.state.apply_domain_event(&different, 99).unwrap_err()
        )
        .contains("approver mismatch")
    );
    assert_eq!(world.state, before);
}

#[test]
fn raw_missing_review_actors_are_accepted_but_actions_are_caught() {
    for kind in 0..3 {
        let (mut world, id) = fixture();
        if kind == 1 {
            set_status(&mut world, id, ModuleReleaseRequestStatus::Shadowed);
        }
        let mut event = match kind {
            0 => shadow(id),
            1 => approval(id),
            _ => rejection(id),
        };
        match &mut event {
            DomainEvent::ModuleReleaseShadowed {
                operator_agent_id, ..
            } => *operator_agent_id = "missing".into(),
            DomainEvent::ModuleReleaseRoleApproved {
                approver_agent_id, ..
            } => *approver_agent_id = "missing".into(),
            DomainEvent::ModuleReleaseRejected {
                rejector_agent_id, ..
            } => *rejector_agent_id = "missing".into(),
            _ => unreachable!(),
        }
        world
            .append_event(WorldEventBody::Domain(event), None)
            .unwrap();
        assert!(!world.state.agents.contains_key("missing"));
    }
    for action in [
        Action::ModuleReleaseShadow {
            operator_agent_id: "missing".into(),
            request_id: 1,
        },
        Action::ModuleReleaseApproveRole {
            approver_agent_id: "missing".into(),
            request_id: 1,
            role: "security".into(),
        },
        Action::ModuleReleaseReject {
            rejector_agent_id: "missing".into(),
            request_id: 1,
            reason: "why".into(),
        },
    ] {
        let (mut world, _) = fixture();
        assert!(dispatch(&mut world, action).unwrap());
        assert!(matches!(
            world.journal().events.last().unwrap().body,
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
        ));
    }
}

#[test]
fn rejection_action_trims_reason_while_raw_event_preserves_it() {
    let (mut action_world, id) = fixture();
    dispatch(
        &mut action_world,
        Action::ModuleReleaseReject {
            rejector_agent_id: "operator".into(),
            request_id: id,
            reason: "  policy  ".into(),
        },
    )
    .unwrap();
    assert_eq!(
        action_world.state.module_release_requests[&id]
            .rejected_reason
            .as_deref(),
        Some("policy")
    );
    let (mut raw_world, id) = fixture();
    let mut event = rejection(id);
    if let DomainEvent::ModuleReleaseRejected { reason, .. } = &mut event {
        *reason = "  policy  ".into();
    }
    raw_world
        .append_event(WorldEventBody::Domain(event), None)
        .unwrap();
    assert_eq!(
        raw_world.state.module_release_requests[&id]
            .rejected_reason
            .as_deref(),
        Some("  policy  ")
    );
}

#[test]
fn review_retry_and_retention_era_publish_exactly_once() {
    let (mut world, id) = fixture();
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(
        world
            .append_event(WorldEventBody::Domain(shadow(id)), None)
            .is_err()
    );
    assert_world_unchanged(&world, &before);
    world.runtime_memory_limits.max_journal_events = 2;
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 7;
    world
        .append_event(WorldEventBody::Domain(shadow(id)), None)
        .unwrap();
    assert_eq!(world.next_event_id_era, 8);
    assert_eq!(world.journal().events.len(), 2);
    assert_eq!(
        world.state.module_release_requests[&id].status,
        ModuleReleaseRequestStatus::Shadowed
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
