use super::director_capability::{DIRECTOR_CAPABILITY_ROUTE, director_capability_unavailable};
use super::hosted_account_identity::{
    HOSTED_ACCOUNT_LOGIN_COMPLETE_ROUTE, HOSTED_ACCOUNT_LOGIN_START_ROUTE,
    HOSTED_ACCOUNT_TEST_LOGIN_ROUTE, HostedAccountIdentityBroker,
    HostedAccountLoginCompleteResponse, HostedAccountLoginStartResponse,
};
use super::hosted_player_session::{
    HOSTED_PLAYER_SESSION_ADMISSION_ROUTE, HOSTED_PLAYER_SESSION_ISSUE_ROUTE,
    HOSTED_PLAYER_SESSION_REFRESH_ROUTE, HOSTED_PLAYER_SESSION_RELEASE_ROUTE,
    HostedPlayerSessionAdmissionResponse, HostedPlayerSessionIssueResponse,
    HostedPlayerSessionIssuer, HostedPlayerSessionReleaseResponse,
};
use super::hosted_strong_auth::{
    HOSTED_PROMPT_CONTROL_STRONG_AUTH_GRANT_ROUTE, HOSTED_STRONG_AUTH_GRANT_ROUTE,
    HostedStrongAuthGrantResponse, issue_hosted_strong_auth_grant,
};
use super::runtime_presence::query_runtime_bound_players_with_probe_sequence;
use super::*;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::io::{ErrorKind, Read};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "static_http_public_payload.rs"]
mod public_payload;

const MAX_HTTP_REQUEST_BYTES: usize = 64 * 1024;
const HOSTED_TEST_LOGIN_ENABLED_ENV: &str = "OASIS7_HOSTED_TEST_LOGIN_ENABLED";
#[cfg(test)]
#[path = "identity_request_security_tests.rs"]
mod identity_request_security_tests;
#[path = "static_http_identity_security.rs"]
mod identity_security;
#[cfg(test)]
#[path = "static_http_passkey_fixture.rs"]
mod passkey_fixture;

pub(super) struct ConnectionBudget(Arc<std::sync::atomic::AtomicUsize>);
pub(super) struct ConnectionPermit(Arc<std::sync::atomic::AtomicUsize>);
impl ConnectionBudget {
    pub(super) fn new() -> Self {
        Self(Arc::new(std::sync::atomic::AtomicUsize::new(0)))
    }
    pub(super) fn try_acquire(&self) -> Option<ConnectionPermit> {
        self.0
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |count| if count < 32 { Some(count + 1) } else { None },
            )
            .ok()
            .map(|_| ConnectionPermit(Arc::clone(&self.0)))
    }
}
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[derive(Debug, Default, Deserialize)]
struct HostedAccountLoginStartRequest {
    #[serde(default)]
    channel: String,
    #[serde(default)]
    handle: String,
}

#[derive(Debug, Default, Deserialize)]
struct HostedAccountLoginCompleteRequest {
    #[serde(default)]
    challenge_id: String,
    #[serde(default)]
    otp_code: String,
    #[serde(default)]
    public_key: String,
}

#[derive(Debug, Default, Deserialize)]
struct HostedPlayerSessionRefreshRequest {
    #[serde(default)]
    player_id: String,
    #[serde(default)]
    release_token: String,
    #[serde(default)]
    public_key: String,
}

#[derive(Debug, Default, Deserialize)]
struct HostedTestLoginRequest {
    #[serde(default)]
    public_key: String,
}

pub(super) struct StaticHttpRuntimeConfig {
    pub(super) live_bind: String,
    viewer_runtime_config_json: String,
    pub(super) identity: Arc<Mutex<Option<super::hosted_identity::IdentityService>>>,
    pub(super) model_client: Option<super::hosted_model_client::HostedModelClient>,
}

