use super::super::World;
use super::*;

fn artifact(value: serde_json::Value) -> Vec<u8> {
    serde_cbor::to_vec(&serde_json::json!({"module_artifact_bytes": {"hash": value}})).unwrap()
}
#[test]
fn real_module_width_passes_snapshot_only_and_typed_roundtrip() {
    let mut snapshot = World::new().snapshot();
    snapshot
        .module_artifact_bytes
        .insert("fixture".into(), vec![42; 203_153]);
    let bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let limits = ObserverReadLimits::default();
    assert!(snapshot_cbor(&bytes, limits).is_ok());
    assert!(matches!(
        cbor(&bytes, limits),
        Err(ObserverLoadError::ResourceLimited)
    ));
    let restored: super::super::Snapshot = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(
        restored.module_artifact_bytes,
        snapshot.module_artifact_bytes
    );
}
#[test]
fn artifact_schema_rejects_non_bytes_nested_and_over_allocation() {
    for value in [
        serde_json::json!([-1]),
        serde_json::json!([256]),
        serde_json::json!([1.5]),
        serde_json::json!([true]),
        serde_json::json!([[1]]),
        serde_json::json!({"nested": [1]}),
    ] {
        assert!(snapshot_cbor(&artifact(value), ObserverReadLimits::default()).is_err());
    }
    let allowed = std::mem::size_of::<WorldEvent>().max(256);
    let limits = ObserverReadLimits {
        max_single_allocation_bytes: allowed,
        ..ObserverReadLimits::default()
    };
    assert!(snapshot_cbor(&artifact(serde_json::json!([0])), limits).is_ok());
    assert!(matches!(
        snapshot_cbor(&artifact(serde_json::json!(vec![0; allowed + 1])), limits),
        Err(ObserverLoadError::ResourceLimited)
    ));
    let nested = serde_json::json!({"fake": {"module_artifact_bytes": {"hash": vec![0;203_153]}}});
    assert!(matches!(
        snapshot_cbor(
            &serde_cbor::to_vec(&nested).unwrap(),
            ObserverReadLimits::default()
        ),
        Err(ObserverLoadError::ResourceLimited)
    ));
}
#[test]
fn artifact_hint_and_total_nodes_are_bounded_before_typed_decode() {
    // {module_artifact_bytes: {hash: array(u64::MAX)}}; no payload needed.
    let mut bytes =
        serde_cbor::to_vec(&serde_json::json!({"module_artifact_bytes":{"hash":[]}})).unwrap();
    assert_eq!(bytes.pop(), Some(0x80));
    bytes.extend_from_slice(&[0x9b, 255, 255, 255, 255, 255, 255, 255, 255]);
    assert!(matches!(
        snapshot_cbor(&bytes, ObserverReadLimits::default()),
        Err(ObserverLoadError::ResourceLimited)
    ));
    let limits = ObserverReadLimits {
        max_elements: 8,
        ..ObserverReadLimits::default()
    };
    assert!(matches!(
        snapshot_cbor(
            &artifact(serde_json::json!([0, 0, 0, 0, 0, 0, 0, 0])),
            limits
        ),
        Err(ObserverLoadError::ResourceLimited)
    ));
}

#[test]
fn byte_buffer_preflight_keeps_allocation_and_node_bounds() {
    use serde_cbor::Value;
    let payload = |bytes: Vec<u8>| {
        serde_cbor::to_vec(&Value::Map(std::collections::BTreeMap::from([(
            Value::Text("module_artifact_bytes".into()),
            Value::Map(std::collections::BTreeMap::from([(
                Value::Text("hash".into()),
                Value::Bytes(bytes),
            )])),
        )])))
        .unwrap()
    };
    let bytes = payload(vec![0; 8]);
    assert!(snapshot_cbor(&bytes, ObserverReadLimits::default()).is_ok());
    // Preflight accepts bounded CBOR byte strings, but Snapshot's Vec<u8>
    // decoder remains the authority for whether that wire format is supported.
    let typed = serde_cbor::from_slice::<
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<u8>>>,
    >(&bytes);
    assert!(typed.is_err());
    let limits = ObserverReadLimits {
        max_elements: 8,
        ..ObserverReadLimits::default()
    };
    assert!(matches!(
        snapshot_cbor(&bytes, limits),
        Err(ObserverLoadError::ResourceLimited)
    ));
    let allowed = std::mem::size_of::<WorldEvent>().max(256);
    let limits = ObserverReadLimits {
        max_single_allocation_bytes: allowed,
        ..ObserverReadLimits::default()
    };
    assert!(snapshot_cbor(&payload(vec![0]), limits).is_ok());
    assert!(matches!(
        snapshot_cbor(&payload(vec![0; allowed + 1]), limits),
        Err(ObserverLoadError::ResourceLimited)
    ));
}

#[test]
fn duplicate_fields_do_not_skip_bytes_or_typed_duplicate_rejection() {
    let snapshot = World::new().snapshot();
    let mut bytes = serde_cbor::to_vec(&snapshot).unwrap();
    // Snapshot currently has >23 and <256 fields: increment the definite map
    // length and append an extra actual field without last-wins Value parsing.
    assert_eq!(bytes[0], 0xb8);
    bytes[1] += 1;
    let duplicate = artifact(serde_json::json!([0]));
    bytes.extend_from_slice(&duplicate[1..]);
    assert!(snapshot_cbor(&bytes, ObserverReadLimits::default()).is_ok());
    let error = serde_cbor::from_slice::<super::super::Snapshot>(&bytes).unwrap_err();
    assert!(error.to_string().contains("duplicate field"));

    let mut duplicate_hash = vec![0xa1];
    duplicate_hash.extend(serde_cbor::to_vec(&"module_artifact_bytes").unwrap());
    duplicate_hash.push(0xa2);
    for _ in 0..2 {
        duplicate_hash.extend(serde_cbor::to_vec(&"hash").unwrap());
        duplicate_hash.extend(serde_cbor::to_vec(&vec![0u8; 4]).unwrap());
    }
    let limits = ObserverReadLimits {
        max_elements: 12,
        ..ObserverReadLimits::default()
    };
    assert!(snapshot_cbor(&artifact(serde_json::json!([0, 0, 0, 0])), limits).is_ok());
    assert!(matches!(
        snapshot_cbor(&duplicate_hash, limits),
        Err(ObserverLoadError::ResourceLimited)
    ));
}
