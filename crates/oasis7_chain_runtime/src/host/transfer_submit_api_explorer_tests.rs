use super::*;

#[test]
fn explorer_transactions_reject_invalid_status_filter() {
    let _guard = lock_transfer_test_state();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-explorer-filter",
            "world-transfer-explorer-filter",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request =
        "GET /v1/chain/explorer/transactions?status=bad HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/transactions",
        "node-transfer-explorer-filter",
        "world-transfer-explorer-filter",
        Path::new("."),
    )
    .expect("request should be handled");
    assert!(handled);
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read response");
    let (status, response): (u16, ChainTransferHistoryResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));
}

#[test]
fn explorer_p0_blocks_txs_tx_search_queries_return_expected_payloads() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("explorer_p0_queries");
    seed_world_for_explorer_p1(temp_dir.as_path());

    let config = NodeConfig::new(
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        NodeRole::Sequencer,
    )
    .expect("node config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick interval");
    let mut node_runtime = NodeRuntime::new(config).with_execution_hook(NoopExecutionHook);
    node_runtime.start().expect("start node runtime");
    let runtime = Arc::new(Mutex::new(node_runtime));

    let (mut submit_server, mut submit_client) = tcp_stream_pair();
    let (public_key, private_key) = transfer_test_signer(41);
    let (to_public_key, _) = transfer_test_signer(42);
    let mut submit_request = build_signed_transfer_request_with_accounts(
        main_token_account_id_from_node_public_key(public_key.as_str()),
        main_token_account_id_from_node_public_key(to_public_key.as_str()),
        5,
        10,
        public_key.clone(),
        private_key.clone(),
    );
    submit_request.asset_id = Some("main_token".to_string());
    submit_request.memo = Some("bridge:deposit:alpha".to_string());
    submit_request.chain_id = Some("oasis7-main".to_string());
    submit_request.network_id = Some("prod".to_string());
    submit_request.tx_version = Some(2);
    submit_request.tx_type = Some("asset_transfer".to_string());
    submit_request.valid_until_unix_ms = Some(crate::now_unix_ms() + 60_000);
    submit_request.max_fee = Some(42);
    submit_request.fee_asset_id = Some("main_token".to_string());
    submit_request.application_payload_hash = Some("sha256:payload-alpha".to_string());
    submit_request.client_request_id = Some("client-alpha".to_string());
    let action = Action::TransferMainToken {
        from_account_id: submit_request.from_account_id.clone(),
        to_account_id: submit_request.to_account_id.clone(),
        amount: submit_request.amount,
        nonce: submit_request.nonce,
        asset_id: submit_request.asset_id.clone(),
        memo: submit_request.memo.clone(),
        chain_id: submit_request.chain_id.clone(),
        network_id: submit_request.network_id.clone(),
        tx_version: submit_request.tx_version,
        tx_type: submit_request.tx_type.clone(),
        valid_until_unix_ms: submit_request.valid_until_unix_ms,
        max_fee: submit_request.max_fee,
        fee_asset_id: submit_request.fee_asset_id.clone(),
        application_payload_hash: submit_request.application_payload_hash.clone(),
        client_request_id: submit_request.client_request_id.clone(),
    };
    submit_request.signature = sign_main_token_runtime_action_auth(
        &action,
        submit_request.from_account_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign p0 explorer v2 request")
    .signature
    .expect("p0 explorer v2 signature");
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
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        temp_dir.as_path(),
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
    assert!(
        submit_response.ok,
        "submit failed: code={:?} error={:?}",
        submit_response.error_code, submit_response.error
    );
    let action_id = submit_response.action_id.expect("action_id");

    let deadline = Instant::now() + Duration::from_secs(3);
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
            "node-transfer-explorer-p0-ok",
            "world-transfer-explorer-p0-ok",
            temp_dir.as_path(),
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
            .is_some_and(|item| item.status == TransferLifecycleStatus::Confirmed)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(80));
    }

    let (mut blocks_server, mut blocks_client) = tcp_stream_pair();
    let blocks_http =
        "GET /v1/chain/explorer/blocks?limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut blocks_server,
        blocks_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/blocks",
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        temp_dir.as_path(),
    )
    .expect("blocks should be handled");
    drop(blocks_server);
    blocks_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut blocks_response_bytes = Vec::new();
    blocks_client
        .read_to_end(&mut blocks_response_bytes)
        .expect("read blocks response");
    let (_, blocks): (u16, ExplorerBlocksResponse) =
        decode_http_json_response(&blocks_response_bytes);
    assert!(blocks.ok);
    assert!(blocks.total >= 1);
    assert!(!blocks.items.is_empty());
    let tx_hash = blocks
        .items
        .iter()
        .find_map(|item| item.tx_hashes.first().cloned())
        .expect("block tx hash");

    let (mut txs_server, mut txs_client) = tcp_stream_pair();
    let txs_http = "GET /v1/chain/explorer/txs?status=confirmed&limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut txs_server,
        txs_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/txs",
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        temp_dir.as_path(),
    )
    .expect("txs should be handled");
    drop(txs_server);
    txs_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut txs_response_bytes = Vec::new();
    txs_client
        .read_to_end(&mut txs_response_bytes)
        .expect("read txs response");
    let (_, txs): (u16, ExplorerTxsResponse) = decode_http_json_response(&txs_response_bytes);
    assert!(txs.ok);
    assert!(txs.items.iter().any(|item| item.tx_hash == tx_hash));
    let tx_list_item = txs
        .items
        .iter()
        .find(|item| item.tx_hash == tx_hash)
        .expect("tx list item");
    assert_eq!(tx_list_item.asset_id.as_deref(), Some("main_token"));
    assert_eq!(tx_list_item.memo.as_deref(), Some("bridge:deposit:alpha"));
    assert_eq!(tx_list_item.chain_id.as_deref(), Some("oasis7-main"));
    assert_eq!(tx_list_item.network_id.as_deref(), Some("prod"));
    assert_eq!(tx_list_item.tx_version, Some(2));
    assert_eq!(tx_list_item.tx_type.as_deref(), Some("asset_transfer"));
    assert_eq!(
        tx_list_item.valid_until_unix_ms,
        submit_request.valid_until_unix_ms
    );
    assert_eq!(tx_list_item.max_fee, Some(42));
    assert_eq!(tx_list_item.fee_asset_id.as_deref(), Some("main_token"));
    assert_eq!(
        tx_list_item.application_payload_hash.as_deref(),
        Some("sha256:payload-alpha")
    );
    assert_eq!(
        tx_list_item.client_request_id.as_deref(),
        Some("client-alpha")
    );

    let (mut tx_server, mut tx_client) = tcp_stream_pair();
    let tx_http = format!(
        "GET /v1/chain/explorer/tx?tx_hash={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
        tx_hash
    );
    maybe_handle_transfer_submit_request(
        &mut tx_server,
        tx_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/tx",
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        temp_dir.as_path(),
    )
    .expect("tx should be handled");
    drop(tx_server);
    tx_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut tx_response_bytes = Vec::new();
    tx_client
        .read_to_end(&mut tx_response_bytes)
        .expect("read tx response");
    let (_, tx): (u16, ExplorerTxResponse) = decode_http_json_response(&tx_response_bytes);
    assert!(tx.ok);
    assert_eq!(
        tx.tx.as_ref().map(|item| item.status),
        Some(TransferLifecycleStatus::Confirmed)
    );
    let tx_detail = tx.tx.expect("tx detail item");
    assert_eq!(tx_detail.asset_id.as_deref(), Some("main_token"));
    assert_eq!(tx_detail.memo.as_deref(), Some("bridge:deposit:alpha"));
    assert_eq!(tx_detail.chain_id.as_deref(), Some("oasis7-main"));
    assert_eq!(tx_detail.network_id.as_deref(), Some("prod"));
    assert_eq!(tx_detail.tx_version, Some(2));
    assert_eq!(tx_detail.tx_type.as_deref(), Some("asset_transfer"));
    assert_eq!(
        tx_detail.valid_until_unix_ms,
        submit_request.valid_until_unix_ms
    );
    assert_eq!(tx_detail.max_fee, Some(42));
    assert_eq!(tx_detail.fee_asset_id.as_deref(), Some("main_token"));
    assert_eq!(
        tx_detail.application_payload_hash.as_deref(),
        Some("sha256:payload-alpha")
    );
    assert_eq!(tx_detail.client_request_id.as_deref(), Some("client-alpha"));

    let (mut search_server, mut search_client) = tcp_stream_pair();
    let search_http = format!(
        "GET /v1/chain/explorer/search?q={} HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n",
        tx_hash
    );
    maybe_handle_transfer_submit_request(
        &mut search_server,
        search_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/search",
        "node-transfer-explorer-p0-ok",
        "world-transfer-explorer-p0-ok",
        temp_dir.as_path(),
    )
    .expect("search should be handled");
    drop(search_server);
    search_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut search_response_bytes = Vec::new();
    search_client
        .read_to_end(&mut search_response_bytes)
        .expect("read search response");
    let (_, search): (u16, ExplorerSearchResponse) =
        decode_http_json_response(&search_response_bytes);
    assert!(search.ok);
    assert!(search.items.iter().any(|item| item.item_type == "tx"));

    runtime
        .lock()
        .expect("lock runtime for stop")
        .stop()
        .expect("stop node runtime");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn explorer_p0_blocks_rejects_invalid_cursor_parameter() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("explorer_p0_invalid_cursor");
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-explorer-p0-invalid",
            "world-transfer-explorer-p0-invalid",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut blocks_server, mut blocks_client) = tcp_stream_pair();
    let blocks_http =
        "GET /v1/chain/explorer/blocks?cursor=bad HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut blocks_server,
        blocks_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/blocks",
        "node-transfer-explorer-p0-invalid",
        "world-transfer-explorer-p0-invalid",
        temp_dir.as_path(),
    )
    .expect("blocks request should be handled");
    assert!(handled);
    drop(blocks_server);

    blocks_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut blocks_response_bytes = Vec::new();
    blocks_client
        .read_to_end(&mut blocks_response_bytes)
        .expect("read response");
    let (status, response): (u16, ExplorerBlocksResponse) =
        decode_http_json_response(&blocks_response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn explorer_p1_endpoints_return_expected_payloads() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("explorer_p1_ok");
    seed_world_for_explorer_p1(temp_dir.as_path());
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-explorer-p1-ok",
            "world-transfer-explorer-p1-ok",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (public_key, private_key) = transfer_test_signer(51);
    let mut accepted_request = build_signed_transfer_request_with_accounts(
        "player:alice".to_string(),
        "player:bob".to_string(),
        9,
        8,
        public_key.clone(),
        private_key.clone(),
    );
    let now_ms = crate::now_unix_ms().saturating_sub(2_000);
    accepted_request.asset_id = Some("main_token".to_string());
    accepted_request.memo = Some("bridge:deposit:alpha".to_string());
    accepted_request.chain_id = Some("oasis7-main".to_string());
    accepted_request.network_id = Some("prod".to_string());
    accepted_request.tx_version = Some(2);
    accepted_request.tx_type = Some("asset_transfer".to_string());
    accepted_request.valid_until_unix_ms = Some(now_ms + 60_000);
    accepted_request.max_fee = Some(42);
    accepted_request.fee_asset_id = Some("main_token".to_string());
    accepted_request.application_payload_hash = Some("sha256:payload-alpha".to_string());
    accepted_request.client_request_id = Some("client-alpha".to_string());
    let accepted_action = Action::TransferMainToken {
        from_account_id: accepted_request.from_account_id.clone(),
        to_account_id: accepted_request.to_account_id.clone(),
        amount: accepted_request.amount,
        nonce: accepted_request.nonce,
        asset_id: accepted_request.asset_id.clone(),
        memo: accepted_request.memo.clone(),
        chain_id: accepted_request.chain_id.clone(),
        network_id: accepted_request.network_id.clone(),
        tx_version: accepted_request.tx_version,
        tx_type: accepted_request.tx_type.clone(),
        valid_until_unix_ms: accepted_request.valid_until_unix_ms,
        max_fee: accepted_request.max_fee,
        fee_asset_id: accepted_request.fee_asset_id.clone(),
        application_payload_hash: accepted_request.application_payload_hash.clone(),
        client_request_id: accepted_request.client_request_id.clone(),
    };
    accepted_request.signature = sign_main_token_runtime_action_auth(
        &accepted_action,
        accepted_request.from_account_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign p1 explorer v2 request")
    .signature
    .expect("p1 explorer v2 signature");
    super::super::with_transfer_tracker(|tracker| {
        tracker.record_accepted(77, &accepted_request, now_ms)
    });

    let (mut address_server, mut address_client) = tcp_stream_pair();
    let address_http = "GET /v1/chain/explorer/address?account_id=player:alice&limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut address_server,
        address_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/address",
        "node-transfer-explorer-p1-ok",
        "world-transfer-explorer-p1-ok",
        temp_dir.as_path(),
    )
    .expect("address request should be handled");
    assert!(handled);
    drop(address_server);
    address_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut address_response_bytes = Vec::new();
    address_client
        .read_to_end(&mut address_response_bytes)
        .expect("read address response");
    let (address_status, address): (u16, ExplorerAddressResponse) =
        decode_http_json_response(&address_response_bytes);
    assert_eq!(address_status, 200);
    assert!(address.ok);
    assert_eq!(address.account_id.as_deref(), Some("player:alice"));
    assert_eq!(address.liquid_balance, 1200);
    assert_eq!(address.restricted_starter_claim_balance, 125);
    assert_eq!(address.last_transfer_nonce, Some(7));
    assert!(!address.items.is_empty());
    let address_tx = address
        .items
        .iter()
        .find(|item| item.action_id == 77)
        .expect("address tx");
    assert_eq!(address_tx.asset_id.as_deref(), Some("main_token"));
    assert_eq!(address_tx.memo.as_deref(), Some("bridge:deposit:alpha"));
    assert_eq!(address_tx.chain_id.as_deref(), Some("oasis7-main"));
    assert_eq!(address_tx.network_id.as_deref(), Some("prod"));
    assert_eq!(address_tx.tx_version, Some(2));
    assert_eq!(address_tx.tx_type.as_deref(), Some("asset_transfer"));
    assert_eq!(
        address_tx.valid_until_unix_ms,
        accepted_request.valid_until_unix_ms
    );
    assert_eq!(address_tx.max_fee, Some(42));
    assert_eq!(address_tx.fee_asset_id.as_deref(), Some("main_token"));
    assert_eq!(
        address_tx.application_payload_hash.as_deref(),
        Some("sha256:payload-alpha")
    );
    assert_eq!(
        address_tx.client_request_id.as_deref(),
        Some("client-alpha")
    );

    let (mut contracts_server, mut contracts_client) = tcp_stream_pair();
    let contracts_http = "GET /v1/chain/explorer/contracts?limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut contracts_server,
        contracts_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/contracts",
        "node-transfer-explorer-p1-ok",
        "world-transfer-explorer-p1-ok",
        temp_dir.as_path(),
    )
    .expect("contracts request should be handled");
    drop(contracts_server);
    contracts_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut contracts_response_bytes = Vec::new();
    contracts_client
        .read_to_end(&mut contracts_response_bytes)
        .expect("read contracts response");
    let (_, contracts): (u16, ExplorerContractsResponse) =
        decode_http_json_response(&contracts_response_bytes);
    assert!(contracts.ok);
    assert!(
        contracts
            .items
            .iter()
            .any(|item| item.contract_id == "contract:alpha")
    );

    let (mut contract_server, mut contract_client) = tcp_stream_pair();
    let contract_http = "GET /v1/chain/explorer/contract?contract_id=contract:alpha HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut contract_server,
        contract_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/contract",
        "node-transfer-explorer-p1-ok",
        "world-transfer-explorer-p1-ok",
        temp_dir.as_path(),
    )
    .expect("contract request should be handled");
    drop(contract_server);
    contract_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut contract_response_bytes = Vec::new();
    contract_client
        .read_to_end(&mut contract_response_bytes)
        .expect("read contract response");
    let (_, contract): (u16, ExplorerContractResponse) =
        decode_http_json_response(&contract_response_bytes);
    assert!(contract.ok);
    assert_eq!(contract.contract_id.as_deref(), Some("contract:alpha"));
    assert!(contract.contract.is_some());
    assert!(!contract.recent_txs.is_empty());

    let (mut assets_server, mut assets_client) = tcp_stream_pair();
    let assets_http =
        "GET /v1/chain/explorer/assets?limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut assets_server,
        assets_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/assets",
        "node-transfer-explorer-p1-ok",
        "world-transfer-explorer-p1-ok",
        temp_dir.as_path(),
    )
    .expect("assets request should be handled");
    drop(assets_server);
    assets_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut assets_response_bytes = Vec::new();
    assets_client
        .read_to_end(&mut assets_response_bytes)
        .expect("read assets response");
    let (_, assets): (u16, ExplorerAssetsResponse) =
        decode_http_json_response(&assets_response_bytes);
    assert!(assets.ok);
    assert_eq!(assets.token_symbol, "OC");
    assert!(
        assets
            .holders
            .iter()
            .any(|item| item.account_id == "player:alice")
    );
    assert!(
        assets
            .holders
            .iter()
            .any(|item| item.account_id == "player:alice"
                && item.restricted_starter_claim_balance == 125
                && item.total_balance == 1500)
    );
    assert!(
        assets
            .holders
            .iter()
            .all(|item| item.total_balance == item.liquid_balance + item.vested_balance)
    );
    assert!(!assets.nft_supported);

    let (mut mempool_server, mut mempool_client) = tcp_stream_pair();
    let mempool_http = "GET /v1/chain/explorer/mempool?status=all&limit=20&cursor=0 HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    maybe_handle_transfer_submit_request(
        &mut mempool_server,
        mempool_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/mempool",
        "node-transfer-explorer-p1-ok",
        "world-transfer-explorer-p1-ok",
        temp_dir.as_path(),
    )
    .expect("mempool request should be handled");
    drop(mempool_server);
    mempool_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut mempool_response_bytes = Vec::new();
    mempool_client
        .read_to_end(&mut mempool_response_bytes)
        .expect("read mempool response");
    let (_, mempool): (u16, ExplorerMempoolResponse) =
        decode_http_json_response(&mempool_response_bytes);
    assert!(mempool.ok);
    assert_eq!(mempool.status_filter, "all");
    assert!(mempool.pending_count >= 1);
    assert!(!mempool.items.is_empty());
    assert!(mempool.items.iter().all(|item| {
        matches!(
            item.status,
            TransferLifecycleStatus::Accepted | TransferLifecycleStatus::Pending
        )
    }));
    let mempool_tx = mempool
        .items
        .iter()
        .find(|item| item.action_id == 77)
        .expect("mempool tx");
    assert_eq!(mempool_tx.asset_id.as_deref(), Some("main_token"));
    assert_eq!(mempool_tx.memo.as_deref(), Some("bridge:deposit:alpha"));
    assert_eq!(mempool_tx.chain_id.as_deref(), Some("oasis7-main"));
    assert_eq!(mempool_tx.network_id.as_deref(), Some("prod"));
    assert_eq!(mempool_tx.tx_version, Some(2));
    assert_eq!(mempool_tx.tx_type.as_deref(), Some("asset_transfer"));
    assert_eq!(
        mempool_tx.valid_until_unix_ms,
        accepted_request.valid_until_unix_ms
    );
    assert_eq!(mempool_tx.max_fee, Some(42));
    assert_eq!(mempool_tx.fee_asset_id.as_deref(), Some("main_token"));
    assert_eq!(
        mempool_tx.application_payload_hash.as_deref(),
        Some("sha256:payload-alpha")
    );
    assert_eq!(
        mempool_tx.client_request_id.as_deref(),
        Some("client-alpha")
    );

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn explorer_p1_mempool_rejects_invalid_status_parameter() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("explorer_p1_invalid_mempool_status");
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-explorer-p1-invalid",
            "world-transfer-explorer-p1-invalid",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut mempool_server, mut mempool_client) = tcp_stream_pair();
    let mempool_http =
        "GET /v1/chain/explorer/mempool?status=bad HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut mempool_server,
        mempool_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/mempool",
        "node-transfer-explorer-p1-invalid",
        "world-transfer-explorer-p1-invalid",
        temp_dir.as_path(),
    )
    .expect("mempool request should be handled");
    assert!(handled);
    drop(mempool_server);

    mempool_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut mempool_response_bytes = Vec::new();
    mempool_client
        .read_to_end(&mut mempool_response_bytes)
        .expect("read mempool response");
    let (status, response): (u16, ExplorerMempoolResponse) =
        decode_http_json_response(&mempool_response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn explorer_p1_address_returns_not_found_for_unknown_account() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("explorer_p1_address_not_found");
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-explorer-p1-address-not-found",
            "world-transfer-explorer-p1-address-not-found",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut address_server, mut address_client) = tcp_stream_pair();
    let address_http = "GET /v1/chain/explorer/address?account_id=player:missing HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut address_server,
        address_http.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/explorer/address",
        "node-transfer-explorer-p1-address-not-found",
        "world-transfer-explorer-p1-address-not-found",
        temp_dir.as_path(),
    )
    .expect("address request should be handled");
    assert!(handled);
    drop(address_server);

    address_client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut address_response_bytes = Vec::new();
    address_client
        .read_to_end(&mut address_response_bytes)
        .expect("read address response");
    let (status, response): (u16, ExplorerAddressResponse) =
        decode_http_json_response(&address_response_bytes);
    assert_eq!(status, 200);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("not_found"));

    let _ = fs::remove_dir_all(temp_dir);
}