impl StaticHttpRuntimeConfig {
    pub(super) fn new(
        deployment_mode: DeploymentMode,
        live_bind: &str,
        web_bind: &str,
    ) -> Result<Self, String> {
        let identity = if deployment_mode == DeploymentMode::HostedPublicJoin {
            super::hosted_identity::IdentityConfig::from_env()?
                .map(|mut config| {
                    config.runtime_live_bind = live_bind.to_owned();
                    super::hosted_identity::IdentityService::new(config)
                })
                .transpose()?
        } else {
            None
        };
        let identity = Arc::new(Mutex::new(identity));
        let model_client = super::hosted_model_client::HostedModelClient::from_env(&identity)?;
        Ok(Self {
            live_bind: live_bind.to_string(),
            viewer_runtime_config_json: viewer_runtime_config_json(deployment_mode, web_bind)?,
            identity,
            model_client,
        })
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
pub(super) fn handle_http_connection(
    mut stream: TcpStream,
    root_dir: &Path,
    runtime_config: &StaticHttpRuntimeConfig,
    allow_hosted_test_login: bool,
    default_viewer_player_id: Option<&str>,
    deployment_mode: DeploymentMode,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
    hosted_account_broker: &Arc<Mutex<HostedAccountIdentityBroker>>,
) -> Result<(), String> {
    let live_bind = runtime_config.live_bind.as_str();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;

    let request_bytes = zeroize::Zeroizing::new(read_http_request_bytes(&mut stream)?);
    if request_bytes.is_empty() {
        return Ok(());
    }

    let request = std::str::from_utf8(request_bytes.as_slice())
        .map_err(|_| "invalid HTTP request encoding")?;
    let Some(line) = request.lines().next() else {
        write_http_response(&mut stream, 400, "text/plain", b"Bad Request", false)
            .map_err(|err| format!("failed to write 400 response: {err}"))?;
        return Ok(());
    };

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let request_body = extract_request_body(request);

    let head_only = method.eq_ignore_ascii_case("HEAD");
    let path_only = target.split('?').next().unwrap_or(target);
    if path_only == super::hosted_model_client::PREFIX || path_only.starts_with("/v1/agent-client/")
    {
        let reply = super::hosted_model_client::handle_request(
            runtime_config,
            method,
            target,
            request,
            request_body,
        );
        write_identity_response(&mut stream, &reply)?;
        return Ok(());
    }
    {
        let mut identity = runtime_config
            .identity
            .lock()
            .map_err(|_| "identity lock poisoned")?;
        if let Some(service) = identity.as_mut() {
            if path_only.starts_with("/api/public/hosted-account/v2/") {
                let payload = if method == "GET" {
                    serde_json::json!({"cursor":parse_query_value(target,"cursor")})
                } else {
                    match identity_security::parse_identity_json(request_body.as_bytes()) {
                        Ok(value) => value,
                        Err(_) => {
                            write_http_response(
                                &mut stream,
                                400,
                                "application/json",
                                b"{\"ok\":false,\"error_code\":\"invalid_request\"}",
                                false,
                            )
                            .map_err(|_| "identity response failed")?;
                            return Ok(());
                        }
                    }
                };
                let peer = stream
                    .peer_addr()
                    .map(|a| a.ip())
                    .map_err(|_| "identity peer unavailable")?;
                let source = identity_source(peer, service.config.trusted_proxy_loopback, request)?;
                let reply = service.handle(
                    method,
                    path_only,
                    request,
                    payload,
                    &source,
                    &mut *hosted_session_issuer
                        .lock()
                        .map_err(|_| "session issuer lock poisoned")?,
                );
                drop(identity);
                if let Some(client) = &runtime_config.model_client {
                    let _ = client.manager.revoke_invalid_sessions();
                }
                write_identity_response(&mut stream, &reply)?;
                return Ok(());
            }
            if path_only.starts_with("/api/public/hosted-account/")
                || matches!(
                    path_only,
                    HOSTED_PLAYER_SESSION_REFRESH_ROUTE
                        | HOSTED_PLAYER_SESSION_ISSUE_ROUTE
                        | HOSTED_PLAYER_SESSION_RELEASE_ROUTE
                )
            {
                write_http_response(
                    &mut stream,
                    401,
                    "application/json",
                    b"{\"ok\":false,\"error_code\":\"identity_v2_required\"}",
                    false,
                )
                .map_err(|_| "identity response failed")?;
                return Ok(());
            }
        }
    }
    let is_admission_route = path_only == HOSTED_PLAYER_SESSION_ADMISSION_ROUTE;
    let is_refresh_route = path_only == HOSTED_PLAYER_SESSION_REFRESH_ROUTE;
    let is_issue_route = path_only == HOSTED_PLAYER_SESSION_ISSUE_ROUTE;
    let is_release_route = path_only == HOSTED_PLAYER_SESSION_RELEASE_ROUTE;
    let is_login_start_route = path_only == HOSTED_ACCOUNT_LOGIN_START_ROUTE;
    let is_login_complete_route = path_only == HOSTED_ACCOUNT_LOGIN_COMPLETE_ROUTE;
    let is_test_login_route = path_only == HOSTED_ACCOUNT_TEST_LOGIN_ROUTE;
    let is_strong_auth_grant_route = path_only == HOSTED_STRONG_AUTH_GRANT_ROUTE
        || path_only == HOSTED_PROMPT_CONTROL_STRONG_AUTH_GRANT_ROUTE;
    let is_director_capability_route = path_only == DIRECTOR_CAPABILITY_ROUTE;
    let is_hosted_player_route = is_admission_route
        || is_refresh_route
        || is_issue_route
        || is_release_route
        || is_login_start_route
        || is_login_complete_route
        || is_test_login_route
        || is_strong_auth_grant_route
        || is_director_capability_route;
    let post_only_route = is_issue_route
        || is_release_route
        || is_refresh_route
        || is_login_start_route
        || is_login_complete_route
        || is_test_login_route;
    let head_mutation_route = is_issue_route
        || is_release_route
        || is_refresh_route
        || is_login_start_route
        || is_login_complete_route
        || is_test_login_route
        || is_strong_auth_grant_route;
    let get_allowed = method.eq_ignore_ascii_case("GET") && !post_only_route;
    let post_allowed = method.eq_ignore_ascii_case("POST") && post_only_route;
    let head_allowed = head_only && !head_mutation_route;
    if !get_allowed && !post_allowed && !head_allowed {
        write_http_response(&mut stream, 405, "text/plain", b"Method Not Allowed", false)
            .map_err(|err| format!("failed to write 405 response: {err}"))?;
        return Ok(());
    }
    if is_hosted_player_route {
        reconcile_hosted_runtime_presence(live_bind, hosted_session_issuer);
    }
    if is_admission_route {
        let response = hosted_player_session_admission(deployment_mode, hosted_session_issuer)?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted session admission response: {err}"))?;
        return Ok(());
    }
    if is_refresh_route {
        let payload: HostedPlayerSessionRefreshRequest =
            serde_json::from_str(request_body).unwrap_or_default();
        let player_id = if payload.player_id.trim().is_empty() {
            parse_query_value(target, "player_id").unwrap_or_default()
        } else {
            payload.player_id
        };
        let release_token = if payload.release_token.trim().is_empty() {
            parse_query_value(target, "release_token").unwrap_or_default()
        } else {
            payload.release_token
        };
        let response = refresh_hosted_player_session(
            deployment_mode,
            player_id.as_str(),
            release_token.as_str(),
            Some(payload.public_key.as_str()),
            hosted_session_issuer,
        )?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted session refresh response: {err}"))?;
        return Ok(());
    }
    if is_issue_route {
        write_http_response(
            &mut stream,
            401,
            "text/plain",
            b"Account login required",
            false,
        )
        .map_err(|err| format!("failed to write hosted session issue rejection: {err}"))?;
        return Ok(());
    }
    if is_release_route {
        let player_id = parse_query_value(target, "player_id").unwrap_or_default();
        let release_token = parse_query_value(target, "release_token").unwrap_or_default();
        let response = release_hosted_player_session(
            deployment_mode,
            player_id.as_str(),
            release_token.as_str(),
            hosted_session_issuer,
        )?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted session release response: {err}"))?;
        return Ok(());
    }
    if is_login_start_route {
        let payload = parse_login_start_request(request_body).unwrap_or_default();
        let login_channel = if payload.channel.trim().is_empty() {
            parse_query_value(target, "channel").unwrap_or_default()
        } else {
            payload.channel
        };
        let login_hint = if payload.handle.trim().is_empty() {
            parse_query_value(target, "handle").unwrap_or_default()
        } else {
            payload.handle
        };
        let response = start_hosted_account_login(
            deployment_mode,
            login_channel.as_str(),
            login_hint.as_str(),
            hosted_account_broker,
        )?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted account login start response: {err}"))?;
        return Ok(());
    }
    if is_login_complete_route {
        let payload = parse_login_complete_request(request_body).unwrap_or_default();
        let challenge_id = if payload.challenge_id.trim().is_empty() {
            parse_query_value(target, "challenge_id").unwrap_or_default()
        } else {
            payload.challenge_id
        };
        let otp_code = if payload.otp_code.trim().is_empty() {
            parse_query_value(target, "otp_code").unwrap_or_default()
        } else {
            payload.otp_code
        };
        let response = complete_hosted_account_login(
            deployment_mode,
            challenge_id.as_str(),
            otp_code.as_str(),
            payload.public_key.as_str(),
            hosted_session_issuer,
            hosted_account_broker,
        )?;
        write_json_response(&mut stream, 200, &response, head_only).map_err(|err| {
            format!("failed to write hosted account login complete response: {err}")
        })?;
        return Ok(());
    }
    if is_test_login_route {
        let enabled = deployment_mode == DeploymentMode::HostedPublicJoin
            && allow_hosted_test_login
            && std::env::var(HOSTED_TEST_LOGIN_ENABLED_ENV).ok().as_deref() == Some("1");
        if !enabled {
            write_http_response(&mut stream, 404, "text/plain", b"Not Found", head_only).map_err(
                |err| format!("failed to write hosted test login disabled response: {err}"),
            )?;
            return Ok(());
        }
        let payload: HostedTestLoginRequest =
            serde_json::from_str(request_body).unwrap_or_default();
        let public_key = payload.public_key.trim();
        if public_key.is_empty() {
            write_http_response(
                &mut stream,
                400,
                "text/plain",
                b"public_key is required",
                head_only,
            )
            .map_err(|err| {
                format!("failed to write hosted test login validation response: {err}")
            })?;
            return Ok(());
        }
        let response = issue_hosted_test_login(deployment_mode, public_key, hosted_session_issuer)?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted test login response: {err}"))?;
        return Ok(());
    }
    if is_strong_auth_grant_route {
        let player_id = parse_query_value(target, "player_id").unwrap_or_default();
        let public_key = parse_query_value(target, "public_key").unwrap_or_default();
        let agent_id = parse_query_value(target, "agent_id").unwrap_or_default();
        let action_id = parse_query_value(target, "action_id").unwrap_or_default();
        let approval_code = parse_query_value(target, "approval_code").unwrap_or_default();
        let release_token = parse_query_value(target, "release_token").unwrap_or_default();
        let response = issue_strong_auth_grant(
            deployment_mode,
            player_id.as_str(),
            public_key.as_str(),
            agent_id.as_str(),
            action_id.as_str(),
            approval_code.as_str(),
            release_token.as_str(),
            hosted_session_issuer,
        )?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write hosted strong-auth grant response: {err}"))?;
        return Ok(());
    }
    if is_director_capability_route {
        let response = director_capability_status(deployment_mode, hosted_session_issuer)?;
        write_json_response(&mut stream, 200, &response, head_only)
            .map_err(|err| format!("failed to write Director capability status response: {err}"))?;
        return Ok(());
    }

    let resolved = match resolve_static_asset_path(root_dir, target) {
        Ok(resolved) => resolved,
        Err(_) => {
            write_http_response(&mut stream, 400, "text/plain", b"Bad Request", head_only)
                .map_err(|err| format!("failed to write 400 response: {err}"))?;
            return Ok(());
        }
    };

    match resolved {
        Some(path) => {
            let public_payload = public_payload::response(root_dir, &path, request)?;
            let (body, payload_headers) = if let Some(payload) = public_payload {
                payload
            } else {
                (
                    fs::read(&path).map_err(|err| {
                        format!("failed to read static asset `{}`: {err}", path.display())
                    })?,
                    String::new(),
                )
            };
            let viewer_auth_bootstrap = resolve_viewer_auth_bootstrap_for_embedded_server(
                deployment_mode,
                default_viewer_player_id,
            );
            let body = sanitize_index_html_for_embedded_server(
                path.as_path(),
                body.as_slice(),
                viewer_auth_bootstrap.as_ref(),
            );
            let body = if path.extension() == Some(OsStr::new("html")) {
                super::viewer_runtime_config::inject_config(
                    &body,
                    &runtime_config.viewer_runtime_config_json,
                )?
            } else {
                body
            };
            let mut security_headers = if path.extension() == Some(OsStr::new("html"))
                && runtime_config
                    .identity
                    .lock()
                    .map_err(|_| "identity lock poisoned")?
                    .is_some()
            {
                identity_security::hosted_html_security_headers(
                    &body,
                    &runtime_config.viewer_runtime_config_json,
                )?
            } else {
                String::new()
            };
            if !payload_headers.is_empty() {
                security_headers.push_str(&payload_headers);
            } else if !security_headers.contains("Cache-Control:") {
                security_headers.push_str("Cache-Control: no-cache\r\n");
            }
            write_http_response_with_headers(
                &mut stream,
                200,
                content_type_for_path(path.as_path()),
                body.as_slice(),
                head_only,
                &security_headers,
            )
            .map_err(|err| format!("failed to write 200 response: {err}"))?;
        }
        None => {
            write_http_response(&mut stream, 404, "text/plain", b"Not Found", head_only)
                .map_err(|err| format!("failed to write 404 response: {err}"))?;
        }
    }

    Ok(())
}

pub(super) fn hosted_test_login_allowed_on_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    trimmed
        .trim_matches(|character| character == '[' || character == ']')
        .parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn start_hosted_account_login(
    deployment_mode: DeploymentMode,
    login_channel: &str,
    login_hint: &str,
    hosted_account_broker: &Arc<Mutex<HostedAccountIdentityBroker>>,
) -> Result<HostedAccountLoginStartResponse, String> {
    let reserved = {
        let mut broker = hosted_account_broker
            .lock()
            .map_err(|_| "hosted account broker lock poisoned".to_string())?;
        match broker.reserve_login_start(deployment_mode, login_channel, login_hint) {
            Ok(reserved) => reserved,
            Err(response) => return Ok(response),
        }
    };
    if let Err(err) = reserved.deliver() {
        let mut broker = hosted_account_broker
            .lock()
            .map_err(|_| "hosted account broker lock poisoned".to_string())?;
        broker.rollback_reserved_login_start(&reserved);
        let (channel, masked_login_hint) = reserved.masked_target();
        let log_message = format!(
            "hosted account login delivery failed: channel={} target={} reason={err}",
            channel, masked_login_hint
        );
        emit_stderr_or_event(
            Level::ERROR,
            log_message.as_str(),
            "hosted account login delivery failed",
        );
        return Ok(HostedAccountLoginStartResponse {
            ok: false,
            error_code: Some("login_delivery_failed".to_string()),
            error: Some("failed to deliver login verification code; retry shortly".to_string()),
            retry_after_seconds: Some(5),
            deployment_mode: deployment_mode.as_str().to_string(),
            challenge: None,
        });
    }
    Ok(reserved.success_response(deployment_mode))
}

fn complete_hosted_account_login(
    deployment_mode: DeploymentMode,
    challenge_id: &str,
    otp_code: &str,
    public_key: &str,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
    hosted_account_broker: &Arc<Mutex<HostedAccountIdentityBroker>>,
) -> Result<HostedAccountLoginCompleteResponse, String> {
    let mut broker = hosted_account_broker
        .lock()
        .map_err(|_| "hosted account broker lock poisoned".to_string())?;
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(broker.complete_login_with_key(
        deployment_mode,
        challenge_id,
        otp_code,
        Some(public_key),
        &mut issuer,
    ))
}

fn reconcile_hosted_runtime_presence(
    live_bind: &str,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) {
    let probe_result = query_runtime_bound_players_with_probe_sequence(live_bind);
    let Ok(mut issuer) = hosted_session_issuer.lock() else {
        return;
    };
    match probe_result {
        Ok((probe_sequence, active_players)) => {
            issuer.observe_runtime_active_players_for_probe(
                probe_sequence,
                active_players.iter().map(String::as_str),
            );
        }
        Err(err) => issuer.record_runtime_probe_failure(err),
    }
}

fn hosted_player_session_admission(
    deployment_mode: DeploymentMode,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<HostedPlayerSessionAdmissionResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(issuer.admission(deployment_mode))
}

fn issue_hosted_test_login(
    deployment_mode: DeploymentMode,
    public_key: &str,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<HostedPlayerSessionIssueResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(issuer.issue_with_key(deployment_mode, public_key))
}

fn refresh_hosted_player_session(
    deployment_mode: DeploymentMode,
    player_id: &str,
    release_token: &str,
    registration_public_key: Option<&str>,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<HostedPlayerSessionAdmissionResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(issuer.refresh(
        deployment_mode,
        player_id,
        release_token,
        registration_public_key,
    ))
}

fn release_hosted_player_session(
    deployment_mode: DeploymentMode,
    player_id: &str,
    release_token: &str,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<HostedPlayerSessionReleaseResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(issuer.release(deployment_mode, player_id, release_token))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
fn issue_strong_auth_grant(
    deployment_mode: DeploymentMode,
    player_id: &str,
    public_key: &str,
    agent_id: &str,
    action_id: &str,
    approval_code: &str,
    release_token: &str,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<HostedStrongAuthGrantResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(issue_hosted_strong_auth_grant(
        deployment_mode,
        player_id,
        public_key,
        agent_id,
        action_id,
        approval_code,
        release_token,
        &mut issuer,
    ))
}

fn director_capability_status(
    deployment_mode: DeploymentMode,
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
) -> Result<super::director_capability::DirectorCapabilityResponse, String> {
    let mut issuer = hosted_session_issuer
        .lock()
        .map_err(|_| "hosted session issuer lock poisoned".to_string())?;
    Ok(director_capability_unavailable(
        deployment_mode,
        &mut issuer,
    ))
}

fn read_http_request_bytes(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    let mut expected_total_len = None;
    loop {
        let bytes = match stream.read(&mut chunk) {
            Ok(bytes) => bytes,
            Err(err)
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut =>
            {
                if buffer.is_empty() {
                    return Ok(Vec::new());
                }
                return Err("request body timed out before completion".to_string());
            }
            Err(err) => return Err(format!("failed to read request: {err}")),
        };
        if bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes]);
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            return Err(format!(
                "request exceeded max size of {MAX_HTTP_REQUEST_BYTES} bytes"
            ));
        }
        if expected_total_len.is_none()
            && let Some((header_end, delimiter_len)) = find_header_terminator(buffer.as_slice())
        {
            let header = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = parse_content_length(header.as_ref())?;
            expected_total_len = Some(
                header_end
                    .saturating_add(delimiter_len)
                    .saturating_add(content_length),
            );
        }
        if let Some(expected_total_len) = expected_total_len
            && buffer.len() >= expected_total_len
        {
            buffer.truncate(expected_total_len);
            break;
        }
    }
    Ok(buffer)
}

