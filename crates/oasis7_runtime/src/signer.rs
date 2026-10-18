//! Receipt signing and verification.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};

use super::effect::{
    EffectReceipt, ReceiptParticipantSignature, ReceiptSignature, SignatureAlgorithm,
};
use super::error::WorldError;
use super::util::sha256_hex;

type HmacSha256 = Hmac<Sha256>;
const RECEIPT_SIG_ED25519_PREFIX: &str = "receiptsig:ed25519:v1:";
const RECEIPT_SIG_THRESHOLD_ED25519_PREFIX: &str = "receiptsig:threshold-ed25519:v1:";

/// Signer for creating and verifying receipt signatures.
#[derive(Debug, Clone)]
pub struct ReceiptSigner {
    mode: ReceiptSignerMode,
}

#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "Signer modes retain their existing private representation; boxing would add churn without changing runtime behavior."
)]
enum ReceiptSignerMode {
    HmacSha256 {
        key: Vec<u8>,
    },
    Ed25519 {
        signer_node_id: String,
        signing_key: SigningKey,
    },
    ThresholdEd25519 {
        threshold: u16,
        signer_keys: BTreeMap<String, SigningKey>,
    },
}

impl ReceiptSigner {
    pub fn hmac_sha256(key: impl Into<Vec<u8>>) -> Self {
        Self {
            mode: ReceiptSignerMode::HmacSha256 { key: key.into() },
        }
    }

    pub fn ed25519(
        signer_node_id: impl Into<String>,
        signer_private_key_hex: &str,
    ) -> Result<Self, WorldError> {
        let signer_node_id = signer_node_id.into();
        if signer_node_id.trim().is_empty() {
            return Err(WorldError::SignatureKeyInvalid);
        }
        let signing_key = signing_key_from_hex(signer_private_key_hex, "receipt private key")?;
        Ok(Self {
            mode: ReceiptSignerMode::Ed25519 {
                signer_node_id,
                signing_key,
            },
        })
    }

    pub fn threshold_ed25519(
        threshold: u16,
        signer_private_keys_hex: BTreeMap<String, String>,
    ) -> Result<Self, WorldError> {
        if threshold < 2 {
            return Err(WorldError::SignatureKeyInvalid);
        }
        if signer_private_keys_hex.len() < threshold as usize {
            return Err(WorldError::SignatureKeyInvalid);
        }
        let mut signer_keys = BTreeMap::new();
        for (node_id, private_key_hex) in signer_private_keys_hex {
            if node_id.trim().is_empty() {
                return Err(WorldError::SignatureKeyInvalid);
            }
            let signing_key =
                signing_key_from_hex(private_key_hex.as_str(), "threshold receipt private key")?;
            signer_keys.insert(node_id, signing_key);
        }
        Ok(Self {
            mode: ReceiptSignerMode::ThresholdEd25519 {
                threshold,
                signer_keys,
            },
        })
    }

    pub fn sign(
        &self,
        receipt: &EffectReceipt,
        consensus_height: u64,
        receipts_root: &str,
    ) -> Result<ReceiptSignature, WorldError> {
        match &self.mode {
            ReceiptSignerMode::HmacSha256 { key } => {
                let bytes = receipt_signing_bytes(
                    receipt,
                    consensus_height,
                    receipts_root,
                    None,
                    None,
                    &[],
                )?;
                let mut mac =
                    HmacSha256::new_from_slice(key).map_err(|_| WorldError::SignatureKeyInvalid)?;
                mac.update(&bytes);
                let signature = mac.finalize().into_bytes();
                Ok(ReceiptSignature {
                    algorithm: SignatureAlgorithm::HmacSha256,
                    signature_hex: hex::encode(signature),
                    signer_node_id: None,
                    threshold: None,
                    participants: Vec::new(),
                    consensus_height: Some(consensus_height),
                    receipts_root: Some(receipts_root.to_string()),
                    participant_signatures: Vec::new(),
                })
            }
            ReceiptSignerMode::Ed25519 {
                signer_node_id,
                signing_key,
            } => self.sign_ed25519(
                receipt,
                signer_node_id.as_str(),
                signing_key,
                consensus_height,
                receipts_root,
            ),
            ReceiptSignerMode::ThresholdEd25519 {
                threshold,
                signer_keys,
            } => self.sign_threshold_ed25519(
                receipt,
                *threshold,
                signer_keys,
                consensus_height,
                receipts_root,
            ),
        }
    }

