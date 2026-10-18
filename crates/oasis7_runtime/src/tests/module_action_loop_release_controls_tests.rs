use super::*;

#[test]
fn module_release_shadow_rejects_missing_artifact_identity() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-missing-identity".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let mut manifest = base_manifest("m.loop.release.missing-identity", "0.1.0", &wasm_hash);
    manifest.artifact_identity = None;
    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest,
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

    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("shadow module release request missing identity");
    assert_rule_denied_note_for_action(&world, action_id, "artifact_identity is required");
}

#[test]
fn module_release_shadow_rejects_unsigned_artifact_identity_signature() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-unsigned-identity".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    let mut manifest = base_manifest("m.loop.release.unsigned-identity", "0.1.0", &wasm_hash);
    let mut identity = manifest
        .artifact_identity
        .clone()
        .expect("base manifest identity");
    identity.artifact_signature = "unsigned:tampered".to_string();
    manifest.artifact_identity = Some(identity);

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest,
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

    let action_id = world.submit_action(Action::ModuleReleaseShadow {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("shadow module release request unsigned identity");
    assert_rule_denied_note_for_action(&world, action_id, "unsigned signature is forbidden");
}

#[test]
fn module_release_apply_rejects_when_required_roles_are_missing() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-missing-roles".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.missing-role", "0.1.0", &wasm_hash),
        activate: true,
        install_target: ModuleInstallTarget::SelfAgent,
        required_roles: vec!["security".to_string(), "runtime".to_string()],
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
    world.step().expect("approve one required role");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::PartiallyApproved)
    ));

    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("apply module release request with missing roles");

    assert_rule_denied_note_for_action(&world, action_id, "required roles are not fully approved");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::PartiallyApproved)
    ));
}

#[test]
fn module_release_duplicate_role_approval_is_idempotent_for_same_approver() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");

    let wasm_bytes = b"module-release-duplicate-role-approval".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.dup-role", "0.1.0", &wasm_hash),
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
        operator_agent_id: "publisher-1".to_string(),
        request_id,
    });
    world.step().expect("shadow module release request");
    bind_release_roles(&mut world, "publisher-1", "publisher-1", &["security"]);

    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "publisher-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve role first time");

    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "publisher-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve role second time");

    let request = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state");
    assert_eq!(request.status, ModuleReleaseRequestStatus::Approved);
    assert_eq!(request.role_approvals.len(), 1);
    assert_eq!(
        request.role_approvals.get("security"),
        Some(&"publisher-1".to_string())
    );
}

#[test]
fn module_release_approve_role_rejects_when_role_not_required() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-role-not-required".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.role-not-required", "0.1.0", &wasm_hash),
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
    bind_release_roles(&mut world, "operator-1", "operator-1", &["runtime"]);

    let action_id = world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "runtime".to_string(),
    });
    world.step().expect("reject role not required");
    assert_rule_denied_note_for_action(&world, action_id, "role not required");
}

#[test]
fn module_release_approve_role_rejects_when_role_already_approved_by_other() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    register_agent(&mut world, "operator-2");

    let wasm_bytes = b"module-release-role-already-approved".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.role-already-approved", "0.1.0", &wasm_hash),
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
    bind_release_roles(&mut world, "operator-1", "operator-2", &["security"]);

    world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("approve required role");

    let action_id = world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-2".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world.step().expect("reject role already approved");
    assert_rule_denied_note_for_action(&world, action_id, "already approved");
}

#[test]
fn module_release_reject_moves_request_to_rejected() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-reject".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.rejected", "0.1.0", &wasm_hash),
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

    world.submit_action(Action::ModuleReleaseReject {
        rejector_agent_id: "operator-1".to_string(),
        request_id,
        reason: "policy violation".to_string(),
    });
    world.step().expect("reject module release request");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleReleaseRejected {
            request_id: event_request_id,
            rejector_agent_id,
            reason,
        })) if *event_request_id == request_id
            && rejector_agent_id == "operator-1"
            && reason == "policy violation"
    ));

    let request = world
        .state()
        .module_release_requests
        .get(&request_id)
        .expect("module release request state");
    assert_eq!(request.status, ModuleReleaseRequestStatus::Rejected);
    assert_eq!(request.rejected_reason.as_deref(), Some("policy violation"));
}

#[test]
fn module_release_approve_role_rejects_when_approver_role_binding_is_missing() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");

    let wasm_bytes = b"module-release-unbound-approver".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");

    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.unbound", "0.1.0", &wasm_hash),
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

    let action_id = world.submit_action(Action::ModuleReleaseApproveRole {
        approver_agent_id: "operator-1".to_string(),
        request_id,
        role: "security".to_string(),
    });
    world
        .step()
        .expect("reject approve role without role binding");
    assert_rule_denied_note_for_action(&world, action_id, "approver role binding missing");
}

