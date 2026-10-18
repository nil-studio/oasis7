use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::Serialize;

use crate::viewer_protocol::{
    DIRECTOR_CAPABILITY_ACTION, DIRECTOR_CAPABILITY_AUDIENCE, DIRECTOR_CAPABILITY_DOMAIN,
    DIRECTOR_CAPABILITY_GRANT_VERSION, DIRECTOR_CAPABILITY_MAX_TTL_MS, DIRECTOR_CAPABILITY_SCOPE,
    DIRECTOR_CAPABILITY_SIGNATURE_V1_PREFIX, DirectorCapabilityGrant,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DirectorCapabilitySigningPayload<'a> {
    action: &'static str,
    audience: &'static str,
    scope: &'static str,
    player_id: &'a str,
    player_public_key: &'a str,
    server: &'a str,
    session_epoch: u64,
    nonce: &'a str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DirectorCapabilitySigningEnvelope<'a> {
    domain: &'static str,
    version: u8,
    payload: DirectorCapabilitySigningPayload<'a>,
}

/// Sign an ephemeral, read-only Director visibility capability.
///
/// The caller must obtain `session_epoch` and the server identity from an
/// authoritative runtime/session policy. This helper deliberately performs no local
/// permission decision and never signs a gameplay or command capability.
#[expect(
    clippy::too_many_arguments,
    reason = "Director capability signing preserves the stable protocol payload shape"
)]
pub fn sign_director_capability_grant(
    player_id: &str,
    player_public_key: &str,
    server: &str,
    session_epoch: u64,
    nonce: &str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<DirectorCapabilityGrant, String> {
    let player_id = normalize_required_field(player_id, "director capability player_id")?;
    let player_public_key =
        normalize_public_key_field(player_public_key, "director capability player_public_key")?;
    let server = normalize_required_field(server, "director capability server")?;
    let nonce = normalize_required_field(nonce, "director capability nonce")?;
    validate_director_capability_window(issued_at_unix_ms, expires_at_unix_ms)?;
    if session_epoch == 0 {
        return Err("director capability session_epoch must be greater than zero".to_string());
    }
    let signer_public_key = normalize_public_key_field(
        signer_public_key_hex,
        "director capability signer public key",
    )?;
    let signing_key = signing_key_from_hex(
        signer_private_key_hex,
        "director capability signer private key",
    )?;
    verify_keypair_match(
        &signing_key,
        signer_public_key.as_str(),
        "director capability signer public key",
    )?;

    let signing_payload = build_director_capability_signing_payload(
        player_id.as_str(),
        player_public_key.as_str(),
        server.as_str(),
        session_epoch,
        nonce.as_str(),
        issued_at_unix_ms,
        expires_at_unix_ms,
    )?;
    let signature = signing_key.sign(signing_payload.as_slice());
    Ok(DirectorCapabilityGrant {
        version: DIRECTOR_CAPABILITY_GRANT_VERSION,
        action: DIRECTOR_CAPABILITY_ACTION.to_string(),
        audience: DIRECTOR_CAPABILITY_AUDIENCE.to_string(),
        scope: DIRECTOR_CAPABILITY_SCOPE.to_string(),
        player_id,
        player_public_key,
        server,
        session_epoch,
        nonce,
        issued_at_unix_ms,
        expires_at_unix_ms,
        signer_public_key,
        signature: format!(
            "{DIRECTOR_CAPABILITY_SIGNATURE_V1_PREFIX}{}",
            hex::encode(signature.to_bytes())
        ),
    })
}

/// Verify an already-issued Director capability against the current player/session.
///
/// Replay consumption is intentionally not hidden in this pure cryptographic verifier;
/// runtime session policy owns the in-memory one-shot nonce guard so that failed signatures
/// do not consume a nonce and no grant ledger is persisted.
pub fn verify_director_capability_grant(
    grant: &DirectorCapabilityGrant,
    expected_player_id: &str,
    expected_player_public_key: &str,
    expected_server: &str,
    expected_session_epoch: u64,
    required_signer_public_key: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    if grant.version != DIRECTOR_CAPABILITY_GRANT_VERSION {
        return Err(format!(
            "director capability grant version mismatch: expected={} actual={}",
            DIRECTOR_CAPABILITY_GRANT_VERSION, grant.version
        ));
    }
    if grant.action.trim() != DIRECTOR_CAPABILITY_ACTION {
        return Err("director capability grant action does not match director_open".to_string());
    }
    if grant.audience.trim() != DIRECTOR_CAPABILITY_AUDIENCE {
        return Err(
            "director capability grant audience does not match viewer_director".to_string(),
        );
    }
    if grant.scope.trim() != DIRECTOR_CAPABILITY_SCOPE {
        return Err("director capability grant scope does not match diagnostics_read".to_string());
    }
    let expected_player_id =
        normalize_required_field(expected_player_id, "director capability expected player_id")?;
    let expected_player_public_key = normalize_public_key_field(
        expected_player_public_key,
        "director capability expected player_public_key",
    )?;
    let expected_server =
        normalize_required_field(expected_server, "director capability expected server")?;
    let grant_player_id = normalize_required_field(
        grant.player_id.as_str(),
        "director capability grant player_id",
    )?;
    let grant_player_public_key = normalize_public_key_field(
        grant.player_public_key.as_str(),
        "director capability grant player_public_key",
    )?;
    let grant_server =
        normalize_required_field(grant.server.as_str(), "director capability grant server")?;
    let nonce = normalize_required_field(grant.nonce.as_str(), "director capability grant nonce")?;
    if grant_player_id != expected_player_id {
        return Err("director capability grant player_id does not match session".to_string());
    }
    if grant_player_public_key != expected_player_public_key {
        return Err(
            "director capability grant player_public_key does not match session".to_string(),
        );
    }
    if grant_server != expected_server {
        return Err("director capability grant server does not match runtime".to_string());
    }
    if grant.session_epoch == 0 || grant.session_epoch != expected_session_epoch {
        return Err("director capability grant session_epoch does not match session".to_string());
    }
    validate_director_capability_window(grant.issued_at_unix_ms, grant.expires_at_unix_ms)?;
    if now_unix_ms > grant.expires_at_unix_ms {
        return Err("director capability grant has expired".to_string());
    }
    let required_signer_public_key = normalize_public_key_field(
        required_signer_public_key,
        "director capability required signer public key",
    )?;
    let grant_signer_public_key = normalize_public_key_field(
        grant.signer_public_key.as_str(),
        "director capability grant signer public key",
    )?;
    if grant_signer_public_key != required_signer_public_key {
        return Err("director capability grant signer is not allowlisted".to_string());
    }
    let signing_payload = build_director_capability_signing_payload(
        grant_player_id.as_str(),
        grant_player_public_key.as_str(),
        grant_server.as_str(),
        grant.session_epoch,
        nonce.as_str(),
        grant.issued_at_unix_ms,
        grant.expires_at_unix_ms,
    )?;
    verify_director_capability_signature(
        grant_signer_public_key.as_str(),
        grant.signature.as_str(),
        signing_payload.as_slice(),
    )
}

fn validate_director_capability_window(
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
) -> Result<(), String> {
    if issued_at_unix_ms == 0 {
        return Err("director capability issued_at_unix_ms must be greater than zero".to_string());
    }
    if expires_at_unix_ms <= issued_at_unix_ms {
        return Err(
            "director capability expires_at_unix_ms must be greater than issued_at_unix_ms"
                .to_string(),
        );
    }
    if expires_at_unix_ms.saturating_sub(issued_at_unix_ms) > DIRECTOR_CAPABILITY_MAX_TTL_MS {
        return Err(format!(
            "director capability TTL must be <= {}ms",
            DIRECTOR_CAPABILITY_MAX_TTL_MS
        ));
    }
    Ok(())
}

fn build_director_capability_signing_payload(
    player_id: &str,
    player_public_key: &str,
    server: &str,
    session_epoch: u64,
    nonce: &str,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
) -> Result<Vec<u8>, String> {
    let payload = DirectorCapabilitySigningPayload {
        action: DIRECTOR_CAPABILITY_ACTION,
        audience: DIRECTOR_CAPABILITY_AUDIENCE,
        scope: DIRECTOR_CAPABILITY_SCOPE,
        player_id,
        player_public_key,
        server,
        session_epoch,
        nonce,
        issued_at_unix_ms,
        expires_at_unix_ms,
    };
    serde_cbor::to_vec(&DirectorCapabilitySigningEnvelope {
        domain: DIRECTOR_CAPABILITY_DOMAIN,
        version: DIRECTOR_CAPABILITY_GRANT_VERSION,
        payload,
    })
    .map_err(|err| format!("encode director capability grant payload failed: {err}"))
}

fn verify_director_capability_signature(
    public_key_hex: &str,
    signature: &str,
    signing_payload: &[u8],
) -> Result<(), String> {
    let public_key_bytes =
        decode_hex_array::<32>(public_key_hex, "director capability signer public key")?;
    let signature_hex = signature
        .strip_prefix(DIRECTOR_CAPABILITY_SIGNATURE_V1_PREFIX)
        .ok_or_else(|| "director capability signature is not awdirectorgrant:v1".to_string())?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "director capability signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse director capability signer public key failed: {err}"))?;
    verifying_key
        .verify(signing_payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|err| format!("verify director capability signature failed: {err}"))
}

fn normalize_required_field(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(value.to_string())
}

fn normalize_public_key_field(raw: &str, label: &str) -> Result<String, String> {
    let normalized = normalize_required_field(raw, label)?;
    let bytes = decode_hex_array::<32>(normalized.as_str(), label)?;
    Ok(hex::encode(bytes))
}

fn signing_key_from_hex(private_key_hex: &str, label: &str) -> Result<SigningKey, String> {
    let private_key_bytes = decode_hex_array::<32>(private_key_hex, label)?;
    Ok(SigningKey::from_bytes(&private_key_bytes))
}

fn verify_keypair_match(
    signing_key: &SigningKey,
    public_key_hex: &str,
    label: &str,
) -> Result<(), String> {
    let expected_public_key = hex::encode(signing_key.verifying_key().to_bytes());
    if expected_public_key != public_key_hex {
        return Err(format!(
            "{label} does not match private key: expected={expected_public_key} actual={public_key_hex}"
        ));
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|err| format!("decode {label} failed: {err}"))?;
    if bytes.len() != N {
        return Err(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        ));
    }
    let mut fixed = [0_u8; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}
