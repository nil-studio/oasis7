use crate::config::{Config, Material, Resource};
use serde_json::{Value, json};
fn fixture() -> Value {
    let fence = json!({"world_id":"world","genesis_digest":"genesis","branch_id":"branch","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"ab".repeat(32),"task_id":"task","task_revision":"2","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"ef".repeat(32),"delegation_generation":"1","executor_epoch":"1"}});
    let resource = json!({"account_ref":"account","endpoint":"https://native.example.test","service_public_key":"12".repeat(32),"material_secret":"DEV_TEST_MATERIAL","fence":fence});
    let typed: Resource = serde_json::from_value(resource.clone()).unwrap();
    let digest = blake3::hash(&serde_json::to_vec(&typed).unwrap())
        .to_hex()
        .to_string();
    json!({"generation":1,"service_grant_id":"service","origin":"https://worker.example.test","expires_at_unix":1000,"max_calls":10,"timeout_ms":1000,"max_request_bytes":4096,
      "credentials":[{"caller_id":"caller","token_secret":"DEV_TEST_TOKEN","expires_at_unix":900,"revoked":false}],
      "accounts":{"account":{"alias":"A","account_id":"identity","environment":"[\"test\",\"private\"]","identity_public_key":"34".repeat(32),"disabled":false}},
      "resources":{"resource":resource},"grants":[{"grant_ref":"grant","grant_id":"service","caller_id":"caller","binding":{"account_ref":"account","environment":"[\"test\",\"private\"]","world":"world","genesis_digest":"genesis","agent_id":"agent","resource":"resource","executor_config_digest":digest,"authorization_version":1},"expires_at_unix":900,"enabled":true,"methods":["lookup"],"move_targets":[]}]})
}
fn accepted(value: &Value) -> Config {
    Config::parse(&value.to_string()).unwrap()
}
fn rejected(value: &Value) {
    assert!(Config::parse(&value.to_string()).is_err());
}
#[test]
fn valid_config_and_authentication_require_current_trusted_secret() {
    let c = accepted(&fixture());
    let token = "56".repeat(32);
    assert_eq!(
        c.authenticate(&token, 899, |name| {
            assert_eq!(name, "DEV_TEST_TOKEN");
            Some(token.clone())
        }),
        Ok("caller")
    );
    assert!(
        c.authenticate(&token, 900, |_| Some(token.clone()))
            .is_err()
    );
    assert!(
        c.authenticate(&token, 1000, |_| Some(token.clone()))
            .is_err()
    );
    assert!(c.authenticate(&token, 1, |_| None).is_err());
    assert!(
        c.authenticate(&token, 1, |_| Some("78".repeat(32)))
            .is_err()
    );
    for invalid in ["short".to_string(), "AB".repeat(32), "56".repeat(33)] {
        assert!(
            c.authenticate(&invalid, 1, |_| Some(token.clone()))
                .is_err()
        );
    }
    let mut disabled = fixture();
    disabled["credentials"][0]["revoked"] = json!(true);
    assert!(
        accepted(&disabled)
            .authenticate(&token, 1, |_| Some(token.clone()))
            .is_err()
    );
}
#[test]
fn secret_references_and_no_host_paths_are_enforced() {
    for path in [
        "/Users/host/auth.json",
        "file:///tmp/auth",
        "DEV_TEST_bad",
        "CREDENTIAL",
        "",
    ] {
        let mut c = fixture();
        c["credentials"][0]["token_secret"] = json!(path);
        rejected(&c);
        let mut c = fixture();
        c["resources"]["resource"]["material_secret"] = json!(path);
        rejected(&c);
    }
    for endpoint in [
        "http://native.example.test",
        "https://user@host",
        "https://host/path",
        "https://host?query",
        "file:///tmp/world",
        "https://host:0",
    ] {
        let mut c = fixture();
        c["resources"]["resource"]["endpoint"] = json!(endpoint);
        rejected(&c);
    }
    for field in ["host_path", "extraContext", "credential"] {
        let mut c = fixture();
        c[field] = json!("injected");
        rejected(&c);
    }
}
#[test]
fn policy_binding_expiry_and_budget_bounds_fail_closed() {
    let original = fixture();
    accepted(&original);
    for (field, value) in [
        ("generation", 0),
        ("expires_at_unix", 0),
        ("max_calls", 10001),
        ("timeout_ms", 30001),
        ("max_request_bytes", 65537),
    ] {
        let mut c = original.clone();
        c[field] = json!(value);
        rejected(&c);
    }
    for (field, value) in [
        ("authorization_version", json!(0)),
        ("world", json!("other")),
        ("agent_id", json!("other")),
        ("executor_config_digest", json!("untrusted")),
    ] {
        let mut c = original.clone();
        c["grants"][0]["binding"][field] = value;
        rejected(&c);
    }
    let mut c = original.clone();
    c["grants"][0]["expires_at_unix"] = json!(1001);
    rejected(&c);
    let mut c = original.clone();
    c["credentials"][0]["expires_at_unix"] = json!(1001);
    rejected(&c);
    let mut c = original.clone();
    c["credentials"]
        .as_array_mut()
        .unwrap()
        .push(original["credentials"][0].clone());
    rejected(&c);
    let mut c = original;
    c["grants"][0]["methods"] = json!(["shell"]);
    rejected(&c);
}
#[test]
fn private_material_is_closed_and_cookie_cannot_inject_headers() {
    let base =
        json!({"executor_seed":"17".repeat(32),"identity_cookie":"__Host-oasis7_sid=private"});
    assert!(Material::parse(&base.to_string()).is_ok());
    for cookie in [
        "other=value",
        "__Host-oasis7_sid=x; owner=y",
        "__Host-oasis7_sid=x\r\nHeader: x",
    ] {
        let mut m = base.clone();
        m["identity_cookie"] = json!(cookie);
        assert!(Material::parse(&m.to_string()).is_err());
    }
    let mut m = base.clone();
    m["host_path"] = json!("/tmp/auth");
    assert!(Material::parse(&m.to_string()).is_err());
    let mut m = base;
    m["executor_seed"] = json!("invalid");
    assert!(Material::parse(&m.to_string()).is_err());
}

