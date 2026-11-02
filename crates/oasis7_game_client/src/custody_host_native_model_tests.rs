//! Local component boundaries, not different-UID or Native/provider qualification.
use super::*;
#[test]
fn native_channel_missing_actual_session_stays_off_without_a_host_attempt() {
    let f = Fixture::new();
    let host = Host::open(f.vault()).unwrap();
    let before = host
        .vault
        .read(|s| serde_json::to_vec(s).map_err(|_| denied()))
        .unwrap();
    assert!(ipc::native::binding(&host, &f.path.join("native.sock")).is_err());
    assert_eq!(
        before,
        host.vault
            .read(|s| serde_json::to_vec(s).map_err(|_| denied()))
            .unwrap()
    );
}
#[test]
fn native_channel_observes_kernel_peer_instead_of_claimed_native_uid() {
    let f = Fixture::new();
    let identity = f.identity();
    let (stream, _other) = std::os::unix::net::UnixStream::pair().unwrap();
    assert_eq!(ipc::peer_uid(&stream).unwrap(), vault::actual_uid());
    let NativePlacement::Local { uid } = &identity.isolation.native else {
        panic!("fixture local");
    };
    assert_ne!(*uid, vault::actual_uid());
    let b = ipc::PublicBinding {
        api_version: 1,
        transport_profile: "native-model-custody-v2".into(),
        socket_path: "/unused".into(),
        identity_digest: identity.digest().unwrap(),
        descriptor_digest: "11".repeat(32),
        session_digest: "22".repeat(32),
        resource_id: "resource".into(),
        restricted_runtime_identity: identity.descriptor.restricted_runtime_identity.clone(),
        isolation: identity.isolation.clone(),
    };
    assert!(native_model::NativePeer::observe(&stream, &identity, &b).is_err());
}
#[test]
fn native_pending_wire_does_not_forward_upstream_secret_or_floor_fields() {
    let good = serde_json::json!({"api_version":2,"domain":"oasis7.custody.native-model-reservation-pending.v2","session_id":"session","reservation_id":"reservation","query_digest":"11".repeat(32),"status":"paid_pending"});
    assert!(serde_json::from_value::<native_model::PendingReply>(good.clone()).is_ok());
    for field in [
        "host_token",
        "provider_key",
        "native_sid",
        "floor",
        "headers",
        "body",
    ] {
        let mut changed = good.clone();
        changed[field] = serde_json::json!("canary-private-upstream-field");
        assert!(serde_json::from_value::<native_model::PendingReply>(changed).is_err());
    }
}
#[test]
fn native_reserve_is_not_an_m_control_verb_or_arbitrary_transport() {
    let original = serde_json::json!({"api_version":1,"resource_id":"resource","request_id":"request","method":"tools_list"});
    assert!(serde_json::from_value::<control::ModelRequest>(original.clone()).is_ok());
    for method in [
        "model_reserve",
        "native_model",
        "inference",
        "provider_call",
    ] {
        let mut changed = original.clone();
        changed["method"] = serde_json::json!(method);
        assert!(serde_json::from_value::<control::ModelRequest>(changed).is_err());
    }
}
