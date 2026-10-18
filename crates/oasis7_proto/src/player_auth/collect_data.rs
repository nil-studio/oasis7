use super::*;
use crate::viewer::{CollectDataCommand, CollectDataRequest};

pub fn sign_collect_data_auth_proof(
    command: &CollectDataCommand,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let (operation, request) = collect_data_command_parts(command);
    let player_id = normalize_required_field(request.player_id.as_str(), "collect_data player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "collect_data public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "collect_data signer public key")?;
    if signer_public_key != request_public_key {
        return Err("collect_data public_key does not match signer public key".to_string());
    }
    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "collect_data signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "collect_data signer public key",
    )?;
    let signing_payload = build_collect_data_signing_payload(
        operation,
        request,
        player_id.as_str(),
        request_public_key.as_str(),
        nonce,
    )?;
    sign_player_auth_proof(
        signing_key,
        player_id,
        signer_public_key,
        nonce,
        signing_payload,
    )
}

pub fn verify_collect_data_auth_proof(
    command: &CollectDataCommand,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_proof_scheme(proof)?;
    let (operation, request) = collect_data_command_parts(command);
    let request_player_id =
        normalize_required_field(request.player_id.as_str(), "collect_data player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "collect_data public_key",
    )?;
    let proof_player_id =
        normalize_required_field(proof.player_id.as_str(), "auth proof player_id")?;
    let proof_public_key =
        normalize_public_key_field(proof.public_key.as_str(), "auth proof public key")?;
    if request_player_id != proof_player_id {
        return Err("auth proof player_id does not match request player_id".to_string());
    }
    if request_public_key != proof_public_key {
        return Err("auth proof public_key does not match request public_key".to_string());
    }
    if proof.nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    crate::collect_data_auth::verify_authorization(
        operation,
        request.electricity_cost,
        request.data_amount,
        proof_player_id.as_str(),
        proof_public_key.as_str(),
        proof.nonce,
        proof.signature.as_str(),
    )?;
    Ok(VerifiedPlayerAuth {
        player_id: proof_player_id,
        public_key: proof_public_key,
        nonce: proof.nonce,
        hosted_registration_nonce: None,
    })
}

fn collect_data_command_parts(command: &CollectDataCommand) -> (&'static str, &CollectDataRequest) {
    match command {
        CollectDataCommand::Preflight { request } => ("collect_data_preflight", request),
        CollectDataCommand::Submit { request } => ("collect_data_submit", request),
    }
}

fn build_collect_data_signing_payload(
    operation: &'static str,
    request: &CollectDataRequest,
    player_id: &str,
    public_key: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    crate::collect_data_auth::encode_authorization_payload(
        operation,
        request.electricity_cost,
        request.data_amount,
        player_id,
        public_key,
        nonce,
    )
}
