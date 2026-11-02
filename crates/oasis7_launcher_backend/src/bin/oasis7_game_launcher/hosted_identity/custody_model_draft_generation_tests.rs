//! Pure contract contrast from a real authenticated owned Draft; not vendor or graph qualification.
use super::*;
use oasis7_custody_core::approval::{NativeModelProxyBasisV2, NativeModelProxyConnectionBindingV2};
use oasis7_model_provider::{
    connection::{
        BindingTarget, ConnectionManager, ConnectionState, Limits, MonotonicClock, Principal,
    },
    preset::{Protocol, ProviderId, ProviderPreset, RequestLimits},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
#[test]
fn native_proxy_basis_accepts_actual_unprobed_owned_draft_generation_zero() {
    let (mut fixture, _) = fixture();
    let now = now_ms();
    let principal = fixture
        .service
        .model_principal(
            &format!(
                "GET / HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\n",
                fixture.token
            ),
            false,
        )
        .unwrap();
    assert!(fixture.service.model_session_current(&principal));
    let native_public = hex::encode(
        fixture
            .service
            .custody
            .as_ref()
            .unwrap()
            .key
            .verifying_key()
            .to_bytes(),
    );
    let mut request = session_options(now);
    let mut budget=super::super::model_budget_summary_tests::model_v2_options(now)["intent"]["plan"]["model_budget"].clone();
    budget["approval_contract"] = json!("native_custody_model_proxy_v2");
    budget["native_account_id"] = json!(principal.account_id);
    budget["native_public_key"] = json!(native_public);
    budget["native_instance_public_key"] = json!(hex::encode(
        SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes()
    ));
    budget["preset_id"] = json!("draft-contract");
    budget["model"] = json!("deepseek-chat");
    request["intent"]["plan"]["model_budget"] = budget;
    let request: ApprovalOptionsRequestV1 = serde_json::from_value(request).unwrap();
    request.validate(now).unwrap();
    let identity = Arc::new(Mutex::new(Some(fixture.service)));
    let actual = Arc::clone(&identity);
    let ordinary_bindings = Arc::new(AtomicUsize::new(0));
    let binding_spy = Arc::clone(&ordinary_bindings);
    let manager = ConnectionManager::new(
        Arc::new(move |p: &Principal| {
            actual
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .model_session_current(p)
        }),
        Arc::new(move |_: &Principal, _: &BindingTarget| {
            binding_spy.fetch_add(1, Ordering::SeqCst);
            false
        }),
        Arc::new(MonotonicClock::default()),
        Limits::default(),
    )
    .unwrap();
    let preset = ProviderPreset {
        id: "draft-contract".into(),
        provider: ProviderId::DeepSeek,
        protocol: Protocol::ChatCompletions,
        model: "deepseek-chat".into(),
        routes: vec![],
        revision: 1,
        limits: RequestLimits::default(),
    };
    let created = manager
        .create(
            &principal,
            preset,
            zeroize::Zeroizing::new(b"local-contract-key-never-sent".to_vec()),
            true,
        )
        .unwrap();
    assert_eq!(created.state, ConnectionState::Draft);
    assert_eq!(created.generation, 0);
    assert!(created.active_version.is_none());
    let metadata = manager
        .custody_connection_metadata(&principal.account_id, &principal.session_id, &created.id)
        .unwrap();
    assert_eq!(metadata.generation, 0);
    assert_eq!(metadata.version, created.version);
    let instance = hex::encode(SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes());
    let ref_digest = oasis7_agent_api::h_v1(
        "oasis7.custody.native-model-connection.v2",
        &(
            &metadata.connection_id,
            &metadata.account_id,
            &metadata.session_id,
            metadata.version,
            metadata.generation,
            &instance,
        ),
    );
    let exact = NativeModelProxyConnectionBindingV2 {
        native_account_id: metadata.account_id.clone(),
        native_instance_public_key: instance,
        connection_ref_digest: ref_digest.as_str().strip_prefix("blake3:").unwrap().into(),
        connection_generation: DecimalU64(metadata.generation),
        endpoint_policy_digest: "62".repeat(32),
    };
    // Only a component contrast: generation1 is not written to real metadata and
    // does not pretend that an ordinary probe, provider qualification or Ready occurred.
    let contrast = NativeModelProxyConnectionBindingV2 {
        native_account_id: exact.native_account_id.clone(),
        native_instance_public_key: exact.native_instance_public_key.clone(),
        connection_ref_digest: exact.connection_ref_digest.clone(),
        connection_generation: DecimalU64(1),
        endpoint_policy_digest: exact.endpoint_policy_digest.clone(),
    };
    NativeModelProxyBasisV2::for_request(&request, &contrast, now, now + 1000)
        .expect("generation1 contract control");
    let before = manager.snapshot(&principal, &created.id).unwrap();
    assert_eq!(before.budget.calls, 0);
    assert_eq!(before.budget.tokens, 0);
    assert_eq!(before.budget.in_flight, 0);
    assert_eq!(ordinary_bindings.load(Ordering::SeqCst), 0);
    // The target is solely the real field0 rejection, not unknown price/graph.
    // for_request is a pure Core constructor; no ModelTransport is invoked.
    let basis = NativeModelProxyBasisV2::for_request(&request, &exact, now, now + 1000)
        .expect("actual unprobed Draft generation zero must remain representable");
    assert_eq!(basis.connection_generation.0, 0);
    let after = manager.snapshot(&principal, &created.id).unwrap();
    assert!(
        serde_json::to_value(before).unwrap() == serde_json::to_value(after).unwrap(),
        "connection snapshot changed"
    );
    assert_eq!(ordinary_bindings.load(Ordering::SeqCst), 0);
}
