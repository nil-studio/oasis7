use super::DurablePeerRecordPublisher;
use libp2p::{PeerId, identity::Keypair};
use oasis7_proto::distributed_dht::{
    PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass, PeerRecord,
    PeerRecordPublicationStateV2, PeerRecordPublicationV2,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[path = "durable_peer_record_publisher/tombstone_tests.rs"]
mod tombstone_tests;

#[path = "durable_peer_record_publisher/initialization_race_tests.rs"]
mod initialization_race_tests;

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let temp_dir = std::fs::canonicalize(std::env::temp_dir())
            .expect("resolve physical temporary directory");
        let path = temp_dir.join(format!(
            "oasis7-peer-publication-v2-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create test directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict test directory permissions");
        }
        Self(path)
    }

    fn initialize(&self, keypair: &Keypair, template: &PeerRecord) {
        DurablePeerRecordPublisher::initialize(&self.0, keypair, template)
            .expect("explicitly initialize publisher");
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct InitializingIntentV2<'a> {
    schema_version: u16,
    status: &'a str,
    binding: super::PublisherBindingV2,
}

#[derive(Serialize)]
struct FutureInitializationIntentV2<'a> {
    schema_version: u16,
    status: &'a str,
    binding: super::PublisherBindingV2,
    future_contract: &'a str,
}

#[derive(Serialize)]
struct FuturePublisherStateV2<'a> {
    schema_version: u16,
    binding: super::PublisherBindingV2,
    last_sequence: u64,
    pending: Option<super::PendingPublicationV2>,
    future_contract: &'a str,
}

#[derive(Clone, Copy)]
enum BootstrapPhase {
    IntentOnly,
    StableLockCreated,
    StateTempPersisted,
    StateTempPartial,
    ZeroStatePersisted,
    ReadyMarkerPrefix,
    ReadyPersisted,
    CorruptMarker,
}

fn intent_path(directory: &TestDirectory) -> PathBuf {
    directory.0.join(super::initialization::INTENT_FILE_NAME)
}

fn write_initializing_intent(
    directory: &TestDirectory,
    keypair: &Keypair,
    template: &PeerRecord,
) -> Vec<u8> {
    let bytes = encoded_initializing_intent(keypair, template);
    write_private_file(&intent_path(directory), &bytes);
    bytes
}

fn encoded_initializing_intent(keypair: &Keypair, template: &PeerRecord) -> Vec<u8> {
    let (_, binding) = super::publisher_binding(template, keypair).expect("publisher binding");
    super::canonical_cbor(&InitializingIntentV2 {
        schema_version: super::STATE_SCHEMA_VERSION,
        status: "initializing",
        binding,
    })
    .expect("encode canonical initialization intent")
}

fn write_private_file(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write private test file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("restrict test file permissions");
    }
}

fn install_interrupted_bootstrap(
    directory: &TestDirectory,
    keypair: &Keypair,
    template: &PeerRecord,
    phase: BootstrapPhase,
) {
    write_initializing_intent(directory, keypair, template);
    if matches!(phase, BootstrapPhase::IntentOnly) {
        return;
    }

    let lock_path = directory.0.join(super::LOCK_FILE_NAME);
    write_private_file(&lock_path, &[]);
    if matches!(phase, BootstrapPhase::StableLockCreated) {
        return;
    }

    let (_, binding) = super::publisher_binding(template, keypair).expect("publisher binding");
    let state = super::PublisherStateV2 {
        schema_version: super::STATE_SCHEMA_VERSION,
        binding,
        last_sequence: 0,
        pending: None,
        state: PeerRecordPublicationStateV2::Live,
    };
    let state_bytes = super::canonical_cbor(&state).expect("encode initial zero state");
    if matches!(phase, BootstrapPhase::StateTempPersisted) {
        write_private_file(&directory.0.join(super::TEMP_FILE_NAME), &state_bytes);
        return;
    }
    if matches!(phase, BootstrapPhase::StateTempPartial) {
        write_private_file(
            &directory.0.join(super::TEMP_FILE_NAME),
            &state_bytes[..state_bytes.len().min(7)],
        );
        return;
    }
    write_private_file(&directory.0.join(super::STATE_FILE_NAME), &state_bytes);
    if matches!(phase, BootstrapPhase::ZeroStatePersisted) {
        return;
    }

    match phase {
        BootstrapPhase::ReadyMarkerPrefix => {
            write_private_file(&lock_path, &super::LOCK_INITIALIZED_MARKER[..8])
        }
        BootstrapPhase::CorruptMarker => write_private_file(&lock_path, b"not-a-marker"),
        _ => write_private_file(&lock_path, super::LOCK_INITIALIZED_MARKER),
    }
}

