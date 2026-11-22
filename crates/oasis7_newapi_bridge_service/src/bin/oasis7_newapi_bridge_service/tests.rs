use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::api::HttpRequest;
use super::model::{
    BindBridgeUserRequest, BridgeBindingStatus, BridgeLedgerState, CreateDepositRouteRequest,
    DepositRouteStatus, OperatorReviewRequest,
};
use super::service::{BridgePricingRuleConfig, BridgeService, BridgeServiceConfig};
use super::store::BridgeStateStore;
use super::{dispatch_request, parse_cli_options};
#[path = "access_observability_tests.rs"]
mod access_observability_tests;
#[path = "deposit_token_tests.rs"]
mod deposit_token_tests;
#[path = "tests_support.rs"]
mod tests_support;
use self::tests_support::{
    MockChainServer, MockChainState, MockChainTx, MockLetaiServer, assert_http_status_line,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn pricing_rules_example_env_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/newapi-bridge-service/pricing-rules.example.env")
}

fn example_pricing_rules_value() -> String {
    fs::read_to_string(pricing_rules_example_env_path())
        .expect("read pricing rules example env")
        .lines()
        .find_map(|line| {
            line.strip_prefix("OASIS7_NEWAPI_BRIDGE_PRICING_RULES=")
                .map(str::trim)
        })
        .expect("pricing rules env entry")
        .trim_matches('"')
        .to_string()
}

fn example_pricing_rules() -> Vec<BridgePricingRuleConfig> {
    example_pricing_rules_value()
        .split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .map(|entry| super::parse_pricing_rule(entry).expect("parse pricing rule from example env"))
        .collect()
}

#[test]
fn bind_user_persists_binding_and_project_binding() {
    let test_service = test_service("bind-reuse", 900);
    let created = test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-1".to_string(),
                external_user_name: Some("User One".to_string()),
                email: Some("user1@example.com".to_string()),
                metadata: Some(json!({"tier": "alpha"})),
                project_name: Some("user-one-project".to_string()),
                project_metadata: Some(json!({"source": "bind"})),
            },
            1_000,
        )
        .expect("create binding");
    assert!(!created.reused_existing_binding);
    assert_eq!(created.binding_status, BridgeBindingStatus::Active);
    assert_eq!(created.letai_external_user_id, "oasis7-user:user-1");
    assert_eq!(
        created.letai_external_project_id,
        "oasis7-project:bridge-user-000001"
    );

    let reused = test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-1".to_string(),
                external_user_name: Some("User One Updated".to_string()),
                email: None,
                metadata: None,
                project_name: Some("user-one-project-updated".to_string()),
                project_metadata: None,
            },
            2_000,
        )
        .expect("reuse binding");
    assert!(reused.reused_existing_binding);
    assert_eq!(reused.bridge_user_id, created.bridge_user_id);
    assert_eq!(reused.project_name, "user-one-project-updated");

    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.bindings.len(), 1);
    assert_eq!(snapshot.project_bindings.len(), 1);
    assert_eq!(
        snapshot.project_bindings[0].project_name,
        "user-one-project-updated"
    );
    let persisted = fs::read(test_service.state_path.as_path()).expect("read state file");
    let persisted_json: Value =
        serde_json::from_slice(persisted.as_slice()).expect("parse state file");
    assert_eq!(
        persisted_json
            .get("project_bindings")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn bind_user_rejects_conflicting_active_binding() {
    let test_service = test_service("bind-conflict", 900);
    test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-1".to_string(),
                external_user_name: None,
                email: None,
                metadata: None,
                project_name: None,
                project_metadata: None,
            },
            1_000,
        )
        .expect("initial binding");

    let err = test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-2".to_string(),
                external_user_name: None,
                email: None,
                metadata: None,
                project_name: None,
                project_metadata: None,
            },
            2_000,
        )
        .expect_err("conflict expected");
    assert_eq!(err.status_code, 409);
    assert_eq!(err.code, "binding_conflict");
}

