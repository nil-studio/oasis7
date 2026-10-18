use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::util::sha256_hex;

const DEFAULT_POINTS_PER_CREDIT: u64 = 10;
const DEFAULT_CREDITS_PER_POWER_UNIT: u64 = 1;
const DEFAULT_MAX_REDEEM_POWER_PER_EPOCH: i64 = 10_000;
const DEFAULT_MIN_REDEEM_POWER_UNIT: i64 = 1;
pub const REWARD_MINT_SIGNATURE_V1_PREFIX: &str = "mintsig:v1:";
pub const REWARD_MINT_SIGNATURE_V2_PREFIX: &str = "mintsig:v2:";
pub const REWARD_REDEEM_SIGNATURE_V1_PREFIX: &str = "redeemsig:v1:";

/// Redeemable reward asset configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardAssetConfig {
    pub points_per_credit: u64,
    pub credits_per_power_unit: u64,
    pub max_redeem_power_per_epoch: i64,
    pub min_redeem_power_unit: i64,
}

impl Default for RewardAssetConfig {
    fn default() -> Self {
        Self {
            points_per_credit: DEFAULT_POINTS_PER_CREDIT,
            credits_per_power_unit: DEFAULT_CREDITS_PER_POWER_UNIT,
            max_redeem_power_per_epoch: DEFAULT_MAX_REDEEM_POWER_PER_EPOCH,
            min_redeem_power_unit: DEFAULT_MIN_REDEEM_POWER_UNIT,
        }
    }
}

/// Signature governance policy for reward settlement/redeem paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardSignatureGovernancePolicy {
    pub require_mintsig_v2: bool,
    pub allow_mintsig_v1_fallback: bool,
    pub require_redeem_signature: bool,
    #[serde(default)]
    pub require_redeem_signer_match_node_id: bool,
}

impl Default for RewardSignatureGovernancePolicy {
    fn default() -> Self {
        Self {
            require_mintsig_v2: false,
            allow_mintsig_v1_fallback: true,
            require_redeem_signature: false,
            require_redeem_signer_match_node_id: false,
        }
    }
}

/// Node-owned reward asset balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeAssetBalance {
    pub node_id: String,
    pub power_credit_balance: u64,
    pub total_minted_credits: u64,
    pub total_burned_credits: u64,
}

/// Protocol reserve for power redemptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProtocolPowerReserve {
    pub epoch_index: u64,
    pub available_power_units: i64,
    pub redeemed_power_units: i64,
}

pub use oasis7_agent_api::world_action::NodeRewardMintRecord;

/// Demand-side system order pool budget for one epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SystemOrderPoolBudget {
    pub epoch_index: u64,
    pub total_credit_budget: u64,
    pub remaining_credit_budget: u64,
    pub node_credit_caps: BTreeMap<String, u64>,
    pub node_credit_allocated: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RewardAssetInvariantViolation {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RewardAssetInvariantReport {
    pub total_nodes: usize,
    pub total_minted_credits: u64,
    pub total_burned_credits: u64,
    pub total_power_credit_balance: u64,
    pub mint_record_count: usize,
    pub violations: Vec<RewardAssetInvariantViolation>,
}

impl RewardAssetInvariantReport {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }
}

pub fn reward_mint_signature_v1(
    epoch_index: u64,
    node_id: &str,
    source_awarded_points: u64,
    minted_power_credits: u64,
    settlement_hash: &str,
    signer_node_id: &str,
    signer_public_key: &str,
) -> String {
    let payload = format!(
        "{epoch_index}|{node_id}|{source_awarded_points}|{minted_power_credits}|{settlement_hash}|{signer_node_id}|{signer_public_key}"
    );
    format!(
        "{REWARD_MINT_SIGNATURE_V1_PREFIX}{}",
        sha256_hex(payload.as_bytes())
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Signature construction preserves the established deterministic preimage argument order."
)]
pub fn reward_mint_signature_v2(
    epoch_index: u64,
    node_id: &str,
    source_awarded_points: u64,
    minted_power_credits: u64,
    settlement_hash: &str,
    signer_node_id: &str,
    signer_public_key: &str,
    signer_private_key_hex: &str,
) -> Result<String, String> {
    let signing_key = signing_key_from_hex(signer_private_key_hex, "reward mint private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key,
        "reward mint signer public key",
    )?;
    let payload = reward_mint_signing_payload_v2(
        epoch_index,
        node_id,
        source_awarded_points,
        minted_power_credits,
        settlement_hash,
        signer_node_id,
        signer_public_key,
    );
    let signature: Signature = signing_key.sign(payload.as_slice());
    Ok(format!(
        "{REWARD_MINT_SIGNATURE_V2_PREFIX}{}",
        hex::encode(signature.to_bytes())
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Signature verification preserves the established deterministic preimage argument order."
)]
pub fn verify_reward_mint_signature_v2(
    signature: &str,
    epoch_index: u64,
    node_id: &str,
    source_awarded_points: u64,
    minted_power_credits: u64,
    settlement_hash: &str,
    signer_node_id: &str,
    signer_public_key: &str,
) -> Result<(), String> {
    let signature_hex = signature
        .strip_prefix(REWARD_MINT_SIGNATURE_V2_PREFIX)
        .ok_or_else(|| "reward mint signature is not mintsig:v2".to_string())?;
    let public_key_bytes = decode_hex_array::<32>(signer_public_key, "reward mint public key")?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "reward mint signature")?;

    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse reward mint public key failed: {err}"))?;
    let payload = reward_mint_signing_payload_v2(
        epoch_index,
        node_id,
        source_awarded_points,
        minted_power_credits,
        settlement_hash,
        signer_node_id,
        signer_public_key,
    );
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(payload.as_slice(), &signature)
        .map_err(|err| format!("verify reward mint signature failed: {err}"))
}

