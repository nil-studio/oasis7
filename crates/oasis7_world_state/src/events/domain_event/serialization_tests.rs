use super::DomainEvent;
use sha2::{Digest, Sha256};

#[path = "serialization_call_trace_test_support.rs"]
mod call_trace;

const JSON: &[u8] = include_bytes!("serialization_fixtures/domain-event-all-variants-v1.json");
const CBOR: &[u8] = include_bytes!("serialization_fixtures/domain-event-all-variants-v1.cbor");
const PACKED: &[u8] =
    include_bytes!("serialization_fixtures/domain-event-all-variants-v1.packed.cbor");
const TRACE: &[u8] =
    include_bytes!("serialization_fixtures/domain-event-all-variants-v1.trace.json");

#[test]
fn domain_event_serialization_retains_legacy_bytes_and_public_calls() {
    let manifest: serde_json::Value = serde_json::from_slice(include_bytes!(
        "serialization_fixtures/domain-event-all-variants-v1.manifest.json"
    ))
    .expect("legacy fixture manifest");
    for (label, bytes, expected) in [
        ("json", JSON, manifest["json_fixture"]["sha256"].as_str()),
        (
            "cbor",
            CBOR,
            manifest["regular_cbor_oracle"]["sha256"].as_str(),
        ),
        (
            "packed",
            PACKED,
            manifest["packed_cbor_oracle"]["sha256"].as_str(),
        ),
        ("trace", TRACE, manifest["trace_sha256"].as_str()),
    ] {
        assert_eq!(
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            expected.expect("legacy artifact SHA"),
            "{label} legacy artifact identity"
        );
    }

    let events: Vec<DomainEvent> = serde_json::from_slice(JSON).expect("decode legacy JSON");
    assert_eq!(events.len(), 208);
    assert_eq!(manifest["base_variant_count"], 104);
    assert_eq!(manifest["trace_event_values"], 208);
    assert_eq!(serde_json::to_vec(&events).expect("encode JSON"), JSON);
    assert_eq!(serde_cbor::to_vec(&events).expect("encode CBOR"), CBOR);
    assert_eq!(
        serde_cbor::ser::to_vec_packed(&events).expect("encode packed CBOR"),
        PACKED
    );
    for (label, bytes) in [("cbor", CBOR), ("packed", PACKED)] {
        let decoded: Vec<DomainEvent> = serde_cbor::from_slice(bytes).expect(label);
        assert_eq!(decoded.len(), events.len());
        for (index, (expected, actual)) in events.iter().zip(&decoded).enumerate() {
            assert_eq!(expected, actual, "{label} event {index}");
        }
    }
    let traces = events
        .iter()
        .map(call_trace::record_public_serde_calls)
        .collect::<Result<Vec<_>, _>>()
        .expect("record public Serde calls");
    assert_eq!(
        serde_json::to_vec_pretty(&traces).expect("encode call trace"),
        TRACE
    );
}
