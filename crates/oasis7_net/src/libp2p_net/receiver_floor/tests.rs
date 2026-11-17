#![cfg(all(test, unix))]

use super::*;
use libp2p::{PeerId, identity::Keypair};
use oasis7_proto::distributed_dht::{
    PeerDeploymentMode, PeerNodeRole, PeerRecord, PeerRecordPublicationStateV2,
    PeerRecordPublicationV2, SignedPeerRecord,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use super::super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};

const STATE_FILE: &str = "peer-record-receiver-v2.state.cbor";
const STATE_TEMP_FILE: &str = "peer-record-receiver-v2.state.cbor.tmp";

struct Fixture {
    _root: tempfile::TempDir,
    directory: PathBuf,
    config: PeerRecordReceiverConfig,
    local_key: Keypair,
    template: PeerRecord,
}

fn fixture() -> Fixture {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir_in(
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root"),
    )
    .expect("receiver test root");
    let directory = root.path().join("receiver");
    fs::create_dir(&directory).expect("create receiver directory");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .expect("make receiver directory private");
    let local_key = Keypair::generate_ed25519();
    let template = record_body(&local_key, "local-template");
    let config = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::DurableCompatible,
        directory: Some(directory.clone()),
    };
    PeerRecordReceiver::initialize(&config, &local_key, &template)
        .expect("initialize receiver floor");
    Fixture {
        _root: root,
        directory,
        config,
        local_key,
        template,
    }
}

fn record_body(key: &Keypair, node_id: &str) -> PeerRecord {
    PeerRecord {
        peer_id: PeerId::from(key.public()).to_string(),
        node_id: node_id.into(),
        world_id: "world-a".into(),
        network_id: "network-a".into(),
        node_role: PeerNodeRole::FullStorage.as_str().into(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
        direct_addrs: vec!["/ip4/127.0.0.1/udp/4103/quic-v1".into()],
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: Vec::new(),
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: crate::util::unix_now_ms_i64(),
        ttl_ms: 60_000,
    }
}

fn signed(key: &Keypair, sequence: u64, node_id: &str) -> SignedPeerRecord {
    let record = record_body(key, node_id);
    super::super::peer_record::sign_peer_record_v2(
        &record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence,
            state: PeerRecordPublicationStateV2::Live,
        },
        key,
    )
    .expect("sign V2 peer record")
}

fn open(fixture: &Fixture) -> SharedPeerRecordReceiver {
    PeerRecordReceiver::open(&fixture.config, &fixture.local_key, Some(&fixture.template))
        .expect("open receiver floor")
}

fn state_path(fixture: &Fixture) -> PathBuf {
    fixture.directory.join(STATE_FILE)
}

fn write_state(fixture: &Fixture, bytes: &[u8]) {
    fs::write(state_path(fixture), bytes).expect("write receiver state fixture");
    File::open(state_path(fixture))
        .and_then(|file| file.sync_all())
        .expect("sync receiver state fixture");
}

fn empty_ledger(fixture: &Fixture, mode: PeerRecordReceiverMode) -> ReceiverLedgerV2 {
    ReceiverLedgerV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        binding: receiver_binding(&fixture.template, &fixture.local_key).expect("binding"),
        mode,
        floors: BTreeMap::new(),
    }
}

fn state_bytes(ledger: &ReceiverLedgerV2) -> Vec<u8> {
    super::super::durable_peer_record_publisher::canonical_cbor(ledger)
        .expect("canonical receiver ledger")
}

#[test]
fn same_sequence_conflict_isolation_survives_reopen_and_rejects_higher_records() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let first = signed(&remote_key, 9, "peer-primary");
    let fork = signed(&remote_key, 9, "peer-fork");
    let higher = signed(&remote_key, 10, "peer-higher");

    assert_eq!(
        receiver
            .admit_serialized(&first)
            .expect("accept first claim"),
        ReceiverAdmission::Accepted
    );
    let first_lease = receiver.lease_guard(&first).expect("committed first claim");
    assert_eq!(
        receiver
            .admit_serialized(&fork)
            .expect("persist same-sequence conflict evidence"),
        ReceiverAdmission::Conflict
    );
    assert!(
        first_lease.validate().is_err(),
        "isolation revokes old lease"
    );
    assert!(receiver.lease_guard(&fork).is_err());
    drop(first_lease);
    drop(receiver);

    let reopened = open(&fixture);
    assert!(
        reopened.admit_serialized(&higher).is_err(),
        "a conflicting peer stays isolated after restart, even at a higher sequence"
    );
    assert!(reopened.lease_guard(&higher).is_err());
}