#[test]
fn explicit_initializer_resumes_every_unissued_bootstrap_phase() {
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    for phase in [
        BootstrapPhase::IntentOnly,
        BootstrapPhase::StableLockCreated,
        BootstrapPhase::StateTempPersisted,
        BootstrapPhase::StateTempPartial,
        BootstrapPhase::ZeroStatePersisted,
        BootstrapPhase::ReadyMarkerPrefix,
        BootstrapPhase::ReadyPersisted,
    ] {
        let directory = TestDirectory::new();
        install_interrupted_bootstrap(&directory, &keypair, &template, phase);
        let intent_path = intent_path(&directory);
        assert!(
            intent_path.exists(),
            "fixture must contain the durable intent"
        );

        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template)
            .expect("same-binding explicit initialization should resume unissued state0");
        assert!(
            !intent_path.exists(),
            "successful resume must durably close the initialization intent"
        );
        let publisher = DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
            .expect("resumed state must open normally");
        assert_eq!(publisher.state.last_sequence, 0);
        drop(publisher);
    }
}

#[test]
fn initializer_rejects_nonprefix_lock_marker_corruption() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    install_interrupted_bootstrap(
        &directory,
        &keypair,
        &template,
        BootstrapPhase::CorruptMarker,
    );
    let lock_before =
        std::fs::read(directory.0.join(super::LOCK_FILE_NAME)).expect("read corrupt marker");
    let state_before =
        std::fs::read(directory.0.join(super::STATE_FILE_NAME)).expect("read verified state0");

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "a non-prefix marker mutation is corruption, not a recoverable write interruption"
    );
    assert_eq!(
        std::fs::read(directory.0.join(super::LOCK_FILE_NAME)).expect("retain marker"),
        lock_before
    );
    assert_eq!(
        std::fs::read(directory.0.join(super::STATE_FILE_NAME)).expect("retain state0"),
        state_before
    );
}

#[test]
fn initializer_rejects_corrupt_or_wrong_scope_intent_without_creating_state() {
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    for corrupt in [false, true] {
        let directory = TestDirectory::new();
        let intent_path = intent_path(&directory);
        let original = if corrupt {
            let bytes = b"not canonical CBOR".to_vec();
            write_private_file(&intent_path, &bytes);
            bytes
        } else {
            let (_, mut binding) =
                super::publisher_binding(&template, &keypair).expect("publisher binding");
            binding.world_id.push_str("-foreign");
            let bytes = super::canonical_cbor(&InitializingIntentV2 {
                schema_version: super::STATE_SCHEMA_VERSION,
                status: "initializing",
                binding,
            })
            .expect("encode foreign intent");
            write_private_file(&intent_path, &bytes);
            bytes
        };

        assert!(
            DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
            "invalid initialization intent must fail closed"
        );
        assert_eq!(
            std::fs::read(&intent_path).expect("retain rejected intent"),
            original
        );
        assert!(
            !directory.0.join(super::STATE_FILE_NAME).exists(),
            "rejected intent must not create a zero-sequence state"
        );
        assert!(
            !directory.0.join(super::LOCK_FILE_NAME).exists(),
            "rejected intent must not leave a permanent publisher lock"
        );
    }
}

