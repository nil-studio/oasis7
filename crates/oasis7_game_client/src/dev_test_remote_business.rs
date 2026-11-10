//! Closed remote business projection. Authentication belongs to the trusted connector;
//! account/session/operation admission belongs exclusively to the common service.
use crate::dev_test_accounts_remote::{self as protocol, HttpRequest};
use serde_json::{Value, json};
pub const TOOL: &str = "dev_test_accounts_call";
const METHODS: [&str; 9] = [
    "accounts",
    "open",
    "close",
    "describe",
    "task",
    "capabilities",
    "context",
    "move",
    "lookup",
];

fn envelope(arguments: &Value, token: &str) -> Result<Vec<u8>, ()> {
    let mut fields = arguments.as_object().cloned().ok_or(())?;
    if fields.keys().any(|k| {
        ![
            "method",
            "grant_ref",
            "session_ref",
            "expires_at_unix",
            "operation",
        ]
        .contains(&k.as_str())
    }) || !fields
        .get("method")
        .and_then(Value::as_str)
        .is_some_and(|m| METHODS.contains(&m))
    {
        return Err(());
    }
    fields.insert("credential".into(), json!(token));
    let value = Value::Object(fields);
    let _: crate::dev_test_sessions::ToolRequest =
        serde_json::from_value(value.clone()).map_err(|_| ())?;
    serde_json::to_vec(&value).map_err(|_| ())
}
fn error(id: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"Unsupported tool or arguments"}})
}

