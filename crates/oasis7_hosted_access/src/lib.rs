use serde::Serialize;
#[cfg(any(test, feature = "test-support"))]
use std::sync::{Mutex, OnceLock};

pub const DEFAULT_DEPLOYMENT_MODE: &str = "hosted_public_join";
#[allow(dead_code)]
const DEFAULT_MAX_GUEST_SESSIONS: u64 = 32;
#[allow(dead_code)]
const DEFAULT_MAX_PLAYER_SESSIONS: u64 = 8;
#[allow(dead_code)]
const DEFAULT_ISSUE_RATE_LIMIT_PER_MINUTE: u64 = 60;
#[allow(dead_code)]
const DEFAULT_WORLD_FULL_POLICY: &str = "reject";
#[allow(dead_code)]
const DEFAULT_KICK_POLICY: &str = "operator_audit_required";
const HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY";
const HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY";
const HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV: &str = "OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE";
const PUBLIC_STATE_ROUTE: &str = "/api/public/state";
const HOSTED_SESSION_LADDER: &[&str] = &["guest_session", "player_session", "strong_auth"];
const PROMPT_CONTROL_ACTION_IDS: &[&str] = &[
    "prompt_control_preview",
    "prompt_control_apply",
    "prompt_control_rollback",
];

#[cfg(any(test, feature = "test-support"))]
pub mod hosted_test_env;

// Retain the shared test API for existing web-launcher consumers. Launcher
// environment scenarios use child startup configuration instead of this lock.
#[cfg(any(test, feature = "test-support"))]
#[allow(dead_code)]
pub fn hosted_strong_auth_test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentMode {
    TrustedLocalOnly,
    HostedPublicJoin,
}

impl DeploymentMode {
    pub fn parse(raw: &str, label: &str) -> Result<Self, String> {
        match raw.trim() {
            "trusted_local_only" => Ok(Self::TrustedLocalOnly),
            "hosted_public_join" => Ok(Self::HostedPublicJoin),
            _ => Err(format!(
                "{label} must be one of: trusted_local_only|hosted_public_join"
            )),
        }
    }

    pub fn parse_user_facing(raw: &str, label: &str) -> Result<Self, String> {
        match raw.trim() {
            "hosted_public_join" => Ok(Self::HostedPublicJoin),
            "trusted_local_only" => Err(format!(
                "{label}=trusted_local_only has been removed from the user launcher flow; use hosted_public_join"
            )),
            _ => Err(format!("{label} must be hosted_public_join")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "trusted_local_only",
            Self::HostedPublicJoin => "hosted_public_join",
        }
    }

    pub fn browser_signer_bootstrap_mode(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "trusted_local_bootstrap_allowed",
            Self::HostedPublicJoin => "disabled_for_public_player_plane",
        }
    }

    pub fn allows_local_chain_runtime(self) -> bool {
        matches!(self, Self::TrustedLocalOnly)
    }

