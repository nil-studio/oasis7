use super::*;
pub(in super::super) fn economy(fixture: &Fixture) -> serde_json::Value {
    serde_json::to_value(
        fixture
            .driver
            .lock()
            .unwrap()
            .execution_world
            .cognition_economy()
            .unwrap(),
    )
    .unwrap()
}
pub(in super::super) fn assert_initial_economic_requests(
    fixture: &Fixture,
    ack_count: usize,
    canonical_ack_count: usize,
) {
    let mut counts = [0usize; 5];
    let mut originals: [Option<SubmitIntentRequest<WorldServicePayloadV1>>; 5] =
        std::array::from_fn(|_| None);
    for entry in fixture.world_gate.feedback_ack_trace() {
        if entry["operation"] != "submit" {
            continue;
        }
        let request: SubmitIntentRequest<WorldServicePayloadV1> =
            serde_json::from_value(entry["request"].clone()).unwrap();
        let index = match &request.signed_payload {
            WorldServicePayloadV1::Cognition(_) => 2,
            WorldServicePayloadV1::FeedbackAck(_) => 4,
            WorldServicePayloadV1::Scheduler(signed) => match &signed.request.operation {
                SchedulerOperationV1::ReserveLease(_) => 0,
                SchedulerOperationV1::ProviderPrefix { .. } => 1,
                SchedulerOperationV1::SettleLease { .. } => 3,
                _ => panic!("unexpected fresh economic operation during ACK recovery"),
            },
            WorldServicePayloadV1::Delegation(_) => continue,
            _ => panic!("unexpected new gameplay during ACK recovery"),
        };
        assert_eq!(
            derive_correlation(
                request.correlation.key.world.clone(),
                &request.signed_payload
            )
            .unwrap(),
            request.correlation
        );
        counts[index] += 1;
        if let Some(original) = &originals[index] {
            assert_eq!(request.correlation, original.correlation);
            assert_eq!(
                request.signed_payload, original.signed_payload,
                "Unknown retries must preserve the complete original signed payload"
            );
        } else {
            originals[index] = Some(request);
        }
    }
    println!("actual raw Reserve/Prefix/Act/Settle/ACK Submit counts: {counts:?}");
    assert_eq!(
        originals.map(|request| usize::from(request.is_some())),
        [1, 1, 1, 1, ack_count]
    );
    // Count canonical effects independently of transport retries.
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let economy = world.cognition_economy().unwrap();
    economy.validate().unwrap();
    let originals: Vec<_> = fixture
        .world_gate
        .feedback_ack_trace()
        .iter()
        .filter(|entry| entry["operation"] == "submit")
        .map(|entry| {
            serde_json::from_value::<SubmitIntentRequest<WorldServicePayloadV1>>(
                entry["request"].clone(),
            )
            .unwrap()
        })
        .collect();
    for request in &originals {
        let matches: Vec<_> = world
            .capability_revocation_state()
            .world_service_results
            .values()
            .map(|value| {
                serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).unwrap()
            })
            .filter(|result| result.request.correlation == request.correlation)
            .collect();
        let expected_results = if matches!(
            &request.signed_payload,
            WorldServicePayloadV1::FeedbackAck(_)
        ) {
            canonical_ack_count
        } else {
            1
        };
        assert_eq!(
            matches.len(),
            expected_results,
            "canonical result count must match the actual dispatch/crash boundary"
        );
        if expected_results == 0 {
            assert!(matches!(
                &request.signed_payload,
                WorldServicePayloadV1::FeedbackAck(_)
            ));
            continue;
        }
        assert_eq!(matches[0].request.signed_payload, request.signed_payload);
        assert!(matches[0].rejected.is_none());
        match &request.signed_payload {
            WorldServicePayloadV1::Scheduler(signed) => match &signed.request.operation {
                SchedulerOperationV1::ReserveLease(reserve) => {
                    let expected = serde_json::to_value(reserve).unwrap();
                    let leases: Vec<_> = economy
                        .leases
                        .values()
                        .filter(|lease| {
                            let actual = serde_json::to_value(lease).unwrap();
                            [
                                "agent_id",
                                "agent_session_id",
                                "agent_turn_id",
                                "decision_request_id",
                                "request_digest",
                                "idempotency_key",
                                "account_id",
                                "quote",
                            ]
                            .iter()
                            .all(|field| actual[*field] == expected[*field])
                        })
                        .collect();
                    assert_eq!(leases.len(), 1);
                    for kind in ["reserve", "settle"] {
                        assert_eq!(
                            economy
                                .journal
                                .iter()
                                .filter(|event| event.lease_id == leases[0].lease_id
                                    && event.event_kind == kind)
                                .count(),
                            1
                        );
                    }
                }
                SchedulerOperationV1::ProviderPrefix { request, .. } => {
                    assert_native_events_once(
                        &world,
                        &serde_json::json!({
                            "agent_id": request.agent_subject,
                            "agent_session_id": request.agent_session_id,
                            "agent_turn_id": request.agent_turn_id,
                            "decision_request_id": request.decision_request_id,
                            "request_digest": request.request_digest,
                        }),
                        &["TurnStarted", "ContextCaptured", "RequestDispatched"],
                    );
                }
                SchedulerOperationV1::SettleLease {
                    lease_id,
                    consumed_amount,
                } => {
                    assert_eq!(economy.leases[lease_id].settled_amount, *consumed_amount);
                }
                _ => {}
            },
            WorldServicePayloadV1::Cognition(signed) => {
                assert_native_events_once(
                    &world,
                    &serde_json::to_value(&signed.request.request).unwrap(),
                    &["WorldReceiptLinked", "CognitionTurnCompleted"],
                );
            }
            _ => {}
        }
    }
}
fn assert_native_events_once(world: &RuntimeWorld, identity: &serde_json::Value, kinds: &[&str]) {
    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .unwrap();
    for kind in kinds {
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event_kind"] == *kind
                    && [
                        "agent_id",
                        "agent_session_id",
                        "agent_turn_id",
                        "decision_request_id",
                        "request_digest"
                    ]
                    .iter()
                    .all(|field| event[*field] == identity[*field]))
                .count(),
            1,
            "canonical native {kind} effect must occur exactly once for the complete original identity"
        );
    }
}

