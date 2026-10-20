//! Local Unix process ownership for existing-world observer compatibility nodes.
//! No world/authority creation, formal profile activation, or validator mutation.
use std::collections::BTreeMap;
use std::path::Path;

#[cfg(unix)]
#[path = "node_lifecycle/bootstrap.rs"]
mod bootstrap;
#[path = "node_lifecycle/formal_full_sync.rs"]
mod formal_full_sync;
#[path = "node_lifecycle/formal_observer.rs"]
mod formal_observer;
#[cfg(unix)]
#[path = "node_lifecycle/legacy_migration.rs"]
pub(crate) mod legacy_migration;
#[path = "node_lifecycle/local_io.rs"]
pub(crate) mod local_io;
#[cfg(unix)]
#[path = "node_lifecycle/migration.rs"]
pub(crate) mod migration;
#[path = "node_lifecycle/model.rs"]
mod model;
#[cfg(unix)]
#[path = "node_lifecycle/package.rs"]
pub(crate) mod package;
#[cfg(unix)]
#[path = "node_lifecycle/runtime_compatibility.rs"]
pub(crate) mod runtime_compatibility;
#[path = "node_lifecycle/types.rs"]
mod types;
pub(crate) use types::{LocalActionRecord, LocalInstance, LocalRuntimeArtifactV1};
#[cfg(unix)]
#[path = "node_lifecycle/actions.rs"]
pub(crate) mod actions;
#[cfg(unix)]
#[path = "node_lifecycle/management.rs"]
pub(crate) mod management;
#[cfg(unix)]
#[path = "node_lifecycle/operation_read.rs"]
pub(crate) mod operation_read;
#[cfg(unix)]
#[path = "node_lifecycle/process.rs"]
mod process;

const HELP: &str = "Usage: oasis7_chain_runtime node init --chain <trusted-chain.json> --data-dir <absolute-directory> [--node-id <local-label>] [--rpc-bind <loopback-ip:port>]\n       oasis7_chain_runtime node full-sync-grant --data-dir <directory> --grant <issuer-signed-grant.json>\n       oasis7_chain_runtime node start --config <node.json>\n       oasis7_chain_runtime node config-check --config <node.json> (read-only identity and chain validation)\n       oasis7_chain_runtime node runtime-compatibility --config <node.json> [--data-dir <same-bound-directory>] [--minimum-full-sync-cut <cut.json> --minimum-full-sync-cut-sha256 <sha256>] (read-only software format descriptor and stored-format probe)\n       oasis7_chain_runtime node status --data-dir <directory>\n       oasis7_chain_runtime node stop --data-dir <directory> [--timeout-ms <1..60000>]\nSupports compatibility_observer_v1, formal_public_observer_v1 and formal_full_state_sync_read_only_v1 on Unix. Full-state sync requires the activation issuer's signed, node-key-bound grant; readiness follows verified Snapshot/Journal/retained-CAS restoration and never grants proposal rights. Public observer readiness remains projection-only. Guarded/archive/validator profiles remain unsupported. Stop preserves all state and keys.";

pub(super) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    #[cfg(not(unix))]
    {
        let _ = args;
        return Err("local node lifecycle requires Unix".into());
    }
    #[cfg(unix)]
    {
        let arguments = Arguments::parse(args)?;
        let Some(arguments) = arguments else {
            println!("{HELP}");
            return Ok(());
        };
        let output = match arguments.command.as_str() {
            "init" => {
                let chain = model::ChainConfig::load(arguments.path("--chain")?)?;
                let data = arguments.path("--data-dir")?;
                let bind = arguments
                    .values
                    .get("--rpc-bind")
                    .map(String::as_str)
                    .unwrap_or("127.0.0.1:9080");
                let config = model::initialize(
                    chain,
                    data,
                    arguments.values.get("--node-id").map(String::as_str),
                    bind,
                )?;
                serde_json::json!({"initialized":true,"config":config,"world_created":false,"authority_created":false})
            }
            "full-sync-grant" => formal_full_sync::install_grant(
                arguments.path("--data-dir")?,
                arguments.path("--grant")?,
            )?,
            "start" => process::start(arguments.path("--config")?)?,
            "config-check" => check_config(arguments.path("--config")?)?,
            "runtime-compatibility" => runtime_compatibility::describe_with_minimum(
                arguments.path("--config")?,
                arguments.values.get("--data-dir").map(Path::new),
                arguments
                    .values
                    .get("--minimum-full-sync-cut")
                    .map(Path::new),
                arguments
                    .values
                    .get("--minimum-full-sync-cut-sha256")
                    .map(String::as_str),
            )?,
            "status" => process::status(arguments.path("--data-dir")?)?,
            "stop" => process::stop(arguments.path("--data-dir")?, arguments.timeout_ms()?)?,
            "_run" => {
                process::supervise(
                    arguments.path("--config")?,
                    arguments
                        .values
                        .get("--incarnation")
                        .ok_or("missing incarnation")?,
                )?;
                return Ok(());
            }
            _ => unreachable!(),
        };
        println!(
            "{}",
            serde_json::to_string(&output).map_err(|e| e.to_string())?
        );
        Ok(())
    }
}