#[test]
fn same_signing_digest_with_different_hex_envelope_is_idempotent_without_rewrite() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let original = signed(&remote_key, 4, "same-claim");
    assert_eq!(
        receiver.admit_serialized(&original).expect("accept claim"),
        ReceiverAdmission::Accepted
    );
    let before = fs::read(state_path(&fixture)).expect("read committed floor");

    let mut alternate_envelope = original.clone();
    alternate_envelope.signature_hex = alternate_envelope.signature_hex.to_uppercase();
    alternate_envelope.identity_public_key_protobuf_hex = alternate_envelope
        .identity_public_key_protobuf_hex
        .to_uppercase();
    assert_ne!(
        serde_cbor::to_vec(&original).expect("original envelope"),
        serde_cbor::to_vec(&alternate_envelope).expect("alternate envelope")
    );
    verify_signed_peer_record(&alternate_envelope).expect("uppercase hex remains valid");
    assert!(same_publication_identity(&original, &alternate_envelope));
    assert_eq!(
        receiver
            .admit_serialized(&alternate_envelope)
            .expect("same signing digest is a duplicate"),
        ReceiverAdmission::Duplicate
    );
    assert_eq!(
        fs::read(state_path(&fixture)).expect("read unchanged floor"),
        before,
        "an envelope-only encoding variation must not rewrite durable state"
    );
}

#[test]
fn full_receiver_floor_rejects_new_peers_but_allows_existing_peer_updates() {
    let fixture = fixture();
    let mut floors = BTreeMap::new();
    let mut update_key = None;
    let mut update_record = None;
    for index in 0..MAX_RECEIVER_FLOORS {
        let key = Keypair::generate_ed25519();
        let record = signed(&key, 1, &format!("capacity-peer-{index}"));
        let publication = record.publication.as_ref().expect("V2 publication");
        let identity = record_identity(&record, publication).expect("record identity");
        floors.insert(
            record.record.peer_id.clone(),
            PeerFloorV2 {
                sequence: publication.sequence,
                signing_payload_digest_hex: encode_digest(&identity),
                primary_signed_record_cbor: canonical_record_bytes(&record)
                    .expect("canonical signed record"),
                conflict_signed_record_cbor: None,
                isolated: false,
            },
        );
        if index == 0 {
            update_key = Some(key);
            update_record = Some(record);
        }
    }
    let ledger = ReceiverLedgerV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        binding: receiver_binding(&fixture.template, &fixture.local_key).expect("binding"),
        mode: fixture.config.mode,
        floors,
    };
    write_state(&fixture, &state_bytes(&ledger));
    let receiver = open(&fixture);

    let untracked = signed(&Keypair::generate_ed25519(), 1, "one-too-many");
    let error = receiver
        .admit_serialized(&untracked)
        .expect_err("a 1025th distinct peer must not enter the durable floor");
    assert!(format!("{error:?}").contains("capacity"));
    {
        let durable = receiver
            .durable_lock_required()
            .expect("durable receiver state");
        assert_eq!(durable.ledger.floors.len(), MAX_RECEIVER_FLOORS);
    }

    let update = signed(
        update_key.as_ref().expect("retained fixture key"),
        2,
        "capacity-peer-updated",
    );
    assert_eq!(
        receiver
            .admit_serialized(&update)
            .expect("update tracked peer"),
        ReceiverAdmission::Accepted
    );
    assert!(receiver.lease_guard(&update).is_ok());
    assert!(
        receiver
            .lease_guard(update_record.as_ref().expect("old record"))
            .is_err()
    );
    let durable = receiver
        .durable_lock_required()
        .expect("durable receiver state");
    assert_eq!(durable.ledger.floors.len(), MAX_RECEIVER_FLOORS);
    assert_eq!(
        durable.ledger.floors[&update.record.peer_id].sequence, 2,
        "updates to an existing floor remain possible at capacity"
    );
}

