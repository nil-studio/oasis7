#![cfg(all(test, unix))]

use super::*;
use crate::{Libp2pNetwork, Libp2pNetworkConfig};
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
        pruned_trusted_time_watermark_ms: None,
        pre_migration_v1_expiries: BTreeMap::new(),
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
                max_expiry_ms: None,
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
        pruned_trusted_time_watermark_ms: None,
        pre_migration_v1_expiries: BTreeMap::new(),
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

#[test]
fn pruning_reclaims_expired_floors_and_allows_new_peers_at_capacity() {
    let fixture = fixture();
    let mut floors = BTreeMap::new();
    let base_time: i64 = 1_000_000;
    let old_keys: Vec<Keypair> = (0..20).map(|_| Keypair::generate_ed25519()).collect();

    // 20 old floors that are expired and prunable
    for (index, key) in old_keys.iter().enumerate() {
        let mut record = record_body(key, &format!("prunable-peer-{index}"));
        record.published_at_ms = base_time;
        record.ttl_ms = 60_000;
        let signed = super::super::peer_record::sign_peer_record_v2(
            &record,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 1,
                state: PeerRecordPublicationStateV2::Live,
            },
            key,
        )
        .expect("sign record");
        let publication = signed.publication.as_ref().expect("publication");
        let identity = record_identity(&signed, publication).expect("identity");
        floors.insert(
            record.peer_id.clone(),
            PeerFloorV2 {
                sequence: 1,
                signing_payload_digest_hex: encode_digest(&identity),
                primary_signed_record_cbor: canonical_record_bytes(&signed).expect("cbor"),
                conflict_signed_record_cbor: None,
                isolated: false,
                max_expiry_ms: None,
            },
        );
    }

    // Remaining 1004 floors that are fresh (published at base_time + 10_000_000)
    for index in 0..(MAX_RECEIVER_FLOORS - 20) {
        let key = Keypair::generate_ed25519();
        let mut record = record_body(&key, &format!("fresh-peer-{index}"));
        record.published_at_ms = base_time + 10_000_000;
        record.ttl_ms = 60_000;
        let signed = super::super::peer_record::sign_peer_record_v2(
            &record,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 1,
                state: PeerRecordPublicationStateV2::Live,
            },
            &key,
        )
        .expect("sign record");
        let publication = signed.publication.as_ref().expect("publication");
        let identity = record_identity(&signed, publication).expect("identity");
        floors.insert(
            record.peer_id.clone(),
            PeerFloorV2 {
                sequence: 1,
                signing_payload_digest_hex: encode_digest(&identity),
                primary_signed_record_cbor: canonical_record_bytes(&signed).expect("cbor"),
                conflict_signed_record_cbor: None,
                isolated: false,
                max_expiry_ms: None,
            },
        );
    }
    assert_eq!(floors.len(), MAX_RECEIVER_FLOORS);

    let ledger = ReceiverLedgerV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        binding: receiver_binding(&fixture.template, &fixture.local_key).expect("binding"),
        mode: fixture.config.mode,
        floors,
        pruned_trusted_time_watermark_ms: None,
        pre_migration_v1_expiries: BTreeMap::new(),
    };
    write_state(&fixture, &state_bytes(&ledger));
    let receiver = open(&fixture);
    assert_eq!(receiver.floor_count().unwrap(), MAX_RECEIVER_FLOORS);

    // 1025th peer is rejected with capacity error
    let untracked_key = Keypair::generate_ed25519();
    let untracked = signed(&untracked_key, 1, "peer-1025");
    let error = receiver
        .admit_serialized(&untracked)
        .expect_err("capacity exhausted must reject 1025th peer");
    assert!(format!("{error:?}").contains("capacity"));

    // Attempting to prune without sufficient time lower bound reclaims 0 floors
    let too_early_now = base_time + 60_000;
    let pruned = receiver
        .prune_floors(too_early_now, &BTreeSet::new())
        .expect("prune too early");
    assert_eq!(
        pruned, 0,
        "must not prune before published_at_ms + MAX_TTL + MAX_FUTURE_SKEW"
    );
    assert_eq!(receiver.floor_count().unwrap(), MAX_RECEIVER_FLOORS);

    // Prune with verified trusted time lower bound >= published_at_ms + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS
    let trusted_now = base_time + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS + 100;
    let pruned = receiver
        .prune_floors(trusted_now, &BTreeSet::new())
        .expect("prune expired floors");
    assert_eq!(pruned, 20, "exactly 20 old floors must be reclaimed");
    assert_eq!(receiver.floor_count().unwrap(), MAX_RECEIVER_FLOORS - 20);

    // Now the 1025th peer can be admitted into the newly freed capacity!
    assert_eq!(
        receiver
            .admit_serialized(&untracked)
            .expect("admit newly permitted peer"),
        ReceiverAdmission::Accepted
    );
    assert_eq!(
        receiver.floor_count().unwrap(),
        MAX_RECEIVER_FLOORS - 20 + 1
    );
    assert!(receiver.lease_guard(&untracked).is_ok());
}

