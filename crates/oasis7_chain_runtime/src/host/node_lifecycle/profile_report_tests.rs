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
) -> Result<Value, String> {
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
    // This collector takes a new sample; an earlier ready status is no lease.
    // Accept only the exact full-sync in-flight projection below, retaining its
    // actual unknown result. All other mismatches keep the existing deadline;
    // command/schema/canonical-publication failures still fail immediately.
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
        if profile["status"] == expected
            || (expected == "ready" && same_full_sync_inflight(profile, status))
        {
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
        || !rendered.contains(profile["status"].as_str().ok_or("profile status missing")?)
    {
        return Err("local profile canonical Markdown omitted readiness".into());
    }
    Ok(profile.clone())
}

// The public projection omits the cut's World object. Its world/chain and the
// exact config/instance binding accompany every allowlisted cut identity field;
// the controlled process case additionally compares the complete raw cut.
fn same_full_sync_inflight(profile: &Value, prior: &Value) -> bool {
    let sync = &prior["formal_full_state_sync"];
    let cut = &sync["last_verified_cut"];
    let identity = &prior["instance"];
    profile["runtime_profile"] == "formal_full_state_sync_read_only_v1"
        && profile["status"] == "unknown"
        && profile["reason"] == "missing_sync_cut"
        && profile["producer_state"] == "syncing"
        && profile["instance_state"] == "running"
        && profile["observation_freshness"] == "fresh"
        && profile["configured_role"] == "full_state_sync"
        && profile["scope"] == "issuer_granted_full_state_read_only"
        && profile["incarnation"] == identity["incarnation"]
        && profile["generation"] == identity["generation"]
        && profile["config_digest"] == identity["config_digest"]
        && profile["world_id"] == cut["world"]["world_id"]
        && profile["chain_id"] == cut["chain_id"]
        && profile["grant_epoch"] == sync["grant_epoch"]
        && profile["producer_ready"] == false
        && profile["formal_readiness"] == false
        && profile["can_propose"] == false
        && profile["state_root_verified"] == false
        && profile["world_loaded"] == false
        && profile["catch_up_pending"] == false
        && profile["lag_positions"].is_null()
        && profile["loaded_cut"].is_null()
        && cut.is_object()
        && [
            "position",
            "authority_epoch",
            "record_id",
            "after_state_root",
            "active_authority_id",
            "finalizing_authority_id",
        ]
        .iter()
        .all(|key| !cut[key].is_null() && profile["last_verified_cut"][key] == cut[key])
}

#[test]
fn full_sync_sampling_accepts_only_the_bound_unloaded_inflight_projection() {
    use serde_json::json;
    let cut = json!({"world":{"world_id":"world","genesis_digest":"genesis"},
        "chain_id":"chain","position":2,"authority_epoch":1,"record_id":"record",
        "after_state_root":"root","active_authority_id":"authority","finalizing_authority_id":"authority"});
    let identity = json!({"incarnation":"instance","generation":2,"config_digest":"config"});
    let prior = json!({"instance":identity,"formal_full_state_sync":{"last_verified_cut":cut,"grant_epoch":1}});
    let profile = json!({"runtime_profile":"formal_full_state_sync_read_only_v1",
        "status":"unknown","reason":"missing_sync_cut","producer_state":"syncing",
        "instance_state":"running","observation_freshness":"fresh","configured_role":"full_state_sync",
        "scope":"issuer_granted_full_state_read_only","incarnation":"instance","generation":2,
        "config_digest":"config","world_id":"world","chain_id":"chain","grant_epoch":1,
        "producer_ready":false,"formal_readiness":false,"can_propose":false,"state_root_verified":false,
        "world_loaded":false,"catch_up_pending":false,"lag_positions":null,
        "last_verified_cut":{"position":2,"authority_epoch":1,"record_id":"record","after_state_root":"root",
        "active_authority_id":"authority","finalizing_authority_id":"authority"}});
    assert!(same_full_sync_inflight(&profile, &prior));
    for (field, wrong) in [
        ("reason", json!("producer_blocked")),
        ("producer_state", json!("sync_unavailable")),
        ("status", json!("blocked")),
        ("observation_freshness", json!("stale")),
        ("incarnation", json!("replacement")),
        ("generation", json!(3)),
        ("config_digest", json!("other")),
        ("world_id", json!("other")),
        ("chain_id", json!("other")),
        ("grant_epoch", json!(2)),
        ("producer_ready", json!(true)),
        ("formal_readiness", json!(true)),
        ("can_propose", json!(true)),
        ("state_root_verified", json!(true)),
        ("world_loaded", json!(true)),
        ("loaded_cut", json!({"position":2})),
        ("catch_up_pending", json!(true)),
        ("lag_positions", json!(0)),
    ] {
        let mut wrong_profile = profile.clone();
        wrong_profile[field] = wrong;
        assert!(
            !same_full_sync_inflight(&wrong_profile, &prior),
            "must reject changed {field}"
        );
    }
    for field in [
        "position",
        "authority_epoch",
        "record_id",
        "after_state_root",
        "active_authority_id",
        "finalizing_authority_id",
    ] {
        let mut wrong_profile = profile.clone();
        wrong_profile["last_verified_cut"][field] = json!("different");
        assert!(
            !same_full_sync_inflight(&wrong_profile, &prior),
            "must reject changed cut {field}"
        );
    }
}