pub fn handle_http(
    config: &crate::dev_test_service::Config,
    expected_origin: &str,
    request: HttpRequest<'_>,
    backend: impl FnOnce(&[u8]) -> Result<Value, ()>,
) -> (u16, Option<Value>) {
    if request.body.len() > config.max_request_bytes {
        return (413, None);
    }
    // Reuse the fixed HTTP boundary and full JSON-RPC validation without admitting
    // a discovery call to the business service. No account data exists here.
    let Ok(mut value) = serde_json::from_slice::<Value>(request.body) else {
        return (400, None);
    };
    let original = value.clone();
    let is_call = value.get("method") == Some(&json!("tools/call"));
    let mut prepared = None;
    if is_call && let Some(params) = value.get_mut("params").and_then(Value::as_object_mut) {
        let allowed = protocol::valid_tool_params(&Value::Object(params.clone()), TOOL);
        if allowed && let Some(arguments) = params.get("arguments") {
            // The token is added only after the HTTP authentication boundary.
            prepared = envelope(arguments, "validation-placeholder-only").ok();
        }
        if prepared.is_some() {
            params.insert("name".into(), json!(protocol::CAPABILITIES_TOOL));
            params.insert("arguments".into(), json!({}));
        }
    }
    let bytes = serde_json::to_vec(&value).expect("JSON value");
    let check = protocol::handle_http(
        config,
        expected_origin,
        HttpRequest {
            body: &bytes,
            ..request
        },
        |_| {
            Ok(crate::dev_test_service::Response {
                profile: crate::dev_test_service::PROFILE.into(),
                status: "ok".into(),
                caller_id: None,
                methods: vec![],
                accounts_available: false,
                writes_available: false,
            })
        },
    );
    if check.0 != 200 || check.1.as_ref().is_some_and(|v| v.get("error").is_some()) {
        return check;
    }
    let mut response = check.1.expect("request response");
    if original.get("method") == Some(&json!("tools/list")) {
        response["result"] = json!({"tools":[{"name":TOOL,"description":"Fixed authorized account/session tools. Preserve original IDs and inspect canonical business outcomes.","inputSchema":{"type":"object","additionalProperties":false,"required":["method"],"properties":{"method":{"enum":METHODS},"grant_ref":{"type":"string"},"session_ref":{"type":"string"},"expires_at_unix":{"type":"integer","minimum":0},"operation":{"type":"object","required":["request_id","arguments"],"additionalProperties":false,"properties":{"request_id":{"type":"string"},"arguments":{"oneOf":[{"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"context"}}},{"type":"object","additionalProperties":false,"required":["kind","context_request_id","to"],"properties":{"kind":{"const":"move"},"context_request_id":{"type":"string"},"to":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"integer"}}}},{"type":"object","additionalProperties":false,"required":["kind","original_request_id"],"properties":{"kind":{"const":"lookup"},"original_request_id":{"type":"string"}}}]}}}}}}]});
    } else if is_call {
        let args = &original["params"]["arguments"];
        if prepared.is_none() {
            return (
                200,
                Some(error(original.get("id").cloned().unwrap_or(Value::Null))),
            );
        }
        let token = request
            .authorization
            .and_then(|h| h.strip_prefix("Bearer "))
            .expect("authenticated boundary");
        let body = envelope(args, token).expect("validated envelope");
        let typed: crate::dev_test_sessions::ToolRequest =
            serde_json::from_slice(&body).expect("validated envelope");
        let business = backend(&body).unwrap_or_else(|_| json!({
            "status":"unknown",
            "request_id":typed.operation.as_ref().map(|op| op.request_id.as_str()),
            "original_request_id":typed.operation.as_ref().map(|op| op.recovery_request_id()),
            "session_ref":typed.session_ref,
            "next":"retain_original_request_id_and_body; lookup_original_under_current_authorization; do_not_create_replacement_operation"
        }));
        response["result"] = json!({"content":[{"type":"text","text":business.to_string()}],"structuredContent":business,"isError":business.get("status") != Some(&json!("ok"))});
    }
    (200, Some(response))
}
/// Trusted daemon bridge; callers never choose the backend path.
pub fn call_backend(
    socket: &std::path::Path,
    body: &[u8],
    deadline: std::time::Instant,
) -> Result<Value, String> {
    crate::dev_test_service::call_serialized_until(socket, body, deadline)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> crate::dev_test_service::Config {
        serde_json::from_value(json!({"profile":"dev-test-accounts-v1","socket":"/unused","usage_directory":"/unused","grant_id":"test","max_request_bytes":65536,"timeout_ms":1000,"max_calls":20,"credentials":[{"caller_id":"fixture","token":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","expires_at_unix":u64::MAX,"revoked":false}]})).unwrap()
    }
    fn call(arguments: Value, backend: impl FnOnce(&[u8]) -> Result<Value, ()>) -> Value {
        call_params(json!({"name":TOOL,"arguments":arguments}), backend)
    }
    fn call_params(params: Value, backend: impl FnOnce(&[u8]) -> Result<Value, ()>) -> Value {
        let bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":"rpc","method":"tools/call","params":params}),
        )
        .unwrap();
        handle_http(
            &config(),
            "https://fixture.example",
            HttpRequest {
                method: "POST",
                host: "fixture.example",
                origin: Some("https://fixture.example"),
                authorization: Some("Bearer xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
                protocol_version: Some(protocol::PROTOCOL_VERSION),
                body: &bytes,
            },
            backend,
        )
        .1
        .unwrap()
    }

    #[test]
    fn business_metadata_is_validated_without_becoming_an_argument() {
        for meta in [
            json!({}),
            json!({"progressToken":42}),
            json!({"progressToken":"rpc", "trace":{"id":1}}),
        ] {
            let result = call_params(
                json!({"name":TOOL,"arguments":{"method":"accounts"},"_meta":meta}),
                |body| {
                    assert_eq!(
                        serde_json::from_slice::<Value>(body).unwrap(),
                        json!({"method":"accounts","credential":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"})
                    );
                    Err(())
                },
            );
            assert_eq!(result["result"]["structuredContent"]["status"], "unknown");
            assert_eq!(
                result["result"]["structuredContent"]["original_request_id"],
                Value::Null
            );
            assert_eq!(
                result["result"]["structuredContent"]["request_id"],
                Value::Null
            );
        }
        for params in [
            json!({"name":TOOL,"arguments":{"method":"accounts"},"_meta":null}),
            json!({"name":TOOL,"arguments":{"method":"accounts"},"_meta":{"progressToken":false}}),
            json!({"name":TOOL,"arguments":{"method":"accounts"},"unexpected":{}}),
            json!({"name":TOOL,"arguments":{"method":"accounts","credential":"bad"},"_meta":{}}),
        ] {
            assert!(
                call_params(params, |_| panic!("invalid envelope reached backend"))
                    .get("error")
                    .is_some()
            );
        }
    }
    #[test]
    fn lost_lookup_response_keeps_the_target_operation() {
        let args = json!({"method":"lookup","session_ref":"A","operation":{
            "request_id":"lookup-A","arguments":{"kind":"lookup","original_request_id":"move-A"}
        }});
        let result = call(args, |_| Err(()));
        assert_eq!(
            result["result"]["structuredContent"]["original_request_id"],
            "move-A"
        );
        assert_eq!(
            result["result"]["structuredContent"]["request_id"],
            "lookup-A"
        );
        assert_eq!(result["result"]["structuredContent"]["status"], "unknown");
    }
    #[test]
    fn closed_envelope_and_original_unknown() {
        for field in ["credential", "url", "path", "caller_id"] {
            let mut args = json!({"method":"accounts"});
            args[field] = json!("forbidden");
            assert!(
                call(args, |_| panic!("must not forward"))
                    .get("error")
                    .is_some()
            );
        }
        let args = json!({"method":"move","session_ref":"A","operation":{"request_id":"move-A","arguments":{"kind":"move","context_request_id":"context-A","to":[1,0,0]}}});
        let expected = args.clone();
        let result = call(args, |body| {
            let mut value: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                value.as_object_mut().unwrap().remove("credential").unwrap(),
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
            );
            assert_eq!(value, expected);
            Err(())
        });
        assert_eq!(result["result"]["structuredContent"]["status"], "unknown");
        assert_eq!(
            result["result"]["structuredContent"]["original_request_id"],
            "move-A"
        );
    }
}
