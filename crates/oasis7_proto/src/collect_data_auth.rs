use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::Serialize;

pub const COLLECT_DATA_PREFLIGHT_OPERATION: &str = "collect_data_preflight";
pub const COLLECT_DATA_SUBMIT_OPERATION: &str = "collect_data_submit";
const AUTH_PAYLOAD_VERSION: u8 = 1;
const AUTH_SIGNATURE_PREFIX: &str = "awviewauth:v1:";

#[derive(Serialize)]
struct SigningEnvelope<'a> {
    version: u8,
    payload: CollectDataSigningPayload<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<&'a str>,
}

#[derive(Serialize)]
struct CollectDataSigningPayload<'a> {
    operation: &'a str,
    electricity_cost: i64,
    data_amount: i64,
    player_id: &'a str,
    public_key: &'a str,
    nonce: u64,
}

pub fn encode_authorization_payload(
    operation: &str,
    electricity_cost: i64,
    data_amount: i64,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(&SigningEnvelope {
        version: AUTH_PAYLOAD_VERSION,
        payload: CollectDataSigningPayload {
            operation,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
        },
        actor: None,
    })
    .map_err(|err| format!("encode auth payload failed: {err}"))
}

pub fn sign_authorization(
    operation: &str,
    electricity_cost: i64,
    data_amount: i64,
    player_id: &str,
    public_key: &str,
    nonce: u64,
    private_key_hex: &str,
) -> Result<String, String> {
    let private_key = decode_hex_array::<32>(private_key_hex, "collect_data private key")?;
    let signing_key = SigningKey::from_bytes(&private_key);
    if hex::encode(signing_key.verifying_key().to_bytes()) != public_key {
        return Err("collect_data public key does not match private key".to_string());
    }
    let payload = encode_authorization_payload(
        operation,
        electricity_cost,
        data_amount,
        player_id,
        public_key,
        nonce,
    )?;
    Ok(format!(
        "{AUTH_SIGNATURE_PREFIX}{}",
        hex::encode(signing_key.sign(payload.as_slice()).to_bytes())
    ))
}

pub fn verify_authorization(
    operation: &str,
    electricity_cost: i64,
    data_amount: i64,
    player_id: &str,
    public_key: &str,
    nonce: u64,
    signature: &str,
) -> Result<(), String> {
    let public_key = decode_hex_array::<32>(public_key, "collect_data public key")?;
    let signature = signature
        .strip_prefix(AUTH_SIGNATURE_PREFIX)
        .ok_or_else(|| "auth signature is not awviewauth:v1".to_string())?;
    let signature = decode_hex_array::<64>(signature, "collect_data signature")?;
    let payload = encode_authorization_payload(
        operation,
        electricity_cost,
        data_amount,
        player_id,
        hex::encode(public_key).as_str(),
        nonce,
    )?;
    VerifyingKey::from_bytes(&public_key)
        .map_err(|err| format!("parse auth public key failed: {err}"))?
        .verify(payload.as_slice(), &Signature::from_bytes(&signature))
        .map_err(|err| format!("verify auth signature failed: {err}"))
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|err| format!("decode {label} failed: {err}"))?;
    if bytes.len() != N {
        return Err(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        ));
    }
    let mut fixed = [0; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}
