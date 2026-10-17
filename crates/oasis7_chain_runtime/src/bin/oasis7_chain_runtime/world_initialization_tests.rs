use super::*;
use ed25519_dalek::Signer;
use oasis7_distfs::controlled_authority::{
    formal::{
        EndpointSignerBindingV1, InitialActivationBodyV1, SuccessorRuleV1,
        fencing::EndpointInstanceBindingV1, initial_activation_signing_bytes,
    },
    replicated_protocol::EndpointRole,
};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn hash(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn private_test_root(prefix: &str) -> std::path::PathBuf {
    std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
}

fn endpoint(
    world: &FormalWorldIdentityV1,
    role: EndpointRole,
    key_byte: u8,
    identity_byte: u8,
) -> EndpointSignerBindingV1 {
    EndpointSignerBindingV1 {
        instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: world.world_id.clone(),
            chain_id: world.chain_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            authority_epoch: 1,
            endpoint_role: role,
            endpoint_id: format!("endpoint-{identity_byte}"),
            endpoint_public_key: hex::encode(key(key_byte).verifying_key().to_bytes()),
            instance_id: hash(identity_byte),
            storage_id: hash(identity_byte + 1),
            storage_fingerprint: hash(identity_byte + 2),
        },
        fault_domain_id: format!("fault-domain-{identity_byte}"),
    }
}

fn prepared_fixture() -> (
    PreparedWorldInitializationV1,
    SignedInitialActivationV1,
    InitialActivationTrustV1,
) {
    let world = FormalWorldIdentityV1 {
        world_id: "world-init-test".into(),
        chain_id: "chain-init-test".into(),
        genesis_digest: hash(1),
    };
    let policy = RecoveryPolicyV1 {
        schema_version: 1,
        recovery_issuer_public_key: hex::encode(key(7).verifying_key().to_bytes()),
        fencing_provider_public_key: hex::encode(key(8).verifying_key().to_bytes()),
        authorized_fencing_resource_bindings: vec![hash(12)],
        issuer_anchor_public_key: hex::encode(key(9).verifying_key().to_bytes()),
        allow_complete_prepared: true,
        allow_replay_frozen: true,
        successor_rule: SuccessorRuleV1::IncrementEpochDistinctBindings,
        terminates_at_profile_transition: true,
    };
    let authority = AuthorityDefinitionV1 {
        schema_version: 1,
        world: world.clone(),
        profile: "controlled_single_authority".into(),
        profile_version: 2,
        epoch: 1,
        writer_public_key: hex::encode(key(4).verifying_key().to_bytes()),
        primary: endpoint(&world, EndpointRole::Primary, 5, 20),
        replica: endpoint(&world, EndpointRole::Replica, 6, 30),
        recovery_policy_digest: policy.digest().unwrap(),
        predecessor_authority_id: None,
        valid_from_position: 1,
    };
    let mut roots = BTreeMap::new();
    for role in [
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Result,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Snapshot,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Journal,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::NonceIndex,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::EffectOutbox,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Wasm,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Rules,
    ] {
        let byte = (roots.len() as u8).saturating_add(40);
        roots.insert(role, hash(byte));
    }
    let snapshot_digest = roots
        [&oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Snapshot]
        .clone();
    let activation_body = InitialActivationBodyV1 {
        schema_version: 1,
        authority,
        recovery_policy: policy.clone(),
        initial_state_root: snapshot_digest.clone(),
        runtime_manifest_digest: roots[
            &oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest
        ]
            .clone(),
        governing_manifest_digest: hash(80),
        execution_policy_digest: hash(81),
        artifact_roots: roots,
    };
    let activation_ref = activation_body.activation_ref().unwrap();
    let request_digest = compute_request_digest(&world, &activation_ref).unwrap();
    let issuer = key(10);
    let mut prepared = PreparedWorldInitializationV1 {
        schema_version: 1,
        domain: REQUEST_DOMAIN.into(),
        mode: WorldBoundaryModeV1::Genesis,
        world: world.clone(),
        snapshot_digest,
        journal_digest: hash(82),
        trusted_issuer_public_key: hex::encode(issuer.verifying_key().to_bytes()),
        node_bootstrap: InitialNodeBootstrapV1 {
            height: 0,
            consensus_block_hash: hash(83),
            execution_block_hash: hash(84),
            execution_state_root: hash(41),
        },
        activation_body: activation_body.clone(),
        activation_ref,
        request_digest,
        prepare_digest: String::new(),
    };
    prepared.node_bootstrap.execution_state_root = prepared.snapshot_digest.clone();
    prepared.prepare_digest = compute_prepare_digest(&prepared).unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: hex::encode(
            issuer
                .sign(&initial_activation_signing_bytes(&prepared.activation_body).unwrap())
                .to_bytes(),
        ),
        body: prepared.activation_body.clone(),
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: hex::encode(issuer.verifying_key().to_bytes()),
        world,
        initial_state_root: prepared.snapshot_digest.clone(),
        governing_manifest_digest: prepared.activation_body.governing_manifest_digest.clone(),
        recovery_policy_digest: policy.digest().unwrap(),
    };
    (prepared, certificate, trust)
}

