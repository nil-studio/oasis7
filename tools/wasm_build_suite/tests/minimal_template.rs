use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmparser::Payload;

use wasm_build_suite::{BuildMetadata, BuildReceipt, BuildRequest, DEFAULT_TARGET, run_build};

fn template_manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join("minimal_module")
        .join("Cargo.toml")
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn has_target_installed(target: &str) -> bool {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == target)
}

fn contains_custom_section(bytes: &[u8]) -> bool {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .filter_map(Result::ok)
        .any(|payload| matches!(payload, Payload::CustomSection(_)))
}

#[test]
fn minimal_template_dry_run_resolves_paths() {
    let out_dir = unique_temp_dir("wasm-build-suite-dry-run");
    let module_id = "kwt.template.dryrun";
    let request = BuildRequest {
        module_id: module_id.to_string(),
        manifest_path: template_manifest_path(),
        out_dir: out_dir.clone(),
        target: DEFAULT_TARGET.to_string(),
        profile: "dev".to_string(),
        dry_run: true,
    };

    let output = run_build(&request).expect("dry-run build should succeed");
    assert!(output.dry_run);
    assert!(
        output
            .source_artifact_path
            .ends_with("wasm32-unknown-unknown/debug/minimal_wasm_module.wasm")
    );
    assert_eq!(
        output.packaged_wasm_path,
        out_dir.join(format!("{module_id}.wasm"))
    );
    assert_eq!(
        output.metadata_path,
        out_dir.join(format!("{module_id}.metadata.json"))
    );
    assert_eq!(
        output.receipt_path,
        out_dir.join(format!("{module_id}.build-receipt.json"))
    );
    assert!(output.wasm_hash_sha256.is_none());
    assert!(output.source_hash.is_none());
    assert!(output.build_manifest_hash.is_none());
    assert!(output.build_timing.is_none());
    assert!(output.wasm_size_bytes.is_none());
}

#[test]
fn minimal_template_rejects_path_shaped_module_id() {
    let out_dir = unique_temp_dir("wasm-build-suite-path-safe");
    let request = BuildRequest {
        module_id: "../escape".to_string(),
        manifest_path: template_manifest_path(),
        out_dir,
        target: DEFAULT_TARGET.to_string(),
        profile: "dev".to_string(),
        dry_run: true,
    };

    let err = run_build(&request).expect_err("path-shaped module_id must be rejected");
    assert!(
        err.to_string().contains("module_id"),
        "unexpected error: {err}"
    );
}

#[test]
fn minimal_template_real_build_writes_wasm_and_metadata() {
    if !has_target_installed(DEFAULT_TARGET) {
        eprintln!(
            "skip minimal_template_real_build_writes_wasm_and_metadata: target {DEFAULT_TARGET} is not installed"
        );
        return;
    }

    let out_dir = unique_temp_dir("wasm-build-suite-build");
    let module_id = "kwt.template.real";
    let request = BuildRequest {
        module_id: module_id.to_string(),
        manifest_path: template_manifest_path(),
        out_dir: out_dir.clone(),
        target: DEFAULT_TARGET.to_string(),
        profile: "dev".to_string(),
        dry_run: false,
    };

    let output = run_build(&request).expect("real build should succeed");
    assert!(!output.dry_run);
    assert!(output.packaged_wasm_path.exists());
    assert!(output.metadata_path.exists());
    assert!(output.receipt_path.exists());
    assert_eq!(
        output.wasm_size_bytes,
        Some(
            fs::metadata(&output.packaged_wasm_path)
                .expect("read wasm metadata")
                .len()
        )
    );
    assert_eq!(
        output.wasm_hash_sha256.as_ref().map(|hash| hash.len()),
        Some(64)
    );
    let timing = output
        .build_timing
        .clone()
        .expect("real build should expose build timing");
    assert!(timing.total_build_wall_ms >= timing.cargo_build_ms);
    assert!(timing.total_build_wall_ms >= timing.receipt_write_ms);
    assert!(timing.total_build_wall_ms >= timing.metadata_write_ms);
    let packaged_wasm_bytes = fs::read(&output.packaged_wasm_path).expect("read packaged wasm");
    assert!(
        !contains_custom_section(&packaged_wasm_bytes),
        "packaged wasm should be canonicalized without custom sections"
    );

    let metadata_bytes = fs::read(&output.metadata_path).expect("read metadata json");
    let metadata: BuildMetadata =
        serde_json::from_slice(&metadata_bytes).expect("parse metadata json");
    assert_eq!(metadata.module_id, module_id);
    assert_eq!(metadata.target, DEFAULT_TARGET);
    assert_eq!(metadata.profile, "dev");
    assert_eq!(
        metadata.packaged_wasm_path,
        output.packaged_wasm_path.to_string_lossy()
    );
    assert_eq!(
        metadata.build_receipt_path,
        output.receipt_path.to_string_lossy()
    );
    assert!(metadata.recorded_at_unix_ms > 0);
    assert_eq!(metadata.build_timing.cargo_build_ms, timing.cargo_build_ms);
    assert_eq!(
        metadata.build_timing.canonicalize_ms,
        timing.canonicalize_ms
    );
    assert_eq!(metadata.build_timing.hash_ms, timing.hash_ms);
    assert!(metadata.build_timing.total_build_wall_ms >= metadata.build_timing.cargo_build_ms);
    assert!(metadata.build_timing.total_build_wall_ms >= metadata.build_timing.receipt_write_ms);
    assert!(metadata.build_timing.total_build_wall_ms >= metadata.build_timing.metadata_write_ms);

    let receipt_bytes = fs::read(&output.receipt_path).expect("read build receipt json");
    let receipt: BuildReceipt =
        serde_json::from_slice(&receipt_bytes).expect("parse build receipt json");
    assert_eq!(receipt.schema_version, 3);
    assert_eq!(receipt.source_input_rule_version, Some(2));
    assert_eq!(receipt.source_inputs, metadata.source_inputs);
    let inputs = oasis7_wasm_build::compute_source_inputs(&request.manifest_path, DEFAULT_TARGET)
        .expect("shared source inputs");
    assert_eq!(receipt.source_hash, inputs.source_hash);
    assert_eq!(
        receipt.source_inputs.as_deref(),
        Some(inputs.source_inputs.as_slice())
    );
    assert_eq!(receipt.module_id, module_id);
    assert_eq!(receipt.target, DEFAULT_TARGET);
    assert_eq!(receipt.profile, "dev");
    assert_eq!(receipt.wasm_hash_sha256, metadata.wasm_hash_sha256);
    assert_eq!(receipt.build_manifest_hash, metadata.build_manifest_hash);
    assert_eq!(receipt.recorded_at_unix_ms, metadata.recorded_at_unix_ms);
    assert_eq!(receipt.build_timing, metadata.build_timing);

    let _ = fs::remove_dir_all(out_dir);
}