#[test]
fn retained_peers_and_conflict_isolated_peers_are_never_pruned() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let base_time: i64 = 1_000_000;

    // Peer A: expired, but in retained_peers (active session or trusted seed)
    let key_a = Keypair::generate_ed25519();
    let peer_a_id = PeerId::from(key_a.public()).to_string();
    let mut rec_a = record_body(&key_a, "peer-a-seed");
    rec_a.published_at_ms = base_time;
    rec_a.ttl_ms = 60_000;
    let signed_a = super::super::peer_record::sign_peer_record_v2(
        &rec_a,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key_a,
    )
    .unwrap();
    receiver.admit_serialized(&signed_a).unwrap();

    // Peer B: expired, but isolated after a conflict
    let key_b = Keypair::generate_ed25519();
    let peer_b_id = PeerId::from(key_b.public()).to_string();
    let mut rec_b1 = record_body(&key_b, "peer-b-fork-1");
    rec_b1.published_at_ms = base_time;
    rec_b1.ttl_ms = 60_000;
    let signed_b1 = super::super::peer_record::sign_peer_record_v2(
        &rec_b1,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key_b,
    )
    .unwrap();
    receiver.admit_serialized(&signed_b1).unwrap();

    let mut rec_b2 = record_body(&key_b, "peer-b-fork-2");
    rec_b2.published_at_ms = base_time;
    rec_b2.ttl_ms = 60_000;
    rec_b2.direct_addrs = vec!["/ip4/127.0.0.1/tcp/9999".into()];
    let signed_b2 = super::super::peer_record::sign_peer_record_v2(
        &rec_b2,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key_b,
    )
    .unwrap();
    assert_eq!(
        receiver.admit_serialized(&signed_b2).unwrap(),
        ReceiverAdmission::Conflict
    );

    // Peer C: expired, not retained, not isolated -> PRUNABLE
    let key_c = Keypair::generate_ed25519();
    let peer_c_id = PeerId::from(key_c.public()).to_string();
    let mut rec_c = record_body(&key_c, "peer-c-inactive");
    rec_c.published_at_ms = base_time;
    rec_c.ttl_ms = 60_000;
    let signed_c = super::super::peer_record::sign_peer_record_v2(
        &rec_c,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key_c,
    )
    .unwrap();
    receiver.admit_serialized(&signed_c).unwrap();

    assert_eq!(receiver.floor_count().unwrap(), 3);

    let mut retained_peers = BTreeSet::new();
    retained_peers.insert(peer_a_id.clone());

    let trusted_now = base_time + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS + 500;
    let pruned = receiver.prune_floors(trusted_now, &retained_peers).unwrap();
    assert_eq!(pruned, 1, "only peer C should be pruned");

    assert!(
        receiver.contains_floor(&peer_a_id).unwrap(),
        "retained peer A must remain"
    );
    assert!(
        receiver.contains_floor(&peer_b_id).unwrap(),
        "isolated peer B must remain"
    );
    assert!(
        !receiver.contains_floor(&peer_c_id).unwrap(),
        "unretained peer C was pruned"
    );
    assert_eq!(receiver.floor_count().unwrap(), 2);
}

