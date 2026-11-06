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
        let allowed = params.len() == 2 && params.get("name") == Some(&json!(TOOL));
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
        response["result"] = json!({"tools":[{"name":TOOL,"description":"Fixed authorized account/session tools. Preserve original IDs and inspect canonical business outcomes.","inputSchema":{"type":"object","additionalProperties":false,"required":["method"],"properties":{"method":{"enum":METHODS},"grant_ref":{"type":"string"},"session_ref":{"type":"string"},"expires_at_unix":{"type":"integer","minimum":0},"operation":{"type":"object","required":["request_id","arguments"],"additionalProperties":false,"properties":{"request_id":{"type":"string"},"arguments":{"type":"object"}}}}}}]});
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
        let business = backend(&body).unwrap_or_else(|_| json!({"status":"unknown","original_request_id":args["operation"]["request_id"],"session_ref":args["session_ref"],"next":"retain_original_request_id_and_body; lookup_original_under_current_authorization; do_not_create_replacement_operation"}));
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
        let bytes=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":"rpc","method":"tools/call","params":{"name":TOOL,"arguments":arguments}})).unwrap();
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
