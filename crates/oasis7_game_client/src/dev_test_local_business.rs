//! Linux account-tool adapter: kernel UID selects the principal, never model JSON.
use crate::dev_test_operations::Call;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "linux", test))]
use serde_json::Value;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<Call>,
}
pub const METHODS: [&str; 9] = [
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
#[cfg(any(target_os = "linux", test))]
pub(crate) fn decode(body: &[u8]) -> Result<ModelRequest, &'static str> {
    let request: ModelRequest =
        serde_json::from_slice(body).map_err(|_| "invalid_local_tool_request")?;
    if !METHODS.contains(&request.method.as_str()) {
        return Err("local_method_unavailable");
    }
    Ok(request)
}
#[cfg(any(target_os = "linux", test))]
fn trusted_envelope(request: ModelRequest, credential: &str) -> Result<Vec<u8>, &'static str> {
    let mut value = serde_json::to_value(request).map_err(|_| "invalid_local_tool_request")?;
    value
        .as_object_mut()
        .ok_or("invalid_local_tool_request")?
        .insert("credential".into(), Value::String(credential.into()));
    // The shared DTO remains the authority for the backend envelope.
    let _: crate::dev_test_sessions::ToolRequest =
        serde_json::from_value(value.clone()).map_err(|_| "invalid_local_tool_request")?;
    serde_json::to_vec(&value).map_err(|_| "invalid_local_tool_request")
}
#[cfg(target_os = "linux")]
pub fn serve(
    socket: &std::path::Path,
    config_path: &std::path::Path,
    caller_uid: u32,
    caller_id: &str,
) -> Result<(), String> {
    use crate::dev_test_local_adapter::{self as transport, LocalPolicy};
    use std::time::{Duration, Instant};
    let service_uid = unsafe { libc::geteuid() };
    if service_uid == caller_uid {
        return Err("distinct_model_uid_required".into());
    }
    let listener = transport::bind(socket, service_uid)?;
    eprintln!("local_business_adapter_ready");
    for connection in listener.incoming() {
        let mut stream = connection.map_err(|_| "local_accept_failed")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|_| "local_timeout_failed")?;
        let body = crate::dev_test_private_config::read_private_config(config_path)?;
        let config: crate::dev_test_sessions::ServiceConfig =
            serde_json::from_slice(&body).map_err(|_| "invalid_private_policy")?;
        config.service.validate()?;
        let deadline = Instant::now() + Duration::from_millis(config.service.timeout_ms);
        let policy = LocalPolicy {
            service_uid,
            caller_uid,
            max_request_bytes: config.service.max_request_bytes,
            timeout: Duration::from_millis(config.service.timeout_ms),
        };
        let result = (|| -> Result<Value, String> {
            let body = transport::receive_until(&mut stream, &policy, deadline)?;
            let request = decode(&body)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "clock_unavailable")?
                .as_secs();
            crate::dev_test_service::authenticate_local(&config.service, caller_id, now)?;
            let credential = config
                .service
                .credentials
                .iter()
                .find(|c| c.caller_id == caller_id && !c.revoked && now < c.expires_at_unix)
                .ok_or("unauthorized")?;
            let bytes = trusted_envelope(request, &credential.token)?;
            crate::dev_test_service::call_serialized_until(&config.service.socket, &bytes, deadline)
        })();
        let value = match result {
            Ok(value) => value,
            Err(_) => {
                serde_json::json!({"status":"local_request_denied","next":"retain_request_and_check_current_local_authorization"})
            }
        };
        if let Ok(bytes) = serde_json::to_vec(&value) {
            let _ = transport::respond_until(&mut stream, &bytes, deadline);
        }
    }
    Err("local_listener_closed".into())
}
#[cfg(target_os = "linux")]
pub fn call(socket: &std::path::Path, body: &[u8]) -> Result<Value, String> {
    decode(body)?;
    let response = crate::dev_test_local_adapter::call_local_bytes(
        socket,
        body,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )?;
    serde_json::from_slice(&response).map_err(|_| "invalid_local_response".into())
}
#[cfg(any(target_os = "linux", test))]
#[path = "dev_test_local_stdio.rs"]
mod stdio;
pub fn run(args: Vec<String>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::{io::Read, path::Path};
        if args.len() == 5 && args[0] == "serve" {
            return serve(
                Path::new(&args[1]),
                Path::new(&args[2]),
                args[3].parse().map_err(|_| "invalid_caller_uid")?,
                &args[4],
            );
        }
        if args.len() == 2 && args[0] == "stdio" {
            return stdio::run(Path::new(&args[1]));
        }
        if args.len() == 2 && args[0] == "call" {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(65537)
                .read_to_end(&mut bytes)
                .map_err(|_| "local_input_unavailable")?;
            if bytes.len() > 65536 {
                return Err("local_request_too_large".into());
            }
            let value = call(Path::new(&args[1]), &bytes)?;
            println!("{value}");
            return Ok(());
        }
        Err("usage: local_tools serve PUBLIC_SOCKET PRIVATE_POLICY CALLER_UID CALLER_ID | call APPROVED_SOCKET | stdio APPROVED_SOCKET".into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = args;
        Err("linux_peer_credentials_required".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_cannot_select_credentials_management_or_paths() {
        for field in [
            "credential",
            "config_path",
            "caller_id",
            "caller_uid",
            "origin",
            "args",
        ] {
            let mut value = serde_json::json!({"method":"accounts"});
            value[field] = serde_json::json!("forbidden");
            assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        for method in [
            "--apply",
            "identity-register",
            "world-rule",
            "transfer",
            "raw-sign",
        ] {
            assert!(
                decode(&serde_json::to_vec(&serde_json::json!({"method":method})).unwrap())
                    .is_err()
            );
        }
    }
    #[test]
    fn operation_identity_and_business_result_are_not_rewritten() {
        for (method, args) in [
            ("context", serde_json::json!({"kind":"context"})),
            (
                "move",
                serde_json::json!({"kind":"move","context_request_id":"context-A","to":[1,0,0]}),
            ),
            (
                "lookup",
                serde_json::json!({"kind":"lookup","original_request_id":"move-A"}),
            ),
        ] {
            let value = serde_json::json!({"method":method,"session_ref":"session-A","operation":{"request_id":"stable-ID","arguments":args}});
            let request = decode(&serde_json::to_vec(&value).unwrap()).unwrap();
            let bytes = trusted_envelope(request, "synthetic-private-credential").unwrap();
            let result: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(result["operation"], value["operation"]);
            assert_eq!(result["session_ref"], "session-A");
            assert_eq!(result["credential"], "synthetic-private-credential");
        }
        for method in [
            "accounts",
            "open",
            "close",
            "describe",
            "task",
            "capabilities",
        ] {
            assert!(
                decode(&serde_json::to_vec(&serde_json::json!({"method":method})).unwrap()).is_ok()
            );
        }
    }
}
