use super::*;
#[path = "test_session_live_context_collision_tests.rs"]
mod collision_tests;
fn rejected(value: serde_json::Value) {
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(value).is_err());
}
#[test]
fn live_context_core_restore_keeps_legacy_empty_but_rejects_stripped_or_generic_payment_links() {
    let v = live_ready();
    restore(&v.before_claim);
    restore(&v.r.f.state);
    let mut legacy = serde_json::to_value(&v.before_claim).unwrap();
    legacy["test_sessions"][ID]["context"]
        .as_object_mut()
        .unwrap()
        .remove("live_reads");
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(legacy).unwrap(),
        v.before_claim
    );
    let original = serde_json::to_value(&v.r.f.state).unwrap();
    for variant in 0..9 {
        let mut value = original.clone();
        let s = &mut value["test_sessions"][ID];
        match variant {
            0 => {
                s["context"].as_object_mut().unwrap().remove("live_reads");
            }
            1 => {
                for key in &v.descriptor.paid_ports {
                    s["purpose_transports"][key] = serde_json::json!("execution");
                }
            }
            2 => {
                s["purpose_transports"][&v.descriptor.paid_ports[0]]
                    .as_object_mut()
                    .unwrap()
                    .remove("kind");
            }
            3 => {
                s["context"]["live_reads"][claim().attempt]["reference_digest"] =
                    serde_json::json!("00".repeat(32));
                for key in &v.descriptor.paid_ports {
                    s["purpose_transports"][key]["reference_digest"] =
                        serde_json::json!("00".repeat(32));
                }
            }
            4 => {
                s["context"]["live_reads"][claim().attempt]["deadline"] =
                    serde_json::json!("50001");
            }
            5 => {
                s["context"]["live_reads"][claim().attempt]
                    .as_object_mut()
                    .unwrap()
                    .remove("archive");
            }
            6 => {
                s["purpose_transports"][&v.descriptor.paid_ports[0]]["port"] =
                    serde_json::json!("native_world_view");
            }
            7 => {
                s["context"]["attempts"][READS[0]]["observation"] = serde_json::Value::Null;
            }
            _ => {
                s["context"].as_object_mut().unwrap().remove("live_reads");
                for key in &v.descriptor.paid_ports {
                    s["purpose_transports"][key] = serde_json::json!("execution");
                }
            }
        }
        // Transport cardinalities and the authentic ledger totals are untouched.
        assert_eq!(
            value["test_sessions"][ID]["roots"],
            original["test_sessions"][ID]["roots"]
        );
        rejected(value);
    }
}
#[test]
fn live_context_core_claim_requires_two_distinct_paid_observed_reads_of_its_source() {
    let v = live_ready();
    let policy = Policy::new(v.r.f.state.draft());
    let provision = policy.current(&v.r.f.context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    for reads in [
        [READS[0], READS[0]],
        ["live-captured-before", "live-captured-after"],
        ["live-captured-put", READS[1]],
        ["not-paid", READS[1]],
    ] {
        let mut state = v.before_claim.clone();
        let before = state.clone();
        let mut input = claim();
        input.historical_reads = reads;
        assert!(
            state
                .claim_test_session_live_context_read(
                    ID,
                    input,
                    &current,
                    UnixMillis::new(12_022),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(state, before);
    }
    // A valid newly paid read without its observation is not an observed body.
    let mut state = v.before_claim.clone();
    state
        .claim_test_session_context_read(
            ID,
            ContextBodyRead {
                slot: ContextBodySlotKindV1::Captured,
                port: ContextBodyPortV1::RecoverBody,
                attempt: "paid-not-observed",
            },
            &current,
            UnixMillis::new(12_022),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = state.clone();
    let mut input = claim();
    input.historical_reads = ["paid-not-observed", READS[1]];
    assert!(
        state
            .claim_test_session_live_context_read(
                ID,
                input,
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(state, before);
}
#[test]
fn live_context_core_nonce_history_and_once_claim_fail_without_recharging() {
    let mut v = live_ready();
    let policy = Policy::new(v.r.f.state.draft());
    let provision = policy.current(&v.r.f.context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    let before = v.r.f.state.clone();
    assert!(
        v.r.f
            .state
            .claim_test_session_live_context_read(
                ID,
                claim(),
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
    let alternate = "e6".repeat(32);
    for (world, gateway) in [
        (claim().observation_nonce, alternate.as_str()),
        (alternate.as_str(), claim().gateway_observation_nonce),
        (claim().gateway_observation_nonce, claim().observation_nonce),
    ] {
        let input = LiveContextReadClaim {
            attempt: "new-live-attempt",
            observation_nonce: world,
            gateway_observation_nonce: gateway,
            historical_reads: READS,
        };
        assert!(
            v.r.f
                .state
                .claim_test_session_live_context_read(
                    ID,
                    input,
                    &current,
                    UnixMillis::new(12_022),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(v.r.f.state, before);
    }
    let source_nonce = "f1".repeat(32);
    let source_lifetime_nonce =
        v.r.f
            .state
            .test_session_live_descriptor(ID, SessionPurposeV1::Execution, SOURCE)
            .unwrap()
            .lifetime_nonce;
    for world in [
        source_nonce.as_str(),
        source_lifetime_nonce.as_str(),
        alternate.as_str(),
    ] {
        let mut state = v.before_claim.clone();
        let before = state.clone();
        let gateway = if world == alternate {
            world
        } else {
            alternate.as_str()
        };
        let input = LiveContextReadClaim {
            attempt: "nonce-collision",
            observation_nonce: world,
            gateway_observation_nonce: gateway,
            historical_reads: READS,
        };
        assert!(
            state
                .claim_test_session_live_context_read(
                    ID,
                    input,
                    &current,
                    UnixMillis::new(12_022),
                    &mut StrictCrypto
                )
                .is_err()
        );
        assert_eq!(state, before);
    }
    assert!(
        v.r.f
            .state
            .reserve_test_session_source(
                ID,
                SessionSourceReservation {
                    operation_id: "reverse-nonce-source",
                    nonce: claim().observation_nonce,
                    funding: SessionSourceFundingV1::Execution
                },
                UnixMillis::new(12_022)
            )
            .is_err()
    );
    assert_eq!(v.r.f.state, before);
}
#[test]
fn live_context_core_insufficient_three_ports_is_atomic_and_never_borrows_control() {
    let v = live_ready();
    let mut state = v.before_claim.clone();
    for i in 0..400 {
        if state.test_sessions()[ID].roots[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
            <= 2
        {
            break;
        }
        state
            .reserve_test_session_purpose_transport(
                ID,
                SessionPurposeV1::Execution,
                &format!("actual-prior-port-{i}"),
                UnixMillis::new(12_022),
            )
            .unwrap();
    }
    restore(&state);
    assert_eq!(
        state.test_sessions()[ID].roots[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap(),
        2
    );
    let policy = Policy::new(state.draft());
    let provision = policy.current(&v.r.f.context);
    let pin = v.r.f.pin.clone();
    let ns = scope(&pin);
    let current = live(&provision, &ns, &v.life, SOURCE, 12_022);
    let before = state.clone();
    assert!(
        state
            .claim_test_session_live_context_read(
                ID,
                claim(),
                &current,
                UnixMillis::new(12_022),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn live_context_core_legacy_prefix_generic_survives_but_new_keys_are_not_generic_ids() {
    let v = live_ready();
    let mut legacy = v.before_claim.clone();
    legacy
        .reserve_test_session_purpose_transport(
            ID,
            SessionPurposeV1::Execution,
            "live-context-legacy",
            UnixMillis::new(12_022),
        )
        .unwrap();
    restore(&legacy);
    let mut value = serde_json::to_value(&legacy).unwrap();
    value["test_sessions"][ID]["context"]
        .as_object_mut()
        .unwrap()
        .remove("live_reads");
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(value).unwrap(),
        legacy
    );
    for key in &v.descriptor.paid_ports {
        assert_eq!(key.len(), 275);
        assert!(key.is_ascii());
        let mut state = v.before_claim.clone();
        let before = state.clone();
        assert!(
            state
                .reserve_test_session_purpose_transport(
                    ID,
                    SessionPurposeV1::Execution,
                    key,
                    UnixMillis::new(12_022),
                )
                .is_err()
        );
        assert_eq!(state, before);
    }
    let original = serde_json::to_value(&v.r.f.state).unwrap();
    for class in 0..2 {
        let mut value = original.clone();
        let key = &v.descriptor.paid_ports[0];
        if class == 0 {
            let mut moved = value["test_sessions"][ID]["purpose_transports"]
                .as_object_mut()
                .unwrap()
                .remove(key)
                .unwrap();
            moved["source_operation_id"] = serde_json::json!("different-source");
            value["test_sessions"][ID]["purpose_transports"][key] = moved;
        } else {
            let replacement = value["test_sessions"][ID]["purpose_transports"]
                .as_object()
                .unwrap()
                .values()
                .find(|v| v["kind"] == "context_body")
                .unwrap()
                .clone();
            value["test_sessions"][ID]["purpose_transports"][key] = replacement;
        }
        rejected(value);
    }
    let mut value = original;
    let key = &v.descriptor.paid_ports[0];
    let marker = value["test_sessions"][ID]["purpose_transports"]
        .as_object_mut()
        .unwrap()
        .remove(key)
        .unwrap();
    let mut changed = key.clone();
    changed.replace_range(16..17, if &key[16..17] == "0" { "1" } else { "0" });
    value["test_sessions"][ID]["purpose_transports"][changed] = marker;
    rejected(value);
}