#[test]
fn pruning_rejects_missing_negative_or_backward_moving_trusted_time() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let retained = BTreeSet::new();

    let err_zero = receiver.prune_floors(0, &retained).unwrap_err();
    assert!(format!("{err_zero:?}").contains("trusted time must be positive"));

    let err_neg = receiver.prune_floors(-500, &retained).unwrap_err();
    assert!(format!("{err_neg:?}").contains("trusted time must be positive"));

    // Advance to 10_000_000
    receiver.prune_floors(10_000_000, &retained).unwrap();

    // Attempting to move backward must fail
    let err_backward = receiver.prune_floors(9_999_999, &retained).unwrap_err();
    assert!(format!("{err_backward:?}").contains("trusted time moved backward"));

    // Monotonic advance succeeds
    assert!(receiver.prune_floors(10_000_001, &retained).is_ok());
}

#[test]
fn pruned_peer_cannot_resurrect_stale_records_tombstone_or_v1() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let base_time: i64 = 1_000_000;
    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public()).to_string();

    let mut record = record_body(&key, "peer-prune-resurrect");
    record.published_at_ms = base_time;
    record.ttl_ms = 60_000;
    let signed_live = super::super::peer_record::sign_peer_record_v2(
        &record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 5,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key,
    )
    .unwrap();
    receiver.admit_serialized(&signed_live).unwrap();
    assert!(receiver.contains_floor(&peer_id).unwrap());

    // Prune floor at trusted time
    let trusted_now = base_time + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS + 1000;
    let pruned = receiver
        .prune_floors(trusted_now, &BTreeSet::new())
        .unwrap();
    assert_eq!(pruned, 1);
    assert!(!receiver.contains_floor(&peer_id).unwrap());

    // Replay of old live record (seq 5, published at base_time):
    // Clock evaluation at trusted_now rejects it as expired!
    let clock = super::super::peer_record_time::DiscoveryClock::default();
    let clock_check = clock.check_at(&signed_live.record, trusted_now, std::time::Instant::now());
    assert!(clock_check.is_err());
    assert!(format!("{clock_check:?}").contains("record expired"));

    // Replay of old tombstone at base_time:
    let mut tombstone_rec = record_body(&key, "peer-tombstone");
    tombstone_rec.direct_addrs.clear();
    tombstone_rec.published_at_ms = base_time;
    tombstone_rec.ttl_ms = 60_000;
    let tombstone = super::super::peer_record::sign_peer_record_v2(
        &tombstone_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 5,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &key,
    )
    .unwrap();
    let tombstone_clock = clock.check_at(&tombstone.record, trusted_now, std::time::Instant::now());
    assert!(tombstone_clock.is_err());
    assert!(format!("{tombstone_clock:?}").contains("record expired"));

    // Replay of old V1 record at base_time:
    let mut v1_rec = record_body(&key, "peer-v1");
    v1_rec.published_at_ms = base_time;
    v1_rec.ttl_ms = 60_000;
    let v1 = super::super::peer_record::sign_peer_record(&v1_rec, &key).unwrap();
    let v1_clock = clock.check_at(&v1.record, trusted_now, std::time::Instant::now());
    assert!(v1_clock.is_err());
    assert!(format!("{v1_clock:?}").contains("record expired"));
}

#[test]
fn pruning_persists_atomically_and_survives_reopen() {
    let fixture = fixture();
    let base_time: i64 = 1_000_000;
    let old_key = Keypair::generate_ed25519();
    let old_peer_id = PeerId::from(old_key.public()).to_string();

    {
        let receiver = open(&fixture);
        let mut rec = record_body(&old_key, "offline-peer");
        rec.published_at_ms = base_time;
        rec.ttl_ms = 60_000;
        let signed = super::super::peer_record::sign_peer_record_v2(
            &rec,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 1,
                state: PeerRecordPublicationStateV2::Live,
            },
            &old_key,
        )
        .unwrap();
        receiver.admit_serialized(&signed).unwrap();
        assert_eq!(receiver.floor_count().unwrap(), 1);
    }

    // Run offline pruning via Libp2pNetwork::prune_peer_record_receiver
    let net_config = Libp2pNetworkConfig {
        keypair: Some(fixture.local_key.clone()),
        peer_record: Some(fixture.template.clone()),
        peer_record_receiver: fixture.config.clone(),
        ..Libp2pNetworkConfig::default()
    };
    let trusted_now = base_time + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS + 1000;
    let pruned =
        Libp2pNetwork::prune_peer_record_receiver(&net_config, trusted_now, &BTreeSet::new())
            .expect("offline prune via Libp2pNetwork");
    assert_eq!(pruned, 1);

    // Reopen and verify floor is gone and state is valid canonical CBOR
    let reopened = open(&fixture);
    assert_eq!(reopened.floor_count().unwrap(), 0);
    assert!(!reopened.contains_floor(&old_peer_id).unwrap());
}

