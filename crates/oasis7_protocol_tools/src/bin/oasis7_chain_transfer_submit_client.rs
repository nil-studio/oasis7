use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[cfg(test)]
use oasis7::runtime::Action;
use oasis7_client_api::{MainTokenTransfer, sign_main_token_transfer};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let config = CliConfig::parse(args)?;
    let request = build_signed_transfer_request(&config)?;
    match config.command {
        Command::Sign => {
            print_json(&request)?;
        }
        Command::Submit => {
            let chain_base_url = config
                .chain_base_url
                .as_deref()
                .ok_or_else(|| "--chain-base-url is required for submit".to_string())?;
            let response = submit_transfer(chain_base_url, &request, config.timeout_ms)?;
            print_json(&response)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Sign,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliConfig {
    command: Command,
    keys_file: PathBuf,
    persona: String,
    to_account_id: String,
    amount: u64,
    nonce: u64,
    memo: Option<String>,
    chain_base_url: Option<String>,
    timeout_ms: u64,
}

impl CliConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let command = match args.next().as_deref() {
            Some("sign") => Command::Sign,
            Some("submit") => Command::Submit,
            Some("-h") | Some("--help") | None => return Err(usage()),
            Some(other) => return Err(format!("unknown subcommand `{other}`\n\n{}", usage())),
        };

        let mut keys_file = None;
        let mut persona = None;
        let mut to_account_id = None;
        let mut amount = None;
        let mut nonce = None;
        let mut memo = None;
        let mut chain_base_url = None;
        let mut timeout_ms = 5_000_u64;

        while let Some(flag) = args.next() {
            let value = match flag.as_str() {
                "--keys-file" => &mut keys_file,
                "--persona" => &mut persona,
                "--to-account-id" => &mut to_account_id,
                "--amount" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--amount requires a value".to_string())?;
                    amount = Some(
                        raw.parse::<u64>()
                            .map_err(|_| format!("invalid --amount value `{raw}`"))?,
                    );
                    continue;
                }
                "--nonce" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--nonce requires a value".to_string())?;
                    nonce = Some(
                        raw.parse::<u64>()
                            .map_err(|_| format!("invalid --nonce value `{raw}`"))?,
                    );
                    continue;
                }
                "--chain-base-url" => &mut chain_base_url,
                "--memo" => &mut memo,
                "--timeout-ms" => {
                    let raw = args
                        .next()
                        .ok_or_else(|| "--timeout-ms requires a value".to_string())?;
                    timeout_ms = raw
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --timeout-ms value `{raw}`"))?
                        .max(1);
                    continue;
                }
                "-h" | "--help" => return Err(usage()),
                other => return Err(format!("unknown option `{other}`\n\n{}", usage())),
            };
            let raw = args
                .next()
                .ok_or_else(|| format!("{flag} requires a value"))?;
            *value = Some(raw);
        }

        Ok(Self {
            command,
            keys_file: PathBuf::from(
                keys_file.ok_or_else(|| "--keys-file is required".to_string())?,
            ),
            persona: persona.ok_or_else(|| "--persona is required".to_string())?,
            to_account_id: to_account_id
                .ok_or_else(|| "--to-account-id is required".to_string())?,
            amount: amount.ok_or_else(|| "--amount is required".to_string())?,
            nonce: nonce.ok_or_else(|| "--nonce is required".to_string())?,
            memo: memo
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            chain_base_url,
            timeout_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestPersona {
    slug: String,
    private_key_hex: String,
    public_key_hex: String,
    account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainTransferSubmitResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lifecycle_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn usage() -> String {
    "Usage: oasis7_chain_transfer_submit_client <sign|submit> --keys-file <path> --persona <slug> --to-account-id <account> --amount <n> --nonce <n> [--memo <text>] [--chain-base-url <url>] [--timeout-ms <n>]\n\n\
Reads the persona private key from a local secret file, signs a TransferMainToken request with the canonical octransferauth proof, and either prints the submit JSON or posts it to /v1/chain/transfer/submit.\n"
        .to_string()
}

fn build_signed_transfer_request(config: &CliConfig) -> Result<ChainTransferSubmitRequest, String> {
    let persona = load_persona(config.keys_file.as_path(), config.persona.as_str())?;
    let uses_v2_context = config.memo.is_some();
    let tx_version = uses_v2_context.then_some(2);
    let tx_type = uses_v2_context.then(|| "asset_transfer".to_string());
    let action = MainTokenTransfer {
        from_account_id: persona.account_id.clone(),
        to_account_id: config.to_account_id.clone(),
        amount: config.amount,
        nonce: config.nonce,
        asset_id: None,
        memo: config.memo.clone(),
        chain_id: None,
        network_id: None,
        tx_version,
        tx_type: tx_type.clone(),
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    };
    let proof = sign_main_token_transfer(
        &action,
        persona.account_id.as_str(),
        persona.public_key_hex.as_str(),
        persona.private_key_hex.as_str(),
    )
    .map_err(|err| format!("sign transfer request failed: {err}"))?;
    Ok(ChainTransferSubmitRequest {
        from_account_id: persona.account_id,
        to_account_id: config.to_account_id.clone(),
        amount: config.amount,
        nonce: config.nonce,
        asset_id: None,
        memo: config.memo.clone(),
        chain_id: None,
        network_id: None,
        tx_version,
        tx_type,
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

fn load_persona(path: &Path, slug: &str) -> Result<TestPersona, String> {
    let content =
        fs::read_to_string(path).map_err(|err| format!("read keys file failed: {err}"))?;
    parse_persona(content.as_str(), slug)
}

fn parse_persona(content: &str, slug: &str) -> Result<TestPersona, String> {
    let mut current_slug = None::<String>;
    let mut current = BTreeMap::<String, String>::new();
    let mut matches = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with("## ") {
            collect_persona_match(&mut matches, current_slug.as_deref(), &current, slug)?;
            current.clear();
            current_slug = parse_heading_slug(line);
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    collect_persona_match(&mut matches, current_slug.as_deref(), &current, slug)?;

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!("persona `{slug}` not found in keys file")),
        _ => Err(format!(
            "persona `{slug}` appears more than once in keys file"
        )),
    }
}

fn parse_heading_slug(line: &str) -> Option<String> {
    let title = line.trim_start_matches('#').trim();
    let after_index = title
        .split_once('.')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(title);
    (!after_index.is_empty()).then(|| after_index.to_string())
}

fn collect_persona_match(
    matches: &mut Vec<TestPersona>,
    current_slug: Option<&str>,
    current: &BTreeMap<String, String>,
    wanted_slug: &str,
) -> Result<(), String> {
    if current_slug != Some(wanted_slug) {
        return Ok(());
    }
    let private_key_hex = normalize_hex_field(
        required_persona_field(current, "chain_private_key_hex", wanted_slug)?.as_str(),
        64,
        "chain_private_key_hex",
    )?;
    let public_key_hex = normalize_hex_field(
        required_persona_field(current, "chain_public_key_hex", wanted_slug)?.as_str(),
        64,
        "chain_public_key_hex",
    )?;
    let account_id = required_persona_field(current, "oasis_sender_account_id", wanted_slug)?;
    let Some(account_public_key_hex) = account_id.strip_prefix("oc:pk:") else {
        return Err(format!(
            "persona `{wanted_slug}` oasis_sender_account_id must start with oc:pk:"
        ));
    };
    if !account_public_key_hex.eq_ignore_ascii_case(public_key_hex.as_str()) {
        return Err(format!(
            "persona `{wanted_slug}` oasis_sender_account_id does not match chain_public_key_hex"
        ));
    }
    let account_id = format!("oc:pk:{public_key_hex}");
    matches.push(TestPersona {
        slug: wanted_slug.to_string(),
        private_key_hex,
        public_key_hex,
        account_id,
    });
    Ok(())
}

fn required_persona_field(
    current: &BTreeMap<String, String>,
    key: &str,
    wanted_slug: &str,
) -> Result<String, String> {
    current
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| format!("persona `{wanted_slug}` missing {key}"))
}

fn normalize_hex_field(value: &str, expected_len: usize, label: &str) -> Result<String, String> {
    if value.len() != expected_len || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} must be {expected_len} hex chars"));
    }
    Ok(value.to_ascii_lowercase())
}

fn submit_transfer(
    chain_base_url: &str,
    request: &ChainTransferSubmitRequest,
    timeout_ms: u64,
) -> Result<ChainTransferSubmitResponse, String> {
    let timeout_ms = timeout_ms.max(1);
    let client = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|err| format!("build http client failed: {err}"))?;
    let url = format!(
        "{}/v1/chain/transfer/submit",
        chain_base_url.trim_end_matches('/')
    );
    let response = client
        .post(url)
        .json(request)
        .send()
        .map_err(|err| format!("submit transfer request failed: {err}"))?;
    let status = response.status();
    let payload = response
        .json::<ChainTransferSubmitResponse>()
        .map_err(|err| format!("decode transfer submit response failed: {err}"))?;
    if !status.is_success() || !payload.ok {
        return Err(format!(
            "transfer submit failed: http_status={status} error_code={} error={}",
            payload.error_code.as_deref().unwrap_or("unknown"),
            payload.error.as_deref().unwrap_or("unknown")
        ));
    }
    Ok(payload)
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let serialized =
        serde_json::to_string_pretty(value).map_err(|err| format!("encode json failed: {err}"))?;
    println!("{serialized}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use oasis7::consensus_action_payload::{
        MainTokenActionAuthProof, MainTokenActionAuthScheme, verify_main_token_runtime_action_auth,
    };

    fn sample_keys_file() -> String {
        let seed = [7_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let private_key = hex::encode(seed);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());
        format!(
            "# test\n\n## 1. happy_path\nemail=oasis7-e2e-001@test.invalid\nchain_private_key_hex={private_key}\nchain_public_key_hex={public_key}\noasis_sender_account_id=oc:pk:{public_key}\n\n## 2. other\nchain_private_key_hex={private_key}\nchain_public_key_hex={public_key}\noasis_sender_account_id=oc:pk:{public_key}\n"
        )
    }

    #[test]
    fn parse_persona_reads_expected_section() {
        let persona = parse_persona(sample_keys_file().as_str(), "happy_path").expect("persona");
        assert_eq!(persona.slug, "happy_path");
        assert_eq!(
            persona.account_id,
            format!("oc:pk:{}", persona.public_key_hex)
        );
        assert_eq!(persona.private_key_hex.len(), 64);
    }

    #[test]
    fn sign_request_verifies_with_runtime_auth() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "oasis7-chain-transfer-submit-client-{}.txt",
            std::process::id()
        ));
        fs::write(&path, sample_keys_file()).expect("write keys file");
        let config = CliConfig {
            command: Command::Sign,
            keys_file: path.clone(),
            persona: "happy_path".to_string(),
            to_account_id: "oc:bridge:test-route".to_string(),
            amount: 100,
            nonce: 42,
            memo: None,
            chain_base_url: None,
            timeout_ms: 5_000,
        };
        let request = build_signed_transfer_request(&config).expect("signed request");
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
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: request.client_request_id.clone(),
        };
        let proof = MainTokenActionAuthProof {
            scheme: MainTokenActionAuthScheme::Ed25519,
            account_id: request.from_account_id.clone(),
            public_key: Some(request.public_key.clone()),
            signature: Some(request.signature.clone()),
            threshold: None,
            participant_signatures: Vec::new(),
        };
        verify_main_token_runtime_action_auth(&action, &proof).expect("verify signature");
        assert!(request.signature.starts_with("octransferauth:v1:"));
        assert!(
            !serde_json::to_string(&request)
                .expect("request json")
                .contains("chain_private_key_hex")
        );
        let memo_config = CliConfig {
            command: Command::Sign,
            keys_file: path.clone(),
            persona: "happy_path".to_string(),
            to_account_id: "oc:bridge:test-route".to_string(),
            amount: 100,
            nonce: 42,
            memo: Some("bridge:deposit:route-000001".to_string()),
            chain_base_url: None,
            timeout_ms: 5_000,
        };
        let memo_request =
            build_signed_transfer_request(&memo_config).expect("memo signed request");
        let memo_action = Action::TransferMainToken {
            from_account_id: memo_request.from_account_id.clone(),
            to_account_id: memo_request.to_account_id.clone(),
            amount: memo_request.amount,
            nonce: memo_request.nonce,
            asset_id: memo_request.asset_id.clone(),
            memo: memo_request.memo.clone(),
            chain_id: memo_request.chain_id.clone(),
            network_id: memo_request.network_id.clone(),
            tx_version: memo_request.tx_version,
            tx_type: memo_request.tx_type.clone(),
            valid_until_unix_ms: memo_request.valid_until_unix_ms,
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: memo_request.client_request_id.clone(),
        };
        let memo_proof = MainTokenActionAuthProof {
            scheme: MainTokenActionAuthScheme::Ed25519,
            account_id: memo_request.from_account_id.clone(),
            public_key: Some(memo_request.public_key.clone()),
            signature: Some(memo_request.signature.clone()),
            threshold: None,
            participant_signatures: Vec::new(),
        };
        verify_main_token_runtime_action_auth(&memo_action, &memo_proof)
            .expect("verify memo signature");
        assert_eq!(
            memo_request.memo.as_deref(),
            Some("bridge:deposit:route-000001")
        );
        assert_eq!(memo_request.tx_version, Some(2));
        assert_eq!(memo_request.tx_type.as_deref(), Some("asset_transfer"));
        assert!(memo_request.signature.starts_with("octransferauth:v2:"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parser_rejects_account_public_key_mismatch() {
        let private_key = "a".repeat(64);
        let public_key = "b".repeat(64);
        let account_key = "c".repeat(64);
        let bad = format!(
            "## 1. happy_path\nchain_private_key_hex={private_key}\nchain_public_key_hex={public_key}\noasis_sender_account_id=oc:pk:{account_key}\n"
        );
        let err = parse_persona(bad.as_str(), "happy_path").expect_err("mismatch");
        assert!(err.contains("does not match"));
    }

    #[test]
    fn parser_accepts_mixed_case_hex_and_canonicalizes_account_id() {
        let seed = [7_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let private_key = hex::encode(seed).to_ascii_uppercase();
        let public_key = hex::encode(signing_key.verifying_key().to_bytes()).to_ascii_uppercase();
        let keys = format!(
            "## 1. happy_path\nchain_private_key_hex={private_key}\nchain_public_key_hex={public_key}\noasis_sender_account_id=oc:pk:{public_key}\n"
        );
        let persona = parse_persona(keys.as_str(), "happy_path").expect("persona");
        assert_eq!(persona.private_key_hex, private_key.to_ascii_lowercase());
        assert_eq!(persona.public_key_hex, public_key.to_ascii_lowercase());
        assert_eq!(
            persona.account_id,
            format!("oc:pk:{}", public_key.to_ascii_lowercase())
        );
    }

    #[test]
    fn parse_clamps_zero_timeout_to_one_ms() {
        let config = CliConfig::parse([
            "sign".to_string(),
            "--keys-file".to_string(),
            "keys.txt".to_string(),
            "--persona".to_string(),
            "happy_path".to_string(),
            "--to-account-id".to_string(),
            "oc:bridge:test-route".to_string(),
            "--amount".to_string(),
            "100".to_string(),
            "--nonce".to_string(),
            "1".to_string(),
            "--memo".to_string(),
            " bridge:deposit:route-000001 ".to_string(),
            "--timeout-ms".to_string(),
            "0".to_string(),
        ])
        .expect("config");
        assert_eq!(config.timeout_ms, 1);
        assert_eq!(config.memo.as_deref(), Some("bridge:deposit:route-000001"));
    }
}
