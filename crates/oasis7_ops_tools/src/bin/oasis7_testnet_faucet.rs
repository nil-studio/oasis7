use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oasis7::consensus_action_payload::{
    sign_main_token_runtime_action_auth, sign_threshold_main_token_runtime_action_auth,
};
use oasis7_runtime::{
    Action, MainTokenGenesisAllocationPlan, main_token_account_id_from_node_public_key,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[path = "oasis7_testnet_faucet/support.rs"]
mod faucet_support;

#[cfg(test)]
use faucet_support::prune_tracker_map;
use faucet_support::{
    ClaimReservation, faucet_claim_status_code, prune_faucet_state_trackers,
    rollback_claim_reservation,
};

const DEFAULT_CLAIM_PATH: &str = "/claim";
const DEFAULT_INFO_PATH: &str = "/";
const DEFAULT_HEALTH_PATH: &str = "/healthz";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 10;
const DEFAULT_COOLDOWN_SECS: u64 = 24 * 60 * 60;
const MAX_TRACKED_FAUCET_CLAIMANTS: usize = 4096;

#[derive(Debug, Clone)]
enum Command {
    Serve(ServeOptions),
    SubmitGenesis(SubmitGenesisOptions),
    ClaimVesting(ClaimVestingOptions),
}

#[derive(Debug, Clone)]
struct ServeOptions {
    listen: String,
    upstream: String,
    faucet_public_key: String,
    faucet_private_key: String,
    amount: u64,
    cooldown_secs: u64,
    request_timeout_secs: u64,
}

#[derive(Debug, Clone)]
struct SubmitGenesisOptions {
    upstream: String,
    controller_account_id: String,
    signers: Vec<SignerKeypair>,
    threshold: u16,
    bucket_id: String,
    recipient: String,
    ratio_bps: u32,
    cliff_epochs: u64,
    linear_unlock_epochs: u64,
    start_epoch: u64,
    request_timeout_secs: u64,
}

#[derive(Debug, Clone)]
struct ClaimVestingOptions {
    upstream: String,
    bucket_id: String,
    beneficiary: String,
    nonce: u64,
    public_key: String,
    private_key: String,
    request_timeout_secs: u64,
}

#[derive(Debug, Clone)]
struct SignerKeypair {
    public_key: String,
    private_key: String,
}

#[derive(Debug, Clone)]
struct FaucetService {
    options: ServeOptions,
    faucet_account_id: String,
    client: Client,
    state: Arc<Mutex<FaucetState>>,
}

#[derive(Debug, Default)]
struct FaucetState {
    next_nonce: Option<u64>,
    last_account_claim_unix_ms: HashMap<String, i64>,
    last_ip_claim_unix_ms: HashMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ChainTransferSubmitRequest {
    from_account_id: String,
    to_account_id: String,
    amount: u64,
    nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    network_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid_until_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_request_id: Option<String>,
    public_key: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ChainTransferSubmitResponse {
    ok: bool,
    #[serde(default)]
    action_id: Option<u64>,
    #[serde(default)]
    submitted_at_unix_ms: Option<i64>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ChainMainTokenSubmitResponse {
    ok: bool,
    #[serde(default)]
    action_id: Option<u64>,
    #[serde(default)]
    submitted_at_unix_ms: Option<i64>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ChainTransferAccountEntry {
    account_id: String,
    liquid_balance: u64,
    vested_balance: u64,
    restricted_starter_claim_balance: u64,
    #[serde(default)]
    last_transfer_nonce: Option<u64>,
    next_nonce_hint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ChainTransferAccountsResponse {
    ok: bool,
    #[serde(default)]
    accounts: Vec<ChainTransferAccountEntry>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct FaucetClaimRequest {
    account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FaucetClaimResponse {
    ok: bool,
    faucet_account_id: String,
    amount: u64,
    cooldown_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FaucetInfoResponse {
    ok: bool,
    faucet_account_id: String,
    amount: u64,
    cooldown_secs: u64,
    claim_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HealthResponse {
    ok: bool,
    observed_at_unix_ms: i64,
}

fn main() {
    let command = match parse_args(env::args().skip(1).collect()) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(1);
        }
    };

    let result = match command {
        Command::Serve(options) => run_serve(options),
        Command::SubmitGenesis(options) => run_submit_genesis(options),
        Command::ClaimVesting(options) => run_claim_vesting(options),
    };
    if let Err(err) = result {
        eprintln!("oasis7_testnet_faucet failed: {err}");
        process::exit(1);
    }
}

fn parse_args(args: Vec<String>) -> Result<Command, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err("subcommand is required".to_string());
    };
    match subcommand {
        "serve" => parse_serve_args(&args[1..]).map(Command::Serve),
        "submit-genesis" => parse_submit_genesis_args(&args[1..]).map(Command::SubmitGenesis),
        "claim-vesting" => parse_claim_vesting_args(&args[1..]).map(Command::ClaimVesting),
        "--help" | "-h" | "help" => {
            print_help();
            process::exit(0);
        }
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

fn parse_serve_args(args: &[String]) -> Result<ServeOptions, String> {
    let mut listen = None;
    let mut upstream = None;
    let mut faucet_public_key = None;
    let mut faucet_private_key = None;
    let mut amount = None;
    let mut cooldown_secs = DEFAULT_COOLDOWN_SECS;
    let mut request_timeout_secs = DEFAULT_REQUEST_TIMEOUT_SECS;
    let mut index = 0_usize;
    while index < args.len() {
        match args[index].as_str() {
            "--listen" => {
                listen = Some(parse_required_value(args, &mut index, "--listen")?);
            }
            "--upstream" => {
                upstream = Some(parse_required_value(args, &mut index, "--upstream")?);
            }
            "--faucet-public-key" => {
                faucet_public_key = Some(parse_required_value(
                    args,
                    &mut index,
                    "--faucet-public-key",
                )?);
            }
            "--faucet-public-key-file" => {
                let path = parse_required_value(args, &mut index, "--faucet-public-key-file")?;
                faucet_public_key = Some(read_trimmed_file(path.as_str())?);
            }
            "--faucet-private-key" => {
                faucet_private_key = Some(parse_required_value(
                    args,
                    &mut index,
                    "--faucet-private-key",
                )?);
            }
            "--faucet-private-key-file" => {
                let path = parse_required_value(args, &mut index, "--faucet-private-key-file")?;
                faucet_private_key = Some(read_trimmed_file(path.as_str())?);
            }
            "--amount" => {
                amount = Some(parse_u64(
                    parse_required_value(args, &mut index, "--amount")?.as_str(),
                    "--amount",
                )?);
            }
            "--cooldown-secs" => {
                cooldown_secs = parse_u64(
                    parse_required_value(args, &mut index, "--cooldown-secs")?.as_str(),
                    "--cooldown-secs",
                )?;
            }
            "--request-timeout-secs" => {
                request_timeout_secs = parse_u64(
                    parse_required_value(args, &mut index, "--request-timeout-secs")?.as_str(),
                    "--request-timeout-secs",
                )?;
            }
            other => return Err(format!("unknown serve argument `{other}`")),
        }
        index += 1;
    }
    Ok(ServeOptions {
        listen: normalize_required(listen, "--listen")?,
        upstream: normalize_required(upstream, "--upstream")?,
        faucet_public_key: normalize_required(faucet_public_key, "--faucet-public-key")?,
        faucet_private_key: normalize_required(faucet_private_key, "--faucet-private-key")?,
        amount: amount.ok_or_else(|| "--amount is required".to_string())?,
        cooldown_secs,
        request_timeout_secs,
    })
}

fn parse_submit_genesis_args(args: &[String]) -> Result<SubmitGenesisOptions, String> {
    let mut upstream = None;
    let mut controller_account_id = Some("msig.genesis.v1".to_string());
    let mut signer_public_keys = Vec::new();
    let mut signer_private_keys = Vec::new();
    let mut threshold = None;
    let mut bucket_id = None;
    let mut recipient = None;
    let mut ratio_bps = 10_000_u32;
    let mut cliff_epochs = 0_u64;
    let mut linear_unlock_epochs = 0_u64;
    let mut start_epoch = 0_u64;
    let mut request_timeout_secs = DEFAULT_REQUEST_TIMEOUT_SECS;
    let mut index = 0_usize;
    while index < args.len() {
        match args[index].as_str() {
            "--upstream" => upstream = Some(parse_required_value(args, &mut index, "--upstream")?),
            "--controller-account-id" => {
                controller_account_id = Some(parse_required_value(
                    args,
                    &mut index,
                    "--controller-account-id",
                )?);
            }
            "--signer-public-key" => {
                signer_public_keys.push(parse_required_value(
                    args,
                    &mut index,
                    "--signer-public-key",
                )?);
            }
            "--signer-public-key-file" => {
                let path = parse_required_value(args, &mut index, "--signer-public-key-file")?;
                signer_public_keys.push(read_trimmed_file(path.as_str())?);
            }
            "--signer-private-key" => {
                signer_private_keys.push(parse_required_value(
                    args,
                    &mut index,
                    "--signer-private-key",
                )?);
            }
            "--signer-private-key-file" => {
                let path = parse_required_value(args, &mut index, "--signer-private-key-file")?;
                signer_private_keys.push(read_trimmed_file(path.as_str())?);
            }
            "--threshold" => {
                threshold = Some(parse_u16(
                    parse_required_value(args, &mut index, "--threshold")?.as_str(),
                    "--threshold",
                )?);
            }
            "--bucket-id" => {
                bucket_id = Some(parse_required_value(args, &mut index, "--bucket-id")?)
            }
            "--recipient" => {
                recipient = Some(parse_required_value(args, &mut index, "--recipient")?)
            }
            "--ratio-bps" => {
                ratio_bps = parse_u32(
                    parse_required_value(args, &mut index, "--ratio-bps")?.as_str(),
                    "--ratio-bps",
                )?;
            }
            "--cliff-epochs" => {
                cliff_epochs = parse_u64(
                    parse_required_value(args, &mut index, "--cliff-epochs")?.as_str(),
                    "--cliff-epochs",
                )?;
            }
            "--linear-unlock-epochs" => {
                linear_unlock_epochs = parse_u64(
                    parse_required_value(args, &mut index, "--linear-unlock-epochs")?.as_str(),
                    "--linear-unlock-epochs",
                )?;
            }
            "--start-epoch" => {
                start_epoch = parse_u64(
                    parse_required_value(args, &mut index, "--start-epoch")?.as_str(),
                    "--start-epoch",
                )?;
            }
            "--request-timeout-secs" => {
                request_timeout_secs = parse_u64(
                    parse_required_value(args, &mut index, "--request-timeout-secs")?.as_str(),
                    "--request-timeout-secs",
                )?;
            }
            other => return Err(format!("unknown submit-genesis argument `{other}`")),
        }
        index += 1;
    }
    if signer_public_keys.len() != signer_private_keys.len() {
        return Err("submit-genesis signer public/private key counts must match".to_string());
    }
    let signers = signer_public_keys
        .into_iter()
        .zip(signer_private_keys)
        .map(|(public_key, private_key)| SignerKeypair {
            public_key,
            private_key,
        })
        .collect::<Vec<_>>();
    if signers.is_empty() {
        return Err("submit-genesis requires at least one signer".to_string());
    }
    Ok(SubmitGenesisOptions {
        upstream: normalize_required(upstream, "--upstream")?,
        controller_account_id: normalize_required(
            controller_account_id,
            "--controller-account-id",
        )?,
        threshold: threshold.ok_or_else(|| "--threshold is required".to_string())?,
        signers,
        bucket_id: normalize_required(bucket_id, "--bucket-id")?,
        recipient: normalize_required(recipient, "--recipient")?,
        ratio_bps,
        cliff_epochs,
        linear_unlock_epochs,
        start_epoch,
        request_timeout_secs,
    })
}

fn parse_claim_vesting_args(args: &[String]) -> Result<ClaimVestingOptions, String> {
    let mut upstream = None;
    let mut bucket_id = None;
    let mut beneficiary = None;
    let mut nonce = None;
    let mut public_key = None;
    let mut private_key = None;
    let mut request_timeout_secs = DEFAULT_REQUEST_TIMEOUT_SECS;
    let mut index = 0_usize;
    while index < args.len() {
        match args[index].as_str() {
            "--upstream" => upstream = Some(parse_required_value(args, &mut index, "--upstream")?),
            "--bucket-id" => {
                bucket_id = Some(parse_required_value(args, &mut index, "--bucket-id")?)
            }
            "--beneficiary" => {
                beneficiary = Some(parse_required_value(args, &mut index, "--beneficiary")?)
            }
            "--nonce" => {
                nonce = Some(parse_u64(
                    parse_required_value(args, &mut index, "--nonce")?.as_str(),
                    "--nonce",
                )?)
            }
            "--public-key" => {
                public_key = Some(parse_required_value(args, &mut index, "--public-key")?)
            }
            "--public-key-file" => {
                let path = parse_required_value(args, &mut index, "--public-key-file")?;
                public_key = Some(read_trimmed_file(path.as_str())?);
            }
            "--private-key" => {
                private_key = Some(parse_required_value(args, &mut index, "--private-key")?)
            }
            "--private-key-file" => {
                let path = parse_required_value(args, &mut index, "--private-key-file")?;
                private_key = Some(read_trimmed_file(path.as_str())?);
            }
            "--request-timeout-secs" => {
                request_timeout_secs = parse_u64(
                    parse_required_value(args, &mut index, "--request-timeout-secs")?.as_str(),
                    "--request-timeout-secs",
                )?;
            }
            other => return Err(format!("unknown claim-vesting argument `{other}`")),
        }
        index += 1;
    }
    Ok(ClaimVestingOptions {
        upstream: normalize_required(upstream, "--upstream")?,
        bucket_id: normalize_required(bucket_id, "--bucket-id")?,
        beneficiary: normalize_required(beneficiary, "--beneficiary")?,
        nonce: nonce.ok_or_else(|| "--nonce is required".to_string())?,
        public_key: normalize_required(public_key, "--public-key")?,
        private_key: normalize_required(private_key, "--private-key")?,
        request_timeout_secs,
    })
}

fn run_serve(options: ServeOptions) -> Result<(), String> {
    let faucet_account_id =
        main_token_account_id_from_node_public_key(options.faucet_public_key.as_str());
    let client = build_http_client(options.request_timeout_secs)?;
    let service = FaucetService {
        options,
        faucet_account_id,
        client,
        state: Arc::new(Mutex::new(FaucetState::default())),
    };
    let listener = TcpListener::bind(service.options.listen.as_str())
        .map_err(|err| format!("bind faucet listener failed: {err}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&FaucetInfoResponse {
            ok: true,
            faucet_account_id: service.faucet_account_id.clone(),
            amount: service.options.amount,
            cooldown_secs: service.options.cooldown_secs,
            claim_path: DEFAULT_CLAIM_PATH.to_string(),
        })
        .map_err(|err| format!("encode startup summary failed: {err}"))?
    );
    loop {
        let (stream, addr) = listener
            .accept()
            .map_err(|err| format!("accept faucet connection failed: {err}"))?;
        let service = service.clone();
        thread::spawn(move || {
            if let Err(err) = service.handle_connection(stream, addr.ip().to_string()) {
                eprintln!("warning: faucet request failed: {err}");
            }
        });
    }
}

fn run_submit_genesis(options: SubmitGenesisOptions) -> Result<(), String> {
    let client = build_http_client(options.request_timeout_secs)?;
    let action = Action::InitializeMainTokenGenesis {
        allocations: vec![MainTokenGenesisAllocationPlan {
            bucket_id: options.bucket_id.clone(),
            ratio_bps: options.ratio_bps,
            recipient: options.recipient.clone(),
            cliff_epochs: options.cliff_epochs,
            linear_unlock_epochs: options.linear_unlock_epochs,
            start_epoch: options.start_epoch,
        }],
    };
    let signer_refs = options
        .signers
        .iter()
        .map(|item| (item.public_key.as_str(), item.private_key.as_str()))
        .collect::<Vec<_>>();
    let proof = sign_threshold_main_token_runtime_action_auth(
        &action,
        options.controller_account_id.as_str(),
        options.threshold,
        signer_refs.as_slice(),
    )
    .map_err(|err| format!("sign genesis request failed: {err}"))?;
    let payload = serde_json::json!({
        "action": "initialize_main_token_genesis",
        "allocations": [{
            "bucket_id": options.bucket_id,
            "ratio_bps": options.ratio_bps,
            "recipient": options.recipient,
            "cliff_epochs": options.cliff_epochs,
            "linear_unlock_epochs": options.linear_unlock_epochs,
            "start_epoch": options.start_epoch,
        }],
        "auth": proof,
    });
    let response: ChainMainTokenSubmitResponse = post_json(
        &client,
        options.upstream.as_str(),
        "/v1/chain/main-token/submit",
        &payload,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response)
            .map_err(|err| format!("encode genesis response failed: {err}"))?
    );
    if !response.ok {
        return Err(format!(
            "submit genesis rejected: {}",
            response
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        ));
    }
    Ok(())
}

fn run_claim_vesting(options: ClaimVestingOptions) -> Result<(), String> {
    let client = build_http_client(options.request_timeout_secs)?;
    let action = Action::ClaimMainTokenVesting {
        bucket_id: options.bucket_id.clone(),
        beneficiary: options.beneficiary.clone(),
        nonce: options.nonce,
    };
    let proof = sign_main_token_runtime_action_auth(
        &action,
        options.beneficiary.as_str(),
        options.public_key.as_str(),
        options.private_key.as_str(),
    )
    .map_err(|err| format!("sign claim request failed: {err}"))?;
    let payload = serde_json::json!({
        "action": "claim_main_token_vesting",
        "bucket_id": options.bucket_id,
        "beneficiary": options.beneficiary,
        "nonce": options.nonce,
        "auth": proof,
    });
    let response: ChainMainTokenSubmitResponse = post_json(
        &client,
        options.upstream.as_str(),
        "/v1/chain/main-token/submit",
        &payload,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response)
            .map_err(|err| format!("encode claim response failed: {err}"))?
    );
    if !response.ok {
        return Err(format!(
            "claim vesting rejected: {}",
            response
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        ));
    }
    Ok(())
}

impl FaucetService {
    fn handle_connection(&self, mut stream: TcpStream, remote_ip: String) -> Result<(), String> {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|err| format!("set faucet read timeout failed: {err}"))?;
        let mut buffer = [0_u8; 65_536];
        let bytes = stream
            .read(&mut buffer)
            .map_err(|err| format!("read faucet request failed: {err}"))?;
        if bytes == 0 {
            return Ok(());
        }
        let request = String::from_utf8_lossy(&buffer[..bytes]);
        let Some(line) = request.lines().next() else {
            write_json_response(
                &mut stream,
                400,
                &serde_json::json!({"ok": false, "error_code": "bad_request", "error": "missing request line"}),
            )?;
            return Ok(());
        };
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or_default();
        let path = target.split('?').next().unwrap_or(target);
        match (method, path) {
            ("GET", DEFAULT_INFO_PATH) => {
                let payload = FaucetInfoResponse {
                    ok: true,
                    faucet_account_id: self.faucet_account_id.clone(),
                    amount: self.options.amount,
                    cooldown_secs: self.options.cooldown_secs,
                    claim_path: DEFAULT_CLAIM_PATH.to_string(),
                };
                write_json_response(&mut stream, 200, &payload)?;
            }
            ("GET", DEFAULT_HEALTH_PATH) => {
                write_json_response(
                    &mut stream,
                    200,
                    &HealthResponse {
                        ok: true,
                        observed_at_unix_ms: now_unix_ms(),
                    },
                )?;
            }
            ("POST", DEFAULT_CLAIM_PATH) => {
                let body = match extract_http_json_body(&buffer[..bytes]) {
                    Ok(body) => body,
                    Err(err) => {
                        write_json_response(
                            &mut stream,
                            400,
                            &serde_json::json!({
                                "ok": false,
                                "error_code": "bad_request",
                                "error": err,
                            }),
                        )?;
                        return Ok(());
                    }
                };
                let request: FaucetClaimRequest = match serde_json::from_slice(body) {
                    Ok(request) => request,
                    Err(err) => {
                        write_json_response(
                            &mut stream,
                            400,
                            &serde_json::json!({
                                "ok": false,
                                "error_code": "bad_request",
                                "error": format!("invalid faucet claim request: {err}"),
                            }),
                        )?;
                        return Ok(());
                    }
                };
                let response = match self.process_claim(request, remote_ip.as_str()) {
                    Ok(response) => response,
                    Err(err) => {
                        write_json_response(
                            &mut stream,
                            500,
                            &serde_json::json!({
                                "ok": false,
                                "error_code": "internal_error",
                                "error": err,
                            }),
                        )?;
                        return Ok(());
                    }
                };
                write_json_response(&mut stream, faucet_claim_status_code(&response), &response)?;
            }
            _ => {
                write_json_response(
                    &mut stream,
                    404,
                    &serde_json::json!({"ok": false, "error_code": "not_found", "error": "unsupported faucet path"}),
                )?;
            }
        }
        Ok(())
    }

    fn process_claim(
        &self,
        request: FaucetClaimRequest,
        remote_ip: &str,
    ) -> Result<FaucetClaimResponse, String> {
        let target_account_id = match validate_target_account_id(request.account_id.as_str()) {
            Ok(account_id) => account_id,
            Err(err) => {
                return Ok(self.claim_error_response(
                    "bad_request",
                    format!("invalid faucet target account: {err}"),
                ));
            }
        };
        let now_ms = now_unix_ms();
        let cooldown_ms = i64::try_from(self.options.cooldown_secs)
            .unwrap_or(i64::MAX)
            .saturating_mul(1000);
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "lock faucet state failed".to_string())?;
            prune_faucet_state_trackers(&mut state, now_ms, cooldown_ms);
            if let Some(cooldown_response) = self.cooldown_response(
                &state,
                target_account_id.as_str(),
                remote_ip,
                now_ms,
                cooldown_ms,
            ) {
                return Ok(cooldown_response);
            }
        }
        let snapshot = match fetch_faucet_account_snapshot(
            &self.client,
            self.options.upstream.as_str(),
            self.faucet_account_id.as_str(),
        ) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                return Ok(self.claim_error_response(
                    "upstream_unavailable",
                    format!("fetch faucet account snapshot failed: {err}"),
                ));
            }
        };
        if snapshot.liquid_balance < self.options.amount {
            return Ok(self.claim_error_response(
                "insufficient_balance",
                format!(
                    "faucet balance too low: liquid_balance={} amount={}",
                    snapshot.liquid_balance, self.options.amount
                ),
            ));
        }
        let (nonce, reservation) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "lock faucet state failed".to_string())?;
            prune_faucet_state_trackers(&mut state, now_ms, cooldown_ms);
            if let Some(cooldown_response) = self.cooldown_response(
                &state,
                target_account_id.as_str(),
                remote_ip,
                now_ms,
                cooldown_ms,
            ) {
                return Ok(cooldown_response);
            }
            let nonce = state
                .next_nonce
                .unwrap_or(snapshot.next_nonce_hint)
                .max(snapshot.next_nonce_hint);
            let previous_next_nonce = state.next_nonce;
            let reserved_next_nonce = nonce.saturating_add(1);
            state.next_nonce = Some(reserved_next_nonce);
            let previous_account_claim_ms = state
                .last_account_claim_unix_ms
                .insert(target_account_id.clone(), now_ms);
            let previous_ip_claim_ms = state
                .last_ip_claim_unix_ms
                .insert(remote_ip.to_string(), now_ms);
            (
                nonce,
                ClaimReservation {
                    previous_next_nonce,
                    reserved_next_nonce,
                    previous_account_claim_ms,
                    previous_ip_claim_ms,
                },
            )
        };
        let transfer_request = match build_signed_transfer_request(
            self.faucet_account_id.as_str(),
            target_account_id.as_str(),
            self.options.amount,
            nonce,
            self.options.faucet_public_key.as_str(),
            self.options.faucet_private_key.as_str(),
        ) {
            Ok(request) => request,
            Err(err) => {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "lock faucet state failed".to_string())?;
                rollback_claim_reservation(
                    &mut state,
                    target_account_id.as_str(),
                    remote_ip,
                    now_ms,
                    &reservation,
                );
                return Err(err);
            }
        };
        let response: ChainTransferSubmitResponse = match post_json(
            &self.client,
            self.options.upstream.as_str(),
            "/v1/chain/transfer/submit",
            &transfer_request,
        ) {
            Ok(response) => response,
            Err(err) => {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "lock faucet state failed".to_string())?;
                rollback_claim_reservation(
                    &mut state,
                    target_account_id.as_str(),
                    remote_ip,
                    now_ms,
                    &reservation,
                );
                return Ok(self.claim_error_response(
                    "upstream_unavailable",
                    format!("submit faucet transfer failed: {err}"),
                ));
            }
        };
        if !response.ok {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "lock faucet state failed".to_string())?;
            rollback_claim_reservation(
                &mut state,
                target_account_id.as_str(),
                remote_ip,
                now_ms,
                &reservation,
            );
        }
        Ok(FaucetClaimResponse {
            ok: response.ok,
            faucet_account_id: self.faucet_account_id.clone(),
            amount: self.options.amount,
            cooldown_secs: self.options.cooldown_secs,
            action_id: response.action_id,
            submitted_at_unix_ms: response.submitted_at_unix_ms,
            error_code: response.error_code,
            error: response.error,
        })
    }

    fn claim_error_response(&self, error_code: &str, error: String) -> FaucetClaimResponse {
        FaucetClaimResponse {
            ok: false,
            faucet_account_id: self.faucet_account_id.clone(),
            amount: self.options.amount,
            cooldown_secs: self.options.cooldown_secs,
            action_id: None,
            submitted_at_unix_ms: None,
            error_code: Some(error_code.to_string()),
            error: Some(error),
        }
    }

    fn cooldown_response(
        &self,
        state: &FaucetState,
        target_account_id: &str,
        remote_ip: &str,
        now_ms: i64,
        cooldown_ms: i64,
    ) -> Option<FaucetClaimResponse> {
        if let Some(last_ms) = state.last_account_claim_unix_ms.get(target_account_id)
            && now_ms.saturating_sub(*last_ms) < cooldown_ms
        {
            return Some(self.claim_error_response(
                "cooldown_active",
                format!("account is still in cooldown: account_id={target_account_id}"),
            ));
        }
        if let Some(last_ms) = state.last_ip_claim_unix_ms.get(remote_ip)
            && now_ms.saturating_sub(*last_ms) < cooldown_ms
        {
            return Some(self.claim_error_response(
                "ip_cooldown_active",
                format!("ip is still in cooldown: ip={remote_ip}"),
            ));
        }
        None
    }
}

