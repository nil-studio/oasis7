use super::*;
use serde::Serialize;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;
use tracing::{Level, info};

#[derive(Debug, Serialize)]
struct PlaneErrorResponse {
    ok: bool,
    error_code: String,
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_state: Option<PublicStateSnapshot>,
}

pub(super) fn run_server(options: CliOptions) -> Result<(), String> {
    install_signal_handler()?;
    TERMINATION_REQUESTED.store(false, Ordering::SeqCst);

    let (listen_host, listen_port) =
        parse_host_port(options.listen_bind.as_str(), "--listen-bind")?;
    let listener = TcpListener::bind((listen_host.as_str(), listen_port)).map_err(|err| {
        format!(
            "failed to bind oasis7_web_launcher at {}:{}: {}",
            listen_host, listen_port, err
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set listener nonblocking: {err}"))?;

    let state = Arc::new(Mutex::new(ServiceState::new(
        options.launcher_bin,
        options.chain_runtime_bin,
        options.console_static_dir,
        options.initial_config,
    )));
    let trace_session_id = oasis7_observability::resolve_trace_session_id("oasis7_web_launcher");

    println!("oasis7_web_launcher started");
    println!(
        "- console: http://{}:{}",
        normalize_bind_host_for_local_access(listen_host.as_str()),
        listen_port
    );
    println!("- listen bind: {listen_host}:{listen_port}");
    println!(
        "- console static dir: {}",
        lock_state(&state).console_static_dir.display()
    );
    println!("Press Ctrl+C to stop.");
    info!(
        trace_session_id = %trace_session_id,
        listen_host = %listen_host,
        listen_port,
        console_static_dir = %lock_state(&state).console_static_dir.display(),
        "web launcher control plane server started"
    );

    loop {
        if TERMINATION_REQUESTED.load(Ordering::SeqCst) {
            break;
        }

        match listener.accept() {
            Ok((stream, peer_addr)) => {
                let shared = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, Some(peer_addr), shared) {
                        let stderr_message =
                            format!("warning: oasis7_web_launcher request failed: {err}");
                        oasis7_observability::emit_stderr_or_event(
                            Level::WARN,
                            stderr_message.as_str(),
                            "web launcher request handling failed",
                        );
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30));
            }
            Err(err) => return Err(format!("accept failed: {err}")),
        }
    }

    let mut state_guard = lock_state(&state);
    poll_service_state(&mut state_guard);
    let _ = stop_process(&mut state_guard);
    let _ = stop_chain_process(&mut state_guard);
    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    peer_addr: Option<SocketAddr>,
    shared_state: Arc<Mutex<ServiceState>>,
) -> Result<(), String> {
    let request = read_http_request(&mut stream)?;
    let path = strip_query(request.path.as_str());
    let request_host = extract_host_header(request.headers.as_slice());
    let hosted_mode = {
        let state = lock_state(&shared_state);
        deployment_mode_from_config(&state.config)
    };

    if let Some(response) = private_plane_rejection(
        hosted_mode,
        path,
        request_host.as_deref(),
        peer_addr,
        &shared_state,
    ) {
        return write_json_response(&mut stream, 403, &response);
    }
    if hosted_mode.requires_loopback_private_control()
        && request.method == "GET"
        && !path.starts_with("/api/")
        && path != "/healthz"
        && !is_loopback_peer(peer_addr)
    {
        return write_http_response(
            &mut stream,
            403,
            "text/plain; charset=utf-8",
            b"Operator Plane Only",
            false,
        );
    }

    match (request.method.as_str(), path) {
        ("GET", "/healthz") => {
            write_http_response(&mut stream, 200, "text/plain; charset=utf-8", b"ok", false)?;
            Ok(())
        }
        ("GET", "/api/public/state") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let snapshot = public_snapshot_from_state(&state, request_host.as_deref());
            write_json_response(&mut stream, 200, &snapshot)
        }
        ("GET", "/api/state") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            write_json_response(&mut stream, 200, &snapshot)
        }
        ("GET", "/api/gui-agent/capabilities") => {
            let response = gui_agent_capabilities_response();
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/gui-agent/state") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            write_json_response(&mut stream, 200, &snapshot)
        }
        ("POST", "/api/gui-agent/action") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let response = execute_gui_agent_action(
                &mut state,
                request.body.as_slice(),
                request_host.as_deref(),
            );
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/ui/schema") => {
            let schema: Vec<_> = launcher_ui_fields_for_web().copied().collect();
            write_json_response(&mut stream, 200, &schema)
        }
        ("POST", "/api/start") => {
            let config = parse_config_request(request.body.as_slice(), "start")?;
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let outcome = start_process(&mut state, config);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            let response = ApiResponse {
                ok: outcome.is_ok(),
                error_code: None,
                error: outcome.err(),
                data: None,
                state: snapshot,
            };
            write_json_response(&mut stream, 200, &response)
        }
        ("POST", "/api/stop") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let outcome = stop_process(&mut state);
            poll_service_state(&mut state);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            let response = ApiResponse {
                ok: outcome.is_ok(),
                error_code: None,
                error: outcome.err(),
                data: None,
                state: snapshot,
            };
            write_json_response(&mut stream, 200, &response)
        }
        ("POST", "/api/chain/start") => {
            let config = parse_config_request(request.body.as_slice(), "chain start")?;
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let outcome = start_chain_process(&mut state, config);
            poll_service_state(&mut state);
            let outcome = finalize_chain_start_outcome(&state, outcome);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            let error = outcome.err();
            let error_code = error
                .as_deref()
                .map(|detail| chain_error_code_for_state(&state, detail).to_string());
            let data = if error.is_some() {
                chain_error_data_for_state(&state)
            } else {
                None
            };
            let response = ApiResponse {
                ok: error.is_none(),
                error_code,
                error,
                data,
                state: snapshot,
            };
            write_json_response(&mut stream, 200, &response)
        }
        ("POST", "/api/chain/stop") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let outcome = stop_chain_process(&mut state);
            poll_service_state(&mut state);
            let snapshot = snapshot_from_state(&state, request_host.as_deref());
            let response = ApiResponse {
                ok: outcome.is_ok(),
                error_code: None,
                error: outcome.err(),
                data: None,
                state: snapshot,
            };
            write_json_response(&mut stream, 200, &response)
        }
        ("POST", "/api/chain/transfer") => {
            let submit_request = match parse_chain_transfer_request(request.body.as_slice()) {
                Ok(request) => request,
                Err(err) => {
                    let response = ChainTransferSubmitResponse::error("invalid_request", err);
                    return write_json_response(&mut stream, 200, &response);
                }
            };
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let response = submit_chain_transfer(&mut state, &submit_request);
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/transfer/accounts") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let response = query_chain_transfer_json(&mut state, "/v1/chain/transfer/accounts");
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/transfer/status") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/transfer/status",
                "/v1/chain/transfer/status",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/transfer/history") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/transfer/history",
                "/v1/chain/transfer/history",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/overview") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let response = query_chain_transfer_json(&mut state, "/v1/chain/explorer/overview");
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/transactions") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/transactions",
                "/v1/chain/explorer/transactions",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/transaction") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/transaction",
                "/v1/chain/explorer/transaction",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/blocks") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/blocks",
                "/v1/chain/explorer/blocks",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/block") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/block",
                "/v1/chain/explorer/block",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/txs") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/txs",
                "/v1/chain/explorer/txs",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/tx") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/tx",
                "/v1/chain/explorer/tx",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/search") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/search",
                "/v1/chain/explorer/search",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/address") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/address",
                "/v1/chain/explorer/address",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/contracts") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/contracts",
                "/v1/chain/explorer/contracts",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/contract") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/contract",
                "/v1/chain/explorer/contract",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/assets") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/assets",
                "/v1/chain/explorer/assets",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("GET", "/api/chain/explorer/mempool") => {
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let runtime_target = remap_transfer_runtime_target(
                request.path.as_str(),
                "/api/chain/explorer/mempool",
                "/v1/chain/explorer/mempool",
            );
            let response = query_chain_transfer_json(&mut state, runtime_target.as_str());
            write_json_response(&mut stream, 200, &response)
        }
        ("POST", "/api/chain/feedback") => {
            let submit_request = match parse_chain_feedback_request(request.body.as_slice()) {
                Ok(request) => request,
                Err(err) => {
                    let response = ChainFeedbackSubmitResponse::error(err);
                    return write_json_response(&mut stream, 200, &response);
                }
            };
            let mut state = lock_state(&shared_state);
            poll_service_state(&mut state);
            let response = submit_chain_feedback(&mut state, &submit_request);
            write_json_response(&mut stream, 200, &response)
        }
        ("OPTIONS", _) => write_http_response(&mut stream, 204, "text/plain", b"", false),
        ("GET", request_path) if !request_path.starts_with("/api/") => {
            serve_console_static_request(&mut stream, request_path, &shared_state)
        }
        (method, "/api/public/state")
        | (method, "/api/state")
        | (method, "/api/gui-agent/capabilities")
        | (method, "/api/gui-agent/state")
        | (method, "/api/gui-agent/action")
        | (method, "/api/start")
        | (method, "/api/stop")
        | (method, "/api/chain/start")
        | (method, "/api/chain/stop")
        | (method, "/api/chain/transfer")
        | (method, "/api/chain/transfer/accounts")
        | (method, "/api/chain/transfer/status")
        | (method, "/api/chain/transfer/history")
        | (method, "/api/chain/explorer/overview")
        | (method, "/api/chain/explorer/transactions")
        | (method, "/api/chain/explorer/transaction")
        | (method, "/api/chain/explorer/blocks")
        | (method, "/api/chain/explorer/block")
        | (method, "/api/chain/explorer/txs")
        | (method, "/api/chain/explorer/tx")
        | (method, "/api/chain/explorer/search")
        | (method, "/api/chain/explorer/address")
        | (method, "/api/chain/explorer/contracts")
        | (method, "/api/chain/explorer/contract")
        | (method, "/api/chain/explorer/assets")
        | (method, "/api/chain/explorer/mempool")
        | (method, "/api/chain/feedback")
            if method != "GET" && method != "POST" =>
        {
            write_http_response(
                &mut stream,
                405,
                "text/plain; charset=utf-8",
                b"Method Not Allowed",
                false,
            )
        }
        _ => write_http_response(
            &mut stream,
            404,
            "text/plain; charset=utf-8",
            b"Not Found",
            false,
        ),
    }
}

