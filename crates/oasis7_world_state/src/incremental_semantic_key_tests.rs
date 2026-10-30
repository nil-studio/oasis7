use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::sync::atomic::{AtomicBool, Ordering};
fn encoded<K: SemanticKeyV1 + ?Sized>(key: &K) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_semantic_key_v1(key, &mut bytes).unwrap();
    bytes
}
#[test]
fn independent_vector_preserves_unicode_extremes_and_typed_pair_boundaries() {
    let key = (MaterialLedgerId::agent("汉"), u64::MAX);
    let bytes = encoded(&key);
    // Independent Python struct.pack big-endian vector, not encoder output.
    let vector = hex::decode("6f61736973372e6977732e73656d616e7469632d6b65792e763100050401010000000000000003e6b18902ffffffffffffffff").unwrap();
    assert_eq!(bytes, vector);
    let SemanticKeyRefV1::Pair { left, right } = read_semantic_key_v1(&bytes).unwrap() else {
        panic!("pair expected")
    };
    assert_eq!(
        left.decode().unwrap(),
        SemanticKeyRefV1::Ledger(LedgerKeyRefV1::Agent("汉"))
    );
    assert_eq!(
        right.decode().unwrap(),
        SemanticKeyRefV1::Unsigned(u64::MAX)
    );
    let memory = MemoryBudget::new(8192, 4096).unwrap();
    let mut budget = SemanticKeyBudget {
        bytes_remaining: bytes.len() as u64,
    };
    let hash =
        hash_semantic_key_v1(&key, &memory, MemoryClass::Ordinary, &mut budget, || false).unwrap();
    assert_eq!(hash.digest, *blake3::hash(&vector).as_bytes());
    assert_eq!(hash.encoded_bytes, bytes.len() as u64);
    assert_eq!(budget.bytes_remaining, 0);
    assert_eq!(memory.used(), 0);
    assert_ne!(encoded(&("a", "bc")), encoded(&("ab", "c")));
    assert_ne!(encoded(&MaterialLedgerId::world()), encoded("world"));
    assert_eq!(encoded(""), encoded(&String::new()));
    assert_eq!(
        read_semantic_key_v1(&encoded(&ResourceKind::Electricity)).unwrap(),
        SemanticKeyRefV1::Resource(ResourceKind::Electricity)
    );
    assert_eq!(
        read_semantic_key_v1(&encoded(&ResourceKind::Data)).unwrap(),
        SemanticKeyRefV1::Resource(ResourceKind::Data)
    );
    assert_eq!(
        read_semantic_key_v1(&encoded(&())).unwrap(),
        SemanticKeyRefV1::Unit
    );
    for (ledger, expected) in [
        (MaterialLedgerId::world(), LedgerKeyRefV1::World),
        (MaterialLedgerId::site(""), LedgerKeyRefV1::Site("")),
        (
            MaterialLedgerId::factory("工厂"),
            LedgerKeyRefV1::Factory("工厂"),
        ),
    ] {
        let wire = encoded(&ledger);
        assert_eq!(
            read_semantic_key_v1(&wire).unwrap(),
            SemanticKeyRefV1::Ledger(expected)
        );
    }
}
#[test]
fn malformed_wire_and_excessive_pair_depth_are_rejected() {
    let bytes = encoded(&(MaterialLedgerId::site("site"), 0u64));
    for length in 0..bytes.len() {
        assert!(read_semantic_key_v1(&bytes[..length]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(read_semantic_key_v1(&trailing).is_err());
    let mut wrong = bytes.clone();
    wrong[0] ^= 1;
    assert!(read_semantic_key_v1(&wrong).is_err());
    for token in [
        vec![6],
        vec![3, 0],
        vec![3, 3],
        vec![4, 4],
        vec![4, 1, 0],
        vec![1, 255, 255, 255, 255, 255, 255, 255, 255],
        vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 255],
    ] {
        let mut wire = SEMANTIC_KEY_DOMAIN.to_vec();
        wire.extend_from_slice(&token);
        assert!(read_semantic_key_v1(&wire).is_err());
    }
    macro_rules! nest { ($value:expr;) => { $value }; ($value:expr; x $($rest:tt)*) => { ((), nest!($value; $($rest)*)) }; }
    let allowed = nest!(() ; x x x x x x x x x x x x x x x x);
    let wire = encoded(&allowed);
    assert!(read_semantic_key_v1(&wire).is_ok());
    let excessive = ((), allowed);
    let mut output = Vec::new();
    assert!(write_semantic_key_v1(&excessive, &mut output).is_err());
    assert!(read_semantic_key_v1(&output).is_err());
    let mut wire = SEMANTIC_KEY_DOMAIN.to_vec();
    for _ in 0..17 {
        wire.extend_from_slice(&[5, 0]);
    }
    wire.push(0);
    assert!(read_semantic_key_v1(&wire).is_err());
    let memory = MemoryBudget::new(8192, 4096).unwrap();
    let mut budget = SemanticKeyBudget {
        bytes_remaining: 1024,
    };
    assert!(matches!(
        hash_semantic_key_v1(
            &excessive,
            &memory,
            MemoryClass::Ordinary,
            &mut budget,
            || false
        ),
        Err(SemanticKeyError::Invalid(_))
    ));
    assert!(budget.bytes_remaining < 1024);
    assert_eq!(memory.used(), 0);
}
#[test]
fn exact_byte_work_and_memory_class_are_enforced_without_partial_success() {
    let memory = MemoryBudget::new(8192, 4096).unwrap();
    let bytes = encoded("key");
    let mut budget = SemanticKeyBudget {
        bytes_remaining: bytes.len() as u64 - 1,
    };
    assert!(matches!(
        hash_semantic_key_v1("key", &memory, MemoryClass::Ordinary, &mut budget, || false),
        Err(SemanticKeyError::ResourceLimited)
    ));
    assert_eq!(memory.used(), 0);
    let full = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut budget = SemanticKeyBudget {
        bytes_remaining: bytes.len() as u64,
    };
    assert!(matches!(
        hash_semantic_key_v1("key", &memory, MemoryClass::Ordinary, &mut budget, || false),
        Err(SemanticKeyError::Memory(_))
    ));
    assert_eq!(budget.bytes_remaining, bytes.len() as u64);
    let hash =
        hash_semantic_key_v1("key", &memory, MemoryClass::Recovery, &mut budget, || false).unwrap();
    assert_eq!(hash.digest, *blake3::hash(&bytes).as_bytes());
    assert_eq!(memory.used(), 4096);
    drop(full);
    assert_eq!(memory.used(), 0);
}
#[path = "incremental_semantic_key_stream_tests.rs"]
mod stream;
