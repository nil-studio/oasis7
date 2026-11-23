use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use crate::formal_read_fixtures as fixtures;

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let unique = crate::node_lifecycle::local_io::random_id().unwrap();
        let path = base.join(format!("oasis7-formal-live-{label}-{unique}"));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    root: TempRoot,
    config_path: PathBuf,
    config: FormalLiveConfigV1,
    anchor: VerifiedFormalReadAnchorV1,
}
impl Fixture {
    fn new() -> Self {
        let (root, config_path, config, anchor) = Self::paths("loader");
        let fixture = Self {
            root,
            config_path,
            config,
            anchor,
        };
        fixture.write_inputs();
        fixture
    }

    fn paths(
        label: &str,
    ) -> (
        TempRoot,
        PathBuf,
        FormalLiveConfigV1,
        VerifiedFormalReadAnchorV1,
    ) {
        let root = TempRoot::new(label);
        let (_world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
        let checkpoint = anchor.checkpoint();
        let trust = FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        };
        let node_directory = root.0.join("node-keys");
        let writer_directory = root.0.join("writer");
        fs::create_dir(&node_directory).unwrap();
        fs::set_permissions(&node_directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(&writer_directory).unwrap();
        fs::set_permissions(&writer_directory, fs::Permissions::from_mode(0o700)).unwrap();
        let node_key = fixtures::key(2);
        let node_key_path = node_directory.join("node-keypair.toml");
        fs::write(
            &node_key_path,
            format!(
                "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
                hex::encode(node_key.to_bytes()),
                hex::encode(node_key.verifying_key().to_bytes()),
            ),
        )
        .unwrap();
        fs::set_permissions(&node_key_path, fs::Permissions::from_mode(0o600)).unwrap();

        let writer_key_path = root.0.join("writer.key");
        fs::write(&writer_key_path, hex::encode(fixtures::key(4).to_bytes())).unwrap();
        fs::set_permissions(&writer_key_path, fs::Permissions::from_mode(0o600)).unwrap();
        let trust_path = root.0.join("formal-read-trust.json");
        fs::write(&trust_path, serde_json::to_vec(&trust).unwrap()).unwrap();
        fs::set_permissions(&trust_path, fs::Permissions::from_mode(0o600)).unwrap();

        let config = FormalLiveConfigV1 {
            schema_version: 1,
            node_id: "formal-node-a".into(),
            node_keypair_directory: PathBuf::from("node-keys"),
            writer_key_file: Some(PathBuf::from("writer.key")),
            writer_custody: None,
            formal_read_trust_file: PathBuf::from("formal-read-trust.json"),
            writer_directory: PathBuf::from("writer"),
            primary: "127.0.0.1:21001".parse().unwrap(),
            replica: "127.0.0.1:21002".parse().unwrap(),
            timeout_ms: 5_000,
            status_bind: "127.0.0.1:21003".parse().unwrap(),
            recovery_permission_file: None,
            recovery_issuer_minimum_file: None,
            recovery_origin_history_directory: None,
            approved_initial_file: None,
            effect_delivery: None,
            full_sync_read_authorization_v3: None,
            full_sync_provider_read_authorization:
                FullSyncProviderReadAuthorizationModeV1::LegacyV1,
        };
        let config_path = root.0.join("formal-live.json");
        (root, config_path, config, anchor)
    }

    fn write_inputs(&self) {
        write_private(
            &self.config_path,
            &serde_json::to_vec(&self.config).unwrap(),
        );
    }

    fn save(&self) {
        self.write_inputs();
    }
}

fn write_private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn formal_loader_rejects_legacy_or_unknown_configuration_without_promoting_it() {
    let fixture = Fixture::new();
    let mut legacy = serde_json::to_value(&fixture.config).unwrap();
    legacy["scope"] = "guarded_local_execution_prerequisite".into();
    write_private(&fixture.config_path, &serde_json::to_vec(&legacy).unwrap());
    let error = FormalLiveConfigV1::load(&fixture.config_path)
        .err()
        .unwrap();
    assert!(error.contains("unknown field"), "{error}");
    assert_eq!(fixture.anchor.head().record().original_epoch, 1);
}

#[test]
fn existing_formal_configuration_defaults_to_legacy_v1_without_serializing_a_new_mode() {
    let fixture = Fixture::new();
    let mut old_shape = serde_json::to_value(&fixture.config).unwrap();
    old_shape
        .as_object_mut()
        .unwrap()
        .remove("full_sync_provider_read_authorization");
    let loaded: FormalLiveConfigV1 = serde_json::from_value(old_shape.clone()).unwrap();
    assert_eq!(
        loaded.full_sync_provider_read_authorization,
        FullSyncProviderReadAuthorizationModeV1::LegacyV1
    );
    let serialized = serde_json::to_value(loaded).unwrap();
    assert_eq!(serialized, old_shape);
}

#[test]
fn formal_writer_versions_reject_mixed_or_missing_custody_fields_before_key_reads() {
    let fixture = Fixture::new();
    let local = serde_json::to_value(&fixture.config).unwrap();
    let custody = serde_json::json!({
        "endpoint": "127.0.0.1:21004", "credential_file": "missing-client.key",
        "minimum_revision": 0,
    });
    let mut remote = local.clone();
    remote["schema_version"] = 2.into();
    remote.as_object_mut().unwrap().remove("writer_key_file");
    remote["writer_custody"] = custody.clone();
    let mut mixed = remote.clone();
    mixed["writer_key_file"] = serde_json::Value::Null;
    let mut local_with_remote = local.clone();
    local_with_remote["writer_custody"] = custody;
    let mut missing = remote.clone();
    missing.as_object_mut().unwrap().remove("writer_custody");
    for wire in [mixed, local_with_remote, missing] {
        write_private(&fixture.config_path, &serde_json::to_vec(&wire).unwrap());
        let error = FormalLiveConfigV1::load(&fixture.config_path)
            .err()
            .unwrap();
        assert!(
            error.contains("schema 2 requires only writer_custody"),
            "{error}"
        );
    }
    let loaded: FormalLiveConfigV1 = serde_json::from_value(remote.clone()).unwrap();
    assert!(loaded.validate_writer_mode(&remote).is_ok());
    assert_eq!(serde_json::to_value(loaded).unwrap(), remote);
}

#[test]
fn remote_writer_capability_loads_only_independent_client_credential_without_network_io() {
    let mut fixture = Fixture::new();
    fs::remove_file(fixture.root.0.join("writer.key")).unwrap();
    let credential = fixture.root.0.join("client.key");
    write_private(
        &credential,
        hex::encode(fixtures::key(120).to_bytes()).as_bytes(),
    );
    fixture.config.schema_version = 2;
    fixture.config.writer_key_file = None;
    fixture.config.writer_custody = Some(FormalWriterCustodyConfigV2 {
        endpoint: "127.0.0.1:21004".parse().unwrap(),
        credential_file: credential.clone(),
        minimum_revision: 7,
    });
    let trust: FormalReadTrustConfigV1 =
        serde_json::from_slice(&fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap())
            .unwrap();
    let authority = fixture.anchor.authority();
    // This constructor uses an independently verified authority. The complete
    // profile-2 loader and actual RPC consumer run in the protected-process fixture.
    let signer = fixture
        .config
        .load_writer_capability(
            authority,
            &[authority],
            &trust.initial_trust.issuer_public_key,
        )
        .unwrap();
    assert_eq!(
        signer.public_key_hex(),
        authority.definition().writer_public_key
    );
    assert!(!signer.supports_healthy_durable_set());
    assert!(!fixture.root.0.join("writer.key").exists());
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
    assert_eq!(
        fixture.config.writer_authentication_input().unwrap(),
        ("writer_client_credential", credential.as_path())
    );

    let definition = authority.definition();
    let policy = authority.policy();
    for public in [
        &definition.writer_public_key,
        &definition.primary.instance.endpoint_public_key,
        &definition.replica.instance.endpoint_public_key,
        &policy.fencing_provider_public_key,
        &policy.recovery_issuer_public_key,
        &policy.issuer_anchor_public_key,
        &trust.initial_trust.issuer_public_key,
    ] {
        let seed = (0..=100u8)
            .find(|seed| hex::encode(fixtures::key(*seed).verifying_key().to_bytes()) == *public)
            .expect("protected fixture authority uses a known isolated test key");
        write_private(
            &credential,
            hex::encode(fixtures::key(seed).to_bytes()).as_bytes(),
        );
        let error = fixture
            .config
            .load_writer_capability(
                authority,
                &[authority],
                &trust.initial_trust.issuer_public_key,
            )
            .err()
            .expect("custody credential must not be an authority key");
        assert!(error.contains("must be independent"), "{error}");
    }
}

#[test]
fn formal_loader_rejects_v1_authorities_with_an_explicit_profile_reason() {
    let fixture = Fixture::new();
    fixture.save();
    let error = FormalLiveConfigV1::load(&fixture.config_path)
        .err()
        .unwrap();
    assert!(error.contains("profile_version 2"), "{error}");
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
}

#[test]
fn missing_private_paths_are_rejected_without_creating_or_repairing_them() {
    let root = TempRoot::new("no-create");
    let config_path = root.0.join("formal-live.json");
    let config = FormalLiveConfigV1 {
        schema_version: 1,
        node_id: "formal-node-a".into(),
        node_keypair_directory: PathBuf::from("missing-node-keys"),
        writer_key_file: Some(PathBuf::from("missing-writer.key")),
        writer_custody: None,
        formal_read_trust_file: PathBuf::from("missing-trust.json"),
        writer_directory: PathBuf::from("missing-writer-dir"),
        primary: "127.0.0.1:21001".parse().unwrap(),
        replica: "127.0.0.1:21002".parse().unwrap(),
        timeout_ms: 1,
        status_bind: "127.0.0.1:21003".parse().unwrap(),
        recovery_permission_file: None,
        recovery_issuer_minimum_file: None,
        recovery_origin_history_directory: None,
        approved_initial_file: None,
        effect_delivery: None,
        full_sync_read_authorization_v3: None,
        full_sync_provider_read_authorization: FullSyncProviderReadAuthorizationModeV1::LegacyV1,
    };
    write_private(&config_path, &serde_json::to_vec(&config).unwrap());
    assert!(FormalLiveConfigV1::load(&config_path).is_err());
    assert!(!root.0.join("missing-node-keys").exists());
    assert!(!root.0.join("missing-writer-dir").exists());
}

#[test]
fn oversized_existing_node_keypair_is_rejected_before_parsing() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .0
        .join("node-keys")
        .join(super::super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE);
    write_private(&path, &vec![b'x'; MAX_NODE_KEYPAIR_BYTES as usize + 1]);
    let error = FormalLiveConfigV1::load(&fixture.config_path)
        .err()
        .unwrap();
    assert!(error.contains("file type/size refused"), "{error}");
}

