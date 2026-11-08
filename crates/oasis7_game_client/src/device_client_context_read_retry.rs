//! Exact-proof context reads retry only the fixed delivery snapshot race.
use super::*;
use crate::WorldServiceClientError as E;
use std::time::Instant;
fn retry_read<T>(
    original: &SignedReadRequest<TaskContextGetV1>,
    mut call: impl FnMut(&SignedReadRequest<TaskContextGetV1>) -> Result<T, E>,
    window: Duration,
) -> Result<T, E> {
    let deadline = Instant::now() + window;
    let mut last_error = E::Transport("context read retry window exhausted".into());
    for _ in 0..3 {
        if Instant::now() >= deadline {
            break;
        }
        match call(original) {
            Ok(captured) => return Ok(captured),
            Err(error) => {
                if !matches!(&error, E::Rejected { status:409, message } if message == TASK_CONTEXT_READ_DELIVERY_DRIFT)
                {
                    return Err(error);
                }
                last_error = error;
            }
        }
    }
    Err(last_error)
}
impl Client {
    pub(super) fn read_context_original(
        &self,
        original: &SignedReadRequest<TaskContextGetV1>,
    ) -> Result<CapturedTaskContextV1, E> {
        self.read_context_original_bounded(original, None)
    }
    pub(super) fn read_context_original_bounded(
        &self,
        original: &SignedReadRequest<TaskContextGetV1>,
        deadline: Option<Instant>,
    ) -> Result<CapturedTaskContextV1, E> {
        let window = deadline.map_or(Duration::from_secs(8), |deadline| {
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(8))
        });
        retry_read(
            original,
            |original| self.remote.read_task_context(original),
            window,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    fn original() -> SignedReadRequest<TaskContextGetV1> {
        let public = authority::sign_read_request("key", (), &hex::encode([7; 32]))
            .unwrap()
            .subject_public_key;
        let control:oasis7_agent_api::game::GameTaskControlV1=serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"a","owner_binding":"owner","owner_generation":"1","owner_public_key":public,"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_id":"executor","executor_binding":{"executor_id":"executor","executor_public_key":public,"delegation_generation":"1","executor_epoch":"1"},"created_operation_id":"create"})).unwrap();
        let context=serde_json::from_value(json!({"schema_revision":1,"fence":GameTaskFenceV1::from_control(&control),"agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request","retry_seq":1,"transport_attempt":1,"request_commitment":format!("blake3:{}","11".repeat(32)),"context_commitment":format!("blake3:{}","22".repeat(32)),"provider_invocation_commitment":format!("blake3:{}","33".repeat(32)),"valid_until_tick":"16"})).unwrap();
        authority::sign_read_request(
            TASK_CONTEXT_READ_PATH,
            TaskContextGetV1 {
                schema_revision: 1,
                context,
                prepare_operation_id: "original-prepare".into(),
            },
            &hex::encode([7; 32]),
        )
        .unwrap()
    }
    #[test]
    fn context_delivery_drift_http_retries_exact_proof_but_not_generic_conflict_or_permission() {
        for (status, code, expected_calls) in [
            (409, TASK_CONTEXT_READ_DELIVERY_DRIFT, 2),
            (409, "private_goal_conflict", 1),
            (403, TASK_CONTEXT_READ_DELIVERY_DRIFT, 1),
        ] {
            let original = original();
            verify_get(&original).unwrap();
            let expected = original.clone();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let config = crate::tests::config(format!("http://{}", listener.local_addr().unwrap()));
            let transport = crate::WorldServiceTransport::new(config.clone()).unwrap();
            let server = thread::spawn(move || {
                for attempt in 0..expected_calls {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut buf = [0; 2048];
                    let proof = loop {
                        let n = stream.read(&mut buf).unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&bytes[..end]);
                            assert!(head.starts_with(&format!("POST {TASK_CONTEXT_READ_PATH} ")));
                            let length: usize = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse().unwrap())
                                })
                                .unwrap();
                            if bytes.len() >= end + 4 + length {
                                break serde_json::from_slice::<SignedReadRequest<TaskContextGetV1>>(&bytes[end+4..end+4+length]).unwrap();
                            }
                        }
                    };
                    assert_eq!(
                        serde_json::to_value(&proof).unwrap(),
                        serde_json::to_value(&expected).unwrap()
                    );
                    verify_get(&proof).unwrap();
                    let (status, body) = if attempt == 0 {
                        (status, serde_json::to_vec(&json!({"error":code})).unwrap())
                    } else {
                        let reply = authority::sign_service_response(
                            TASK_CONTEXT_READ_PATH,
                            authority::request_digest(TASK_CONTEXT_READ_PATH, &proof).unwrap(),
                            json!({"same_context":proof.request.context}),
                            &hex::encode([9; 32]),
                        )
                        .unwrap();
                        (200, serde_json::to_vec(&reply).unwrap())
                    };
                    write!(stream,"HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                    stream.write_all(&body).unwrap();
                }
            });
            let calls = Cell::new(0);
            let result = retry_read(
                &original,
                |proof| {
                    calls.set(calls.get() + 1);
                    let response: SignedServiceResponse<Value> =
                        transport.private_signed_response(TASK_CONTEXT_READ_PATH, proof, 4096)?;
                    verify_private_reply(
                        TASK_CONTEXT_READ_PATH,
                        proof,
                        &response,
                        &config.trusted_service_public_key,
                    )
                    .map_err(E::Assurance)?;
                    Ok(response.payload)
                },
                Duration::from_secs(8),
            );
            if expected_calls == 2 {
                assert_eq!(
                    result.unwrap()["same_context"],
                    serde_json::to_value(&original.request.context).unwrap()
                );
            } else {
                assert!(result.is_err());
            }
            assert_eq!(calls.get(), expected_calls);
            server.join().unwrap();
        }
    }
    #[test]
    fn context_read_code_cannot_cross_quote_routes_and_is_bounded() {
        use oasis7_client_api::world_service::task_factory_quote::{
            TASK_FACTORY_QUOTE_PATH, TASK_QUOTE_BASE_DRIFT, TASK_QUOTE_DELIVERY_DRIFT,
        };
        for (path, code) in [
            (TASK_FACTORY_QUOTE_PATH, TASK_CONTEXT_READ_DELIVERY_DRIFT),
            (TASK_CONTEXT_READ_PATH, TASK_QUOTE_BASE_DRIFT),
            (TASK_CONTEXT_READ_PATH, TASK_QUOTE_DELIVERY_DRIFT),
        ] {
            let body = serde_json::to_vec(&json!({"error":code})).unwrap();
            assert_eq!(
                crate::quote_error_category(path, 409, &mut body.as_slice()),
                None
            );
        }
        let calls = Cell::new(0);
        assert!(
            retry_read(
                &original(),
                |_| {
                    calls.set(calls.get() + 1);
                    Err::<(), _>(E::Rejected {
                        status: 409,
                        message: TASK_CONTEXT_READ_DELIVERY_DRIFT.into(),
                    })
                },
                Duration::from_secs(8)
            )
            .is_err()
        );
        assert_eq!(calls.get(), 3);
        let calls = Cell::new(0);
        assert!(
            retry_read(
                &original(),
                |_| {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
                Duration::ZERO
            )
            .is_err()
        );
        assert_eq!(calls.get(), 0);
    }
}
