//! Real producer status passes through the shipping file report entrypoint.
use serde_json::Value;
use std::{path::Path, process::Command};

pub(super) fn assert_report(root: &Path, status: &Value, expected: &str) -> Result<(), String> {
    let input = root.join("profile-report-input.json");
    let output = root.join("profile-report.json");
    let markdown = root.join("profile-report.md");
    std::fs::write(
        &input,
        serde_json::to_vec(status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new("bash")
        .arg(repo.join("scripts/oasis7-node-observability-report.sh"))
        .args(["--status-json-path"])
        .arg(&input)
        .arg("--out-dir")
        .arg(root.join("profile-report-output"))
        .arg("--summary-json")
        .arg(&output)
        .arg("--summary-md")
        .arg(&markdown)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .map_err(|e| format!("profile report spawn: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "profile report failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let summary: Value =
        serde_json::from_slice(&std::fs::read(&output).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let profile = &summary["profile_readiness"];
    if profile["status"] != expected
        || profile["runtime_profile"] != status["runtime_profile"]
        || profile["can_propose"] != false
    {
        return Err(format!("profile report expected {expected}, got {profile}"));
    }
    let (producer, key) = if status["runtime_profile"] == "formal_public_observer_v1" {
        (&status["formal_public_observer"], "last_verified_commit")
    } else {
        (&status["formal_full_state_sync"], "last_verified_cut")
    };
    if producer[key].is_object() && profile[key]["position"] != producer[key]["position"] {
        return Err("profile report lost the retained verified position".into());
    }
    if producer["grant_epoch"].is_number() && profile["grant_epoch"] != producer["grant_epoch"] {
        return Err("profile report lost the installed grant epoch".into());
    }
    let rendered = std::fs::read_to_string(markdown).map_err(|e| e.to_string())?;
    if !rendered.contains("Profile Readiness") || !rendered.contains(expected) {
        return Err("profile report Markdown omitted producer readiness".into());
    }
    if summary.get("chain").is_some() || summary.get("paths").is_some() {
        return Err("profile report copied private CLI configuration".into());
    }
    Ok(())
}

/// Exercise the explicit local collector against a shipping node binary. The
/// expected producer value stays in memory; the collector receives only its
/// executable and existing data directory, and never starts or stops the node.
pub(super) fn assert_local_collection(
    root: &Path,
    binary: &Path,
    data: &Path,
    status: &Value,
    expected: &str,
) -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join("local-profile-collection");
    let read = |format: &str| {
        Command::new("python3")
            .arg(repo.join("scripts/p2p-real-env-observability-attempt.py"))
            .arg("read")
            .args([
                "--collector",
                "node-profile",
                "--format",
                format,
                "--out-dir",
            ])
            .arg(&output)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .map_err(|e| format!("local profile canonical read spawn: {e}"))
    };
    // A successful status probe may observe an intermediate sync tick after
    // wait_ready returned. Retry only the observed role state, never a failed
    // command, malformed output, or invalid canonical publication.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let summary = loop {
        let collected = Command::new("bash")
            .arg(repo.join("scripts/p2p-node-profile-monitor.sh"))
            .arg("--node-binary")
            .arg(binary)
            .arg("--data-dir")
            .arg(data)
            .arg("--out-dir")
            .arg(&output)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .map_err(|e| format!("local profile collector spawn: {e}"))?;
        if !collected.status.success() {
            return Err(format!(
                "local profile collection failed: {}",
                String::from_utf8_lossy(&collected.stderr)
            ));
        }
        let canonical = read("json")?;
        if !canonical.status.success() {
            return Err("local profile canonical JSON read failed".into());
        }
        let summary: Value = serde_json::from_slice(&canonical.stdout)
            .map_err(|e| format!("local profile canonical JSON: {e}"))?;
        let profile = &summary["profile_readiness"];
        if profile["source"] != "local_cli"
            || profile["runtime_profile"] != status["runtime_profile"]
            || profile["can_propose"] != false
            || summary["collection"]["complete"] != true
            || summary["collection"]["result"] != "success"
            || summary["current_measurement_available"] != true
            || summary["canonical_state"]["validated"] != true
        {
            return Err(format!(
                "local profile publication contract failed: {profile}"
            ));
        }
        if profile["status"] == expected {
            break summary;
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("local profile expected {expected}, got {profile}"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let profile = &summary["profile_readiness"];
    let producer = &status["formal_full_state_sync"];
    if producer["last_verified_cut"].is_object()
        && profile["last_verified_cut"]["position"] != producer["last_verified_cut"]["position"]
    {
        return Err("local profile lost the retained verified cut".into());
    }
    if producer["grant_epoch"].is_number() && profile["grant_epoch"] != producer["grant_epoch"] {
        return Err("local profile lost the installed grant epoch".into());
    }
    if summary.get("chain").is_some() || summary.get("paths").is_some() {
        return Err("local profile copied private CLI configuration".into());
    }
    let markdown = read("markdown")?;
    let rendered = String::from_utf8(markdown.stdout).map_err(|e| e.to_string())?;
    if !markdown.status.success()
        || !rendered.contains("Profile Readiness")
        || !rendered.contains("local_cli")
        || !rendered.contains(expected)
    {
        return Err("local profile canonical Markdown omitted readiness".into());
    }
    Ok(())
}