#[test]
fn initializer_rejects_complete_future_intent_temp_without_replacing_it() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    let (_, binding) = super::publisher_binding(&template, &keypair).expect("publisher binding");
    let future_bytes = super::canonical_cbor(&FutureInitializationIntentV2 {
        schema_version: super::STATE_SCHEMA_VERSION + 1,
        status: "initializing",
        binding,
        future_contract: "must-not-be-downgraded",
    })
    .expect("encode complete future intent");
    let temp_path = directory
        .0
        .join(super::initialization::INTENT_TEMP_FILE_NAME);
    write_private_file(&temp_path, &future_bytes);

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "a complete unsupported intent is not a crash-truncated intent"
    );
    assert_eq!(
        std::fs::read(&temp_path).expect("retain future intent temp"),
        future_bytes,
        "initializer must not overwrite an intent from a newer schema"
    );
    assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
    assert!(!directory.0.join(super::LOCK_FILE_NAME).exists());
    assert!(!intent_path(&directory).exists());
}

#[test]
fn initializer_resumes_exact_prefix_of_current_intent_temp() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    let expected = encoded_initializing_intent(&keypair, &template);
    let temp_path = directory
        .0
        .join(super::initialization::INTENT_TEMP_FILE_NAME);
    write_private_file(&temp_path, &expected[..expected.len() / 2]);

    DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template)
        .expect("resume a torn write that is an exact prefix of this initialization");
    assert!(!temp_path.exists());
    assert!(!intent_path(&directory).exists());
    let publisher = DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
        .expect("open state completed from exact intent prefix");
    assert_eq!(publisher.state.last_sequence, 0);
}

#[test]
fn initializer_rejects_partial_future_intent_after_first_schema_difference() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    let expected = encoded_initializing_intent(&keypair, &template);
    let (_, binding) = super::publisher_binding(&template, &keypair).expect("publisher binding");
    let future = super::canonical_cbor(&FutureInitializationIntentV2 {
        schema_version: super::STATE_SCHEMA_VERSION + 1,
        status: "initializing",
        binding,
        future_contract: "future",
    })
    .expect("encode future initialization contract");
    let first_difference = expected
        .iter()
        .zip(&future)
        .position(|(expected, actual)| expected != actual)
        .expect("future schema differs from the current intent");
    let partial_future = &future[..=first_difference];
    assert!(!expected.starts_with(partial_future));

    let temp_path = directory
        .0
        .join(super::initialization::INTENT_TEMP_FILE_NAME);
    write_private_file(&temp_path, partial_future);

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "a future-version prefix that already differs from current bytes must fail closed"
    );
    assert_eq!(
        std::fs::read(&temp_path)
            .expect("retain future prefix")
            .as_slice(),
        partial_future
    );
    assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
    assert!(!directory.0.join(super::LOCK_FILE_NAME).exists());
}

#[test]
fn initializer_rejects_complete_future_state_temp_without_removing_it() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    write_initializing_intent(&directory, &keypair, &template);
    let (_, binding) = super::publisher_binding(&template, &keypair).expect("publisher binding");
    write_private_file(&directory.0.join(super::LOCK_FILE_NAME), &[]);
    let future_bytes = super::canonical_cbor(&FuturePublisherStateV2 {
        schema_version: super::STATE_SCHEMA_VERSION + 1,
        binding,
        last_sequence: 0,
        pending: None,
        future_contract: "must-not-be-downgraded",
    })
    .expect("encode complete future state");
    let temp_path = directory.0.join(super::TEMP_FILE_NAME);
    write_private_file(&temp_path, &future_bytes);

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "a complete unsupported state temp is not a crash-truncated state"
    );
    assert_eq!(
        std::fs::read(&temp_path).expect("retain future state temp"),
        future_bytes,
        "initializer must not delete a future state it cannot decode"
    );
    assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
    assert_eq!(
        std::fs::read(directory.0.join(super::LOCK_FILE_NAME)).expect("retain lock marker"),
        b""
    );
}