#[derive(Debug)]
struct Arguments {
    command: String,
    values: BTreeMap<String, String>,
}

#[cfg(unix)]
fn check_config(path: &Path) -> Result<serde_json::Value, String> {
    let config = model::NodeConfig::load(path)?;
    config.verify_identity()?;
    config.identity.chain.verify()?;
    Ok(serde_json::json!({
        "configuration_valid": true,
        "node_id": config.identity.node_id,
        "peer_id": config.identity.peer_id,
        "world_id": config.identity.chain.world_id(),
        "chain_id": config.identity.chain.chain_id(),
        "configured_role": config.role,
        "data_dir": config.identity.data_dir,
        "rpc_bind": config.rpc_bind,
        "process_started": false,
        "authority_created": false
    }))
}
impl Arguments {
    fn parse<'a>(args: impl Iterator<Item = &'a str>) -> Result<Option<Self>, String> {
        let mut args = args.peekable();
        let command = args.next().ok_or(HELP)?;
        if matches!(command, "--help" | "-h") {
            return Ok(None);
        }
        let allowed: &[&str] = match command {
            "init" => &["--chain", "--data-dir", "--node-id", "--rpc-bind"],
            "full-sync-grant" => &["--data-dir", "--grant"],
            "start" | "config-check" => &["--config"],
            "runtime-compatibility" => &[
                "--config",
                "--data-dir",
                "--minimum-full-sync-cut",
                "--minimum-full-sync-cut-sha256",
            ],
            "status" => &["--data-dir"],
            "stop" => &["--data-dir", "--timeout-ms"],
            "_run" => &["--config", "--incarnation"],
            _ => return Err(HELP.into()),
        };
        let mut values = BTreeMap::new();
        while let Some(flag) = args.next() {
            if matches!(flag, "--help" | "-h") && values.is_empty() && args.peek().is_none() {
                return Ok(None);
            }
            if !allowed.contains(&flag) {
                return Err(format!("unsupported {command} option: {flag}"));
            }
            let value = args
                .next()
                .filter(|s| !s.starts_with("--") && !s.trim().is_empty())
                .ok_or_else(|| format!("{flag} requires a value"))?;
            if values.insert(flag.to_string(), value.to_string()).is_some() {
                return Err(format!("duplicate {flag}"));
            }
        }
        let result = Self {
            command: command.into(),
            values,
        };
        if command == "runtime-compatibility"
            && result.values.contains_key("--minimum-full-sync-cut")
                != result.values.contains_key("--minimum-full-sync-cut-sha256")
        {
            return Err("minimum full-sync cut path and SHA-256 must be supplied together".into());
        }
        for required in match command {
            "init" => vec!["--chain", "--data-dir"],
            "full-sync-grant" => vec!["--data-dir", "--grant"],
            "start" | "config-check" | "runtime-compatibility" | "_run" => vec!["--config"],
            _ => vec!["--data-dir"],
        } {
            result.path(required)?;
        }
        result.timeout_ms()?;
        Ok(Some(result))
    }
    fn path(&self, flag: &str) -> Result<&Path, String> {
        self.values
            .get(flag)
            .map(Path::new)
            .ok_or_else(|| format!("{} requires {flag}", self.command))
    }
    fn timeout_ms(&self) -> Result<u64, String> {
        self.values.get("--timeout-ms").map_or(Ok(10_000), |s| {
            s.parse::<u64>().map_err(|e| e.to_string()).and_then(|v| {
                if (1..=60_000).contains(&v) {
                    Ok(v)
                } else {
                    Err("timeout outside 1..60000ms".into())
                }
            })
        })
    }
}

#[cfg(test)]
#[path = "node_lifecycle/tests.rs"]
pub(crate) mod tests;
