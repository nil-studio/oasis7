//! Deterministic protocol producer for the Node/WASM integration smoke test.
//! Fixed synthetic keys and snapshots only; no network, model, or real account.
use oasis7_agent_api::game::{
    DecimalU64, GAME_PARTICIPANT_ADMISSION_DOMAIN, GAME_PARTICIPANT_REGISTER_DOMAIN,
    GameParticipantAdmissionV1, GameParticipantRegisterV1, TaskContextAuthorityModeV1,
    TaskContextIdentityV1,
};
use oasis7_client_api::world_service::{
    self as ws,
    participant_discovery::{
        PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerControlResponseV1,
        ParticipantOwnerViewRequestV1,
    },
    task_context::{
        CapturedTaskContextV1, GoalExecutionSnapshotV1, PreparedTaskContextV1,
        TASK_CONTEXT_PREPARE_PATH, TASK_CONTEXT_READ_PATH, TaskContextGetV1, TaskContextPrepareV1,
        TaskPublicContextV1, context_deadline, verify_prepare,
    },
};
use oasis7_client_api::world_service_wire::{
    AuthenticatedLookup, CanonicalIntentResultV1, SchedulerOperationV1, WorldServicePayloadV1,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn key(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}
fn public(seed: u8) -> String {
    ws::sign_read_request("fixture", (), &key(seed))
        .unwrap()
        .subject_public_key
}
fn reply<T: Serialize>(domain: &str, request: &Value, payload: T) -> Value {
    let response = ws::sign_service_response(
        domain,
        ws::request_digest(domain, request).unwrap(),
        payload,
        &key(9),
    )
    .unwrap();
    json!({"ok":true,"world_response":response})
}
fn prepared(
    proof: ws::SignedReadRequest<TaskContextPrepareV1>,
    snapshot: GoalExecutionSnapshotV1,
) -> PreparedTaskContextV1 {
    verify_prepare(&proof).unwrap();
    snapshot.validate().unwrap();
    let operation = proof.request.client_operation_id.clone();
    let until = context_deadline(
        snapshot.captured_base_binding.base_tick,
        snapshot.authority_valid_until_tick().unwrap(),
    )
    .unwrap();
    let mut record = PreparedTaskContextV1 {
        schema_revision: 2,
        prepare_proof: proof.clone(),
        snapshot,
        reference_salt: "ab".repeat(32),
        valid_until_tick: until,
        identity: TaskContextIdentityV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            native_authorization_root: None,
            schema_revision: 2,
            fence: proof.request.fence,
            valid_until_tick: until,
            agent_session_id: format!("fixture-session-{operation}"),
            agent_turn_id: format!("fixture-turn-{operation}"),
            decision_request_id: format!("fixture-decision-{operation}"),
            retry_seq: 1,
            transport_attempt: 1,
            request_commitment: format!("blake3:{}", "0".repeat(64)),
            context_commitment: format!("blake3:{}", "0".repeat(64)),
            provider_invocation_commitment: format!("blake3:{}", "0".repeat(64)),
        },
    };
    record.identity.native_authorization_root = Some(
        record
            .snapshot
            .native_authority
            .as_ref()
            .unwrap()
            .native_authorization_root
            .clone(),
    );
    record.identity.request_commitment =
        record.commitment("oasis7/private-task-request/v1").unwrap();
    record.identity.context_commitment =
        record.commitment("oasis7/private-task-context/v1").unwrap();
    record.identity.provider_invocation_commitment = record
        .commitment("oasis7/private-task-invocation/v1")
        .unwrap();
    record.validate().unwrap();
    record
}
fn admission_response(command: &Value, snapshot: &GoalExecutionSnapshotV1) -> Value {
    let world = &snapshot.read_commit.world;

    let owner: ws::SignedReadRequest<GameParticipantRegisterV1> =
        serde_json::from_value(command["owner"].clone()).unwrap();
    ws::private_goal::verify_owner_signature_strict(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        &owner.request,
        &owner.subject_public_key,
        &owner.signature_hex,
    )
    .unwrap();
    let registration = &owner.request;
    let admission = GameParticipantAdmissionV1 {
        api_rev: 1,
        business_digest: oasis7_client_api::world_service_correlation::participant_business_digest(
            registration,
        )
        .unwrap(),
        identity_namespace: registration.identity_namespace.clone(),
        account_id: registration.account_id.clone(),
        world_id: world.world_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        owner_public_key: owner.subject_public_key.clone(),
        branch_id: snapshot.control.branch_id.clone(),
        reorg_epoch: snapshot.control.reorg_epoch,
        policy_generation: DecimalU64(1),
        valid_from_tick: DecimalU64(0),
        valid_until_tick: DecimalU64(10000),
    };
    serde_json::to_value(
        ws::sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN, admission, &key(9)).unwrap(),
    )
    .unwrap()
}
fn owner_response(body: &Value, snapshot: &GoalExecutionSnapshotV1) -> Value {
    let world = &snapshot.read_commit.world;
    let original: ParticipantOwnerViewRequestV1 = serde_json::from_value(body.clone()).unwrap();
    ws::verify_read_request(ws::VIEW_JSON_PATH, &original.read).unwrap();
    let f = &snapshot.control;
    let owner_view = json!({"canonical_agent_owner":{"agent_id":f.agent_id,"player_id":f.owner_binding,"public_key":f.owner_public_key,"world_id":f.world_id,"reorg_epoch":f.reorg_epoch.0,"canonical_authority":{"branch_id":f.branch_id,"agent_identity_generation":f.owner_generation.0},"current_intent_id":null},"game_task_control":f});
    let version = snapshot.read_commit.clone();
    let response: ParticipantOwnerControlResponseV1 = serde_json::from_value(json!({"schema_revision":1,"world":world,"version":{"commit":version,"visibility_scope":original.read.request.scope_id,"projection_revision":ws::request_digest("projection",&owner_view).unwrap()},"logical_tick":snapshot.captured_base_binding.base_tick,"view":owner_view})).unwrap();
    response.validate(&original).unwrap();
    reply(PARTICIPANT_OWNER_VIEW_PATH, body, response)
}
fn main() {
    let snapshot: GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(
        "../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    snapshot.validate().unwrap();
    let owner_seed = (0u8..=255)
        .find(|seed| public(*seed) == snapshot.control.owner_public_key)
        .unwrap();
    let world = snapshot.read_commit.world.clone();
    let mut last_prepared = None;
    let mut prefix = None;
    for line in io::stdin().lock().lines() {
        let command: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = match command["kind"].as_str().unwrap() {
            "init" => {
                json!({"world":world,"servicePublicKey":public(9),"ownerSeed":owner_seed,"executorSeed":73,"control":snapshot.control})
            }
            "admission" => admission_response(&command, &snapshot),
            "gateway" => {
                let body: Value =
                    serde_json::from_str(command["rawBody"].as_str().unwrap()).unwrap();
                let path = command["path"].as_str().unwrap();
                if path == "/participant/owner-view" {
                    owner_response(&body, &snapshot)
                } else {
                    let operation = &body["operation"];
                    let request = &operation["request"];
                    let kind = operation["kind"].as_str().unwrap();
                    match kind {
                        "prepare_context" => {
                            let proof = serde_json::from_value(request.clone()).unwrap();
                            let value = prepared(proof, snapshot.clone());
                            last_prepared = Some(value.clone());
                            reply(TASK_CONTEXT_PREPARE_PATH, request, value)
                        }
                        "read_context" => {
                            let original: ws::SignedReadRequest<TaskContextGetV1> =
                                serde_json::from_value(request.clone()).unwrap();
                            let prepared = last_prepared.clone().unwrap();
                            let captured: CapturedTaskContextV1 = CapturedTaskContextV1 {
                                schema_revision: 1,
                                prepared,
                                capture_commit: snapshot.read_commit.clone(),
                                capture_proof: CanonicalIntentResultV1 {
                                    request: prefix.clone().unwrap(),
                                    action_id: 1,
                                    committed_height: snapshot.read_commit.position,
                                    receipt: json!({"fixture":"prefix"}),
                                    rejected: None,
                                },
                                current_public_context: TaskPublicContextV1 {
                                    read_commit: snapshot.read_commit.clone(),
                                    captured_base_binding: snapshot.captured_base_binding.clone(),
                                    provider_authority: None,
                                    native_authority: snapshot.native_authority.clone(),
                                    actor_observation: snapshot.actor_observation.clone(),
                                },
                            };
                            captured.validate_against(&original).unwrap();
                            reply(TASK_CONTEXT_READ_PATH, request, captured)
                        }
                        "submit" | "lookup_executor" | "lookup" => {
                            let (domain, original, correlation) = if kind == "submit" {
                                let original: ws::SubmitIntentRequest<WorldServicePayloadV1> =
                                    serde_json::from_value(request.clone()).unwrap();
                                let correlation=oasis7_client_api::world_service_correlation::derive_correlation(world.clone(),&original.signed_payload).unwrap();
                                assert_eq!(correlation, original.correlation);
                                if matches!(&original.signed_payload,WorldServicePayloadV1::Scheduler(s) if matches!(&s.request.operation,SchedulerOperationV1::TaskProviderPrefix{..}))
                                {
                                    prefix = Some(original.clone());
                                }
                                (ws::SUBMIT_PATH, original.signed_payload, correlation)
                            } else {
                                let lookup: AuthenticatedLookup =
                                    serde_json::from_value(request.clone()).unwrap();
                                let correlation=oasis7_client_api::world_service_correlation::derive_correlation(world.clone(),&lookup.original).unwrap();
                                assert_eq!(correlation.key, lookup.request.key);
                                (ws::LOOKUP_PATH, lookup.original, correlation)
                            };
                            let _ = original;
                            let result = ws::IntentResponse {
                                contract_version: 1,
                                formal_profile: None,
                                execution_evidence_scope: None,
                                correlation,
                                outcome: ws::IntentOutcome::Committed {
                                    commit: Box::new(snapshot.read_commit.clone()),
                                    receipt: json!({"source":kind,"fixture":true}),
                                },
                            };
                            reply(domain, request, result)
                        }
                        _ => panic!("unsupported fixture operation {kind}"),
                    }
                }
            }
            _ => panic!("unsupported fixture command"),
        };
        println!("{result}");
    }
}
