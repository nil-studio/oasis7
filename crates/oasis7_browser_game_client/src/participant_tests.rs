use super::*;
use oasis7_agent_api::game::DecimalU64;
use serde_json::json;

#[test]
fn reused_imported_binding_accepts_previous_epoch_but_never_another_owner_or_account() {
    let snapshot: ws::task_context::GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(
        "../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    let original: GameParticipantRegisterV1 = serde_json::from_value(json!({"api_rev":1,
        "client_operation_id":"current", "identity_namespace":{"service_id":"s",
        "environment_id":"e","instance_epoch":"2"},"account_id":"acct",
        "world_id":snapshot.control.world_id,"genesis_digest":snapshot.control.genesis_digest,
        "owner_public_key":snapshot.control.owner_public_key,"owner_key_version":"1"}))
    .unwrap();
    let signed = ws::sign_read_request(
        "game_task_create",
        oasis7_agent_api::game::GameTaskCreateV1 {
            api_rev: 1,
            client_operation_id: "scope".into(),
            world_id: original.world_id.clone(),
            genesis_digest: original.genesis_digest.clone(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
        },
        &"07".repeat(32),
    )
    .unwrap();
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        snapshot.read_commit.world.clone(),
        &oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameTaskCreate(signed),
    )
    .unwrap();
    let mut receipt = GameParticipantReceiptV1 {
        api_rev: 1,
        identity_namespace: original.identity_namespace.clone(),
        account_id: "acct".into(),
        world_id: original.world_id.clone(),
        genesis_digest: original.genesis_digest.clone(),
        agent_id: "migrated-agent".into(),
        owner_public_key: original.owner_public_key.clone(),
        owner_binding: "migration-binding".into(),
        owner_generation: DecimalU64(3),
        client_operation_id: "old-operation".into(),
        business_digest: "old-business-digest".into(),
        registered_tick: DecimalU64(1),
        imported: true,
    };
    receipt.identity_namespace.instance_epoch = DecimalU64(1);
    let response = |receipt: &GameParticipantReceiptV1| {
        serde_json::to_string(&ws::IntentResponse {
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation: correlation.clone(),
            outcome: ws::IntentOutcome::Committed {
                commit: Box::new(snapshot.read_commit.clone()),
                receipt: serde_json::to_value(receipt).unwrap(),
            },
        })
        .unwrap()
    };
    let request = serde_json::to_string(&original).unwrap();
    super::receipt(&request, &response(&receipt)).unwrap();
    let mut invalid = receipt.clone();
    invalid.owner_public_key = "00".repeat(32);
    assert!(super::receipt(&request, &response(&invalid)).is_err());
    invalid = receipt.clone();
    invalid.account_id = "other".into();
    assert!(super::receipt(&request, &response(&invalid)).is_err());
    invalid = receipt.clone();
    invalid.identity_namespace.environment_id = "other".into();
    assert!(super::receipt(&request, &response(&invalid)).is_err());
    invalid = receipt;
    invalid.imported = false;
    assert!(super::receipt(&request, &response(&invalid)).is_err());
}
