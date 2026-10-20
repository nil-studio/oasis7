use super::{BTreeMap, FeedbackSubmitSigner, PosValidator, node_keypair_config};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};

pub(super) fn build_feedback_submit_signer(
    node_id: &str,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
) -> Result<FeedbackSubmitSigner, String> {
    let feedback_signer_id = format!("{node_id}-feedback-submit");
    let signer = derive_node_consensus_signer_keypair(feedback_signer_id.as_str(), root_keypair)?;
    Ok(FeedbackSubmitSigner {
        private_key_hex: signer.private_key_hex,
        public_key_hex: signer.public_key_hex,
    })
}

pub(super) fn derive_node_consensus_signer_keypair(
    node_id: &str,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
) -> Result<node_keypair_config::NodeKeypairConfig, String> {
    derive_node_scoped_keypair(
        node_id,
        root_keypair,
        b"oasis7-node-consensus-signer-v1",
        "node consensus signer",
    )
}

pub(super) fn derive_node_libp2p_identity_keypair_config(
    node_id: &str,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
) -> Result<node_keypair_config::NodeKeypairConfig, String> {
    derive_node_scoped_keypair(
        node_id,
        root_keypair,
        b"oasis7-node-libp2p-identity-v1",
        "node libp2p identity",
    )
}

pub(super) fn derive_node_scoped_keypair(
    node_id: &str,
    root_keypair: &node_keypair_config::NodeKeypairConfig,
    namespace: &[u8],
    label: &str,
) -> Result<node_keypair_config::NodeKeypairConfig, String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err(format!("{label} derivation requires non-empty node_id"));
    }
    let root_private_bytes = hex::decode(root_keypair.private_key_hex.as_str())
        .map_err(|_| "root node.private_key must be valid hex".to_string())?;
    let root_private: [u8; 32] = root_private_bytes
        .try_into()
        .map_err(|_| "root node.private_key must be 32-byte hex".to_string())?;

    let mut hasher = Sha256::new();
    hasher.update(namespace);
    hasher.update(root_private);
    hasher.update(b"|");
    hasher.update(node_id.as_bytes());
    let digest = hasher.finalize();

    let mut derived_private = [0_u8; 32];
    derived_private.copy_from_slice(&digest[..32]);
    let signing_key = SigningKey::from_bytes(&derived_private);
    Ok(node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    })
}

pub(super) fn build_validator_signer_public_keys(
    validators: &[PosValidator],
    root_keypair: &node_keypair_config::NodeKeypairConfig,
    explicit_bindings: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut bindings = BTreeMap::new();
    for validator in validators {
        let validator_id = validator.validator_id.trim();
        if validator_id.is_empty() {
            return Err("validator_id cannot be empty when deriving signer bindings".to_string());
        }
        if let Some(public_key_hex) = explicit_bindings.get(validator_id) {
            bindings.insert(validator_id.to_string(), public_key_hex.clone());
            continue;
        }
        let keypair = derive_node_consensus_signer_keypair(validator_id, root_keypair)?;
        bindings.insert(validator_id.to_string(), keypair.public_key_hex);
    }
    for validator_id in explicit_bindings.keys() {
        if !validators
            .iter()
            .any(|validator| validator.validator_id.trim() == validator_id)
        {
            return Err(format!(
                "validator signer override references unknown validator: {validator_id}"
            ));
        }
    }
    Ok(bindings)
}