pub(in super::super) fn assert_recovery_delta(
    fixture: &Fixture,
    baseline: &[serde_json::Value],
    ack: &serde_json::Value,
    original_pending: &serde_json::Value,
    old_economy: &serde_json::Value,
) {
    let payload: WorldServicePayloadV1 = serde_json::from_value(ack["payload"].clone()).unwrap();
    let expected: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(ack["correlation"].clone()).unwrap();
    let original_payload: WorldServicePayloadV1 =
        serde_json::from_value(original_pending["payload"].clone()).unwrap();
    let original_correlation: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(original_pending["correlation"].clone()).unwrap();
    let WorldServicePayloadV1::Cognition(original_signed) = &original_payload else {
        panic!("actual original checkpoint Cognition required")
    };
    oasis7::world_service::authority::verify_read_request("cognition", original_signed).unwrap();
    assert_eq!(
        oasis7::world_service::derive_correlation(
            original_correlation.key.world.clone(),
            &original_payload
        )
        .unwrap(),
        original_correlation
    );
    assert!(
        baseline.iter().any(|entry| entry["operation"] == "submit"
            && entry["request"]["correlation"] == original_pending["correlation"]
            && entry["request"]["signed_payload"] == original_pending["payload"]),
        "original checkpoint must match actual pre-crash Cognition Submit"
    );
    let trace = fixture.world_gate.feedback_ack_trace();
    assert_eq!(&trace[..baseline.len()], baseline);
    for entry in &trace[baseline.len()..] {
        if entry["operation"] == "submit" {
            let request: SubmitIntentRequest<WorldServicePayloadV1> =
                serde_json::from_value(entry["request"].clone()).unwrap();
            assert_eq!(request.correlation, expected);
            assert_eq!(request.signed_payload, payload);
        } else {
            assert_eq!(entry["operation"], "lookup");
            let request: wire::AuthenticatedLookup =
                serde_json::from_value(entry["request"].clone()).unwrap();
            if request.request.key == expected.key {
                assert_eq!(request.request.key, expected.key);
                assert_eq!(request.original, payload);
            } else {
                // Recovery revalidates already-issued Cognition/settlement
                // receipts. Authority comes from exact actual pre-crash Submit
                // bytes, never a domain-level allowlist or a new request.
                let issued = baseline
                    .iter()
                    .filter(|entry| entry["operation"] == "submit")
                    .map(|entry| {
                        serde_json::from_value::<SubmitIntentRequest<WorldServicePayloadV1>>(
                            entry["request"].clone(),
                        )
                        .unwrap()
                    })
                    .find(|issued| issued.correlation.key == request.request.key)
                    .expect("recovery Lookup must match actual pre-crash Submit");
                assert_eq!(
                    oasis7::world_service::derive_correlation(
                        issued.correlation.key.world.clone(),
                        &issued.signed_payload
                    )
                    .unwrap(),
                    issued.correlation,
                    "original signature and complete correlation must validate"
                );
                assert_eq!(request.original, issued.signed_payload);
            }
        }
    }
    assert_eq!(
        &economy(fixture),
        old_economy,
        "actual canonical cognition economy cannot change during original ACK-only recovery"
    );
}
pub(in super::super) fn assert_full_ack_readback(fixture: &Fixture, ack: &serde_json::Value) {
    let payload: WorldServicePayloadV1 = serde_json::from_value(ack["payload"].clone()).unwrap();
    let WorldServicePayloadV1::FeedbackAck(signed) = &payload else {
        panic!("signed original ACK required")
    };
    oasis7::world_service::authority::verify_read_request("feedback_ack", signed).unwrap();
    let expected: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(ack["correlation"].clone()).unwrap();
    assert_eq!(
        oasis7::world_service::derive_correlation(expected.key.world.clone(), &payload).unwrap(),
        expected
    );
    let response = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: expected.key.clone(),
            },
            payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed { receipt, .. } = response.outcome else {
        panic!("actual canonical ACK receipt missing")
    };
    assert_eq!(
        receipt["acknowledged"],
        serde_json::to_value(&signed.request).unwrap()
    );
    assert_eq!(receipt["delivery_state"], "acked");
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let record = world
        .runtime_feedback_outbox()
        .unwrap()
        .into_iter()
        .find(|r| r.feedback_id == signed.request.feedback_id)
        .unwrap();
    assert_eq!(record.state, "acked");
    assert_eq!(
        record.envelope_digest,
        signed.request.original_envelope_digest
    );
    assert_eq!(record.agent_subject, signed.request.agent_id);
    assert_eq!(record.agent_session_id, signed.request.agent_session_id);
    assert_eq!(record.agent_turn_id, signed.request.agent_turn_id);
    assert_eq!(
        record.decision_request_id,
        signed.request.decision_request_id
    );
    assert_eq!(record.request_digest, signed.request.request_digest);
    assert_eq!(record.feedback_seq, signed.request.feedback_seq);
    let key = correlation::key_digest(&expected.key).unwrap();
    assert_eq!(
        fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|v| **v == format!("submit:{key}"))
            .count(),
        1
    );
}