fn race_admission(
    receiver: SharedPeerRecordReceiver,
    record: SignedPeerRecord,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<Result<ReceiverAdmission, crate::error::WorldError>> {
    thread::spawn(move || {
        barrier.wait();
        let _transaction = receiver.begin_ingress()?;
        receiver.admit_serialized(&record)
    })
}

#[test]
fn concurrent_sequences_do_not_regress_and_conflict_isolation_persists() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let sequence_three = signed(&remote_key, 3, "peer-three");
    let sequence_four = signed(&remote_key, 4, "peer-four");
    let sequence_five = signed(&remote_key, 5, "peer-five");
    receiver
        .admit_serialized(&sequence_three)
        .expect("seed lower floor");

    let barrier = Arc::new(Barrier::new(3));
    let lower = race_admission(
        Arc::clone(&receiver),
        sequence_four.clone(),
        Arc::clone(&barrier),
    );
    let higher = race_admission(
        Arc::clone(&receiver),
        sequence_five.clone(),
        Arc::clone(&barrier),
    );
    barrier.wait();
    let lower_result = lower.join().expect("lower admission thread");
    let higher_result = higher.join().expect("higher admission thread");
    assert!(matches!(higher_result, Ok(ReceiverAdmission::Accepted)));
    match lower_result {
        Ok(ReceiverAdmission::Accepted) => {}
        Err(error) => assert!(
            format!("{error:?}").contains("sequence regressed"),
            "only a stale sequence may lose this race: {error:?}"
        ),
        outcome => panic!("unexpected lower-sequence outcome: {outcome:?}"),
    }
    assert!(receiver.lease_guard(&sequence_five).is_ok());
    assert!(receiver.lease_guard(&sequence_four).is_err());

    let conflict_a = signed(&remote_key, 6, "peer-six-a");
    let conflict_b = signed(&remote_key, 6, "peer-six-b");
    let barrier = Arc::new(Barrier::new(3));
    let left = race_admission(Arc::clone(&receiver), conflict_a, Arc::clone(&barrier));
    let right = race_admission(Arc::clone(&receiver), conflict_b, Arc::clone(&barrier));
    barrier.wait();
    let outcomes = [
        left.join().expect("left conflict thread"),
        right.join().expect("right conflict thread"),
    ];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(ReceiverAdmission::Accepted)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(ReceiverAdmission::Conflict)))
            .count(),
        1
    );
    drop(receiver);

    let reopened = open(&fixture);
    let sequence_seven = signed(&remote_key, 7, "peer-seven");
    assert!(reopened.admit_serialized(&sequence_seven).is_err());
    assert!(reopened.lease_guard(&sequence_seven).is_err());
}

#[test]
fn receiver_lock_and_binding_mismatches_fail_without_rewriting_state() {
    let fixture = fixture();
    let original_state = fs::read(state_path(&fixture)).expect("initial state bytes");
    let held = open(&fixture);
    assert!(
        PeerRecordReceiver::open(&fixture.config, &fixture.local_key, Some(&fixture.template))
            .is_err(),
        "a second live receiver must not acquire the same directory"
    );
    drop(held);
    let reopened = open(&fixture);
    drop(reopened);

    let mut wrong_world = fixture.template.clone();
    wrong_world.world_id = "world-other".into();
    assert!(
        PeerRecordReceiver::open(&fixture.config, &fixture.local_key, Some(&wrong_world)).is_err()
    );

    let wrong_key = Keypair::generate_ed25519();
    assert!(
        PeerRecordReceiver::open(&fixture.config, &wrong_key, Some(&fixture.template)).is_err()
    );

    let wrong_mode = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::V2Only,
        directory: fixture.config.directory.clone(),
    };
    assert!(
        PeerRecordReceiver::open(&wrong_mode, &fixture.local_key, Some(&fixture.template)).is_err()
    );

    let legacy_mode = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::Legacy,
        directory: fixture.config.directory.clone(),
    };
    assert!(
        PeerRecordReceiver::open(&legacy_mode, &fixture.local_key, Some(&fixture.template))
            .is_err()
    );
    assert_eq!(
        fs::read(state_path(&fixture)).expect("state retained"),
        original_state
    );
}

#[derive(Serialize)]
struct FutureLedgerWithUnknownField {
    schema_version: u16,
    binding: ReceiverBindingV2,
    mode: PeerRecordReceiverMode,
    floors: BTreeMap<String, PeerFloorV2>,
    future_required_field: bool,
}