    pub fn verify(
        &self,
        receipt: &EffectReceipt,
        signature: &ReceiptSignature,
        trusted_signers: &BTreeMap<String, String>,
        expected_consensus_height: u64,
        expected_receipts_root: &str,
    ) -> Result<(), WorldError> {
        if signature.algorithm != self.algorithm() {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        if signature.consensus_height != Some(expected_consensus_height) {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        if signature.receipts_root.as_deref() != Some(expected_receipts_root) {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        match &self.mode {
            ReceiptSignerMode::HmacSha256 { .. } => {
                let expected =
                    self.sign(receipt, expected_consensus_height, expected_receipts_root)?;
                if signature.signature_hex != expected.signature_hex {
                    return Err(WorldError::ReceiptSignatureInvalid {
                        intent_id: receipt.intent_id.clone(),
                    });
                }
                Ok(())
            }
            ReceiptSignerMode::Ed25519 { .. } => self.verify_ed25519(
                receipt,
                signature,
                trusted_signers,
                expected_consensus_height,
                expected_receipts_root,
            ),
            ReceiptSignerMode::ThresholdEd25519 { threshold, .. } => self.verify_threshold_ed25519(
                receipt,
                signature,
                trusted_signers,
                expected_consensus_height,
                expected_receipts_root,
                *threshold,
            ),
        }
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        match self.mode {
            ReceiptSignerMode::HmacSha256 { .. } => SignatureAlgorithm::HmacSha256,
            ReceiptSignerMode::Ed25519 { .. } => SignatureAlgorithm::Ed25519,
            ReceiptSignerMode::ThresholdEd25519 { .. } => SignatureAlgorithm::ThresholdEd25519,
        }
    }

    fn sign_ed25519(
        &self,
        receipt: &EffectReceipt,
        signer_node_id: &str,
        signing_key: &SigningKey,
        consensus_height: u64,
        receipts_root: &str,
    ) -> Result<ReceiptSignature, WorldError> {
        let payload = receipt_signing_bytes(
            receipt,
            consensus_height,
            receipts_root,
            Some(signer_node_id),
            None,
            &[],
        )?;
        let signature = signing_key.sign(payload.as_slice());
        Ok(ReceiptSignature {
            algorithm: SignatureAlgorithm::Ed25519,
            signature_hex: format!(
                "{RECEIPT_SIG_ED25519_PREFIX}{}",
                hex::encode(signature.to_bytes())
            ),
            signer_node_id: Some(signer_node_id.to_string()),
            threshold: None,
            participants: Vec::new(),
            consensus_height: Some(consensus_height),
            receipts_root: Some(receipts_root.to_string()),
            participant_signatures: Vec::new(),
        })
    }

    fn sign_threshold_ed25519(
        &self,
        receipt: &EffectReceipt,
        threshold: u16,
        signer_keys: &BTreeMap<String, SigningKey>,
        consensus_height: u64,
        receipts_root: &str,
    ) -> Result<ReceiptSignature, WorldError> {
        let participants: Vec<String> = signer_keys
            .keys()
            .take(threshold as usize)
            .cloned()
            .collect();
        if participants.len() < threshold as usize {
            return Err(WorldError::SignatureKeyInvalid);
        }

        let mut participant_signatures = Vec::with_capacity(participants.len());
        for signer_node_id in &participants {
            let Some(signing_key) = signer_keys.get(signer_node_id) else {
                return Err(WorldError::SignatureKeyInvalid);
            };
            let payload = receipt_signing_bytes(
                receipt,
                consensus_height,
                receipts_root,
                Some(signer_node_id.as_str()),
                Some(threshold),
                participants.as_slice(),
            )?;
            let signature = signing_key.sign(payload.as_slice());
            participant_signatures.push(ReceiptParticipantSignature {
                signer_node_id: signer_node_id.clone(),
                signature_hex: format!(
                    "{RECEIPT_SIG_ED25519_PREFIX}{}",
                    hex::encode(signature.to_bytes())
                ),
            });
        }
        let commitment = threshold_signature_commitment(participant_signatures.as_slice());
        Ok(ReceiptSignature {
            algorithm: SignatureAlgorithm::ThresholdEd25519,
            signature_hex: format!("{RECEIPT_SIG_THRESHOLD_ED25519_PREFIX}{commitment}"),
            signer_node_id: None,
            threshold: Some(threshold),
            participants,
            consensus_height: Some(consensus_height),
            receipts_root: Some(receipts_root.to_string()),
            participant_signatures,
        })
    }

    fn verify_ed25519(
        &self,
        receipt: &EffectReceipt,
        signature: &ReceiptSignature,
        trusted_signers: &BTreeMap<String, String>,
        consensus_height: u64,
        receipts_root: &str,
    ) -> Result<(), WorldError> {
        let signer_node_id = signature.signer_node_id.as_deref().ok_or_else(|| {
            WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            }
        })?;
        verify_single_ed25519_signature(
            receipt,
            signer_node_id,
            signature.signature_hex.as_str(),
            trusted_signers,
            consensus_height,
            receipts_root,
            None,
            &[],
        )
    }

    fn verify_threshold_ed25519(
        &self,
        receipt: &EffectReceipt,
        signature: &ReceiptSignature,
        trusted_signers: &BTreeMap<String, String>,
        consensus_height: u64,
        receipts_root: &str,
        configured_threshold: u16,
    ) -> Result<(), WorldError> {
        let threshold = signature
            .threshold
            .ok_or_else(|| WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            })?;
        if threshold != configured_threshold || threshold < 2 {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        if signature.participants.len() < threshold as usize {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        if signature.participant_signatures.len() < threshold as usize {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }

        let mut participant_set = BTreeSet::new();
        for participant in &signature.participants {
            if participant.trim().is_empty() || !participant_set.insert(participant.clone()) {
                return Err(WorldError::ReceiptSignatureInvalid {
                    intent_id: receipt.intent_id.clone(),
                });
            }
        }
        let Some(commitment_hex) = signature
            .signature_hex
            .strip_prefix(RECEIPT_SIG_THRESHOLD_ED25519_PREFIX)
        else {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        };
        let expected_commitment = threshold_signature_commitment(&signature.participant_signatures);
        if commitment_hex != expected_commitment {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }

        let mut valid = 0_usize;
        let mut used_signers = BTreeSet::new();
        for participant_signature in &signature.participant_signatures {
            if !participant_set.contains(participant_signature.signer_node_id.as_str()) {
                continue;
            }
            if !used_signers.insert(participant_signature.signer_node_id.clone()) {
                continue;
            }
            verify_single_ed25519_signature(
                receipt,
                participant_signature.signer_node_id.as_str(),
                participant_signature.signature_hex.as_str(),
                trusted_signers,
                consensus_height,
                receipts_root,
                Some(threshold),
                signature.participants.as_slice(),
            )?;
            valid += 1;
        }
        if valid < threshold as usize {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Receipt signature verification keeps the stable serialized preimage and verification context together."
)]
fn verify_single_ed25519_signature(
    receipt: &EffectReceipt,
    signer_node_id: &str,
    signature_with_prefix: &str,
    trusted_signers: &BTreeMap<String, String>,
    consensus_height: u64,
    receipts_root: &str,
    threshold: Option<u16>,
    participants: &[String],
) -> Result<(), WorldError> {
    let signer_public_key =
        trusted_signers
            .get(signer_node_id)
            .ok_or_else(|| WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            })?;
    let signature_hex = signature_with_prefix
        .strip_prefix(RECEIPT_SIG_ED25519_PREFIX)
        .ok_or_else(|| WorldError::ReceiptSignatureInvalid {
            intent_id: receipt.intent_id.clone(),
        })?;
    let payload = receipt_signing_bytes(
        receipt,
        consensus_height,
        receipts_root,
        Some(signer_node_id),
        threshold,
        participants,
    )?;
    let public_key_bytes = decode_hex_array::<32>(signer_public_key)?;
    let signature_bytes = decode_hex_array::<64>(signature_hex)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|_| {
        WorldError::ReceiptSignatureInvalid {
            intent_id: receipt.intent_id.clone(),
        }
    })?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(payload.as_slice(), &signature)
        .map_err(|_| WorldError::ReceiptSignatureInvalid {
            intent_id: receipt.intent_id.clone(),
        })
}

fn threshold_signature_commitment(
    participant_signatures: &[ReceiptParticipantSignature],
) -> String {
    let mut entries: Vec<&ReceiptParticipantSignature> = participant_signatures.iter().collect();
    entries.sort_by(|left, right| {
        threshold_signature_entry_bytes(left).cmp(threshold_signature_entry_bytes(right))
    });

    let payload_len = entries
        .iter()
        .map(|entry| entry.signer_node_id.len() + 1 + entry.signature_hex.len())
        .sum::<usize>()
        + entries.len().saturating_sub(1);
    let mut payload = String::with_capacity(payload_len);
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            payload.push('|');
        }
        payload.push_str(entry.signer_node_id.as_str());
        payload.push(':');
        payload.push_str(entry.signature_hex.as_str());
    }

