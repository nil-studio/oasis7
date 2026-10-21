//! Authenticate an existing preview identity, without initializing or recovering history.
use super::*;

/// The service pin must come from the existing node key, never the receipt itself.
/// This verifies origin identity only. It does not replace `verify_at`, authenticate
/// a current head, or waive the initial-publication archive required for recovery.
pub(crate) fn verify_existing_origin(
    root: &Path,
    world_id: &str,
    node_id: &str,
    service_public: &str,
) -> Result<WorldIdentity, String> {
    files::normalized(root)?;
    if root
        .canonicalize()
        .map_err(|_| "preview origin unavailable")?
        != root
    {
        return Err("preview origin must be canonical and absolute".into());
    }
    for relative in [
        "initialization-receipt.json",
        "genesis/initial-artifact.json",
        "world/world-service-identity.json",
    ] {
        files::normalized(&root.join(relative))?;
    }
    let receipt: Receipt = serde_json::from_slice(&files::read_bounded(
        &root.join("initialization-receipt.json"),
        256 * 1024,
    )?)
    .map_err(|_| "signed preview origin receipt required")?;
    receipt
        .world
        .validate()
        .map_err(|_| "invalid preview origin world")?;
    if receipt.schema != SCHEMA
        || receipt.claim != CLAIM
        || receipt.storage_profile != "dev_local"
        || receipt.world.world_id != world_id
        || receipt.node_id != node_id
        || receipt.service_public_key != service_public
        || (receipt.receipt_schema_hash != blake3_hex(RECEIPT_SCHEMA.as_bytes())
            && receipt.receipt_schema_hash != blake3_hex(LEGACY_RECEIPT_SCHEMA.as_bytes()))
    {
        return Err("preview origin scope or independent service pin mismatch".into());
    }
    let public: [u8; 32] = hex::decode(service_public)
        .map_err(|_| "invalid preview origin pin")?
        .try_into()
        .map_err(|_| "invalid preview origin pin")?;
    let signature = Signature::from_slice(
        &hex::decode(&receipt.node_scoped_signature)
            .map_err(|_| "invalid preview origin signature")?,
    )
    .map_err(|_| "invalid preview origin signature")?;
    VerifyingKey::from_bytes(&public)
        .map_err(|_| "invalid preview origin pin")?
        .verify_strict(&receipt_payload(&receipt)?, &signature)
        .map_err(|_| "preview origin signature rejected")?;
    let artifact = files::read_bounded(
        &root.join("genesis/initial-artifact.json"),
        16 * 1024 * 1024,
    )?;
    let value: Value =
        serde_json::from_slice(&artifact).map_err(|_| "invalid preview origin artifact")?;
    if artifact != encode(value.clone())?
        || blake3_hex(&artifact) != receipt.genesis_artifact_hash
        || receipt.world.genesis_digest != format!("blake3:{}", receipt.genesis_artifact_hash)
        || value["domain"] != GENESIS_DOMAIN
        || value["world_id"] != world_id
        || value["node_id"] != node_id
        || value["genesis_nonce"] != receipt.genesis_nonce
        || value["service_public_key"] != service_public
    {
        return Err("preview origin artifact or genesis mismatch".into());
    }
    let identity = bridge::world_service_read::identity(&root.join("world"), world_id)?;
    let original: oasis7::world_service::CommitRef =
        serde_json::from_value(receipt.published_commit)
            .map_err(|_| "invalid preview origin commit")?;
    if identity != receipt.world
        || original.world != receipt.world
        // Runtime bootstrap time and the first published chain position are
        // separate boundaries; the genuine initializer publishes a successor.
        || receipt.initial_height == 0
        || original.position == 0
    {
        return Err("preview origin identity or initial commit mismatch".into());
    }
    Ok(identity)
}
