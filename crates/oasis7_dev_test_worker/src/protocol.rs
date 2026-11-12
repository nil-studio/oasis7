//! Closed authenticated MCP envelope. Metadata never becomes business arguments.
use oasis7_custody_core::dev_test::{METHODS, ModelRequest};
use serde_json::{Value, json};
pub const VERSION: &str = "2025-11-25";
pub enum Request {
    Metadata(Option<Value>),
    Business { id: Value, body: ModelRequest },
}
fn error(id: Value, code: i64) -> Request {
    Request::Metadata(Some(
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":"tool_request_rejected"}}),
    ))
}
pub fn parse(raw: &str) -> Request {
    let Ok(v) = oasis7_agent_api::game::parse_bounded_json::<Value>(raw.as_bytes(), 65536) else {
        return error(Value::Null, -32700);
    };
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    if v.get("jsonrpc") != Some(&json!("2.0"))
        || !v.is_object()
        || v.as_object().is_some_and(|m| {
            m.keys()
                .any(|k| !["jsonrpc", "id", "method", "params"].contains(&k.as_str()))
        })
        || v.get("id")
            .is_some_and(|i| !i.is_string() && !i.is_i64() && !i.is_u64())
    {
        return error(id, -32600);
    }
    let method = v.get("method").and_then(Value::as_str);
    let params = v.get("params").cloned().unwrap_or(json!({}));
    if v.get("id").is_none() {
        return if method == Some("notifications/initialized") && params == json!({}) {
            Request::Metadata(None)
        } else {
            error(id, -32600)
        };
    }
    let result = match method {
        Some("initialize")
            if params.as_object().is_some_and(|p| {
                p.keys().all(|k| {
                    ["protocolVersion", "capabilities", "clientInfo", "_meta"].contains(&k.as_str())
                })
            }) && params.get("protocolVersion") == Some(&json!(VERSION))
                && params.get("capabilities").is_some_and(Value::is_object)
                && params.get("clientInfo").is_some_and(|i| {
                    i.get("name").is_some_and(Value::is_string)
                        && i.get("version").is_some_and(Value::is_string)
                }) =>
        {
            json!({"protocolVersion":VERSION,"capabilities":{"tools":{}},"serverInfo":{"name":"oasis7-dev-test-accounts","version":"1"}})
        }
        Some("ping") if params == json!({}) => json!({}),
        Some("tools/list") if params == json!({}) => {
            json!({"tools":[{"name":"dev_test_accounts_call","description":"Restricted account tools. Preserve original IDs; inspect canonical world outcomes.","inputSchema":{"type":"object","additionalProperties":false,"required":["method"],"properties":{"method":{"enum":METHODS},"grant_ref":{"type":"string"},"session_ref":{"type":"string"},"expires_at_unix":{"type":"integer","minimum":0},"operation":{"type":"object","additionalProperties":false,"required":["request_id","arguments"],"properties":{"request_id":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.-]+$"},"arguments":{"oneOf":[{"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"context"}}},{"type":"object","additionalProperties":false,"required":["kind","context_request_id","to"],"properties":{"kind":{"const":"move"},"context_request_id":{"type":"string","minLength":1,"maxLength":128},"to":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"integer"}}}},{"type":"object","additionalProperties":false,"required":["kind","original_request_id"],"properties":{"kind":{"const":"lookup"},"original_request_id":{"type":"string","minLength":1,"maxLength":128}}}] }}}}}}]})
        }
        Some("tools/call") => {
            if params.as_object().is_none_or(|p| {
                p.keys()
                    .any(|k| !["name", "arguments", "_meta"].contains(&k.as_str()))
            }) || params.get("name") != Some(&json!("dev_test_accounts_call"))
                || params.get("_meta").is_some_and(|m| {
                    !m.is_object()
                        || m.get("progressToken")
                            .is_some_and(|t| !t.is_string() && !t.is_number())
                })
            {
                return error(id, -32602);
            }
            match params
                .get("arguments")
                .cloned()
                .and_then(|x| serde_json::from_value::<ModelRequest>(x).ok())
            {
                Some(body) if METHODS.contains(&body.method.as_str()) => {
                    return Request::Business { id, body };
                }
                _ => return error(id, -32602),
            }
        }
        Some("initialize" | "ping" | "tools/list") => return error(id, -32602),
        _ => return error(id, -32601),
    };
    Request::Metadata(Some(json!({"jsonrpc":"2.0","id":id,"result":result})))
}
pub fn business_reply(id: Value, value: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value.get("status") != Some(&json!("ok"))}})
}