fn serve_console_static_request(
    stream: &mut TcpStream,
    request_path: &str,
    shared_state: &Arc<Mutex<ServiceState>>,
) -> Result<(), String> {
    let (console_static_dir, deployment_mode) = {
        let state = lock_state(shared_state);
        (
            state.console_static_dir.clone(),
            deployment_mode_from_config(&state.config),
        )
    };
    let viewer_auth_bootstrap = if deployment_mode.disables_browser_signer_bootstrap() {
        None
    } else {
        resolve_optional_viewer_auth_bootstrap()
    };

    match load_console_static_asset(console_static_dir.as_path(), request_path) {
        StaticAsset::Ok { content_type, body } => {
            let body = inject_viewer_auth_bootstrap_if_html(
                body.as_slice(),
                content_type,
                viewer_auth_bootstrap.as_ref(),
            );
            write_http_response(stream, 200, content_type, body.as_slice(), false)
        }
        StaticAsset::NotFound => write_http_response(
            stream,
            404,
            "text/plain; charset=utf-8",
            b"Not Found",
            false,
        ),
        StaticAsset::InvalidPath => write_http_response(
            stream,
            400,
            "text/plain; charset=utf-8",
            b"Bad Request",
            false,
        ),
    }
}

fn private_plane_rejection(
    deployment_mode: DeploymentMode,
    path: &str,
    request_host: Option<&str>,
    peer_addr: Option<SocketAddr>,
    shared_state: &Arc<Mutex<ServiceState>>,
) -> Option<PlaneErrorResponse> {
    if !deployment_mode.requires_loopback_private_control()
        || !path_requires_private_control_plane(path)
        || is_loopback_peer(peer_addr)
    {
        return None;
    }

    let mut state = lock_state(shared_state);
    poll_service_state(&mut state);
    Some(PlaneErrorResponse {
        ok: false,
        error_code: "operator_plane_only".to_string(),
        error: format!(
            "path `{path}` is only available on the private control plane; use the public join URL for player access"
        ),
        public_state: Some(public_snapshot_from_state(&state, request_host)),
    })
}