#[test]
fn bind_user_eagerly_provisions_platform_user_project_and_token_when_letai_is_configured() {
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "bind-eager-letai",
        900,
        None,
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let created = bind_default_user(&test_service);
    assert_eq!(created.bridge_user_id, "bridge-user-000001");

    let snapshot = test_service.service.snapshot();
    assert_eq!(
        snapshot.bindings[0].platform_user_id.as_deref(),
        Some("platform-user-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].platform_project_id.as_deref(),
        Some("platform-project-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].token_key.as_deref(),
        Some("token-key-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].token_status.as_deref(),
        Some("1")
    );
}

#[test]
fn bind_user_retry_reuses_binding_and_retries_eager_provisioning_after_upsert_failure() {
    let letai_server = MockLetaiServer::spawn();
    let letai_base_url = letai_server.base_url.clone();
    letai_server.fail_first_user_upsert_requests(1);
    let test_service = test_service_with_endpoints(
        "bind-eager-letai-retry",
        900,
        None,
        Some(letai_base_url),
        1,
        None,
    );

    let first_err = test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-1".to_string(),
                external_user_name: Some("User One".to_string()),
                email: Some("user1@example.com".to_string()),
                metadata: None,
                project_name: Some("user-one-project".to_string()),
                project_metadata: None,
            },
            1_000,
        )
        .expect_err("first eager provision should fail");
    assert_eq!(first_err.code, "letai_response_not_success");
    let failed_snapshot = test_service.service.snapshot();
    assert!(failed_snapshot.bindings.is_empty());
    assert!(failed_snapshot.project_bindings.is_empty());

    let retried = bind_default_user(&test_service);
    assert!(!retried.reused_existing_binding);

    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.bindings.len(), 1);
    assert_eq!(
        snapshot.bindings[0].platform_user_id.as_deref(),
        Some("platform-user-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].platform_project_id.as_deref(),
        Some("platform-project-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].token_key.as_deref(),
        Some("token-key-000001")
    );
}

#[test]
fn reconcile_provisions_letai_user_project_token_and_marks_reconciled() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-happy",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        2,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-1".to_string(),
            action_id: 11,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let first = test_service
        .service
        .reconcile_once(6_000)
        .expect("first reconcile");
    assert_eq!(first.reconciled_credit_count, 0);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger.len(), 1);
    assert_eq!(
        snapshot.ledger[0].state,
        BridgeLedgerState::PendingConfirmations
    );

    chain_server.set_committed_height(11);
    let second = test_service
        .service
        .reconcile_once(7_000)
        .expect("second reconcile");
    assert_eq!(second.updated_deposit_count, 1);
    assert_eq!(second.reconciled_credit_count, 1);

    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::Reconciled);
    assert_eq!(snapshot.ledger[0].quota, Some(100_000));
    assert_eq!(
        snapshot.ledger[0].external_order_id.as_deref(),
        Some("letai-topup:bridge-deposit-000001")
    );
    assert_eq!(
        snapshot.bindings[0].platform_user_id.as_deref(),
        Some("platform-user-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].platform_project_id.as_deref(),
        Some("platform-project-000001")
    );
    assert_eq!(
        snapshot.project_bindings[0].token_key.as_deref(),
        Some("token-key-000001")
    );
    assert_eq!(letai_server.recorded_topup_requests().len(), 1);
}

#[test]
fn health_reports_last_reconcile_success_observability() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-observability-success",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    issue_default_route(&test_service);
    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: Vec::new(),
    });

    test_service
        .service
        .reconcile_once(6_000)
        .expect("reconcile succeeds");

    let health = test_service.service.health(7_000);
    let reconcile = health
        .last_reconcile
        .expect("health exposes last reconcile observability");
    assert_eq!(reconcile.started_at_unix_ms, Some(6_000));
    assert!(reconcile.finished_at_unix_ms.unwrap_or_default() >= 6_000);
    assert!(reconcile.duration_ms.is_some());
    assert_eq!(reconcile.ok, Some(true));
    assert_eq!(reconcile.error_code.as_deref(), None);
    assert_eq!(reconcile.error_message.as_deref(), None);
}