fn find_header_terminator(buffer: &[u8]) -> Option<(usize, usize)> {
    for (pattern, delimiter_len) in [
        (b"\r\n\r\n".as_slice(), 4usize),
        (b"\n\n".as_slice(), 2usize),
    ] {
        if let Some(index) = buffer
            .windows(pattern.len())
            .position(|window| window == pattern)
        {
            return Some((index, delimiter_len));
        }
    }
    None
}

fn parse_content_length(headers: &str) -> Result<usize, String> {
    let mut length = None;
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("duplicate Content-Length header".into());
            }
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| "invalid Content-Length header".to_string())?;
            if parsed > MAX_HTTP_REQUEST_BYTES {
                return Err("request body too large".into());
            }
            length = Some(parsed);
        }
        if name.trim().eq_ignore_ascii_case("transfer-encoding") {
            return Err("Transfer-Encoding is unsupported".into());
        }
    }
    Ok(length.unwrap_or(0))
}

fn extract_request_body(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .or_else(|| request.split_once("\n\n").map(|(_, body)| body))
        .unwrap_or("")
}

fn parse_login_start_request(body: &str) -> Option<HostedAccountLoginStartRequest> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn parse_login_complete_request(body: &str) -> Option<HostedAccountLoginCompleteRequest> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn parse_query_value(target: &str, key: &str) -> Option<String> {
    let query = target.split('?').nth(1)?;
    for pair in query.split('&') {
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        if raw_key == key {
            return Some(percent_decode(raw_value));
        }
    }
    None
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = from_hex(bytes[index + 1]);
            let lo = from_hex(bytes[index + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                decoded.push((hi << 4) | lo);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(decoded.as_slice()).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn resolve_static_asset_path(
    root_dir: &Path,
    raw_target: &str,
) -> Result<Option<PathBuf>, String> {
    let path_only = raw_target
        .split('?')
        .next()
        .unwrap_or(raw_target)
        .split('#')
        .next()
        .unwrap_or(raw_target);

    let relative = sanitize_relative_request_path(path_only)?;
    let direct_path = if relative.as_os_str().is_empty() {
        root_dir.join("index.html")
    } else {
        root_dir.join(relative.as_path())
    };

    if direct_path.is_file() {
        return Ok(Some(direct_path));
    }

    let has_extension = Path::new(path_only)
        .file_name()
        .and_then(|name| Path::new(name).extension())
        .is_some();
    if !has_extension {
        let spa_index = root_dir.join("index.html");
        if spa_index.is_file() {
            return Ok(Some(spa_index));
        }
    }

    Ok(None)
}

pub(super) fn sanitize_relative_request_path(raw_path: &str) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Ok(PathBuf::new());
    }

    let normalized = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut cleaned = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\\') {
            return Err("path traversal is not allowed".to_string());
        }
        cleaned.push(segment);
    }

    Ok(cleaned)
}

