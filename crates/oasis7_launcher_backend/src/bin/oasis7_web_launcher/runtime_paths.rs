use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GAME_LAUNCHER_BIN_ENV: &str = "OASIS7_GAME_LAUNCHER_BIN";
const OASIS7_CHAIN_RUNTIME_BIN_ENV: &str = "OASIS7_CHAIN_RUNTIME_BIN";
const GAME_STATIC_DIR_ENV: &str = "OASIS7_GAME_STATIC_DIR";

pub(super) fn resolve_oasis7_game_launcher_binary() -> PathBuf {
    if let Some(path) = resolve_non_empty_override_value(env::var(GAME_LAUNCHER_BIN_ENV).ok()) {
        return PathBuf::from(path);
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(bin_dir) = current_exe.parent()
    {
        return bin_dir.join(binary_name("oasis7_game_launcher"));
    }

    PathBuf::from(binary_name("oasis7_game_launcher"))
}

pub(super) fn resolve_oasis7_chain_runtime_binary() -> PathBuf {
    if let Some(path) =
        resolve_non_empty_override_value(env::var(OASIS7_CHAIN_RUNTIME_BIN_ENV).ok())
    {
        return PathBuf::from(path);
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(bin_dir) = current_exe.parent()
    {
        return bin_dir.join(binary_name("oasis7_chain_runtime"));
    }

    PathBuf::from(binary_name("oasis7_chain_runtime"))
}

pub(super) fn resolve_static_dir_path(default_viewer_static_dir: &str) -> PathBuf {
    if let Some(path) = resolve_non_empty_override_value(env::var(GAME_STATIC_DIR_ENV).ok()) {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::new();
    if let Ok(current_exe) = env::current_exe()
        && let Some(bin_dir) = current_exe.parent()
    {
        candidates.push(bin_dir.join("..").join("web"));
        candidates.push(bin_dir.join("..").join("..").join("web"));
    }
    candidates.extend(viewer_dev_dist_candidates());
    candidates.push(PathBuf::from(default_viewer_static_dir));

    first_existing_dir(candidates).unwrap_or_else(|| PathBuf::from(default_viewer_static_dir))
}

pub(super) fn viewer_dev_dist_candidates() -> Vec<PathBuf> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    vec![repo_root.join("oasis7_viewer").join("dist")]
}

pub(super) fn repo_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

pub(super) fn normalize_bind_host_for_local_access(host: &str) -> String {
    let host = host.trim();
    if host == "0.0.0.0" || host == "::" || host == "[::]" {
        "127.0.0.1".to_string()
    } else {
        host.to_string()
    }
}

pub(super) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn binary_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn first_existing_dir(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| path.is_dir())
}

fn resolve_non_empty_override_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{first_existing_dir, resolve_non_empty_override_value, viewer_dev_dist_candidates};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn first_existing_dir_returns_first_existing_candidate() {
        let missing = make_temp_path("missing");
        let fallback = make_temp_path("fallback");
        fs::create_dir_all(&fallback).expect("create fallback dir");

        let resolved = first_existing_dir(vec![missing, fallback.clone()]);
        assert_eq!(resolved, Some(fallback.clone()));

        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn first_existing_dir_returns_none_when_all_candidates_missing() {
        let first = make_temp_path("first_missing");
        let second = make_temp_path("second_missing");
        let resolved = first_existing_dir(vec![first, second]);
        assert!(resolved.is_none());
    }

    #[test]
    fn resolve_non_empty_override_value_returns_trimmed_current_value() {
        let resolved = resolve_non_empty_override_value(Some(" primary ".to_string()));
        assert_eq!(resolved.as_deref(), Some("primary"));
    }

    #[test]
    fn resolve_non_empty_override_value_rejects_blank_value() {
        let resolved = resolve_non_empty_override_value(Some("  ".to_string()));
        assert!(resolved.is_none());
    }

    #[test]
    fn viewer_dev_dist_candidates_only_return_oasis7_path() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let candidates = viewer_dev_dist_candidates();

        assert_eq!(
            candidates,
            vec![repo_root.join("oasis7_viewer").join("dist")]
        );
    }

    fn make_temp_path(label: &str) -> PathBuf {
        let mut path = env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        path.push(format!(
            "oasis7_provider_agent_paths_{label}_{}_{}",
            std::process::id(),
            stamp
        ));
        path
    }
}
