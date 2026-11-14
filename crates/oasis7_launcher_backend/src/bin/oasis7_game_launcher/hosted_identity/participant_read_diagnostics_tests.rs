use super::*;
use std::{io::Write, net::TcpListener, thread};

fn entry(endpoint: String) -> ParticipantEntry {
    ParticipantEntry::new(
        EntryTrust {
            endpoint,
            service_public_key: sign_read_request("public", (), &hex::encode([90; 32]))
                .unwrap()
                .subject_public_key,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
        },
        Zeroizing::new(hex::encode([91; 32])),
    )
    .unwrap()
}

// The fixture accepts the real HTTP request and supplies only the controlled
// upstream response. No production body or error text is copied into diagnostics.
fn reply(
    status: u16,
    body: Vec<u8>,
    declared_extra: usize,
    delay_ms: u64,
) -> (ParticipantEntry, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut entry = entry(format!("http://{}", listener.local_addr().unwrap()));
    if delay_ms > 0 {
        entry.http = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(30))
            .build()
            .unwrap();
    }
    let task = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut incoming = Vec::new();
        loop {
            let mut chunk = [0; 2048];
            let n = socket.read(&mut chunk).unwrap();
            if n == 0 {
                return;
            }
            incoming.extend_from_slice(&chunk[..n]);
            if let Some(end) = incoming.windows(4).position(|x| x == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&incoming[..end]).to_ascii_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|x| x.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if incoming.len() >= end + 4 + length {
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(delay_ms));
        let _ = write!(
            socket,
            "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len() + declared_extra
        );
        let _ = socket.write_all(&body);
    });
    (entry, task)
}

fn error(entry: &ParticipantEntry, path: &str, maximum: usize) -> StoreError {
    entry
        .forward_bounded(path, VIEW_JSON_PATH, &json!({}), &json!({}), maximum)
        .unwrap_err()
}

#[test]
fn owner_read_failure_stages_are_fixed_and_do_not_leak_upstream_text() {
    let fixtures = [
        (
            502,
            b"private upstream body".to_vec(),
            0,
            0,
            1024,
            "participant_read_http_status",
        ),
        (
            200,
            b"private non-json body".to_vec(),
            0,
            0,
            1024,
            "participant_read_response_decode",
        ),
        (
            200,
            b"{}".to_vec(),
            0,
            0,
            1024,
            "participant_read_response_schema",
        ),
        (
            200,
            b"private".to_vec(),
            0,
            0,
            2,
            "participant_read_response_limit",
        ),
        (
            200,
            b"private".to_vec(),
            20,
            0,
            1024,
            "participant_read_response_read",
        ),
        (
            200,
            b"{}".to_vec(),
            0,
            100,
            1024,
            "participant_read_timeout",
        ),
    ];
    for (status, body, extra, delay, maximum, expected) in fixtures {
        let (entry, task) = reply(status, body, extra, delay);
        let result = error(&entry, PARTICIPANT_OWNER_VIEW_PATH, maximum);
        task.join().unwrap();
        assert!(
            matches!(&result, StoreError::Rejected(code) if code == expected),
            "{expected}: {result:?}"
        );
        assert_eq!(result.to_string(), expected);
    }
}

#[test]
fn owner_read_transport_failure_is_distinct_but_submit_and_lookup_remain_unknown() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let entry = entry(format!("http://{}", listener.local_addr().unwrap()));
    drop(listener);
    assert!(
        matches!(error(&entry, PARTICIPANT_OWNER_VIEW_PATH, 1024), StoreError::Rejected(code) if code == "participant_read_transport")
    );
    for path in [SUBMIT_PATH, LOOKUP_PATH] {
        assert!(matches!(
            error(&entry, path, 1024),
            StoreError::OutcomeUnknown
        ));
        let (entry, task) = reply(502, b"private diagnostic".to_vec(), 0, 0);
        assert!(matches!(
            error(&entry, path, 1024),
            StoreError::OutcomeUnknown
        ));
        task.join().unwrap();
    }
}

#[test]
fn owner_read_rejects_shape_context_and_signature_before_returning_payload() {
    for (mutation, expected) in [
        (0, "participant_read_response_shape"),
        (1, "participant_read_response_context"),
        (2, "participant_read_response_signature"),
    ] {
        let signed = oasis7_client_api::world_service::sign_service_response(
            VIEW_JSON_PATH,
            request_digest(VIEW_JSON_PATH, &json!({})).unwrap(),
            json!({"private":"must not appear in error"}),
            &hex::encode([90; 32]),
        )
        .unwrap();
        let mut body = serde_json::to_value(signed).unwrap();
        match mutation {
            0 => body["unexpected_private_field"] = json!("private"),
            1 => body["request_digest"] = json!("different-original"),
            _ => body["signature_hex"] = json!("00".repeat(64)),
        }
        let (entry, task) = reply(200, serde_json::to_vec(&body).unwrap(), 0, 0);
        let result = error(&entry, PARTICIPANT_OWNER_VIEW_PATH, 4096);
        task.join().unwrap();
        assert!(
            matches!(&result, StoreError::Rejected(code) if code == expected),
            "{expected}: {result:?}"
        );
        assert_eq!(result.to_string(), expected);
    }
}

