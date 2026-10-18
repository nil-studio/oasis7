use super::hosted_player_session::{
    HostedPlayerSessionAdmissionResponse, HostedPlayerSessionAdmissionSnapshot,
    HostedPlayerSessionIssuer,
};
use super::*;
use oasis7_client_api::{
    viewer_auth::sign_hosted_prompt_control_strong_auth_grant,
    viewer_protocol::HostedStrongAuthGrant,
};
use serde::Serialize;

pub(super) const HOSTED_STRONG_AUTH_GRANT_ROUTE: &str = "/api/public/strong-auth/grant";
pub(super) const HOSTED_PROMPT_CONTROL_STRONG_AUTH_GRANT_ROUTE: &str =
    "/api/public/strong-auth/grant/prompt-control";
const HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY";
const HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY";
const HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE";
const HOSTED_STRONG_AUTH_GRANT_TTL_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize)]
pub(super) struct HostedStrongAuthGrantResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    pub(super) deployment_mode: String,
    pub(super) admission: HostedPlayerSessionAdmissionSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) grant: Option<HostedStrongAuthGrant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostedStrongAuthActionGrantMode {
    PromptControlBackendReauthPreview,
    BlockedUntilDedicatedLane,
    Unsupported,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
pub(super) fn issue_hosted_strong_auth_grant(
    deployment_mode: DeploymentMode,
    player_id: &str,
    public_key: &str,
    agent_id: &str,
    action_id: &str,
    approval_code: &str,
    release_token: &str,
    issuer: &mut HostedPlayerSessionIssuer,
) -> HostedStrongAuthGrantResponse {
    let admission = issuer.refresh(deployment_mode, player_id, release_token, None);
    if !admission.ok {
        return response_from_admission(admission, None);
    }
    if deployment_mode != DeploymentMode::HostedPublicJoin {
        return HostedStrongAuthGrantResponse {
            ok: false,
            error_code: Some("strong_auth_grant_disabled".to_string()),
            error: Some(
                "hosted prompt-control strong-auth grant is only available on hosted_public_join"
                    .to_string(),
            ),
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: admission.admission,
            grant: None,
        };
    }
    let normalized_action_id = action_id.trim();
    match hosted_strong_auth_action_grant_mode(normalized_action_id) {
        HostedStrongAuthActionGrantMode::PromptControlBackendReauthPreview => {}
        HostedStrongAuthActionGrantMode::BlockedUntilDedicatedLane => {
            return HostedStrongAuthGrantResponse {
                ok: false,
                error_code: Some("strong_auth_action_not_enabled".to_string()),
                error: Some(format!(
                    "hosted public join does not enable backend strong-auth grant for action_id `{normalized_action_id}` yet"
                )),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission: admission.admission,
                grant: None,
            };
        }
        HostedStrongAuthActionGrantMode::Unsupported => {
            return HostedStrongAuthGrantResponse {
                ok: false,
                error_code: Some("unsupported_action_id".to_string()),
                error: Some("unsupported strong-auth action_id".to_string()),
                deployment_mode: deployment_mode.as_str().to_string(),
                admission: admission.admission,
                grant: None,
            };
        }
    }
    if !hosted_strong_auth_backend_grant_enabled() {
        return HostedStrongAuthGrantResponse {
            ok: false,
            error_code: Some("strong_auth_backend_unavailable".to_string()),
            error: Some(
                "hosted strong-auth backend grant is not configured on this server".to_string(),
            ),
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: admission.admission,
            grant: None,
        };
    }
    let expected_approval_code = std::env::var(HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());
    if expected_approval_code.as_deref() != Some(approval_code.trim()) {
        return HostedStrongAuthGrantResponse {
            ok: false,
            error_code: Some("approval_code_invalid".to_string()),
            error: Some("hosted strong-auth approval code is invalid".to_string()),
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: admission.admission,
            grant: None,
        };
    }
    let signer_public_key = std::env::var(HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .unwrap_or_default();
    let signer_private_key = std::env::var(HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .unwrap_or_default();
    let issued_at_unix_ms = now_unix_ms();
    let expires_at_unix_ms = issued_at_unix_ms.saturating_add(HOSTED_STRONG_AUTH_GRANT_TTL_MS);
    match sign_hosted_prompt_control_strong_auth_grant(
        normalized_action_id,
        player_id.trim(),
        public_key.trim(),
        agent_id.trim(),
        issued_at_unix_ms,
        expires_at_unix_ms,
        signer_public_key.as_str(),
        signer_private_key.as_str(),
    ) {
        Ok(grant) => HostedStrongAuthGrantResponse {
            ok: true,
            error_code: None,
            error: None,
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: admission.admission,
            grant: Some(grant),
        },
        Err(err) => HostedStrongAuthGrantResponse {
            ok: false,
            error_code: Some("strong_auth_grant_sign_failed".to_string()),
            error: Some(err),
            deployment_mode: deployment_mode.as_str().to_string(),
            admission: admission.admission,
            grant: None,
        },
    }
}

fn response_from_admission(
    admission: HostedPlayerSessionAdmissionResponse,
    grant: Option<HostedStrongAuthGrant>,
) -> HostedStrongAuthGrantResponse {
    HostedStrongAuthGrantResponse {
        ok: admission.ok && grant.is_some(),
        error_code: admission.error_code,
        error: admission.error,
        deployment_mode: admission.deployment_mode,
        admission: admission.admission,
        grant,
    }
}

fn hosted_strong_auth_action_grant_mode(action_id: &str) -> HostedStrongAuthActionGrantMode {
    match action_id {
        "prompt_control_preview" | "prompt_control_apply" | "prompt_control_rollback" => {
            HostedStrongAuthActionGrantMode::PromptControlBackendReauthPreview
        }
        "main_token_transfer" => HostedStrongAuthActionGrantMode::BlockedUntilDedicatedLane,
        _ => HostedStrongAuthActionGrantMode::Unsupported,
    }
}

fn hosted_strong_auth_backend_grant_enabled() -> bool {
    [
        HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
        HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
        HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
    ]
    .into_iter()
    .all(env_non_empty)
}

fn env_non_empty(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|raw| !raw.trim().is_empty())
        .unwrap_or(false)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn signer(seed: u8) -> (String, String) {
        let private_key = [seed; 32];
        let signing_key = SigningKey::from_bytes(&private_key);
        (
            hex::encode(signing_key.verifying_key().to_bytes()),
            hex::encode(private_key),
        )
    }

    #[test]
    fn hosted_prompt_control_strong_auth_grant_requires_approval_code() {
        let (public_key, private_key) = signer(41);
        if !crate::hosted_test_env::run(&[
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new(&public_key),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new(&private_key),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("correct-code"),
            ),
        ]) {
            return;
        }

        let mut issuer = HostedPlayerSessionIssuer::default();
        let issue = issuer.issue(DeploymentMode::HostedPublicJoin);
        let grant = issue.grant.expect("grant");
        let response = issue_hosted_strong_auth_grant(
            DeploymentMode::HostedPublicJoin,
            grant.player_id.as_str(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "agent-0",
            "prompt_control_apply",
            "wrong-code",
            grant.release_token.as_str(),
            &mut issuer,
        );
        assert!(!response.ok);
        assert_eq!(
            response.error_code.as_deref(),
            Some("approval_code_invalid")
        );
    }

    #[test]
    fn hosted_prompt_control_strong_auth_grant_issues_preview_grant() {
        let (public_key, private_key) = signer(42);
        if !crate::hosted_test_env::run(&[
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new(&public_key),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new(&private_key),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("correct-code"),
            ),
        ]) {
            return;
        }

        let mut issuer = HostedPlayerSessionIssuer::default();
        let issue = issuer.issue(DeploymentMode::HostedPublicJoin);
        let grant = issue.grant.expect("grant");
        let response = issue_hosted_strong_auth_grant(
            DeploymentMode::HostedPublicJoin,
            grant.player_id.as_str(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "agent-0",
            "prompt_control_apply",
            "correct-code",
            grant.release_token.as_str(),
            &mut issuer,
        );
        assert!(response.ok, "{response:?}");
        let issued_grant = response.grant.expect("backend grant");
        assert_eq!(issued_grant.action_id, "prompt_control_apply");
        assert_eq!(issued_grant.player_id, grant.player_id);
        assert_eq!(
            issued_grant.player_public_key,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(issued_grant.agent_id, "agent-0");
        assert_eq!(issued_grant.signer_public_key, public_key);
        assert!(issued_grant.expires_at_unix_ms > issued_grant.issued_at_unix_ms);
    }

    #[test]
    fn hosted_strong_auth_grant_rejects_main_token_transfer_until_lane_lands() {
        let (public_key, private_key) = signer(43);
        if !crate::hosted_test_env::run(&[
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new(&public_key),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new(&private_key),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("correct-code"),
            ),
        ]) {
            return;
        }

        let mut issuer = HostedPlayerSessionIssuer::default();
        let issue = issuer.issue(DeploymentMode::HostedPublicJoin);
        let grant = issue.grant.expect("grant");
        let response = issue_hosted_strong_auth_grant(
            DeploymentMode::HostedPublicJoin,
            grant.player_id.as_str(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "agent-0",
            "main_token_transfer",
            "correct-code",
            grant.release_token.as_str(),
            &mut issuer,
        );
        assert!(!response.ok);
        assert_eq!(
            response.error_code.as_deref(),
            Some("strong_auth_action_not_enabled")
        );
        assert!(
            response
                .error
                .as_deref()
                .is_some_and(|message| message.contains("main_token_transfer"))
        );
    }
}
