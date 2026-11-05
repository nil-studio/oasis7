use super::*;

// Frozen schema-1 field order and required historical commit; ignores unknown fields.
#[derive(serde::Serialize, serde::Deserialize)]
struct FrozenHistoricalAuthorityV1 {
    pub schema_version: u16,
    pub role: TerminalRecoveryRoleV1,
    pub world: WorldIdentity,
    pub agent_id: String,
    pub owner_binding: String,
    pub identity_generation: u64,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub grant_id: String,
    pub grant_issued_at_tick: u64,
    pub grant_expires_at_tick: u64,
    pub lease: TerminalLeaseIdentityV1,
    pub prefix_correlation: RequestCorrelation,
    pub prefix_result_digest: String,
    pub prefix_action_id: u64,
    pub prefix_deadline_unix_ms: Option<u64>,
    pub prefix_committed_height: u64,
    pub prefix_commit: CommitRef,
    pub admitted_at_tick: u64,
    pub writer_public_key: String,
    pub delegation_generation: u64,
    pub current_runtime_binding: oasis7_agent_api::RuntimeBindingV1,
    pub current_task_fence: Option<oasis7_agent_api::game::GameTaskFenceV1>,
    pub read_commit: CommitRef,
}
fn current_value() -> Value {
    let mut value = terminal_authority_fixture();
    value["schema_version"] = json!(2);
    value.as_object_mut().unwrap().remove("prefix_commit");
    value["current_authenticated_journal"] = json!({
        "prefix_event_id":71,"prefix_event_time":224,"prefix_receipt":{"dispatched":true,"request_digest":value["request_digest"]},
        "reserve_correlation":correlation(),"reserve_full_result_digest":"reserve-result","reserve_receipt_digest":"reserve-receipt",
        "reserve_committed_height":226,"reserve_event_id":70,"reserve_event_time":224
    });
    value
}

#[test]
fn historical_terminal_bytes_and_signing_payload_remain_exact() {
    let old: FrozenHistoricalAuthorityV1 =
        serde_json::from_value(terminal_authority_fixture()).unwrap();
    let current: TerminalRecoveryAuthorityV1 =
        serde_json::from_value(terminal_authority_fixture()).unwrap();
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&current).unwrap()
    );
    assert_eq!(
        signing_bytes(LOOKUP_PATH, &old).unwrap(),
        signing_bytes(LOOKUP_PATH, &current).unwrap()
    );
    assert!(serde_json::from_value::<FrozenHistoricalAuthorityV1>(current_value()).is_err());
}

#[test]
fn current_journal_terminal_modes_events_and_receipt_fail_closed() {
    let original = current_value();
    let proof: TerminalRecoveryAuthorityV1 = serde_json::from_value(original.clone()).unwrap();
    proof.validate(&correlation()).unwrap();
    let response = IntentResponse::<Value> {
        contract_version: 1,
        correlation: correlation(),
        outcome: IntentOutcome::HistoryUnavailable,
        formal_profile: None,
        execution_evidence_scope: None,
        terminal_recovery_authority: Some(Box::new(proof)),
    };
    response.validate(&correlation()).unwrap();
    for (pointer, changed) in [
        ("/schema_version", json!(1)),
        ("/schema_version", json!(3)),
        ("/prefix_commit", serde_json::to_value(commit(227)).unwrap()),
        ("/current_authenticated_journal/prefix_event_id", json!(0)),
        (
            "/current_authenticated_journal/prefix_event_time",
            json!(303),
        ),
        ("/current_authenticated_journal/reserve_event_id", json!(71)),
        (
            "/current_authenticated_journal/reserve_event_time",
            json!(225),
        ),
        (
            "/current_authenticated_journal/reserve_committed_height",
            json!(228),
        ),
        (
            "/current_authenticated_journal/prefix_receipt/dispatched",
            json!(false),
        ),
        (
            "/current_authenticated_journal/prefix_receipt/request_digest",
            json!("other"),
        ),
        ("/prefix_deadline_unix_ms", json!(1)),
    ] {
        let mut value = original.clone();
        if pointer == "/prefix_commit" {
            value["prefix_commit"] = changed;
        } else {
            *value.pointer_mut(pointer).unwrap() = changed;
        }
        let accepted = serde_json::from_value::<TerminalRecoveryAuthorityV1>(value)
            .is_ok_and(|proof| proof.validate(&correlation()).is_ok());
        assert!(!accepted, "accepted changed {pointer}");
    }
    let mut late = original.clone();
    late["admitted_at_tick"] = json!(303);
    late["current_authenticated_journal"]["prefix_event_time"] = json!(303);
    assert!(
        serde_json::from_value::<TerminalRecoveryAuthorityV1>(late)
            .unwrap()
            .validate(&correlation())
            .is_err()
    );
    let mut formal = response.clone();
    formal.formal_profile = Some(FormalReadProfileV1::controlled());
    assert!(formal.validate(&correlation()).is_err());
    let mut fake_commit = response;
    fake_commit.outcome = IntentOutcome::Committed {
        commit: Box::new(commit(227)),
        receipt: json!({"dispatched":true}),
    };
    assert!(fake_commit.validate(&correlation()).is_err());
    for pointer in [
        "/current_authenticated_journal",
        "/current_authenticated_journal/prefix_receipt",
        "/current_authenticated_journal/reserve_correlation/key",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(serde_json::from_value::<TerminalRecoveryAuthorityV1>(value).is_err());
    }
}