#[test]
fn verified_owner_read_remains_available() {
    let signed = oasis7_client_api::world_service::sign_service_response(
        VIEW_JSON_PATH,
        request_digest(VIEW_JSON_PATH, &json!({})).unwrap(),
        json!({"observation":"original"}),
        &hex::encode([90; 32]),
    )
    .unwrap();
    let expected = serde_json::to_value(&signed).unwrap();
    let (entry, task) = reply(200, serde_json::to_vec(&signed).unwrap(), 0, 0);
    let result = entry
        .forward_bounded(
            PARTICIPANT_OWNER_VIEW_PATH,
            VIEW_JSON_PATH,
            &json!({}),
            &json!({}),
            4096,
        )
        .unwrap();
    task.join().unwrap();
    assert_eq!(result["world_response"], expected);
}

#[test]
fn context_conflict_read_is_a_read_failure_not_a_pending_operation() {
    use oasis7_client_api::world_service::task_context::{
        TASK_CONTEXT_LIVE_READ_PATH, TASK_CONTEXT_READ_PATH,
    };
    use oasis7_client_api::world_service::task_context_projection::CONTEXT_READ_ATTESTED_PATH_V2;
    for path in [
        TASK_CONTEXT_READ_PATH,
        TASK_CONTEXT_LIVE_READ_PATH,
        CONTEXT_READ_ATTESTED_PATH_V2,
    ] {
        let (entry, task) = reply(
            409,
            br#"{"error":"private_goal_conflict","private":"must not leak"}"#.to_vec(),
            0,
            0,
        );
        let result = error(&entry, path, 4096);
        task.join().unwrap();
        let response = super::super::service::IdentityReply::error(result);
        assert_eq!(response.status, 409, "{path}");
        assert_eq!(response.body["error_code"], "participant_read_http_status");
        assert!(!response.body.to_string().contains("must not leak"));
        assert!(!response.body.to_string().contains("operation_pending"));
    }
}

#[test]
fn context_read_diagnostics_preserve_mutation_unknown_and_drift_contracts() {
    use oasis7_client_api::world_service::{task_context::*, task_context_projection::*};
    for path in [
        TASK_CONTEXT_READ_PATH,
        TASK_CONTEXT_LIVE_READ_PATH,
        CONTEXT_READ_ATTESTED_PATH_V2,
    ] {
        for (status, body, maximum, expected) in [
            (
                403,
                b"private upstream permission".to_vec(),
                4096,
                "participant_read_http_status",
            ),
            (
                502,
                b"private upstream failure".to_vec(),
                4096,
                "participant_read_http_status",
            ),
            (
                200,
                b"private invalid json".to_vec(),
                4096,
                "participant_read_response_decode",
            ),
            (
                200,
                b"private oversized response".to_vec(),
                2,
                "participant_read_response_limit",
            ),
        ] {
            let (entry, task) = reply(status, body, 0, 0);
            let result = error(&entry, path, maximum);
            task.join().unwrap();
            assert!(matches!(result, StoreError::Rejected(ref code) if code == expected));
        }
        let (entry, task) = reply(
            409,
            br#"{"error":"task_context_read_delivery_drift"}"#.to_vec(),
            0,
            0,
        );
        let result = entry
            .forward_bounded(path, VIEW_JSON_PATH, &json!({}), &json!({}), 4096)
            .unwrap();
        task.join().unwrap();
        assert_eq!(
            result["upstream_read_error"]["code"],
            "task_context_read_delivery_drift"
        );
        assert!(result.get("world_response").is_none());
    }
    for path in [
        SUBMIT_PATH,
        LOOKUP_PATH,
        TASK_CONTEXT_PREPARE_PATH,
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        CONTEXT_MOVE_PROJECTION_PATH_V2,
        "/unrecognized-read",
    ] {
        let (entry, task) = reply(
            409,
            br#"{"error":"private_goal_conflict","private":"hidden"}"#.to_vec(),
            0,
            0,
        );
        assert!(matches!(
            error(&entry, path, 4096),
            StoreError::OutcomeUnknown
        ));
        task.join().unwrap();
    }
}
