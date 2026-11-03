//! Transport ownership/codec tests only. These bytes provide no World or
//! independent issuer permission evidence; Runtime exercises the real gate.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1, BftPrivateRetentionInputObjectKindV1,
    BftPrivateRetentionInputObjectV1, BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1,
    BftSuccessorWindowArchiveRefV1,
};
use std::time::Duration;
fn input() -> Input {
    let mut objects = [
        (
            BftPrivateRetentionInputObjectKindV1::OriginalData,
            vec![1, 2],
        ),
        (
            BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
            vec![3, 4],
        ),
    ]
    .into_iter()
    .map(|(kind, bytes)| BftPrivateRetentionInputObjectV1 {
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            bytes: bytes.len() as u64,
        },
        kind,
        bytes,
    })
    .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    Input {
        schema_version: 1,
        publication_reservation_bytes: 1024,
        source: BftPrivateRetentionSourceV1 {
            binding_digest: "11".repeat(32),
            handoff_digest: "22".repeat(32),
            material_source_digest: "33".repeat(32),
        },
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: "44".repeat(32),
            height: 7,
            header_id: "55".repeat(32),
        },
        objects,
    }
}
#[test]
fn admission_publisher_keeps_original_and_shared_budget_until_last_reply_drops() {
    let buffers = Arc::new(crate::NodeBftPrivateTransportBuffersV1::default());
    let publisher = Arc::new(NodeBftAdmissionInputPublisherV1::new(buffers.clone()));
    let original = input();
    let reference = publisher
        .reserve()
        .unwrap()
        .publish(original.clone())
        .unwrap();
    let reply = publisher
        .original(&reference, Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert!(reply.bytes.belongs_to(&buffers));
    assert_eq!(Input::from_canonical_bytes(&reply.bytes).unwrap(), original);
    assert!(publisher.reserve().is_err());
    assert!(
        buffers
            .outbound(2 * super::super::wire::MAX_PACKAGE)
            .is_err()
    );
    let mut different = reference.clone();
    different.key.height += 1;
    assert!(publisher.clear(&different).is_err());
    assert!(
        publisher
            .original(&different, Instant::now() + Duration::from_secs(1))
            .is_err()
    );
    assert!(publisher.original(&reference, Instant::now()).is_err());
    publisher.clear(&reference).unwrap();
    assert!(
        buffers
            .outbound(2 * super::super::wire::MAX_PACKAGE)
            .is_err(),
        "an active immutable reply still owns actual bytes"
    );
    drop(reply);
    assert!(
        buffers
            .outbound(2 * super::super::wire::MAX_PACKAGE)
            .is_ok()
    );
    assert!(publisher.reserve().unwrap().publish(input()).is_ok());
}
#[test]
fn admission_failed_encoding_returns_original_shared_reservation() {
    let buffers = Arc::new(crate::NodeBftPrivateTransportBuffersV1::default());
    let publisher = Arc::new(NodeBftAdmissionInputPublisherV1::new(buffers.clone()));
    let mut invalid = input();
    invalid.objects[0].bytes.push(9);
    assert!(publisher.reserve().unwrap().publish(invalid).is_err());
    assert!(
        buffers
            .outbound(2 * super::super::wire::MAX_PACKAGE)
            .is_ok()
    );
    assert!(publisher.reserve().unwrap().publish(input()).is_ok());
}
#[test]
fn admission_read_owns_inbound_peak_through_consumer_validation() {
    let buffers = crate::NodeBftPrivateTransportBuffersV1::default();
    let hold = buffers
        .inbound(2 * super::super::wire::MAX_PACKAGE)
        .unwrap();
    let original = input();
    let reference = original.descriptor().unwrap();
    let read = BftPrivateRetentionAdmissionInputReadV1::new(Arc::new(original), Box::new(hold));
    reference.validate_input(read.input()).unwrap();
    assert!(
        buffers.inbound(1).is_err(),
        "consumer validation must not release a live DTO permit"
    );
    drop(read);
    assert!(buffers.inbound(2 * super::super::wire::MAX_PACKAGE).is_ok());
}

#[test]
fn admission_publication_slot_reserves_before_original_copy_and_releases_on_abort() {
    let buffers = Arc::new(crate::NodeBftPrivateTransportBuffersV1::default());
    let publisher = Arc::new(NodeBftAdmissionInputPublisherV1::new(buffers.clone()));
    let slot = publisher.reserve().unwrap();
    assert!(buffers.outbound(1).is_err());
    assert!(publisher.reserve().is_err());
    drop(slot);
    assert!(
        buffers
            .outbound(2 * super::super::wire::MAX_PACKAGE)
            .is_ok()
    );
    assert!(publisher.reserve().is_ok());
}
