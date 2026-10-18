//! Host authentication adapter: pure signature contracts plus durable replay checks.
use super::protocol::{AuthoritativeSessionRegisterRequest, PlayerAuthProof};
pub use oasis7::runtime_host::auth_storage::{
    ExclusiveDirectoryProcessLock, HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
    preflight_hosted_registration_replay_ledger,
};
pub(crate) use oasis7::runtime_host::auth_storage::{
    claim_registration_grant_nonce_for_recovery, consume_registration_grant_nonce,
};
pub use oasis7_client_api::viewer_auth::*;

pub fn verify_session_register_auth_proof(
    request: &AuthoritativeSessionRegisterRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    let verified = verify_session_register_auth_proof_signature(request, proof)?;
    if let Some(nonce) = &verified.hosted_registration_nonce {
        oasis7::runtime_host::auth_storage::ensure_registration_grant_nonce_unused(nonce)?;
    }
    Ok(verified)
}
pub(crate) fn verify_session_register_auth_proof_for_recovery(
    request: &AuthoritativeSessionRegisterRequest,
    proof: &PlayerAuthProof,
) -> Result<VerifiedPlayerAuth, String> {
    verify_session_register_auth_proof_signature(request, proof)
}

pub(crate) fn verify_hosted_authorization_grant(
    grant: &str,
    player_id: &str,
    public_key: &str,
    require_unused: bool,
) -> Result<HostedAuthContextV2, String> {
    let context = oasis7_client_api::viewer_auth::verify_hosted_authorization_grant_signature(
        grant, player_id, public_key,
    )?;
    if require_unused {
        oasis7::runtime_host::auth_storage::ensure_registration_grant_nonce_unused(
            &authorization_nonce(&context),
        )?;
    }
    Ok(context)
}

#[cfg(test)]
pub(crate) fn test_hosted_authorization_context() -> HostedAuthContextV2 {
    HostedAuthContextV2 {
        version: 2,
        identity_service_id: "identity".into(),
        environment_id: "test".into(),
        identity_instance_epoch: 1,
        account_id: "account".into(),
        account_session_id: "account-session".into(),
        credential_record_id: "credential".into(),
        credential_version: 1,
        account_security_epoch: 1,
        auth_method: "webauthn".into(),
        auth_time: 900,
        recovery_context: None,
        world_identity_scope: "test-world-scope".into(),
        world_id: "world".into(),
        player_id: "hosted-player-a".into(),
        device_session_id: "world-session".into(),
        session_public_key: hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[3; 32])
                .verifying_key()
                .to_bytes(),
        ),
        world_session_epoch: 1,
        capabilities: vec!["player_session".into()],
        checked_at: 1_000,
        issued_at: 1_100,
        expires_at: 31_000,
        nonce: "nonce".into(),
        issuer_key_id: "issuer".into(),
    }
}

#[cfg(test)]
pub(crate) use oasis7_proto::player_auth::sign_agency_control_auth_proof as sign_agency_control_auth_proof_for_test;
#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