#[test]
fn initializer_rejects_partial_future_state_after_first_schema_difference() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    write_initializing_intent(&directory, &keypair, &template);
    let (_, binding) = super::publisher_binding(&template, &keypair).expect("publisher binding");
    write_private_file(&directory.0.join(super::LOCK_FILE_NAME), &[]);
    let expected_state = super::PublisherStateV2 {
        schema_version: super::STATE_SCHEMA_VERSION,
        binding: binding.clone(),
        last_sequence: 0,
        pending: None,
        state: PeerRecordPublicationStateV2::Live,
    };
    let expected = super::canonical_cbor(&expected_state).expect("encode zero state");
    let future = super::canonical_cbor(&FuturePublisherStateV2 {
        schema_version: super::STATE_SCHEMA_VERSION + 1,
        binding,
        last_sequence: 0,
        pending: None,
        future_contract: "future",
    })
    .expect("encode future state");
    let first_difference = expected
        .iter()
        .zip(&future)
        .position(|(expected, actual)| expected != actual)
        .expect("future schema differs from the current state");
    let partial_future = &future[..=first_difference];
    assert!(!expected.starts_with(partial_future));
    let temp_path = directory.0.join(super::TEMP_FILE_NAME);
    write_private_file(&temp_path, partial_future);

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "a future state prefix that already differs from current bytes must fail closed"
    );
    assert_eq!(
        std::fs::read(&temp_path)
            .expect("retain future state prefix")
            .as_slice(),
        partial_future
    );
    assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
    assert_eq!(
        std::fs::read(directory.0.join(super::LOCK_FILE_NAME)).expect("retain lock marker"),
        b""
    );
}

#[test]
fn initializer_rejects_corrupt_nonprefix_intent_and_state_temps() {
    for state_temp in [false, true] {
        let directory = TestDirectory::new();
        let keypair = Keypair::generate_ed25519();
        let template = sample_record(&keypair, 0);
        let temp_path = if state_temp {
            write_initializing_intent(&directory, &keypair, &template);
            write_private_file(&directory.0.join(super::LOCK_FILE_NAME), &[]);
            directory.0.join(super::TEMP_FILE_NAME)
        } else {
            directory
                .0
                .join(super::initialization::INTENT_TEMP_FILE_NAME)
        };
        let corrupt = b"complete but unrelated bytes";
        write_private_file(&temp_path, corrupt);

        assert!(
            DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
            "non-prefix corruption must not be treated as a torn write"
        );
        assert_eq!(
            std::fs::read(&temp_path)
                .expect("retain corrupt temp")
                .as_slice(),
            corrupt
        );
        assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
        if state_temp {
            assert_eq!(
                std::fs::read(directory.0.join(super::LOCK_FILE_NAME))
                    .expect("retain empty marker"),
                b""
            );
        } else {
            assert!(!directory.0.join(super::LOCK_FILE_NAME).exists());
        }
    }
}

#[test]
fn ready_marker_without_state_cannot_be_resumed_from_an_initialization_intent() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    write_initializing_intent(&directory, &keypair, &template);
    write_private_file(
        &directory.0.join(super::LOCK_FILE_NAME),
        super::LOCK_INITIALIZED_MARKER,
    );
    let intent_path = intent_path(&directory);
    let intent_before = std::fs::read(&intent_path).expect("read intent");

    assert!(
        DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err(),
        "Ready plus a missing state is evidence of loss, not an unissued bootstrap phase"
    );
    assert!(!directory.0.join(super::STATE_FILE_NAME).exists());
    assert_eq!(
        std::fs::read(intent_path).expect("retain intent"),
        intent_before
    );
}

#[test]
fn duplicate_initializer_while_publisher_is_open_leaves_no_intent() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");

    assert!(DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err());
    assert!(
        !directory
            .0
            .join(super::initialization::INTENT_FILE_NAME)
            .exists(),
        "duplicate init must not leave an intent that blocks the live publisher"
    );
    drop(publisher);
    DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
        .expect("normal reopen remains available after rejected duplicate init");
}

#[test]
fn concurrent_initializers_publish_one_locked_intent_without_leaving_a_blocker() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let path = directory.0.clone();
    let first_barrier = std::sync::Arc::clone(&barrier);
    let first_key = keypair.clone();
    let first_template = template.clone();
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        DurablePeerRecordPublisher::initialize(&path, &first_key, &first_template)
    });
    let path = directory.0.clone();
    let second_barrier = std::sync::Arc::clone(&barrier);
    let second_key = keypair.clone();
    let second_template = template.clone();
    let second = std::thread::spawn(move || {
        second_barrier.wait();
        DurablePeerRecordPublisher::initialize(&path, &second_key, &second_template)
    });
    let results = [
        first.join().expect("first initializer thread"),
        second.join().expect("second initializer thread"),
    ];
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "one concurrent initializer must own and complete the bootstrap"
    );
    assert!(
        !intent_path(&directory).exists(),
        "completed concurrent initialization must remove its intent"
    );
    DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
        .expect("completed initialization remains available to ordinary open");
}