fn assert_open_rejects_and_preserves(fixture: &Fixture, path: &Path, bytes: &[u8]) {
    let error =
        PeerRecordReceiver::open(&fixture.config, &fixture.local_key, Some(&fixture.template))
            .expect_err("unsupported or incomplete receiver state must fail closed");
    assert!(!format!("{error:?}").is_empty());
    assert_eq!(fs::read(path).expect("retained original evidence"), bytes);
}

#[test]
fn missing_corrupt_future_and_unknown_temp_receiver_state_is_preserved_and_rejected() {
    let missing = fixture();
    let missing_state = state_path(&missing);
    fs::remove_file(&missing_state).expect("remove receiver state");
    assert!(
        PeerRecordReceiver::open(&missing.config, &missing.local_key, Some(&missing.template))
            .is_err()
    );
    assert!(
        !missing_state.exists(),
        "open must not recreate missing state"
    );

    let future_schema_fixture = fixture();
    let mut future_schema = empty_ledger(&future_schema_fixture, future_schema_fixture.config.mode);
    future_schema.schema_version = RECEIVER_SCHEMA_VERSION + 1;
    let future_schema_bytes = state_bytes(&future_schema);
    write_state(&future_schema_fixture, &future_schema_bytes);
    assert_open_rejects_and_preserves(
        &future_schema_fixture,
        &state_path(&future_schema_fixture),
        &future_schema_bytes,
    );

    let unknown_field_fixture = fixture();
    let ledger = empty_ledger(&unknown_field_fixture, unknown_field_fixture.config.mode);
    let unknown_field_bytes = super::super::durable_peer_record_publisher::canonical_cbor(
        &FutureLedgerWithUnknownField {
            schema_version: ledger.schema_version,
            binding: ledger.binding,
            mode: ledger.mode,
            floors: ledger.floors,
            future_required_field: true,
        },
    )
    .expect("encode future state field");
    write_state(&unknown_field_fixture, &unknown_field_bytes);
    assert_open_rejects_and_preserves(
        &unknown_field_fixture,
        &state_path(&unknown_field_fixture),
        &unknown_field_bytes,
    );

    let corrupt_fixture = fixture();
    let corrupt_bytes = b"\x9f\xff";
    write_state(&corrupt_fixture, corrupt_bytes);
    assert_open_rejects_and_preserves(
        &corrupt_fixture,
        &state_path(&corrupt_fixture),
        corrupt_bytes,
    );

    let temp_fixture = fixture();
    let temp_path = temp_fixture.directory.join(STATE_TEMP_FILE);
    let temp_bytes = b"future or interrupted receiver temp";
    fs::write(&temp_path, temp_bytes).expect("write unknown state temp");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))
        .expect("make unknown state temp private");
    assert_open_rejects_and_preserves(&temp_fixture, &temp_path, temp_bytes);
}

#[test]
fn persistence_failure_before_rename_poisons_receiver_without_exposing_new_lease() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let first = signed(&remote_key, 1, "before-rename-one");
    let next = signed(&remote_key, 2, "before-rename-two");
    assert_eq!(
        receiver
            .admit_serialized(&first)
            .expect("accept initial floor"),
        ReceiverAdmission::Accepted
    );
    let previous_bytes = fs::read(state_path(&fixture)).expect("old state");
    let stale_temp = fixture.directory.join(STATE_TEMP_FILE);
    let stale_temp_bytes = b"unknown temp must not be overwritten";
    fs::write(&stale_temp, stale_temp_bytes).expect("create blocking temp");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&stale_temp, fs::Permissions::from_mode(0o600))
        .expect("make blocking temp private");

    assert!(receiver.admit_serialized(&next).is_err());
    assert!(receiver.lease_guard(&first).is_err());
    assert!(receiver.lease_guard(&next).is_err());
    assert_eq!(
        fs::read(state_path(&fixture)).expect("unchanged floor"),
        previous_bytes
    );
    assert_eq!(
        fs::read(&stale_temp).expect("preserved temp"),
        stale_temp_bytes
    );
}