pub(super) fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("map") => "application/json; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub(super) fn sanitize_index_html_for_embedded_server(
    path: &Path,
    body: &[u8],
    viewer_auth_bootstrap: Option<&ViewerAuthBootstrap>,
) -> Vec<u8> {
    if path.extension() != Some(OsStr::new("html")) {
        return body.to_vec();
    }
    let sanitized = if path.file_name() == Some(OsStr::new("index.html")) {
        strip_trunk_autoreload_script(body)
    } else {
        body.to_vec()
    };
    if let Some(viewer_auth_bootstrap) = viewer_auth_bootstrap {
        inject_viewer_auth_bootstrap_script(sanitized.as_slice(), viewer_auth_bootstrap)
    } else {
        sanitized
    }
}

pub(super) fn viewer_runtime_config_json(
    deployment_mode: DeploymentMode,
    web_bind: &str,
) -> Result<String, String> {
    let configured = std::env::var("OASIS7_VIEWER_PUBLIC_WS_URL").ok();
    let config = super::viewer_runtime_config::viewer_runtime_config_json(
        deployment_mode,
        web_bind,
        configured.as_deref(),
    )?;
    match std::env::var("OASIS7_BROWSER_BYOK_ENABLED").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => Ok(config),
        Ok("1") => {
            for flag in [
                "OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED",
                "OASIS7_HOSTED_OWNER_CONTROL_ENABLED",
                "OASIS7_HOSTED_EXECUTOR_CONTROL_ENABLED",
            ] {
                if std::env::var(flag).as_deref() != Ok("1") {
                    return Err(format!("browser BYOK requires {flag}=1"));
                }
            }
            let trust = std::env::var("OASIS7_HOSTED_PARTICIPANT_ENTRY_TRUST")
                .map_err(|_| "browser BYOK requires participant trust")?;
            let config = super::viewer_runtime_config::enable_browser_game(&config, &trust)?;
            let mut value: serde_json::Value =
                serde_json::from_str(&config).map_err(|_| "invalid viewer config")?;
            let required =
                |name| std::env::var(name).map_err(|_| format!("browser BYOK requires {name}"));
            let epoch = required("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH")?
                .parse::<u64>()
                .map_err(|_| "invalid browser identity epoch")?;
            if epoch == 0 {
                return Err("invalid browser identity epoch".into());
            }
            let issuer =
                zeroize::Zeroizing::new(required("OASIS7_HOSTED_PARTICIPANT_ISSUER_PRIVATE_KEY")?);
            let seed = zeroize::Zeroizing::new(
                hex::decode(issuer.as_str()).map_err(|_| "invalid browser issuer")?,
            );
            let seed: &[u8; 32] = seed
                .as_slice()
                .try_into()
                .map_err(|_| "invalid browser issuer")?;
            value["browserGameClient"]["participantIssuerPublicKey"] =
                serde_json::json!(hex::encode(
                    ed25519_dalek::SigningKey::from_bytes(seed)
                        .verifying_key()
                        .to_bytes()
                ));
            value["browserGameClient"]["identity"] = serde_json::json!({"service_id":required("OASIS7_HOSTED_IDENTITY_SERVICE_ID")?,"environment_id":required("OASIS7_HOSTED_IDENTITY_ENVIRONMENT")?,"instance_epoch":epoch.to_string()});
            serde_json::to_string(&value).map_err(|_| "invalid viewer config".into())
        }
        _ => Err("OASIS7_BROWSER_BYOK_ENABLED must be 0 or 1".into()),
    }
}

