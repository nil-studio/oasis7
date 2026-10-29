use super::{
    CliOptions, build_default_replication_network_config, node_keypair_config, parse_options,
};
use oasis7::development_node_config::{
    DevelopmentPeerRecordPublicationModeV1, DevelopmentPeerRecordReceptionModeV1,
};
use oasis7_node::{Libp2pReplicationNetwork, PeerRecordPublicationTransition};
use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, SignedPeerRecord};
use serde::Deserialize;
use std::path::Path;
use std::time::{Duration, Instant};

const PUBLICATION_STATE_FILE: &str = "peer-record-publication-v2.cbor";
const MISSING_RECEIVER_OPEN_ERROR: &str = concat!(
    "receiver directory is missing; restore the original receiver ",
    "state instead of reinitializing",
);
const RECEIVER_BUSY_ERROR: &str = "receiver directory is already in use:";
const PUBLICATION_BUSY_ERROR: &str = "publication directory is already in use:";

fn reopen_after_network_drop<T>(mut open: impl FnMut() -> Result<T, String>) -> T {
    // Drop requests asynchronous swarm shutdown. Only reacquiring its original
    // durable directory leases proves completion; a fixed sleep does not.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match open() {
            Ok(network) => {
                assert!(
                    Instant::now() <= deadline,
                    "network reopen exceeded its fixed deadline"
                );
                return network;
            }
            Err(error) => {
                assert!(
                    error.contains(RECEIVER_BUSY_ERROR) || error.contains(PUBLICATION_BUSY_ERROR),
                    "unexpected restart failure: {error}"
                );
                assert!(
                    Instant::now() < deadline,
                    "network directory owner did not stop: {error}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[derive(Deserialize)]
struct PublisherStateView {
    last_sequence: u64,
    #[serde(default = "live_publication_state")]
    state: PeerRecordPublicationStateV2,
    pending: Option<PendingPublicationView>,
}

#[derive(Deserialize)]
struct PendingPublicationView {
    sequence: u64,
    #[serde(default)]
    signed_record_cbor: Option<Vec<u8>>,
    #[serde(default = "live_publication_state")]
    state: PeerRecordPublicationStateV2,
}

fn live_publication_state() -> PeerRecordPublicationStateV2 {
    PeerRecordPublicationStateV2::Live
}

fn read_publisher_state(directory: &Path) -> PublisherStateView {
    let bytes = std::fs::read(directory.join(PUBLICATION_STATE_FILE))
        .expect("read durable publisher state written by production startup");
    serde_cbor::from_slice(&bytes).expect("decode durable publisher state")
}

fn read_persisted_signed_record(state: &PublisherStateView) -> Option<SignedPeerRecord> {
    let pending = state.pending.as_ref()?;
    serde_cbor::from_slice(pending.signed_record_cbor.as_deref()?).ok()
}

fn durable_options(action: &str) -> CliOptions {
    parse_options(
        [
            "--p2p-peer-record-publication",
            "v2_durable",
            "--p2p-peer-record-reception",
            "v2_durable_compatible",
            action,
        ]
        .into_iter(),
    )
    .expect("explicit durable publisher and receiver transition should parse")
}

#[test]
fn publication_transition_switches_are_ephemeral_and_do_not_roundtrip_as_config() {
    for (action, withdrawing) in [
        ("--withdraw-peer-record-publication", true),
        ("--resume-peer-record-publication", false),
    ] {
        let options = durable_options(action);
        assert_eq!(options.withdraw_peer_record_publication, withdrawing);
        assert_eq!(options.resume_peer_record_publication, !withdrawing);

        let typed = super::development_runtime::from_options(&options)
            .expect("durable publication policy remains persistable");
        let serialized = serde_json::to_value(&typed).expect("serialize typed config");
        let runtime = serialized.get("runtime").expect("runtime config object");
        for transient_field in [
            "withdraw_peer_record_publication",
            "resume_peer_record_publication",
            "peer_record_publication_transition",
        ] {
            assert!(
                runtime.get(transient_field).is_none(),
                "startup action must not persist as {transient_field}"
            );
        }

        let reopened = super::development_runtime::into_options(typed, false)
            .expect("ordinary typed runtime config should reopen without startup action");
        assert!(!reopened.withdraw_peer_record_publication);
        assert!(!reopened.resume_peer_record_publication);
    }
}

#[test]
fn publication_transition_switches_enforce_mode_and_startup_action_boundaries() {
    let legacy_error = parse_options(["--withdraw-peer-record-publication"].into_iter())
        .expect_err("withdrawal requires a durable V2 publisher");
    assert!(legacy_error.contains("require --p2p-peer-record-publication v2_durable"));

    let legacy_receiver_error = parse_options(
        [
            "--p2p-peer-record-publication",
            "v2_durable",
            "--withdraw-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("withdrawal must not be accepted with the legacy receiver");
    assert!(
        legacy_receiver_error
            .contains("require --p2p-peer-record-reception v2_durable_compatible or v2_only")
    );

    let conflict = parse_options(
        [
            "--p2p-peer-record-publication",
            "v2_durable",
            "--withdraw-peer-record-publication",
            "--resume-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("withdrawal and resume cannot be requested together");
    assert!(conflict.contains("conflicting option"));

    let formal_error = parse_options(
        [
            "--formal-live-config",
            "formal.json",
            "--withdraw-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("formal live profile does not construct this development network");
    assert!(formal_error.contains("--withdraw-peer-record-publication cannot be combined"));

    let guarded_error = parse_options(
        [
            "--guarded-initial-config",
            "guarded.json",
            "--resume-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("guarded profile does not construct this development network");
    assert!(guarded_error.contains("--resume-peer-record-publication is inapplicable"));

    let publisher_init_error = parse_options(
        [
            "--p2p-peer-record-publication",
            "v2_durable",
            "--p2p-peer-record-reception",
            "v2_durable_compatible",
            "--initialize-peer-record-publication",
            "--withdraw-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("publisher initialization and withdrawal are separate operations");
    assert!(publisher_init_error.contains("mutually exclusive startup actions"));

    let receiver_init_error = parse_options(
        [
            "--p2p-peer-record-publication",
            "v2_durable",
            "--p2p-peer-record-reception",
            "v2_only",
            "--initialize-peer-record-receiver",
            "--resume-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect_err("receiver initialization and resume are separate operations");
    assert!(receiver_init_error.contains("mutually exclusive startup actions"));
}

#[test]
fn publication_transition_is_passed_to_node_network_config() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[31_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };

    let ordinary = CliOptions {
        p2p_peer_record_publication: DevelopmentPeerRecordPublicationModeV1::V2Durable,
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible,
        ..CliOptions::default()
    };
    let paths = super::resolve_runtime_paths(&ordinary);
    let ordinary_config = build_default_replication_network_config(&ordinary, &keypair, &paths)
        .expect("ordinary durable publisher network config");
    assert_eq!(ordinary_config.peer_record_publication_transition, None);

    for (action, expected) in [
        (
            "--withdraw-peer-record-publication",
            PeerRecordPublicationTransition::Withdraw,
        ),
        (
            "--resume-peer-record-publication",
            PeerRecordPublicationTransition::Resume,
        ),
    ] {
        let options = durable_options(action);
        let paths = super::resolve_runtime_paths(&options);
        let config = build_default_replication_network_config(&options, &keypair, &paths)
            .expect("host config should carry the selected transition");
        assert_eq!(config.peer_record_publication_transition, Some(expected));
    }
}

#[test]
#[cfg(unix)]
fn host_publication_withdrawal_survives_restart_and_resume_publishes_higher_live_sequence() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let scratch_alias = std::env::temp_dir().join(format!(
        "oasis7-peer-record-transition-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&scratch_alias).expect("create test scratch directory");
    let scratch = std::fs::canonicalize(&scratch_alias)
        .expect("canonicalize test scratch directory to its real path");
    let runtime_root = scratch.join("runtime");
    let config_path = scratch.join("config.toml");
    let init_options = parse_options(
        [
            "--runtime-root",
            runtime_root.to_str().expect("UTF-8 runtime path"),
            "--config",
            config_path.to_str().expect("UTF-8 config path"),
            "--p2p-peer-record-publication",
            "v2_durable",
            "--initialize-peer-record-publication",
        ]
        .into_iter(),
    )
    .expect("parse explicit publisher initialization");
    super::run_chain_runtime(init_options.clone())
        .expect("initialize publisher through the standard host entry point");

    let mut ordinary_options = CliOptions {
        initialize_peer_record_publication: false,
        p2p_peer_record_reception: DevelopmentPeerRecordReceptionModeV1::V2DurableCompatible,
        ..init_options
    };
    let root_keypair = node_keypair_config::ensure_node_keypair_in_config(&config_path)
        .expect("read stable identity generated by host initialization");
    let paths = super::resolve_runtime_paths(&ordinary_options);
    let publisher_directory = runtime_root.join("p2p").join("peer-record-publication");
    let receiver_directory = runtime_root.join("p2p").join("peer-record-receiver");
    let initialized = read_publisher_state(&publisher_directory);
    assert_eq!(initialized.last_sequence, 0);
    assert!(initialized.pending.is_none());
    assert!(!receiver_directory.exists());

    ordinary_options.withdraw_peer_record_publication = true;
    let missing_receiver_error =
        super::create_default_replication_network(&ordinary_options, &root_keypair, &paths)
            .err()
            .expect("withdrawal must fail before receiver state has been initialized");
    assert!(
        missing_receiver_error.contains(MISSING_RECEIVER_OPEN_ERROR),
        "the receiver's open-existing path should reject the missing directory: {missing_receiver_error}"
    );
    assert!(!receiver_directory.exists());
    assert!(publisher_directory.is_dir());
    let publisher_before_receiver = read_publisher_state(&publisher_directory);
    assert_eq!(
        publisher_before_receiver.state,
        PeerRecordPublicationStateV2::Live
    );
    assert_eq!(publisher_before_receiver.last_sequence, 0);
    assert!(publisher_before_receiver.pending.is_none());

    ordinary_options.withdraw_peer_record_publication = false;
    let receiver_init_options = CliOptions {
        initialize_peer_record_receiver: true,
        ..ordinary_options.clone()
    };
    super::run_chain_runtime(receiver_init_options)
        .expect("initialize durable receiver as a separate one-time host action");
    assert!(receiver_directory.is_dir());
    ordinary_options.initialize_peer_record_receiver = false;

    ordinary_options.withdraw_peer_record_publication = true;
    let withdrawn_network =
        super::create_default_replication_network(&ordinary_options, &root_keypair, &paths)
            .expect("host-to-node network startup should apply explicit withdrawal");
    drop(withdrawn_network);

    let withdrawn = read_publisher_state(&publisher_directory);
    assert_eq!(withdrawn.state, PeerRecordPublicationStateV2::Tombstone);
    let withdrawn_pending = withdrawn
        .pending
        .as_ref()
        .expect("withdrawal reserves a durable signed record");
    assert_eq!(
        withdrawn_pending.state,
        PeerRecordPublicationStateV2::Tombstone
    );
    let withdrawn_signed =
        read_persisted_signed_record(&withdrawn).expect("withdrawal persists its signed tombstone");
    let withdrawn_publication = withdrawn_signed
        .publication
        .as_ref()
        .expect("withdrawal uses signed V2 publication");
    assert_eq!(
        withdrawn_publication.state,
        PeerRecordPublicationStateV2::Tombstone
    );
    assert!(withdrawn_signed.record.direct_addrs.is_empty());
    assert!(withdrawn_signed.record.hole_punch_addrs.is_empty());
    assert!(withdrawn_signed.record.relay_addrs.is_empty());
    let withdrawn_sequence = withdrawn_pending.sequence;
    assert_eq!(withdrawn_publication.sequence, withdrawn_sequence);

    ordinary_options.withdraw_peer_record_publication = false;
    let restarted_network = reopen_after_network_drop(|| {
        super::create_default_replication_network(&ordinary_options, &root_keypair, &paths)
    });
    std::thread::sleep(Duration::from_millis(150));
    drop(restarted_network);
    let after_restart = read_publisher_state(&publisher_directory);
    assert_eq!(after_restart.state, PeerRecordPublicationStateV2::Tombstone);
    assert_eq!(after_restart.last_sequence, withdrawn_sequence);
    let after_restart_signed = read_persisted_signed_record(&after_restart)
        .expect("ordinary restart keeps the persisted tombstone");
    assert_eq!(
        after_restart_signed, withdrawn_signed,
        "ordinary startup must preserve the exact signed tombstone without advancing it"
    );

    ordinary_options.resume_peer_record_publication = true;
    let mut resume_config =
        build_default_replication_network_config(&ordinary_options, &root_keypair, &paths)
            .expect("host config builder should pass the resume action to node");
    resume_config.republish_interval_ms = 250;
    resume_config.discovery_query_interval_ms = 0;
    resume_config.allow_local_handler_fallback_when_no_peers = true;
    let resumed_network = reopen_after_network_drop(|| {
        Libp2pReplicationNetwork::try_new(resume_config.clone())
            .map_err(|error| format!("{error:?}"))
    });

    let resume_deadline = Instant::now() + Duration::from_secs(3);
    let resumed = loop {
        let state = read_publisher_state(&publisher_directory);
        let signed = read_persisted_signed_record(&state);
        if let Some(signed) = signed
            && state.state == PeerRecordPublicationStateV2::Live
            && signed.publication.as_ref().is_some_and(|publication| {
                publication.state == PeerRecordPublicationStateV2::Live
                    && publication.sequence > withdrawn_sequence
            })
        {
            break (state, signed);
        }
        assert!(
            Instant::now() < resume_deadline,
            "resume should republish a higher-sequence live record through the network event loop"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    // Force a real busy first attempt, independently of scheduler speed.
    // The retry must respect this live owner and resume only after it drops.
    let mut retained_owner = Some(resumed_network.clone());
    drop(resumed_network);

    assert_eq!(resumed.0.state, PeerRecordPublicationStateV2::Live);
    let resumed_publication = resumed
        .1
        .publication
        .as_ref()
        .expect("resumed record uses signed V2 publication");
    assert_eq!(
        resumed_publication.state,
        PeerRecordPublicationStateV2::Live
    );
    assert!(resumed_publication.sequence > withdrawn_sequence);

    ordinary_options.resume_peer_record_publication = false;
    let mut observed_busy_owner = false;
    let ordinary_after_resume = reopen_after_network_drop(|| {
        let opened =
            super::create_default_replication_network(&ordinary_options, &root_keypair, &paths);
        if opened
            .as_ref()
            .is_err_and(|error| error.contains(RECEIVER_BUSY_ERROR))
        {
            observed_busy_owner = true;
            drop(retained_owner.take());
        }
        opened
    });
    assert!(
        observed_busy_owner,
        "a retained original owner must prevent reopening"
    );
    drop(ordinary_after_resume);
    std::thread::sleep(Duration::from_millis(150));
    let reopened = read_publisher_state(&publisher_directory);
    let reopened_signed = read_persisted_signed_record(&reopened)
        .expect("resumed live publication persists across normal restart");
    assert_eq!(reopened.state, PeerRecordPublicationStateV2::Live);
    let reopened_publication = reopened_signed
        .publication
        .as_ref()
        .expect("normal reopening keeps V2 publication");
    assert_eq!(
        reopened_publication.state,
        PeerRecordPublicationStateV2::Live
    );
    assert!(reopened_publication.sequence >= resumed_publication.sequence);
    assert_eq!(reopened.last_sequence, reopened_publication.sequence);
    assert_eq!(
        reopened_signed.identity_public_key_protobuf_hex,
        resumed.1.identity_public_key_protobuf_hex
    );
    assert_eq!(reopened_signed.record.peer_id, resumed.1.record.peer_id);
    assert_eq!(reopened_signed.record.world_id, resumed.1.record.world_id);
    assert_eq!(
        reopened_signed.record.network_id,
        resumed.1.record.network_id
    );

    std::fs::remove_dir_all(scratch).expect("remove test scratch directory");
}
