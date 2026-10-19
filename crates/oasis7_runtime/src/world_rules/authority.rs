//! Shared verification for world-rule facts certified by the live finality set.

use crate::{WorldError, WorldState};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_agent_api::world_action::GovernanceFinalityCertificate;
use std::collections::{BTreeMap, BTreeSet};

/// Verify a full finality certificate against the current trusted validator
/// registry and node identity bindings. Callers bind domain-specific facts in
/// `manifest_hash` and enforce purpose-specific replay/expiry rules separately.
pub(crate) fn verify_world_rule_authority(
    state: &WorldState,
    manifest_hash: &str,
    consensus_height: u64,
    certificate: &GovernanceFinalityCertificate,
) -> Result<(), WorldError> {
    let registry = state
        .governance_finality_signer_registry
        .as_ref()
        .ok_or_else(|| invalid("world_rule_authority_registry_missing"))?;
    if registry.signer_bindings.is_empty()
        || registry.threshold == 0
        || usize::from(registry.threshold) > registry.signer_bindings.len()
        || !(1..=10_000).contains(&registry.threshold_bps)
    {
        return Err(invalid("world_rule_authority_registry_invalid"));
    }
    if certificate.manifest_hash != manifest_hash
        || certificate.consensus_height != consensus_height
        || certificate.consensus_height == 0
    {
        return Err(invalid("world_rule_authority_payload_mismatch"));
    }

    if !(1..=10_000).contains(&certificate.threshold_bps) {
        return Err(invalid("world_rule_authority_threshold_bps_invalid"));
    }
    let signers = registry.signer_bindings.keys().cloned().collect::<Vec<_>>();
    let threshold_bps = registry.threshold_bps;
    let validator_set_hash = crate::util::sha256_hex(signers.join("|").as_bytes());
    let mut decoded_registry_keys = BTreeSet::new();
    let mut total_stake = 0u128;
    for node_id in &signers {
        let encoded = registry.signer_bindings.get(node_id).expect("registry key");
        let public_key: [u8; 32] = hex::decode(encoded)
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?
            .try_into()
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?;
        VerifyingKey::from_bytes(&public_key)
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?;
        if !decoded_registry_keys.insert(public_key) {
            return Err(invalid("world_rule_authority_registry_duplicate_key"));
        }
        if state.node_identity_bindings.get(node_id) != Some(encoded) {
            return Err(invalid("world_rule_authority_signer_untrusted"));
        }
        let stake = registry
            .validator_stakes
            .get(node_id)
            .copied()
            .filter(|stake| *stake > 0)
            .ok_or_else(|| invalid("world_rule_authority_stake_invalid"))?;
        total_stake = total_stake
            .checked_add(u128::from(stake))
            .ok_or_else(|| invalid("world_rule_authority_stake_overflow"))?;
    }
    let stake_root = finality_stake_root(&signers, &registry.validator_stakes)?;
    if certificate.validator_set_hash != validator_set_hash
        || certificate.stake_root != stake_root
        || certificate.threshold_bps != threshold_bps
        || certificate.effective_min_unique_signers() != registry.threshold
        || certificate.signatures.len() < usize::from(registry.threshold)
    {
        return Err(invalid("world_rule_authority_threshold_or_set_mismatch"));
    }

    if total_stake == 0 {
        return Err(invalid("world_rule_authority_zero_stake"));
    }
    let mut signed_stake = 0u128;
    let mut decoded_signing_keys = BTreeSet::new();
    for (node_id, encoded_signature) in &certificate.signatures {
        let public_key_hex = registry
            .signer_bindings
            .get(node_id)
            .filter(|key| state.node_identity_bindings.get(node_id) == Some(key))
            .ok_or_else(|| invalid("world_rule_authority_signer_untrusted"))?;
        let signature_hex = encoded_signature
            .strip_prefix(GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1)
            .ok_or_else(|| invalid("world_rule_authority_signature_prefix_invalid"))?;
        let public_key: [u8; 32] = hex::decode(public_key_hex)
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?
            .try_into()
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?;
        if !decoded_signing_keys.insert(public_key) {
            return Err(invalid("world_rule_authority_duplicate_signing_key"));
        }
        let signature: [u8; 64] = hex::decode(signature_hex)
            .map_err(|_| invalid("world_rule_authority_signature_invalid"))?
            .try_into()
            .map_err(|_| invalid("world_rule_authority_signature_invalid"))?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| invalid("world_rule_authority_public_key_invalid"))?;
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            certificate.proposal_id,
            manifest_hash,
            certificate.consensus_height,
            certificate.epoch_id,
            &certificate.validator_set_hash,
            &certificate.stake_root,
            certificate.threshold_bps,
            certificate.effective_min_unique_signers(),
            node_id,
        );
        verifying_key
            .verify(&payload, &Signature::from_bytes(&signature))
            .map_err(|_| invalid("world_rule_authority_signature_invalid"))?;
        let stake = registry
            .validator_stakes
            .get(node_id)
            .copied()
            .filter(|stake| *stake > 0)
            .ok_or_else(|| invalid("world_rule_authority_stake_invalid"))?;
        signed_stake = signed_stake
            .checked_add(u128::from(stake))
            .ok_or_else(|| invalid("world_rule_authority_stake_overflow"))?;
    }
    let signed_bps_numerator = signed_stake
        .checked_mul(10_000)
        .ok_or_else(|| invalid("world_rule_authority_stake_overflow"))?;
    let required_bps_numerator = total_stake
        .checked_mul(u128::from(threshold_bps))
        .ok_or_else(|| invalid("world_rule_authority_stake_overflow"))?;
    if signed_bps_numerator < required_bps_numerator {
        return Err(invalid("world_rule_authority_stake_threshold_not_met"));
    }
    Ok(())
}

fn finality_stake_root(
    signers: &[String],
    stakes: &BTreeMap<String, u64>,
) -> Result<String, WorldError> {
    let mut payload = Vec::with_capacity(signers.len());
    for node_id in signers {
        let stake = stakes
            .get(node_id)
            .copied()
            .filter(|stake| *stake > 0)
            .ok_or_else(|| invalid("world_rule_authority_stake_invalid"))?;
        payload.push(format!("{node_id}:{stake}"));
    }
    Ok(crate::util::sha256_hex(payload.join("|").as_bytes()))
}

fn invalid(reason: &str) -> WorldError {
    WorldError::GovernanceFinalityInvalid {
        reason: reason.to_owned(),
    }
}
