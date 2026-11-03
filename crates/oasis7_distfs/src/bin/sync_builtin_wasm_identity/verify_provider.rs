//! Read-only CI test material validation. Never enters identity sync/write paths.
use super::*;
const MODULE: &str = "module.runtime.local-test-provider";
const MANIFEST: &str = "crates/oasis7_builtin_wasm_modules/local_test_provider/Cargo.toml";
pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 5 {
        return Err(
            "--verify-provider-artifact requires <workspace-root> <wasm> <metadata> <receipt>"
                .into(),
        );
    }
    let root = Path::new(&args[1])
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let wasm = fs::read(&args[2]).map_err(|e| e.to_string())?;
    let metadata_bytes = fs::read(&args[3]).map_err(|e| e.to_string())?;
    let receipt_bytes = fs::read(&args[4]).map_err(|e| e.to_string())?;
    validate_builder_field_presence(&metadata_bytes, true)?;
    validate_builder_field_presence(&receipt_bytes, true)?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata_bytes).map_err(|e| e.to_string())?;
    let raw_receipt: serde_json::Value =
        serde_json::from_slice(&receipt_bytes).map_err(|e| e.to_string())?;
    let receipt: BuildReceipt =
        serde_json::from_slice(&receipt_bytes).map_err(|e| e.to_string())?;
    if receipt.module_id != MODULE
        || receipt.profile != "release"
        || receipt.purpose.as_deref() != Some("candidate-build")
        || wasm.get(..8) != Some(b"\0asm\x01\0\0\0")
        || receipt.wasm_hash_sha256 != sha256_hex(&wasm)
    {
        return Err("provider module/profile/WASM identity mismatch".into());
    }
    for key in [
        "module_id",
        "target",
        "profile",
        "source_input_rule_version",
        "source_inputs",
        "source_hash",
        "build_manifest_hash",
        "canonicalizer_version",
        "container_platform",
        "wasm_toolchain",
        "wasm_build_std",
        "wasm_build_std_components",
        "wasm_build_std_features",
        "wasm_deterministic_guard",
        "purpose",
        "build_manifest_schema_version",
        "builder_recipe",
        "builder_execution",
        "wasm_hash_sha256",
        "source_manifest_path",
    ] {
        if metadata.get(key).is_none() || metadata.get(key) != raw_receipt.get(key) {
            return Err(format!("provider metadata/receipt differs: {key}"));
        }
    }
    if metadata.get("wasm_size_bytes").and_then(|v| v.as_u64()) != Some(wasm.len() as u64) {
        return Err("provider WASM length differs".into());
    }
    let expected =
        oasis7_wasm_build::compute_source_inputs(&root.join(MANIFEST), DEFAULT_WASM_TARGET)
            .map_err(|e| e.to_string())?;
    validate_receipt_source(&receipt, &expected)?;
    let recipe = oasis7_wasm_build::compute_builder_recipe(&root).map_err(|e| e.to_string())?;
    if receipt.builder_recipe.as_ref() != Some(&recipe) {
        return Err("provider producer recipe differs from tested checkout".into());
    }
    println!("verified_provider_module={MODULE}");
    Ok(())
}