fn build_signed_transfer_request(
    from_account_id: &str,
    to_account_id: &str,
    amount: u64,
    nonce: u64,
    public_key: &str,
    private_key: &str,
) -> Result<ChainTransferSubmitRequest, String> {
    let action = Action::TransferMainToken {
        from_account_id: from_account_id.to_string(),
        to_account_id: to_account_id.to_string(),
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
    };
    let proof =
        sign_main_token_runtime_action_auth(&action, from_account_id, public_key, private_key)
            .map_err(|err| format!("sign transfer request failed: {err}"))?;
    Ok(ChainTransferSubmitRequest {
        from_account_id: from_account_id.to_string(),
        to_account_id: to_account_id.to_string(),
        amount,
        nonce,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        client_request_id: None,
        public_key: proof
            .public_key
            .ok_or_else(|| "signed transfer proof missing public_key".to_string())?,
        signature: proof
            .signature
            .ok_or_else(|| "signed transfer proof missing signature".to_string())?,
    })
}

fn fetch_faucet_account_snapshot(
    client: &Client,
    upstream: &str,
    faucet_account_id: &str,
) -> Result<ChainTransferAccountEntry, String> {
    let response: ChainTransferAccountsResponse =
        get_json(client, upstream, "/v1/chain/transfer/accounts")?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "transfer accounts endpoint returned error".to_string()));
    }
    response
        .accounts
        .into_iter()
        .find(|entry| entry.account_id == faucet_account_id)
        .ok_or_else(|| format!("faucet account not found on upstream: {faucet_account_id}"))
}

fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    client: &Client,
    upstream: &str,
    path: &str,
    payload: &T,
) -> Result<R, String> {
    let url = format!("{}{}", normalize_upstream(upstream), path);
    let response = client
        .post(url)
        .json(payload)
        .send()
        .map_err(|err| format!("POST {path} failed: {err}"))?;
    decode_http_json::<R>(response)
}

fn get_json<R: for<'de> Deserialize<'de>>(
    client: &Client,
    upstream: &str,
    path: &str,
) -> Result<R, String> {
    let url = format!("{}{}", normalize_upstream(upstream), path);
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("GET {path} failed: {err}"))?;
    decode_http_json::<R>(response)
}

fn decode_http_json<R: for<'de> Deserialize<'de>>(
    response: reqwest::blocking::Response,
) -> Result<R, String> {
    let status = response.status();
    let text = response
        .text()
        .map_err(|err| format!("read http response body failed: {err}"))?;
    let decoded = serde_json::from_str::<R>(text.as_str()).map_err(|err| {
        format!("decode http response failed: status={status} error={err} body={text}")
    })?;
    Ok(decoded)
}

fn build_http_client(timeout_secs: u64) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|err| format!("build http client failed: {err}"))
}

fn extract_http_json_body(request_bytes: &[u8]) -> Result<&[u8], String> {
    let boundary = request_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "invalid HTTP request: missing body separator".to_string())?;
    Ok(&request_bytes[(boundary + 4)..])
}

