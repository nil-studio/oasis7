#![cfg(all(test, unix))]

use std::collections::BTreeMap;

use libp2p::{PeerId, identity::Keypair};
use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};

use super::tests::{fixture, open, record_body, signed, state_bytes, write_state};
use super::*;
use crate::{Libp2pNetwork, Libp2pNetworkConfig};

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
                admitted_at_ms: None,
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
                admitted_at_ms: None,
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
            admitted_at_ms: None,
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

#[test]
fn combined_v1_and_v2_capacity_limits_admission_and_allows_v1_migration_at_capacity() {
    let fixture = fixture();
    let mut pre_migration_v1_expiries = BTreeMap::new();
    let mut tracked_key = None;
    for i in 0..MAX_RECEIVER_FLOORS {
        let key = Keypair::generate_ed25519();
        let peer_id = PeerId::from(key.public()).to_string();
        pre_migration_v1_expiries.insert(peer_id, 10_000_000);
        if i == 0 {
            tracked_key = Some(key);
        }
    }
    let ledger = ReceiverLedgerV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        binding: receiver_binding(&fixture.template, &fixture.local_key).unwrap(),
        mode: fixture.config.mode,
        floors: BTreeMap::new(),
        pruned_trusted_time_watermark_ms: None,
        pre_migration_v1_expiries,
    };
    write_state(&fixture, &state_bytes(&ledger));

    let receiver = open(&fixture);

    // 1. New untracked V1 peer is rejected
    let untracked_v1_key = Keypair::generate_ed25519();
    let v1_rec = record_body(&untracked_v1_key, "untracked-v1");
    let signed_untracked_v1 =
        super::super::peer_record::sign_peer_record(&v1_rec, &untracked_v1_key).unwrap();
    let err = receiver.admit_serialized(&signed_untracked_v1).unwrap_err();
    assert!(format!("{err:?}").contains("capacity"));

    // 2. New untracked V2 peer is rejected
    let untracked_v2_key = Keypair::generate_ed25519();
    let signed_untracked_v2 = signed(&untracked_v2_key, 1, "untracked-v2");
    let err = receiver.admit_serialized(&signed_untracked_v2).unwrap_err();
    assert!(format!("{err:?}").contains("capacity"));

    // 3. Already tracked V1 peer migrating to V2 SUCCEEDS because total count does not increase
    let tracked_key = tracked_key.unwrap();
    let tracked_v2 = signed(&tracked_key, 1, "tracked-v1-migrating");
    assert_eq!(
        receiver.admit_serialized(&tracked_v2).unwrap(),
        ReceiverAdmission::Accepted
    );
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // 4. Dropping and reopening passes validation cleanly
    drop(receiver);
    let reopened = open(&fixture);
    assert_eq!(reopened.floor_count().unwrap(), 1);
}

#[test]
fn unobserved_lower_sequence_record_cannot_resurrect_withdrawn_peer_after_pruning() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public()).to_string();

    let t = 10_000_000i64;
    // Receiver observes trusted time T before sequence 2 arrives
    receiver.observe_trusted_time(t).unwrap();

    // Sequence 1: published at T with 3,600,000 ms TTL (expires at T + 3,600,000 ms),
    // but WITHHELD from receiver (receiver has never observed it).
    let mut rec1 = record_body(&key, "peer-unobserved-seq1");
    rec1.published_at_ms = t;
    rec1.ttl_ms = 3_600_000;
    let signed1 = super::super::peer_record::sign_peer_record_v2(
        &rec1,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key,
    )
    .unwrap();

    // Sequence 2: tombstone published at T - 3,599,000 ms with 3,600,000 ms TTL
    // (expires at T + 1,000 ms). Delivered to receiver at T.
    let mut tombstone_rec = record_body(&key, "peer-unobserved-seq2-tombstone");
    tombstone_rec.direct_addrs.clear();
    tombstone_rec.hole_punch_addrs.clear();
    tombstone_rec.relay_addrs.clear();
    tombstone_rec.published_at_ms = t - 3_599_000;
    tombstone_rec.ttl_ms = 3_600_000;
    let signed2 = super::super::peer_record::sign_peer_record_v2(
        &tombstone_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &key,
    )
    .unwrap();

    assert_eq!(
        receiver.admit_serialized(&signed2).unwrap(),
        ReceiverAdmission::Accepted
    );
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // Floor records admitted_at_ms = T = 10_000_000
    let floor = receiver
        .durable_lock()
        .unwrap()
        .unwrap()
        .ledger
        .floors
        .get(&peer_id)
        .cloned()
        .unwrap();
    assert_eq!(floor.admitted_at_ms, Some(t));

    // Pruning attempt at T + 31,000 ms MUST be rejected because unobserved lower-sequence
    // records published prior to admission could still be valid until:
    // T + MAX_RECORD_TTL_MS + MAX_FUTURE_SKEW_MS = T + 3_630_000 ms.
    let pruned = receiver.prune_floors(t + 31_000, &BTreeSet::new()).unwrap();
    assert_eq!(pruned, 0);
    assert!(receiver.contains_floor(&peer_id).unwrap());

    // Delivering withheld sequence 1 while floor is retained is rejected due to sequence regression
    let regressed_err = receiver.admit_serialized(&signed1).unwrap_err();
    assert!(
        format!("{regressed_err:?}").contains("peer-record V2 sequence regressed"),
        "expected sequence regressed error, got {regressed_err:?}"
    );

    // Reopening receiver from disk preserves admitted_at_ms and retains floor at T + 31,000 ms
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
    assert_eq!(reopened_floor.admitted_at_ms, Some(t));
    assert_eq!(
        reopened.prune_floors(t + 31_000, &BTreeSet::new()).unwrap(),
        0
    );

    // When trusted time advances past the unobserved retention bound (T + 3_600_000 + 30_000 + 1000):
    let safe_prune_time = t + 3_600_000 + MAX_FUTURE_SKEW_MS + 1000;
    let pruned = reopened
        .prune_floors(safe_prune_time, &BTreeSet::new())
        .unwrap();
    assert_eq!(pruned, 1);
    assert!(!reopened.contains_floor(&peer_id).unwrap());

    // Delivering sequence 1 after floor pruning is rejected by receiver pruning watermark
    let replay_watermark_err = reopened.admit_serialized(&signed1).unwrap_err();
    assert!(
        format!("{replay_watermark_err:?}")
            .contains("peer record expired before receiver pruning watermark"),
        "expected watermark rejection, got {replay_watermark_err:?}"
    );
}