#[test]
fn stale_initialization_temp_fd_cannot_publish_over_replacement_inode() {
    use std::io::Write as _;

    let directory = TestDirectory::new();
    let temp_path = directory
        .0
        .join(super::initialization::INTENT_TEMP_FILE_NAME);
    let intent_path = intent_path(&directory);
    let stale = create_private_test_file(&temp_path);
    stale.try_lock().expect("lock initial temp inode");
    std::fs::remove_file(&temp_path).expect("simulate unlinking locked temp path");

    let mut replacement = create_private_test_file(&temp_path);
    replacement.try_lock().expect("lock replacement temp inode");
    replacement
        .write_all(b"replacement initializer owns this inode")
        .expect("write replacement temp");
    replacement.sync_all().expect("persist replacement temp");
    super::initialization::publish_intent_no_replace(
        &replacement,
        &temp_path,
        &intent_path,
        &directory.0,
    )
    .expect("replacement owner atomically publishes its intent");
    let published = std::fs::read(&intent_path).expect("read published intent");

    assert!(
        super::initialization::ensure_file_is_current_path(&stale, &temp_path).is_err(),
        "a locked descriptor for an unlinked inode must fail the path identity check"
    );
    assert!(
        super::initialization::publish_intent_no_replace(
            &stale,
            &temp_path,
            &intent_path,
            &directory.0,
        )
        .is_err(),
        "a stale descriptor must never publish the replacement path"
    );
    assert_eq!(
        std::fs::read(intent_path).expect("retain current intent"),
        published
    );
}

fn create_private_test_file(path: &std::path::Path) -> std::fs::File {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).expect("create test temp inode")
}

#[cfg(unix)]
#[test]
fn initializer_rejects_a_symlinked_ancestor_before_creating_publication_state() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let real_parent = directory.0.join("real-parent");
    std::fs::create_dir(&real_parent).expect("create real parent");
    std::fs::set_permissions(&real_parent, std::fs::Permissions::from_mode(0o700))
        .expect("restrict real parent");
    let symlink_parent = directory.0.join("parent-link");
    std::os::unix::fs::symlink(&real_parent, &symlink_parent).expect("create parent symlink");
    let target = symlink_parent.join("publication");
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);

    assert!(DurablePeerRecordPublisher::initialize(&target, &keypair, &template).is_err());
    assert!(
        !target.exists(),
        "initializer must reject a symlinked ancestor before writing under its target"
    );
    assert!(
        !real_parent.join("publication").exists(),
        "initializer must not create state beneath the symlink target"
    );
}

fn sample_record(keypair: &Keypair, published_at_ms: i64) -> PeerRecord {
    let peer_id = PeerId::from(keypair.public());
    PeerRecord {
        peer_id: peer_id.to_string(),
        node_id: "node-publication-v2".to_string(),
        world_id: "world-publication-v2".to_string(),
        network_id: "network-publication-v2".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: PeerReachabilityClass::Hybrid,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![PeerDiscoverySource::Dht],
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: crate::util::unix_now_ms_i64().saturating_add(published_at_ms),
        ttl_ms: 60_000,
    }
}

fn request_local_record_from_handler(
    template: &PeerRecord,
    keypair: &Keypair,
    local_publisher: &Arc<Mutex<super::LocalPeerRecordPublisher>>,
) -> Result<Vec<u8>, crate::error::WorldError> {
    let listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let reachability = Arc::new(Mutex::new(
        super::super::reachability::Libp2pReachabilitySnapshot::default(),
    ));
    let cache = super::super::peer_record_cache::PeerRecordCache::default();
    super::super::discovery::handle_request_response_request_with_publisher(
        &oasis7_proto::distributed_net::NetworkRequest {
            protocol: "/aw/rr/1.0.0/get_local_peer_record".into(),
            payload: Vec::new(),
        },
        &std::collections::HashMap::new(),
        Some(template),
        keypair,
        &listening_addrs,
        &reachability,
        false,
        &cache,
        local_publisher,
    )
}