pub fn reward_redeem_signature_v1(
    node_id: &str,
    target_agent_id: &str,
    redeem_credits: u64,
    nonce: u64,
    signer_node_id: &str,
    signer_public_key: &str,
    signer_private_key_hex: &str,
) -> Result<String, String> {
    let signing_key = signing_key_from_hex(signer_private_key_hex, "redeem private key")?;
    verify_keypair_match(&signing_key, signer_public_key, "redeem signer public key")?;
    let payload = reward_redeem_signing_payload_v1(
        node_id,
        target_agent_id,
        redeem_credits,
        nonce,
        signer_node_id,
        signer_public_key,
    );
    let signature: Signature = signing_key.sign(payload.as_slice());
    Ok(format!(
        "{REWARD_REDEEM_SIGNATURE_V1_PREFIX}{}",
        hex::encode(signature.to_bytes())
    ))
}

pub fn verify_reward_redeem_signature_v1(
    signature: &str,
    node_id: &str,
    target_agent_id: &str,
    redeem_credits: u64,
    nonce: u64,
    signer_node_id: &str,
    signer_public_key: &str,
) -> Result<(), String> {
    let signature_hex = signature
        .strip_prefix(REWARD_REDEEM_SIGNATURE_V1_PREFIX)
        .ok_or_else(|| "redeem signature is not redeemsig:v1".to_string())?;
    let public_key_bytes = decode_hex_array::<32>(signer_public_key, "redeem public key")?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "redeem signature")?;

    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse redeem public key failed: {err}"))?;
    let payload = reward_redeem_signing_payload_v1(
        node_id,
        target_agent_id,
        redeem_credits,
        nonce,
        signer_node_id,
        signer_public_key,
    );
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(payload.as_slice(), &signature)
        .map_err(|err| format!("verify redeem signature failed: {err}"))
}

fn reward_mint_signing_payload_v2(
    epoch_index: u64,
    node_id: &str,
    source_awarded_points: u64,
    minted_power_credits: u64,
    settlement_hash: &str,
    signer_node_id: &str,
    signer_public_key: &str,
) -> Vec<u8> {
    format!(
        "mintsig:v2|{epoch_index}|{node_id}|{source_awarded_points}|{minted_power_credits}|{settlement_hash}|{signer_node_id}|{signer_public_key}"
    )
    .into_bytes()
}

fn reward_redeem_signing_payload_v1(
    node_id: &str,
    target_agent_id: &str,
    redeem_credits: u64,
    nonce: u64,
    signer_node_id: &str,
    signer_public_key: &str,
) -> Vec<u8> {
    format!(
        "redeemsig:v1|{node_id}|{target_agent_id}|{redeem_credits}|{nonce}|{signer_node_id}|{signer_public_key}"
    )
    .into_bytes()
}

fn signing_key_from_hex(private_key_hex: &str, label: &str) -> Result<SigningKey, String> {
    let private_key_bytes = decode_hex_array::<32>(private_key_hex, label)?;
    Ok(SigningKey::from_bytes(&private_key_bytes))
}

fn verify_keypair_match(
    signing_key: &SigningKey,
    signer_public_key: &str,
    label: &str,
) -> Result<(), String> {
    let expected_public_key = hex::encode(signing_key.verifying_key().to_bytes());
    if expected_public_key != signer_public_key {
        return Err(format!(
            "{label} does not match private key: expected={} actual={}",
            expected_public_key, signer_public_key
        ));
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|_| format!("{label} must be valid hex"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be {N}-byte hex"))
}