#[test]
fn duplicate_policy_and_private_material_keys_fail_closed() {
    let mut raw = fixture().to_string();
    raw.insert_str(1, r#""generation":2,"#);
    assert!(Config::parse(&raw).is_err());
    let raw = format!(
        r#"{{"executor_seed":"{}","executor_seed":"{}","identity_cookie":"__Host-oasis7_sid=x"}}"#,
        "17".repeat(32),
        "18".repeat(32)
    );
    assert!(Material::parse(&raw).is_err());
}

/// Explicit opt-in export for local Wasm routing/auth/budget tests only.
/// The synthetic native endpoint is not a World acceptance fixture.
#[test]
fn export_local_entry_fixture_when_requested() {
    let mut policy = fixture();
    let executor = oasis7_client_api::world_service::sign_read_request(
        "fixture-public-key",
        (),
        &"17".repeat(32),
    )
    .unwrap()
    .subject_public_key;
    policy["resources"]["resource"]["fence"]["executor_binding"]["executor_public_key"] =
        json!(executor);
    policy["origin"] = json!("https://worker.example.test");
    policy["expires_at_unix"] = json!(4_102_444_800u64);
    policy["credentials"][0]["expires_at_unix"] = json!(4_102_444_800u64);
    policy["grants"][0]["expires_at_unix"] = json!(4_102_444_800u64);
    policy["max_calls"] = json!(1);
    policy["grants"][0]["methods"] = json!([
        "describe",
        "task",
        "capabilities",
        "context",
        "move",
        "lookup"
    ]);
    let resource: Resource =
        serde_json::from_value(policy["resources"]["resource"].clone()).unwrap();
    policy["grants"][0]["binding"]["executor_config_digest"] = json!(
        blake3::hash(&serde_json::to_vec(&resource).unwrap())
            .to_hex()
            .to_string()
    );
    let config = accepted(&policy);
    assert_ne!(config.accounts["account"].identity_public_key, executor);
    assert_eq!(
        config.resources["resource"]
            .fence
            .executor_binding
            .as_ref()
            .unwrap()
            .executor_public_key,
        executor
    );
    if let Some(path) = std::env::var_os("OASIS7_WORKER_TEST_FIXTURE_OUT") {
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    }
}
