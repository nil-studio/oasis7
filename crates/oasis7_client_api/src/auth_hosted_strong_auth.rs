use super::*;

pub(super) fn verify_hosted_prompt_control_strong_auth_grant(
    expected_action_id: &str,
    request_agent_id: &str,
    request_player_id: &str,
    request_public_key: Option<&str>,
    grant: &HostedStrongAuthGrant,
    required_signer_public_key: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    if grant.version != VIEWER_HOSTED_STRONG_AUTH_GRANT_PAYLOAD_VERSION {
        return Err(format!(
            "hosted strong-auth grant version mismatch: expected={} actual={}",
            VIEWER_HOSTED_STRONG_AUTH_GRANT_PAYLOAD_VERSION, grant.version
        ));
    }
    let action_id = normalize_prompt_control_grant_operation(grant.action_id.as_str())?;
    if action_id != expected_action_id {
        return Err("hosted strong-auth grant action_id does not match request".to_string());
    }
    let request_agent_id =
        normalize_required_field(request_agent_id, "hosted strong-auth request agent_id")?;
    let request_player_id =
        normalize_required_field(request_player_id, "hosted strong-auth request player_id")?;
    let request_public_key = normalize_required_optional_public_key(
        request_public_key,
        "hosted strong-auth request public_key",
    )?;
    let grant_player_id = normalize_required_field(
        grant.player_id.as_str(),
        "hosted strong-auth grant player_id",
    )?;
    let grant_player_public_key = normalize_public_key_field(
        grant.player_public_key.as_str(),
        "hosted strong-auth grant player_public_key",
    )?;
    let grant_agent_id =
        normalize_required_field(grant.agent_id.as_str(), "hosted strong-auth grant agent_id")?;
    if request_player_id != grant_player_id {
        return Err("hosted strong-auth grant player_id does not match request".to_string());
    }
    if request_public_key != grant_player_public_key {
        return Err("hosted strong-auth grant public_key does not match request".to_string());
    }
    if request_agent_id != grant_agent_id {
        return Err("hosted strong-auth grant agent_id does not match request".to_string());
    }
    if grant.expires_at_unix_ms <= grant.issued_at_unix_ms {
        return Err(
            "hosted strong-auth grant expires_at_unix_ms must be greater than issued_at_unix_ms"
                .to_string(),
        );
    }
    if now_unix_ms > grant.expires_at_unix_ms {
        return Err("hosted strong-auth grant has expired".to_string());
    }
    let required_signer_public_key = normalize_public_key_field(
        required_signer_public_key,
        "hosted strong-auth required signer public key",
    )?;
    let grant_signer_public_key = normalize_public_key_field(
        grant.signer_public_key.as_str(),
        "hosted strong-auth grant signer public key",
    )?;
    if grant_signer_public_key != required_signer_public_key {
        return Err("hosted strong-auth grant signer is not allowlisted".to_string());
    }
    let signing_payload = build_hosted_prompt_control_strong_auth_grant_payload(
        action_id,
        grant_player_id.as_str(),
        grant_player_public_key.as_str(),
        grant_agent_id.as_str(),
        grant.issued_at_unix_ms,
        grant.expires_at_unix_ms,
    )?;
    verify_hosted_strong_auth_grant_signature(
        grant_signer_public_key.as_str(),
        grant.signature.as_str(),
        signing_payload.as_slice(),
    )
}

pub(super) fn verify_hosted_strong_auth_grant_signature(
    public_key_hex: &str,
    signature: &str,
    signing_payload: &[u8],
) -> Result<(), String> {
    let public_key_bytes = decode_hex_array::<32>(public_key_hex, "hosted strong-auth public key")?;
    let signature_hex = signature
        .strip_prefix(VIEWER_HOSTED_STRONG_AUTH_GRANT_SIGNATURE_V1_PREFIX)
        .ok_or_else(|| "hosted strong-auth signature is not awhostedgrant:v1".to_string())?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "hosted strong-auth signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse hosted strong-auth public key failed: {err}"))?;
    verifying_key
        .verify(signing_payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|err| format!("verify hosted strong-auth signature failed: {err}"))
}