#[test]
fn directory_sync_failure_after_rename_poisons_memory_and_reopen_follows_disk_floor() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let old = signed(&remote_key, 1, "post-rename-one");
    let next = signed(&remote_key, 2, "post-rename-two");
    assert_eq!(
        receiver
            .admit_serialized(&old)
            .expect("accept initial floor"),
        ReceiverAdmission::Accepted
    );
    let old_lease = receiver.lease_guard(&old).expect("old durable lease");

    fs::set_permissions(&fixture.directory, fs::Permissions::from_mode(0o300))
        .expect("allow rename but deny opening directory for sync");
    let error = receiver
        .admit_serialized(&next)
        .expect_err("directory fsync must fail after atomic rename");
    fs::set_permissions(&fixture.directory, fs::Permissions::from_mode(0o700))
        .expect("restore receiver directory permissions");
    assert!(format!("{error:?}").contains("open publication directory"));
    assert!(receiver.lease_guard(&old).is_err());
    assert!(receiver.lease_guard(&next).is_err());
    drop(old_lease);
    drop(receiver);

    let bytes = fs::read(state_path(&fixture)).expect("renamed receiver state");
    let ledger: ReceiverLedgerV2 = serde_cbor::from_slice(&bytes).expect("decode renamed state");
    assert_eq!(ledger.floors[&next.record.peer_id].sequence, 2);
    let reopened = open(&fixture);
    assert!(reopened.lease_guard(&next).is_ok());
    assert!(reopened.lease_guard(&old).is_err());
}

