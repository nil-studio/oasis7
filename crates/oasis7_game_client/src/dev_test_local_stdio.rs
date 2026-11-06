use serde_json::{json, Value};

fn error(id: Value, code: i64) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":"local_tool_request_rejected"}})
}
fn dispatch(request: Value, backend: impl FnOnce(&[u8]) -> Result<Value, String>) -> Option<Value> {
    let id = request.get("id").cloned();
    if request.get("jsonrpc") != Some(&json!("2.0")) || !request.is_object() {
        return Some(error(Value::Null, -32600));
    }
    if id.is_none() && request.get("method") == Some(&json!("notifications/initialized")) {
        return None;
    }
    let Some(id) = id.filter(|v| v.is_string() || v.is_number()) else {
        return Some(error(Value::Null, -32600));
    };
    let result = match request.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"oasis7-local-account-tools","version":"1"}})
        }
        Some("ping") => json!({}),
        Some("tools/list") => {
            json!({"tools":[{"name":"dev_test_accounts_call","description":"Restricted account/session tools. Preserve operation request IDs and inspect business outcomes; transport success does not mean an action committed.","inputSchema":{"type":"object","additionalProperties":false,"required":["method"],"properties":{"method":{"enum":super::METHODS},"grant_ref":{"type":"string"},"session_ref":{"type":"string"},"expires_at_unix":{"type":"integer","minimum":0},"operation":{"type":"object","additionalProperties":false,"required":["request_id","arguments"],"properties":{"request_id":{"type":"string"},"arguments":{"type":"object"}}}}}}]})
        }
        Some("tools/call") => {
            let Some(params) = request.get("params").and_then(Value::as_object) else {
                return Some(error(id, -32602));
            };
            if params.len() != 2 || params.get("name") != Some(&json!("dev_test_accounts_call")) {
                return Some(error(id, -32602));
            }
            let bytes = match params
                .get("arguments")
                .and_then(|v| serde_json::to_vec(v).ok())
            {
                Some(bytes) if super::decode(&bytes).is_ok() => bytes,
                _ => return Some(error(id, -32602)),
            };
            match backend(&bytes) {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"isError":value.get("status") != Some(&json!("ok"))})
                }
                Err(_) => {
                    json!({"content":[{"type":"text","text":"local_transport_unavailable; retain original request ID and body"}],"isError":true})
                }
            }
        }
        _ => return Some(error(id, -32601)),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
#[cfg(target_os = "linux")]
pub(super) fn run(socket: &std::path::Path) -> Result<(), String> {
    use std::io::{BufRead, Read, Write};
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut bytes = Vec::new();
        let size = input
            .by_ref()
            .take(65538)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "stdio_input_failed")?;
        if size == 0 {
            return Ok(());
        }
        if size > 65536 {
            return Err("stdio_request_too_large".into());
        }
        let response = match serde_json::from_slice(&bytes) {
            Ok(request) => dispatch(request, |body| super::call(socket, body)),
            Err(_) => Some(error(Value::Null, -32700)),
        };
        if let Some(response) = response {
            let mut stdout = std::io::stdout().lock();
            serde_json::to_writer(&mut stdout, &response).map_err(|_| "stdio_output_failed")?;
            writeln!(stdout).map_err(|_| "stdio_output_failed")?;
            stdout.flush().map_err(|_| "stdio_output_failed")?;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_secrets_before_backend_and_preserves_unknown() {
        let rejected = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dev_test_accounts_call","arguments":{"method":"accounts","credential":"forbidden"}}});
        assert!(dispatch(rejected, |_| panic!("backend must not run"))
            .unwrap()
            .get("error")
            .is_some());
        let request = json!({"jsonrpc":"2.0","id":"rpc-A","method":"tools/call","params":{"name":"dev_test_accounts_call","arguments":{"method":"lookup","session_ref":"S","operation":{"request_id":"lookup-A","arguments":{"kind":"lookup","original_request_id":"move-A"}}}}});
        let expected = request["params"]["arguments"].clone();
        let response = dispatch(request, |body| {
            assert_eq!(serde_json::from_slice::<Value>(body).unwrap(), expected);
            Ok(json!({"status":"ok","result":{"status":"unknown"}}))
        })
        .unwrap();
        assert_eq!(response["id"], "rpc-A");
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unknown"));
    }
}