#[test]
fn module_release_bind_roles_normalizes_and_updates_state() {
    let mut world = World::new();
    register_agent(&mut world, "operator-1");
    register_agent(&mut world, "auditor-1");

    world.submit_action(Action::ModuleReleaseBindRoles {
        operator_agent_id: "operator-1".to_string(),
        target_agent_id: "auditor-1".to_string(),
        roles: vec![
            "Security".to_string(),
            " runtime ".to_string(),
            "security".to_string(),
            "".to_string(),
        ],
    });
    world.step().expect("bind module release roles");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleReleaseRolesBound {
            operator_agent_id,
            target_agent_id,
            roles,
        })) if operator_agent_id == "operator-1"
            && target_agent_id == "auditor-1"
            && roles == &vec!["runtime".to_string(), "security".to_string()]
    ));
    let bound_roles = world
        .state()
        .module_release_role_bindings
        .get("auditor-1")
        .expect("bound roles");
    assert!(bound_roles.contains("security"));
    assert!(bound_roles.contains("runtime"));

    world.submit_action(Action::ModuleReleaseBindRoles {
        operator_agent_id: "operator-1".to_string(),
        target_agent_id: "auditor-1".to_string(),
        roles: Vec::new(),
    });
    world.step().expect("unbind module release roles");
    assert!(!world
        .state()
        .module_release_role_bindings
        .contains_key("auditor-1"));
}

#[test]
fn rollback_module_instance_reverts_to_historical_version_and_emits_audit() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_v1_bytes = b"module-rollback-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.rollback", "0.1.0", &wasm_v1_hash),
        activate: true,
    });
    world.step().expect("install v1");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let wasm_v2_bytes = b"module-rollback-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");

    world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: base_manifest("m.loop.rollback", "0.2.0", &wasm_v2_hash),
        activate: true,
    });
    world.step().expect("upgrade to v2");

    world.submit_action(Action::RollbackModuleInstance {
        operator_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        target_module_version: "0.1.0".to_string(),
    });
    world.step().expect("rollback to v1");

    let rollback_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleRollbackApplied {
                instance_id: event_instance_id,
                from_module_version,
                to_module_version,
                wasm_hash,
                proposal_id,
                ..
            }) if event_instance_id == &instance_id => Some((
                from_module_version.clone(),
                to_module_version.clone(),
                wasm_hash.clone(),
                *proposal_id,
            )),
            _ => None,
        })
        .expect("module rollback event");
    assert_eq!(rollback_event.0, "0.2.0");
    assert_eq!(rollback_event.1, "0.1.0");
    assert_eq!(rollback_event.2, wasm_v1_hash);
    assert!(rollback_event.3 > 0);

    let instance = world
        .state()
        .module_instances
        .get(&instance_id)
        .expect("instance state");
    assert_eq!(instance.module_version, "0.1.0");
    assert_eq!(instance.wasm_hash, rollback_event.2);
}

#[test]
fn rollback_module_instance_rejects_when_target_version_missing() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_bytes = b"module-rollback-missing-target".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.rollback.missing", "0.1.0", &wasm_hash),
        activate: true,
    });
    world.step().expect("install module");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let action_id = world.submit_action(Action::RollbackModuleInstance {
        operator_agent_id: "owner-1".to_string(),
        instance_id,
        target_module_version: "9.9.9".to_string(),
    });
    world.step().expect("reject missing rollback target");
    assert_rule_denied_note_for_action(&world, action_id, "target version not found");
}

#[test]
fn rollback_module_instance_rejects_when_operator_does_not_own_instance() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");
    register_agent(&mut world, "owner-2");

    let wasm_bytes = b"module-rollback-owner-mismatch".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.rollback.owner", "0.1.0", &wasm_hash),
        activate: true,
    });
    world.step().expect("install module");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let action_id = world.submit_action(Action::RollbackModuleInstance {
        operator_agent_id: "owner-2".to_string(),
        instance_id,
        target_module_version: "0.1.0".to_string(),
    });
    world.step().expect("reject owner mismatch rollback");
    assert_rule_denied_note_for_action(&world, action_id, "does not own instance");
}

