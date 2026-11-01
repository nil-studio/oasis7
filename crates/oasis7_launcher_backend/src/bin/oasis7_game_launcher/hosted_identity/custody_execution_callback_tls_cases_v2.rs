//! Actual Native static dispatcher → TLS callback → Native current reentry.
//! Account receipt responder and signed World are component fixtures. No real
//! CF limiter/Owner/Account chain or 45-edge qualification is claimed here.
use super::*;
use oasis7_custody_core::native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2 as Edge;
use oasis7_custody_core::{execution_dispatch_gate_v2::*, native_execution_prepare_v2::*};
#[path = "custody_execution_callback_request_fixture_v2.rs"]
mod request_fixture;
#[path = "custody_execution_callback_tls_fixture_v2.rs"]
mod tls_fixture;

fn run(name: &str, scenario: &str) {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        let certificates = tls_fixture::Certificates::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let origin = format!("https://localhost:{port}");
        child_for(
            name,
            &origin,
            &[
                ("OASIS7_EXECUTION_CALLBACK_PORT", port.to_string()),
                (
                    "OASIS7_EXECUTION_CALLBACK_CERT_DIR",
                    certificates.path().to_str().unwrap().to_owned(),
                ),
            ],
        );
        return;
    }
    let (ready, world) =
        ready_registry_with_world_limit(if scenario == "second_denied" { 4 } else { 3 });
    let q = request_fixture::request(&ready);
    let raw = q.wire_bytes().unwrap();
    let previous = request_fixture::claim(&q, Edge::IdentityActorToNative, &raw, 107);
    let directory =
        std::path::PathBuf::from(std::env::var("OASIS7_EXECUTION_CALLBACK_CERT_DIR").unwrap());
    let port = std::env::var("OASIS7_EXECUTION_CALLBACK_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let server =
        tls_fixture::TlsServer::start(&directory, port, &ready, q.clone(), scenario.to_owned());
    // Old receipt-less Native10 never silently becomes the funded profile.
    assert!(
        exchange(
            &ready.service,
            NATIVE_EXECUTION_PREPARE_PATH_V2,
            &ready.headers,
            &q
        )
        .is_err()
    );
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&previous).unwrap());
    let headers = format!(
        "{}\r\nOasis7-Custody-Execution-Dispatch: {}\r\n\r\n",
        ready.headers.strip_suffix("\r\n\r\n").unwrap(),
        header
    );
    let root = if scenario == "untrusted" {
        None
    } else {
        Some(std::fs::read(directory.join("ca.pem")).unwrap())
    };
    let result = crate::static_http::cleanup_dispatch_with_root_for_test(
        ready.service.clone(),
        NATIVE_EXECUTION_PREPARE_PATH_V2,
        &headers,
        &raw,
        root,
    );
    assert_ne!(
        result.status, 200,
        "denied, unknown or changed Current must not return a prepared context"
    );
    // Replay uses the same real dispatcher and immutable signed originals.
    // The independent Native claim remains consumed after transport failure.
    assert!(
        exchange(
            &ready.service,
            NATIVE_EXECUTION_PREPARE_PATH_V2,
            &headers,
            &q
        )
        .is_err()
    );
    let expected_callbacks = if scenario == "second_denied" { 2 } else { 1 };
    verify_sql_callback_history(&ready, &q, expected_callbacks);
    let counts = server.finish();
    assert_eq!(
        counts.tls_connections,
        if scenario == "second_denied" { 2 } else { 1 },
        "all accepted TLS connections, including failed trust, no retry"
    );
    assert_eq!(
        counts.http_requests,
        match scenario {
            "second_denied" => 2,
            "untrusted" => 0,
            _ => 1,
        },
        "exact callback HTTP count"
    );
    assert_eq!(
        world.finish(),
        if scenario == "second_denied" { 4 } else { 3 },
        "no unclaimed World subtree; Ready prefix contains exactly three reads"
    );
}
#[test]
fn native45_callback_tls_second_denial_stops_before_prepare() {
    run(
        "native45_callback_tls_second_denial_stops_before_prepare",
        "second_denied",
    );
}
#[test]
fn native45_callback_tls_unknown_consumes_actual_native_once() {
    run(
        "native45_callback_tls_unknown_consumes_actual_native_once",
        "unknown",
    );
}
#[test]
fn native45_callback_tls_logout_during_reply_blocks_world() {
    run(
        "native45_callback_tls_logout_during_reply_blocks_world",
        "logout",
    );
}
#[test]
fn native45_callback_tls_wrong_signature_blocks_world() {
    run(
        "native45_callback_tls_wrong_signature_blocks_world",
        "wrong_signature",
    );
}
#[test]
fn native45_callback_tls_untrusted_ca_fails_before_http() {
    run(
        "native45_callback_tls_untrusted_ca_fails_before_http",
        "untrusted",
    );
}

fn verify_sql_callback_history(
    ready: &ReadyNativeRegistryV2,
    q: &NativeExecutionPrepareRequestV2,
    count: usize,
) {
    let (path, namespace, rp) = {
        let lock = ready.service.lock().unwrap();
        let config = &lock.as_ref().unwrap().config;
        (
            config.sqlite_path.clone(),
            config.namespace.clone(),
            config.rp.clone(),
        )
    };
    let mut cold = identity::store::IdentityStore::open(&path, namespace, rp).unwrap();
    cold.attach_machine_restore_guard_from_env(&path).unwrap();
    for edge in [Edge::NativeToWorldCurrent, Edge::NativeToWorldPrepare]
        .into_iter()
        .take(count)
    {
        let attempt = execution_reentry_inventory(q)
            .unwrap()
            .into_iter()
            .find(|v| v.edge == edge)
            .unwrap()
            .port
            .attempt_id;
        let callback = cold
            .transaction(|tx| {
                Ok(tx
                    .get::<NativeExecutionDispatchCallbackV2>(&format!(
                        "custody_execution_world_reentry_v2:{attempt}"
                    ))?
                    .unwrap()
                    .value)
            })
            .unwrap();
        assert!(
            callback.request.as_ref() == q,
            "cold SQLite keeps exact immutable original"
        );
        let query = &callback.query.query;
        callback
            .query
            .verify_native(
                q,
                "approval-test-1",
                &hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
                &query.identity_origin,
                query.issued_at.0,
            )
            .unwrap();
        assert_eq!(query.leaf.edge, edge);
    }
}
#[test]
fn native45_callback_tls_executor_disabled_during_reply_blocks_world() {
    run(
        "native45_callback_tls_executor_disabled_during_reply_blocks_world",
        "executor_disabled",
    );
}
#[test]
fn native45_callback_tls_wrong_body_strong_receipt_blocks_world() {
    run(
        "native45_callback_tls_wrong_body_strong_receipt_blocks_world",
        "wrong_body",
    );
}
