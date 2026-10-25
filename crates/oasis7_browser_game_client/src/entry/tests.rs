use super::*;
use serde_json::{Value, json};

fn task() -> GameTaskControlV1 {
    let snapshot: ws::task_context::GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(
        "../../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    snapshot.control
}
fn owner_secret(task: &GameTaskControlV1) -> String {
    (0u8..=255)
        .map(|seed| format!("{seed:02x}").repeat(32))
        .find(|secret| {
            ws::sign_read_request("fixture", (), secret)
                .unwrap()
                .subject_public_key
                == task.owner_public_key
        })
        .unwrap()
}
fn prepared() -> (SignedGoalPrepareV1, PreparedGoalReceiptV1) {
    let task = task();
    let raw = build("goal_prepare", &json!({"task_json":encode(&task).unwrap(),
        "client_operation_id":"browser-goal", "client_nonce":"ab".repeat(32),
        "content":{"goal_text":"Preserve owner complete goal 正文", "soft_constraints":["a", "b"]}
    }).to_string()).unwrap();
    let request: GameGoalPrepareV1 = parse(&raw).unwrap();
    assert_eq!(request.goal_revision.0, task.latest_goal_revision.0 + 1);
    let original = ws::private_goal::sign_goal_prepare(request, &owner_secret(&task)).unwrap();
    let salt = [42u8; 32];
    let receipt = PreparedGoalReceiptV1 {
        schema_revision: 1,
        goal_ref: ws::private_goal::prepared_goal_commitment(&salt, &original).unwrap(),
        reference_salt: "2a".repeat(32),
        owner_proof: original.clone(),
    };
    (original, receipt)
}
#[test]
fn goal_preparation_and_publication_use_exact_native_private_contracts() {
    let (original, receipt) = prepared();
    let original_json = encode(&original).unwrap();
    let receipt_json = encode(&receipt).unwrap();
    validate_prepared(&original_json, &receipt_json).unwrap();
    let published = build(
        "goal_publish",
        &json!({"original_json":original_json,
        "receipt_json":receipt_json,"client_operation_id":"browser-publish"})
        .to_string(),
    )
    .unwrap();
    let expected =
        ws::private_goal_read::publication(&original, &receipt, "browser-publish".into()).unwrap();
    assert_eq!(published, encode(&expected).unwrap());
    assert!(!published.contains("goal_digest"));
    assert!(!published.contains("Preserve owner"));
    assert!(!published.contains("reference_salt"));
    let mut altered = receipt.clone();
    altered.reference_salt = "00".repeat(32);
    assert_eq!(
        validate_prepared(&encode(&original).unwrap(), &encode(&altered).unwrap()),
        Err("contract_mismatch")
    );
    let mut wrong_original = original.clone();
    wrong_original.request.client_operation_id.push('x');
    assert!(
        validate_prepared(
            &encode(&wrong_original).unwrap(),
            &encode(&receipt).unwrap()
        )
        .is_err()
    );
}
#[test]
fn goal_prepare_rejects_overflow_invalid_nonce_and_incomplete_content() {
    let mut task = task();
    let mut input = json!({"task_json":encode(&task).unwrap(),
        "client_operation_id":"goal", "client_nonce":"ab".repeat(32),
        "content":{"goal_text":"goal", "soft_constraints":[]}});
    input["client_nonce"] = json!("bad");
    assert_eq!(
        build("goal_prepare", &input.to_string()),
        Err("contract_mismatch")
    );
    input["client_nonce"] = json!("ab".repeat(32));
    input["content"]["goal_text"] = json!(" ");
    assert_eq!(
        build("goal_prepare", &input.to_string()),
        Err("contract_mismatch")
    );
    input["content"]["goal_text"] = json!("goal");
    task.latest_goal_revision = DecimalU64(u64::MAX);
    input["task_json"] = json!(encode(&task).unwrap());
    assert_eq!(
        build("goal_prepare", &input.to_string()),
        Err("revision_overflow")
    );
}
#[test]
fn participant_candidate_is_account_stable_and_excludes_owner_epoch_operation() {
    let mut r: GameParticipantRegisterV1 = serde_json::from_value(json!({"api_rev":1,
        "client_operation_id":"first", "identity_namespace":{"service_id":"s",
        "environment_id":"e","instance_epoch":"1"},"account_id":"acct", "world_id":"w",
        "genesis_digest":"g","owner_public_key":"07".repeat(32),"owner_key_version":"1"}))
    .unwrap();
    let expected =
        ws::request_digest("participant-account-world", &("s", "e", "acct", "w", "g")).unwrap();
    let candidate = build("participant_agent_id", &encode(&r).unwrap()).unwrap();
    assert_eq!(
        candidate,
        encode(&format!(
            "participant:{}",
            expected.strip_prefix("blake3:").unwrap()
        ))
        .unwrap()
    );
    r.identity_namespace.instance_epoch = DecimalU64(2);
    r.client_operation_id = "retry".into();
    r.owner_public_key = "08".repeat(32);
    assert_eq!(
        candidate,
        build("participant_agent_id", &encode(&r).unwrap()).unwrap()
    );
    r.account_id.push('x');
    assert_ne!(
        candidate,
        build("participant_agent_id", &encode(&r).unwrap()).unwrap()
    );
}
#[test]
fn task_id_matches_native_correlation_and_task_create_keeps_u64_owner_generation() {
    let task = task();
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    let mut control = json!({"schema_revision":1,"world":snapshot["read_commit"]["world"],
        "version":{"commit":snapshot["read_commit"],"visibility_scope":"agent:a","projection_revision":"unused"},
        "logical_tick":0,"view":{"canonical_agent_owner":{"agent_id":task.agent_id,
        "player_id":task.owner_binding,"public_key":task.owner_public_key,"world_id":task.world_id,
        "reorg_epoch":18446744073709551615u64,"canonical_authority":{"branch_id":task.branch_id,
        "agent_identity_generation":18446744073709551615u64},"current_intent_id":null},"game_task_control":null}});
    let raw = build(
        "task_create",
        &json!({"owner_control_json":control.to_string(),
        "client_operation_id":"create"})
        .to_string(),
    )
    .unwrap();
    let request: GameTaskCreateV1 = parse(&raw).unwrap();
    assert_eq!(request.owner_generation.0, u64::MAX);
    assert_eq!(request.reorg_epoch.0, u64::MAX);
    let signed = ws::sign_read_request("game_task_create", request, &owner_secret(&task)).unwrap();
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        parse(&snapshot["read_commit"]["world"].to_string()).unwrap(),
        &oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameTaskCreate(signed),
    )
    .unwrap();
    assert_eq!(
        build("task_id", &encode(&correlation).unwrap()).unwrap(),
        encode(&format!(
            "game-task-{}",
            ws::request_digest("game-task-id", &correlation.key).unwrap()
        ))
        .unwrap()
    );
    control["view"]["game_task_control"] = serde_json::to_value(&task).unwrap();
    assert_eq!(
        build(
            "task_create",
            &json!({"owner_control_json":control.to_string(),
        "client_operation_id":"create"})
            .to_string()
        ),
        Err("task_already_present")
    );
}

#[test]
fn private_service_response_verifies_exact_owner_proposal_and_salted_reference() {
    let (original, receipt) = prepared();
    let secret = "09".repeat(32);
    let key = ws::sign_read_request("fixture", (), &secret)
        .unwrap()
        .subject_public_key;
    let domain = ws::private_goal_read::GOAL_PREPARE_PATH;
    let original_json = encode(&original).unwrap();
    let response = ws::sign_service_response(
        domain,
        ws::request_digest(domain, &original).unwrap(),
        receipt.clone(),
        &secret,
    )
    .unwrap();
    let verified =
        crate::verify(domain, &original_json, &encode(&response).unwrap(), &key).unwrap();
    assert_eq!(verified, encode(&receipt).unwrap());
    let mut altered = receipt;
    altered.goal_ref = "00".repeat(32);
    let signed_altered = ws::sign_service_response(
        domain,
        ws::request_digest(domain, &original).unwrap(),
        altered,
        &secret,
    )
    .unwrap();
    assert_eq!(
        crate::verify(
            domain,
            &original_json,
            &encode(&signed_altered).unwrap(),
            &key
        ),
        Err("response_verification_failed")
    );
}
