//! Recovery of one durable private prepare only. Never accepts a submit intent,
//! signs a request, changes its nonce/lifetime, or creates a public prefix.
use super::*;
use crate::WorldServiceClientError as E;
use std::time::Instant;
fn retry_original<T>(
    original: &SignedReadRequest<TaskContextPrepareV1>,
    mut call: impl FnMut(&SignedReadRequest<TaskContextPrepareV1>) -> Result<T, E>,
    window: Duration,
    attempts: usize,
) -> Result<T, E> {
    let deadline = Instant::now() + window;
    let mut last_error = E::Transport("private preparation retry window exhausted".into());
    for attempt in 0..attempts {
        if Instant::now() >= deadline {
            break;
        }
        match call(original) {
            Ok(prepared) => return Ok(prepared),
            Err(error) => {
                let Some(delay) = polling::retry_delay(&error) else {
                    return Err(error);
                };
                last_error = error;
                if attempt + 1 == attempts {
                    break;
                }
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    break;
                };
                std::thread::sleep(delay.min(remaining));
            }
        }
    }
    Err(last_error)
}
impl Client {
    pub(super) fn prepare_original(
        &self,
        original: &SignedReadRequest<TaskContextPrepareV1>,
    ) -> Result<PreparedTaskContextV1, String> {
        retry_original(
            original,
            |original| self.remote.prepare_task_context(original),
            polling::POLL_WINDOW,
            polling::POLL_ATTEMPTS,
        )
        .map_err(|error| {
            format!(
                "private preparation {}; retry original operation",
                failure_tag(&error)
            )
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };
    fn original() -> SignedReadRequest<TaskContextPrepareV1> {
        let public = authority::sign_read_request("key", (), &hex::encode([7; 32]))
            .unwrap()
            .subject_public_key;
        let control:oasis7_agent_api::game::GameTaskControlV1=serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"a","owner_binding":"owner","owner_generation":"1","owner_public_key":public,"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_id":"executor","executor_binding":{"executor_id":"executor","executor_public_key":public,"delegation_generation":"1","executor_epoch":"1"},"created_operation_id":"create"})).unwrap();
        authority::sign_read_request(
            TASK_CONTEXT_PREPARE_PATH,
            TaskContextPrepareV1 {
                authority_mode: None,
                schema_revision: 1,
                fence: GameTaskFenceV1::from_control(&control),
                client_operation_id: "immutable-prepare".into(),
                client_nonce: "ab".repeat(32),
            },
            &hex::encode([7; 32]),
        )
        .unwrap()
    }
    #[test]
    fn authority_selection_preserves_provider_wire_and_is_signed_for_native() {
        #[derive(Serialize)]
        struct LegacyPrepare<'a> {
            schema_revision: u32,
            fence: &'a GameTaskFenceV1,
            client_operation_id: &'a str,
            client_nonce: &'a str,
        }
        let provider = original();
        let request = &provider.request;
        let legacy = LegacyPrepare {
            schema_revision: request.schema_revision,
            fence: &request.fence,
            client_operation_id: &request.client_operation_id,
            client_nonce: &request.client_nonce,
        };
        assert_eq!(
            serde_json::to_vec(request).unwrap(),
            serde_json::to_vec(&legacy).unwrap()
        );
        assert_eq!(
            serde_cbor::to_vec(request).unwrap(),
            serde_cbor::to_vec(&legacy).unwrap()
        );
        let mut native = request.clone();
        native.authority_mode = Some(TaskContextAuthorityModeV1::NativeExecutorV1);
        native.schema_revision = 2;
        let native =
            authority::sign_read_request(TASK_CONTEXT_PREPARE_PATH, native, &hex::encode([7; 32]))
                .unwrap();
        verify_prepare(&native).unwrap();
        assert_ne!(
            authority::request_digest(TASK_CONTEXT_PREPARE_PATH, &provider).unwrap(),
            authority::request_digest(TASK_CONTEXT_PREPARE_PATH, &native).unwrap()
        );
        let mut substituted = provider.clone();
        substituted.request.authority_mode = native.request.authority_mode;
        substituted.request.schema_revision = 2;
        assert!(verify_prepare(&substituted).is_err());
        let mut wrong_revision = native.request.clone();
        wrong_revision.schema_revision = 1;
        let wrong_revision = authority::sign_read_request(
            TASK_CONTEXT_PREPARE_PATH,
            wrong_revision,
            &hex::encode([7; 32]),
        )
        .unwrap();
        assert!(verify_prepare(&wrong_revision).is_err());
    }

    #[test]
    fn prepare_persisted_then_response_lost_retries_identical_signed_original() {
        let original = original();
        verify_prepare(&original).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = crate::tests::config(format!("http://{}", listener.local_addr().unwrap()));
        let transport = crate::WorldServiceTransport::new(config.clone()).unwrap();
        let persisted = Arc::new(Mutex::new(None));
        let stored = persisted.clone();
        let expected = original.clone();
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 1024];
                let proof = loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]);
                        assert!(head.starts_with(&format!("POST {TASK_CONTEXT_PREPARE_PATH} ")));
                        let length: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            break serde_json::from_slice::<SignedReadRequest<TaskContextPrepareV1>>(&bytes[end+4..end+4+length]).unwrap();
                        }
                    }
                };
                assert_eq!(
                    serde_json::to_value(&proof).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
                verify_prepare(&proof).unwrap();
                if attempt == 0 {
                    *stored.lock().unwrap() = Some(serde_json::to_value(&proof).unwrap());
                    continue;
                }
                assert_eq!(
                    stored.lock().unwrap().as_ref(),
                    Some(&serde_json::to_value(&proof).unwrap())
                );
                // The test service returns its already persisted winner; production
                // additionally validates full PreparedTaskContext payload semantics.
                let reply = authority::sign_service_response(
                    TASK_CONTEXT_PREPARE_PATH,
                    authority::request_digest(TASK_CONTEXT_PREPARE_PATH, &proof).unwrap(),
                    json!({"persisted_original":proof}),
                    &hex::encode([9; 32]),
                )
                .unwrap();
                let body = serde_json::to_vec(&reply).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        let attempts = Cell::new(0);
        let result = retry_original(
            &original,
            |proof| {
                attempts.set(attempts.get() + 1);
                let reply: SignedServiceResponse<Value> =
                    transport.private_signed_response(TASK_CONTEXT_PREPARE_PATH, proof, 4096)?;
                verify_private_reply(
                    TASK_CONTEXT_PREPARE_PATH,
                    proof,
                    &reply,
                    &config.trusted_service_public_key,
                )
                .map_err(E::Assurance)?;
                Ok(reply.payload)
            },
            Duration::from_secs(8),
            40,
        )
        .unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(
            result["persisted_original"],
            serde_json::to_value(&original).unwrap()
        );
        server.join().unwrap();
    }
    #[test]
    fn prepare_does_not_retry_authority_conflicts_or_extend_bounded_window() {
        for error in [
            E::Rejected {
                status: 400,
                message: "invalid".into(),
            },
            E::Rejected {
                status: 403,
                message: "forbidden".into(),
            },
            E::Rejected {
                status: 409,
                message: "expired".into(),
            },
            E::Assurance("signature".into()),
            E::Configuration("config".into()),
        ] {
            let calls = Cell::new(0);
            let mut error = Some(error);
            assert!(
                retry_original(
                    &original(),
                    |_| {
                        calls.set(calls.get() + 1);
                        Err::<(), _>(error.take().unwrap())
                    },
                    Duration::from_secs(8),
                    40
                )
                .is_err()
            );
            assert_eq!(calls.get(), 1);
        }
        let calls = Cell::new(0);
        let started = Instant::now();
        assert!(
            retry_original(
                &original(),
                |_| {
                    calls.set(calls.get() + 1);
                    Err::<(), _>(E::Cooldown {
                        status: 503,
                        retry_after: Duration::from_secs(60),
                    })
                },
                Duration::from_millis(20),
                40
            )
            .is_err()
        );
        assert_eq!(calls.get(), 1);
        assert!(started.elapsed() < Duration::from_secs(1));
        let calls = Cell::new(0);
        assert!(
            retry_original(
                &original(),
                |_| {
                    calls.set(calls.get() + 1);
                    Err::<(), _>(E::Rejected {
                        status: 503,
                        message: "unavailable".into(),
                    })
                },
                Duration::from_secs(8),
                2
            )
            .is_err()
        );
        assert_eq!(calls.get(), 2);
    }
}
