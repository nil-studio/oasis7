//! Stateless MCP projection. The caller must authenticate and enforce transport limits
//! before dispatch; this module never accepts credentials or executes world actions.
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2025-11-25";
pub const CAPABILITIES_TOOL: &str = "dev_test_accounts_capabilities";

/// Projects an authenticated request onto the sole read-only capability provider.
/// `None` denotes an accepted notification (HTTP 202, empty body).
pub fn dispatch_authenticated(
    request: &Value,
    capabilities: impl FnOnce() -> Value,
) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let error = |code, message| {
        Some(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
    };
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || !request.is_object()
        || request
            .get("id")
            .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
    {
        return error(-32600, "Invalid request");
    }
    if request.as_object().is_some_and(|fields| {
        fields
            .keys()
            .any(|key| !["jsonrpc", "id", "method", "params"].contains(&key.as_str()))
    }) {
        return error(-32600, "Invalid request");
    }
    let method = request.get("method").and_then(Value::as_str);
    if request.get("id").is_none() {
        return if method == Some("notifications/initialized")
            && request.get("params").is_none_or(|p| *p == json!({}))
        {
            None
        } else {
            error(-32600, "Unsupported notification")
        };
    }
    let params = request.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        Some("initialize") => {
            if params.get("protocolVersion").and_then(Value::as_str) != Some(PROTOCOL_VERSION) {
                return error(-32602, "Unsupported protocol version");
            }
            if !params.get("capabilities").is_some_and(Value::is_object)
                || !params.get("clientInfo").is_some_and(|info| {
                    info.get("name").is_some_and(Value::is_string)
                        && info.get("version").is_some_and(Value::is_string)
                })
                || params.as_object().is_none_or(|fields| {
                    fields.keys().any(|key| {
                        !["protocolVersion", "capabilities", "clientInfo", "_meta"]
                            .contains(&key.as_str())
                    })
                })
            {
                return error(-32602, "Invalid initialization parameters");
            }
            json!({"protocolVersion":PROTOCOL_VERSION,"capabilities":{"tools":{}},
                "serverInfo":{"name":"oasis7-dev-test-accounts","version":"1"}})
        }
        Some("ping") if params == json!({}) => json!({}),
        Some("tools/list") if params == json!({}) => json!({"tools":[{
            "name":CAPABILITIES_TOOL,"description":"Read service capabilities; does not create a game context.",
            "inputSchema":{"type":"object","additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false}}]}),
        Some("tools/call") => {
            if params.get("name").and_then(Value::as_str) != Some(CAPABILITIES_TOOL)
                || params.get("arguments").is_some_and(|v| *v != json!({}))
                || params
                    .as_object()
                    .is_none_or(|m| m.keys().any(|k| k != "name" && k != "arguments"))
            {
                return error(-32602, "Unsupported tool or arguments");
            }
            let value = capabilities();
            json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
        }
        Some("tools/list" | "ping") => return error(-32602, "Invalid parameters"),
        _ => return error(-32601, "Method not found"),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_capabilities_can_reach_provider() {
        for params in [
            json!({"name":"shell","arguments":{}}),
            json!({"name":CAPABILITIES_TOOL,"arguments":{"url":"https://invalid"}}),
        ] {
            let response = dispatch_authenticated(
                &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":params}),
                || panic!("provider must not run"),
            )
            .unwrap();
            assert_eq!(response["error"]["code"], -32602);
        }
        let response=dispatch_authenticated(&json!({"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":CAPABILITIES_TOOL,"arguments":{}}}),||json!({"writeAvailable":false})).unwrap();
        assert_eq!(response["id"], "call");
        assert_eq!(
            response["result"]["structuredContent"]["writeAvailable"],
            false
        );
    }
    #[test]
    fn rejects_batches_and_unknown_versions() {
        for request in [
            json!([]),
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"unknown"}}),
        ] {
            assert!(
                dispatch_authenticated(&request, || panic!())
                    .unwrap()
                    .get("error")
                    .is_some()
            );
        }
        assert!(
            dispatch_authenticated(
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                || panic!()
            )
            .is_none()
        );
    }
}

/// HTTP projection behind a trusted TLS proxy. Proxy must overwrite Host and
/// forward Authorization unchanged; forwarding headers never establish trust.
pub struct HttpRequest<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub origin: Option<&'a str>,
    pub authorization: Option<&'a str>,
    pub protocol_version: Option<&'a str>,
    pub body: &'a [u8],
}
pub fn handle_http(
    config: &crate::dev_test_service::Config,
    expected_origin: &str,
    request: HttpRequest<'_>,
    call_service: impl FnOnce(
        crate::dev_test_service::Request,
    ) -> Result<crate::dev_test_service::Response, ()>,
) -> (u16, Option<Value>) {
    let reject = |status| (status, None);
    let Ok(origin_url) = reqwest::Url::parse(expected_origin) else {
        return reject(500);
    };
    if origin_url.scheme() != "https"
        || origin_url.host_str().is_none()
        || !origin_url.username().is_empty()
        || origin_url.password().is_some()
        || origin_url.query().is_some()
        || origin_url.fragment().is_some()
        || origin_url.path() != "/"
    {
        return reject(500);
    }
    let Some(host) = expected_origin.strip_prefix("https://") else {
        return reject(500);
    };
    if host.is_empty()
        || host.contains('/')
        || request.host != host
        || request
            .origin
            .is_some_and(|origin| origin != expected_origin)
    {
        return reject(403);
    }
    let Some(token) = request
        .authorization
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return reject(401);
    };
    if request.method != "POST" {
        return reject(405);
    };
    if request.body.len() > config.max_request_bytes {
        return reject(413);
    };
    let Ok(value) = serde_json::from_slice::<Value>(request.body) else {
        return reject(400);
    };
    if value.get("method").and_then(Value::as_str) != Some("initialize")
        && request.protocol_version != Some(PROTOCOL_VERSION)
    {
        return reject(400);
    };
    // Authentication precheck does not consume admission. The authoritative
    // service still reauthenticates every valid request before consuming quota.
    if crate::dev_test_service::authenticate_token(
        config,
        token,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(u64::MAX, |d| d.as_secs()),
    )
    .is_err()
    {
        return reject(401);
    }
    let preview = dispatch_authenticated(&value, || json!({}));
    if preview
        .as_ref()
        .is_some_and(|response| response.get("error").is_some())
    {
        return (if value.get("id").is_none() { 400 } else { 200 }, preview);
    }
    // Every accepted protocol request goes through the same authoritative
    // service admission as local callers, including discovery and notifications.
    let admitted = call_service(crate::dev_test_service::Request {
        profile: crate::dev_test_service::PROFILE.into(),
        audience: crate::dev_test_service::AUDIENCE.into(),
        credential: token.into(),
        method: "capabilities".into(),
    });
    let Ok(response) = admitted else {
        return reject(503);
    };
    if response.status != "ok" {
        return reject(if response.status == "unauthorized" {
            401
        } else {
            403
        });
    }
    let result = dispatch_authenticated(&value, || {
        serde_json::to_value(response).expect("serializable response")
    });
    (if result.is_some() { 200 } else { 202 }, result)
}

