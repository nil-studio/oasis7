use super::{
    CliOptions, DEFAULT_WORLD_ID, build_default_replication_network_config,
    create_default_replication_network, node_keypair_config, parse_options,
};
use ed25519_dalek::SigningKey;
use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

fn loaded_manifest(chain_id: &str) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: "peer-publication-test-manifest.json".into(),
        manifest: NetworkTierManifest {
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.into(),
            tier: "public_testnet".into(),
            status: "rehearsal".into(),
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            network_id: "peer-publication-test-network".into(),
            chain_id: chain_id.into(),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".into(),
                genesis_ref: "genesis.json".into(),
                bootstrap_peer_ref: "peers.txt".into(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "https://rpc.example.invalid".into(),
                explorer_ref: "https://explorer.example.invalid".into(),
                faucet_ref: None,
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "managed".into(),
                validator_admission: "allowlist".into(),
                target_validator_count: 1,
                allow_observer_nodes: true,
            },
            token_policy: NetworkTierTokenPolicy {
                symbol: "OC".into(),
                faucet_mode: "disabled".into(),
                reset_policy: None,
                value_semantics: "test-only".into(),
            },
            claims_policy: NetworkTierClaimsPolicy {
                allowed_claims: Vec::new(),
                denied_claims: Vec::new(),
            },
            promotion_policy: NetworkTierPromotionPolicy {
                promote_from: Vec::new(),
                required_gates: Vec::new(),
            },
            evidence_refs: Vec::new(),
        },
        bootstrap_peers: Vec::new(),
    }
}

#[test]
fn peer_publication_initializer_requires_explicit_v2_and_rejects_formal_live() {
    let legacy_error = parse_options(["--initialize-peer-record-publication"].into_iter())
        .expect_err("one-time initializer must reject the legacy V1 default");
    assert!(legacy_error.contains("requires --p2p-peer-record-publication v2_durable"));

    let formal_error = parse_options(
        [
            "--initialize-peer-record-publication",
            "--p2p-peer-record-publication",
            "v2_durable",
            "--formal-live-config",
            "formal.json",
        ]
        .into_iter(),
    )
    .expect_err("formal-live profiles must not accept the development P2P initializer");
    assert!(formal_error.contains("cannot be combined with --formal-live-config"));
}

#[test]
fn default_peer_record_publication_stays_legacy_without_explicit_opt_in() {
    let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let options = CliOptions::default();
    let paths = super::resolve_runtime_paths(&options);
    let config = build_default_replication_network_config(&options, &keypair, &paths)
        .expect("default replication network config should build");
    assert!(config.peer_record_publication_directory.is_none());
}