#[test]
fn prepared_request_is_exactly_bound_to_external_v2_approval() {
    let (prepared, certificate, trust) = prepared_fixture();
    validate_prepared(&prepared).unwrap();
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    require_exact_approval(&prepared, &certificate, &authority).unwrap();
    let quote = approval_quote(&prepared, &trust).unwrap();
    assert_eq!(quote.request_digest, prepared.request_digest);
    assert_eq!(quote.activation_ref, prepared.activation_ref);
    assert_eq!(quote.world, prepared.world);

    let original_prepare_digest = prepared.prepare_digest.clone();
    let mut changed = prepared;
    changed.node_bootstrap.consensus_block_hash = hash(99);
    changed.prepare_digest = compute_prepare_digest(&changed).unwrap();
    validate_prepared(&changed).unwrap();
    assert_ne!(changed.prepare_digest, original_prepare_digest);
}

#[test]
fn certificate_artifact_cannot_choose_its_own_trust_pin() {
    let (_, certificate, _) = prepared_fixture();
    let certificate_value = serde_json::to_value(certificate).unwrap();
    let valid = serde_json::json!({ "certificate": certificate_value });
    assert!(serde_json::from_value::<ActivationCertificateInputV1>(valid.clone()).is_ok());

    let mut self_pinned = valid;
    self_pinned["trust"] = serde_json::json!({ "issuer_public_key": hash(77) });
    assert!(serde_json::from_value::<ActivationCertificateInputV1>(self_pinned).is_err());
}

#[test]
fn configured_issuer_pin_requires_canonical_ed25519_public_key() {
    let (prepared, _, _) = prepared_fixture();
    validate_issuer_public_key(&prepared.trusted_issuer_public_key).unwrap();
    assert!(
        validate_issuer_public_key(&prepared.trusted_issuer_public_key.to_uppercase()).is_err()
    );
    assert!(validate_issuer_public_key("00").is_err());
}