#[test]
fn node_keypair_parser_checks_the_derived_public_key_from_bounded_bytes() {
    let signing_key = fixtures::key(2);
    let bytes = format!(
        "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
        hex::encode(signing_key.to_bytes()),
        hex::encode(signing_key.verifying_key().to_bytes()),
    );
    let parsed = parse_node_keypair(bytes.as_bytes()).unwrap();
    assert_eq!(parsed.private_key_hex, hex::encode(signing_key.to_bytes()));
    assert_eq!(
        parsed.public_key_hex,
        hex::encode(signing_key.verifying_key().to_bytes())
    );

    let mismatched = bytes.replace(
        &hex::encode(signing_key.verifying_key().to_bytes()),
        &hex::encode(fixtures::key(3).verifying_key().to_bytes()),
    );
    assert!(
        parse_node_keypair(mismatched.as_bytes())
            .unwrap_err()
            .contains("does not match")
    );
}

#[test]
fn loopback_roles_and_timeout_are_checked_before_any_endpoint_use() {
    let mut fixture = Fixture::new();
    fixture.config.primary = "192.0.2.10:21001".parse().unwrap();
    fixture.save();
    assert!(
        FormalLiveConfigV1::load(&fixture.config_path)
            .err()
            .unwrap()
            .contains("loopback")
    );

    fixture.config.primary = "127.0.0.1:21001".parse().unwrap();
    fixture.config.replica = fixture.config.primary;
    fixture.save();
    assert!(
        FormalLiveConfigV1::load(&fixture.config_path)
            .err()
            .unwrap()
            .contains("loopback")
    );

    fixture.config.replica = "127.0.0.1:21002".parse().unwrap();
    fixture.config.timeout_ms = 60_001;
    fixture.save();
    assert!(
        FormalLiveConfigV1::load(&fixture.config_path)
            .err()
            .unwrap()
            .contains("timeout")
    );
}

#[test]
fn fixed_checkpoint_digest_mismatch_fails_closed() {
    let fixture = Fixture::new();
    let trust_path = fixture.root.0.join("formal-read-trust.json");
    let mut trust: FormalReadTrustConfigV1 =
        serde_json::from_slice(&fs::read(&trust_path).unwrap()).unwrap();
    trust.checkpoint_digest = fixtures::h(77);
    write_private(&trust_path, &serde_json::to_vec(&trust).unwrap());
    fixture.save();
    assert!(
        FormalLiveConfigV1::load(&fixture.config_path)
            .err()
            .unwrap()
            .contains("checkpoint mismatch")
    );
}

#[test]
fn approved_initial_requires_a_bounded_cbor_complete_publication() {
    let fixture = Fixture::new();
    let path = fixture.root.0.join("approved-initial.cbor");
    write_private(&path, &[0xff]);
    assert!(
        load_approved_initial(&path, &fixture.anchor)
            .unwrap_err()
            .contains("approved initial publication")
    );
}

#[path = "formal_live_recovery_config_tests.rs"]
mod recovery_tests;

#[path = "formal_live_recovery_lineage_config_tests.rs"]
mod recovery_lineage_config_tests;
