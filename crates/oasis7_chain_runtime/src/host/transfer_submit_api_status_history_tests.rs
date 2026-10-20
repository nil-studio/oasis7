use super::*;
use crate::transfer_submit_api::transfer_submit_api_support::with_transfer_tracker;

fn tracked_transfer_for_history(
    action_id: u64,
    submitted_at_unix_ms: i64,
    status: TransferLifecycleStatus,
) -> super::super::TrackedTransfer {
    super::super::TrackedTransfer {
        action_id,
        from_account_id: "player:a".to_string(),
        to_account_id: format!("player:{action_id}"),
        amount: action_id,
        nonce: action_id,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
        status,
        submitted_at_unix_ms,
        updated_at_unix_ms: submitted_at_unix_ms,
        error_code: None,
        error: None,
    }
}

#[test]
fn transfer_history_query_limits_page_without_losing_total_or_order() {
    let _guard = lock_transfer_test_state();
    with_transfer_tracker(|tracker| {
        for (action_id, submitted_at_unix_ms, status) in [
            (31, 1_000, TransferLifecycleStatus::Confirmed),
            (32, 3_000, TransferLifecycleStatus::Pending),
            (33, 2_000, TransferLifecycleStatus::Confirmed),
            (34, 3_000, TransferLifecycleStatus::Confirmed),
            (35, 500, TransferLifecycleStatus::Confirmed),
        ] {
            tracker.by_action_id.insert(
                action_id,
                tracked_transfer_for_history(action_id, submitted_at_unix_ms, status),
            );
        }
        tracker.action_order.extend([31, 32, 33, 34, 35]);

        let (total, items) = tracker.query_history(None, None, None, 3);

        assert_eq!(total, 5);
        assert_eq!(
            items.iter().map(|item| item.action_id).collect::<Vec<_>>(),
            vec![34, 32, 33]
        );

        let (confirmed_total, confirmed_items) =
            tracker.query_history(None, Some(TransferLifecycleStatus::Confirmed), None, 2);
        assert_eq!(confirmed_total, 4);
        assert_eq!(
            confirmed_items
                .iter()
                .map(|item| item.action_id)
                .collect::<Vec<_>>(),
            vec![34, 33]
        );
    });
}

