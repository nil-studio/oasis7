use super::*;
use oasis7_client_api::world_service::participant_identity_snapshot::{
    SnapshotIdentityNamespace, SnapshotRpConfig,
};

fn public(seed: u8) -> String {
    hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
            .verifying_key()
            .to_bytes(),
    )
}
fn fixture() -> (World, Config, IdentitySnapshot, ParticipantEntryPolicyV1) {
    let world = World::new_with_state(oasis7_runtime::WorldState::default());
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "world",
            chain_id: "test-chain",
            genesis_ref: Some("genesis"),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: 0,
        },
        "fixture",
        "fixture",
    );
    let mut world = World::from_snapshot_with_devlocal_participant_authority(
        snapshot,
        world.journal().clone(),
        authority(&public(77)).unwrap(),
    )
    .unwrap();
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    let namespace = GameIdentityNamespaceV1 {
        service_id: "identity".into(),
        environment_id: "fixture".into(),
        instance_epoch: DecimalU64(1),
    };
    let config = Config {
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        world_directory: "/private/world".into(),
        origin_root: "/private".into(),
        node_id: "node".into(),
        node_key_directory: "/private/node".into(),
        service_public_key: public(90),
        operator_public_key: public(77),
        identity_namespace: namespace.clone(),
        world_identity_scope: "world".into(),
    };
    let mut snapshot = IdentitySnapshot {
        api_rev: 1,
        identity_namespace: SnapshotIdentityNamespace {
            identity_service_id: "identity".into(),
            environment_id: "fixture".into(),
        },
        rp_config: SnapshotRpConfig {
            rp_id: "game.invalid".into(),
            allowed_origins: vec!["https://game.invalid".into()],
            version: 1,
            identity_instance_epoch: 1,
        },
        source_snapshot_epoch: 1,
        source_watermark: format!("sqlite-content-v1:0:{}", "a".repeat(64)),
        source_snapshot_digest: String::new(),
        complete: true,
        source_kind: "sqlite_single_read_transaction_v1".into(),
        account_count: 0,
        binding_count: 0,
        accounts: vec![],
        bindings: vec![],
    };
    snapshot.source_snapshot_digest = snapshot.computed_digest().unwrap();
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        identity_namespace: namespace,
        issuer_public_key: public(71),
        generation: 1,
        max_grant_ticks: 16,
        max_participants: 10,
        registry_complete: true,
        enabled: true,
    };
    (world, config, snapshot, policy)
}

#[test]
fn signed_snapshot_drives_only_detached_authorization_and_normal_install() {
    let (world, config, snapshot, policy) = fixture();
    let prepared = prepare(&world, &config.world).unwrap();
    let before = world.snapshot();
    let mut candidate = world.clone();
    let signed = assemble(
        &mut candidate,
        &config,
        &prepared,
        &snapshot,
        policy,
        vec![],
        &[71; 32],
        &[77; 32],
    )
    .unwrap();
    assert_eq!(world.snapshot(), before);
    assert_eq!(
        signed.request.manifest.request.source_snapshot_digest,
        snapshot.source_snapshot_digest
    );
    assert_eq!(signed.subject_public_key, public(77));
    assert_eq!(
        candidate.participant_policy_view()["authority_source"],
        "dev_local_operator"
    );
    assert!(world.participant_policy_view()["policy"].is_null());
}

#[test]
fn tamper_wrong_namespace_signer_and_stale_predecessor_leave_world_unchanged() {
    for variant in 0..5 {
        let (mut world, config, mut snapshot, policy) = fixture();
        let mut prepared = prepare(&world, &config.world).unwrap();
        match variant {
            0 => snapshot.complete = false,
            1 => {
                snapshot.identity_namespace.environment_id = "other".into();
                snapshot.source_snapshot_digest = snapshot.computed_digest().unwrap();
            }
            2 => prepared.registry_digest = "wrong".into(),
            3 => prepared.legacy_owner_state_digest = "wrong".into(),
            _ => prepared.branch_id = "other".into(),
        }
        let before = world.snapshot();
        assert!(
            assemble(
                &mut world,
                &config,
                &prepared,
                &snapshot,
                policy,
                vec![],
                &[71; 32],
                &[77; 32]
            )
            .is_err()
        );
        assert_eq!(before, world.snapshot());
    }
    let (mut world, config, snapshot, policy) = fixture();
    let prepared = prepare(&world, &config.world).unwrap();
    assert!(
        assemble(
            &mut world,
            &config,
            &prepared,
            &snapshot,
            policy,
            vec![],
            &[72; 32],
            &[77; 32]
        )
        .is_err()
    );
}

#[test]
fn private_material_no_link_bounds_and_immutable_output() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "o7-participant-tool-{}",
        hex::encode(random)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.join("prepared.json");
    files::publish(&output, &serde_json::json!({"a":1})).unwrap();
    assert!(files::publish(&output, &serde_json::json!({"a":2})).is_err());
    let value: serde_json::Value = files::read(&output).unwrap();
    assert_eq!(value["a"], 1);
    let oversized = root.join("oversized.json");
    assert!(files::publish(&oversized, &"x".repeat(1024 * 1024)).is_err());
    assert!(!oversized.exists());
    let link = root.join("link");
    symlink(&output, &link).unwrap();
    assert!(files::read::<serde_json::Value>(&link).is_err());
    let hard = root.join("hard");
    std::fs::hard_link(&output, &hard).unwrap();
    assert!(files::read::<serde_json::Value>(&hard).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn actual_restore_writes_stay_in_private_copy_and_source_bytes_remain_identical() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn inventory(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                for (key, bytes) in inventory(&entry.path()) {
                    out.insert(PathBuf::from(entry.file_name()).join(key), bytes);
                }
            } else {
                out.insert(
                    PathBuf::from(entry.file_name()),
                    std::fs::read(entry.path()).unwrap(),
                );
            }
        }
        out
    }
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("o7-offline-copy-{}", hex::encode(random)));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = root.join("world");
    let (world, config, _, _) = fixture();
    world.save_to_dir(&source).unwrap();
    let before = inventory(&source);
    let scratch_path;
    {
        let scratch = copy::isolated_copy(&source, &root).unwrap();
        scratch_path = scratch.path.clone();
        let loaded =
            oasis7::runtime_host::world::load_from_dir_with_devlocal_participant_authority(
                &scratch.path,
                authority(&config.operator_public_key).unwrap(),
            )
            .unwrap();
        assert_eq!(loaded.state().time, world.state().time);
        assert_eq!(inventory(&source), before);
    }
    assert!(!scratch_path.exists());
    let link = source.join("untrusted-link");
    symlink(root.join("outside"), &link).unwrap();
    assert!(copy::isolated_copy(&source, &root).is_err());
    std::fs::remove_file(link).unwrap();
    assert_eq!(inventory(&source), before);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
