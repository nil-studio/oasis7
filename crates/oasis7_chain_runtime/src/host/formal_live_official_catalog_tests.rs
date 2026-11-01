//! Official serializer→real Node admission codec only; no commit/World authority.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{task_cleanup_v2::*, task_stop_v2::*};
struct OwnerSignatureOnly;
impl oasis7_node::NodeExecutionHook for OwnerSignatureOnly {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn validate_guarded_stop_v2_action_payload(&self, q: &TaskStopSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()
    }
    fn validate_cleanup_v2_action_payload(&self, q: &TaskCleanupSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()
    }
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("codec fixture never commits".into())
    }
}
fn admit_official(raw: Vec<u8>) {
    oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(&raw).unwrap();
    let node = oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new(
            "node",
            "formal-fixture-world",
            oasis7_node::NodeRole::Observer,
        )
        .unwrap(),
    )
    .with_execution_hook(OwnerSignatureOnly);
    assert!(
        node.submit_consensus_action_payload(84, raw).is_ok(),
        "official closed frame must pass real Node canonical admission codec"
    );
}
#[test]
fn formal_official_guarded_stop_catalog_is_accepted_by_real_node_codec() {
    let (_, close, owner) = actual_task_fixture::fixture();
    let p = close.request;
    let signed = sign_read_request(
        GAME_TASK_STOP_DOMAIN_V2,
        GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: "official-stop".into(),
            custody_account_id: p.custody_account_id,
            termination_plan_digest: p.termination_plan_digest,
            initial_fence: p.permitted_branches.exact_initial_active,
            target_executor_public_key: p.signer_public_key,
            target_executor_generation: p.signer_generation,
            target_executor_nonce: p.signer_nonce,
            execute_until_ms: DecimalU64(5000),
            observation_grant: None,
        },
        &owner,
    )
    .unwrap();
    let q = TaskStopSubmitV2 {
        api_revision: 2,
        expected_world: WorldIdentity {
            world_id: signed.request.initial_fence.world_id.clone(),
            genesis_digest: signed.request.initial_fence.genesis_digest.clone(),
        },
        original: SignedGameTaskStopV2 {
            request: signed.request,
            subject_public_key: signed.subject_public_key,
            signature_hex: signed.signature_hex,
        },
        observation_nonce: "aa".repeat(32),
    };
    q.verify_owner_signature().unwrap();
    admit_official(correlation::encode_guarded_stop_consensus_v2(&q).unwrap());
}
#[test]
fn formal_official_close_catalog_is_accepted_by_real_node_codec() {
    let (_, original, _) = actual_task_fixture::fixture();
    let f = &original.request.permitted_branches.exact_initial_active;
    let q = TaskCleanupSubmitV2 {
        api_revision: 2,
        expected_world: WorldIdentity {
            world_id: f.world_id.clone(),
            genesis_digest: f.genesis_digest.clone(),
        },
        original,
    };
    q.verify_owner_signature().unwrap();
    admit_official(correlation::encode_cleanup_consensus_v2(&q).unwrap());
}