#[test]
fn pruning_respects_superseded_records_maximum_expiry_even_with_lower_timestamp_tombstone() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let remote_key = Keypair::generate_ed25519();
    let peer_id = remote_key.public().to_peer_id().to_string();

    let t = 10_000_000i64;
    // Sequence 1: published at T, TTL = 3_600_000 ms -> expires at T + 3_600_000 ms
    let mut rec1 = record_body(&remote_key, "peer-seq1");
    rec1.published_at_ms = t;
    rec1.ttl_ms = 3_600_000;
    let signed1 = super::super::peer_record::sign_peer_record_v2(
        &rec1,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &remote_key,
    )
    .unwrap();
    assert_eq!(
        receiver.admit_serialized(&signed1).unwrap(),
        ReceiverAdmission::Accepted
    );

    // Sequence 2: tombstone published at T - 3_599_000 ms, TTL = 3_600_000 ms -> expires at T + 1_000 ms
    let mut rec2 = record_body(&remote_key, "peer-seq2-tombstone");
    rec2.direct_addrs.clear();
    rec2.hole_punch_addrs.clear();
    rec2.relay_addrs.clear();
    rec2.published_at_ms = t - 3_599_000;
    rec2.ttl_ms = 3_600_000;
    let signed2 = super::super::peer_record::sign_peer_record_v2(
        &rec2,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &remote_key,
    )
    .unwrap();
    assert_eq!(
        receiver.admit_serialized(&signed2).unwrap(),
        ReceiverAdmission::Accepted
    );

    // At T + 31_000 ms: sequence 2 tombstone max_possible_expiry is passed, BUT sequence 1
    // is valid until T + 3_600_000 ms. Pruning MUST be refused (returns 0).
    let trusted_now_early = t + 31_000;
    assert_eq!(
        receiver
            .prune_floors(trusted_now_early, &BTreeSet::new())
            .unwrap(),
        0
    );
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // At T + 3_600_000 + MAX_FUTURE_SKEW_MS + 1000: all possible records have expired.
    let trusted_now_safe = t + 3_600_000 + MAX_FUTURE_SKEW_MS + 1000;
    assert_eq!(
        receiver
            .prune_floors(trusted_now_safe, &BTreeSet::new())
            .unwrap(),
        1
    );
    assert_eq!(receiver.floor_count().unwrap(), 0);
    assert!(!receiver.contains_floor(&peer_id).unwrap());

    // Replaying sequence 1 MUST be rejected by preflight and admission because it expired
    // before the receiver pruning watermark.
    assert!(receiver.preflight_record(&signed1).is_err());
    assert!(receiver.admit_serialized(&signed1).is_err());
}

