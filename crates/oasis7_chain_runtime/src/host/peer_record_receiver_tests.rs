use super::{
    CliOptions, DEFAULT_WORLD_ID, build_default_replication_network_config,
    create_default_replication_network, node_keypair_config, parse_options,
};
use oasis7::development_node_config::DevelopmentPeerRecordReceptionModeV1;
use oasis7_node::PeerRecordReceiverMode;
#[cfg(unix)]
use std::path::Path;

#[test]
fn receiver_initializer_requires_an_explicit_durable_policy_and_development_profile() {
    let legacy_error = parse_options(["--initialize-peer-record-receiver"].into_iter())
        .expect_err("legacy-compatible must not bootstrap a durable receiver implicitly");
    assert!(legacy_error.contains("requires --p2p-peer-record-reception"));

    let formal_error = parse_options(
        [
            "--initialize-peer-record-receiver",
            "--p2p-peer-record-reception",
            "v2_only",
            "--formal-live-config",
            "formal.json",
        ]
        .into_iter(),
    )
    .expect_err("formal profiles do not construct the development libp2p network");
    assert!(
        formal_error.contains(
            "--initialize-peer-record-receiver cannot be combined with --formal-live-config"
        ),
        "unexpected formal profile rejection: {formal_error}"
    );

    let guarded_error = parse_options(
        [
            "--initialize-peer-record-receiver",
            "--p2p-peer-record-reception",
            "v2_durable_compatible",
            "--guarded-initial-config",
            "guarded.json",
        ]
        .into_iter(),
    )
    .expect_err("guarded profiles do not construct the development libp2p network");
    assert!(
        guarded_error
            .contains("--initialize-peer-record-receiver is inapplicable to guarded engineering"),
        "unexpected guarded profile rejection: {guarded_error}"
    );

    let both_initializers_error = parse_options(
        [
            "--initialize-peer-record-publication",
            "--p2p-peer-record-publication",
            "v2_durable",
            "--initialize-peer-record-receiver",
            "--p2p-peer-record-reception",
            "v2_only",
        ]
        .into_iter(),
    )
    .expect_err("publisher and receiver setup are distinct one-time operations");
    assert!(both_initializers_error.contains("separate one-time operations"));
}

#[test]
fn nonlegacy_receiver_policy_is_rejected_by_formal_and_guarded_host_validation() {
    let formal = CliOptions {
        formal_live_config: Some("formal.json".into()),
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2Only,
        ..CliOptions::default()
    };
    let formal_error = super::cli::validate_effective_options(&formal)
        .expect_err("formal profile has no standard host libp2p receiver");
    assert!(formal_error.contains("requires the standard development host runtime"));

    let guarded = CliOptions {
        guarded_initial_config: Some("guarded.json".into()),
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible,
        ..CliOptions::default()
    };
    let guarded_error = super::cli::validate_effective_options(&guarded)
        .expect_err("guarded profile has no standard host libp2p receiver");
    assert!(guarded_error.contains("requires the standard development host runtime"));
}

#[test]
fn v2_only_receiver_requires_v2_publisher_for_start_but_allows_receiver_only_init() {
    let normal_start = parse_options(["--p2p-peer-record-reception", "v2_only"].into_iter())
        .expect_err("V2-only normal startup cannot publish a local V1 peer record");
    assert!(normal_start.contains("requires durable V2 peer-record publication"));

    let receiver_init = parse_options(
        [
            "--p2p-peer-record-reception",
            "v2_only",
            "--initialize-peer-record-receiver",
        ]
        .into_iter(),
    )
    .expect("receiver-only bootstrap does not start a local publisher");
    assert!(receiver_init.initialize_peer_record_receiver);
    assert_eq!(
        receiver_init.p2p_peer_record_publication,
        oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1::V1Legacy
    );
}

