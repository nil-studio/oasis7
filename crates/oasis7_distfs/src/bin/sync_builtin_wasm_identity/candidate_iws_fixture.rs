//! Read-only validation of the fixed actual IWS Rule candidate test material.
//! This never generates, refreshes or selects a release identity.
use super::*;
const MODULE: &str = "m1.rule.transfer";
const MANIFEST: &str = "crates/oasis7_builtin_wasm_modules/m1_rule_transfer/Cargo.toml";

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err(
            "--verify-iws-rule-candidate-fixture requires <workspace-root> <fixture-directory>"
                .into(),
        );
    }
    let root = Path::new(&args[1])
        .canonicalize()
        .map_err(|e| e.to_string())?;
    // Keep the unresolved fixture path for the original ancestor/symlink checks.
    let fixture = env::current_dir()
        .map_err(|e| e.to_string())?
        .join(&args[2]);
    let mut paths = Vec::with_capacity(3);
    for (suffix, limit) in [
        (".wasm", 8 << 20),
        (".metadata.json", 256 << 10),
        (".build-receipt.json", 256 << 10),
    ] {
        let path = fixture.join(format!("{MODULE}{suffix}"));
        for ancestor in path.ancestors() {
            let info = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
            if info.file_type().is_symlink() || (ancestor != path && !info.is_dir()) {
                return Err("IWS candidate fixture requires real files and directories".into());
            }
        }
        let info = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !info.is_file() || info.len() == 0 || info.len() > limit {
            return Err("IWS candidate fixture file kind/size mismatch".into());
        }
        paths.push(
            path.to_str()
                .ok_or("IWS candidate fixture path is not UTF8")?
                .to_owned(),
        );
    }
    // The existing checker verifies actual bytes, paired metadata/receipt,
    // SourceRule2 closure, manifest hash and the current producer recipe.
    let receipt = crate::verify_provider::verify(&root, MODULE, MANIFEST, &paths)?;
    verify_selected_candidate(&root, &receipt)?;
    println!("verified_iws_rule_candidate_module={MODULE}");
    println!(
        "verified_iws_rule_candidate_source_hash={}",
        receipt.source_hash
    );
    println!(
        "verified_iws_rule_candidate_build_manifest_hash={}",
        receipt.build_manifest_hash
    );
    println!(
        "verified_iws_rule_candidate_wasm_hash={}",
        receipt.wasm_hash_sha256
    );
    Ok(())
}

fn verify_selected_candidate(root: &Path, receipt: &BuildReceipt) -> Result<(), String> {
    frozen(root)?;
    let directory = root.join(ARTIFACTS);
    // This validates purpose/schema and the descriptor's complete file hashes.
    // It does not rebuild the other 36 guests or replace their runtime evidence.
    let baseline = descriptor(&directory)?;
    let selected = baseline
        .module_sets
        .get("m1")
        .ok_or("IWS candidate m1 set missing")?;
    let manifest: IdentityManifest =
        serde_json::from_slice(&bytes(&directory, &selected.identity_file)?)
            .map_err(|e| e.to_string())?;
    membership(root, "m1", &manifest)?;
    if manifest.purpose.as_deref() != Some(PURPOSE)
        || manifest.schema_version != 2
        || manifest.module_set != "m1"
        || manifest.build_recipe != baseline.build_recipe
        || manifest.modules.len() != selected.module_count
    {
        return Err("IWS fixture is not bound to the current candidate manifest".into());
    }
    let mut entries = manifest
        .modules
        .iter()
        .filter(|entry| entry.module_id == MODULE);
    let entry = entries
        .next()
        .ok_or("IWS Rule missing from candidate manifest")?;
    if entries.next().is_some() {
        return Err("duplicate IWS Rule candidate entry".into());
    }
    let expected_identity = sha256_hex(
        format!(
            "{MODULE}:{}:{}",
            entry.source_hash, entry.build_manifest_hash
        )
        .as_bytes(),
    );
    if entry.source_input_rule_version != receipt.source_input_rule_version
        || entry.source_inputs != receipt.source_inputs
        || entry.source_hash != receipt.source_hash
        || entry.build_manifest_hash != receipt.build_manifest_hash
        || entry.build_manifest_hash != selected.build_manifest_hash
        || entry.identity_hash != expected_identity
        || entry.hash_tokens != [format!("linux-x86_64={}", receipt.wasm_hash_sha256)]
        || receipt.builder_recipe.as_ref() != baseline.build_recipe.builder_recipe.as_ref()
        || receipt.builder_execution.as_ref() != baseline.generation_executions.get(MODULE)
    {
        return Err("IWS Rule fixture source/recipe/WASM differs from current candidate".into());
    }
    let hashes = read_hash_manifest(&directory.join(&selected.hash_file))?;
    if hashes.get(MODULE) != Some(&entry.hash_tokens) {
        return Err("IWS Rule candidate hash manifest mismatch".into());
    }
    Ok(())
}