fn strip_trunk_autoreload_script(body: &[u8]) -> Vec<u8> {
    let html = String::from_utf8_lossy(body);
    let marker = ".well-known/trunk/ws";
    let Some(marker_index) = html.find(marker) else {
        return body.to_vec();
    };
    let Some(script_start) = html[..marker_index].rfind("<script") else {
        return body.to_vec();
    };
    let Some(script_end_rel) = html[marker_index..].find("</script>") else {
        return body.to_vec();
    };
    let script_end = marker_index + script_end_rel + "</script>".len();

    let mut sanitized = String::with_capacity(html.len());
    sanitized.push_str(&html[..script_start]);
    sanitized.push_str(&html[script_end..]);
    sanitized.into_bytes()
}

fn inject_viewer_auth_bootstrap_script(body: &[u8], auth: &ViewerAuthBootstrap) -> Vec<u8> {
    let html = String::from_utf8_lossy(body);
    let script = build_viewer_auth_bootstrap_script(auth);
    let insert_at = html
        .rfind("</head>")
        .or_else(|| html.rfind("</body>"))
        .unwrap_or(html.len());
    let mut injected = String::with_capacity(html.len() + script.len() + 1);
    injected.push_str(&html[..insert_at]);
    injected.push_str(script.as_str());
    injected.push_str(&html[insert_at..]);
    injected.into_bytes()
}