#[test]
#[cfg(unix)]
fn v2_only_receiver_initializer_runs_with_legacy_publisher_without_starting_world() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let scratch_alias = std::env::temp_dir().join(format!(
        "oasis7-peer-record-receiver-only-init-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch_alias).expect("create test scratch directory");
    let scratch = std::fs::canonicalize(&scratch_alias)
        .expect("canonicalize test scratch directory to avoid temp symlink aliases");
    let runtime_root = scratch.join("runtime");
    let config_path = scratch.join("node-config.json");
    let options = parse_options(
        [
            "--runtime-root",
            runtime_root.to_str().expect("UTF-8 runtime path"),
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--p2p-peer-record-publication",
            "v1_legacy",
            "--p2p-peer-record-reception",
            "v2_only",
            "--initialize-peer-record-receiver",
        ]
        .into_iter(),
    )
    .expect("receiver-only bootstrap does not need a V2 publisher configuration");
    let paths = super::resolve_runtime_paths(&options);

    super::run_chain_runtime(options).expect("V2-only receiver initialization should succeed");

    assert!(
        runtime_root
            .join("p2p")
            .join("peer-record-receiver")
            .is_dir(),
        "receiver initializer creates the receiver floor"
    );
    assert!(
        !runtime_root
            .join("p2p")
            .join("peer-record-publication")
            .exists(),
        "receiver-only initialization must not create publisher state"
    );
    assert!(
        !paths.execution_world_dir.exists(),
        "one-time receiver setup exits before starting a World"
    );

    std::fs::remove_dir_all(&scratch).expect("remove isolated receiver initializer fixture");
}

#[test]
fn host_network_keeps_receiver_policy_independent_and_uses_stable_runtime_path() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[9_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let mut options = CliOptions {
        p2p_peer_record_publication:
            oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1::V2Durable,
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible,
        runtime_root: Some(std::env::temp_dir().join("receiver-config-root")),
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&options);
    let config = build_default_replication_network_config(&options, &keypair, &paths)
        .expect("configured network should be constructible");

    assert!(config.peer_record_publication_directory.is_some());
    assert_eq!(
        config.peer_record_receiver.mode,
        PeerRecordReceiverMode::DurableCompatible
    );
    assert_eq!(
        config.peer_record_receiver.directory,
        Some(paths.runtime_root.join("p2p").join("peer-record-receiver"))
    );

    options.p2p_peer_record_publication =
        oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1::V1Legacy;
    options.p2p_peer_record_reception = DevelopmentPeerRecordReceptionModeV1::LegacyCompatible;
    let legacy = build_default_replication_network_config(&options, &keypair, &paths)
        .expect("legacy network should retain a receiver path for downgrade detection");
    assert!(legacy.peer_record_publication_directory.is_none());
    assert_eq!(
        legacy.peer_record_receiver.mode,
        PeerRecordReceiverMode::Legacy
    );
    assert_eq!(
        legacy.peer_record_receiver.directory,
        Some(paths.runtime_root.join("p2p").join("peer-record-receiver"))
    );
}

#[test]
#[cfg(unix)]
fn host_receiver_bootstrap_is_one_time_and_normal_open_fails_closed() {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let scratch_alias = std::env::temp_dir().join(format!(
        "oasis7-peer-record-receiver-v2-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch_alias).expect("create test scratch directory");
    let scratch = std::fs::canonicalize(&scratch_alias)
        .expect("canonicalize test scratch directory to avoid temp symlink aliases");
    let runtime_root = scratch.join("runtime");
    let options = parse_options(
        [
            "--runtime-root",
            runtime_root.to_str().expect("UTF-8 test path"),
            "--config",
            scratch
                .join("config.json")
                .to_str()
                .expect("UTF-8 config path"),
            "--p2p-peer-record-reception",
            "v2_durable_compatible",
            "--initialize-peer-record-receiver",
        ]
        .into_iter(),
    )
    .expect("explicit receiver bootstrap should parse");
    assert!(options.initialize_peer_record_receiver);
    assert_eq!(
        options.p2p_peer_record_reception,
        DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible
    );

    let typed = super::development_runtime::from_options(&options)
        .expect("receiver policy should be present in typed runtime configuration");
    let encoded = serde_json::to_string(&typed).expect("encode host config");
    assert!(
        !encoded.contains("initialize_peer_record_receiver"),
        "one-time initialization intent must not persist into runtime configuration"
    );
    let receiver_directory = runtime_root.join("p2p").join("peer-record-receiver");
    let paths = super::resolve_runtime_paths(&options);

    super::run_chain_runtime(options.clone()).expect("receiver bootstrap should succeed");
    assert!(
        receiver_directory.is_dir(),
        "bootstrap creates receiver state"
    );
    assert!(
        !runtime_root
            .join("p2p")
            .join("peer-record-publication")
            .exists(),
        "receiver bootstrap must not initialize the independent publisher"
    );
    assert!(
        !paths.execution_world_dir.exists(),
        "bootstrap exits before creating or starting a World"
    );
    assert!(
        super::run_chain_runtime(options.clone()).is_err(),
        "repeated receiver initialization must refuse existing state"
    );

    let start_options = CliOptions {
        initialize_peer_record_receiver: false,
        ..options.clone()
    };
    let root_keypair = node_keypair_config::ensure_node_keypair_in_config(Path::new(
        start_options.config_path.as_str(),
    ))
    .expect("reopen stable root keypair");
    let config = build_default_replication_network_config(&start_options, &root_keypair, &paths)
        .expect("host receiver network config should build");
    assert_eq!(
        config.peer_record_receiver.directory,
        Some(receiver_directory.clone())
    );
    let network = create_default_replication_network(&start_options, &root_keypair, &paths)
        .expect("normal startup should open explicitly initialized receiver state");
    assert!(!network.peer_id().to_string().is_empty());
    drop(network);
    std::thread::sleep(Duration::from_millis(100));

    let mut changed_policy = start_options.clone();
    changed_policy.p2p_peer_record_reception = DevelopmentPeerRecordReceptionModeV1::V2Only;
    assert!(
        create_default_replication_network(&changed_policy, &root_keypair, &paths).is_err(),
        "normal open must preserve the receiver policy fixed at initialization"
    );

    let mut legacy_options = start_options.clone();
    legacy_options.p2p_peer_record_reception =
        DevelopmentPeerRecordReceptionModeV1::LegacyCompatible;
    assert!(
        create_default_replication_network(&legacy_options, &root_keypair, &paths).is_err(),
        "standard host must not silently disable receiver protection while its path remains"
    );

    std::fs::remove_file(receiver_directory.join("peer-record-receiver-v2.state.cbor"))
        .expect("remove only the receiver ledger while leaving its directory markers");
    assert!(
        create_default_replication_network(&legacy_options, &root_keypair, &paths).is_err(),
        "a receiver directory with a missing ledger must still prevent legacy downgrade"
    );

    std::fs::remove_dir_all(&receiver_directory).expect("remove receiver state fixture");
    assert!(
        create_default_replication_network(&start_options, &root_keypair, &paths).is_err(),
        "V2 startup must open existing receiver state instead of creating an empty floor"
    );
    assert!(
        !receiver_directory.exists(),
        "failed V2 startup must not recreate the deleted receiver directory"
    );
    std::fs::create_dir(&receiver_directory).expect("leave an empty receiver directory");
    assert!(
        create_default_replication_network(&legacy_options, &root_keypair, &paths).is_err(),
        "even an empty receiver directory must prevent a silent legacy downgrade"
    );

    std::fs::remove_dir_all(&scratch).expect("remove isolated receiver test files");
}

#[test]
#[cfg(not(unix))]
fn durable_receiver_initialization_rejects_unsupported_platforms() {
    let options = parse_options(
        [
            "--p2p-peer-record-reception",
            "v2_only",
            "--initialize-peer-record-receiver",
        ]
        .into_iter(),
    )
    .expect("mode parsing remains available on unsupported platforms");
    let error = super::initialize_default_peer_record_receiver(&options)
        .expect_err("platform without durable filesystem guarantees must reject V2");
    assert!(error.contains("requires Unix filesystem guarantees"));
}

#[test]
fn compatible_receiver_policy_remains_independent_from_legacy_publisher() {
    let options = parse_options(
        [
            "--p2p-peer-record-publication",
            "v1_legacy",
            "--p2p-peer-record-reception",
            "v2_durable_compatible",
        ]
        .into_iter(),
    )
    .expect("compatible receiving can be opted into independently of legacy publishing");
    assert_eq!(
        options.p2p_peer_record_reception,
        DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible
    );
    assert_eq!(
        options.p2p_peer_record_publication,
        oasis7::development_node_config::DevelopmentPeerRecordPublicationModeV1::V1Legacy
    );
    assert_eq!(options.world_id, DEFAULT_WORLD_ID);
}
