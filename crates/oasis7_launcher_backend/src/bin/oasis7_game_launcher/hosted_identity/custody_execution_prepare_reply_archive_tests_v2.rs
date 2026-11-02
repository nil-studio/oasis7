use super::*;

#[test]
fn prepare_reply_archive_checks_serialized_byte_boundary_including_wrappers() {
    use oasis7_custody_core::native_execution_prepare_v2::MAX_NATIVE_EXECUTION_PREPARE_REPLY_BYTES_V2;
    // The public Native reply profile, rather than the request-body bound, is
    // authoritative. This exercises the archive's actual serialization guard;
    // it is a bound-unit fixture, not a cryptographically valid large reply.
    let maximum = MAX_NATIVE_EXECUTION_PREPARE_REPLY_BYTES_V2;
    assert_eq!(maximum, 256 * 1024);
    let exact = "a".repeat(maximum - 2);
    assert_eq!(bounded(&exact).unwrap().len(), maximum);
    assert!(bounded(&"a".repeat(maximum - 1)).is_err());
    // JSON escaping and UTF-8 spend encoded bytes rather than character count.
    assert!(bounded(&"\\".repeat(maximum / 2)).is_err());
    assert!(bounded(&"界".repeat(maximum / 3)).is_err());
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