    sha256_hex(payload.as_bytes())
}

fn threshold_signature_entry_bytes(
    entry: &ReceiptParticipantSignature,
) -> impl Iterator<Item = u8> + '_ {
    entry
        .signer_node_id
        .bytes()
        .chain(std::iter::once(b':'))
        .chain(entry.signature_hex.bytes())
}

fn receipt_signing_bytes(
    receipt: &EffectReceipt,
    consensus_height: u64,
    receipts_root: &str,
    signer_node_id: Option<&str>,
    threshold: Option<u16>,
    participants: &[String],
) -> Result<Vec<u8>, WorldError> {
    match (
        receipt.birth_era,
        receipt.world.as_ref(),
        receipt.intent_payload_digest.as_deref(),
    ) {
        (None, None, None) => {}
        (Some(era), Some(world), Some(digest)) => {
            world
                .validate()
                .map_err(|error| WorldError::ResourceBalanceInvalid {
                    reason: error.to_string(),
                })?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(WorldError::ReceiptSignatureInvalid {
                    intent_id: receipt.intent_id.clone(),
                });
            }
            return Ok(serde_json::to_vec(&(
                "oasis7.world.effect.receipt.v2",
                world,
                era,
                receipt.intent_id.as_str(),
                digest,
                receipt.status.as_str(),
                &receipt.payload,
                receipt.cost_cents,
                consensus_height,
                receipts_root,
                signer_node_id,
                threshold,
                participants,
            ))?);
        }
        _ => {
            return Err(WorldError::ReceiptSignatureInvalid {
                intent_id: receipt.intent_id.clone(),
            });
        }
    }
    #[derive(Serialize)]
    struct ReceiptPayload<'a> {
        intent_id: &'a str,
        status: &'a str,
        payload: &'a JsonValue,
        cost_cents: Option<u64>,
        consensus_height: u64,
        receipts_root: &'a str,
        signer_node_id: Option<&'a str>,
        threshold: Option<u16>,
        participants: &'a [String],
    }

    let payload = ReceiptPayload {
        intent_id: &receipt.intent_id,
        status: &receipt.status,
        payload: &receipt.payload,
        cost_cents: receipt.cost_cents,
        consensus_height,
        receipts_root,
        signer_node_id,
        threshold,
        participants,
    };

    Ok(serde_json::to_vec(&payload)?)
}

