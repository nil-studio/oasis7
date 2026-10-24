//! Real producer status passes through the shipping file report entrypoint.
use serde_json::Value;
use std::{path::Path, process::Command};

pub(super) fn assert_report(root: &Path, status: &Value, expected: &str) -> Result<(), String> {
    let input = root.join("profile-report-input.json");
    let output = root.join("profile-report.json");
    let markdown = root.join("profile-report.md");
    std::fs::write(&input, serde_json::to_vec(status).map_err(|e| e.to_string())?)
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
        return Err(format!("profile report failed: {}", String::from_utf8_lossy(&result.stderr)));
    }
    let summary: Value = serde_json::from_slice(&std::fs::read(&output).map_err(|e| e.to_string())?)
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
    if producer[key].is_object()
        && profile[key]["position"] != producer[key]["position"]
    {
        return Err("profile report lost the retained verified position".into());
    }
    if producer["grant_epoch"].is_number()
        && profile["grant_epoch"] != producer["grant_epoch"]
    {
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
