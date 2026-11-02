//! Selection/byte ownership only. The hook is an explicit scope-test double;
//! the separate actual HTTP test uses the real formal writer and Runtime.
use super::*;
use crate::node_runtime_core::cleanup_v2_tests as fixture;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{sign_read_request, task_cleanup_v2::*, task_stop_v2::*};
use std::sync::atomic::Ordering;
struct Hook;
impl NodeExecutionHook for Hook {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn validate_guarded_stop_v2_action_payload(&self, q: &TaskStopSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()?;
        if q.original.request.custody_account_id == "blocked" {
            Err("explicit test scope unavailable".into())
        } else {
            Ok(())
        }
    }
    fn validate_cleanup_v2_action_payload(&self, q: &TaskCleanupSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("selection test never installs an effect".into())
    }
}
fn stop(account: &str, op: &str, until: u64) -> TaskStopSubmitV2 {
    let close = fixture::request();
    let p = close.original.request;
    let raw = sign_read_request(
        GAME_TASK_STOP_DOMAIN_V2,
        GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: op.into(),
            custody_account_id: account.into(),
            termination_plan_digest: p.termination_plan_digest,
            initial_fence: p.permitted_branches.exact_initial_active,
            target_executor_public_key: p.signer_public_key,
            target_executor_generation: p.signer_generation,
            target_executor_nonce: p.signer_nonce,
            execute_until_ms: DecimalU64(until),
            observation_grant: None,
        },
        &"11".repeat(32),
    )
    .unwrap();
    TaskStopSubmitV2 {
        api_revision: 2,
        expected_world: close.expected_world,
        original: SignedGameTaskStopV2 {
            request: raw.request,
            subject_public_key: raw.subject_public_key,
            signature_hex: raw.signature_hex,
        },
        observation_nonce: "55".repeat(32),
    }
}
fn action(id: u64, payload: Vec<u8>) -> NodeConsensusAction {
    NodeConsensusAction::from_payload(id, "catalog-unit", payload).unwrap()
}
fn engine(actions: Vec<NodeConsensusAction>) -> PosNodeEngine {
    let config = NodeConfig::new("node", "world", crate::NodeRole::Observer).unwrap();
    let mut e = PosNodeEngine::new(&config).unwrap();
    merge_pending_consensus_actions_with_budget(
        &mut e.pending_consensus_actions,
        actions,
        e.max_pending_consensus_actions,
        &e.pending_consensus_action_queue_bytes,
        e.max_pending_consensus_action_queue_bytes,
        false,
    )
    .unwrap();
    e
}
#[test]
fn closed_task_singleton_preserves_multi_account_originals_and_all_byte_reservations() {
    let a = action(2, fixture::stop_catalog(stop("account-a", "a", 5000)));
    let b = action(3, fixture::stop_catalog(stop("account-b", "b", 5000)));
    let mut e = engine(vec![b.clone(), a.clone()]);
    let bytes = e
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    let first = e
        .drain_proposable_consensus_actions(1000, Some(&Hook))
        .unwrap();
    assert_eq!(first, vec![a]);
    assert_eq!(e.pending_consensus_actions.get(&3), Some(&b));
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
    assert_eq!(e.closed_v2_parked.get(&3), Some(&ParkedV2::WaitingTurn));
    let second = e
        .drain_proposable_consensus_actions(1001, Some(&Hook))
        .unwrap();
    assert_eq!(second, vec![b]);
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
}
#[test]
fn closed_task_ordinary_batch_is_unchanged_and_defers_closed_bytes_without_rebinding() {
    let raw = serde_cbor::to_vec(&serde_json::json!({"type":"Noop","data":{}})).unwrap();
    let a = action(1, raw.clone());
    let b = action(9, raw);
    let v2 = action(3, fixture::stop_catalog(stop("account-a", "a", 5000)));
    let mut ordinary = engine(vec![b.clone(), a.clone()]);
    assert_eq!(
        ordinary
            .drain_proposable_consensus_actions(1000, None)
            .unwrap(),
        vec![a.clone(), b.clone()]
    );
    let mut mixed = engine(vec![b.clone(), v2.clone(), a.clone()]);
    let bytes = mixed
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    assert_eq!(
        mixed
            .drain_proposable_consensus_actions(1000, Some(&Hook))
            .unwrap(),
        vec![a, b]
    );
    assert_eq!(mixed.pending_consensus_actions.get(&3), Some(&v2));
    assert_eq!(
        mixed
            .pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
    assert_eq!(
        mixed.closed_v2_parked.get(&3),
        Some(&ParkedV2::OrdinaryBatchPending)
    );
    assert_eq!(
        mixed
            .drain_proposable_consensus_actions(1001, Some(&Hook))
            .unwrap(),
        vec![v2]
    );
}
#[test]
fn closed_task_expired_or_blocked_stop_does_not_occupy_independent_legal_close() {
    let expired = action(1, fixture::stop_catalog(stop("account-a", "expired", 1000)));
    let blocked = action(2, fixture::stop_catalog(stop("blocked", "blocked", 5000)));
    let close = action(3, fixture::close_catalog(fixture::request()));
    let mut e = engine(vec![expired.clone(), blocked.clone(), close.clone()]);
    let bytes = e
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    assert_eq!(
        e.drain_proposable_consensus_actions(1000, Some(&Hook))
            .unwrap(),
        vec![close]
    );
    assert_eq!(e.pending_consensus_actions.get(&1), Some(&expired));
    assert_eq!(e.pending_consensus_actions.get(&2), Some(&blocked));
    assert_eq!(e.closed_v2_parked.get(&1), Some(&ParkedV2::Expired));
    assert_eq!(
        e.closed_v2_parked.get(&2),
        Some(&ParkedV2::PreflightUnavailable)
    );
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
    assert!(
        e.drain_proposable_consensus_actions(1001, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
}
#[test]
fn closed_task_unsupported_and_uncertain_clock_never_drop_or_propose_original() {
    let original = action(1, fixture::stop_catalog(stop("account-a", "a", 5000)));
    let mut e = engine(vec![original.clone()]);
    let bytes = e
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    assert!(
        e.drain_proposable_consensus_actions(0, Some(&Hook))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.closed_v2_parked.get(&1),
        Some(&ParkedV2::ClockUnavailable)
    );
    assert!(
        e.drain_proposable_consensus_actions(1000, None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.closed_v2_parked.get(&1),
        Some(&ParkedV2::PreflightUnavailable)
    );
    assert_eq!(e.pending_consensus_actions.get(&1), Some(&original));
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
}

#[test]
fn closed_task_mixed_height_bound_input_retains_actual_current_height_and_exact_bytes() {
    let mut e = engine(Vec::new());
    let height = e.next_height;
    let input = NodeReplicatedExecutionInputV1::new(
        PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND,
        height,
        vec![1, 2, 3, 5, 8],
    );
    let ordinary = NodeConsensusAction::from_payload(
        REPLICATED_EXECUTION_INPUT_ACTION_ID,
        REPLICATED_EXECUTION_INPUT_SUBMITTER,
        input.encode().unwrap(),
    )
    .unwrap();
    let closed = action(
        3,
        fixture::stop_catalog(stop("account-a", "height-bound", 5000)),
    );
    merge_pending_consensus_actions_with_budget(
        &mut e.pending_consensus_actions,
        vec![closed.clone(), ordinary.clone()],
        e.max_pending_consensus_actions,
        &e.pending_consensus_action_queue_bytes,
        e.max_pending_consensus_action_queue_bytes,
        false,
    )
    .unwrap();
    let bytes = e
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    let selected = e
        .drain_proposable_consensus_actions(1000, Some(&Hook))
        .unwrap();
    assert_eq!(selected, vec![ordinary.clone()]);
    assert_eq!(
        decode_replicated_execution_input_action(&selected[0])
            .unwrap()
            .unwrap()
            .target_height,
        height
    );
    assert_eq!(selected[0].payload_cbor, ordinary.payload_cbor);
    assert_eq!(e.pending_consensus_actions.get(&3), Some(&closed));
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
}

#[test]
fn closed_task_failed_proposal_retains_original_and_reserved_bytes() {
    let original = action(
        1,
        fixture::stop_catalog(stop("account-a", "failed-proposal", 5000)),
    );
    let mut e = engine(vec![original.clone()]);
    let bytes = e
        .pending_consensus_action_queue_bytes
        .load(Ordering::SeqCst);
    let selected = e
        .drain_proposable_consensus_actions(1000, Some(&Hook))
        .unwrap();
    assert_eq!(selected, vec![original.clone()]);
    e.retain_failed_closed_selection(&selected, bytes);
    assert_eq!(e.pending_consensus_actions.get(&1), Some(&original));
    assert_eq!(
        e.pending_consensus_action_queue_bytes
            .load(Ordering::SeqCst),
        bytes
    );
}