fn signing_key_from_hex(private_key_hex: &str, _label: &str) -> Result<SigningKey, WorldError> {
    let private_key_bytes = decode_hex_array::<32>(private_key_hex)?;
    Ok(SigningKey::from_bytes(&private_key_bytes))
}

fn decode_hex_array<const N: usize>(raw: &str) -> Result<[u8; N], WorldError> {
    let bytes = hex::decode(raw).map_err(|_| WorldError::SignatureKeyInvalid)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| WorldError::SignatureKeyInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_signature_commitment_matches_legacy_payload_order() {
        let participant_signatures = vec![
            ReceiptParticipantSignature {
                signer_node_id: "node.10".to_string(),
                signature_hex: "receiptsig:ed25519:v1:cccc".to_string(),
            },
            ReceiptParticipantSignature {
                signer_node_id: "node.1".to_string(),
                signature_hex: "receiptsig:ed25519:v1:aaaa".to_string(),
            },
            ReceiptParticipantSignature {
                signer_node_id: "node.1".to_string(),
                signature_hex: "receiptsig:ed25519:v1:bbbb".to_string(),
            },
        ];

        let optimized = threshold_signature_commitment(&participant_signatures);
        let legacy = legacy_threshold_signature_commitment(&participant_signatures);

        assert_eq!(optimized, legacy);
    }

    fn legacy_threshold_signature_commitment(
        participant_signatures: &[ReceiptParticipantSignature],
    ) -> String {
        let mut entries: Vec<String> = participant_signatures
            .iter()
            .map(|entry| format!("{}:{}", entry.signer_node_id, entry.signature_hex))
            .collect();
        entries.sort();
        sha256_hex(entries.join("|").as_bytes())
    }
}
