use super::*;
use serde::Serialize;
use serde_json::Value;

const AGENCY_CONTROL_AUTH_DOMAIN: &str = "oasis7.viewer.agency-control.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AgencyControlSigningPayloadV1<'a> {
    domain: &'static str,
    version: u8,
    operation_request_id: &'a str,
    player_id: &'a str,
    public_key: &'a str,
    agent_id: &'a str,
    world_id: &'a str,
    reorg_epoch: u64,
    nonce: u64,
    command: &'a Value,
}

// Keep the complete signed identity fields explicit at this authentication boundary.
#[allow(clippy::too_many_arguments)]
fn payload_bytes(
    request_id: &str,
    player_id: &str,
    public_key: &str,
    agent_id: &str,
    world_id: &str,
    reorg_epoch: u64,
    nonce: u64,
    command: &Value,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&AgencyControlSigningPayloadV1 {
        domain: AGENCY_CONTROL_AUTH_DOMAIN,
        version: 1,
        operation_request_id: request_id,
        player_id,
        public_key,
        agent_id,
        world_id,
        reorg_epoch,
        nonce,
        command,
    })
    .map_err(|error| format!("agency_control signing payload failed: {error}"))
}

// Keep the complete signed identity fields explicit at this authentication boundary.
#[allow(clippy::too_many_arguments)]
pub fn verify_agency_control_auth_proof(
    request_id: &str,
    player_id: &str,
    public_key: &str,
    agent_id: &str,
    world_id: &str,
    reorg_epoch: u64,
    command: &Value,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let request_player_id = normalize_required_field(player_id, "agency_control player_id")?;
    let request_public_key = normalize_public_key_field(public_key, "agency_control public_key")?;
    let proof_player_id =
        normalize_required_field(proof.player_id.as_str(), "auth proof player_id")?;
    let proof_public_key =
        normalize_public_key_field(proof.public_key.as_str(), "auth proof public key")?;
    let request_id = normalize_required_field(request_id, "agency_control request_id")?;
    let agent_id = normalize_required_field(agent_id, "agency_control agent_id")?;
    let world_id = normalize_required_field(world_id, "agency_control world_id")?;
    if request_player_id != proof_player_id {
        return Err("auth proof player_id does not match request player_id".to_string());
    }
    if request_public_key != proof_public_key {
        return Err("auth proof public_key does not match request public_key".to_string());
    }
    if proof.nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let bytes = payload_bytes(
        request_id.as_str(),
        proof_player_id.as_str(),
        proof_public_key.as_str(),
        agent_id.as_str(),
        world_id.as_str(),
        reorg_epoch,
        proof.nonce,
        command,
    )?;
    verify_player_auth_signature(proof_public_key.as_str(), proof.signature.as_str(), &bytes)?;
    Ok(VerifiedPlayerAuth {
        player_id: proof_player_id,
        public_key: proof_public_key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    })
}

// Keep the complete signed identity fields explicit at this authentication boundary.
#[allow(clippy::too_many_arguments)]
pub fn sign_agency_control_auth_proof(
    request_id: &str,
    player_id: &str,
    public_key: &str,
    agent_id: &str,
    world_id: &str,
    reorg_epoch: u64,
    command: &Value,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    let player_id = normalize_required_field(player_id, "agency_control player_id")?;
    let public_key = normalize_public_key_field(public_key, "agency_control public_key")?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "agency signer public key")?;
    if signer_public_key != public_key {
        return Err("agency_control public key does not match signer".to_string());
    }
    let signing_key = signing_key_from_hex(signer_private_key_hex, "agency signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "agency signer public key",
    )?;
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let bytes = payload_bytes(
        request_id,
        player_id.as_str(),
        public_key.as_str(),
        agent_id,
        world_id,
        reorg_epoch,
        nonce,
        command,
    )?;
    sign_player_auth_proof(signing_key, player_id, public_key, nonce, bytes)
}