#[test]
fn explicit_policy_upgrade_preserves_floors_and_acceptance_matrix() {
    for phase in [
        "uninterrupted",
        "intent-prefix",
        "intent",
        "state-prefix",
        "state-complete",
        "renamed",
    ] {
        let fixture = fixture();
        let receiver = open(&fixture);
        let remote = Keypair::generate_ed25519();
        let withdrawn = Keypair::generate_ed25519();
        let conflicted = Keypair::generate_ed25519();
        let legacy_key = Keypair::generate_ed25519();
        let legacy = super::super::peer_record::sign_peer_record(
            &record_body(&legacy_key, "legacy"),
            &legacy_key,
        )
        .expect("legacy signature");
        assert_eq!(
            receiver.admit_serialized(&legacy).unwrap(),
            ReceiverAdmission::LegacyV1
        );
        receiver
            .admit_serialized(&signed(&remote, 7, "live"))
            .unwrap();
        let mut withdrawn_body = record_body(&withdrawn, "withdrawn");
        withdrawn_body.direct_addrs.clear();
        let tombstone = super::super::peer_record::sign_peer_record_v2(
            &withdrawn_body,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 3,
                state: PeerRecordPublicationStateV2::Tombstone,
            },
            &withdrawn,
        )
        .unwrap();
        receiver.admit_serialized(&tombstone).unwrap();
        receiver
            .admit_serialized(&signed(&conflicted, 4, "first"))
            .unwrap();
        receiver
            .admit_serialized(&signed(&conflicted, 4, "fork"))
            .unwrap();
        let before: ReceiverLedgerV2 =
            serde_cbor::from_slice(&fs::read(state_path(&fixture)).unwrap()).unwrap();
        let mut target = fixture.config.clone();
        target.mode = PeerRecordReceiverMode::V2Only;
        assert!(
            PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &fixture.template)
                .is_err(),
            "live writer holds lock"
        );
        drop(receiver);
        let mut target_ledger = before.clone();
        target_ledger.mode = PeerRecordReceiverMode::V2Only;
        let target_bytes = state_bytes(&target_ledger);
        if phase != "uninterrupted" {
            use std::os::unix::fs::PermissionsExt;
            #[derive(Serialize)]
            struct Intent {
                schema_version: u16,
                binding: ReceiverBindingV2,
                source_blake3: String,
                target_blake3: String,
            }
            let intent = super::super::durable_peer_record_publisher::canonical_cbor(&Intent {
                schema_version: 3,
                binding: before.binding.clone(),
                source_blake3: blake3::hash(&state_bytes(&before)).to_hex().to_string(),
                target_blake3: blake3::hash(&target_bytes).to_hex().to_string(),
            })
            .unwrap();
            let intent_path = fixture
                .directory
                .join("peer-record-receiver-v2.initializing.cbor");
            let write_private = |path: &Path, bytes: &[u8]| {
                fs::write(path, bytes).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            };
            if phase == "intent-prefix" {
                write_private(
                    &fixture
                        .directory
                        .join("peer-record-receiver-v2.initializing.cbor.tmp"),
                    &intent[..intent.len() / 2],
                );
            } else {
                write_private(&intent_path, &intent);
            }
            match phase {
                "state-prefix" => write_private(
                    &fixture.directory.join(STATE_TEMP_FILE),
                    &target_bytes[..target_bytes.len() / 2],
                ),
                "state-complete" => {
                    write_private(&fixture.directory.join(STATE_TEMP_FILE), &target_bytes)
                }
                "renamed" => write_state(&fixture, &target_bytes),
                _ => {}
            }
            assert!(
                PeerRecordReceiver::open(
                    &fixture.config,
                    &fixture.local_key,
                    Some(&fixture.template),
                )
                .is_err(),
                "old mode must reject pending {phase}"
            );
            assert!(
                PeerRecordReceiver::open(&target, &fixture.local_key, Some(&fixture.template))
                    .is_err()
            );
        }
        PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &fixture.template)
            .unwrap();
        PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &fixture.template)
            .unwrap();
        let recovered_bytes = fs::read(state_path(&fixture)).unwrap();
        assert_eq!(recovered_bytes, target_bytes, "exact target after {phase}");
        assert!(!fixture.directory.join(STATE_TEMP_FILE).exists());
        for pending in [
            "peer-record-receiver-v2.initializing.cbor",
            "peer-record-receiver-v2.initializing.cbor.tmp",
        ] {
            assert!(
                !fixture.directory.join(pending).exists(),
                "{phase}: {pending}"
            );
        }
        let after: ReceiverLedgerV2 = serde_cbor::from_slice(&recovered_bytes).unwrap();
        assert_eq!(before.binding, after.binding);
        assert_eq!(
            before.floors, after.floors,
            "signed bytes and isolation remain exact"
        );
        assert_eq!(after.mode, PeerRecordReceiverMode::V2Only);
        assert!(
            PeerRecordReceiver::open(&fixture.config, &fixture.local_key, Some(&fixture.template))
                .is_err()
        );
        let receiver =
            PeerRecordReceiver::open(&target, &fixture.local_key, Some(&fixture.template)).unwrap();
        assert!(receiver.admit_serialized(&legacy).is_err());
        assert!(
            receiver
                .admit_serialized(&signed(&remote, 6, "regressed"))
                .is_err()
        );
        assert!(
            receiver
                .admit_serialized(&signed(&withdrawn, 2, "stale-live"))
                .is_err()
        );
        assert!(
            receiver
                .admit_serialized(&signed(&conflicted, 5, "higher"))
                .is_err()
        );
        assert_eq!(
            receiver
                .admit_serialized(&signed(&remote, 8, "higher"))
                .unwrap(),
            ReceiverAdmission::Accepted
        );
    }
}

#[test]
fn policy_upgrade_rejects_wrong_identity_and_unknown_writes_without_mutation() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture();
    let mut target = fixture.config.clone();
    target.mode = PeerRecordReceiverMode::V2Only;
    let before = fs::read(state_path(&fixture)).unwrap();
    let wrong_key = Keypair::generate_ed25519();
    assert!(
        PeerRecordReceiver::upgrade_to_v2_only(&target, &wrong_key, &fixture.template).is_err()
    );
    let mut wrong_domain = fixture.template.clone();
    wrong_domain.network_id = "another-network".into();
    assert!(
        PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &wrong_domain).is_err()
    );
    assert!(
        PeerRecordReceiver::upgrade_to_v2_only(
            &fixture.config,
            &fixture.local_key,
            &fixture.template,
        )
        .is_err()
    );
    for name in [STATE_TEMP_FILE, "peer-record-receiver-v2.initializing.cbor"] {
        let unknown = fixture.directory.join(name);
        fs::write(&unknown, b"unknown-operation").unwrap();
        fs::set_permissions(&unknown, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &fixture.template)
                .is_err()
        );
        assert_eq!(fs::read(&unknown).unwrap(), b"unknown-operation");
        assert_eq!(fs::read(state_path(&fixture)).unwrap(), before);
        fs::remove_file(unknown).unwrap();
    }
    PeerRecordReceiver::upgrade_to_v2_only(&target, &fixture.local_key, &fixture.template).unwrap();
}

