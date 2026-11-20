use super::*;

pub fn sign_agent_chat_auth_proof(
    request: &AgentChatRequest,
    nonce: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<PlayerAuthProof, String> {
    if nonce == 0 {
        return Err("auth nonce must be greater than zero".to_string());
    }
    let player_id =
        normalize_required_optional_field(request.player_id.as_deref(), "agent_chat player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request.public_key.as_deref(),
        "agent_chat public_key",
    )?;
    let signer_public_key =
        normalize_public_key_field(signer_public_key_hex, "agent_chat signer public key")?;
    if signer_public_key != request_public_key {
        return Err("agent_chat public_key does not match signer public key".to_string());
    }

    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "agent_chat signer private key")?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "agent_chat signer public key",
    )?;

    let signing_payload = build_agent_chat_signing_payload(
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

pub fn verify_agent_chat_auth_proof(
    request: &AgentChatRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    work::verify_agent_chat_auth_proof_with_work_v1(
        request,
        proof,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map(work::PaidVerifiedPlayerAuthDataV1::into_legacy)
    .map_err(work::legacy_error)
}

/// Verify an Agent Chat proof and require that it is scoped to the current
/// authoritative world position.
///
/// `verify_agent_chat_auth_proof` intentionally remains wire-compatible with
/// legacy requests.  V2 runtime acceptance must use this helper so that a
/// signature made for another world, reorg epoch, or protocol scope cannot be
/// replayed at this boundary.
pub fn verify_agent_chat_auth_proof_with_authority(
    request: &AgentChatRequest,
    proof: &PlayerAuthProof,
    expected_world_id: &str,
    expected_reorg_epoch: u64,
    expected_authority_scope: &str,
) -> Result<VerifiedPlayerAuth, String> {
    let verified = verify_agent_chat_auth_proof(request, proof)?;
    let Some((world_id, reorg_epoch, authority_scope)) = normalize_agent_chat_authority(request)?
    else {
        return Err("agent_chat authority envelope is required".to_string());
    };
    let expected_world_id = normalize_required_field(expected_world_id, "agent_chat world_id")?;
    let expected_authority_scope =
        normalize_required_field(expected_authority_scope, "agent_chat authority_scope")?;
    if world_id != expected_world_id {
        return Err(format!(
            "agent_chat world_id mismatch: expected={expected_world_id} actual={world_id}"
        ));
    }
    if reorg_epoch != expected_reorg_epoch {
        return Err(format!(
            "agent_chat reorg_epoch mismatch: expected={expected_reorg_epoch} actual={reorg_epoch}"
        ));
    }
    if authority_scope != expected_authority_scope {
        return Err(format!(
            "agent_chat authority_scope mismatch: expected={expected_authority_scope} actual={authority_scope}"
        ));
    }
    Ok(verified)
}
