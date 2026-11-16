use super::{
    config::Config,
    state::{Request, State},
};
use serde_json::{Value, json};
fn policy() -> Value {
    json!({"version":1,"generation":1,"service_id":"standalone","origin":"https://worker.example.test","expires_at_unix":1000,"max_calls":20,"credentials":[{"caller_id":"caller","token_secret":"ACCOUNT_SERVICE_TOKEN","expires_at_unix":900,"revoked":false},{"caller_id":"other","token_secret":"ACCOUNT_SERVICE_OTHER","expires_at_unix":900,"revoked":false}],"accounts":{"A":{"alias":"Alice","account_id":"id-A","provider":"generic","environment":"test","credential_secret":"ACCOUNT_SERVICE_A","credential_version":1,"disabled":false},"B":{"alias":"Bob","account_id":"id-B","provider":"generic","environment":"test","credential_secret":"ACCOUNT_SERVICE_B","credential_version":1,"disabled":false}},"grants":[{"grant_ref":"grant-A","caller_id":"caller","account_ref":"A","authorization_version":1,"expires_at_unix":800,"enabled":true},{"grant_ref":"grant-B","caller_id":"caller","account_ref":"B","authorization_version":1,"expires_at_unix":800,"enabled":true}]})
}
fn configured(v: &Value, s: &mut State) -> Config {
    let raw = v.to_string();
    let c = Config::parse(&raw).unwrap();
    s.reconcile(&c, blake3::hash(raw.as_bytes()).to_hex().as_ref())
        .unwrap();
    c
}
fn call(
    s: &mut State,
    c: &Config,
    caller: &str,
    args: Value,
    n: u8,
) -> Result<Value, &'static str> {
    s.dispatch(
        c,
        caller,
        &serde_json::from_value::<Request>(args).unwrap(),
        20,
        &format!("{n:064x}"),
        |_| true,
    )
}
#[test]
fn independent_accounts_sessions_and_metadata_survive_restart_and_updates() {
    let mut s = State::default();
    let mut p = policy();
    let c = configured(&p, &mut s);
    let a = call(
        &mut s,
        &c,
        "caller",
        json!({"method":"open","grant_ref":"grant-A","expires_at_unix":100}),
        1,
    )
    .unwrap();
    let b = call(
        &mut s,
        &c,
        "caller",
        json!({"method":"open","grant_ref":"grant-B","expires_at_unix":100}),
        2,
    )
    .unwrap();
    let ar = a["result"]["session_ref"].as_str().unwrap();
    let br = b["result"]["session_ref"].as_str().unwrap();
    s = State::load(&s.encode().unwrap()).unwrap();
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"describe","session_ref":br}),
            3
        )
        .unwrap()["result"]["account_ref"],
        "B"
    );
    assert_eq!(
        call(
            &mut s,
            &c,
            "other",
            json!({"method":"describe","session_ref":ar}),
            4
        )
        .unwrap_err(),
        "session_unavailable"
    );
    p["generation"] = json!(2);
    p["accounts"]["A"]["alias"] = json!("Renamed");
    let c = configured(&p, &mut s);
    let result = call(
        &mut s,
        &c,
        "caller",
        json!({"method":"describe","session_ref":ar}),
        5,
    )
    .unwrap();
    assert_eq!(result["result"]["alias"], "Renamed");
    assert_eq!(result["policy_generation"], 2);
    assert_eq!(result["usage"]["used"], 4);
    assert!(!result.to_string().contains("ACCOUNT_SERVICE_A"));
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"close","session_ref":ar}),
            6
        )
        .unwrap()["result"]["closed"],
        true
    );
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"describe","session_ref":ar}),
            7
        )
        .unwrap_err(),
        "session_unavailable"
    );
}
#[test]
fn rotation_revocation_policy_rollback_and_budget_never_reopen_old_sessions() {
    let mut s = State::default();
    let mut p = policy();
    p["max_calls"] = json!(2);
    let c = configured(&p, &mut s);
    let a = call(
        &mut s,
        &c,
        "caller",
        json!({"method":"open","grant_ref":"grant-A","expires_at_unix":100}),
        1,
    )
    .unwrap();
    let ar = a["result"]["session_ref"].as_str().unwrap();
    p["generation"] = json!(2);
    p["accounts"]["A"]["credential_secret"] = json!("ACCOUNT_SERVICE_A_V2");
    assert!(
        s.clone()
            .reconcile(&Config::parse(&p.to_string()).unwrap(), "new")
            .is_err()
    );
    p["grants"][0]["authorization_version"] = json!(2);
    p["accounts"]["A"]["credential_version"] = json!(2);
    let c = configured(&p, &mut s);
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"describe","session_ref":ar}),
            2
        )
        .unwrap_err(),
        "session_scope_denied"
    );
    call(&mut s, &c, "caller", json!({"method":"accounts"}), 3).unwrap();
    assert_eq!(
        call(&mut s, &c, "caller", json!({"method":"accounts"}), 4).unwrap_err(),
        "call_limit_exhausted"
    );
    assert_eq!(
        s.clone()
            .reconcile(&Config::parse(&policy().to_string()).unwrap(), "old")
            .unwrap_err(),
        "account_policy_retired"
    );
    p["generation"] = json!(3);
    p["max_calls"] = json!(3);
    assert_eq!(
        s.clone()
            .reconcile(&Config::parse(&p.to_string()).unwrap(), "expand")
            .unwrap_err(),
        "account_budget_expansion_denied"
    );
    p["max_calls"] = json!(2);
    p["grants"][0]["enabled"] = json!(false);
    p["grants"][0]["authorization_version"] = json!(3);
    let c = configured(&p, &mut s);
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"open","grant_ref":"grant-A","expires_at_unix":100}),
            5
        )
        .unwrap_err(),
        "grant_unavailable"
    );
}
#[test]
fn malformed_missing_material_expiry_and_exhausted_close_are_safe() {
    let mut p = policy();
    p["max_calls"] = json!(1);
    let mut s = State::default();
    let c = configured(&p, &mut s);
    let req: Request = serde_json::from_value(
        json!({"method":"open","grant_ref":"grant-A","expires_at_unix":100}),
    )
    .unwrap();
    assert_eq!(
        s.dispatch(&c, "caller", &req, 20, &"01".repeat(32), |_| false)
            .unwrap_err(),
        "credential_unavailable"
    );
    let a = s
        .dispatch(&c, "caller", &req, 20, &"01".repeat(32), |_| true)
        .unwrap();
    let ar = a["result"]["session_ref"].as_str().unwrap();
    let describe = serde_json::from_value(json!({"method":"describe","session_ref":ar})).unwrap();
    assert_eq!(
        s.dispatch(&c, "caller", &describe, 100, &"02".repeat(32), |_| true)
            .unwrap_err(),
        "session_unavailable"
    );
    assert_eq!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"describe","session_ref":ar}),
            2
        )
        .unwrap_err(),
        "call_limit_exhausted"
    );
    assert!(
        call(
            &mut s,
            &c,
            "caller",
            json!({"method":"close","session_ref":ar}),
            3
        )
        .is_ok()
    );
    let mut broken: Value = serde_json::from_str(&s.encode().unwrap()).unwrap();
    broken["usage"] = json!({});
    assert!(State::load(&broken.to_string()).is_err());
    assert!(State::load("{}").is_err());
    assert!(State::load("null").is_err());
    let token = "12".repeat(32);
    assert_eq!(
        c.authenticate(&token, 20, |_| Some(token.clone())).unwrap(),
        "caller"
    );
    assert!(
        c.authenticate(&token, 1000, |_| Some(token.clone()))
            .is_err()
    );
    p["world"] = json!("forbidden");
    assert!(Config::parse(&p.to_string()).is_err());
}