#[test]
fn transfer_status_and_history_endpoint_report_confirmed_record() {
    let _guard = lock_transfer_test_state();
    let config = NodeConfig::new(
        "node-transfer-query-ok",
        "world-transfer-query-ok",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick interval");
    let mut node_runtime = NodeRuntime::new(config).with_execution_hook(NoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let runtime = Arc::new(Mutex::new(node_runtime));

    let (mut submit_server, mut submit_client) = tcp_stream_pair();
    let mut submit_request = build_signed_transfer_request(21, 22, 3, 8);
    let (_, submit_private_key) = transfer_test_signer(21);
    submit_request.asset_id = Some("main_token".to_string());
    submit_request.memo = Some("confirmed-history".to_string());
    submit_request.chain_id = Some("oasis7-main".to_string());
    submit_request.network_id = Some("testnet".to_string());
    submit_request.tx_version = Some(2);
    submit_request.tx_type = Some("asset_transfer".to_string());
    submit_request.valid_until_unix_ms = Some(1_900_000_123_456);
    submit_request.max_fee = Some(42);
    submit_request.fee_asset_id = Some("main_token".to_string());
    submit_request.application_payload_hash = Some("sha256:confirmed-history".to_string());
    submit_request.client_request_id = Some("confirmed-history-client".to_string());
    resign_transfer_request(&mut submit_request, submit_private_key.as_str());
    let submit_body = serde_json::to_string(&submit_request).expect("serialize request");
    let submit_http = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        submit_body.len(),
        submit_body
    );
    maybe_handle_transfer_submit_request(
        &mut submit_server,
        submit_http.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-query-ok",
        "world-transfer-query-ok",
        Path::new("."),
    )
    .expect("submit should be handled");
    drop(submit_server);

    let mut submit_response_bytes = Vec::new();
    submit_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    submit_client
        .read_to_end(&mut submit_response_bytes)
        .expect("read submit response");
    let (_, submit_response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&submit_response_bytes);
    assert_eq!(
        submit_response.lifecycle_status,
        Some(TransferLifecycleStatus::Accepted)
    );
    let action_id = submit_response.action_id.expect("action_id");
    with_transfer_tracker(|tracker| {
        tracker.by_action_id.clear();
        tracker.action_order.clear();
    });

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut observed_confirmed = false;
    while Instant::now() < deadline {
        let (mut status_server, mut status_client) = tcp_stream_pair();
        let status_http = format!(
            "GET /v1/chain/transfer/status?action_id={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
            action_id
        );
        maybe_handle_transfer_submit_request(
            &mut status_server,
            status_http.as_bytes(),
            &runtime,
            "GET",
            "/v1/chain/transfer/status",
            "node-transfer-query-ok",
            "world-transfer-query-ok",
            Path::new("."),
        )
        .expect("status should be handled");
        drop(status_server);

        status_client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        let mut status_response_bytes = Vec::new();
        status_client
            .read_to_end(&mut status_response_bytes)
            .expect("read status response");
        let (_, status_response): (u16, ChainTransferStatusResponse) =
            decode_http_json_response(&status_response_bytes);
        let Some(status) = status_response.status else {
            std::thread::sleep(Duration::from_millis(80));
            continue;
        };
        if status.status == TransferLifecycleStatus::Confirmed {
            assert_eq!(status.asset_id.as_deref(), Some("main_token"));
            assert_eq!(status.memo.as_deref(), Some("confirmed-history"));
            assert_eq!(status.chain_id.as_deref(), Some("oasis7-main"));
            assert_eq!(status.network_id.as_deref(), Some("testnet"));
            assert_eq!(status.tx_version, Some(2));
            assert_eq!(status.tx_type.as_deref(), Some("asset_transfer"));
            assert_eq!(status.valid_until_unix_ms, Some(1_900_000_123_456));
            assert_eq!(status.max_fee, Some(42));
            assert_eq!(status.fee_asset_id.as_deref(), Some("main_token"));
            assert_eq!(
                status.application_payload_hash.as_deref(),
                Some("sha256:confirmed-history")
            );
            assert_eq!(
                status.client_request_id.as_deref(),
                Some("confirmed-history-client")
            );
            observed_confirmed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    assert!(
        observed_confirmed,
        "status should eventually become confirmed"
    );

    let (mut history_server, mut history_client) = tcp_stream_pair();
    let history_http = format!(
        "GET /v1/chain/transfer/history?action_id={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
        action_id
    );
    maybe_handle_transfer_submit_request(
        &mut history_server,
        history_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/transfer/history",
        "node-transfer-query-ok",
        "world-transfer-query-ok",
        Path::new("."),
    )
    .expect("history should be handled");
    drop(history_server);

    history_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut history_response_bytes = Vec::new();
    history_client
        .read_to_end(&mut history_response_bytes)
        .expect("read history response");
    let (_, history_response): (u16, ChainTransferHistoryResponse) =
        decode_http_json_response(&history_response_bytes);
    assert!(history_response.ok);
    assert_eq!(history_response.total, 1);
    assert_eq!(history_response.items[0].action_id, action_id);
    assert_eq!(
        history_response.items[0].asset_id.as_deref(),
        Some("main_token")
    );
    assert_eq!(
        history_response.items[0].memo.as_deref(),
        Some("confirmed-history")
    );
    assert_eq!(
        history_response.items[0].chain_id.as_deref(),
        Some("oasis7-main")
    );
    assert_eq!(
        history_response.items[0].network_id.as_deref(),
        Some("testnet")
    );
    assert_eq!(history_response.items[0].tx_version, Some(2));
    assert_eq!(
        history_response.items[0].tx_type.as_deref(),
        Some("asset_transfer")
    );
    assert_eq!(
        history_response.items[0].valid_until_unix_ms,
        Some(1_900_000_123_456)
    );
    assert_eq!(history_response.items[0].max_fee, Some(42));
    assert_eq!(
        history_response.items[0].fee_asset_id.as_deref(),
        Some("main_token")
    );
    assert_eq!(
        history_response.items[0]
            .application_payload_hash
            .as_deref(),
        Some("sha256:confirmed-history")
    );
    assert_eq!(
        history_response.items[0].client_request_id.as_deref(),
        Some("confirmed-history-client")
    );

    runtime
        .lock()
        .expect("lock runtime for stop")
        .stop()
        .expect("stop node runtime");
}

#[test]
fn explorer_overview_and_transaction_queries_return_expected_payloads() {
    let _guard = lock_transfer_test_state();
    let config = NodeConfig::new(
        "node-transfer-explorer-ok",
        "world-transfer-explorer-ok",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick interval");
    let mut node_runtime = NodeRuntime::new(config).with_execution_hook(NoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let runtime = Arc::new(Mutex::new(node_runtime));

    let (mut submit_server, mut submit_client) = tcp_stream_pair();
    let submit_request = build_signed_transfer_request(31, 32, 4, 9);
    let submit_body = serde_json::to_string(&submit_request).expect("serialize request");
    let submit_http = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        submit_body.len(),
        submit_body
    );
    maybe_handle_transfer_submit_request(
        &mut submit_server,
        submit_http.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-explorer-ok",
        "world-transfer-explorer-ok",
        Path::new("."),
    )
    .expect("submit should be handled");
    drop(submit_server);

    let mut submit_response_bytes = Vec::new();
    submit_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    submit_client
        .read_to_end(&mut submit_response_bytes)
        .expect("read submit response");
    let (_, submit_response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&submit_response_bytes);
    let action_id = submit_response.action_id.expect("action_id");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut confirmed = false;
    while Instant::now() < deadline {
        let (mut status_server, mut status_client) = tcp_stream_pair();
        let status_http = format!(
            "GET /v1/chain/transfer/status?action_id={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
            action_id
        );
        maybe_handle_transfer_submit_request(
            &mut status_server,
            status_http.as_bytes(),
            &runtime,
            "GET",
            "/v1/chain/transfer/status",
            "node-transfer-explorer-ok",
            "world-transfer-explorer-ok",
            Path::new("."),
        )
        .expect("status should be handled");
        drop(status_server);

        status_client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        let mut status_response_bytes = Vec::new();
        status_client
            .read_to_end(&mut status_response_bytes)
            .expect("read status response");
        let (_, status_response): (u16, ChainTransferStatusResponse) =
            decode_http_json_response(&status_response_bytes);
        if status_response
            .status
            .as_ref()
            .is_some_and(|record| record.status == TransferLifecycleStatus::Confirmed)
        {
            confirmed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    assert!(confirmed, "status should eventually become confirmed");

    let (mut overview_server, mut overview_client) = tcp_stream_pair();
    let overview_http = "GET /v1/chain/explorer/overview HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut overview_server,
        overview_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/overview",
        "node-transfer-explorer-ok",
        "world-transfer-explorer-ok",
        Path::new("."),
    )
    .expect("overview should be handled");
    drop(overview_server);

    overview_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut overview_response_bytes = Vec::new();
    overview_client
        .read_to_end(&mut overview_response_bytes)
        .expect("read overview response");
    let (_, overview): (u16, ChainExplorerOverviewResponse) =
        decode_http_json_response(&overview_response_bytes);
    assert!(overview.ok);
    assert_eq!(overview.node_id, "node-transfer-explorer-ok");
    assert_eq!(overview.world_id, "world-transfer-explorer-ok");
    assert!(overview.transfer_total >= 1);
    assert!(overview.transfer_confirmed >= 1);
    assert!(overview.latest_height >= 1);

    let (mut txs_server, mut txs_client) = tcp_stream_pair();
    let txs_http = "GET /v1/chain/explorer/transactions?status=confirmed&limit=10 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut txs_server,
        txs_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/transactions",
        "node-transfer-explorer-ok",
        "world-transfer-explorer-ok",
        Path::new("."),
    )
    .expect("transactions should be handled");
    drop(txs_server);

    txs_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut txs_response_bytes = Vec::new();
    txs_client
        .read_to_end(&mut txs_response_bytes)
        .expect("read transactions response");
    let (_, txs): (u16, ChainTransferHistoryResponse) =
        decode_http_json_response(&txs_response_bytes);
    assert!(txs.ok);
    assert_eq!(txs.status_filter, Some(TransferLifecycleStatus::Confirmed));
    assert!(txs.items.iter().any(|item| item.action_id == action_id));

    let (mut tx_server, mut tx_client) = tcp_stream_pair();
    let tx_http = format!(
        "GET /v1/chain/explorer/transaction?action_id={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
        action_id
    );
    maybe_handle_transfer_submit_request(
        &mut tx_server,
        tx_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/transaction",
        "node-transfer-explorer-ok",
        "world-transfer-explorer-ok",
        Path::new("."),
    )
    .expect("transaction detail should be handled");
    drop(tx_server);

    tx_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut tx_response_bytes = Vec::new();
    tx_client
        .read_to_end(&mut tx_response_bytes)
        .expect("read transaction detail response");
    let (_, tx_detail): (u16, ChainTransferStatusResponse) =
        decode_http_json_response(&tx_response_bytes);
    assert!(tx_detail.ok);
    assert_eq!(tx_detail.action_id, action_id);
    assert_eq!(
        tx_detail.status.as_ref().map(|item| item.status),
        Some(TransferLifecycleStatus::Confirmed)
    );

    runtime
        .lock()
        .expect("lock runtime for stop")
        .stop()
        .expect("stop node runtime");
}
