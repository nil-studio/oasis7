use std::env;
use std::path::PathBuf;
use wasm_build_suite::{BuildOutput, BuildRequest, DEFAULT_OUT_DIR, DEFAULT_TARGET, run_build};

fn main() {
    if let Err(err) = run_cli() {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Err("missing command".to_string());
    };

    match command.as_str() {
        "producer-recipe" => {
            let args = args.collect::<Vec<_>>();
            if args.len() != 2 {
                return Err("producer-recipe requires <workspace-root> <output-json>".into());
            }
            let recipe = oasis7_wasm_build::compute_builder_recipe(std::path::Path::new(&args[0]))
                .map_err(|e| e.to_string())?;
            let encoded = serde_json::to_vec_pretty(&recipe).map_err(|e| e.to_string())?;
            std::fs::write(&args[1], encoded).map_err(|e| e.to_string())?;
            Ok(())
        }
        "candidate-builder-evidence" => {
            let args = args.collect::<Vec<_>>();
            if args.len() != 3 {
                return Err("candidate-builder-evidence requires <recipe-json> <docker-inspect-json> <output-json>".into());
            }
            let recipe: oasis7_wasm_build::BuilderRecipe =
                serde_json::from_slice(&std::fs::read(&args[0]).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let inspected: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&args[1]).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let evidence = oasis7_wasm_build::candidate_builder_evidence(recipe, &inspected)?;
            std::fs::write(
                &args[2],
                serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        "build" => {
            let request = parse_build_args(args.collect())?;
            let output = run_build(&request).map_err(|err| err.to_string())?;
            print_build_output(&output);
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown command: {other}")),
    }
}

fn parse_build_args(args: Vec<String>) -> Result<BuildRequest, String> {
    let mut module_id: Option<String> = None;
    let mut manifest_path: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from(DEFAULT_OUT_DIR);
    let mut profile = "release".to_string();
    let mut target = DEFAULT_TARGET.to_string();
    let mut dry_run = false;

    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--module-id" => {
                index += 1;
                module_id = args.get(index).cloned();
            }
            "--manifest-path" => {
                index += 1;
                manifest_path = args.get(index).map(PathBuf::from);
            }
            "--out-dir" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    out_dir = PathBuf::from(value);
                } else {
                    return Err("--out-dir requires a value".to_string());
                }
            }
            "--profile" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    profile = value.clone();
                } else {
                    return Err("--profile requires a value".to_string());
                }
            }
            "--target" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    target = value.clone();
                } else {
                    return Err("--target requires a value".to_string());
                }
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown option for build: {other}"));
            }
        }
        index += 1;
    }

    let module_id = module_id.ok_or_else(|| "--module-id is required".to_string())?;
    let manifest_path = manifest_path.ok_or_else(|| "--manifest-path is required".to_string())?;
    Ok(BuildRequest {
        module_id,
        manifest_path,
        out_dir,
        target,
        profile,
        dry_run,
    })
}

fn print_build_output(output: &BuildOutput) {
    println!("module_id={}", output.module_id);
    println!(
        "source_artifact_path={}",
        output.source_artifact_path.to_string_lossy()
    );
    println!(
        "packaged_wasm_path={}",
        output.packaged_wasm_path.to_string_lossy()
    );
    println!("metadata_path={}", output.metadata_path.to_string_lossy());
    println!("receipt_path={}", output.receipt_path.to_string_lossy());
    if output.dry_run {
        println!("mode=dry-run");
    } else {
        if let Some(rule) = output.source_input_rule_version {
            println!("source_input_rule_version={rule}");
        }
        if let Some(source_hash) = &output.source_hash {
            println!("source_hash={source_hash}");
        }
        if let Some(build_manifest_hash) = &output.build_manifest_hash {
            println!("build_manifest_hash={build_manifest_hash}");
        }
        if let Some(hash) = &output.wasm_hash_sha256 {
            println!("wasm_hash_sha256={hash}");
        }
        if let Some(size) = output.wasm_size_bytes {
            println!("wasm_size_bytes={size}");
        }
    }
}

fn print_usage() {
    println!("wasm_build_suite build --module-id <id> --manifest-path <path> [options]");
    println!("options:");
    println!("  --out-dir <path>      default: .tmp/wasm-build-suite");
    println!("  --profile <profile>   release|dev (default: release)");
    println!("  --target <triple>     default: wasm32-unknown-unknown");
    println!("  --dry-run             validate and print resolved paths");
}