#[test]
fn health_reports_last_reconcile_failure_observability() {
    let test_service = test_service("reconcile-observability-failure", 900);
    issue_default_route(&test_service);

    let err = test_service
        .service
        .reconcile_once(6_000)
        .expect_err("missing chain config");
    assert_eq!(err.code, "chain_explorer_not_configured");

    let health = test_service.service.health(7_000);
    let reconcile = health
        .last_reconcile
        .expect("health exposes failed reconcile observability");
    assert_eq!(reconcile.started_at_unix_ms, Some(6_000));
    assert!(reconcile.finished_at_unix_ms.unwrap_or_default() >= 6_000);
    assert!(reconcile.duration_ms.is_some());
    assert_eq!(reconcile.ok, Some(false));
    assert_eq!(
        reconcile.error_code.as_deref(),
        Some("chain_explorer_not_configured")
    );
    assert!(
        !reconcile
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("--chain-base-url")
    );
    assert_eq!(
        reconcile.error_message.as_deref(),
        Some("reconcile failed with error code chain_explorer_not_configured")
    );
}

#[test]
fn reconcile_marks_underpay_for_manual_review() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-underpay",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-underpay".to_string(),
            action_id: 12,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 99,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let reconcile = test_service
        .service
        .reconcile_once(6_000)
        .expect("reconcile");
    assert_eq!(reconcile.manual_review_count, 1);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::ManualReview);
    assert_eq!(
        snapshot.ledger[0].review_reason.as_deref(),
        Some("underpay")
    );
    assert!(letai_server.recorded_topup_requests().is_empty());
}

#[test]
fn reconcile_retries_topup_with_stable_external_order_id() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    letai_server.fail_first_topup_requests(1);
    let test_service = test_service_with_endpoints(
        "reconcile-retry",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-retry".to_string(),
            action_id: 13,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let first = test_service
        .service
        .reconcile_once(6_000)
        .expect("first reconcile");
    assert_eq!(first.failed_credit_count, 1);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::Failed);
    assert_eq!(snapshot.ledger[0].credit_attempt_count, 1);

    let second = test_service
        .service
        .reconcile_once(7_000)
        .expect("second reconcile");
    assert_eq!(second.reconciled_credit_count, 1);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::Reconciled);
    let requests = letai_server.recorded_topup_requests();
    assert_eq!(requests.len(), 2);
    let first_key = requests[0]
        .get("external_order_id")
        .and_then(Value::as_str)
        .expect("first external order id");
    let second_key = requests[1]
        .get("external_order_id")
        .and_then(Value::as_str)
        .expect("second external order id");
    assert_eq!(first_key, second_key);
}

#[test]
fn reconcile_moves_to_manual_review_when_verification_logs_missing() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    letai_server.omit_logs_for_order("letai-topup:bridge-deposit-000001");
    let test_service = test_service_with_endpoints(
        "reconcile-verification-mismatch",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-verification".to_string(),
            action_id: 14,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let reconcile = test_service
        .service
        .reconcile_once(6_000)
        .expect("reconcile");
    assert_eq!(reconcile.reconciled_credit_count, 0);
    assert_eq!(reconcile.manual_review_count, 1);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::ManualReview);
    assert_eq!(
        snapshot.ledger[0].review_reason.as_deref(),
        Some("letai_topup_log_mismatch")
    );
}

#[test]
fn reconcile_moves_to_manual_review_when_project_binding_missing() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-project-binding-missing",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);
    test_service
        .service
        .store_mutate_test(|state| {
            state.project_bindings.clear();
            Ok(())
        })
        .expect("remove project binding");

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-project-binding-missing".to_string(),
            action_id: 16,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let reconcile = test_service
        .service
        .reconcile_once(6_000)
        .expect("reconcile");
    assert_eq!(reconcile.reconciled_credit_count, 0);
    assert_eq!(reconcile.manual_review_count, 1);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::ManualReview);
    assert_eq!(
        snapshot.ledger[0].review_reason.as_deref(),
        Some("project_binding_not_found")
    );
}

