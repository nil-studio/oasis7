//! Optional operating-domain records for local and managed runtime lifecycle.
//! Formal authority changes require exact externally verified protocol evidence.
#[cfg(unix)]
#[path = "node_operations/executor.rs"]
mod executor;
#[cfg(unix)]
#[path = "node_operations/healthy_durable_set.rs"]
mod healthy_durable_set;
#[cfg(unix)]
#[path = "node_operations/model.rs"]
mod model;
#[cfg(unix)]
#[path = "node_operations/planner.rs"]
mod planner;
#[cfg(unix)]
#[path = "node_operations/protected_handoff.rs"]
mod protected_handoff;
#[cfg(unix)]
#[path = "node_operations/provider.rs"]
mod provider;
#[cfg(unix)]
#[path = "node_operations/recover.rs"]
mod recover;
#[cfg(unix)]
#[path = "node_operations/result.rs"]
mod result;
#[cfg(unix)]
#[path = "node_operations/store.rs"]
mod store;
#[cfg(all(test, unix))]
#[path = "node_operations/tests.rs"]
mod tests;

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let args: Vec<String> = args.map(str::to_owned).collect();
    #[cfg(not(unix))]
    {
        let _ = args;
        return Err("native local operations unsupported on this platform".into());
    }
    #[cfg(unix)]
    {
        run_unix(args)
    }
}
#[cfg(unix)]
fn run_unix(args: Vec<String>) -> Result<(), String> {
    use super::node_lifecycle::local_io as io;
    use std::{collections::BTreeMap, path::PathBuf};
    let tail = args.as_slice();
    if tail.is_empty()
        || tail == ["--help"]
        || (tail.len() == 2
            && tail[1] == "--help"
            && ["plan", "submit", "recover", "status", "resume", "cancel"]
                .contains(&tail[0].as_str()))
    {
        println!(
            "op plan|submit|recover --request FILE [--store DIR]\nop status --store DIR --id ID\nop resume|cancel --store DIR --id ID --revision N\nop agent (fixed SSH/systemd helper; JSON on stdin)\nProtected signer handoff requires exact owner identities, externally verified profile-2 recovery evidence, and independent primary/replica readback. Healthy remove_critical_replica uses HealthyDurableSet authorization and waits for externally provisioned candidate management endpoints; cancel is permitted only before arm."
        );
        return Ok(());
    }
    let command = &tail[0];
    if command == "agent" {
        if tail.len() != 1 {
            return Err("op agent accepts no command-line arguments".into());
        }
        return provider::run_agent_stdin();
    }
    if command == "formal-owner-worker" {
        let mut values = BTreeMap::new();
        let mut index = 1;
        while index < tail.len() {
            let key = &tail[index];
            if ![
                "--config",
                "--operation-id",
                "--request-digest",
                "--config-digest",
                "--expected-formal-read-trust-digest",
            ]
            .contains(&key.as_str())
            {
                return Err(format!("unsupported formal owner option {key}"));
            }
            let value = tail
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .ok_or("formal owner option requires a value")?;
            if values.insert(key.as_str(), value.as_str()).is_some() {
                return Err("duplicate formal owner option".into());
            }
            index += 2;
        }
        let value = |key| {
            values
                .get(key)
                .copied()
                .ok_or_else(|| format!("formal owner requires {key}"))
        };
        return super::formal_live_runtime::run_operation_owner_worker_with_fixed_trust(
            std::path::Path::new(value("--config")?),
            value("--operation-id")?,
            value("--request-digest")?,
            value("--config-digest")?,
            values.get("--expected-formal-read-trust-digest").copied(),
        );
    }
    if command == "formal-runtime-status" {
        if tail.len() != 3 || tail[1] != "--config" {
            return Err("formal-runtime-status requires --config FILE".into());
        }
        let snapshot =
            super::formal_live_runtime::current_operation_owner(std::path::Path::new(&tail[2]))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let mut options = BTreeMap::new();
    let mut i = 1;
    while i < tail.len() {
        let key = &tail[i];
        if !["--request", "--store", "--id", "--revision"].contains(&key.as_str()) {
            return Err(format!("unsupported op option {key}"));
        }
        let value = tail
            .get(i + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or("op option requires value")?;
        if options.insert(key.as_str(), value.as_str()).is_some() {
            return Err("duplicate op option".into());
        }
        i += 2;
    }
    let value = |key| {
        options
            .get(key)
            .copied()
            .ok_or_else(|| format!("required op option {key}"))
    };
    let output = match command.as_str() {
        "plan" | "submit" | "recover" => {
            if options.contains_key("--id") || options.contains_key("--revision") {
                return Err("inapplicable op option".into());
            }
            let request: model::Request = io::read(&PathBuf::from(value("--request")?))?;
            if command == "plan" {
                if options.contains_key("--store") {
                    return Err("plan has no store effects".into());
                }
                serde_json::to_value(planner::plan(request)?)
            } else {
                let mut store = store::Store::open(&PathBuf::from(value("--store")?))?;
                // Retry reads the original record before reevaluating mutable facts.
                let record = if let Some(old) = store.existing(&request.operation_id) {
                    if old.request_digest != io::digest(&request)? {
                        return Err("operation identity reused with different request".into());
                    }
                    old
                } else {
                    let record = if command == "recover" {
                        recover::reconstruct(request)?
                    } else {
                        planner::plan(request)?
                    };
                    store.admit(record)?
                };
                serde_json::to_value(record)
            }
        }
        "status" | "resume" | "cancel" => {
            if options.contains_key("--request") {
                return Err("inapplicable request option".into());
            }
            let store_path = PathBuf::from(value("--store")?);
            let id = value("--id")?;
            let record = if command == "status" {
                if options.contains_key("--revision") {
                    return Err("status requires no CAS revision".into());
                }
                store::Store::snapshot(&store_path, id)?
            } else {
                let mut store = store::Store::open(&store_path)?;
                let revision = value("--revision")?
                    .parse::<u64>()
                    .map_err(|_| "invalid CAS revision")?;
                if command == "resume" {
                    executor::resume(&mut store, id, revision)?
                } else {
                    executor::cancel(&mut store, id, revision)?
                }
            };
            serde_json::to_value(record)
        }
        _ => {
            return Err(
                "unsupported operation; authority/validator/recovery not implemented".into(),
            );
        }
    }
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
    );
    Ok(())
}
