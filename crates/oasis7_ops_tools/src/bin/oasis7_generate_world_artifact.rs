use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use oasis7::simulator::initialize_kernel;
use oasis7_simulator::{WorldConfig, WorldInitConfig, WorldScenario};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    scenario: WorldScenario,
    world_dir: PathBuf,
    provenance_out: PathBuf,
    public_manifest: PathBuf,
}

#[derive(Debug, Serialize)]
struct WorldGenerationProvenance {
    artifact_kind: &'static str,
    scenario_id: String,
    seed: u64,
    config: serde_json::Value,
    public_manifest_sha256: String,
    public_manifest_entry_count: usize,
}

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let options = match parse_options(raw_args.iter().map(|arg| arg.as_str())) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            print_help();
            process::exit(2);
        }
    };

    if let Err(err) = generate_world_artifact(&options) {
        eprintln!("oasis7_generate_world_artifact failed: {err}");
        process::exit(1);
    }
}

fn generate_world_artifact(options: &CliOptions) -> Result<(), String> {
    let config = WorldConfig::default();
    let spec = options.scenario.load_spec();
    let public_manifest = fs::read(&options.public_manifest).map_err(|err| {
        format!(
            "read public manifest {} failed: {err}",
            options.public_manifest.display()
        )
    })?;
    let public_manifest_entry_count = public_manifest_entry_count(&public_manifest)?;
    let init = WorldInitConfig::from_scenario(options.scenario, &config);
    let seed = init.seed;
    let (kernel, _report) = initialize_kernel(config, init).map_err(|err| {
        format!(
            "initialize scenario {} failed: {err:?}",
            options.scenario.as_str()
        )
    })?;

    fs::create_dir_all(&options.world_dir).map_err(|err| {
        format!(
            "create world dir {} failed: {err}",
            options.world_dir.display()
        )
    })?;
    kernel.save_to_dir(&options.world_dir).map_err(|err| {
        format!(
            "save world dir {} failed: {err:?}",
            options.world_dir.display()
        )
    })?;

    let provenance = WorldGenerationProvenance {
        artifact_kind: "simulator_world_generation",
        scenario_id: options.scenario.as_str().to_string(),
        seed,
        config: serde_json::to_value(&spec)
            .map_err(|err| format!("encode scenario spec failed: {err}"))?,
        public_manifest_sha256: sha256_hex(&public_manifest),
        public_manifest_entry_count,
    };
    write_pretty_json(&options.provenance_out, &provenance)?;
    Ok(())
}

fn public_manifest_entry_count(bytes: &[u8]) -> Result<usize, String> {
    let manifest: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|err| format!("decode public manifest json failed: {err}"))?;
    manifest
        .as_array()
        .map(|entries| entries.len())
        .filter(|count| *count > 0)
        .ok_or_else(|| "public manifest must be a non-empty JSON array".to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn write_pretty_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create provenance dir {} failed: {err}", parent.display()))?;
    }
    let payload = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("encode provenance json failed: {err}"))?;
    let mut payload = payload;
    payload.push(b'\n');
    fs::write(path, payload).map_err(|err| format!("write {} failed: {err}", path.display()))
}

fn parse_options<'a>(args: impl Iterator<Item = &'a str>) -> Result<CliOptions, String> {
    let mut scenario: Option<WorldScenario> = None;
    let mut world_dir: Option<PathBuf> = None;
    let mut provenance_out: Option<PathBuf> = None;
    let mut public_manifest: Option<PathBuf> = None;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg {
            "--scenario" => {
                let raw = require_arg("--scenario", args.next())?;
                scenario = Some(WorldScenario::parse(raw).ok_or_else(|| {
                    format!(
                        "unknown scenario: {raw}; available: {}",
                        WorldScenario::variants().join(", ")
                    )
                })?);
            }
            "--world-dir" => {
                world_dir = Some(PathBuf::from(require_arg("--world-dir", args.next())?));
            }
            "--provenance-out" => {
                provenance_out = Some(PathBuf::from(require_arg("--provenance-out", args.next())?));
            }
            "--public-manifest" => {
                public_manifest = Some(PathBuf::from(require_arg(
                    "--public-manifest",
                    args.next(),
                )?));
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }

    Ok(CliOptions {
        scenario: scenario.ok_or_else(|| "--scenario is required".to_string())?,
        world_dir: world_dir.ok_or_else(|| "--world-dir is required".to_string())?,
        provenance_out: provenance_out.ok_or_else(|| "--provenance-out is required".to_string())?,
        public_manifest: public_manifest
            .ok_or_else(|| "--public-manifest is required".to_string())?,
    })
}

fn require_arg<'a>(flag: &str, value: Option<&'a str>) -> Result<&'a str, String> {
    value.ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    println!(
        "Usage: oasis7_generate_world_artifact --scenario <name> --world-dir <dir> --provenance-out <path> --public-manifest <path>"
    );
    println!(
        "Available scenarios: {}",
        WorldScenario::variants().join(", ")
    );
}