#[test]
fn stale_trusted_bound_must_not_allow_unobserved_sequence_resurrection() {
    let fixture = fixture();
    let receiver = open(&fixture);
    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public()).to_string();

    let t = 10_000_000i64;
    // Receiver observes a stale trusted time bound (T - 3,600,000)
    receiver.observe_trusted_time(t - 3_600_000).unwrap();

    // Sequence 1: published at T with 3,600,000 ms TTL (expires at T + 3,600,000 ms), withheld
    let mut rec1 = record_body(&key, "peer-stale-bound-seq1");
    rec1.published_at_ms = t;
    rec1.ttl_ms = 3_600_000;
    let signed1 = super::super::peer_record::sign_peer_record_v2(
        &rec1,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &key,
    )
    .unwrap();

    // Sequence 2: tombstone published at T - 3,599,000 ms with 3,600,000 ms TTL
    let mut tombstone_rec = record_body(&key, "peer-stale-bound-seq2-tombstone");
    tombstone_rec.direct_addrs.clear();
    tombstone_rec.hole_punch_addrs.clear();
    tombstone_rec.relay_addrs.clear();
    tombstone_rec.published_at_ms = t - 3_599_000;
    tombstone_rec.ttl_ms = 3_600_000;
    let signed2 = super::super::peer_record::sign_peer_record_v2(
        &tombstone_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &key,
    )
    .unwrap();

    assert_eq!(
        receiver.admit_serialized(&signed2).unwrap(),
        ReceiverAdmission::Accepted
    );
    assert_eq!(receiver.floor_count().unwrap(), 1);

    // Because the observed trusted time was stale (T - 3,600,000 < published_at_ms),
    // no fresh admission bound could be established. An early prune attempt at T + 31,000 ms
    // MUST NOT reclaim the floor.
    let pruned = receiver.prune_floors(t + 31_000, &BTreeSet::new()).unwrap();
    assert_eq!(pruned, 0, "stale trusted bound must not allow premature floor reclamation");
    assert!(receiver.contains_floor(&peer_id).unwrap());

    // Delivering sequence 1 while floor is preserved is rejected
    let regressed_err = receiver.admit_serialized(&signed1).unwrap_err();
    assert!(
        format!("{regressed_err:?}").contains("peer-record V2 sequence regressed"),
        "expected sequence regressed error, got {regressed_err:?}"
    );
}

#[test]
fn legacy_missing_admission_bound_must_retain_floor_after_reopen() {
    let fixture = fixture();
    let key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(key.public()).to_string();

    let t = 10_000_000i64;
    // Sequence 2: tombstone published at T - 3,599,000 ms with 3,600,000 ms TTL
    let mut tombstone_rec = record_body(&key, "peer-legacy-bound-seq2-tombstone");
    tombstone_rec.direct_addrs.clear();
    tombstone_rec.hole_punch_addrs.clear();
    tombstone_rec.relay_addrs.clear();
    tombstone_rec.published_at_ms = t - 3_599_000;
    tombstone_rec.ttl_ms = 3_600_000;
    let signed2 = super::super::peer_record::sign_peer_record_v2(
        &tombstone_rec,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &key,
    )
    .unwrap();
    let publication = signed2.publication.as_ref().unwrap();
    let identity = record_identity(&signed2, publication).unwrap();

    let mut floors = BTreeMap::new();
    floors.insert(
        peer_id.clone(),
        PeerFloorV2 {
            sequence: 2,
            signing_payload_digest_hex: encode_digest(&identity),
            primary_signed_record_cbor: canonical_record_bytes(&signed2).unwrap(),
            conflict_signed_record_cbor: None,
            isolated: false,
            max_expiry_ms: Some(tombstone_rec.published_at_ms + tombstone_rec.ttl_ms),
            admitted_at_ms: None, // Pre-upgrade legacy floor without admission timestamp
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

    // Pruning attempt at T + 31,000 ms MUST NOT reclaim sequence 2 floor when admitted_at_ms is None
    let pruned = receiver.prune_floors(t + 31_000, &BTreeSet::new()).unwrap();
    assert_eq!(pruned, 0, "legacy floor without admitted_at_ms must be retained against lower sequence resurrection");
    assert!(receiver.contains_floor(&peer_id).unwrap());
}
