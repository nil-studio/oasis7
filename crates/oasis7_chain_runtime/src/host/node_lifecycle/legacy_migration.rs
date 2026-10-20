//! Explicit preserving migration of stopped legacy observer launch material.
//! This is not NodeConfig initialization, authority activation or signer transfer.
#[path = "legacy_migration/source.rs"]
mod source;
#[cfg(test)]
#[path = "legacy_migration/tests.rs"]
mod tests;
#[path = "legacy_migration/transaction.rs"]
mod transaction;
#[path = "legacy_migration/tree.rs"]
mod tree;
#[path = "legacy_migration/verify.rs"]
mod verify;
use super::local_io as io;
pub(crate) use source::load_launch;
use std::{collections::BTreeMap, path::Path};

pub(crate) fn run_legacy<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let first = args.next();
    if first.is_none() || matches!(first, Some("--help" | "help" | "-h")) {
        println!("Usage: oasis7_chain_runtime legacy-run --config <legacy-launch.json>");
        return Ok(());
    }
    if first != Some("--config") {
        return Err("Usage: oasis7_chain_runtime legacy-run --config <legacy-launch.json>".into());
    }
    let path = args
        .next()
        .filter(|p| !p.starts_with("--"))
        .ok_or("--config requires migrated aggregate launch")?;
    if args.next().is_some() {
        return Err("legacy-run accepts only its immutable aggregate config".into());
    }
    let options = load_launch(Path::new(path))?;
    crate::run_chain_runtime(options)
}

pub(crate) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut args = args;
    let Some(command) = args.next() else {
        println!("{HELP}");
        return Ok(());
    };
    if matches!(command, "--help" | "help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if !matches!(command, "plan" | "apply" | "resume" | "status") {
        return Err(HELP.into());
    }
    let mut flags = BTreeMap::new();
    while let Some(flag) = args.next() {
        if !matches!(flag, "--source" | "--destination" | "--operation-id")
            || flags.contains_key(flag)
        {
            return Err(format!("unsupported or repeated migration option {flag}"));
        }
        let value = args
            .next()
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("{flag} requires a value"))?;
        flags.insert(flag, value);
    }
    let source = Path::new(*flags.get("--source").ok_or("--source required")?);
    let destination = Path::new(*flags.get("--destination").ok_or("--destination required")?);
    let id = *flags
        .get("--operation-id")
        .ok_or("--operation-id required")?;
    let output = match command {
        "plan" => serde_json::to_value(source::plan(source, destination, id)?),
        "apply" | "resume" => serde_json::to_value(transaction::apply(source, destination, id)?),
        "status" => serde_json::to_value(transaction::status(source, destination, id)?),
        _ => unreachable!(),
    }
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
    );
    Ok(())
}
const HELP: &str = "Usage: oasis7_chain_runtime legacy-migrate <plan|apply|resume|status> --source <legacy-launch-source.json> --destination <absolute-directory> --operation-id <stable-id>\nStopped legacy observer only. No key/world creation, writer handoff or formal proof upgrade. Original directories remain. External chain artifacts remain explicit backup dependencies.";
