use super::*;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn source() -> BftSuccessorWindowSourceV1 {
    BftSuccessorWindowSourceV1 {
        scope: BftScopeV2 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: h(1),
        },
        binding_digest: h(2),
        handoff_position: 2,
        handoff_record_id: h(3),
        handoff_snapshot_root: h(4),
        target_digest: h(5),
        activation_digest: h(6),
        material_identity: h(7),
    }
}

#[test]
fn successor_source_preserves_original_canonical_identity_and_domain() {
    let original = source();
    let expected = serde_json::json!({
        "scope": {"world_id":"world", "chain_id":"chain", "genesis_hash":h(1)},
        "binding_digest":h(2), "handoff_position":2, "handoff_record_id":h(3),
        "handoff_snapshot_root":h(4), "target_digest":h(5),
        "activation_digest":h(6), "material_identity":h(7)
    });
    let canonical = oasis7_cas::to_canonical_cbor(&expected).unwrap();
    assert_eq!(oasis7_cas::to_canonical_cbor(&original).unwrap(), canonical);
    let domain = b"oasis7.bft.successor-window.v1.origin";
    let mut framed = (domain.len() as u64).to_be_bytes().to_vec();
    framed.extend(domain);
    framed.extend(&canonical);
    assert_eq!(
        original.origin_digest().unwrap(),
        blake3::hash(&framed).to_hex().to_string()
    );
}

#[test]
fn successor_source_binds_every_original_identity_component() {
    let original = source();
    let digest = original.origin_digest().unwrap();
    let mut values = Vec::new();
    for field in [
        "binding_digest",
        "handoff_record_id",
        "handoff_snapshot_root",
        "target_digest",
        "activation_digest",
        "material_identity",
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value[field] = serde_json::json!(h(30));
        values.push(value);
    }
    for field in ["world_id", "chain_id", "genesis_hash"] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["scope"][field] = serde_json::json!(h(30));
        values.push(value);
    }
    let mut value = serde_json::to_value(&original).unwrap();
    value["handoff_position"] = serde_json::json!(3);
    values.push(value);
    for value in values {
        let changed: BftSuccessorWindowSourceV1 = serde_json::from_value(value).unwrap();
        assert_ne!(changed.origin_digest().unwrap(), digest);
    }
}

#[test]
fn successor_source_retains_strict_hash_and_height_bounds() {
    for invalid_hash in ["A".repeat(64), "0".repeat(63), "g".repeat(64)] {
        let mut changed = source();
        changed.binding_digest = invalid_hash;
        assert!(changed.origin_digest().is_err());
    }
    for position in [0, u64::MAX] {
        let mut changed = source();
        changed.handoff_position = position;
        assert!(changed.origin_digest().is_err());
    }
    let mut edge = source();
    edge.handoff_position = u64::MAX - 1;
    edge.validate().unwrap();
}

#[test]
fn successor_source_retains_identity_bounds_and_rejects_unknown_fields() {
    for invalid_identity in [
        "".to_string(),
        " world".into(),
        "world ".into(),
        "x".repeat(257),
    ] {
        let mut changed = source();
        changed.scope.world_id = invalid_identity;
        assert!(changed.origin_digest().is_err());
    }
    let mut edge = source();
    edge.scope.world_id = "x".repeat(256);
    edge.validate().unwrap();
    let mut value = serde_json::to_value(source()).unwrap();
    value["authority"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BftSuccessorWindowSourceV1>(value).is_err());
}

#[test]
fn successor_source_canonical_roundtrip_preserves_identity() {
    let original = source();
    let bytes = oasis7_cas::to_canonical_cbor(&original).unwrap();
    let decoded: BftSuccessorWindowSourceV1 = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(
        decoded.origin_digest().unwrap(),
        original.origin_digest().unwrap()
    );
}