pub(super) fn build_viewer_auth_bootstrap_script(auth: &ViewerAuthBootstrap) -> String {
    let payload = serde_json::json!({
        VIEWER_PLAYER_ID_ENV: auth.player_id,
        VIEWER_AUTH_PUBLIC_KEY_ENV: auth.public_key,
        VIEWER_AUTH_PRIVATE_KEY_ENV: auth.private_key,
    });
    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    format!(
        "<script>const __oasis7ViewerAuthEnv=Object.freeze({payload});window.{VIEWER_AUTH_BOOTSTRAP_OBJECT}=__oasis7ViewerAuthEnv;</script>"
    )
}

fn identity_source(peer: IpAddr, trusted_loopback: bool, headers: &str) -> Result<String, String> {
    let address = if trusted_loopback && peer.is_loopback() {
        super::hosted_identity::header(headers, "x-real-ip")
            .and_then(|s| s.parse::<IpAddr>().ok())
            .ok_or_else(|| "trusted identity proxy must supply one valid X-Real-IP".to_string())?
    } else {
        peer
    };
    Ok(match address {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|v| v.to_string())
            .unwrap_or_else(|| ip.to_string()),
        IpAddr::V4(ip) => ip.to_string(),
    })
}

fn write_identity_response(
    stream: &mut TcpStream,
    reply: &super::hosted_identity::IdentityReply,
) -> Result<(), String> {
    let body =
        serde_json::to_vec(&reply.body).map_err(|_| "identity response serialization failed")?;
    let mut headers = format!(
        "HTTP/1.1 {} Identity Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nConnection: close\r\n",
        reply.status,
        body.len()
    );
    for cookie in &reply.cookies {
        headers.push_str("Set-Cookie: ");
        headers.push_str(cookie);
        headers.push_str("\r\n");
    }
    if reply.status == 429 {
        headers.push_str("Retry-After: 60\r\n");
    }
    headers.push_str("\r\n");
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|_| "identity response write failed".into())
}

