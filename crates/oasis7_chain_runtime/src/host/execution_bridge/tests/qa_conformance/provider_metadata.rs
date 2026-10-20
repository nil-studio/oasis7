//! Separate, real HTTP model-provider metadata contract; it never handles WorldService routes.
use super::*;
use std::io::Write;
pub(super) type DecisionObservation =
    Arc<dyn Fn(&oasis7::simulator::ContinuousAgentRequestContextV1) + Send + Sync>;
pub(super) struct MetadataServer {
    pub(super) endpoint: String,
    pub(super) decision_count: Arc<std::sync::atomic::AtomicUsize>,
    pub(super) feedback_payloads: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl MetadataServer {
    pub(super) fn assert_canonical_feedback_delivery(&self, fixture: &Fixture, required: bool) {
        let delivered = self.feedback_payloads.lock().unwrap();
        if !required {
            assert!(
                delivered.is_empty(),
                "missing runner must not deliver feedback"
            );
            return;
        }
        assert!(
            !delivered.is_empty(),
            "actual provider feedback delivery required"
        );
        let driver = fixture.driver.lock().unwrap();
        let records = driver.execution_world.runtime_feedback_outbox().unwrap();
        for raw in delivered.iter() {
            let record = records
                .iter()
                .find(|r| r.payload == *raw)
                .expect("provider must receive exact full canonical Node feedback");
            record.validate().unwrap();
        }
        assert!(
            delivered.iter().all(|raw| raw == &delivered[0]),
            "recovery delivery must retain exact original payload"
        );
        println!(
            "actual_provider_feedback_delivery_count={} exact_canonical_payload=true",
            delivered.len()
        );
    }

    pub(super) fn assert_canonical_terminal_feedback_delivery(
        &self,
        fixture: &Fixture,
        terminal_feedback_id: &str,
    ) {
        let delivered = self.feedback_payloads.lock().unwrap();
        assert!(
            !delivered.is_empty(),
            "genuine Wait/wake must deliver Runtime feedback to its configured provider"
        );
        let driver = fixture.driver.lock().unwrap();
        let records = driver.execution_world.runtime_feedback_outbox().unwrap();
        for raw in delivered.iter() {
            let record = records
                .iter()
                .find(|record| record.payload == *raw)
                .expect("provider must receive exact canonical Runtime feedback");
            record.validate().unwrap();
        }
        assert!(
            delivered
                .iter()
                .any(|raw| { raw["feedback_id"].as_str() == Some(terminal_feedback_id) }),
            "terminal Wait/wake feedback must reach the configured provider"
        );
        println!(
            "actual_provider_feedback_delivery_count={} terminal_feedback_delivered=true exact_canonical_payload=true",
            delivered.len()
        );
    }

    pub(super) fn start(
        gates: Option<std::path::PathBuf>,
        accounting_root: std::path::PathBuf,
        observation: Option<DecisionObservation>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let decision_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let decisions = decision_count.clone();
        let feedback_payloads = Arc::new(Mutex::new(Vec::new()));
        let feedbacks = feedback_payloads.clone();
        let worker = thread::spawn(move || {
            let mut preflight_reset = false;
            while !stopped.load(Ordering::SeqCst) {
                if !preflight_reset
                    && accounting_root
                        .join("fresh-provider-preflight-complete")
                        .exists()
                {
                    let observed = decisions.swap(0, Ordering::SeqCst);
                    assert_eq!(
                        observed, 1,
                        "one independently validated preflight decision"
                    );
                    fs::write(
                        accounting_root.join("fresh-provider-counter-reset"),
                        b"preflight=1; fresh=0",
                    )
                    .unwrap();
                    preflight_reset = true;
                }
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let bytes = http_fixture::read_request(&mut socket);
                        if bytes.is_empty() {
                            continue;
                        }
                        let first = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
                        let (status, static_body) = match first
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .as_slice()
                        {
                            ["GET", "/v1/provider/info"] => (
                                200,
                                r#"{"provider_id":"provider_local_bridge","name":"Provider Local Bridge","version":"0.1.0","protocol_version":"world-simulator-provider-loopback-http-v1","chain_resource_manifest_schema_version":"oasis7.world_resource_manifest.v1","chain_resource_delta_schema_version":"oasis7.world_resource_delta.v1","capabilities":["decision","feedback"],"supported_action_sets":["wait","wait_ticks","move_agent","speak_to_nearby","inspect_target","simple_interact"]}"#,
                            ),
                            ["GET", "/v1/provider/health"] => (
                                200,
                                r#"{"ok":true,"status":"ready","uptime_ms":42,"last_error":null,"queue_depth":0}"#,
                            ),
                            _ => (404, r#"{"error":"unsupported_metadata_route"}"#),
                        };
                        let (status, body) = if first
                            .starts_with("POST /v1/world-simulator/decision-context ")
                        {
                            let boundary = bytes
                                .windows(4)
                                .position(|window| window == b"\r\n\r\n")
                                .unwrap()
                                + 4;
                            let incoming: oasis7::simulator::ContinuousAgentRequestContextV1 =
                                serde_json::from_slice(&bytes[boundary..]).unwrap();
                            incoming.validate_production_lane().unwrap();
                            if preflight_reset && let Some(observe) = &observation {
                                observe(&incoming);
                            }
                            let hosted_wait = preflight_reset
                                && accounting_root.join("provider-hosted-wait").exists();
                            let final_budget = preflight_reset
                                && accounting_root
                                    .join("provider-hosted-final-budget")
                                    .exists();
                            let first_wait = hosted_wait
                                && decisions.load(Ordering::SeqCst)
                                    < if final_budget { 2 } else { 1 };
                            let repeated = preflight_reset
                                && accounting_root.join("provider-repeated-turns").exists();
                            if repeated {
                                let identity = serde_json::json!({
                                    "agent_session_id": incoming.agent_session_id,
                                    "agent_turn_id": incoming.agent_turn_id,
                                    "decision_request_id": incoming.decision_request_id,
                                    "request_digest": incoming.request_digest
                                });
                                let digest = blake3::hash(incoming.decision_request_id.as_bytes());
                                fs::write(
                                    accounting_root.join(format!("model-request-{digest}.json")),
                                    serde_json::to_vec(&identity).unwrap(),
                                )
                                .unwrap();
                            }
                            let base = oasis7::simulator::DecisionResponse {
                                decision: if first_wait {
                                    oasis7::simulator::ProviderDecision::WaitTicks { ticks: 2 }
                                } else {
                                    oasis7::simulator::ProviderDecision::Act {
                                        action_ref: "move_agent".into(),
                                        action: oasis7_simulator::Action::MoveAgent {
                                            agent_id: incoming.agent_subject.clone(),
                                            to: "runtime:2:2:0".into(),
                                        },
                                    }
                                },
                                module_command: None,
                                provider_error: None,
                                diagnostics: Default::default(),
                                trace_payload: Default::default(),
                                memory_write_intents: if !final_budget
                                    && (repeated || (hosted_wait && !first_wait))
                                {
                                    vec![oasis7::simulator::MemoryWriteIntent {
                                        scope: "session_private".into(),
                                        summary: format!(
                                            "actual repeated turn {}",
                                            incoming.decision_request_id
                                        ),
                                        tags: vec!["repeated-turn-conformance".into()],
                                    }]
                                } else {
                                    vec![]
                                },
                            };
                            let response = oasis7::simulator::ContinuousAgentResponseContextV1 {
                                response_digest: oasis7::simulator::cognition_response_digest(
                                    &base,
                                ),
                                base_decision_response: base,
                                context_discriminator: incoming.context_discriminator,
                                context_version: incoming.context_version,
                                agent_session_id: incoming.agent_session_id,
                                agent_turn_id: incoming.agent_turn_id,
                                decision_request_id: incoming.decision_request_id,
                                retry_seq: incoming.retry_seq,
                                transport_attempt: incoming.transport_attempt,
                                request_digest: incoming.request_digest,
                            };
                            decisions.fetch_add(1, Ordering::SeqCst);
                            (200, serde_json::to_string(&response).unwrap())
                        } else if first.starts_with("POST /v1/world-simulator/feedback-context ") {
                            let boundary =
                                bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                            let raw: serde_json::Value =
                                serde_json::from_slice(&bytes[boundary..]).unwrap();
                            let feedback: oasis7::simulator::FeedbackEnvelopeV1 =
                                serde_json::from_value(raw.clone()).unwrap();
                            assert_eq!(feedback.provenance, "runtime_authoritative");
                            assert!(feedback.feedback_seq > 0);
                            assert!(!feedback.feedback_id.is_empty());
                            assert!(matches!(
                                feedback.status.as_str(),
                                "committed" | "rejected" | "failed" | "pending"
                            ));
                            // Retain all Runtime projection extensions. The parent
                            // verifies these bytes against the actual Node outbox.
                            feedbacks.lock().unwrap().push(raw);
                            (200, r#"{"ok":true}"#.to_string())
                        } else {
                            (status, static_body.to_string())
                        };
                        if let Some(root) = &gates
                            && (!root.join("provider-gates-deferred").exists()
                                || root.join("provider-admission-gates-arm").exists())
                        {
                            let kind = if first.contains("/info ") {
                                "info"
                            } else if first.contains("/health ") {
                                "health"
                            } else {
                                "other"
                            };
                            if kind != "other" {
                                fs::write(
                                    root.join(format!("provider-{kind}-started")),
                                    b"parsed GET",
                                )
                                .unwrap();
                                let deadline = Instant::now() + Duration::from_secs(2);
                                while !root.join(format!("provider-{kind}-release")).exists()
                                    && Instant::now() < deadline
                                {
                                    thread::sleep(Duration::from_millis(2));
                                }
                            }
                        }
                        println!(
                            "separate_provider_metadata_request path={} status={status}",
                            first.split_whitespace().nth(1).unwrap_or("")
                        );
                        let response = format!(
                            "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("metadata fixture accept: {e}"),
                }
            }
        });
        Self {
            endpoint,
            decision_count,
            feedback_payloads,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for MetadataServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