#[test]
fn remote_local_record_request_without_a_persisted_publication_is_unavailable() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let publisher =
        super::LocalPeerRecordPublisher::open(Some(&directory.0), &keypair, Some(&template))
            .expect("open initialized publisher");
    let local_publisher = Arc::new(Mutex::new(publisher));
    let state_path = directory.0.join(super::STATE_FILE_NAME);
    let before = std::fs::read(&state_path).expect("read initial state");

    let response = request_local_record_from_handler(&template, &keypair, &local_publisher);
    assert!(
        response.is_err(),
        "remote query must not issue the first record"
    );
    assert_eq!(
        std::fs::read(state_path).expect("read state after query"),
        before
    );
}

#[test]
fn remote_local_record_request_does_not_replace_an_expired_publication() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let mut template = sample_record(&keypair, 0);
    template.ttl_ms = 1_000;
    directory.initialize(&keypair, &template);
    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
    let mut initial = template.clone();
    initial.published_at_ms = crate::util::unix_now_ms_i64();
    let signed = publisher
        .publish_materialized(&initial)
        .expect("publish record before expiry");
    assert_eq!(signed.publication.expect("V2 metadata").sequence, 1);
    let local_publisher = Arc::new(Mutex::new(super::LocalPeerRecordPublisher::Durable(
        Box::new(publisher),
    )));
    let state_path = directory.0.join(super::STATE_FILE_NAME);
    let before = std::fs::read(&state_path).expect("read signed state");
    std::thread::sleep(std::time::Duration::from_millis(1_100));

    let response = request_local_record_from_handler(&template, &keypair, &local_publisher);
    assert!(
        response.is_err(),
        "remote query must not renew expired signed bytes"
    );
    assert_eq!(
        std::fs::read(state_path).expect("read state after query"),
        before
    );
}

#[test]
fn remote_local_record_request_rejects_persisted_addresses_that_are_no_longer_current() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
    let mut previously_published = template.clone();
    previously_published.direct_addrs = vec!["/ip4/8.8.8.8/tcp/4001".into()];
    previously_published.published_at_ms = crate::util::unix_now_ms_i64();
    publisher
        .publish_materialized(&previously_published)
        .expect("persist previous address declaration");
    let local_publisher = Arc::new(Mutex::new(super::LocalPeerRecordPublisher::Durable(
        Box::new(publisher),
    )));
    let state_path = directory.0.join(super::STATE_FILE_NAME);
    let before = std::fs::read(&state_path).expect("read persisted declaration");

    let response = request_local_record_from_handler(&template, &keypair, &local_publisher);
    assert!(
        response.is_err(),
        "remote query must not return a signed record for an address no longer advertised locally"
    );
    assert_eq!(
        std::fs::read(state_path).expect("state after address mismatch"),
        before,
        "address mismatch must not mutate publisher state"
    );
}

#[test]
fn durable_publication_sequence_increments_after_restart() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let first = {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        publisher
            .publish_materialized(&sample_record(&keypair, 100))
            .expect("first publication")
    };
    assert_eq!(first.publication.expect("V2 metadata").sequence, 1);

    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    let next = publisher
        .publish_materialized(&sample_record(&keypair, 101))
        .expect("publication after restart");
    assert_eq!(next.publication.expect("V2 metadata").sequence, 2);
}

#[test]
fn ordinary_open_requires_explicit_initialization_and_initializer_is_one_shot() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());
    assert_eq!(
        std::fs::read_dir(&directory.0)
            .expect("list untouched directory")
            .count(),
        0,
        "ordinary open must not create a lock, marker, or zero-sequence state"
    );

    directory.initialize(&keypair, &template);
    assert!(DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err());
}