#[test]
fn rollback_module_instance_rejects_when_target_interface_is_incompatible() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_v1_bytes = b"module-rollback-incompatible-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.rollback.incompatible", "0.1.0", &wasm_v1_hash),
        activate: true,
    });
    world.step().expect("install v1");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let wasm_v2_bytes = b"module-rollback-incompatible-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");

    let mut manifest_v2 = base_manifest("m.loop.rollback.incompatible", "0.2.0", &wasm_v2_hash);
    manifest_v2.exports.push("audit".to_string());
    world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: manifest_v2,
        activate: true,
    });
    world.step().expect("upgrade to v2");

    let action_id = world.submit_action(Action::RollbackModuleInstance {
        operator_agent_id: "owner-1".to_string(),
        instance_id,
        target_module_version: "0.1.0".to_string(),
    });
    world
        .step()
        .expect("reject incompatible rollback target interface");
    assert_rule_denied_note_for_action(&world, action_id, "exports incompatible");
}

#[test]
fn install_module_rejects_without_finality_in_production_policy() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_bytes = b"module-install-prod-no-finality".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.enable_production_release_policy();
    let action_id = world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.prod.install.reject", "0.1.0", &wasm_hash),
        activate: true,
    });
    world.step().expect("reject install without finality");
    assert_rule_denied_note_for_action(&world, action_id, "local finality path is disabled");
}

#[test]
fn upgrade_and_rollback_reject_without_finality_in_production_policy() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_v1_bytes = b"module-upgrade-rollback-prod-no-finality-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.prod.upgrade.reject", "0.1.0", &wasm_v1_hash),
        activate: true,
    });
    world.step().expect("install v1");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let wasm_v2_bytes = b"module-upgrade-rollback-prod-no-finality-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");
    world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: base_manifest("m.loop.prod.upgrade.reject", "0.2.0", &wasm_v2_hash),
        activate: true,
    });
    world.step().expect("upgrade to v2");

    let wasm_v3_bytes = b"module-upgrade-rollback-prod-no-finality-v3".to_vec();
    let wasm_v3_hash = util::sha256_hex(&wasm_v3_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v3_hash.clone(),
        wasm_bytes: wasm_v3_bytes,
    });
    world.step().expect("deploy v3 artifact");

    world.enable_production_release_policy();
    let upgrade_action_id = world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.2.0".to_string(),
        manifest: base_manifest("m.loop.prod.upgrade.reject", "0.3.0", &wasm_v3_hash),
        activate: true,
    });
    world.step().expect("reject upgrade without finality");
    assert_rule_denied_note_for_action(
        &world,
        upgrade_action_id,
        "local finality path is disabled",
    );

    let rollback_action_id = world.submit_action(Action::RollbackModuleInstance {
        operator_agent_id: "owner-1".to_string(),
        instance_id,
        target_module_version: "0.1.0".to_string(),
    });
    world.step().expect("reject rollback without finality");
    assert_rule_denied_note_for_action(
        &world,
        rollback_action_id,
        "local finality path is disabled",
    );
}

#[test]
fn install_upgrade_rollback_with_finality_succeed_in_production_policy() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");
    set_test_governance_finality_epoch_snapshot(
        &mut world,
        2,
        &[TEST_FINALITY_SIGNER_NODE_1, TEST_FINALITY_SIGNER_NODE_2],
    );

    let wasm_v1_bytes = b"module-with-finality-prod-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");

    world.enable_production_release_policy();
    let install_manifest = base_manifest("m.loop.prod.with-finality", "0.1.0", &wasm_v1_hash);
    let install_finality = derive_module_action_finality_certificate(&world, |simulated| {
        simulated.submit_action(Action::InstallModuleFromArtifact {
            installer_agent_id: "owner-1".to_string(),
            manifest: install_manifest.clone(),
            activate: true,
        });
    });
    world.submit_action(Action::InstallModuleFromArtifactWithFinality {
        installer_agent_id: "owner-1".to_string(),
        manifest: install_manifest,
        activate: true,
        finality_certificate: install_finality,
    });
    world.step().expect("install with finality");
    let instance_id = match &world.journal().events.last().expect("install event").body {
        WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) => {
            instance_id.clone()
        }
        other => panic!("expected module installed event: {other:?}"),
    };

    let wasm_v2_bytes = b"module-with-finality-prod-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");
    let upgrade_manifest = base_manifest("m.loop.prod.with-finality", "0.2.0", &wasm_v2_hash);
    let upgrade_finality = derive_module_action_finality_certificate(&world, |simulated| {
        simulated.submit_action(Action::UpgradeModuleFromArtifact {
            upgrader_agent_id: "owner-1".to_string(),
            instance_id: instance_id.clone(),
            from_module_version: "0.1.0".to_string(),
            manifest: upgrade_manifest.clone(),
            activate: true,
        });
    });
    world.submit_action(Action::UpgradeModuleFromArtifactWithFinality {
        upgrader_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: upgrade_manifest,
        activate: true,
        finality_certificate: upgrade_finality,
    });
    world.step().expect("upgrade with finality");

    let rollback_finality = derive_module_action_finality_certificate(&world, |simulated| {
        simulated.submit_action(Action::RollbackModuleInstance {
            operator_agent_id: "owner-1".to_string(),
            instance_id: instance_id.clone(),
            target_module_version: "0.1.0".to_string(),
        });
    });
    world.submit_action(Action::RollbackModuleInstanceWithFinality {
        operator_agent_id: "owner-1".to_string(),
        instance_id: instance_id.clone(),
        target_module_version: "0.1.0".to_string(),
        finality_certificate: rollback_finality,
    });
    world.step().expect("rollback with finality");

    let instance = world
        .state()
        .module_instances
        .get(&instance_id)
        .expect("module instance state");
    assert_eq!(instance.module_version, "0.1.0");
}

