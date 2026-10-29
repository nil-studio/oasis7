//! Offline synthetic fixtures only. No online Current or production clock grant.
use super::*;
use serde::Serialize;

fn replay_case<T: Serialize>(
    name: &str,
    f: &SessionFixture,
    current: &TestSessionLiveCurrent<'_, '_>,
    component_now: UnixMillis,
    response: &T,
) -> serde_json::Value {
    assert_eq!(
        current.private_sid_generation,
        f.state.test_sessions()[ID].session.private_sid_generation
    );
    let config = current.provision;
    let native = current.native_scope;
    serde_json::json!({
        "name": name,
        "session_id": ID,
        "component_now": component_now,
        "account_state": f.state,
        "approval_context": config.approval,
        "policy": {
            "routes": config.policy.routes,
            "managers": config.policy.managers,
            "version": config.policy.version,
            "world_identity": config.world_identity,
            "native_identity_namespace": config.native_identity_namespace,
        },
        "native_scope": {
            "http_origin": native.http_origin,
            "native_root_key_id": native.native_root_key_id,
            "native_root_public_key": native.native_root_public_key,
            "owner_binding_pin": native.owner_binding_pin,
        },
        "source": {
            "operation_id": current.source_operation_id,
            "source_ticket": current.source_ticket,
            "observed_at": current.observed_at,
            "private_sid_generation": current.private_sid_generation,
        },
        "lifetime": current.lifetime,
        "response": response,
    })
}

#[test]
fn context_body_export_native_webcrypto_replay_fixture() {
    let (mut f, _, _) = claimed();
    let (original, response) = prepared_artifact(&f);
    let life = exec_source(&mut f, "context-export-prepared", "da", 12_000);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let pin = f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &life, "context-export-prepared", 12_002);
    let original_checks = f
        .state
        .test_session_context_original_checks(ID, &original, &current, UnixMillis::new(12_002))
        .unwrap();
    f.state
        .register_test_session_context(
            ID,
            original.clone(),
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(original_checks),
        )
        .unwrap();
    restore(&f.state);
    let prepared_case = replay_case("prepared", &f, &current, UnixMillis::new(12_002), &response);
    let checks = f
        .state
        .test_session_prepared_context_checks(ID, &response, &current, UnixMillis::new(12_002))
        .unwrap();
    let body = f
        .state
        .validate_test_session_prepared_body(
            ID,
            &response,
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(checks.clone()),
        )
        .unwrap();
    let candidate = envelope(body.scope(), body.canonical_bytes().len());
    // The captured case needs the actual Core immutable Prepared reference.
    // This structural envelope is not an AEAD/actor-read success claim.
    f.state
        .claim_test_session_context_write(
            ID,
            ContextBodyWrite {
                attempt: "export-prepared-write",
                body: &body,
                candidate: &candidate,
            },
            &current,
            UnixMillis::new(12_002),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    f.state
        .record_test_session_context_storage_fact(
            ID,
            "export-prepared-write",
            candidate.storage_fact().unwrap(),
        )
        .unwrap();
    let (get, prefix, captured) = captured_artifact(&f, &response.payload, &original);
    let life2 = exec_source(&mut f, "context-export-captured", "db", 12_010);
    let policy = Policy::new(f.state.draft());
    let provision = policy.current(&f.context);
    let current = live(&provision, &ns, &life2, "context-export-captured", 12_012);
    let checks = f
        .state
        .test_session_context_get_checks(
            ID,
            &body,
            &get,
            &prefix,
            &current,
            UnixMillis::new(12_012),
        )
        .unwrap();
    f.state
        .register_test_session_context_get(
            ID,
            ContextGetRegistration {
                prepared: &body,
                original: &get,
                prefix: &prefix,
            },
            &current,
            UnixMillis::new(12_012),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    restore(&f.state);
    let captured_case = replay_case("captured", &f, &current, UnixMillis::new(12_012), &captured);
    let checks = f
        .state
        .test_session_captured_context_checks(ID, &captured, &current, UnixMillis::new(12_012))
        .unwrap();
    f.state
        .validate_test_session_captured_body(
            ID,
            &captured,
            &current,
            UnixMillis::new(12_012),
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    let fixture = serde_json::json!({
        "label": "synthetic-native-core-webcrypto-replay-not-live-authority",
        "clock_mode": "fixed-component-replay-only",
        "scope_write_deadlines_are_historical": true,
        "captured_prepared_envelope_is_structural_not_aead_proof": true,
        "cases": [prepared_case, captured_case],
    });
    if let Some(path) = std::env::var_os("OASIS7_CONTEXT_BODY_NATIVE_FIXTURE_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        assert!(path.is_absolute() && path.starts_with("/tmp"));
        std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    }
}
