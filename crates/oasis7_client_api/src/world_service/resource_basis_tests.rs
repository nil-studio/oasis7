use super::*;
use crate::world_service::{
    SignedReadRequest, WorldIdentity, entry_control::EntryOwnerOperationV1,
    private_goal_read::CurrentGoalRequest,
};
#[test]
fn resource_basis_decimal_wire_is_exact_and_closed() {
    for value in [i64::MIN, -9007199254740993, 0, 9007199254740993, i64::MAX] {
        let bytes = serde_json::to_vec(&ResourceDecimalI64V1(value)).unwrap();
        assert_eq!(
            serde_json::from_slice::<ResourceDecimalI64V1>(&bytes)
                .unwrap()
                .0,
            value
        );
        assert_eq!(bytes, format!("\"{value}\"").as_bytes());
    }
    for bad in [
        "0",
        "-1",
        "1.5",
        "\"-0\"",
        "\"+1\"",
        "\"01\"",
        "\"9223372036854775808\"",
        "\"-9223372036854775809\"",
    ] {
        assert!(
            serde_json::from_str::<ResourceDecimalI64V1>(bad).is_err(),
            "{bad}"
        );
    }
    let good = r#"{"kind":"transfer","from":{"kind":"agent","agent_id":"a"},"to":{"kind":"world"},"material":"iron_ore","amount":"9007199254740993","distance_km":"0","priority":"standard","routing":{"kind":"direct"}}"#;
    let selection: ResourceBasisSelectionV1 = serde_json::from_str(good).unwrap();
    selection.validate().unwrap();
    assert!(
        serde_json::from_str::<ResourceBasisSelectionV1>(&good.replace(
            "\"amount\":\"9007199254740993\"",
            "\"amount\":9007199254740993"
        ))
        .is_err()
    );
    assert!(
        serde_json::from_str::<ResourceBasisSelectionV1>(&good.replace(
            "\"amount\":\"9007199254740993\"",
            "\"amount\":\"1\",\"amount\":\"2\""
        ))
        .is_err()
    );
    assert!(
        serde_json::from_str::<ResourceBasisSelectionV1>(
            &good.replace("\"material\":", "\"verified\":true,\"material\":")
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<ResourceBasisSelectionV1>(
            &good.replace("\"agent\",\"agent_id\":\"a\"", "\"world\"")
        )
        .unwrap()
        .validate()
        .is_err()
    );
}
#[test]
fn resource_basis_legacy_read_goal_bytes_have_no_v2_fields() {
    let legacy = EntryOwnerOperationV1::ReadGoal(SignedReadRequest {
        request: CurrentGoalRequest {
            schema_revision: 1,
            world: WorldIdentity {
                world_id: "w".into(),
                genesis_digest: "g".into(),
            },
            agent_id: "a".into(),
            task_id: "t".into(),
        },
        subject_public_key: "k".into(),
        signature_hex: "s".into(),
    });
    let bytes = serde_json::to_string(&legacy).unwrap();
    assert_eq!(
        bytes,
        r#"{"kind":"read_goal","request":{"request":{"schema_revision":1,"world":{"world_id":"w","genesis_digest":"g"},"agent_id":"a","task_id":"t"},"subject_public_key":"k","signature_hex":"s"}}"#
    );
}
#[test]
fn resource_basis_nullable_maps_are_required_and_reject_duplicate_keys() {
    let base = r#"{"payer_agent_id":"a","profile_key":"f","profile_revision":"1","electricity_amount":"1","mode":"start_only_sink","material_ledger":null,"electricity_before":null,"electricity_after":null,"material_balances_before":{"ore":"1"},"material_balances_after":null}"#;
    assert!(serde_json::from_str::<ResourceBuildPowerV1>(base).is_ok());
    assert!(
        serde_json::from_str::<ResourceBuildPowerV1>(
            &base.replace("\"ore\":\"1\"", "\"ore\":\"1\",\"ore\":\"2\"")
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<ResourceBuildPowerV1>(
            &base.replace(",\"material_balances_after\":null", "")
        )
        .is_err()
    );
}
#[test]
fn resource_basis_final_native_envelope_is_measured_exactly() {
    let sample = SignedResourceResponseV1 {
        request_digest: "ab".repeat(32),
        payload: "x".to_owned(),
        signature_hex: "cd".repeat(64),
    };
    let overhead = encode_native_resource_envelope(sample.clone())
        .unwrap()
        .len()
        - 1;
    let mut exact = sample.clone();
    exact.payload = "x".repeat(MAX_RESOURCE_ENVELOPE_BYTES - overhead);
    assert_eq!(
        encode_native_resource_envelope(exact.clone())
            .unwrap()
            .len(),
        MAX_RESOURCE_ENVELOPE_BYTES
    );
    exact.payload.push('x');
    assert!(encode_native_resource_envelope(exact).is_err());
    // Escapes are measured after the real serializer, not counted as input bytes.
    let escaped = SignedResourceResponseV1 {
        payload: "\"".repeat(MAX_RESOURCE_ENVELOPE_BYTES / 2),
        ..sample
    };
    assert!(encode_native_resource_envelope(escaped).is_err());
}