#[test]
fn pruning_watermark_persists_across_restart_and_rejects_resurrection_under_skewed_clock() {
    let fixture = fixture();
    let remote_key = Keypair::generate_ed25519();
    let peer_id = remote_key.public().to_peer_id().to_string();

    let base_time = 1_000_000i64;
    {
        let receiver = open(&fixture);
        let mut rec = record_body(&remote_key, "peer-init");
        rec.published_at_ms = base_time;
        rec.ttl_ms = 60_000; // expires at 1_060_000
        let signed = super::super::peer_record::sign_peer_record_v2(
            &rec,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 1,
                state: PeerRecordPublicationStateV2::Live,
            },
            &remote_key,
        )
        .unwrap();
        receiver.admit_serialized(&signed).unwrap();
        assert_eq!(receiver.floor_count().unwrap(), 1);

        // Prune at 5_000_000
        let prune_time = 5_000_000i64;
        assert_eq!(
            receiver.prune_floors(prune_time, &BTreeSet::new()).unwrap(),
            1
        );
        assert_eq!(receiver.floor_count().unwrap(), 0);
    }

    // Reopen receiver from disk
    let reopened = open(&fixture);
    assert_eq!(reopened.floor_count().unwrap(), 0);
    assert!(!reopened.contains_floor(&peer_id).unwrap());

    // Trusted time moving backward relative to the persisted watermark MUST be rejected
    assert!(reopened.prune_floors(4_000_000, &BTreeSet::new()).is_err());

    // Replaying the old record (expiry 1_060_000 <= watermark 5_000_000) MUST be rejected
    let mut old_rec = record_body(&remote_key, "peer-replayed");
    old_rec.published_at_ms = base_time;
    old_rec.ttl_ms = 60_000;
    let old_signed = super::super::peer_record::sign_peer_record_v2(
        &old_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &remote_key,
    )
    .unwrap();
    assert!(reopened.preflight_record(&old_signed).is_err());
    assert!(reopened.admit_serialized(&old_signed).is_err());

    // A fresh peer record whose expiry is after the watermark MUST be admitted
    let fresh_key = Keypair::generate_ed25519();
    let mut fresh_rec = record_body(&fresh_key, "peer-fresh");
    fresh_rec.published_at_ms = 5_100_000;
    fresh_rec.ttl_ms = 60_000; // expires at 5_160_000 > 5_000_000
    let fresh_signed = super::super::peer_record::sign_peer_record_v2(
        &fresh_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &fresh_key,
    )
    .unwrap();
    assert!(reopened.preflight_record(&fresh_signed).is_ok());
    assert_eq!(
        reopened.admit_serialized(&fresh_signed).unwrap(),
        ReceiverAdmission::Accepted
    );
    assert_eq!(reopened.floor_count().unwrap(), 1);
}

#[test]
fn pre_upgrade_floor_with_none_max_expiry_survives_conflict_with_earlier_expiry() {
    let fixture = fixture();
    let remote_key = Keypair::generate_ed25519();
    let peer_id = remote_key.public().to_peer_id().to_string();

    // Primary record has a long expiry (10_000_000 + 3_600_000 = 13_600_000)
    let mut rec_primary = record_body(&remote_key, "peer-primary");
    rec_primary.published_at_ms = 10_000_000;
    rec_primary.ttl_ms = 3_600_000;
    let signed_primary = super::super::peer_record::sign_peer_record_v2(
        &rec_primary,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 5,
            state: PeerRecordPublicationStateV2::Live,
        },
        &remote_key,
    )
    .unwrap();
    let publication = signed_primary.publication.as_ref().unwrap();
    let identity = record_identity(&signed_primary, publication).unwrap();

    // Create a pre-upgrade floor with max_expiry_ms: None
    let mut floors = BTreeMap::new();
    floors.insert(
        peer_id.clone(),
        PeerFloorV2 {
            sequence: 5,
            signing_payload_digest_hex: encode_digest(&identity),
            primary_signed_record_cbor: canonical_record_bytes(&signed_primary).unwrap(),
            conflict_signed_record_cbor: None,
            isolated: false,
            max_expiry_ms: None,
        },
    );
    let ledger = ReceiverLedgerV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        binding: receiver_binding(&fixture.template, &fixture.local_key).unwrap(),
        mode: fixture.config.mode,
        floors,
        pruned_trusted_time_watermark_ms: None,
        pre_migration_v1_expiries: BTreeMap::new(),
    };
    write_state(&fixture, &state_bytes(&ledger));

    let receiver = open(&fixture);
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // Now present a conflicting record with same sequence (5) but earlier publication and short TTL
    // (published_at_ms = 8_000_000, ttl = 60_000, expiry = 8_060_000 < 13_600_000)
    let mut rec_conflict = record_body(&remote_key, "peer-conflict");
    rec_conflict.published_at_ms = 8_000_000;
    rec_conflict.ttl_ms = 60_000;
    let signed_conflict = super::super::peer_record::sign_peer_record_v2(
        &rec_conflict,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 5,
            state: PeerRecordPublicationStateV2::Live,
        },
        &remote_key,
    )
    .unwrap();

    assert_eq!(
        receiver.admit_serialized(&signed_conflict).unwrap(),
        ReceiverAdmission::Conflict
    );
    drop(receiver);

    // Reopening receiver from disk MUST succeed and not fail validation
    let reopened = open(&fixture);
    assert_eq!(reopened.floor_count().unwrap(), 1);
    let floor = reopened
        .durable_lock()
        .unwrap()
        .unwrap()
        .ledger
        .floors
        .get(&peer_id)
        .cloned()
        .unwrap();
    assert!(floor.isolated);
    assert_eq!(floor.max_expiry_ms, Some(13_600_000));
}