#[test]
fn reconcile_retries_resolved_rows_after_operator_review() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-resolved-retry",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);
    let project_binding = test_service.service.snapshot().project_bindings[0].clone();
    test_service
        .service
        .store_mutate_test(|state| {
            state.project_bindings.clear();
            Ok(())
        })
        .expect("remove project binding");

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-resolved-retry".to_string(),
            action_id: 17,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let first = test_service
        .service
        .reconcile_once(6_000)
        .expect("first reconcile");
    assert_eq!(first.reconciled_credit_count, 0);
    assert_eq!(first.manual_review_count, 1);
    let manual_review_id = test_service
        .service
        .snapshot()
        .ledger
        .iter()
        .find(|entry| entry.state == BridgeLedgerState::ManualReview)
        .map(|entry| entry.bridge_deposit_id.clone())
        .expect("manual review entry");

    test_service
        .service
        .store_mutate_test(|state| {
            state.project_bindings.push(project_binding);
            Ok(())
        })
        .expect("restore project binding");
    let review = test_service
        .service
        .apply_operator_review(
            manual_review_id.as_str(),
            OperatorReviewRequest {
                resolution: "mark_resolved".to_string(),
                operator_note: Some("binding restored".to_string()),
            },
            7_000,
        )
        .expect("mark resolved");
    assert_eq!(review.state, BridgeLedgerState::Resolved);

    let second = test_service
        .service
        .reconcile_once(8_000)
        .expect("second reconcile");
    assert_eq!(second.reconciled_credit_count, 1);
    assert_eq!(second.manual_review_count, 0);
    let snapshot = test_service.service.snapshot();
    assert_eq!(snapshot.ledger[0].state, BridgeLedgerState::Reconciled);
    assert_eq!(letai_server.recorded_topup_requests().len(), 1);
}

#[test]
fn operator_review_can_close_manual_review_row() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "operator-review",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![
            MockChainTx {
                tx_hash: "tx-a".to_string(),
                action_id: 21,
                from_account_id: "oc:pk:sender-1".to_string(),
                to_account_id: deposit_account_id.clone(),
                amount: 100,
                memo: Some("bridge:deposit:route-000001".to_string()),
                submitted_at_unix_ms: 5_000,
                updated_at_unix_ms: 5_100,
                block_height: Some(10),
            },
            MockChainTx {
                tx_hash: "tx-b".to_string(),
                action_id: 22,
                from_account_id: "oc:pk:sender-1".to_string(),
                to_account_id: deposit_account_id,
                amount: 100,
                memo: Some("bridge:deposit:route-000001".to_string()),
                submitted_at_unix_ms: 5_200,
                updated_at_unix_ms: 5_300,
                block_height: Some(10),
            },
        ],
    });

    test_service
        .service
        .reconcile_once(6_000)
        .expect("reconcile");
    let manual_review_id = test_service
        .service
        .snapshot()
        .ledger
        .iter()
        .find(|entry| entry.state == BridgeLedgerState::ManualReview)
        .map(|entry| entry.bridge_deposit_id.clone())
        .expect("manual review entry");
    let response = test_service
        .service
        .apply_operator_review(
            manual_review_id.as_str(),
            OperatorReviewRequest {
                resolution: "close".to_string(),
                operator_note: Some("handled offline".to_string()),
            },
            7_000,
        )
        .expect("apply review");
    assert_eq!(response.state, BridgeLedgerState::Closed);
}

#[test]
fn dispatch_request_handles_bind_and_route_http_contract() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "http-contract",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let bind_response = dispatch_request(
        &test_service.service,
        HttpRequest {
            method: "POST".to_string(),
            path: "/v1/bridge/bind".to_string(),
            body: serde_json::to_vec(&json!({
                "newapi_user_ref": "user-1",
                "oasis_sender_account_id": "oc:pk:sender-1",
                "external_user_name": "User One"
            }))
            .expect("encode bind request"),
        },
    )
    .expect("dispatch bind");
    assert_eq!(bind_response.status_code, 200);
    let bind_json: Value =
        serde_json::from_slice(bind_response.body.as_slice()).expect("parse bind response");
    let bridge_user_id = bind_json
        .get("bridge_user_id")
        .and_then(Value::as_str)
        .expect("bridge_user_id")
        .to_string();

    let route_response = dispatch_request(
        &test_service.service,
        HttpRequest {
            method: "POST".to_string(),
            path: "/v1/bridge/deposit-route".to_string(),
            body: serde_json::to_vec(&json!({
                "bridge_user_id": bridge_user_id,
                "pricing_version": "pv-1"
            }))
            .expect("encode route request"),
        },
    )
    .expect("dispatch route");
    assert_eq!(route_response.status_code, 200);
    let route_json: Value =
        serde_json::from_slice(route_response.body.as_slice()).expect("parse route response");
    assert_eq!(
        route_json.get("route_status").and_then(Value::as_str),
        Some("issued")
    );
    assert_eq!(
        route_json.get("deposit_token").and_then(Value::as_str),
        Some("bridge:deposit:route-000001")
    );

    let reconcile_response = dispatch_request(
        &test_service.service,
        HttpRequest {
            method: "POST".to_string(),
            path: "/v1/bridge/reconcile".to_string(),
            body: Vec::new(),
        },
    )
    .expect("dispatch reconcile");
    assert_eq!(reconcile_response.status_code, 200);
}

