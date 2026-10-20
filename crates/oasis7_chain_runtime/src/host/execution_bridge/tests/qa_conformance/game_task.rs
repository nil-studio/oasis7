use super::*;
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
#[test]
fn game_task_real_tcp_create_lookup_replay_and_signed_owner_read() {
    let fixture = Fixture::new();
    let world = fixture.client.config().expected_world.clone();
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "native-task-create".into(),
                world_id: world.world_id,
                genesis_digest: world.genesis_digest,
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent-a".into(),
                owner_binding: "owner-a".into(),
                owner_generation: DecimalU64(1),
            },
            &fixture.owner,
        )
        .unwrap(),
    );
    let original = fixture.request(payload);
    fixture.client.submit(original.clone()).unwrap();
    let committed = fixture.committed(&original);
    let response = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    assert!(matches!(response.outcome, IntentOutcome::Committed { .. }));
    let replay = fixture.client.submit(original.clone()).unwrap();
    assert!(
        matches!(replay,SubmitObservation::Response(response) if matches!(response.outcome,IntentOutcome::Committed {..}))
    );
    let mut read = fixture.view(Some(committed));
    read.scope_id = "agent:agent-a".into();
    let mut config = fixture.client.config().clone();
    config.scope_id = read.scope_id.clone();
    let owner_client = RemoteWorldServiceClient::new(config).unwrap();
    let view = owner_client.read_view(read).unwrap();
    let control = view.projection().game_task_control.as_ref().unwrap();
    assert_eq!(control.created_operation_id, "native-task-create");
    assert_eq!(control.executor_epoch, DecimalU64(0));
    assert!(control.executor_id.is_none());
    fixture.finish_http_workers().unwrap();
}