/// An attacker can recompute a public checksum/correlation without the Agent
/// signing key. Exercise that exact stronger mutation on a genuine checkpoint.
pub(in super::super) fn recompute_tampered_private_acceptance(
    original: &serde_json::Value,
) -> serde_json::Value {
    let mut altered = original.clone();
    altered["provider_memory_store"]["entries"][0]["summary"] =
        serde_json::json!("tampered private content with recomputed public checksum");
    let pending = &altered["provider_service_pending"]["agent-a"];
    let ack = &pending["feedback_ack"];
    let receipt_id = ack["receipt_lineage"]["receipt_id"].as_str().unwrap();
    let memory = &altered["provider_memory_store"];
    let mappings: serde_json::Map<String, serde_json::Value> = memory["committed_by_digest"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| v.as_str() == Some(receipt_id))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let entries: Vec<serde_json::Value> = memory["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["receipt_id"] == receipt_id)
        .cloned()
        .collect();
    let digest=oasis7::simulator::h_v1("oasis7.app.private-feedback-acceptance.v1",&serde_json::json!({
        "original_cognition":pending["payload"],"original_feedback":ack["original_feedback"],"receipt_lineage":ack["receipt_lineage"],
        "captured_intents":pending["cognition"]["memory_write_intents"],"committed_by_digest":mappings,"receipt_memory_identity":entries
    })).to_string();
    let mut payload: WorldServicePayloadV1 =
        serde_json::from_value(ack["payload"].clone()).unwrap();
    let old_signature = match &mut payload {
        WorldServicePayloadV1::FeedbackAck(signed) => {
            let old = signed.signature_hex.clone();
            assert_ne!(signed.request.private_acceptance_digest, digest);
            signed.request.private_acceptance_digest = digest;
            old
        }
        _ => panic!("real signed ACK required"),
    };
    let WorldServicePayloadV1::FeedbackAck(signed) = &payload else {
        unreachable!()
    };
    assert_eq!(
        signed.signature_hex, old_signature,
        "attacker must retain original Agent signature"
    );
    assert!(oasis7::world_service::authority::verify_read_request("feedback_ack", signed).is_err());
    let old: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(ack["correlation"].clone()).unwrap();
    let mut new = old.clone();
    new.payload_digest =
        oasis7::world_service::authority::request_digest("intent", &payload).unwrap();
    let ack = &mut altered["provider_service_pending"]["agent-a"]["feedback_ack"];
    ack["payload"] = serde_json::to_value(payload).unwrap();
    ack["correlation"] = serde_json::to_value(new).unwrap();
    altered
}

pub(in super::super) fn replace_ack_signer(original: &serde_json::Value) -> serde_json::Value {
    let mut altered = original.clone();
    let ack = &original["provider_service_pending"]["agent-a"]["feedback_ack"];
    let payload: WorldServicePayloadV1 = serde_json::from_value(ack["payload"].clone()).unwrap();
    let WorldServicePayloadV1::FeedbackAck(signed) = payload else {
        panic!("real ACK required")
    };
    let foreign = oasis7::world_service::authority::sign_read_request(
        "feedback_ack",
        signed.request,
        &hex::encode([37u8; 32]),
    )
    .unwrap();
    assert_ne!(foreign.subject_public_key, signed.subject_public_key);
    oasis7::world_service::authority::verify_read_request("feedback_ack", &foreign).unwrap();
    let payload = WorldServicePayloadV1::FeedbackAck(foreign);
    let old: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(ack["correlation"].clone()).unwrap();
    let new = oasis7::world_service::derive_correlation(old.key.world, &payload).unwrap();
    let ack = &mut altered["provider_service_pending"]["agent-a"]["feedback_ack"];
    ack["payload"] = serde_json::to_value(payload).unwrap();
    ack["correlation"] = serde_json::to_value(new).unwrap();
    altered
}