fn write_json_response<T: Serialize>(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &T,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload)
        .map_err(|err| format!("encode faucet response failed: {err}"))?;
    let status_line = match status_code {
        200 => "HTTP/1.1 200 OK",
        400 => "HTTP/1.1 400 Bad Request",
        404 => "HTTP/1.1 404 Not Found",
        429 => "HTTP/1.1 429 Too Many Requests",
        502 => "HTTP/1.1 502 Bad Gateway",
        503 => "HTTP/1.1 503 Service Unavailable",
        _ => "HTTP/1.1 500 Internal Server Error",
    };
    let headers = format!(
        "{status_line}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(body.as_slice()))
        .map_err(|err| format!("write faucet response failed: {err}"))
}

fn validate_target_account_id(raw: &str) -> Result<String, String> {
    let value = raw.trim().to_ascii_lowercase();
    if !value.starts_with("oc:pk:") {
        return Err("target account_id must start with oc:pk:".to_string());
    }
    let hex_part = &value["oc:pk:".len()..];
    if hex_part.len() != 64 {
        return Err("target account_id public key suffix must be 64 hex chars".to_string());
    }
    if !hex_part.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("target account_id public key suffix must be hex".to_string());
    }
    Ok(value)
}

fn normalize_required(value: Option<String>, label: &str) -> Result<String, String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| format!("{label} is required"))
}