#[test]
fn pre_migration_v1_record_expiry_bounds_subsequent_v2_floor_reclamation_and_prevents_v1_resurrection()
 {
    let fixture = fixture();
    let receiver = open(&fixture);
    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public()).to_string();

    // 1. Admit a V1 live record published at T with 3,600,000 ms TTL
    let base_t = 5_000_000;
    let ttl_ms = 3_600_000;
    let mut v1_rec = record_body(&key, "peer-v1-migration");
    v1_rec.published_at_ms = base_t;
    v1_rec.ttl_ms = ttl_ms;
    let signed_v1 = super::super::peer_record::sign_peer_record(&v1_rec, &key).unwrap();

    assert_eq!(
        receiver.admit_serialized(&signed_v1).unwrap(),
        ReceiverAdmission::LegacyV1
    );

    // 2. Admit a V2 tombstone published at T - 3,599,000 with 3,600,000 ms TTL (expiry = T + 1,000 ms)
    let mut tombstone_rec = record_body(&key, "peer-v1-migration");
    tombstone_rec.direct_addrs.clear();
    tombstone_rec.published_at_ms = base_t - 3_599_000;
    tombstone_rec.ttl_ms = ttl_ms;
    let signed_tombstone = super::super::peer_record::sign_peer_record_v2(
        &tombstone_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &key,
    )
    .unwrap();

    assert_eq!(
        receiver.admit_serialized(&signed_tombstone).unwrap(),
        ReceiverAdmission::Accepted
    );
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // Verify floor max_expiry_ms incorporated the pre-migration V1 expiry (base_t + ttl_ms = 8_600_000)
    let floor = receiver
        .durable_lock()
        .unwrap()
        .unwrap()
        .ledger
        .floors
        .get(&peer_id)
        .cloned()
        .unwrap();
    assert_eq!(floor.max_expiry_ms, Some(base_t + ttl_ms));

    // 3. Pruning at T + 31,000 ms MUST NOT delete the floor because V1 expiry has not passed
    let pruned = receiver
        .prune_floors(base_t + 31_000, &BTreeSet::new())
        .unwrap();
    assert_eq!(pruned, 0);
    assert!(receiver.contains_floor(&peer_id).unwrap());

    // 4. Replaying the V1 record while floor is retained is rejected due to migration
    let replay_err = receiver.admit_serialized(&signed_v1).unwrap_err();
    assert!(
        format!("{replay_err:?}").contains("legacy peer record rejected after V2 floor migration"),
        "expected migration rejection, got {replay_err:?}"
    );

    // 5. Reopen receiver to ensure max_expiry_ms persisted across restart
    drop(receiver);
    let reopened = open(&fixture);
    assert!(reopened.contains_floor(&peer_id).unwrap());
    let reopened_floor = reopened
        .durable_lock()
        .unwrap()
        .unwrap()
        .ledger
        .floors
        .get(&peer_id)
        .cloned()
        .unwrap();
    assert_eq!(reopened_floor.max_expiry_ms, Some(base_t + ttl_ms));

    // Prune attempt at T + 31,000 ms on reopened receiver still retains floor
    assert_eq!(
        reopened
            .prune_floors(base_t + 31_000, &BTreeSet::new())
            .unwrap(),
        0
    );

    // 6. When trusted time advances past V1 record max possible expiry:
    let final_trusted_time = base_t + ttl_ms + 30_000 + 1000;
    let pruned = reopened
        .prune_floors(final_trusted_time, &BTreeSet::new())
        .unwrap();
    assert_eq!(pruned, 1);
    assert!(!reopened.contains_floor(&peer_id).unwrap());

    // 7. Replaying the V1 record after floor pruning is rejected by pruning watermark
    let replay_watermark_err = reopened.admit_serialized(&signed_v1).unwrap_err();
    assert!(
        format!("{replay_watermark_err:?}")
            .contains("legacy peer record expired before receiver pruning watermark"),
        "expected watermark rejection, got {replay_watermark_err:?}"
    );
}
