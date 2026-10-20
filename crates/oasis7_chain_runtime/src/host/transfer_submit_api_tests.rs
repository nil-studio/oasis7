use super::super::explorer_p0_api::{
    ExplorerBlocksResponse, ExplorerSearchResponse, ExplorerTxResponse, ExplorerTxsResponse,
};
use super::explorer_p1_api::{
    ExplorerAddressResponse, ExplorerAssetsResponse, ExplorerContractResponse,
    ExplorerContractsResponse, ExplorerMempoolResponse,
};
use super::{
    ChainExplorerOverviewResponse, ChainTransferAccountsResponse, ChainTransferHistoryResponse,
    ChainTransferStatusResponse, ChainTransferSubmitRequest, ChainTransferSubmitResponse,
    TransferLifecycleStatus, build_transfer_submit_action_payload,
    maybe_handle_transfer_submit_request, parse_transfer_submit_request,
    verify_transfer_submit_request_auth,
};
use crate::transfer_submit_api::preflight_validate_transfer_request;
use ed25519_dalek::SigningKey;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadBody, decode_consensus_action_payload,
    sign_main_token_runtime_action_auth,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitResult, NodeExecutionHook, NodeRole,
    NodeRuntime,
};
use oasis7_runtime::{
    Action, EconomicContractFulfillmentKind, EconomicContractState, EconomicContractStatus,
    MainTokenAccountBalance, MainTokenConfig, MainTokenSupplyState, WorldState,
    main_token_account_id_from_node_public_key,
};
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs};

#[path = "transfer_submit_api_explorer_tests.rs"]
mod explorer_tests;
#[path = "transfer_submit_api_status_history_tests.rs"]
mod status_history_tests;

fn tcp_stream_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let bind = listener.local_addr().expect("read local addr");
    let client = TcpStream::connect(bind).expect("connect loopback client");
    let (server, _) = listener.accept().expect("accept loopback connection");
    (server, client)
}

#[derive(Debug)]
struct NoopExecutionHook;

impl NodeExecutionHook for NoopExecutionHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("noop-block-{}", context.height),
            execution_state_root: format!("noop-root-{}", context.height),
        })
    }
}

fn decode_http_json_response<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> (u16, T) {
    let boundary = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response must include HTTP body separator");
    let header = std::str::from_utf8(&bytes[..boundary]).expect("response header utf-8");
    let status = header
        .split_whitespace()
        .nth(1)
        .and_then(|token| token.parse::<u16>().ok())
        .expect("response status code");
    let payload =
        serde_json::from_slice::<T>(&bytes[(boundary + 4)..]).expect("response json payload");
    (status, payload)
}

