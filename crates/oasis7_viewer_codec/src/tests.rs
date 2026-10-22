use super::*;
use oasis7::simulator::{WorldEventKind, WorldKernel};
use oasis7_client_api::viewer_protocol::GameplayActionError;

fn typed_error() -> ViewerResponse {
    ViewerResponse::GameplayActionError {
        error: GameplayActionError {
            code: "session_not_found".into(),
            message: "missing session".into(),
            action_id: Some("inspect_target".into()),
            target_agent_id: None,
        },
    }
}

#[test]
fn concrete_writer_and_string_preserve_legacy_compact_bytes() {
    let snapshot = WorldKernel::new().snapshot();
    let responses = vec![
        ViewerResponse::Snapshot {
            snapshot: snapshot.clone(),
        },
        ViewerResponse::ScopeSnapshot {
            request_generation: "generation-1".into(),
            snapshot,
        },
        ViewerResponse::Event {
            event: WorldEvent {
                id: 7,
                time: 3,
                kind: WorldEventKind::RuntimeEvent {
                    kind: "snapshot_created".into(),
                    domain_kind: None,
                },
                runtime_event: None,
            },
        },
        serde_json::from_str::<ViewerResponse>(
            r#"{"type":"hello_ack","server":"test","version":2,"world_id":"world"}"#,
        )
        .unwrap(),
        typed_error(),
    ]
    .into_boxed_slice();
    for response in responses {
        let expected = serde_json::to_vec(&response).unwrap();
        let mut actual = Vec::new();
        write_response_json(&mut actual, &response).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(response_to_json(&response).unwrap().as_bytes(), expected);
        assert_eq!(
            serde_json::from_slice::<ViewerResponse>(&actual).unwrap(),
            response
        );
    }
    assert_eq!(
        response_to_json(&typed_error()).unwrap(),
        r#"{"type":"gameplay_action_error","error":{"code":"session_not_found","message":"missing session","action_id":"inspect_target"}}"#
    );
}

#[test]
fn correlated_envelope_preserves_flatten_none_some_and_typed_error() {
    for nonce in [None, Some(0), Some(42), Some(u64::MAX)] {
        let response = typed_error();
        let mut actual = Vec::new();
        write_correlated_response_json(&mut actual, &response, nonce).unwrap();
        let base = r#"{"type":"gameplay_action_error","error":{"code":"session_not_found","message":"missing session","action_id":"inspect_target"}}"#;
        let expected = nonce.map_or_else(
            || base.to_owned(),
            |nonce| {
                format!(
                    "{},\"request_auth_nonce\":{nonce}}}",
                    &base[..base.len() - 1]
                )
            },
        );
        assert_eq!(actual, expected.as_bytes());
        let wire: serde_json::Value = serde_json::from_slice(&actual).unwrap();
        assert!(wire.get("response").is_none());
        assert_eq!(wire["type"], "gameplay_action_error");
        assert_eq!(
            wire.get("request_auth_nonce"),
            nonce.map(serde_json::Value::from).as_ref()
        );
        assert_eq!(
            serde_json::from_slice::<ViewerResponse>(&actual).unwrap(),
            response
        );
    }
}

#[test]
fn request_parser_preserves_success_and_original_errors() {
    for input in [
        r#"{"type":"control","mode":{"mode":"step","count":2},"request_id":1}"#,
        r#" {"type":"request_snapshot"} "#,
        "",
        "{",
        r#"{"type":"unknown"}"#,
        r#"{"type":"control"}"#,
    ] {
        match (
            request_from_json(input),
            serde_json::from_str::<ViewerRequest>(input),
        ) {
            (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
            (Err(actual), Err(expected)) => {
                assert_eq!(actual.to_string(), expected.to_string());
                assert_eq!(actual.classify(), expected.classify());
            }
            _ => panic!("request parsing changed for {input}"),
        }
    }
    assert!(request_from_json(r#"{"type":"request_snapshot"}"#).is_ok());
}

#[test]
fn writer_errors_propagate_without_flush_or_full_response_buffer() {
    struct LimitedWriter {
        remaining: usize,
    }
    impl Write for LimitedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "limit"));
            }
            let count = bytes.len().min(self.remaining);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            panic!("codec must not flush")
        }
    }
    for correlated in [false, true] {
        let mut writer = LimitedWriter { remaining: 8 };
        let result = if correlated {
            write_correlated_response_json(&mut writer, &typed_error(), Some(42))
        } else {
            write_response_json(&mut writer, &typed_error())
        };
        assert_eq!(
            result.unwrap_err().io_error_kind(),
            Some(std::io::ErrorKind::TimedOut)
        );
        assert_eq!(writer.remaining, 0);
    }
}