#[test]
fn dispatch_request_returns_405_for_known_path_with_wrong_method() {
    let test_service = test_service("http-405", 900);
    let response = dispatch_request(
        &test_service.service,
        HttpRequest {
            method: "GET".to_string(),
            path: "/v1/bridge/bind".to_string(),
            body: Vec::new(),
        },
    )
    .expect("dispatch 405");
    assert_eq!(response.status_code, 405);
    let payload: Value = serde_json::from_slice(response.body.as_slice()).expect("parse 405");
    assert_eq!(
        payload
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str),
        Some("method_not_allowed")
    );
}

#[test]
fn store_reloads_persisted_bindings_routes_and_projects() {
    let test_service = test_service("reload-state", 900);
    let binding = bind_default_user(&test_service);
    test_service
        .service
        .create_deposit_route(
            CreateDepositRouteRequest {
                bridge_user_id: binding.bridge_user_id,
                pricing_version: Some("pv-1".to_string()),
                topup_plan_id: None,
            },
            2_000,
        )
        .expect("route");

    let expected_snapshot = test_service.service.snapshot();
    let persisted = fs::read(test_service.state_path.as_path()).expect("read state file");
    assert_eq!(
        persisted,
        serde_json::to_vec(&expected_snapshot).expect("serialize compact expected state")
    );

    let reloaded_store =
        BridgeStateStore::new(test_service.state_path.clone()).expect("reload bridge state store");
    let snapshot = reloaded_store.snapshot();
    assert_eq!(snapshot, expected_snapshot);
    assert_eq!(snapshot.bindings.len(), 1);
    assert_eq!(snapshot.project_bindings.len(), 1);
    assert_eq!(snapshot.routes.len(), 1);
    assert_eq!(snapshot.ledger.len(), 0);
}

#[test]
fn parse_cli_options_rejects_zero_route_ttl() {
    let err = parse_cli_options(vec!["--route-ttl-seconds".to_string(), "0".to_string()])
        .expect_err("ttl validation");
    assert!(err.contains("greater than 0"));
}

#[test]
fn parse_cli_options_rejects_route_ttl_that_overflows_milliseconds() {
    let err = parse_cli_options(vec![
        "--route-ttl-seconds".to_string(),
        u64::MAX.to_string(),
    ])
    .expect_err("ttl overflow validation");
    assert!(err.contains("too large") || err.contains("supported millisecond range"));
}

#[test]
fn create_deposit_route_rejects_ttl_overflow_from_service_config() {
    let test_service = test_service("ttl-overflow", u64::MAX);
    let binding = bind_default_user(&test_service);
    let err = test_service
        .service
        .create_deposit_route(
            CreateDepositRouteRequest {
                bridge_user_id: binding.bridge_user_id,
                pricing_version: Some("pv-1".to_string()),
                topup_plan_id: None,
            },
            2_000,
        )
        .expect_err("ttl overflow");
    assert_eq!(err.status_code, 500);
    assert_eq!(err.code, "internal_error");
}

#[test]
fn parse_cli_options_accepts_bridge_automation_flags() {
    let options = parse_cli_options(vec![
        "--chain-base-url".to_string(),
        "http://127.0.0.1:5121".to_string(),
        "--pricing-rule".to_string(),
        example_pricing_rules_value(),
        "--letai-base-url".to_string(),
        "https://api.letai.run".to_string(),
        "--letai-platform-key".to_string(),
        "platform-key".to_string(),
        "--letai-parent-channel-id".to_string(),
        "parent-channel".to_string(),
        "--reconcile-interval-seconds".to_string(),
        "15".to_string(),
    ])
    .expect("parse options");
    assert_eq!(
        options.chain_base_url.as_deref(),
        Some("http://127.0.0.1:5121")
    );
    assert_eq!(options.pricing_rules.len(), 1);
    assert_eq!(
        options.letai_base_url.as_deref(),
        Some("https://api.letai.run")
    );
    assert_eq!(options.reconcile_interval_seconds, 15);
}