fn make_temp_dir(label: &str) -> std::path::PathBuf {
    let mut path = env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    path.push(format!(
        "oasis7_transfer_submit_api_tests_{label}_{}_{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn reset_transfer_state_for_tests() {
    super::with_transfer_tracker(|tracker| {
        tracker.by_action_id.clear();
        tracker.action_order.clear();
    });
    super::NEXT_TRANSFER_ACTION_ID.store(1, Ordering::Relaxed);
    super::super::explorer_p0_api::reset_store_for_tests();
}

fn lock_transfer_test_state() -> std::sync::MutexGuard<'static, ()> {
    static TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = TEST_GUARD.get_or_init(|| Mutex::new(()));
    let guard = guard
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_transfer_state_for_tests();
    guard
}

fn transfer_test_signer(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

fn build_signed_transfer_request_with_accounts(
    from_account_id: String,
    to_account_id: String,
    amount: u64,
    nonce: u64,
    public_key: String,
    private_key: String,
) -> ChainTransferSubmitRequest {
    let mut request = ChainTransferSubmitRequest {
        from_account_id,
        to_account_id,
        amount,
        nonce,
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
        public_key,
        signature: String::new(),
    };
    let action = Action::TransferMainToken {
        from_account_id: request.from_account_id.clone(),
        to_account_id: request.to_account_id.clone(),
        amount: request.amount,
        nonce: request.nonce,
        asset_id: request.asset_id.clone(),
        memo: request.memo.clone(),
        chain_id: request.chain_id.clone(),
        network_id: request.network_id.clone(),
        tx_version: request.tx_version,
        tx_type: request.tx_type.clone(),
        valid_until_unix_ms: request.valid_until_unix_ms,
        max_fee: request.max_fee,
        fee_asset_id: request.fee_asset_id.clone(),
        application_payload_hash: request.application_payload_hash.clone(),
        client_request_id: request.client_request_id.clone(),
    };
    request.signature = sign_main_token_runtime_action_auth(
        &action,
        request.from_account_id.as_str(),
        request.public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign transfer request")
    .signature
    .expect("single signer transfer proof signature");
    request
}

fn build_signed_transfer_request(
    seed: u8,
    to_seed: u8,
    amount: u64,
    nonce: u64,
) -> ChainTransferSubmitRequest {
    let (public_key, private_key) = transfer_test_signer(seed);
    let (to_public_key, _) = transfer_test_signer(to_seed);
    build_signed_transfer_request_with_accounts(
        main_token_account_id_from_node_public_key(public_key.as_str()),
        main_token_account_id_from_node_public_key(to_public_key.as_str()),
        amount,
        nonce,
        public_key,
        private_key,
    )
}

fn resign_transfer_request(request: &mut ChainTransferSubmitRequest, private_key: &str) {
    let action = Action::TransferMainToken {
        from_account_id: request.from_account_id.clone(),
        to_account_id: request.to_account_id.clone(),
        amount: request.amount,
        nonce: request.nonce,
        asset_id: request.asset_id.clone(),
        memo: request.memo.clone(),
        chain_id: request.chain_id.clone(),
        network_id: request.network_id.clone(),
        tx_version: request.tx_version,
        tx_type: request.tx_type.clone(),
        valid_until_unix_ms: request.valid_until_unix_ms,
        max_fee: request.max_fee,
        fee_asset_id: request.fee_asset_id.clone(),
        application_payload_hash: request.application_payload_hash.clone(),
        client_request_id: request.client_request_id.clone(),
    };
    request.signature = sign_main_token_runtime_action_auth(
        &action,
        request.from_account_id.as_str(),
        request.public_key.as_str(),
        private_key,
    )
    .expect("re-sign transfer request")
    .signature
    .expect("single signer transfer proof signature");
}

fn serialize_transfer_request(request: &ChainTransferSubmitRequest) -> Vec<u8> {
    serde_json::to_vec(request).expect("serialize transfer request")
}

#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn seed_world_for_explorer_p1(temp_dir: &Path) {
    let mut state = WorldState::default();
    state.main_token_config = MainTokenConfig {
        symbol: "OC".to_string(),
        decimals: 9,
        ..MainTokenConfig::default()
    };
    state.main_token_supply = MainTokenSupplyState {
        total_supply: 1_000_000,
        circulating_supply: 500_000,
        total_issued: 600_000,
        total_burned: 100_000,
    };
    state.main_token_balances.insert(
        "player:alice".to_string(),
        MainTokenAccountBalance {
            account_id: "player:alice".to_string(),
            liquid_balance: 1200,
            vested_balance: 300,
            restricted_starter_claim_balance: 125,
        },
    );
    state
        .main_token_transfer_nonces
        .insert("player:alice".to_string(), 7);
    state.economic_contracts.insert(
        "contract:alpha".to_string(),
        EconomicContractState {
            contract_id: "contract:alpha".to_string(),
            creator_agent_id: "player:alice".to_string(),
            counterparty_agent_id: "player:bob".to_string(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Electricity,
            settlement_amount: 88,
            reputation_stake: 5,
            expires_at: 1234,
            description: "power swap".to_string(),
            status: EconomicContractStatus::Accepted,
            accepted_at: Some(1000),
            settled_at: None,
            settlement_success: None,
            transfer_amount: 0,
            tax_amount: 0,
            settlement_notes: None,
        },
    );

    let world = oasis7::runtime_host::world::new_with_state(state);
    world
        .save_to_dir(temp_dir)
        .expect("save seeded world for explorer p1 tests");
}

#[test]
fn parse_transfer_submit_request_rejects_same_account() {
    let _guard = lock_transfer_test_state();
    let (public_key, private_key) = transfer_test_signer(7);
    let account_id = main_token_account_id_from_node_public_key(public_key.as_str());
    let request = build_signed_transfer_request_with_accounts(
        account_id.clone(),
        account_id,
        7,
        1,
        public_key,
        private_key,
    );
    let body = serialize_transfer_request(&request);
    let err = parse_transfer_submit_request(body.as_slice())
        .expect_err("same source and target must fail");
    assert!(err.contains("cannot be the same"));
}

#[test]
fn build_transfer_submit_action_payload_encodes_runtime_action() {
    let _guard = lock_transfer_test_state();
    let request = build_signed_transfer_request(7, 8, 7, 2);
    let body = serialize_transfer_request(&request);
    let request = parse_transfer_submit_request(body.as_slice()).expect("request should parse");
    let payload = build_transfer_submit_action_payload(&request).expect("payload");
    let decoded = decode_consensus_action_payload(payload.as_slice()).expect("decode payload");
    match decoded {
        ConsensusActionPayloadBody::RuntimeAction { action } => match action {
            Action::TransferMainToken {
                from_account_id,
                to_account_id,
                amount,
                nonce,
                asset_id,
                memo,
                chain_id,
                network_id,
                tx_version,
                tx_type,
                valid_until_unix_ms,
                max_fee,
                fee_asset_id,
                application_payload_hash,
                client_request_id,
            } => {
                let expected = build_signed_transfer_request(7, 8, 7, 2);
                assert_eq!(from_account_id, expected.from_account_id);
                assert_eq!(to_account_id, expected.to_account_id);
                assert_eq!(amount, 7);
                assert_eq!(nonce, 2);
                assert_eq!(asset_id, expected.asset_id);
                assert_eq!(memo, expected.memo);
                assert_eq!(chain_id, expected.chain_id);
                assert_eq!(network_id, expected.network_id);
                assert_eq!(tx_version, expected.tx_version);
                assert_eq!(tx_type, expected.tx_type);
                assert_eq!(valid_until_unix_ms, expected.valid_until_unix_ms);
                assert_eq!(max_fee, expected.max_fee);
                assert_eq!(fee_asset_id, expected.fee_asset_id);
                assert_eq!(application_payload_hash, expected.application_payload_hash);
                assert_eq!(client_request_id, expected.client_request_id);
            }
            other => panic!("expected TransferMainToken action, got {other:?}"),
        },
        other => panic!("expected runtime action payload, got {other:?}"),
    }
}

#[test]
fn build_transfer_submit_action_payload_carries_v2_context_fields() {
    let _guard = lock_transfer_test_state();
    let (public_key, private_key) = transfer_test_signer(17);
    let (to_public_key, _) = transfer_test_signer(18);
    let mut request = build_signed_transfer_request_with_accounts(
        main_token_account_id_from_node_public_key(public_key.as_str()),
        main_token_account_id_from_node_public_key(to_public_key.as_str()),
        9,
        4,
        public_key,
        private_key.clone(),
    );
    request.asset_id = Some("main_token".to_string());
    request.memo = Some("bridge:deposit:alpha".to_string());
    request.chain_id = Some("oasis7-main".to_string());
    request.network_id = Some("prod".to_string());
    request.tx_version = Some(2);
    request.tx_type = Some("asset_transfer".to_string());
    request.valid_until_unix_ms = Some(1_900_000_000_000);
    request.max_fee = Some(42);
    request.fee_asset_id = Some("main_token".to_string());
    request.application_payload_hash = Some("sha256:payload-alpha".to_string());
    request.client_request_id = Some("client-alpha".to_string());

    let action = Action::TransferMainToken {
        from_account_id: request.from_account_id.clone(),
        to_account_id: request.to_account_id.clone(),
        amount: request.amount,
        nonce: request.nonce,
        asset_id: request.asset_id.clone(),
        memo: request.memo.clone(),
        chain_id: request.chain_id.clone(),
        network_id: request.network_id.clone(),
        tx_version: request.tx_version,
        tx_type: request.tx_type.clone(),
        valid_until_unix_ms: request.valid_until_unix_ms,
        max_fee: request.max_fee,
        fee_asset_id: request.fee_asset_id.clone(),
        application_payload_hash: request.application_payload_hash.clone(),
        client_request_id: request.client_request_id.clone(),
    };
    request.signature = sign_main_token_runtime_action_auth(
        &action,
        request.from_account_id.as_str(),
        request.public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign v2 transfer request")
    .signature
    .expect("v2 signature");

    let body = serialize_transfer_request(&request);
    let parsed = parse_transfer_submit_request(body.as_slice()).expect("request should parse");
    let payload = build_transfer_submit_action_payload(&parsed).expect("payload");
    let decoded = decode_consensus_action_payload(payload.as_slice()).expect("decode payload");
    match decoded {
        ConsensusActionPayloadBody::RuntimeAction { action } => match action {
            Action::TransferMainToken {
                asset_id,
                memo,
                chain_id,
                network_id,
                tx_version,
                tx_type,
                valid_until_unix_ms,
                max_fee,
                fee_asset_id,
                application_payload_hash,
                client_request_id,
                ..
            } => {
                assert_eq!(asset_id.as_deref(), Some("main_token"));
                assert_eq!(memo.as_deref(), Some("bridge:deposit:alpha"));
                assert_eq!(chain_id.as_deref(), Some("oasis7-main"));
                assert_eq!(network_id.as_deref(), Some("prod"));
                assert_eq!(tx_version, Some(2));
                assert_eq!(tx_type.as_deref(), Some("asset_transfer"));
                assert_eq!(valid_until_unix_ms, Some(1_900_000_000_000));
                assert_eq!(max_fee, Some(42));
                assert_eq!(fee_asset_id.as_deref(), Some("main_token"));
                assert_eq!(
                    application_payload_hash.as_deref(),
                    Some("sha256:payload-alpha")
                );
                assert_eq!(client_request_id.as_deref(), Some("client-alpha"));
            }
            other => panic!("expected TransferMainToken action, got {other:?}"),
        },
        other => panic!("expected runtime action payload, got {other:?}"),
    }
    verify_transfer_submit_request_auth(&parsed).expect("v2 request auth should verify");
}

#[test]
fn verify_transfer_submit_request_auth_accepts_live_browser_captured_signature() {
    let _guard = lock_transfer_test_state();
    let request = ChainTransferSubmitRequest {
        from_account_id:
            "oc:pk:fded5085f1e8099257b7bfb2346eb6bd4194c3351d8f97686b18cfcc5969e0a3"
                .to_string(),
        to_account_id: "oc:pk:1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        amount: 1,
        nonce: 1,
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
        public_key: "fded5085f1e8099257b7bfb2346eb6bd4194c3351d8f97686b18cfcc5969e0a3"
            .to_string(),
        signature:
            "octransferauth:v1:9200ab505c80b5d27fb1a4e79624a083f2047669404d8c7d562e7d6b7da9154fcf7b85e96ab4c02bd5f2910845e4a7ce791d87398e0e3b9004bb022749d6830b"
                .to_string(),
    };
    let body = serialize_transfer_request(&request);
    let parsed = parse_transfer_submit_request(body.as_slice()).expect("request should parse");
    let action = Action::TransferMainToken {
        from_account_id: parsed.from_account_id.clone(),
        to_account_id: parsed.to_account_id.clone(),
        amount: parsed.amount,
        nonce: parsed.nonce,
        asset_id: parsed.asset_id.clone(),
        memo: parsed.memo.clone(),
        chain_id: parsed.chain_id.clone(),
        network_id: parsed.network_id.clone(),
        tx_version: parsed.tx_version,
        tx_type: parsed.tx_type.clone(),
        valid_until_unix_ms: parsed.valid_until_unix_ms,
        max_fee: parsed.max_fee,
        fee_asset_id: parsed.fee_asset_id.clone(),
        application_payload_hash: parsed.application_payload_hash.clone(),
        client_request_id: parsed.client_request_id.clone(),
    };
    let expected_signature = sign_main_token_runtime_action_auth(
        &action,
        parsed.from_account_id.as_str(),
        parsed.public_key.as_str(),
        "c7a149783d4d97d4b36f6f97ae43eb71af7fe595b7f717d329c96be3e58fdc29",
    )
    .expect("runtime helper should sign")
    .signature
    .expect("runtime helper signature");
    assert_eq!(
        parsed.signature, expected_signature,
        "runtime helper signature drift"
    );
    verify_transfer_submit_request_auth(&parsed).expect("browser-captured signature should verify");
}

#[test]
fn preflight_transfer_rejects_expired_v2_request() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("transfer_preflight_expired_v2");
    seed_world_for_explorer_p1(temp_dir.as_path());
    let (public_key, private_key) = transfer_test_signer(19);
    let (to_public_key, _) = transfer_test_signer(20);
    let mut request = build_signed_transfer_request_with_accounts(
        main_token_account_id_from_node_public_key(public_key.as_str()),
        main_token_account_id_from_node_public_key(to_public_key.as_str()),
        5,
        2,
        public_key.clone(),
        private_key.clone(),
    );
    request.asset_id = Some("main_token".to_string());
    request.memo = Some("expiry-check".to_string());
    request.chain_id = Some("oasis7-main".to_string());
    request.tx_version = Some(2);
    request.tx_type = Some("asset_transfer".to_string());
    request.valid_until_unix_ms = Some(1);
    request.max_fee = Some(77);
    request.fee_asset_id = Some("main_token".to_string());
    request.application_payload_hash = Some("sha256:payload-expiry".to_string());
    request.client_request_id = Some("client-expiry".to_string());
    let action = Action::TransferMainToken {
        from_account_id: request.from_account_id.clone(),
        to_account_id: request.to_account_id.clone(),
        amount: request.amount,
        nonce: request.nonce,
        asset_id: request.asset_id.clone(),
        memo: request.memo.clone(),
        chain_id: request.chain_id.clone(),
        network_id: request.network_id.clone(),
        tx_version: request.tx_version,
        tx_type: request.tx_type.clone(),
        valid_until_unix_ms: request.valid_until_unix_ms,
        max_fee: request.max_fee,
        fee_asset_id: request.fee_asset_id.clone(),
        application_payload_hash: request.application_payload_hash.clone(),
        client_request_id: request.client_request_id.clone(),
    };
    request.signature = sign_main_token_runtime_action_auth(
        &action,
        request.from_account_id.as_str(),
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign expired request")
    .signature
    .expect("signature");

    let err = preflight_validate_transfer_request(temp_dir.as_path(), &request)
        .expect_err("expired request should fail");
    assert_eq!(err.0, "expired");
    assert!(err.1.contains("expired"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn parse_transfer_submit_request_rejects_non_main_token_asset_id() {
    let _guard = lock_transfer_test_state();
    let (public_key, private_key) = transfer_test_signer(21);
    let (to_public_key, _) = transfer_test_signer(22);
    let mut request = build_signed_transfer_request_with_accounts(
        main_token_account_id_from_node_public_key(public_key.as_str()),
        main_token_account_id_from_node_public_key(to_public_key.as_str()),
        5,
        3,
        public_key,
        private_key.clone(),
    );
    request.asset_id = Some("oc_usdc".to_string());
    request.memo = Some("unsupported-asset".to_string());
    request.chain_id = Some("oasis7-main".to_string());
    request.tx_version = Some(2);
    request.tx_type = Some("asset_transfer".to_string());
    resign_transfer_request(&mut request, private_key.as_str());

    let body = serialize_transfer_request(&request);
    let err =
        parse_transfer_submit_request(body.as_slice()).expect_err("non-main asset should fail");
    assert!(err.contains("asset_id must be `main_token`"));
}

#[test]
fn transfer_submit_handler_returns_invalid_request_for_bad_payload() {
    let _guard = lock_transfer_test_state();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-submit-bad",
            "world-transfer-submit-bad",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let (public_key, private_key) = transfer_test_signer(9);
    let account_id = main_token_account_id_from_node_public_key(public_key.as_str());
    let request = build_signed_transfer_request_with_accounts(
        account_id.clone(),
        account_id,
        7,
        2,
        public_key,
        private_key,
    );
    let body = serde_json::to_string(&request).expect("serialize request");
    let request = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handled = maybe_handle_transfer_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-submit-bad",
        "world-transfer-submit-bad",
        Path::new("."),
    )
    .expect("handler should process request");
    assert!(handled);
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));
}

#[test]
fn preflight_transfer_rejects_restricted_only_balance() {
    let _guard = lock_transfer_test_state();
    let temp_dir = env::temp_dir().join(format!(
        "oasis7-transfer-preflight-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let mut world = oasis7::runtime_host::world::new();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 125,
        circulating_supply: 125,
        ..MainTokenSupplyState::default()
    });
    world
        .set_main_token_account_balance_with_restricted("player:starter", 0, 0, 125)
        .expect("seed restricted-only account");
    world
        .save_to_dir(temp_dir.as_path())
        .expect("save execution world");

    let request = ChainTransferSubmitRequest {
        from_account_id: "player:starter".to_string(),
        to_account_id: "player:receiver".to_string(),
        amount: 1,
        nonce: 1,
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
        public_key: "test-public-key".to_string(),
        signature: "test-signature".to_string(),
    };

    let err = preflight_validate_transfer_request(temp_dir.as_path(), &request)
        .expect_err("restricted-only transfer should fail");
    assert_eq!(err.0, "insufficient_balance");
    assert!(err.1.contains("transferable_balance=0"));
    assert!(err.1.contains("restricted_starter_claim_balance=125"));
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn transfer_accounts_endpoint_exposes_restricted_balance_separately() {
    let _guard = lock_transfer_test_state();
    let temp_dir = make_temp_dir("transfer_accounts_restricted");
    seed_world_for_explorer_p1(temp_dir.as_path());
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-accounts-restricted",
            "world-transfer-accounts-restricted",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (mut server, mut client) = tcp_stream_pair();
    let request = "GET /v1/chain/transfer/accounts HTTP/1.1\r\nHost: 127.0.0.1:5121\r\n\r\n";
    let handled = maybe_handle_transfer_submit_request(
        &mut server,
        request.as_bytes(),
        &runtime,
        "GET",
        "/v1/chain/transfer/accounts",
        "node-transfer-accounts-restricted",
        "world-transfer-accounts-restricted",
        temp_dir.as_path(),
    )
    .expect("accounts request should be handled");
    assert!(handled);
    drop(server);

    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let mut response_bytes = Vec::new();
    client
        .read_to_end(&mut response_bytes)
        .expect("read accounts response");
    let (status, response): (u16, ChainTransferAccountsResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 200);
    assert!(response.ok);
    let alice = response
        .accounts
        .iter()
        .find(|account| account.account_id == "player:alice")
        .expect("alice account entry");
    assert_eq!(alice.liquid_balance, 1200);
    assert_eq!(alice.vested_balance, 300);
    assert_eq!(alice.restricted_starter_claim_balance, 125);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn transfer_tracker_metrics_status_summarizes_lifecycle_and_latency() {
    let _guard = lock_transfer_test_state();
    super::with_transfer_tracker(|tracker| {
        tracker.by_action_id.insert(
            11,
            super::TrackedTransfer {
                action_id: 11,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:b".to_string(),
                amount: 10,
                nonce: 1,
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
                status: TransferLifecycleStatus::Accepted,
                submitted_at_unix_ms: 1_500,
                updated_at_unix_ms: 1_500,
                error_code: None,
                error: None,
            },
        );
        tracker.by_action_id.insert(
            12,
            super::TrackedTransfer {
                action_id: 12,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:c".to_string(),
                amount: 20,
                nonce: 2,
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
                status: TransferLifecycleStatus::Pending,
                submitted_at_unix_ms: 800,
                updated_at_unix_ms: 1_200,
                error_code: None,
                error: None,
            },
        );
        tracker.by_action_id.insert(
            13,
            super::TrackedTransfer {
                action_id: 13,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:d".to_string(),
                amount: 30,
                nonce: 3,
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
                status: TransferLifecycleStatus::Confirmed,
                submitted_at_unix_ms: 100,
                updated_at_unix_ms: 200,
                error_code: None,
                error: None,
            },
        );
        tracker.by_action_id.insert(
            14,
            super::TrackedTransfer {
                action_id: 14,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:e".to_string(),
                amount: 40,
                nonce: 4,
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
                status: TransferLifecycleStatus::Confirmed,
                submitted_at_unix_ms: 100,
                updated_at_unix_ms: 500,
                error_code: None,
                error: None,
            },
        );
        tracker.by_action_id.insert(
            15,
            super::TrackedTransfer {
                action_id: 15,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:f".to_string(),
                amount: 50,
                nonce: 5,
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
                status: TransferLifecycleStatus::Confirmed,
                submitted_at_unix_ms: 100,
                updated_at_unix_ms: 1_000,
                error_code: None,
                error: None,
            },
        );
        tracker.by_action_id.insert(
            16,
            super::TrackedTransfer {
                action_id: 16,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:g".to_string(),
                amount: 60,
                nonce: 6,
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
                status: TransferLifecycleStatus::Timeout,
                submitted_at_unix_ms: 50,
                updated_at_unix_ms: 1_500,
                error_code: Some("timeout".to_string()),
                error: Some("expired".to_string()),
            },
        );
        tracker.action_order.extend([11, 12, 13, 14, 15, 16]);
        let metrics = tracker.metrics_status(2_000);
        assert_eq!(metrics.tracked_records, 6);
        assert_eq!(metrics.accepted_count, 1);
        assert_eq!(metrics.pending_count, 1);
        assert_eq!(metrics.confirmed_count, 3);
        assert_eq!(metrics.failed_count, 0);
        assert_eq!(metrics.timeout_count, 1);
        assert_eq!(metrics.inflight_count, 2);
        assert_eq!(metrics.oldest_inflight_age_ms, Some(1_200));
        assert_eq!(metrics.recent_confirmation_latency.sample_count, 3);
        assert_eq!(
            metrics.recent_confirmation_latency.avg_latency_ms,
            Some(466)
        );
        assert_eq!(
            metrics.recent_confirmation_latency.max_latency_ms,
            Some(900)
        );
        assert_eq!(
            metrics.recent_confirmation_latency.p50_latency_ms,
            Some(400)
        );
        assert_eq!(
            metrics.recent_confirmation_latency.p95_latency_ms,
            Some(900)
        );
    });
}

#[test]
fn transfer_metrics_status_ignores_future_inflight_timestamps() {
    let _guard = lock_transfer_test_state();
    super::with_transfer_tracker(|tracker| {
        tracker.by_action_id.insert(
            21,
            super::TrackedTransfer {
                action_id: 21,
                from_account_id: "player:a".to_string(),
                to_account_id: "player:b".to_string(),
                amount: 10,
                nonce: 1,
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
                status: TransferLifecycleStatus::Accepted,
                submitted_at_unix_ms: 2_500,
                updated_at_unix_ms: 2_500,
                error_code: None,
                error: None,
            },
        );
        tracker.action_order.push_back(21);
        let metrics = tracker.metrics_status(2_000);
        assert_eq!(metrics.inflight_count, 1);
        assert_eq!(metrics.oldest_inflight_age_ms, None);
    });
}

#[test]
fn summarize_transfer_latency_samples_sorts_and_rounds_percentiles() {
    let summary = super::summarize_transfer_latency_samples(vec![900, 100, 400]);
    assert_eq!(summary.sample_count, 3);
    assert_eq!(summary.avg_latency_ms, Some(466));
    assert_eq!(summary.max_latency_ms, Some(900));
    assert_eq!(summary.p50_latency_ms, Some(400));
    assert_eq!(summary.p95_latency_ms, Some(900));
}

#[test]
fn transfer_submit_handler_rejects_missing_signature() {
    let _guard = lock_transfer_test_state();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-submit-missing-signature",
            "world-transfer-submit-missing-signature",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let (public_key, _) = transfer_test_signer(11);
    let from_account_id = main_token_account_id_from_node_public_key(public_key.as_str());
    let (to_public_key, _) = transfer_test_signer(12);
    let to_account_id = main_token_account_id_from_node_public_key(to_public_key.as_str());
    let body = format!(
        r#"{{"from_account_id":"{from_account_id}","to_account_id":"{to_account_id}","amount":7,"nonce":2,"public_key":"{public_key}"}}"#
    );

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    maybe_handle_transfer_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-submit-missing-signature",
        "world-transfer-submit-missing-signature",
        Path::new("."),
    )
    .expect("handler should process request");
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));
}

#[test]
fn transfer_submit_handler_rejects_invalid_signature() {
    let _guard = lock_transfer_test_state();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-submit-invalid-signature",
            "world-transfer-submit-invalid-signature",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let mut request = build_signed_transfer_request(13, 14, 7, 2);
    request.signature = format!("{}{}", "octransferauth:v1:", "f".repeat(128));
    let body = serde_json::to_string(&request).expect("serialize request");

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    maybe_handle_transfer_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-submit-invalid-signature",
        "world-transfer-submit-invalid-signature",
        Path::new("."),
    )
    .expect("handler should process request");
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_signature"));
}

#[test]
fn transfer_submit_handler_rejects_account_auth_mismatch() {
    let _guard = lock_transfer_test_state();
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(
            "node-transfer-submit-auth-mismatch",
            "world-transfer-submit-auth-mismatch",
            NodeRole::Sequencer,
        )
        .expect("node config"),
    )));

    let mut request = build_signed_transfer_request(15, 16, 7, 2);
    let (mismatch_public_key, _) = transfer_test_signer(17);
    request.from_account_id =
        main_token_account_id_from_node_public_key(mismatch_public_key.as_str());
    let body = serde_json::to_string(&request).expect("serialize request");

    let (mut server_stream, mut client_stream) = tcp_stream_pair();
    let request = format!(
        "POST /v1/chain/transfer/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    maybe_handle_transfer_submit_request(
        &mut server_stream,
        request.as_bytes(),
        &runtime,
        "POST",
        "/v1/chain/transfer/submit",
        "node-transfer-submit-auth-mismatch",
        "world-transfer-submit-auth-mismatch",
        Path::new("."),
    )
    .expect("handler should process request");
    drop(server_stream);

    client_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set client timeout");
    let mut response_bytes = Vec::new();
    client_stream
        .read_to_end(&mut response_bytes)
        .expect("read handler response");
    let (status, response): (u16, ChainTransferSubmitResponse) =
        decode_http_json_response(&response_bytes);
    assert_eq!(status, 400);
    assert!(!response.ok);
    assert_eq!(
        response.error_code.as_deref(),
        Some("account_auth_mismatch")
    );
}
