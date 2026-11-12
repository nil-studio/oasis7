//! Linux account-tool adapter: kernel UID selects the principal, never model JSON.
#[cfg(any(target_os = "linux", test))]
use crate::dev_test_operations::Call;
pub use oasis7_custody_core::dev_test::{METHODS, ModelRequest};
#[cfg(any(target_os = "linux", test))]
use serde_json::Value;
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
// Once forwarding begins, transport failure cannot establish business rejection.
#[cfg(any(target_os = "linux", test))]
fn forwarded_result(
    result: Result<Value, String>,
    operation: Option<&Call>,
    session_ref: Option<&str>,
) -> Value {
    result.unwrap_or_else(|_| serde_json::json!({
        "status":"unknown",
        "request_id":operation.map(|op| op.request_id.as_str()),
        "original_request_id":operation.map(Call::recovery_request_id),
        "session_ref":session_ref,
        "next":"retain_original_request_id_and_body; use_lookup_for_original_operation_under_current_authorization; do_not_resubmit_as_new_operation"
    }))
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
        let prepared = (|| -> Result<_, String> {
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
            let operation = request.operation.clone();
            let session_ref = request.session_ref.clone();
            let bytes = trusted_envelope(request, &credential.token)?;
            Ok((bytes, operation, session_ref))
        })();
        let value = match prepared {
            Ok((bytes, operation, session_ref)) => forwarded_result(
                crate::dev_test_service::call_serialized_until(
                    &config.service.socket,
                    &bytes,
                    deadline,
                ),
                operation.as_ref(),
                session_ref.as_deref(),
            ),
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
    fn lost_lookup_keeps_its_original_target_and_separate_request_id() {
        let lookup: Call = serde_json::from_value(serde_json::json!({"request_id":"lookup-A","arguments":{"kind":"lookup","original_request_id":"move-A"}})).unwrap();
        let value = forwarded_result(Err("lost".into()), Some(&lookup), Some("session-A"));
        assert_eq!(value["original_request_id"], "move-A");
        assert_eq!(value["request_id"], "lookup-A");
        let context = Call {
            request_id: "context-A".into(),
            arguments: crate::dev_test_operations::Arguments::Context,
        };
        let value = forwarded_result(Err("lost".into()), Some(&context), None);
        assert_eq!(value["original_request_id"], "context-A");
        let value = forwarded_result(Err("lost".into()), None, None);
        assert!(value["original_request_id"].is_null());
        assert!(value["request_id"].is_null());
    }
    #[test]
    fn forwarded_failure_preserves_original_identity_as_unknown() {
        let movement = Call {
            request_id: "move-A".into(),
            arguments: crate::dev_test_operations::Arguments::Move {
                context_request_id: "context-A".into(),
                to: [1, 0, 0],
            },
        };
        let value = forwarded_result(
            Err("synthetic transport loss".into()),
            Some(&movement),
            Some("session-A"),
        );
        assert_eq!(value["status"], "unknown");
        assert_eq!(value["original_request_id"], "move-A");
        assert_eq!(value["session_ref"], "session-A");
        let committed = serde_json::json!({"status":"ok","result":{"active_goal":{"content":{"goal_text":"full goal", "soft_constraints":["constraint"]},"source":{"task_fence":{"agent_id":"A"},"active_goal_reference":{"goal_ref":"retained"},"owner_public_key":"verified owner","read_commit":{"position":7},"activation_commit":{"position":2}}}}});
        assert_eq!(
            forwarded_result(Ok(committed.clone()), Some(&movement), Some("session-A")),
            committed
        );
    }
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