#[test]
fn parse_cli_options_accepts_letai_platform_key_env() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct EnvRestore {
        key: &'static str,
        previous: Option<String>,
    }
    impl Drop for EnvRestore {
        fn drop(&mut self) {
            // SAFETY: serialized by ENV_LOCK across tests in this binary.
            unsafe {
                match &self.previous {
                    Some(value) => env::set_var(self.key, value),
                    None => env::remove_var(self.key),
                }
            }
        }
    }

    const ENV_NAME: &str = "OASIS7_TEST_NEWAPI_BRIDGE_PLATFORM_KEY_ENV";
    let _lock = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let _restore = EnvRestore {
        key: ENV_NAME,
        previous: env::var(ENV_NAME).ok(),
    };
    // SAFETY: serialized by ENV_LOCK across tests in this binary.
    unsafe {
        env::set_var(ENV_NAME, "platform-key-from-env");
    }
    let options = parse_cli_options(vec![
        "--letai-platform-key-env".to_string(),
        ENV_NAME.to_string(),
    ])
    .expect("parse env platform key");
    assert_eq!(
        options.letai_platform_key.as_deref(),
        Some("platform-key-from-env")
    );
}

#[test]
fn reconcile_requires_chain_base_url_configuration() {
    let test_service = test_service("chain-config-missing", 900);
    issue_default_route(&test_service);

    let err = test_service
        .service
        .reconcile_once(6_000)
        .expect_err("missing chain config");
    assert_eq!(err.status_code, 503);
    assert_eq!(err.code, "chain_explorer_not_configured");
    assert!(err.message.contains("--chain-base-url"));
}

#[test]
fn reconcile_requires_letai_configuration() {
    let chain_server = MockChainServer::spawn();
    let test_service = test_service_with_endpoints(
        "letai-config-missing",
        900,
        Some(chain_server.base_url.clone()),
        None,
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);
    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-letai-config".to_string(),
            action_id: 34,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let err = test_service
        .service
        .reconcile_once(6_000)
        .expect_err("missing letai config");
    assert_eq!(err.status_code, 503);
    assert_eq!(err.code, "letai_openapi_not_configured");
    assert!(err.message.contains("--letai-base-url"));
}

#[test]
fn write_http_response_uses_upstream_status_texts() {
    assert_http_status_line(502, "HTTP/1.1 502 Bad Gateway\r\n");
    assert_http_status_line(503, "HTTP/1.1 503 Service Unavailable\r\n");
}

struct TestBridgeService {
    service: BridgeService,
    state_path: PathBuf,
}

fn test_service(name: &str, route_ttl_seconds: u64) -> TestBridgeService {
    test_service_with_endpoints(name, route_ttl_seconds, None, None, 1, None)
}

fn test_service_with_endpoints(
    name: &str,
    route_ttl_seconds: u64,
    chain_base_url: Option<String>,
    letai_base_url: Option<String>,
    chain_confirmations_required: u64,
    state_path: Option<PathBuf>,
) -> TestBridgeService {
    let state_path = state_path.unwrap_or_else(|| temp_state_path(name));
    let store =
        Arc::new(BridgeStateStore::new(state_path.clone()).expect("create bridge state store"));
    TestBridgeService {
        service: BridgeService::new(
            store,
            BridgeServiceConfig {
                route_ttl_seconds,
                deposit_account_prefix: "oc:bridge:".to_string(),
                chain_base_url,
                chain_timeout_ms: 2_000,
                chain_confirmations_required,
                pricing_rules: example_pricing_rules(),
                letai_base_url,
                letai_platform_key: Some("platform-key".to_string()),
                letai_parent_channel_id: Some("parent-channel".to_string()),
                letai_timeout_ms: 2_000,
                max_credit_attempts: 3,
            },
        ),
        state_path,
    }
}