#[test]
#[cfg(unix)]
fn durable_peer_record_publication_is_opt_in_and_bound_to_runtime_root() {
    use oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1;

    let listener_reservation =
        TcpListener::bind(("127.0.0.1", 0)).expect("reserve host listener port");
    let listener_addr = listener_reservation
        .local_addr()
        .expect("read reserved host listener address");
    drop(listener_reservation);
    let listener_multiaddr = format!("/ip4/127.0.0.1/tcp/{}", listener_addr.port());

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let scratch_alias = std::env::temp_dir().join(format!(
        "oasis7-peer-record-publication-v2-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch_alias).expect("create test scratch directory");
    let scratch = std::fs::canonicalize(&scratch_alias)
        .expect("canonicalize test scratch directory to its real path");
    let runtime_root = scratch.join("runtime");
    let options = parse_options(
        [
            "--runtime-root",
            runtime_root.to_str().expect("UTF-8 test path"),
            "--config",
            scratch
                .join("config.toml")
                .to_str()
                .expect("UTF-8 config path"),
            "--p2p-peer-record-publication",
            "v2_durable",
            "--replication-network-listen",
            listener_multiaddr.as_str(),
            "--initialize-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect("explicit V2 initialization should parse");
    assert!(options.initialize_peer_record_publication);
    assert_eq!(
        options.p2p_peer_record_publication,
        DevelopmentPeerRecordPublicationModeV1::V2Durable
    );
    let typed = super::development_runtime::from_options(&options)
        .expect("the one-shot action should leave typed runtime configuration valid");
    let serialized = serde_json::to_string(&typed).expect("serialize typed runtime config");
    assert!(
        !serialized.contains("initialize_peer_record_publication"),
        "bootstrap intent must never persist into restart configuration"
    );
    let paths = super::resolve_runtime_paths(&options);
    let expected = runtime_root
        .as_path()
        .join("p2p")
        .join("peer-record-publication");
    let start_options = CliOptions {
        initialize_peer_record_publication: false,
        ..options.clone()
    };

    // Normal host construction must fail closed before the explicit bootstrap.
    let test_key = SigningKey::from_bytes(&[19_u8; 32]);
    let test_keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(test_key.to_bytes()),
        public_key_hex: hex::encode(test_key.verifying_key().to_bytes()),
    };
    assert!(create_default_replication_network(&start_options, &test_keypair, &paths).is_err());
    assert!(
        !expected.exists(),
        "normal startup must not create publisher state"
    );

    super::run_chain_runtime(options.clone()).expect("one-time host bootstrap should succeed");
    assert!(
        expected.is_dir(),
        "initializer should create durable publisher state"
    );
    assert!(
        !paths.execution_world_dir.exists(),
        "initialization must exit before creating or starting a world"
    );
    assert!(
        super::run_chain_runtime(options.clone()).is_err(),
        "repeated initialization must reject existing state"
    );
    let keypair =
        node_keypair_config::ensure_node_keypair_in_config(Path::new(options.config_path.as_str()))
            .expect("read initialized stable root keypair");

    let paths = super::resolve_runtime_paths(&options);
    let config = build_default_replication_network_config(&start_options, &keypair, &paths)
        .expect("explicit V2 network config should build");
    assert_eq!(
        config.peer_record_publication_directory,
        Some(expected.clone())
    );
    assert!(config.peer_record.is_some());
    assert!(config.keypair.is_some());

    let again = build_default_replication_network_config(&start_options, &keypair, &paths)
        .expect("repeated config should build");
    assert_eq!(
        again.peer_record_publication_directory,
        Some(expected.clone())
    );

    let publication_directory = config
        .peer_record_publication_directory
        .as_ref()
        .expect("V2 publisher directory");
    let network = create_default_replication_network(&start_options, &keypair, &paths)
        .expect("host-built V2 configuration should open initialized state");
    assert!(!network.peer_id().to_string().is_empty());
    assert!(publication_directory.is_dir());
    let listen_deadline = Instant::now() + Duration::from_secs(5);
    let mut listener_reachable = false;
    while !listener_reachable && Instant::now() < listen_deadline {
        listener_reachable =
            TcpStream::connect_timeout(&listener_addr, Duration::from_millis(100)).is_ok();
        if !listener_reachable {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(
        listener_reachable,
        "host-built V2 network should bind the configured real TCP listener"
    );
    drop(network);

    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::remove_dir_all(&expected).expect("remove complete publisher directory");
    let missing_directory_error =
        create_default_replication_network(&start_options, &keypair, &paths)
            .err()
            .expect("missing publisher directory must reject normal startup");
    assert!(
        !expected.exists(),
        "normal startup must not recreate a missing publication directory"
    );
    assert!(
        missing_directory_error.contains("restore") || missing_directory_error.contains("original"),
        "startup error should direct operators to restore original state: {missing_directory_error}"
    );
    assert!(
        !missing_directory_error.contains("--initialize-peer-record-publication"),
        "startup errors must not suggest initialization as recovery"
    );

    let other_root = CliOptions {
        runtime_root: Some(scratch.join("other-runtime")),
        ..start_options
    };
    let other_paths = super::resolve_runtime_paths(&other_root);
    let other = build_default_replication_network_config(&other_root, &keypair, &other_paths)
        .expect("another root should build");
    assert_ne!(
        config.peer_record_publication_directory,
        other.peer_record_publication_directory
    );
    std::fs::remove_dir_all(scratch).expect("remove test scratch directory");
}

#[test]
#[cfg(unix)]
fn durable_peer_record_initializer_and_normal_start_share_manifest_world_binding() {
    use oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1;

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let scratch_alias = std::env::temp_dir().join(format!(
        "oasis7-peer-record-manifest-binding-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch_alias).expect("create test scratch directory");
    let scratch = std::fs::canonicalize(&scratch_alias)
        .expect("canonicalize test scratch directory to its real path");
    let runtime_root = scratch.join("runtime");
    let mut options = parse_options(
        [
            "--runtime-root",
            runtime_root.to_str().expect("UTF-8 runtime path"),
            "--config",
            scratch
                .join("config.toml")
                .to_str()
                .expect("UTF-8 config path"),
            "--p2p-peer-record-publication",
            "v2_durable",
            "--initialize-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect("parse explicit durable publisher bootstrap");
    assert_eq!(options.world_id, DEFAULT_WORLD_ID);
    assert_eq!(
        options.p2p_peer_record_publication,
        DevelopmentPeerRecordPublicationModeV1::V2Durable
    );
    let manifest_chain_id = "oasis7-peer-record-manifest-binding";
    options.loaded_network_tier_manifest = Some(loaded_manifest(manifest_chain_id));

    super::run_chain_runtime(options.clone())
        .expect("bootstrap should apply the manifest's default world id");

    let normal_options = CliOptions {
        initialize_peer_record_publication: false,
        ..options
    };
    let mut normal_options = normal_options;
    super::apply_network_tier_manifest_defaults(&mut normal_options);
    assert_eq!(normal_options.world_id, manifest_chain_id);

    let keypair = node_keypair_config::ensure_node_keypair_in_config(Path::new(
        normal_options.config_path.as_str(),
    ))
    .expect("read stable identity created by bootstrap");
    let paths = super::resolve_runtime_paths(&normal_options);
    let config = build_default_replication_network_config(&normal_options, &keypair, &paths)
        .expect("normal network config should use manifest defaults");
    let peer_record = config.peer_record.as_ref().expect("peer record template");
    assert_eq!(peer_record.world_id, manifest_chain_id);
    assert_eq!(peer_record.network_id, manifest_chain_id);

    let network = create_default_replication_network(&normal_options, &keypair, &paths)
        .expect("normal startup should open the initialized matching binding");
    drop(network);
    std::thread::sleep(Duration::from_millis(100));
    std::fs::remove_dir_all(scratch).expect("remove test scratch directory");
}

#[cfg(not(unix))]
#[test]
fn durable_peer_record_publication_explicit_initialization_is_unsupported() {
    use oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let scratch = std::env::temp_dir().join(format!(
        "oasis7-peer-record-publication-unsupported-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create test scratch directory");
    let signing_key = SigningKey::from_bytes(&[21_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };

    let options = CliOptions {
        runtime_root: Some(scratch.join("runtime")),
        p2p_peer_record_publication: DevelopmentPeerRecordPublicationModeV1::V2Durable,
        initialize_peer_record_publication: true,
        ..CliOptions::default()
    };
    let error = super::initialize_default_peer_record_publication(&options)
        .expect_err("V2 initialization must reject unsupported filesystem guarantees");
    assert!(error.contains("requires Unix filesystem guarantees"));

    let start_options = CliOptions {
        initialize_peer_record_publication: false,
        ..options.clone()
    };
    let paths = super::resolve_runtime_paths(&start_options);
    let start_error = create_default_replication_network(&start_options, &keypair, &paths)
        .err()
        .expect("normal V2 startup must reject unsupported filesystem guarantees");
    assert!(start_error.contains("Unix") || start_error.contains("unsupported"));
    assert!(!paths.runtime_root.join("p2p").exists());
    std::fs::remove_dir_all(scratch).expect("remove test scratch directory");
}

#[test]
fn durable_peer_record_publication_setup_failure_is_returned_from_node_adapter() {
    use oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1;

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let scratch = std::env::temp_dir().join(format!(
        "oasis7-peer-record-publication-host-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch).expect("create test scratch directory");
    let blocker = scratch.join("runtime-root-file");
    std::fs::write(&blocker, b"not a directory").expect("create blocking file");

    let signing_key = SigningKey::from_bytes(&[20_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let options = CliOptions {
        runtime_root: Some(blocker.clone()),
        p2p_peer_record_publication: DevelopmentPeerRecordPublicationModeV1::V2Durable,
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&options);
    assert!(create_default_replication_network(&options, &keypair, &paths).is_err());

    std::fs::remove_dir_all(scratch).expect("remove test scratch directory");
}