    pub fn local_chain_runtime_mode(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "launcher_managed_local_runtime_allowed",
            Self::HostedPublicJoin => "blocked_for_public_player_plane",
        }
    }

    pub fn node_admission_mode(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "trusted_local_preview_only",
            Self::HostedPublicJoin => "operator_managed_node_onboarding_only",
        }
    }

    pub fn gui_agent_action_surface(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "legacy_shared_local_preview",
            Self::HostedPublicJoin => "legacy_private_control_plane_only",
        }
    }

    pub fn hosted_player_access_verdict(self) -> &'static str {
        match self {
            Self::TrustedLocalOnly => "trusted_local_only_preview",
            Self::HostedPublicJoin if hosted_strong_auth_backend_grant_enabled() => {
                "hosted_public_join_strong_auth_preview"
            }
            Self::HostedPublicJoin => "hosted_public_join_blocked_until_strong_auth",
        }
    }

    #[allow(dead_code)]
    pub fn requires_loopback_private_control(self) -> bool {
        matches!(self, Self::HostedPublicJoin)
    }

    #[allow(dead_code)]
    pub fn disables_browser_signer_bootstrap(self) -> bool {
        matches!(self, Self::HostedPublicJoin)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct HostedAdmissionControlContract {
    pub max_guest_sessions: u64,
    pub max_player_sessions: u64,
    pub issue_rate_limit_per_minute: u64,
    pub world_full_policy: String,
    pub kick_policy: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct HostedPlayerAccessContract {
    pub deployment_mode: String,
    pub verdict: String,
    pub browser_signer_bootstrap: String,
    pub local_chain_runtime: String,
    pub node_admission: String,
    pub gui_agent_action_surface: String,
    pub public_state_route: String,
    pub public_endpoints: Vec<String>,
    pub private_endpoints: Vec<String>,
    pub session_ladder: Vec<String>,
    pub action_matrix: Vec<HostedActionAccessPolicy>,
    pub admission: HostedAdmissionControlContract,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct HostedActionAccessPolicy {
    pub action_id: String,
    pub required_auth: String,
    pub availability: String,
    pub reason: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct HostedViewerAccessHint {
    pub deployment_mode: String,
    pub verdict: String,
    pub browser_signer_bootstrap: String,
    pub local_chain_runtime: String,
    pub node_admission: String,
    pub session_ladder: Vec<String>,
    pub action_matrix: Vec<HostedActionAccessPolicy>,
}

#[allow(dead_code)]
pub fn hosted_player_access_contract(mode: DeploymentMode) -> HostedPlayerAccessContract {
    HostedPlayerAccessContract {
        deployment_mode: mode.as_str().to_string(),
        verdict: mode.hosted_player_access_verdict().to_string(),
        browser_signer_bootstrap: mode.browser_signer_bootstrap_mode().to_string(),
        local_chain_runtime: mode.local_chain_runtime_mode().to_string(),
        node_admission: mode.node_admission_mode().to_string(),
        gui_agent_action_surface: mode.gui_agent_action_surface().to_string(),
        public_state_route: PUBLIC_STATE_ROUTE.to_string(),
        public_endpoints: string_list(web_launcher_public_endpoints()),
        private_endpoints: string_list(web_launcher_private_endpoints()),
        session_ladder: hosted_session_ladder(),
        action_matrix: hosted_action_matrix(mode),
        admission: HostedAdmissionControlContract {
            max_guest_sessions: DEFAULT_MAX_GUEST_SESSIONS,
            max_player_sessions: DEFAULT_MAX_PLAYER_SESSIONS,
            issue_rate_limit_per_minute: DEFAULT_ISSUE_RATE_LIMIT_PER_MINUTE,
            world_full_policy: DEFAULT_WORLD_FULL_POLICY.to_string(),
            kick_policy: DEFAULT_KICK_POLICY.to_string(),
        },
    }
}

#[allow(dead_code)]
pub fn hosted_viewer_access_hint(mode: DeploymentMode) -> HostedViewerAccessHint {
    HostedViewerAccessHint {
        deployment_mode: mode.as_str().to_string(),
        verdict: mode.hosted_player_access_verdict().to_string(),
        browser_signer_bootstrap: mode.browser_signer_bootstrap_mode().to_string(),
        local_chain_runtime: mode.local_chain_runtime_mode().to_string(),
        node_admission: mode.node_admission_mode().to_string(),
        session_ladder: hosted_session_ladder(),
        action_matrix: hosted_action_matrix(mode),
    }
}

fn string_list(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn hosted_session_ladder() -> Vec<String> {
    string_list(HOSTED_SESSION_LADDER)
}

fn hosted_action_policy(
    action_id: &str,
    required_auth: &str,
    availability: &str,
    reason: &str,
) -> HostedActionAccessPolicy {
    HostedActionAccessPolicy {
        action_id: action_id.to_string(),
        required_auth: required_auth.to_string(),
        availability: availability.to_string(),
        reason: reason.to_string(),
    }
}

#[allow(dead_code)]
fn hosted_action_matrix(mode: DeploymentMode) -> Vec<HostedActionAccessPolicy> {
    let hosted_strong_auth_backend_grant_enabled = hosted_strong_auth_backend_grant_enabled();
    let prompt_strong_auth_availability = match mode {
        DeploymentMode::TrustedLocalOnly => "trusted_local_preview_only",
        DeploymentMode::HostedPublicJoin if hosted_strong_auth_backend_grant_enabled => {
            "public_player_plane_with_backend_reauth_preview"
        }
        DeploymentMode::HostedPublicJoin => "blocked_until_strong_auth",
    };
    let prompt_strong_auth_reason = match mode {
        DeploymentMode::TrustedLocalOnly => {
            "trusted local preview may still use preview bootstrap; hosted/public strong-auth lane remains pending"
        }
        DeploymentMode::HostedPublicJoin if hosted_strong_auth_backend_grant_enabled => {
            "hosted public join allows prompt_control through browser-local player auth plus short-lived backend strong-auth grant; this remains preview-grade until stronger custody lands"
        }
        DeploymentMode::HostedPublicJoin => {
            "hosted public join keeps this action behind strong_auth/private plane until the dedicated proof lane lands"
        }
    };
    let asset_strong_auth_availability = match mode {
        DeploymentMode::TrustedLocalOnly => "trusted_local_preview_only",
        DeploymentMode::HostedPublicJoin => "blocked_until_strong_auth",
    };
    let asset_strong_auth_reason = match mode {
        DeploymentMode::TrustedLocalOnly => {
            "trusted local preview may still use preview bootstrap; hosted/public strong-auth lane remains pending"
        }
        DeploymentMode::HostedPublicJoin => {
            "hosted public join keeps this action behind strong_auth/private plane until the dedicated proof lane lands"
        }
    };
    let mut matrix = Vec::with_capacity(3 + PROMPT_CONTROL_ACTION_IDS.len());
    matrix.push(hosted_action_policy(
        "gameplay_action",
        "player_session",
        "public_player_plane",
        "core gameplay input stays on the player_session lane",
    ));
    matrix.push(hosted_action_policy(
        "agent_chat",
        "player_session",
        "public_player_plane",
        "agent chat currently stays on the low-risk player_session lane",
    ));
    matrix.extend(PROMPT_CONTROL_ACTION_IDS.iter().map(|action_id| {
        hosted_action_policy(
            action_id,
            "strong_auth",
            prompt_strong_auth_availability,
            prompt_strong_auth_reason,
        )
    }));
    matrix.push(hosted_action_policy(
        "main_token_transfer",
        "strong_auth",
        asset_strong_auth_availability,
        asset_strong_auth_reason,
    ));
    matrix
}

#[allow(dead_code)]
pub fn web_launcher_public_endpoints() -> &'static [&'static str] {
    &[
        "/healthz",
        "/api/public/player-session/admission",
        "/api/public/player-session/refresh",
        "/api/public/hosted-account/v2/capabilities",
        "/api/public/hosted-account/v2/session",
        "/api/public/hosted-account/v2/passkeys/registration/options",
        "/api/public/hosted-account/v2/passkeys/registration/complete",
        "/api/public/hosted-account/v2/passkeys/authentication/options",
        "/api/public/hosted-account/v2/passkeys/authentication/complete",
        "/api/public/hosted-account/v2/logout",
        "/api/public/hosted-account/v2/credentials",
        "/api/public/hosted-account/v2/sessions",
        "/api/public/hosted-account/v2/actions",
        "/api/public/hosted-account/v2/email/login/options",
        "/api/public/hosted-account/v2/email/login/complete",
        "/api/public/hosted-account/v2/email/binding/options",
        "/api/public/hosted-account/v2/email/binding/complete",
        "/api/public/hosted-account/v2/email/unbind",
        "/api/public/hosted-account/v2/recovery/eligibility/options",
        "/api/public/hosted-account/v2/recovery/eligibility",
        "/api/public/hosted-account/v2/recovery/candidate/options",
        "/api/public/hosted-account/v2/recovery/candidate/complete",
        "/api/public/hosted-account/v2/recovery",
        "/api/public/hosted-account/v2/world/join",
        "/api/public/hosted-account/v2/world/refresh",
        "/api/public/hosted-account/v2/world/release",
        "/api/public/hosted-account/v2/account/freeze",
        "/api/public/hosted-account/login/start",
        "/api/public/hosted-account/login/complete",
        "/api/public/hosted-account/test-login",
        "/api/public/state",
        "/api/public/player-session/issue",
        "/api/public/player-session/release",
        "/api/public/strong-auth/grant",
        "/api/public/director/capability",
        "/api/chain/transfer",
        "/api/chain/transfer/accounts",
        "/api/chain/transfer/status",
        "/api/chain/transfer/history",
        "/api/chain/explorer/overview",
        "/api/chain/explorer/transactions",
        "/api/chain/explorer/transaction",
        "/api/chain/explorer/blocks",
        "/api/chain/explorer/block",
        "/api/chain/explorer/txs",
        "/api/chain/explorer/tx",
        "/api/chain/explorer/search",
        "/api/chain/explorer/address",
        "/api/chain/explorer/contracts",
        "/api/chain/explorer/contract",
        "/api/chain/explorer/assets",
        "/api/chain/explorer/mempool",
        "/api/chain/feedback",
    ]
}

fn hosted_strong_auth_backend_grant_enabled() -> bool {
    env_non_empty(HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV)
        && env_non_empty(HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV)
        && env_non_empty(HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV)
}

fn env_non_empty(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|raw| !raw.trim().is_empty())
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn web_launcher_private_endpoints() -> &'static [&'static str] {
    &[
        "/",
        "/api/state",
        "/api/gui-agent/capabilities",
        "/api/gui-agent/state",
        "/api/gui-agent/action",
        "/api/ui/schema",
        "/api/start",
        "/api/stop",
        "/api/chain/start",
        "/api/chain/stop",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_control_apply_policy(mode: DeploymentMode) -> HostedActionAccessPolicy {
        hosted_viewer_access_hint(mode)
            .action_matrix
            .into_iter()
            .find(|policy| policy.action_id == "prompt_control_apply")
            .expect("prompt_control_apply policy")
    }

    fn main_token_transfer_policy(mode: DeploymentMode) -> HostedActionAccessPolicy {
        hosted_viewer_access_hint(mode)
            .action_matrix
            .into_iter()
            .find(|policy| policy.action_id == "main_token_transfer")
            .expect("main_token_transfer policy")
    }

    #[test]
    fn hosted_access_verdict_tracks_deployment_mode_and_backend_grant_readiness() {
        let ready = [
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new("public-key"),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new("private-key"),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("approval"),
            ),
        ];
        let Some(scenario) = hosted_test_env::run_scenarios(&[("absent", &[]), ("ready", &ready)])
        else {
            return;
        };
        assert_eq!(
            hosted_viewer_access_hint(DeploymentMode::TrustedLocalOnly).verdict,
            "trusted_local_only_preview"
        );
        if scenario == 0 {
            assert_eq!(
                hosted_viewer_access_hint(DeploymentMode::HostedPublicJoin).verdict,
                "hosted_public_join_blocked_until_strong_auth"
            );
            return;
        }
        assert_eq!(
            hosted_viewer_access_hint(DeploymentMode::HostedPublicJoin).verdict,
            "hosted_public_join_strong_auth_preview"
        );
    }

    #[test]
    fn hosted_public_join_prompt_control_stays_blocked_without_backend_grant_env() {
        if !hosted_test_env::run(&[]) {
            return;
        }
        let policy = prompt_control_apply_policy(DeploymentMode::HostedPublicJoin);
        assert_eq!(policy.required_auth, "strong_auth");
        assert_eq!(policy.availability, "blocked_until_strong_auth");
    }

    #[test]
    fn hosted_public_join_prompt_control_exposes_backend_reauth_preview_when_env_ready() {
        if !hosted_test_env::run(&[
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new(
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                ),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("preview-code"),
            ),
        ]) {
            return;
        }
        let policy = prompt_control_apply_policy(DeploymentMode::HostedPublicJoin);
        assert_eq!(
            policy.availability,
            "public_player_plane_with_backend_reauth_preview"
        );
        assert!(policy.reason.contains("backend strong-auth grant"));
    }

    #[test]
    fn hosted_public_join_main_token_transfer_stays_blocked_even_when_prompt_reauth_env_ready() {
        if !hosted_test_env::run(&[
            (
                HOSTED_STRONG_AUTH_PUBLIC_KEY_ENV,
                std::ffi::OsStr::new(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
            ),
            (
                HOSTED_STRONG_AUTH_PRIVATE_KEY_ENV,
                std::ffi::OsStr::new(
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                ),
            ),
            (
                HOSTED_STRONG_AUTH_APPROVAL_CODE_ENV,
                std::ffi::OsStr::new("preview-code"),
            ),
        ]) {
            return;
        }
        let policy = main_token_transfer_policy(DeploymentMode::HostedPublicJoin);
        assert_eq!(policy.required_auth, "strong_auth");
        assert_eq!(policy.availability, "blocked_until_strong_auth");
        assert!(policy.reason.contains("dedicated proof lane"));
    }

    #[test]
    fn web_launcher_public_endpoints_expose_generic_strong_auth_grant_route() {
        assert!(web_launcher_public_endpoints().contains(&"/api/public/strong-auth/grant"));
        assert!(
            !web_launcher_public_endpoints()
                .contains(&"/api/public/strong-auth/grant/prompt-control")
        );
    }

    #[test]
    fn web_launcher_public_endpoints_expose_director_capability_status_route() {
        assert!(web_launcher_public_endpoints().contains(&"/api/public/director/capability"));
    }

    #[test]
    fn web_launcher_public_endpoints_expose_hosted_account_login_routes() {
        assert!(
            web_launcher_public_endpoints().contains(&"/api/public/hosted-account/login/start")
        );
        assert!(
            web_launcher_public_endpoints().contains(&"/api/public/hosted-account/login/complete")
        );
        assert!(web_launcher_public_endpoints().contains(&"/api/public/hosted-account/test-login"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod auth_storage;