#[test]
fn loss_of_the_entire_publication_directory_must_not_reset_sequence() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        let first = publisher
            .publish_materialized(&sample_record(&keypair, 100))
            .expect("publish before simulated data loss");
        assert_eq!(first.publication.expect("V2 metadata").sequence, 1);
    }

    std::fs::remove_dir_all(&directory.0).expect("simulate loss of all local publication state");
    let error = match DurablePeerRecordPublisher::open(&directory.0, &keypair, &template) {
        Ok(_) => panic!("ordinary open must not recreate publisher identity at sequence zero"),
        Err(error) => error,
    };
    assert!(
        format!("{error:?}").contains("restore the original publication state"),
        "missing-state error should direct operators to restore the original publisher"
    );
    assert!(
        !directory.0.exists(),
        "ordinary open must leave a lost publication directory absent"
    );
}

#[test]
fn durable_publication_reservation_survives_crash_before_signing() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let materialized = sample_record(&keypair, 100);
    {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        assert_eq!(
            publisher
                .reserve_materialized_record(&materialized)
                .expect("persist reservation"),
            1
        );
    }

    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    let recovered = publisher
        .publish_materialized(&materialized)
        .expect("sign exact reserved record after restart");
    assert_eq!(recovered.record, materialized);
    assert_eq!(recovered.publication.expect("V2 metadata").sequence, 1);
}

#[test]
fn durable_signed_result_survives_crash_before_network_send() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let materialized = sample_record(&keypair, 100);
    let original = {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        publisher
            .publish_materialized(&materialized)
            .expect("persist signed result")
    };

    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    let recovered = publisher
        .publish_materialized(&materialized)
        .expect("reuse persisted signed result");
    assert_eq!(
        crate::util::to_canonical_cbor(&recovered).expect("recovered bytes"),
        crate::util::to_canonical_cbor(&original).expect("original bytes")
    );

    let next = publisher
        .publish_materialized(&sample_record(&keypair, 101))
        .expect("new publication supersedes uncertain old send");
    assert_eq!(next.publication.expect("V2 metadata").sequence, 2);
}

#[test]
fn new_materialized_payload_after_unfinished_reservation_consumes_a_sequence_gap() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        publisher
            .reserve_materialized_record(&sample_record(&keypair, 100))
            .expect("persist reservation");
    }

    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    let next = publisher
        .publish_materialized(&sample_record(&keypair, 101))
        .expect("allocate a fresh sequence for a new payload");
    assert_eq!(next.publication.expect("V2 metadata").sequence, 2);
}

#[test]
fn durable_publication_lock_refuses_a_second_owner_and_releases_on_drop() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let first =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("first owner");
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());
    drop(first);
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_ok());
}

#[test]
fn durable_publication_rejects_identity_scope_changes_and_missing_state() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    {
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
            .expect("initialize publisher");
    }

    let mut changed_template = template.clone();
    changed_template.world_id = "different-world".to_string();
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &changed_template).is_err());

    let mut changed_network = template.clone();
    changed_network.network_id = "different-network".to_string();
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &changed_network).is_err());

    let other_keypair = Keypair::generate_ed25519();
    assert!(DurablePeerRecordPublisher::open(&directory.0, &other_keypair, &template).is_err());

    std::fs::remove_file(directory.0.join(super::STATE_FILE_NAME)).expect("remove state");
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());

    let marker_directory = TestDirectory::new();
    marker_directory.initialize(&keypair, &template);
    std::fs::write(marker_directory.0.join(super::LOCK_FILE_NAME), b"")
        .expect("remove initialized marker");
    assert!(DurablePeerRecordPublisher::open(&marker_directory.0, &keypair, &template).is_err());
}

#[test]
fn expired_pending_record_is_not_replayed_but_fresh_intent_gets_a_new_sequence() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let mut template = sample_record(&keypair, 0);
    template.ttl_ms = 1_000;
    directory.initialize(&keypair, &template);
    let expiring = template.clone();
    let original = {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
        publisher
            .publish_materialized(&expiring)
            .expect("persist expiring signed result")
    };
    assert_eq!(original.publication.expect("V2 metadata").sequence, 1);

    std::thread::sleep(std::time::Duration::from_millis(1_100));
    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    assert!(
        publisher.publish_materialized(&expiring).is_err(),
        "expired pending signed bytes must not be returned after restart"
    );
    let mut fresh_record = template;
    fresh_record.published_at_ms = crate::util::unix_now_ms_i64();
    let fresh = publisher
        .publish_materialized(&fresh_record)
        .expect("fresh intent supersedes expired pending state");
    assert_eq!(fresh.publication.expect("V2 metadata").sequence, 2);
}

