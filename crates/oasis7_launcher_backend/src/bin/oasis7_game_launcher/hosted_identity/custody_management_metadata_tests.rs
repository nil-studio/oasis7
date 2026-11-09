#[test]
fn custody_management_capability_is_explicit_and_never_changes_session_authentication() {
    let (mut f, _) = super::tests::fixture();
    let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::default();
    let path = format!("{}/capabilities", crate::hosted_identity::service::PREFIX);
    let headers = format!(
        "GET / HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\n\r\n",
        f.token
    );
    f.service.custody_management_enabled = false;
    let disabled = f.service.handle(
        "GET",
        &path,
        &headers,
        serde_json::json!({}),
        "fixture",
        &mut issuer,
    );
    assert_eq!(disabled.status, 200);
    assert_eq!(disabled.body["custody_management"]["available"], false);
    assert_eq!(
        disabled.body["custody_management"]["audience"],
        serde_json::Value::Null
    );
    f.service.custody_management_enabled = true;
    let enabled = f.service.handle(
        "GET",
        &path,
        &headers,
        serde_json::json!({}),
        "fixture",
        &mut issuer,
    );
    assert_eq!(enabled.body["custody_management"]["available"], true);
    assert_eq!(
        enabled.body["custody_management"]["audience"],
        "account-authority:test"
    );
    assert_eq!(
        enabled.body["custody_management"]["parents_query_path"],
        oasis7_custody_core::management_metadata::PARENTS_QUERY_PATH
    );
    let session = f.service.handle(
        "GET",
        &format!("{}/session", crate::hosted_identity::service::PREFIX),
        &headers,
        serde_json::json!({}),
        "fixture",
        &mut issuer,
    );
    assert_eq!(session.status, 200);
    assert_eq!(session.body["authentication"]["method"], "webauthn");
    f.service.custody = None;
    let missing = f.service.handle(
        "GET",
        &path,
        &headers,
        serde_json::json!({}),
        "fixture",
        &mut issuer,
    );
    assert_eq!(missing.body["custody_management"]["available"], false);
}