fn bind_default_user(test_service: &TestBridgeService) -> super::model::BindBridgeUserResponse {
    test_service
        .service
        .bind_user(
            BindBridgeUserRequest {
                newapi_user_ref: "user-1".to_string(),
                oasis_sender_account_id: "oc:pk:sender-1".to_string(),
                external_user_name: Some("User One".to_string()),
                email: Some("user1@example.com".to_string()),
                metadata: Some(json!({"tier": "alpha"})),
                project_name: Some("user-one-project".to_string()),
                project_metadata: Some(json!({"owner": "user-1"})),
            },
            1_000,
        )
        .expect("binding")
}

#[test]
fn reconcile_recovers_inflight_rows_after_restart() {
    let chain_server = MockChainServer::spawn();
    let letai_server = MockLetaiServer::spawn();
    let test_service = test_service_with_endpoints(
        "reconcile-restart",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        None,
    );
    let deposit_account_id = issue_default_route(&test_service);

    chain_server.set_state(MockChainState {
        committed_height: 10,
        txs: vec![MockChainTx {
            tx_hash: "tx-restart".to_string(),
            action_id: 15,
            from_account_id: "oc:pk:sender-1".to_string(),
            to_account_id: deposit_account_id,
            amount: 100,
            memo: Some("bridge:deposit:route-000001".to_string()),
            submitted_at_unix_ms: 5_000,
            updated_at_unix_ms: 5_100,
            block_height: Some(10),
        }],
    });

    let first = test_service
        .service
        .reconcile_once(6_000)
        .expect("first reconcile");
    assert_eq!(first.reconciled_credit_count, 1);
    assert_eq!(letai_server.recorded_topup_requests().len(), 1);

    test_service
        .service
        .store_mutate_test(|state| {
            state.ledger[0].state = BridgeLedgerState::Credited;
            state.ledger[0].updated_at_unix_ms = 6_500;
            Ok(())
        })
        .expect("reset ledger to credited");

    let restarted = test_service_with_endpoints(
        "reconcile-restart",
        900,
        Some(chain_server.base_url.clone()),
        Some(letai_server.base_url.clone()),
        1,
        Some(test_service.state_path.clone()),
    );

    let second = restarted
        .service
        .reconcile_once(7_000)
        .expect("second reconcile");
    assert_eq!(second.reconciled_credit_count, 1);
    assert_eq!(second.manual_review_count, 0);
    assert_eq!(
        restarted.service.snapshot().ledger[0].state,
        BridgeLedgerState::Reconciled
    );
    assert_eq!(letai_server.recorded_topup_requests().len(), 1);
}

#[test]
fn bind_response_does_not_expose_platform_credentials() {
    let test_service = test_service("bind-redaction", 900);
    let response = dispatch_request(
        &test_service.service,
        HttpRequest {
            method: "POST".to_string(),
            path: "/v1/bridge/bind".to_string(),
            body: serde_json::to_vec(&json!({
                "newapi_user_ref": "user-1",
                "oasis_sender_account_id": "oc:pk:sender-1",
                "external_user_name": "User One"
            }))
            .expect("encode bind request"),
        },
    )
    .expect("dispatch bind");
    assert_eq!(response.status_code, 200);
    let bind_json: Value =
        serde_json::from_slice(response.body.as_slice()).expect("parse bind response");
    assert!(bind_json.get("platform_user_id").is_none());
    assert!(bind_json.get("platform_project_id").is_none());
    assert!(bind_json.get("token_key").is_none());
}

fn issue_default_route(test_service: &TestBridgeService) -> String {
    let binding = bind_default_user(test_service);
    test_service
        .service
        .create_deposit_route(
            CreateDepositRouteRequest {
                bridge_user_id: binding.bridge_user_id,
                pricing_version: Some("pv-1".to_string()),
                topup_plan_id: None,
            },
            2_000,
        )
        .expect("route")
        .deposit_account_id
}

fn temp_state_path(name: &str) -> PathBuf {
    let nonce = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = format!(
        "oasis7-newapi-bridge-{name}-{}-{}.json",
        process::id(),
        nonce
    );
    let path = env::temp_dir().join(file_name);
    let _ = fs::remove_file(path.as_path());
    path
}
