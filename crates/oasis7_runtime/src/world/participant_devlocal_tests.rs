use super::*;
#[test]
fn devlocal_context_requires_explicit_safe_profile_and_strong_pin() {
    let public = crate::service_admission::sign_read_request("test", (), &hex::encode([77; 32]))
        .unwrap()
        .subject_public_key;
    assert!(
        DevLocalParticipantAuthority::new(
            oasis7_proto::storage_profile::StorageProfile::DevLocal,
            &ReleaseSecurityPolicy::default(),
            public.clone()
        )
        .is_ok()
    );
    assert!(
        DevLocalParticipantAuthority::new(
            oasis7_proto::storage_profile::StorageProfile::ReleaseDefault,
            &ReleaseSecurityPolicy::default(),
            public.clone()
        )
        .is_err()
    );
    assert!(
        DevLocalParticipantAuthority::new(
            oasis7_proto::storage_profile::StorageProfile::DevLocal,
            &ReleaseSecurityPolicy::production_hardened(),
            public
        )
        .is_err()
    );
}
use crate::service_admission::{SignedReadRequest, participant_policy::*, sign_read_request};
fn authority(key: u8) -> DevLocalParticipantAuthority {
    DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &ReleaseSecurityPolicy::default(),
        sign_read_request("test", (), &hex::encode([key; 32]))
            .unwrap()
            .subject_public_key,
    )
    .unwrap()
}
fn authorization(
    world: &World,
    policy: &crate::service_admission::participant::ParticipantEntryPolicyV1,
) -> SignedReadRequest<DevLocalParticipantPolicyAuthorizationV1> {
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: policy_digest(policy).unwrap(),
        world_id: policy.world_id.clone(),
        genesis_digest: policy.genesis_digest.clone(),
        branch_id: "main".into(),
        reorg_epoch: 0,
        identity_namespace: policy.identity_namespace.clone(),
        source_snapshot_epoch: 1,
        source_watermark: "explicit-complete-empty".into(),
        source_snapshot_digest: format!("blake3:{}", "a".repeat(64)),
        complete: true,
        previous_registry_digest: world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest: world.participant_legacy_owner_state_digest().unwrap(),
        entries: Vec::new(),
    };
    let request = DevLocalParticipantPolicyAuthorizationV1 {
        policy: policy.clone(),
        manifest: sign_read_request(
            PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
            manifest,
            &hex::encode([71; 32]),
        )
        .unwrap(),
    };
    sign_read_request(
        devlocal_policy_signing_domain(policy.generation),
        request,
        &hex::encode([77; 32]),
    )
    .unwrap()
}
#[test]
fn devlocal_no_governance_install_register_idempotence_and_explicit_restore() {
    let mut world = super::participant_tests::fixture()
        .with_devlocal_participant_authority(authority(77))
        .unwrap();
    assert!(world.proposals.is_empty());
    assert!(world.governance_finality_epoch_snapshots.is_empty());
    let checkpoint = world.snapshot();
    let policy = super::participant_tests::policy();
    let signed = authorization(&world, &policy);
    world
        .install_devlocal_participant_policy(policy.clone(), signed.clone())
        .unwrap();
    let installed = world.snapshot();
    world
        .install_devlocal_participant_policy(policy.clone(), signed)
        .unwrap();
    assert_eq!(installed, world.snapshot());
    assert_eq!(
        world.participant_policy_view()["authority_source"],
        "dev_local_operator"
    );
    let (owner, admission) = super::participant_tests::request("account", "op");
    world
        .apply_participant_registration(&owner, &admission, "main", 0)
        .unwrap();
    assert!(World::from_snapshot(world.snapshot(), world.journal().clone()).is_err());
    assert!(World::from_snapshot(checkpoint.clone(), world.journal().clone()).is_err());
    assert!(
        World::from_snapshot_with_release_security_policy(
            world.snapshot(),
            world.journal().clone(),
            ReleaseSecurityPolicy::production_hardened()
        )
        .is_err()
    );
    assert!(
        World::from_snapshot_with_devlocal_participant_authority(
            world.snapshot(),
            world.journal().clone(),
            authority(78)
        )
        .is_err()
    );
    for snapshot in [world.snapshot(), checkpoint] {
        let restored = World::from_snapshot_with_devlocal_participant_authority(
            snapshot,
            world.journal().clone(),
            authority(77),
        )
        .unwrap();
        assert_eq!(
            restored.capability_revocation_state,
            world.capability_revocation_state
        );
    }
    assert!(world.proposals.is_empty());
    assert!(world.governance_finality_epoch_snapshots.is_empty());
}
#[test]
fn devlocal_bad_pin_signature_manifest_predecessor_and_hardened_world_are_atomic() {
    let mut world = super::participant_tests::fixture()
        .with_devlocal_participant_authority(authority(77))
        .unwrap();
    let policy = super::participant_tests::policy();
    let valid = authorization(&world, &policy);
    let before = world.snapshot();
    let mut wrong = valid.clone();
    wrong.signature_hex = "00".repeat(64);
    assert!(
        world
            .install_devlocal_participant_policy(policy.clone(), wrong)
            .is_err()
    );
    assert_eq!(before, world.snapshot());
    let mut wrong = valid.clone();
    wrong.request.manifest.request.previous_registry_digest = "different".into();
    wrong.request.manifest = sign_read_request(
        PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
        wrong.request.manifest.request,
        &hex::encode([71; 32]),
    )
    .unwrap();
    wrong = sign_read_request(
        devlocal_policy_signing_domain(policy.generation),
        wrong.request,
        &hex::encode([77; 32]),
    )
    .unwrap();
    assert!(
        world
            .install_devlocal_participant_policy(policy.clone(), wrong)
            .is_err()
    );
    assert_eq!(before, world.snapshot());
    world.set_release_security_policy(ReleaseSecurityPolicy::production_hardened());
    assert!(
        world
            .install_devlocal_participant_policy(policy, valid)
            .is_err()
    );
}
#[test]
fn devlocal_closed_epoch_update_preserves_original_and_governed_cannot_mix() {
    let mut world = super::participant_tests::fixture()
        .with_devlocal_participant_authority(authority(77))
        .unwrap();
    let policy = super::participant_tests::policy();
    let signed = authorization(&world, &policy);
    world
        .install_devlocal_participant_policy(policy.clone(), signed)
        .unwrap();
    let (owner, admission) = super::participant_tests::request("account", "op");
    world
        .apply_participant_registration(&owner, &admission, "main", 0)
        .unwrap();
    let mut closed = policy;
    closed.generation = 2;
    closed.enabled = false;
    closed.identity_namespace.instance_epoch.0 = 2;
    let signed = authorization(&world, &closed);
    world
        .install_devlocal_participant_policy(closed, signed)
        .unwrap();
    assert!(
        crate::service_admission::participant::namespace_was_authorized(
            &world,
            &owner.request.identity_namespace
        )
    );
    assert_eq!(
        world
            .capability_revocation_state
            .participants
            .bindings
            .len(),
        1
    );
    World::from_snapshot_with_devlocal_participant_authority(
        world.snapshot(),
        world.journal().clone(),
        authority(77),
    )
    .unwrap();
}
#[test]
fn devlocal_issuer_owner_migration_preserves_assets_and_load_requires_pin() {
    use crate::Action;
    let mut world = super::participant_tests::fixture()
        .with_devlocal_participant_authority(authority(77))
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "legacy".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    let public = sign_read_request("test", (), &hex::encode([72; 32]))
        .unwrap()
        .subject_public_key;
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "legacy".into(),
        player_id: "legacy-owner".into(),
        public_key: Some(public.clone()),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("legacy", "legacy-owner", 1)
        .unwrap();
    assert!(world.current_agent_owner("legacy").is_some());
    let before = world.state().clone();
    let policy = super::participant_tests::policy();
    let mut signed = authorization(&world, &policy);
    let entry = ParticipantMigrationBindingV1 {
        registration: oasis7_agent_api::game::GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: "import".into(),
            identity_namespace: policy.identity_namespace.clone(),
            account_id: "legacy-account".into(),
            world_id: policy.world_id.clone(),
            genesis_digest: policy.genesis_digest.clone(),
            owner_public_key: public,
            owner_key_version: oasis7_agent_api::game::DecimalU64(1),
        },
        agent_id: "legacy".into(),
        owner_binding: "legacy-owner".into(),
        owner_generation: 1,
    };
    signed.request.manifest.request.entries = vec![
        sign_read_request(
            PARTICIPANT_MIGRATION_OWNER_DOMAIN,
            entry,
            &hex::encode([72; 32]),
        )
        .unwrap(),
    ];
    signed.request.manifest = sign_read_request(
        PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
        signed.request.manifest.request,
        &hex::encode([71; 32]),
    )
    .unwrap();
    signed = sign_read_request(
        devlocal_policy_signing_domain(policy.generation),
        signed.request,
        &hex::encode([77; 32]),
    )
    .unwrap();
    let mut bad = signed.clone();
    bad.request.manifest.request.entries[0].signature_hex = "00".repeat(64);
    bad.request.manifest = sign_read_request(
        PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
        bad.request.manifest.request,
        &hex::encode([71; 32]),
    )
    .unwrap();
    bad = sign_read_request(
        devlocal_policy_signing_domain(policy.generation),
        bad.request,
        &hex::encode([77; 32]),
    )
    .unwrap();
    assert!(
        world
            .install_devlocal_participant_policy(policy.clone(), bad)
            .is_err()
    );
    assert_eq!(world.state(), &before);
    world
        .install_devlocal_participant_policy(policy, signed)
        .unwrap();
    assert_eq!(world.state(), &before);
    assert_eq!(
        world
            .capability_revocation_state
            .participants
            .bindings
            .values()
            .next()
            .unwrap()
            .agent_id,
        "legacy"
    );
    let path = std::env::temp_dir().join(format!(
        "devlocal-participant-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    world.save_to_dir(&path).unwrap();
    assert!(World::load_from_dir(&path).is_err());
    assert!(
        World::load_from_dir_with_release_security_policy(
            &path,
            ReleaseSecurityPolicy::production_hardened()
        )
        .is_err()
    );
    let restored =
        World::load_from_dir_with_devlocal_participant_authority(&path, authority(77)).unwrap();
    assert_eq!(
        restored.capability_revocation_state.participants,
        world.capability_revocation_state.participants
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn devlocal_trusted_authority_setters_reject_before_mutation() {
    let pin = authority(77).operator_public_key().to_string();
    let registry = crate::GovernanceFinalitySignerRegistry {
        slot_id: "local-test".into(),
        threshold: 1,
        signer_bindings: std::collections::BTreeMap::from([("node-local".into(), pin.clone())]),
        ..Default::default()
    };
    let rollback = crate::RollbackAuthorityRegistry::new([
        crate::RollbackAuthorityRecord {
            authority_id: "operator".into(),
            role: crate::RollbackAuthorityRole::OnCall,
            public_key_hex: pin,
            active: true,
        },
        crate::RollbackAuthorityRecord {
            authority_id: "governance".into(),
            role: crate::RollbackAuthorityRole::Governance,
            public_key_hex: authority(78).operator_public_key().to_string(),
            active: true,
        },
    ])
    .unwrap();
    let mut governed = World::new();
    governed
        .set_governance_finality_signer_registry(registry.clone())
        .unwrap();
    governed
        .set_rollback_authority_registry(rollback.clone())
        .unwrap();
    let mut local = World::new()
        .with_devlocal_participant_authority(authority(77))
        .unwrap();
    let before = local.snapshot();
    assert!(
        local
            .set_governance_finality_signer_registry(registry)
            .is_err()
    );
    assert!(local.set_rollback_authority_registry(rollback).is_err());
    assert_eq!(local.snapshot(), before);
    local
        .validate_devlocal_participant_authority_context()
        .unwrap();
}