#[cfg(unix)]
#[test]
fn saved_request_rejects_a_changed_world_or_node_boundary() {
    let (prepared, _, _) = prepared_fixture();
    let root = private_test_root("oasis7-world-init-request");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("request.cbor");
    let bytes = serde_cbor::to_vec(&prepared).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    verify_saved_request(&path, &prepared).unwrap();

    let mut changed = prepared;
    changed.node_bootstrap.consensus_block_hash = hash(99);
    changed.prepare_digest = compute_prepare_digest(&changed).unwrap();
    assert!(verify_saved_request(&path, &changed).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn initialization_config_rejects_profile_v1_before_loading_any_world_source() {
    let (prepared, _, _) = prepared_fixture();
    let mut authority = prepared.activation_body.authority.clone();
    authority.profile_version = 1;
    let config = WorldInitializationConfigV1 {
        schema_version: 1,
        mode: WorldBoundaryModeV1::Genesis,
        world: prepared.world,
        snapshot_file: "missing-snapshot.cbor".into(),
        journal_file: "missing-journal.cbor".into(),
        trusted_issuer_public_key: prepared.trusted_issuer_public_key,
        node_bootstrap: prepared.node_bootstrap,
        authority,
        recovery_policy: prepared.activation_body.recovery_policy,
    };
    let root = private_test_root("oasis7-world-init-profile");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("config.json");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let error = match rebuild(&path) {
        Ok(_) => panic!("profile v1 initialization unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(error.contains("formal profile version 2"));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn prepare_and_activation_outputs_are_create_only_and_idempotent() {
    let root = private_test_root("oasis7-world-init-output");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = root.join("prepared.cbor");
    write_private_idempotent(&output, b"original", 64).unwrap();
    write_private_idempotent(&output, b"original", 64).unwrap();
    assert!(write_private_idempotent(&output, b"different", 64).is_err());
    assert_eq!(
        read_private_owned_bounded(&output, 64).unwrap(),
        b"original"
    );

    let target = root.join("target");
    std::fs::write(&target, b"target").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = root.join("output-link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(write_private_idempotent(&link, b"replacement", 64).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"target");
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn private_inputs_are_checked_on_the_open_file_descriptor() {
    let root = private_test_root("oasis7-world-init-input");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let input = root.join("private.json");
    std::fs::write(&input, b"safe").unwrap();
    std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(read_private_owned_bounded(&input, 4).unwrap(), b"safe");
    assert!(read_private_owned_bounded(&input, 3).is_err());

    std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_private_owned_bounded(&input, 4).is_err());

    let link = root.join("private-link.json");
    std::os::unix::fs::symlink(&input, &link).unwrap();
    assert!(read_private_owned_bounded(&link, 4).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn world_init_cli_rejects_incomplete_or_unknown_activation_arguments() {
    let result = dispatch(["world", "init", "activate", "--config", "unused"].into_iter())
        .expect("world init dispatch");
    assert!(result.unwrap_err().contains("requires exactly"));
    let result = dispatch(
        [
            "world",
            "init",
            "prepare",
            "--config",
            "unused",
            "--request",
            "unused",
            "--self-pin",
            "yes",
        ]
        .into_iter(),
    )
    .expect("world init dispatch");
    assert!(result.unwrap_err().contains("requires exactly"));
}

#[cfg(unix)]
#[test]
fn new_world_prepare_cli_requires_native_schema_before_reading_world_sources() {
    let (prepared, _, _) = prepared_fixture();
    let config = WorldInitializationConfigV1 {
        schema_version: 1,
        mode: WorldBoundaryModeV1::Genesis,
        world: prepared.world,
        snapshot_file: "missing-snapshot.cbor".into(),
        journal_file: "missing-journal.cbor".into(),
        trusted_issuer_public_key: prepared.trusted_issuer_public_key,
        node_bootstrap: prepared.node_bootstrap,
        authority: prepared.activation_body.authority,
        recovery_policy: prepared.activation_body.recovery_policy,
    };
    let root = private_test_root("oasis7-world-init-native-gate");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("config.json");
    write_private_idempotent(
        &path,
        &serde_json::to_vec(&config).unwrap(),
        MAX_CONFIG_BYTES,
    )
    .unwrap();
    let output = root.join("request.cbor");
    let result = dispatch(
        [
            "world",
            "init",
            "prepare",
            "--config",
            path.to_str().unwrap(),
            "--request",
            output.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap();
    assert!(
        result
            .unwrap_err()
            .contains("schema 2 with the original native environment")
    );
    assert!(!output.exists());
    std::fs::remove_dir_all(root).unwrap();
}
