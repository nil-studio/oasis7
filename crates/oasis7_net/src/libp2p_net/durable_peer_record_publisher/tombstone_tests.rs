use super::super::PendingPublicationV2;
use super::*;
use crate::libp2p_net::config::PeerRecordPublicationTransition;
use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};

#[test]
fn withdrawal_is_idempotent_across_restart_and_resume_allocates_a_higher_live_sequence() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let mut template = sample_record(&keypair, 0);
    template.direct_addrs = vec!["/ip4/192.0.2.10/tcp/4103".into()];
    directory.initialize(&keypair, &template);

    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
    let mut live = template.clone();
    live.published_at_ms = crate::util::unix_now_ms_i64();
    let first_live = publisher
        .publish_materialized(&live)
        .expect("publish initial live declaration");
    assert_eq!(first_live.publication.expect("V2").sequence, 1);

    publisher
        .apply_transition(
            PeerRecordPublicationTransition::Withdraw,
            &template,
            &keypair,
        )
        .expect("persist withdrawal");
    let tombstone = publisher
        .publish_materialized(&live)
        .expect("periodic refresh replays the tombstone");
    assert_eq!(
        tombstone.publication,
        Some(PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Tombstone,
        })
    );
    assert!(tombstone.record.direct_addrs.is_empty());
    assert!(tombstone.record.hole_punch_addrs.is_empty());
    assert!(tombstone.record.relay_addrs.is_empty());
    let tombstone_bytes = super::super::canonical_cbor(&tombstone).expect("encode tombstone");
    let state_path = directory.0.join(super::super::STATE_FILE_NAME);
    let persisted_withdrawal = std::fs::read(&state_path).expect("read withdrawn state");
    drop(publisher);

    let mut reopened =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    reopened
        .apply_transition(
            PeerRecordPublicationTransition::Withdraw,
            &template,
            &keypair,
        )
        .expect("repeated withdrawal resumes the same reservation");
    let repeated = reopened
        .publish_materialized(&live)
        .expect("ordinary refresh remains withdrawn");
    assert_eq!(
        super::super::canonical_cbor(&repeated).expect("encode repeated tombstone"),
        tombstone_bytes,
        "restarting or repeating Withdraw cannot allocate or sign a new sequence"
    );
    assert_eq!(
        std::fs::read(&state_path).expect("read state after repeated withdrawal"),
        persisted_withdrawal,
        "repeating an already completed transition does not rewrite durable state"
    );

    reopened
        .apply_transition(PeerRecordPublicationTransition::Resume, &template, &keypair)
        .expect("persist explicit resume intent");
    drop(reopened);
    let mut resumed =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen resume");
    live.published_at_ms = crate::util::unix_now_ms_i64();
    let resumed_live = resumed
        .publish_materialized(&live)
        .expect("explicit resume publishes live");
    assert_eq!(
        resumed_live.publication,
        Some(PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 3,
            state: PeerRecordPublicationStateV2::Live,
        })
    );
    assert_eq!(resumed_live.record.direct_addrs, template.direct_addrs);
}

#[test]
fn withdrawn_reservation_recovers_exact_tombstone_without_signing_live() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
    let mut live = template.clone();
    live.published_at_ms = crate::util::unix_now_ms_i64();
    publisher
        .publish_materialized(&live)
        .expect("publish initial live declaration");

    let mut tombstone_record = template.clone();
    tombstone_record.peer_id = publisher.state.binding.peer_id.clone();
    tombstone_record.direct_addrs.clear();
    tombstone_record.hole_punch_addrs.clear();
    tombstone_record.relay_addrs.clear();
    tombstone_record.published_at_ms = crate::util::unix_now_ms_i64();
    let tombstone_record = publisher
        .normalize_and_check_tombstone_record(&tombstone_record)
        .expect("normalize reserved tombstone");
    let sequence = publisher.next_sequence().expect("next sequence");
    let mut pending_state = publisher.state.clone();
    pending_state.state = PeerRecordPublicationStateV2::Tombstone;
    pending_state.last_sequence = sequence;
    pending_state.pending = Some(PendingPublicationV2 {
        sequence,
        materialized_record_cbor: super::super::canonical_cbor(&tombstone_record)
            .expect("encode reservation"),
        signed_record_cbor: None,
        state: PeerRecordPublicationStateV2::Tombstone,
    });
    publisher
        .persist_state(pending_state)
        .expect("persist crash point after tombstone reservation");
    drop(publisher);

    let mut reopened =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("reopen");
    reopened
        .apply_transition(
            PeerRecordPublicationTransition::Withdraw,
            &template,
            &keypair,
        )
        .expect("complete the original reservation");
    let recovered = reopened
        .publish_materialized(&live)
        .expect("normal refresh cannot replace a withdrawn reservation");
    assert_eq!(
        recovered.publication,
        Some(PeerRecordPublicationV2 {
            schema_version: 2,
            sequence,
            state: PeerRecordPublicationStateV2::Tombstone,
        })
    );
    assert_eq!(recovered.record, tombstone_record);
}

#[test]
fn resume_never_reuses_a_tombstone_reservation_with_identical_record_bytes() {
    let directory = TestDirectory::new();
    let keypair = Keypair::generate_ed25519();
    let template = sample_record(&keypair, 0);
    directory.initialize(&keypair, &template);
    let mut publisher =
        DurablePeerRecordPublisher::open(&directory.0, &keypair, &template).expect("open");
    let mut live = template.clone();
    live.published_at_ms = crate::util::unix_now_ms_i64();
    publisher
        .publish_materialized(&live)
        .expect("publish initial live declaration");
    publisher
        .apply_transition(
            PeerRecordPublicationTransition::Withdraw,
            &template,
            &keypair,
        )
        .expect("withdraw publication");
    let reserved_tombstone: PeerRecord = serde_cbor::from_slice(
        &publisher
            .state
            .pending
            .as_ref()
            .expect("tombstone reservation")
            .materialized_record_cbor,
    )
    .expect("decode tombstone reservation");

    publisher
        .apply_transition(PeerRecordPublicationTransition::Resume, &template, &keypair)
        .expect("explicitly resume publication");
    // The body and timestamp deliberately match the tombstone reservation. The
    // state byte is the only signed-payload difference, so this must allocate
    // sequence 3 rather than return or rewrite the old sequence-2 signature.
    live.published_at_ms = reserved_tombstone.published_at_ms;
    let resumed = publisher
        .publish_materialized(&live)
        .expect("publish resumed live state");
    assert_eq!(resumed.record, reserved_tombstone);
    assert_eq!(
        resumed.publication,
        Some(PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 3,
            state: PeerRecordPublicationStateV2::Live,
        })
    );
}