fn write_json_response<T: Serialize>(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &T,
    head_only: bool,
) -> Result<(), String> {
    let body =
        serde_json::to_vec(payload).map_err(|err| format!("serialize JSON failed: {err}"))?;
    write_http_response(
        stream,
        status_code,
        "application/json; charset=utf-8",
        body.as_slice(),
        head_only,
    )
    .map_err(|err| format!("write JSON response failed: {err}"))
}

fn write_http_response(
    stream: &mut TcpStream,
    status_code: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    write_http_response_with_headers(stream, status_code, content_type, body, head_only, "")
}

fn write_http_response_with_headers(
    stream: &mut TcpStream,
    status_code: u16,
    content_type: &str,
    body: &[u8],
    head_only: bool,
    security_headers: &str,
) -> std::io::Result<()> {
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{security_headers}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()?;
    Ok(())
}

pub(super) fn resolve_viewer_auth_bootstrap_from_path(
    path: &Path,
    default_viewer_player_id: Option<&str>,
) -> Result<ViewerAuthBootstrap, String> {
    let content =
        fs::read_to_string(path).map_err(|err| format!("read {} failed: {err}", path.display()))?;
    let value: toml::Value = toml::from_str(content.as_str())
        .map_err(|err| format!("parse {} failed: {err}", path.display()))?;
    let node = value
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{NODE_TABLE_KEY} table is missing in {}", path.display()))?;
    let private_key =
        resolve_required_toml_string(node, NODE_PRIVATE_KEY_FIELD, "node.private_key")?;
    let public_key = resolve_required_toml_string(node, NODE_PUBLIC_KEY_FIELD, "node.public_key")?;
    let player_id = resolve_viewer_player_id_override(
        env::var(VIEWER_PLAYER_ID_ENV).ok(),
        default_viewer_player_id,
    );
    Ok(ViewerAuthBootstrap {
        player_id,
        public_key,
        private_key,
    })
}

pub(super) fn resolve_viewer_auth_bootstrap_for_embedded_server(
    deployment_mode: DeploymentMode,
    default_viewer_player_id: Option<&str>,
) -> Option<ViewerAuthBootstrap> {
    if deployment_mode.disables_browser_signer_bootstrap() {
        None
    } else {
        resolve_viewer_auth_bootstrap_from_path(
            Path::new(NODE_CONFIG_FILE_NAME),
            default_viewer_player_id,
        )
        .ok()
    }
}

pub(super) fn parse_model_json(body: &[u8]) -> Result<serde_json::Value, String> {
    identity_security::parse_identity_json(body)
}
