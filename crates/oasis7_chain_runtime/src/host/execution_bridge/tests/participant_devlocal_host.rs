//! Genuine candidate commit and restart, without governance fixtures.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::ReleaseSecurityPolicy;
use oasis7_runtime::service_admission::participant::ParticipantEntryPolicyV1;
use oasis7_runtime::service_admission::participant_policy::*;

fn config(
    world: &RuntimeWorld,
    identity: &WorldIdentity,
) -> crate::execution_config::DevLocalParticipantConfig {
    let issuer = hex::encode([71; 32]);
    let operator = hex::encode([72; 32]);
    let pin = sign_read_request("fixture", (), &operator)
        .unwrap()
        .subject_public_key;
    let authority = oasis7_runtime::DevLocalParticipantAuthority::new(
        oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &ReleaseSecurityPolicy::default(),
        pin,
    )
    .unwrap();
    let namespace = GameIdentityNamespaceV1 {
        service_id: "private-fixture".into(),
        environment_id: "integration".into(),
        instance_epoch: DecimalU64(1),
    };
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        identity_namespace: namespace.clone(),
        issuer_public_key: sign_read_request("fixture", (), &issuer)
            .unwrap()
            .subject_public_key,
        generation: 1,
        max_grant_ticks: 256,
        max_participants: 8,
        registry_complete: true,
        enabled: true,
    };
    let binding = world.current_cognition_runtime_binding().unwrap();
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: policy_digest(&policy).unwrap(),
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        branch_id: binding.branch_id,
        reorg_epoch: binding.reorg_epoch,
        identity_namespace: namespace,
        source_snapshot_epoch: 1,
        source_watermark: "host-fixture".into(),
        source_snapshot_digest: format!("blake3:{}", "a".repeat(64)),
        complete: true,
        previous_registry_digest: world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest: world.participant_legacy_owner_state_digest().unwrap(),
        entries: Vec::new(),
    };
    let manifest =
        sign_read_request(PARTICIPANT_MIGRATION_MANIFEST_DOMAIN, manifest, &issuer).unwrap();
    let startup = sign_read_request(
        "participant_devlocal_policy_install_v1",
        DevLocalParticipantPolicyAuthorizationV1 { policy, manifest },
        &operator,
    )
    .unwrap();
    crate::execution_config::DevLocalParticipantConfig {
        authority: Some(authority),
        startup: Some(startup),
        ..crate::execution_config::DevLocalParticipantConfig::default()
    }
}
fn commit(driver: &mut NodeRuntimeExecutionDriver) -> Result<(), String> {
    commit_expected(driver, false)
}
fn commit_expected(driver: &mut NodeRuntimeExecutionDriver, mismatch: bool) -> Result<(), String> {
    let height = driver.state.last_applied_committed_height + 1;
    driver
        .on_commit_with_expected(
            NodeExecutionCommitContext {
                world_id: "w1".into(),
                node_id: "node-a".into(),
                proposer_id: "node-a".into(),
                height,
                slot: height,
                epoch: 0,
                node_block_hash: format!("devlocal-policy-{height}"),
                action_root: compute_consensus_action_root(&[]).unwrap(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: height as i64 * 1000,
            },
            mismatch.then_some("wrong-peer-block"),
            mismatch.then_some("wrong-peer-state"),
        )
        .map(|_| ())
}
#[test]
fn devlocal_host_signed_policy_candidate_rolls_back_then_commits_and_restores() {
    let (dir, mut driver, identity, _, _) = canonical_agent_chat::driver_fixture_without_provider();
    let control = config(&driver.execution_world, &identity);
    driver.execution_world = driver
        .execution_world
        .clone()
        .with_devlocal_participant_authority(control.authority.clone().unwrap())
        .unwrap();
    driver.devlocal_participant = control.clone();
    let before_agents = driver.execution_world.state().agents.clone();
    let before_claims = driver.execution_world.state().starter_oc_claims.clone();
    let before_registry = driver
        .execution_world
        .participant_registry_digest()
        .unwrap();
    let mut bad = control.clone();
    bad.startup.as_mut().unwrap().signature_hex = "00".repeat(64);
    driver.devlocal_participant = bad;
    assert!(commit(&mut driver).is_err());
    assert_eq!(
        driver
            .execution_world
            .participant_registry_digest()
            .unwrap(),
        before_registry
    );
    driver.devlocal_participant = control.clone();
    let disk_snapshot = fs::read(driver.world_dir.join("snapshot.json")).unwrap();
    let disk_journal = fs::read(driver.world_dir.join("journal.json")).unwrap();
    let disk_head = fs::read(driver.records_dir.join("latest.json")).ok();
    let previous_height = driver.state.last_applied_committed_height;
    // Expected-hash validation occurs after installation, stepping and CAS
    // materialization, but before the authoritative record/head publication.
    assert!(
        commit_expected(&mut driver, true)
            .unwrap_err()
            .contains("peer mismatch")
    );
    assert_eq!(
        driver
            .execution_world
            .participant_registry_digest()
            .unwrap(),
        before_registry
    );
    assert_eq!(driver.state.last_applied_committed_height, previous_height);
    assert!(driver.devlocal_participant.startup.is_some());
    assert_eq!(
        fs::read(driver.world_dir.join("snapshot.json")).unwrap(),
        disk_snapshot
    );
    assert_eq!(
        fs::read(driver.world_dir.join("journal.json")).unwrap(),
        disk_journal
    );
    assert_eq!(
        fs::read(driver.records_dir.join("latest.json")).ok(),
        disk_head
    );
    assert!(
        !super::super::super::checkpoint::execution_bridge_record_path(
            &driver.records_dir,
            previous_height + 1,
        )
        .exists()
    );
    commit(&mut driver).unwrap();
    let registry = driver
        .execution_world
        .participant_registry_digest()
        .unwrap();
    assert_ne!(registry, before_registry);
    assert_eq!(driver.execution_world.state().agents, before_agents);
    assert_eq!(
        driver.execution_world.state().starter_oc_claims,
        before_claims
    );
    assert!(driver.devlocal_participant.startup.is_none());
    let state = driver.state_path.clone();
    let world = driver.world_dir.clone();
    let records = driver.records_dir.clone();
    let store = dir.join("store");
    drop(driver);
    let mut recovered = NodeRuntimeExecutionDriver::new_with_participant_control(
        state.clone(),
        world.clone(),
        records.clone(),
        store.clone(),
        &StorageProfileConfig::default(),
        control.clone(),
    )
    .unwrap();
    assert_eq!(
        recovered
            .execution_world
            .participant_registry_digest()
            .unwrap(),
        registry
    );
    commit(&mut recovered).unwrap();
    assert!(recovered.devlocal_participant.startup.is_none());
    assert_eq!(
        recovered
            .execution_world
            .chain_resource_manifest()
            .genesis_ref
            .as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    drop(recovered);
    // The signed startup file may be removed after commit; retain the pin.
    let mut restore_only = control;
    restore_only.startup = None;
    let twice = NodeRuntimeExecutionDriver::new_with_participant_control(
        state,
        world,
        records.clone(),
        store.clone(),
        &StorageProfileConfig::default(),
        restore_only.clone(),
    )
    .unwrap();
    assert_eq!(
        twice.execution_world.participant_registry_digest().unwrap(),
        registry
    );
    let context = super::super::super::world_service_read::RestoreContext::new(
        ReleaseSecurityPolicy::default(),
        restore_only.authority,
    )
    .unwrap();
    let pinned = super::super::super::world_service_read::pin_with_context(
        &records, &store, &identity, None, &context,
    )
    .unwrap();
    assert_eq!(
        pinned.world.participant_registry_digest().unwrap(),
        registry
    );
    assert_eq!(
        pinned
            .world
            .chain_resource_manifest()
            .genesis_ref
            .as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn devlocal_host_rejects_effective_policy_and_profile_mismatch_before_disk_access() {
    let (dir, driver, identity, _, _) = canonical_agent_chat::driver_fixture_without_provider();
    let control = config(&driver.execution_world, &identity);
    let missing = dir.join("must-not-be-created");
    let error = super::super::super::driver_persistence::load_execution_world_with_context(
        &missing,
        ReleaseSecurityPolicy::production_hardened(),
        control.authority.as_ref(),
    )
    .err()
    .unwrap();
    assert!(error.contains("context mismatch"));
    for profile in [
        oasis7_proto::storage_profile::StorageProfile::ReleaseDefault,
        oasis7_proto::storage_profile::StorageProfile::SoakForensics,
    ] {
        let error = NodeRuntimeExecutionDriver::new_with_participant_control(
            missing.join("state"),
            missing.join("world"),
            missing.join("records"),
            missing.join("store"),
            &StorageProfileConfig::for_profile(profile),
            control.clone(),
        )
        .err()
        .unwrap();
        assert!(error.contains("actual DevLocal"));
    }
    assert!(!missing.exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn devlocal_host_rejects_unsigned_formal_admission_and_conflicted_recovery_without_publishing() {
    use oasis7_runtime::{
        GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
        MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    };
    use std::collections::{BTreeMap, BTreeSet};
    let controller = "msig.genesis.v1";
    let liveops = "liveops";
    let registry = GovernanceMainTokenControllerRegistry {
        genesis_controller_account_id: controller.into(),
        treasury_bucket_controller_slots: BTreeMap::from([(
            MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(),
            liveops.into(),
        )]),
        restricted_starter_claim_admin_account_ids: BTreeSet::from([liveops.into()]),
        controller_signer_policies: [controller, liveops]
            .into_iter()
            .enumerate()
            .map(|(i, account)| {
                let public = hex::encode(
                    SigningKey::from_bytes(&[91 + i as u8; 32])
                        .verifying_key()
                        .to_bytes(),
                );
                (
                    account.into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([public]),
                    },
                )
            })
            .collect(),
    };
    let (dir, mut driver, identity, _, _) =
        canonical_agent_chat::driver_fixture_with_controller_registry(registry);
    let control = config(&driver.execution_world, &identity);
    driver.execution_world = driver
        .execution_world
        .clone()
        .with_devlocal_participant_authority(control.authority.clone().unwrap())
        .unwrap();
    driver.devlocal_participant = control;
    commit(&mut driver).unwrap();
    let before = driver.execution_world.snapshot();
    let head = fs::read(driver.records_dir.join("latest.json")).unwrap();
    let snapshot = fs::read(driver.world_dir.join("snapshot.json")).unwrap();
    let journal = fs::read(driver.world_dir.join("journal.json")).unwrap();
    let height = driver.state.last_applied_committed_height + 1;
    let action = oasis7_runtime::Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: controller.into(),
        candidate_id: "candidate.fixture".into(),
        node_id: "validator.fixture".into(),
        finality_signer_public_key: hex::encode(
            SigningKey::from_bytes(&[93; 32]).verifying_key().to_bytes(),
        ),
        stake: 25,
        operator_owner: "ops.team".into(),
        public_manifest_hash: "manifest:candidate.fixture".into(),
    };
    let payload = oasis7::consensus_action_payload::encode_consensus_action_payload(
        &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
            action,
        ),
    )
    .unwrap();
    let actions =
        vec![oasis7_node::NodeConsensusAction::from_payload(height, "node-a", payload).unwrap()];
    let error = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("devlocal-conflict-{height}"),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap_err();
    assert!(
        error.contains("governance authorization mismatch"),
        "{error}"
    );
    assert_eq!(driver.execution_world.snapshot(), before);
    assert_eq!(
        fs::read(driver.records_dir.join("latest.json")).unwrap(),
        head
    );
    assert_eq!(
        fs::read(driver.world_dir.join("snapshot.json")).unwrap(),
        snapshot
    );
    assert_eq!(
        fs::read(driver.world_dir.join("journal.json")).unwrap(),
        journal
    );
    assert!(
        !super::super::super::checkpoint::execution_bridge_record_path(&driver.records_dir, height)
            .exists()
    );
    // Authentication now rejects this request before stepping. Independently
    // retain the DevLocal/formal authority boundary at recovery: an established
    // formal membership cannot be imported as DevLocal authority either.
    let mut conflicted = before.clone();
    conflicted.state.governance_validator_admissions.insert("candidate.fixture".into(), oasis7_runtime::GovernanceValidatorAdmissionRecord {
        candidate_id: "candidate.fixture".into(), node_id: "validator.fixture".into(),
        finality_signer_public_key: hex::encode(SigningKey::from_bytes(&[93; 32]).verifying_key().to_bytes()),
        stake: 25, operator_owner: "ops.team".into(), public_manifest_hash: "manifest:candidate.fixture".into(),
        ..Default::default()
    });
    let error = oasis7::runtime_host::world::from_snapshot_with_devlocal_participant_authority(
        conflicted, driver.execution_world.journal().clone(), driver.devlocal_participant.authority.clone().unwrap(),
    ).expect_err("formal membership cannot recover under DevLocal authority");
    assert!(format!("{error:?}").contains("DevLocal") || format!("{error:?}").contains("local participant"));
    // The forbidden action is not retained as a hidden pending side effect.
    commit(&mut driver).unwrap();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn devlocal_host_null_genesis_requires_verified_origin_and_commits_original_binding_once() {
    let (dir, mut legacy, identity, _, _) = canonical_agent_chat::driver_fixture_without_provider();
    // Reproduce the old ordinary constructor: canonical commits did not retain
    // resource_identity, even though the separate service origin was fixed.
    commit(&mut legacy).unwrap();
    let state = legacy.state_path.clone();
    let world = legacy.world_dir.clone();
    let records = legacy.records_dir.clone();
    let store = dir.join("store");
    drop(legacy);
    let legacy = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state.clone(),
        world.clone(),
        records.clone(),
        store.clone(),
        &StorageProfileConfig::default(),
    )
    .unwrap();
    assert!(
        legacy
            .execution_world
            .chain_resource_manifest()
            .genesis_ref
            .is_none()
    );
    let agents = legacy.execution_world.state().agents.clone();
    let claims = legacy.execution_world.state().starter_oc_claims.clone();
    let mut control = config(&legacy.execution_world, &identity);
    drop(legacy);
    assert!(
        NodeRuntimeExecutionDriver::new_with_participant_control(
            state.clone(),
            world.clone(),
            records.clone(),
            store.clone(),
            &StorageProfileConfig::default(),
            control.clone(),
        )
        .err()
        .unwrap()
        .contains("signed binding migration")
    );
    // This setter is the trusted Host seam: production calls it only after
    // verify_existing_origin authenticates the receipt under the node key.
    control.set_verified_origin(identity.clone()).unwrap();
    let mut migrated = NodeRuntimeExecutionDriver::new_with_participant_control(
        state.clone(),
        world.clone(),
        records.clone(),
        store.clone(),
        &StorageProfileConfig::default(),
        control.clone(),
    )
    .unwrap();
    assert!(
        migrated
            .execution_world
            .chain_resource_manifest()
            .genesis_ref
            .is_none(),
        "preflight never publishes a repaired world"
    );
    commit(&mut migrated).unwrap();
    assert_eq!(
        migrated
            .execution_world
            .chain_resource_manifest()
            .genesis_ref
            .as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    assert_eq!(migrated.execution_world.state().agents, agents);
    assert_eq!(migrated.execution_world.state().starter_oc_claims, claims);
    let registry = migrated
        .execution_world
        .participant_registry_digest()
        .unwrap();
    drop(migrated);
    control.startup = None;
    control.verified_origin = None;
    let restored = NodeRuntimeExecutionDriver::new_with_participant_control(
        state,
        world,
        records.clone(),
        store.clone(),
        &StorageProfileConfig::default(),
        control.clone(),
    )
    .unwrap();
    assert_eq!(
        restored
            .execution_world
            .participant_registry_digest()
            .unwrap(),
        registry
    );
    let context = super::super::super::world_service_read::RestoreContext::new(
        ReleaseSecurityPolicy::default(),
        control.authority,
    )
    .unwrap();
    let pinned = super::super::super::world_service_read::pin_with_context(
        &records, &store, &identity, None, &context,
    )
    .unwrap();
    assert_eq!(
        pinned
            .world
            .chain_resource_manifest()
            .genesis_ref
            .as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    let _ = fs::remove_dir_all(dir);
}