mod policy_upgrade {
    use super::super::persistence::upgrade::{UpgradeIntent, upgrade_with_finish_sync};
    use super::super::persistence::{initialize, open, upgrade_to_v2_only};
    use super::super::*;
    use crate::libp2p_net::durable_peer_record_publisher as durable_files;
    use std::fs;
    use std::path::Path;
    const STATE_FILE_NAME: &str = "peer-record-receiver-v2.state.cbor";
    const STATE_TEMP_FILE_NAME: &str = "peer-record-receiver-v2.state.cbor.tmp";
    const INTENT_FILE_NAME: &str = "peer-record-receiver-v2.initializing.cbor";
    const INTENT_TEMP_FILE_NAME: &str = "peer-record-receiver-v2.initializing.cbor.tmp";
    use std::os::unix::fs::PermissionsExt;

    fn write_private(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn upgrade_recovers_each_atomic_boundary_without_resetting_the_ledger() {
        for phase in [
            "intent-prefix",
            "intent",
            "state-prefix",
            "state-complete",
            "renamed",
        ] {
            let temp_root = tempfile::tempdir().unwrap();
            let root = temp_root.path().canonicalize().unwrap();
            let directory = root.join("receiver");
            let binding = ReceiverBindingV2 {
                schema_version: 2,
                local_peer_id: "receiver".into(),
                world_id: "world-a".into(),
                network_id: "network-a".into(),
                identity_public_key_protobuf_hex: "00".into(),
            };
            initialize(
                &directory,
                binding.clone(),
                PeerRecordReceiverMode::DurableCompatible,
            )
            .unwrap();
            let source = fs::read(directory.join(STATE_FILE_NAME)).unwrap();
            let mut ledger: ReceiverLedgerV2 = serde_cbor::from_slice(&source).unwrap();
            ledger.mode = PeerRecordReceiverMode::V2Only;
            let target = durable_files::canonical_cbor(&ledger).unwrap();
            let intent = durable_files::canonical_cbor(&UpgradeIntent {
                schema_version: 3,
                binding: binding.clone(),
                source_blake3: blake3::hash(&source).to_hex().to_string(),
                target_blake3: blake3::hash(&target).to_hex().to_string(),
            })
            .unwrap();
            if phase == "intent-prefix" {
                write_private(
                    &directory.join(INTENT_TEMP_FILE_NAME),
                    &intent[..intent.len() / 2],
                );
            } else {
                write_private(&directory.join(INTENT_FILE_NAME), &intent);
            }
            match phase {
                "state-prefix" => write_private(
                    &directory.join(STATE_TEMP_FILE_NAME),
                    &target[..target.len() / 2],
                ),
                "state-complete" => write_private(&directory.join(STATE_TEMP_FILE_NAME), &target),
                "renamed" => write_private(&directory.join(STATE_FILE_NAME), &target),
                _ => {}
            }
            assert!(
                open(
                    &directory,
                    binding.clone(),
                    PeerRecordReceiverMode::DurableCompatible
                )
                .is_err()
            );
            assert!(open(&directory, binding.clone(), PeerRecordReceiverMode::V2Only).is_err());
            assert!(
                upgrade_with_finish_sync(&directory, binding.clone(), |_| {
                    Err(protocol_error(
                        "injected final directory sync failure".into(),
                    ))
                })
                .is_err()
            );
            assert!(!directory.join(INTENT_FILE_NAME).exists());
            let sync_called = std::cell::Cell::new(false);
            upgrade_with_finish_sync(&directory, binding.clone(), |directory| {
                sync_called.set(true);
                durable_files::sync_directory(directory)
            })
            .unwrap();
            assert!(
                sync_called.get(),
                "idempotent recovery must finish directory sync"
            );
            assert_eq!(fs::read(directory.join(STATE_FILE_NAME)).unwrap(), target);
            upgrade_to_v2_only(&directory, binding.clone()).unwrap();
            drop(open(&directory, binding.clone(), PeerRecordReceiverMode::V2Only).unwrap());
            assert!(
                open(
                    &directory,
                    binding,
                    PeerRecordReceiverMode::DurableCompatible
                )
                .is_err()
            );
        }
    }
}