#[test]
fn module_release_apply_rejects_without_finality_in_production_policy() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    let request_id = prepare_module_release_apply_ready_request(
        &mut world,
        "publisher-1",
        "operator-1",
        "m.loop.release.prod.reject",
    );

    world.enable_production_release_policy();
    let action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world
        .step()
        .expect("reject module release apply without finality");
    assert_rule_denied_note_for_action(&world, action_id, "local finality path is disabled");
}

#[test]
fn module_release_apply_uses_stake_weighted_attestation_threshold_bps() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    bind_attestor_node_identity(&mut world, TEST_FINALITY_SIGNER_3);

    let wasm_bytes = b"module-release-weighted-threshold".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy module artifact");
    world.submit_action(Action::ModuleReleaseSubmit {
        requester_agent_id: "publisher-1".to_string(),
        manifest: base_manifest("m.loop.release.weighted.threshold", "0.1.0", &wasm_hash),
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
    set_module_release_attestation_epoch_snapshot_with_stakes(
        &mut world,
        2,
        &[
            LOCAL_FINALITY_SIGNER_1,
            LOCAL_FINALITY_SIGNER_2,
            TEST_FINALITY_SIGNER_3,
        ],
        &[
            (LOCAL_FINALITY_SIGNER_1, 70),
            (LOCAL_FINALITY_SIGNER_2, 20),
            (TEST_FINALITY_SIGNER_3, 10),
        ],
    );
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_2,
        "linux-x86_64",
        "bafyreleaseattestweightedmid",
    );
    world.step().expect("submit mid-stake attestation");
    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        TEST_FINALITY_SIGNER_3,
        "linux-x86_64",
        "bafyreleaseattestweightedlow",
    );
    world.step().expect("submit low-stake attestation");

    let low_stake_action_id = world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply with low stake attestations");
    assert_rule_denied_note_for_action(
        &world,
        low_stake_action_id,
        "aggregated_stake_bps=3000",
    );

    submit_test_module_release_attestation(
        &mut world,
        "operator-1",
        request_id,
        LOCAL_FINALITY_SIGNER_1,
        "linux-x86_64",
        "bafyreleaseattestweightedhigh",
    );
    world.step().expect("submit high-stake attestation");
    world.submit_action(Action::ModuleReleaseApply {
        operator_agent_id: "operator-1".to_string(),
        request_id,
    });
    world.step().expect("apply with high stake attestations");
    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Applied)
    ));
}

#[test]
fn module_release_apply_with_finality_succeeds_in_production_policy() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");
    register_agent(&mut world, "operator-1");
    let request_id = prepare_module_release_apply_ready_request(
        &mut world,
        "publisher-1",
        "operator-1",
        "m.loop.release.prod.with-finality",
    );

    world.enable_production_release_policy();
    let finality_certificate = derive_module_action_finality_certificate(&world, |simulated| {
        simulated.submit_action(Action::ModuleReleaseApply {
            operator_agent_id: "operator-1".to_string(),
            request_id,
        });
    });
    world.submit_action(Action::ModuleReleaseApplyWithFinality {
        operator_agent_id: "operator-1".to_string(),
        request_id,
        finality_certificate,
    });
    world
        .step()
        .expect("apply module release with finality in production");

    assert!(matches!(
        world
            .state()
            .module_release_requests
            .get(&request_id)
            .map(|item| item.status),
        Some(ModuleReleaseRequestStatus::Applied)
    ));
}
