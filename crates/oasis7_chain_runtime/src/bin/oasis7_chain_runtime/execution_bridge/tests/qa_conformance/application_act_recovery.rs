//! Genuine signed Unknown before publication must reconcile only the original Act.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;

#[test]
fn real_tcp_hosted_act_unpublished_original_replays_exactly_once_canonically() {
    let fixture = Fixture::with_controlled_commits(true);
    let registration = fixture.delegation();
    fixture.client.submit(registration.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(registration.clone()),
    );
    fixture.committed(&registration);
    let original = cognition_request(&fixture);
    let original_bytes = serde_json::to_vec(&original).unwrap();
    // No Submit has happened: this covers a durable checkpoint before network I/O.
    let response = ViewerRuntimeLiveServer::test_replay_original_hosted_act(
        fixture.client.clone(),
        original.clone(),
    )
    .expect("verified unpublished original must replay exact signed request");
    assert!(matches!(response, SubmitObservation::Response(r)
        if matches!(r.outcome, IntentOutcome::Received { .. })));
    // A volatile received admission still has no published canonical receipt.
    // Its genuine signed Unknown must also reconcile without inventing a new key.
    let response = ViewerRuntimeLiveServer::test_replay_original_hosted_act(
        fixture.client.clone(),
        original.clone(),
    )
    .expect("received but unpublished original must reconcile");
    assert!(matches!(response, SubmitObservation::Response(r)
        if matches!(r.outcome, IntentOutcome::Received { .. })));
    assert_eq!(serde_json::to_vec(&original).unwrap(), original_bytes);
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        3,
        Some(original.clone()),
    );
    let committed = fixture.committed(&original);
    let first = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    first.validate(&original.correlation).unwrap();
    let IntentOutcome::Committed { commit, receipt } = first.outcome else {
        panic!("original Act must have genuine canonical receipt")
    };
    assert_eq!(*commit, committed);
    let canonical = fixture.driver.lock().unwrap().execution_world.clone();
    assert_eq!(canonical.agent_causal_receipts("agent-a").unwrap().len(), 1);
    let before = canonical.cognition().clone();
    let replay = fixture.client.submit(original.clone()).unwrap();
    let SubmitObservation::Response(replay) = replay else {
        panic!("completed replay must return original canonical result")
    };
    let IntentOutcome::Committed {
        receipt: replayed_receipt,
        ..
    } = replay.outcome
    else {
        panic!("completed replay must retain committed disposition")
    };
    assert_eq!(replayed_receipt, receipt);
    assert_eq!(
        fixture.driver.lock().unwrap().execution_world.cognition(),
        &before
    );
    println!(
        "PRE2_ACT_UNPUBLISHED_ORIGINAL_REPLAY_PASSED same_signed_bytes=true canonical_effects=1"
    );
}

#[test]
fn real_tcp_hosted_settlement_unpublished_original_replays_exactly_once_canonically() {
    use oasis7_runtime::{CognitionLeaseQuoteV1, CognitionLeaseRequestV1};
    let fixture = Fixture::with_controlled_commits(true);
    let registration = fixture.delegation();
    fixture.client.submit(registration.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(registration.clone()),
    );
    fixture.committed(&registration);
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let invocation = world.capability_invocation_contexts().values().find(|c| matches!(&c.subject, oasis7_wasm_abi::CapabilitySubject::Agent {agent_id,..} if agent_id=="agent-a") && c.presenter.presenter_kind=="provider").unwrap();
    let authority = oasis7::simulator::h_v1(
        oasis7::simulator::COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        invocation,
    )
    .to_string();
    let reserve = fixture.request(WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            wire::SchedulerIntentV1 {
                agent_id: "agent-a".into(),
                request_id: "qa-reserve-recovery".into(),
                delegation_generation: 1,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::ReserveLease(CognitionLeaseRequestV1::new(
                    "qa-settle-lease",
                    "owner-a",
                    "agent-a",
                    "qa-session",
                    "qa-turn",
                    "qa-decision",
                    "qa-request",
                    CognitionLeaseQuoteV1::new("qa-quote", "cognition_units", 1).with_authority(
                        "owner-a",
                        oasis7_runtime::COGNITION_RESOURCE_VERSION_V1,
                        "provider_cognition",
                        "agent_turn",
                        oasis7_runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                        authority,
                        world
                            .current_cognition_runtime_binding()
                            .unwrap()
                            .base_world_hash
                            .to_string(),
                    ),
                )),
            },
            &hex::encode([8u8; 32]),
        )
        .unwrap(),
    ));
    fixture.client.submit(reserve.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        3,
        Some(reserve.clone()),
    );
    fixture.committed(&reserve);
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let lease_id = world
        .cognition_economy()
        .unwrap()
        .leases
        .values()
        .find(|l| l.decision_request_id == "qa-decision")
        .unwrap()
        .lease_id
        .clone();
    let original = fixture.request(WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            wire::SchedulerIntentV1 {
                agent_id: "agent-a".into(),
                request_id: "qa-settlement-recovery".into(),
                delegation_generation: 1,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::SettleLease {
                    lease_id: lease_id.clone(),
                    consumed_amount: 1,
                },
            },
            &hex::encode([8u8; 32]),
        )
        .unwrap(),
    ));
    for _ in 0..2 {
        let response = ViewerRuntimeLiveServer::test_replay_original_hosted_settlement(
            fixture.client.clone(),
            original.clone(),
        )
        .expect("verified unpublished original settlement must replay");
        assert!(
            matches!(response, SubmitObservation::Response(r) if matches!(r.outcome,IntentOutcome::Received {..}))
        );
    }
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        4,
        Some(original.clone()),
    );
    fixture.committed(&original);
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .cognition()
        .clone();
    let economy = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .cognition_economy()
        .unwrap();
    assert_eq!(economy.leases[&lease_id].settled_amount, 1);
    assert_eq!(
        economy
            .journal
            .iter()
            .filter(|e| e.lease_id == lease_id && e.event_kind == "settle")
            .count(),
        1
    );
    let replay = fixture.client.submit(original).unwrap();
    assert!(
        matches!(replay, SubmitObservation::Response(r) if matches!(r.outcome,IntentOutcome::Committed {..}))
    );
    assert_eq!(
        fixture.driver.lock().unwrap().execution_world.cognition(),
        &before
    );
    assert_eq!(
        fixture
            .driver
            .lock()
            .unwrap()
            .execution_world
            .cognition_economy()
            .unwrap(),
        economy,
        "original duplicate must preserve the complete canonical economy ledger"
    );
    println!("PRE2_SETTLEMENT_UNPUBLISHED_ORIGINAL_REPLAY_PASSED canonical_settlements=1");
}