fn path_requires_private_control_plane(path: &str) -> bool {
    matches!(
        path,
        "/api/state"
            | "/api/gui-agent/capabilities"
            | "/api/gui-agent/state"
            | "/api/gui-agent/action"
            | "/api/ui/schema"
            | "/api/start"
            | "/api/stop"
            | "/api/chain/start"
            | "/api/chain/stop"
    )
}

fn is_loopback_peer(peer_addr: Option<SocketAddr>) -> bool {
    peer_addr.is_some_and(|addr| addr.ip().is_loopback())
}

fn strip_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

pub(super) fn remap_transfer_runtime_target(
    request_path: &str,
    api_prefix: &str,
    runtime_prefix: &str,
) -> String {
    let suffix = request_path.strip_prefix(api_prefix).unwrap_or_default();
    format!("{runtime_prefix}{suffix}")
}

fn extract_host_header(headers: &[(String, String)]) -> Option<String> {
    headers
        .iter()
        .find(|(name, _)| name == "host")
        .map(|(_, value)| normalize_host_header(value.as_str()))
        .filter(|value| !value.is_empty())
}

fn normalize_host_header(raw: &str) -> String {
    let value = raw.trim();
    if value.starts_with('[')
        && let Some((host, _)) = value.rsplit_once(']')
    {
        return host.trim_start_matches('[').to_string();
    }
    if let Some((host, _port)) = value.rsplit_once(':') {
        if host.contains(':') {
            return value.to_string();
        }
        return host.to_string();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::super::{
        LauncherConfig, ServiceState, deployment_mode_from_config,
        hosted_access::hosted_strong_auth_test_env_lock, public_snapshot_from_state,
    };
    use super::{is_loopback_peer, path_requires_private_control_plane, private_plane_rejection};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn clear_hosted_strong_auth_env() {
        for name in [
            "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY",
            "OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY",
            "OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE",
        ] {
            // SAFETY: This test/setup code mutates process environment in a controlled scope.
            unsafe {
                std::env::remove_var(name);
            }
        }
    }

    #[test]
    fn hosted_mode_requires_private_control_plane_for_operator_routes() {
        assert!(path_requires_private_control_plane("/api/state"));
        assert!(path_requires_private_control_plane("/api/gui-agent/action"));
        assert!(!path_requires_private_control_plane("/api/public/state"));
        assert!(!path_requires_private_control_plane(
            "/api/chain/explorer/overview"
        ));
    }

    #[test]
    fn loopback_detection_matches_localhost_only() {
        assert!(is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            5410,
        ))));
        assert!(!is_loopback_peer(Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(39, 104, 204, 172)),
            5410,
        ))));
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn hosted_mode_rejects_remote_private_state_request() {
        let mut config = LauncherConfig::default();
        config.deployment_mode = "hosted_public_join".to_string();
        let state = Arc::new(Mutex::new(ServiceState::new(
            "launcher".to_string(),
            "chain".to_string(),
            PathBuf::from("."),
            config,
        )));
        let deployment_mode = {
            let guard = state.lock().expect("lock");
            deployment_mode_from_config(&guard.config)
        };

        let response = private_plane_rejection(
            deployment_mode,
            "/api/state",
            Some("39.104.204.172"),
            Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(39, 104, 204, 172)),
                5410,
            )),
            &state,
        )
        .expect("should reject");
        assert_eq!(response.error_code, "operator_plane_only");
        assert!(response.public_state.is_some());
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn hosted_mode_rejects_remote_private_control_plane_matrix() {
        let _guard = hosted_strong_auth_test_env_lock().lock().expect("env lock");
        clear_hosted_strong_auth_env();

        let private_paths = [
            "/api/state",
            "/api/gui-agent/capabilities",
            "/api/gui-agent/state",
            "/api/gui-agent/action",
            "/api/ui/schema",
            "/api/start",
            "/api/stop",
            "/api/chain/start",
            "/api/chain/stop",
        ];

        let mut config = LauncherConfig::default();
        config.deployment_mode = "hosted_public_join".to_string();
        let state = Arc::new(Mutex::new(ServiceState::new(
            "launcher".to_string(),
            "chain".to_string(),
            PathBuf::from("."),
            config,
        )));
        let deployment_mode = {
            let guard = state.lock().expect("lock");
            deployment_mode_from_config(&guard.config)
        };
        let remote_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(39, 104, 204, 172)), 5410);

        for path in private_paths {
            let response = private_plane_rejection(
                deployment_mode,
                path,
                Some("play.example.com"),
                Some(remote_peer),
                &state,
            )
            .unwrap_or_else(|| panic!("path should reject on remote public origin: {path}"));
            assert_eq!(response.error_code, "operator_plane_only");
            assert!(
                response.error.contains(path),
                "error should mention rejected path: {path}"
            );
            let public_state = response
                .public_state
                .as_ref()
                .unwrap_or_else(|| panic!("path should expose public snapshot: {path}"));
            assert_eq!(
                public_state.hosted_access.verdict,
                "hosted_public_join_blocked_until_strong_auth"
            );
            assert!(
                public_state.game_url.contains("play.example.com"),
                "public snapshot should stay on public host for path: {path}"
            );
        }
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn public_snapshot_exposes_hosted_access_contract() {
        let _guard = hosted_strong_auth_test_env_lock().lock().expect("env lock");
        clear_hosted_strong_auth_env();

        let mut config = LauncherConfig::default();
        config.deployment_mode = "hosted_public_join".to_string();
        let state = ServiceState::new(
            "launcher".to_string(),
            "chain".to_string(),
            PathBuf::from("."),
            config,
        );
        let snapshot = public_snapshot_from_state(&state, Some("127.0.0.1"));
        assert_eq!(snapshot.hosted_access.deployment_mode, "hosted_public_join");
        assert_eq!(
            snapshot.hosted_access.verdict,
            "hosted_public_join_blocked_until_strong_auth"
        );
        assert_eq!(
            snapshot.hosted_access.local_chain_runtime,
            "blocked_for_public_player_plane"
        );
        assert_eq!(
            snapshot.hosted_access.node_admission,
            "operator_managed_node_onboarding_only"
        );
        assert!(snapshot.hosted_access.action_matrix.iter().any(|policy| {
            policy.action_id == "main_token_transfer"
                && policy.required_auth == "strong_auth"
                && policy.availability == "blocked_until_strong_auth"
        }));
        assert!(snapshot.hosted_access.action_matrix.iter().any(|policy| {
            policy.action_id == "agent_chat"
                && policy.required_auth == "player_session"
                && policy.availability == "public_player_plane"
        }));
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn public_snapshot_keeps_asset_lane_blocked_when_prompt_reauth_env_is_ready() {
        let _guard = hosted_strong_auth_test_env_lock().lock().expect("env lock");
        clear_hosted_strong_auth_env();
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var(
                "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            );
        }
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var(
                "OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            );
        }
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            std::env::set_var("OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE", "preview-code");
        }

        let mut config = LauncherConfig::default();
        config.deployment_mode = "hosted_public_join".to_string();
        let state = ServiceState::new(
            "launcher".to_string(),
            "chain".to_string(),
            PathBuf::from("."),
            config,
        );
        let snapshot = public_snapshot_from_state(&state, Some("127.0.0.1"));
        assert!(snapshot.hosted_access.action_matrix.iter().any(|policy| {
            policy.action_id == "prompt_control_apply"
                && policy.required_auth == "strong_auth"
                && policy.availability == "public_player_plane_with_backend_reauth_preview"
        }));
        assert!(snapshot.hosted_access.action_matrix.iter().any(|policy| {
            policy.action_id == "main_token_transfer"
                && policy.required_auth == "strong_auth"
                && policy.availability == "blocked_until_strong_auth"
        }));
        clear_hosted_strong_auth_env();
    }
}
