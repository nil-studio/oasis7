//! Data-index regressions only. These do not construct accepted/Current owners.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
#[test]
fn receipt_any_join_preserves_duplicates_deletions_and_persistent_parent() {
    let memory = MemoryBudget::new(16 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut current = index::Index::from_rows(&[], &[], &[], &mut common).unwrap();
    current
        .replace_row(true, 0, Some(("receipt", "a")), &mut common)
        .unwrap();
    current
        .replace_row(true, 1, Some(("receipt", "a")), &mut common)
        .unwrap();
    current
        .replace_row(false, 0, Some(("receipt", "b")), &mut common)
        .unwrap();
    assert!(!current.has_receipt("receipt", &mut common).unwrap());
    current
        .replace_row(false, 1, Some(("receipt", "a")), &mut common)
        .unwrap();
    assert!(current.has_receipt("receipt", &mut common).unwrap());
    let parent = current.share(&mut common).unwrap();
    current.replace_row(true, 0, None, &mut common).unwrap();
    assert!(current.has_receipt("receipt", &mut common).unwrap());
    current.replace_row(true, 1, None, &mut common).unwrap();
    assert!(!current.has_receipt("receipt", &mut common).unwrap());
    assert!(parent.has_receipt("receipt", &mut common).unwrap());
    current
        .replace_row(true, 2, Some(("receipt", "b")), &mut common)
        .unwrap();
    assert!(current.has_receipt("receipt", &mut common).unwrap());
    current.replace_row(false, 0, None, &mut common).unwrap();
    assert!(!current.has_receipt("receipt", &mut common).unwrap());
}
#[test]
fn ordered_adversarial_inserts_keep_all_semantic_digests_and_finite_lookup() {
    let memory = MemoryBudget::new(16 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut index = index::Index::from_rows(&[], &[], &[], &mut common).unwrap();
    for i in 0..512u64 {
        let mut digest = [0; 32];
        digest[..8].copy_from_slice(&i.to_be_bytes());
        index.add_event(&digest, &mut common).unwrap();
    }
    for i in (0..512u64).rev() {
        let mut digest = [0; 32];
        digest[..8].copy_from_slice(&i.to_be_bytes());
        assert!(index.has_event(&digest, &mut common).unwrap());
    }
    assert!(!index.has_event(&[255; 32], &mut common).unwrap());
}

#[test]
fn request_retry_locator_preserves_first_ordinal_and_full_identity() {
    let memory = MemoryBudget::new(16 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut index = index::Index::from_rows(&[], &[], &[], &mut common).unwrap();
    let identity = ["agent", "session", "turn", "request", "digest"];
    index
        .replace_request_row(90, Some(identity), &mut common)
        .unwrap();
    index
        .replace_request_row(3, Some(identity), &mut common)
        .unwrap();
    let parent = index.share(&mut common).unwrap();
    assert_eq!(
        index.first_request_marker(identity, &mut common).unwrap(),
        Some(3)
    );
    for field in 0..5 {
        let mut other = identity;
        other[field] = "other";
        assert_eq!(
            index.first_request_marker(other, &mut common).unwrap(),
            None
        );
    }
    // Length framing distinguishes concatenation collisions.
    let a = ["ab", "c", "turn", "request", "digest"];
    let b = ["a", "bc", "turn", "request", "digest"];
    index.replace_request_row(1, Some(a), &mut common).unwrap();
    assert_eq!(index.first_request_marker(b, &mut common).unwrap(), None);
    index.replace_request_row(3, None, &mut common).unwrap();
    assert_eq!(
        index.first_request_marker(identity, &mut common).unwrap(),
        Some(90)
    );
    assert_eq!(
        parent.first_request_marker(identity, &mut common).unwrap(),
        Some(3)
    );
    index.replace_request_row(90, Some(b), &mut common).unwrap();
    assert_eq!(
        index.first_request_marker(identity, &mut common).unwrap(),
        None
    );
    assert_eq!(
        index.first_request_marker(b, &mut common).unwrap(),
        Some(90)
    );
}
