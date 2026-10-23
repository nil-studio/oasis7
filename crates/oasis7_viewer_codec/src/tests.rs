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

#[test]
fn buffered_writer_preserves_bytes_across_capacity_and_large_chunk_boundaries() {
    struct NoFlush(Vec<u8>);
    impl Write for NoFlush {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            panic!("codec must not flush the transport")
        }
    }
    let overhead = serde_json::to_vec(&ViewerResponse::Error {
        message: String::new(),
    })
    .unwrap()
    .len();
    let mut messages: Vec<String> = [127, 128, 129, 1023, 1024, 1025, 8191, 8192, 8193, 20000]
        .into_iter()
        .map(|length| "x".repeat(length - overhead))
        .collect();
    messages.push("雪🙂\n".repeat(2500));
    for message in messages {
        for response in vec![
            ViewerResponse::Error {
                message: message.clone(),
            },
            ViewerResponse::Event {
                event: WorldEvent {
                    id: 7,
                    time: 3,
                    kind: WorldEventKind::RuntimeEvent {
                        kind: message,
                        domain_kind: None,
                    },
                    runtime_event: None,
                },
            },
        ]
        .into_boxed_slice()
        {
            let expected = serde_json::to_vec(&response).unwrap();
            let mut writer = NoFlush(Vec::new());
            write_buffered_response_json(&mut writer, &response).unwrap();
            assert_eq!(writer.0, expected);
            assert_eq!(response_to_json(&response).unwrap().as_bytes(), expected);
        }
    }
}

#[test]
fn buffered_partial_errors_do_not_retry_pending_bytes_or_flush() {
    struct FaultWriter {
        bytes: Vec<u8>,
        remaining: usize,
        failed: bool,
        calls_after_error: usize,
        flushes: usize,
        kind: std::io::ErrorKind,
    }
    impl Write for FaultWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.failed {
                self.calls_after_error += 1;
                return Err(std::io::Error::new(self.kind, "injected failure"));
            }
            if self.remaining == 0 {
                self.failed = true;
                return Err(std::io::Error::new(self.kind, "injected failure"));
            }
            let count = bytes.len().min(self.remaining).min(3);
            self.bytes.extend_from_slice(&bytes[..count]);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }
    // Small responses fail while draining the final tail. Escapes force many
    // small writes, failing an internal full-buffer drain rather than a bypass.
    for (message, budget) in [("x".repeat(80), 8), ("\n".repeat(10000), 257)] {
        for response in vec![
            ViewerResponse::Error {
                message: message.clone(),
            },
            ViewerResponse::Event {
                event: WorldEvent {
                    id: 7,
                    time: 3,
                    kind: WorldEventKind::RuntimeEvent {
                        kind: message,
                        domain_kind: None,
                    },
                    runtime_event: None,
                },
            },
        ]
        .into_boxed_slice()
        {
            let expected = serde_json::to_vec(&response).unwrap();
            for kind in [std::io::ErrorKind::TimedOut, std::io::ErrorKind::BrokenPipe] {
                let mut writer = FaultWriter {
                    bytes: Vec::new(),
                    remaining: budget,
                    failed: false,
                    calls_after_error: 0,
                    flushes: 0,
                    kind,
                };
                let error = write_buffered_response_json(&mut writer, &response).unwrap_err();
                assert_eq!(error.classify(), serde_json::error::Category::Io);
                assert_eq!(error.io_error_kind(), Some(kind));
                assert_eq!(error.to_string(), "injected failure");
                assert_eq!(writer.bytes, expected[..budget]);
                assert!(writer.failed);
                assert_eq!(writer.calls_after_error, 0);
                assert_eq!(writer.flushes, 0);
            }
        }
    }
}