fn parse_required_value(args: &[String], index: &mut usize, label: &str) -> Result<String, String> {
    let next = index.saturating_add(1);
    let value = args
        .get(next)
        .cloned()
        .ok_or_else(|| format!("{label} requires a value"))?;
    *index = next;
    Ok(value)
}

fn parse_u64(raw: &str, label: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|err| format!("parse {label} failed: {err}"))
}

fn parse_u32(raw: &str, label: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .map_err(|err| format!("parse {label} failed: {err}"))
}

fn parse_u16(raw: &str, label: &str) -> Result<u16, String> {
    raw.parse::<u16>()
        .map_err(|err| format!("parse {label} failed: {err}"))
}

fn read_trimmed_file(path: &str) -> Result<String, String> {
    let content =
        std::fs::read_to_string(path).map_err(|err| format!("read {path} failed: {err}"))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(format!("{path} is empty"));
    }
    Ok(trimmed.to_string())
}

fn normalize_upstream(raw: &str) -> String {
    raw.trim_end_matches('/').to_string()
}

fn now_unix_ms() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
}

fn print_help() {
    eprintln!(
        "Usage:\n  oasis7_testnet_faucet serve --listen <host:port> --upstream <base_url> --faucet-public-key[-file] <value> --faucet-private-key[-file] <value> --amount <u64> [--cooldown-secs <u64>] [--request-timeout-secs <u64>]\n  oasis7_testnet_faucet submit-genesis --upstream <base_url> --threshold <u16> --signer-public-key[-file] <value> --signer-private-key[-file] <value> --bucket-id <id> --recipient <account_id> [--controller-account-id <id>] [--ratio-bps <u32>] [--cliff-epochs <u64>] [--linear-unlock-epochs <u64>] [--start-epoch <u64>]\n  oasis7_testnet_faucet claim-vesting --upstream <base_url> --bucket-id <id> --beneficiary <account_id> --nonce <u64> --public-key[-file] <value> --private-key[-file] <value>"
    );
}

#[cfg(test)]
#[path = "oasis7_testnet_faucet/tests.rs"]
mod tests;