#[cfg(test)]
mod http_tests {
    use super::*;
    #[test]
    fn rejects_untrusted_origin_before_backend_and_never_leaks_token() {
        let config=serde_json::from_value(json!({"profile":"dev-test-accounts-v1","socket":"/unused","usage_directory":"/unused","grant_id":"test","max_request_bytes":1024,"timeout_ms":100,"max_calls":2,"credentials":[]})).unwrap();
        let result = handle_http(
            &config,
            "https://fixture.example",
            HttpRequest {
                method: "POST",
                host: "fixture.example",
                origin: Some("https://evil.example"),
                authorization: Some("Bearer sensitive-fixture"),
                protocol_version: Some(PROTOCOL_VERSION),
                body: b"{}",
            },
            |_| panic!("untrusted source reached service"),
        );
        assert_eq!(result, (403, None));
        let result = handle_http(
            &config,
            "https://user:secret@fixture.example",
            HttpRequest {
                method: "POST",
                host: "user:secret@fixture.example",
                origin: None,
                authorization: None,
                protocol_version: None,
                body: b"{}",
            },
            |_| panic!(),
        );
        assert_eq!(result, (500, None));
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn invalid_protocol_never_calls_backend() {
        let config=serde_json::from_value(json!({"profile":"dev-test-accounts-v1","socket":"/unused","usage_directory":"/unused","grant_id":"test","max_request_bytes":4096,"timeout_ms":100,"max_calls":2,"credentials":[{"caller_id":"fixture","token":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","expires_at_unix":u64::MAX,"revoked":false}]})).unwrap();
        for value in [
            json!({"jsonrpc":"2.0","id":1,"method":"shell"}),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":CAPABILITIES_TOOL,"arguments":{"url":"invalid"}}}),
            json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":{},"method":"ping"}),
            json!({"jsonrpc":"2.0","id":1.5,"method":"ping"}),
        ] {
            let body = serde_json::to_vec(&value).unwrap();
            let (status, response) = handle_http(
                &config,
                "https://fixture.example",
                HttpRequest {
                    method: "POST",
                    host: "fixture.example",
                    origin: None,
                    authorization: Some("Bearer xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
                    protocol_version: Some(PROTOCOL_VERSION),
                    body: &body,
                },
                |_| panic!("invalid protocol consumed admission"),
            );
            assert_eq!(status, 200);
            assert!(response.unwrap().get("error").is_some());
        }
    }
}

#[cfg(test)]
mod notification_tests {
    use super::*;
    #[test]
    fn rejected_notification_is_http_error_without_admission() {
        let config=serde_json::from_value(json!({"profile":"dev-test-accounts-v1","socket":"/unused","usage_directory":"/unused","grant_id":"test","max_request_bytes":4096,"timeout_ms":100,"max_calls":2,"credentials":[{"caller_id":"fixture","token":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","expires_at_unix":u64::MAX,"revoked":false}]})).unwrap();
        let body = br#"{"jsonrpc":"2.0","method":"notifications/unknown"}"#;
        let (status, response) = handle_http(
            &config,
            "https://fixture.example",
            HttpRequest {
                method: "POST",
                host: "fixture.example",
                origin: None,
                authorization: Some("Bearer xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
                protocol_version: Some(PROTOCOL_VERSION),
                body,
            },
            |_| panic!("notification consumed admission"),
        );
        assert_eq!(status, 400);
        assert!(response.unwrap().get("error").is_some());
    }
}