#[test]
fn durable_publisher_allows_template_updates_within_fixed_identity_scope() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let original_template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &original_template);
    let first = {
        let mut publisher =
            DurablePeerRecordPublisher::open(&directory.0, &keypair, &original_template)
                .expect("open original template");
        publisher
            .publish_materialized(&sample_record(&keypair, 10))
            .expect("publish with original template")
    };
    assert_eq!(first.publication.expect("V2 metadata").sequence, 1);

    let mut updated_template = original_template;
    updated_template.ttl_ms = 120_000;
    let mut updated_record = updated_template.clone();
    updated_record.published_at_ms = crate::util::unix_now_ms_i64().saturating_add(20);
    let mut publisher = DurablePeerRecordPublisher::open(&directory.0, &keypair, &updated_template)
        .expect("same identity scope may adopt an updated template");
    let updated = publisher
        .publish_materialized(&updated_record)
        .expect("updated template receives a new publication sequence");
    assert_eq!(updated.record.ttl_ms, 120_000);
    assert_eq!(updated.publication.expect("V2 metadata").sequence, 2);
}

#[test]
fn durable_publication_rejects_corrupt_state_and_sequence_overflow() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    {
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
            .expect("initialize publisher");
    }
    std::fs::write(directory.0.join(super::STATE_FILE_NAME), b"not cbor").expect("corrupt state");
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());

    let overflow_directory = TestDirectory::new();
    overflow_directory.initialize(&keypair, &template);
    {
        let mut publisher =
            DurablePeerRecordPublisher::open(&overflow_directory.0, &keypair, &template)
                .expect("initialize overflow fixture");
        publisher
            .publish_materialized(&sample_record(&keypair, 100))
            .expect("initial publication");
    }
    let state_path = overflow_directory.0.join(super::STATE_FILE_NAME);
    let bytes = std::fs::read(&state_path).expect("read state");
    let mut state: super::PublisherStateV2 = serde_cbor::from_slice(&bytes).expect("decode state");
    let pending = state.pending.as_mut().expect("pending signed record");
    let materialized: PeerRecord =
        serde_cbor::from_slice(&pending.materialized_record_cbor).expect("decode record");
    let signed = super::super::peer_record::sign_peer_record_v2(
        &materialized,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: u64::MAX,
            state: PeerRecordPublicationStateV2::Live,
        },
        &keypair,
    )
    .expect("sign maximum sequence fixture");
    pending.sequence = u64::MAX;
    pending.signed_record_cbor =
        Some(crate::util::to_canonical_cbor(&signed).expect("encode signed fixture"));
    state.last_sequence = u64::MAX;
    std::fs::write(
        state_path,
        crate::util::to_canonical_cbor(&state).expect("encode overflow state"),
    )
    .expect("persist overflow state");

    let mut publisher =
        DurablePeerRecordPublisher::open(&overflow_directory.0, &keypair, &template)
            .expect("load valid maximum sequence");
    assert!(
        publisher
            .publish_materialized(&sample_record(&keypair, 101))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn durable_publication_rejects_symlinked_state_and_public_directory_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    {
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template)
            .expect("initialize publisher");
    }
    let state = directory.0.join(super::STATE_FILE_NAME);
    let target = directory.0.join("elsewhere");
    std::fs::rename(&state, &target).expect("move state");
    symlink(&target, &state).expect("symlink state");
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());

    std::fs::remove_file(&state).expect("remove symlink");
    std::fs::rename(&target, &state).expect("restore state");
    std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o755))
        .expect("make directory public");
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());
}

#[cfg(not(unix))]
#[test]
fn durable_publication_v2_fails_closed_on_unsupported_platforms() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    assert!(DurablePeerRecordPublisher::initialize(&directory.0, &keypair, &template).is_err());
    assert!(DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).is_err());
}
