use super::*;
use oasis7_client_api::bootstrap::FORMAL_RELEASE_DEFAULT_WORLD_ID as VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID;
use tracing::Level;

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(hex_upper(byte >> 4));
            encoded.push(hex_upper(byte & 0x0f));
        }
    }
    encoded
}

pub(super) fn encoded_query_pair(key: &str, value: &str) -> String {
    format!("{key}={}", encode_query_value(value))
}

fn hex_upper(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + (nibble - 10)) as char,
        _ => '0',
    }
}

pub(super) fn resolve_runtime_host(config_host: &str, request_host: Option<&str>) -> String {
    let config_host = config_host.trim();
    if config_host.is_empty()
        || config_host == "0.0.0.0"
        || config_host == "::"
        || config_host == "[::]"
        || config_host == "127.0.0.1"
        || config_host == "localhost"
    {
        if let Some(request_host) = request_host {
            let request_host = request_host.trim();
            if !request_host.is_empty() {
                return request_host.to_string();
            }
        }
        return "127.0.0.1".to_string();
    }
    config_host.to_string()
}

pub(super) fn host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn resolve_viewer_static_dir_candidate_for_launcher(
    raw: &str,
    launcher_bin: &str,
) -> Option<std::path::PathBuf> {
    let user_path = std::path::PathBuf::from(raw);
    if user_path.is_dir() {
        return Some(user_path);
    }

    if user_path.is_relative() {
        let launcher_bin = launcher_bin.trim();
        if !launcher_bin.is_empty()
            && let Some(bin_dir) = Path::new(launcher_bin).parent()
        {
            let sibling_candidate = bin_dir.join("..").join(&user_path);
            if sibling_candidate.is_dir() {
                return Some(sibling_candidate);
            }
        }
    }

    None
}

pub(super) fn resolve_viewer_static_dir_for_launcher(
    raw: &str,
    launcher_bin: &str,
) -> Option<std::path::PathBuf> {
    if raw == DEFAULT_VIEWER_STATIC_DIR
        && let Some(override_path) =
            resolve_viewer_static_env_override(std::env::var(GAME_STATIC_DIR_ENV).ok())
    {
        return resolve_viewer_static_dir_candidate_for_launcher(
            override_path.as_str(),
            launcher_bin,
        );
    }

    if let Some(dir) = resolve_viewer_static_dir_candidate_for_launcher(raw, launcher_bin) {
        return Some(dir);
    }

    if raw == DEFAULT_VIEWER_STATIC_DIR
        && let Some(dev_fallback) = viewer_dev_dist_candidates()
            .into_iter()
            .find(|candidate| candidate.is_dir())
    {
        return Some(dev_fallback);
    }

    None
}

pub(super) fn resolve_viewer_static_env_override(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(super) fn chain_execution_world_dir(node_id: &str) -> String {
    Path::new("output")
        .join("chain-runtime")
        .join(node_id)
        .join("reward-runtime-execution-world")
        .to_string_lossy()
        .into_owned()
}

pub(super) fn resolve_chain_world_id(config: &LauncherConfig) -> String {
    if config.chain_world_id.trim().is_empty() {
        let scenario = config.scenario.trim();
        if scenario.is_empty() {
            VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID.to_string()
        } else {
            format!("live-{scenario}")
        }
    } else {
        config.chain_world_id.trim().to_string()
    }
}

pub(super) fn resolve_launcher_bin_from_config(
    config: &LauncherConfig,
    default_bin: &str,
) -> String {
    let value = config.launcher_bin.trim();
    if value.is_empty() {
        default_bin.to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn resolve_chain_runtime_bin_from_config(
    config: &LauncherConfig,
    default_bin: &str,
) -> String {
    let value = config.chain_runtime_bin.trim();
    if value.is_empty() {
        default_bin.to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn spawn_child_process(
    bin: &str,
    args: &[String],
    process_label: &'static str,
) -> Result<RunningProcess, String> {
    let mut child = Command::new(bin)
        .args(args)
        .env(
            oasis7_observability::TRACE_SESSION_ID_ENV,
            oasis7_observability::resolve_trace_session_id("oasis7_web_launcher"),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("spawn process `{bin}` failed: {err}"))?;

    let (log_tx, log_rx) = mpsc::channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, process_label, "stdout", log_tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, process_label, "stderr", log_tx.clone());
    }

    Ok(RunningProcess { child, log_rx })
}

fn spawn_log_reader<R: Read + Send + 'static>(
    reader: R,
    process_label: &'static str,
    source: &'static str,
    tx: Sender<String>,
) {
    thread::spawn(move || {
        let buffered = BufReader::new(reader);
        for line in buffered.lines() {
            match line {
                Ok(content) => {
                    let _ = tx.send(format!("[{process_label} {source}] {content}"));
                }
                Err(err) => {
                    let _ = tx.send(format!("[{process_label} {source}] <read error: {err}>"));
                    break;
                }
            }
        }
    });
}

pub(super) fn stop_child_process(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|err| format!("query child status failed: {err}"))?
        .is_some()
    {
        return Ok(());
    }

    if let Err(err) = send_interrupt_signal(child) {
        oasis7_observability::emit_stderr_or_event(
            Level::WARN,
            format!("warning: failed to request graceful process stop: {err}").as_str(),
            "web launcher graceful child stop request failed",
        );
    } else {
        let deadline = Instant::now() + Duration::from_millis(GRACEFUL_STOP_TIMEOUT_MS);
        while Instant::now() < deadline {
            if child
                .try_wait()
                .map_err(|err| format!("query child status failed: {err}"))?
                .is_some()
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(STOP_POLL_INTERVAL_MS));
        }
    }

    if let Ok(None) = child.try_wait() {
        child
            .kill()
            .map_err(|err| format!("kill child failed: {err}"))?;
    }
    child
        .wait()
        .map_err(|err| format!("wait child failed: {err}"))?;
    Ok(())
}

fn send_interrupt_signal(child: &Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        let pid = child.id().to_string();
        let status = Command::new("kill")
            .arg("-INT")
            .arg(pid.as_str())
            .status()
            .map_err(|err| format!("run kill -INT failed: {err}"))?;
        if status.success() {
            return Ok(());
        }
        Err(format!("kill -INT exited with {status}"))
    }

    #[cfg(not(unix))]
    {
        let _ = child;
        Ok(())
    }
}
