use super::*;

#[test]
fn prepare_reply_archive_checks_serialized_byte_boundary_including_wrappers() {
    // JSON string quotes are part of the protected row/wire bound. Independent
    // literal limits catch an off-by-one or payload-character-only check.
    let exact = "a".repeat(65_534);
    assert_eq!(bounded(&exact).unwrap().len(), 65_536);
    assert!(bounded(&"a".repeat(65_535)).is_err());
    // Escaping and multibyte text spend real encoded bytes, not chars.
    assert!(bounded(&"\\".repeat(32_768)).is_err());
    assert!(bounded(&"界".repeat(21_845)).is_err());
}

#[test]
#[ignore = "requires an externally generated actual Runtime World receipt; no bundled substitute"]
fn prepare_reply_archive_large_actual_world_material_retains_full_strong_delivery() {
    use oasis7_client_api::world_service::task_context_projection::{
        TaskContextPrepareAttestationRequestV2, TaskContextPreparedDeliveryV2,
        WorldContextAttestationV2,
    };
    // This is an explicit external World component receipt, not a qualified
    // Native/CF success fixture. No fixture body/key is logged or committed.
    let path = std::env::var_os("OASIS7_PREPARE_REPLY_WORLD_FIXTURE")
        .expect("set the actual World Prepare receipt fixture path");
    let bytes = zeroize::Zeroizing::new(std::fs::read(path).unwrap());
    let material: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let q: TaskContextPrepareAttestationRequestV2 =
        serde_json::from_value(material["request"].clone()).unwrap();
    let delivery: WorldContextAttestationV2<TaskContextPreparedDeliveryV2> =
        serde_json::from_value(material["delivery"].clone()).unwrap();
    let key = material["world"]["service_public_key"].as_str().unwrap();
    verify_prepared_delivery_v2(&q, &delivery, key).unwrap();
    let wire = zeroize::Zeroizing::new(bounded(&delivery).unwrap());
    assert!(wire.len() > 32_768 && wire.len() <= 65_536);
    let mut tampered = delivery.clone();
    tampered
        .payload
        .private_body
        .payload
        .reference_salt
        .push('0');
    assert!(verify_prepared_delivery_v2(&q, &tampered, key).is_err());
}
