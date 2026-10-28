//! Local staged refresh from one actual candidate build, preserving other evidence.
use super::*;

fn preserved_source(
    entry: &ModuleIdentityEntry,
    source: &oasis7_wasm_build::SourceInputs,
) -> Result<(), String> {
    oasis7_wasm_build::validate_source_evidence(
        entry.source_input_rule_version,
        entry.source_inputs.as_deref(),
        &entry.source_hash,
        source,
    )
    .map_err(|error| {
        format!(
            "preserved candidate source changed {}: {error}",
            entry.module_id
        )
    })?;
    let identity = sha256_hex(
        format!(
            "{}:{}:{}",
            entry.module_id, entry.source_hash, entry.build_manifest_hash
        )
        .as_bytes(),
    );
    if identity != entry.identity_hash {
        return Err("preserved candidate identity mismatch".into());
    }
    Ok(())
}

fn recorded_count(record: &CandidateSet, manifest: &IdentityManifest) -> Result<(), String> {
    if record.module_count != manifest.modules.len() {
        return Err("partial candidate preserved module count mismatch".into());
    }
    Ok(())
}

fn selected_set<'a>(baseline: &'a Descriptor, module: &str) -> Result<&'a str, String> {
    if !baseline.generation_executions.contains_key(module) {
        return Err("unknown partial candidate module".into());
    }
    baseline
        .module_sets
        .keys()
        .find(|set| module.starts_with(&format!("{set}.")))
        .map(String::as_str)
        .ok_or_else(|| "unknown partial candidate module set".into())
}

pub(super) fn prepare(
    root: &Path,
    metadata: &Path,
    module: &str,
    output: &Path,
) -> Result<(), String> {
    if ci_active(env::var("CI").ok().as_deref())
        || ci_active(env::var("GITHUB_ACTIONS").ok().as_deref())
        || env::var("OASIS7_WASM_SYNC_WRITE_ALLOW").as_deref() != Ok("local-dev")
    {
        return Err(
            "candidate partial prepare requires explicit local-dev opt-in outside CI".into(),
        );
    }
    frozen(root)?;
    known_receipts(root, metadata, false)?;
    let dir = root.join(ARTIFACTS);
    let mut baseline = descriptor(&dir)?;
    let set = selected_set(&baseline, module)?.to_string();
    let current_recipe =
        oasis7_wasm_build::compute_builder_recipe(root).map_err(|e| e.to_string())?;
    if baseline.build_recipe.builder_recipe.as_ref() != Some(&current_recipe) {
        return Err("partial candidate update cannot change shared producer recipe".into());
    }
    let receipt_names: BTreeSet<_> = fs::read_dir(metadata)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".build-receipt.json"))
        .collect();
    if receipt_names != BTreeSet::from([format!("{module}.build-receipt.json")]) {
        return Err("partial candidate update requires exactly the selected actual receipt".into());
    }
    let mapping =
        read_module_manifest_map(&root.join(ARTIFACTS).join("builtin_module_manifest_map.txt"))?;
    let mut all_ids = BTreeSet::new();
    let mut manifests = BTreeMap::new();
    for name in ["m1", "m4", "m5"] {
        let (identity_name, hash_name) = names(name);
        let manifest: IdentityManifest =
            serde_json::from_slice(&bytes(&dir, &identity_name)?).map_err(|e| e.to_string())?;
        membership(root, name, &manifest)?;
        if manifest.purpose.as_deref() != Some(PURPOSE)
            || manifest.schema_version != 2
            || manifest.build_recipe != baseline.build_recipe
        {
            return Err("partial candidate identity purpose/recipe mismatch".into());
        }
        let hashes = read_hash_manifest(&dir.join(hash_name))?;
        let record = &baseline.module_sets[name];
        recorded_count(record, &manifest)?;
        for entry in &manifest.modules {
            all_ids.insert(entry.module_id.clone());
            if hashes.get(&entry.module_id) != Some(&entry.hash_tokens)
                || entry.build_manifest_hash != record.build_manifest_hash
            {
                return Err("partial candidate preserved hash/build mismatch".into());
            }
            if entry.module_id != module {
                let relative = mapping
                    .get(&entry.module_id)
                    .ok_or("missing preserved module mapping")?;
                let source = oasis7_wasm_build::compute_source_inputs(
                    &root.join(relative),
                    DEFAULT_WASM_TARGET,
                )
                .map_err(|e| e.to_string())?;
                preserved_source(entry, &source)?;
            }
        }
        manifests.insert(name.to_string(), manifest);
    }
    if all_ids != baseline.generation_executions.keys().cloned().collect() {
        return Err("partial candidate generation membership mismatch".into());
    }
    let stage = output.with_extension(format!("prepare-{}", std::process::id()));
    validate_staging_output(output)?;
    validate_staging_output(&stage)?;
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        for record in baseline.module_sets.values() {
            for name in [&record.identity_file, &record.hash_file] {
                fs::write(stage.join(name), bytes(&dir, name)?).map_err(|e| e.to_string())?;
            }
        }
        let receipt: BuildReceipt =
            serde_json::from_slice(&bytes(metadata, &format!("{module}.build-receipt.json"))?)
                .map_err(|e| e.to_string())?;
        let ids_path = stage.join("selected-module.txt");
        fs::write(&ids_path, format!("{module}\n")).map_err(|e| e.to_string())?;
        let mut selected = options(root, metadata, &set, &stage);
        selected.module_ids_path = ids_path.clone();
        let manifest = manifests.get_mut(&set).ok_or("missing selected set")?;
        let hashes: String = manifest
            .modules
            .iter()
            .map(|entry| {
                if entry.module_id == module {
                    format!("{module} linux-x86_64={}\n", receipt.wasm_hash_sha256)
                } else {
                    format!("{} {}\n", entry.module_id, entry.hash_tokens.join(" "))
                }
            })
            .collect();
        fs::write(&selected.hash_manifest_path, &hashes).map_err(|e| e.to_string())?;
        let fresh = expected_manifest(&selected)?;
        actual_wasm(metadata, &fresh)?;
        if fresh.modules.len() != 1
            || fresh.modules[0].module_id != module
            || fresh.build_recipe != baseline.build_recipe
            || fresh.modules[0].build_manifest_hash
                != baseline.module_sets[&set].build_manifest_hash
        {
            return Err("partial candidate update changes shared build contract".into());
        }
        let entry = manifest
            .modules
            .iter_mut()
            .find(|entry| entry.module_id == module)
            .ok_or("selected module missing from identity")?;
        *entry = fresh.modules[0].clone();
        baseline.generation_executions.insert(
            module.to_string(),
            receipt
                .builder_execution
                .ok_or("selected actual execution missing")?,
        );
        fs::remove_file(ids_path).map_err(|e| e.to_string())?;
        let mut encoded = serde_json::to_vec_pretty(&sort_manifest(manifest.clone()))
            .map_err(|e| e.to_string())?;
        encoded.push(b'\n');
        fs::write(&selected.identity_manifest_path, &encoded).map_err(|e| e.to_string())?;
        let record = baseline
            .module_sets
            .get_mut(&set)
            .ok_or("missing selected descriptor")?;
        record.hash_sha256 = sha256_hex(hashes.as_bytes());
        record.identity_sha256 = sha256_hex(&encoded);
        let mut descriptor_bytes =
            serde_json::to_vec_pretty(&baseline).map_err(|e| e.to_string())?;
        descriptor_bytes.push(b'\n');
        fs::write(stage.join(DESCRIPTOR), descriptor_bytes).map_err(|e| e.to_string())?;
        descriptor(&stage)?;
        frozen(root)?;
        fs::rename(&stage, output).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserved_source_and_identity_must_both_still_match() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let manifest: IdentityManifest =
            serde_json::from_slice(&bytes(&root.join(ARTIFACTS), &names("m1").0).unwrap()).unwrap();
        let entry = manifest
            .modules
            .iter()
            .find(|entry| entry.module_id == "m1.rule.move")
            .unwrap();
        let source = oasis7_wasm_build::compute_source_inputs(
            &root.join("crates/oasis7_builtin_wasm_modules/m1_rule_move/Cargo.toml"),
            DEFAULT_WASM_TARGET,
        )
        .unwrap();
        preserved_source(entry, &source).unwrap();
        for field in ["identity", "hash", "inputs", "version"] {
            let mut invalid = entry.clone();
            match field {
                "identity" => invalid.identity_hash = "0".repeat(64),
                "hash" => invalid.source_hash = "0".repeat(64),
                "inputs" => invalid.source_inputs = Some(vec![]),
                _ => invalid.source_input_rule_version = None,
            }
            assert!(preserved_source(&invalid, &source).is_err(), "{field}");
        }
    }

    #[test]
    fn exact_known_module_selection_rejects_path_injection_and_other_ids() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let baseline = descriptor(&root.join(ARTIFACTS)).unwrap();
        assert_eq!(selected_set(&baseline, "m1.power.storage").unwrap(), "m1");
        for module in [
            "../m1.power.storage",
            "m1.power.storage/other",
            "m1.unknown",
            "m4.power.storage",
            "",
        ] {
            assert!(selected_set(&baseline, module).is_err(), "{module}");
        }
    }
    #[test]
    fn corrupted_preserved_module_count_is_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let mut baseline = descriptor(&root.join(ARTIFACTS)).unwrap();
        let manifest: IdentityManifest =
            serde_json::from_slice(&bytes(&root.join(ARTIFACTS), &names("m1").0).unwrap()).unwrap();
        recorded_count(&baseline.module_sets["m1"], &manifest).unwrap();
        baseline.module_sets.get_mut("m1").unwrap().module_count += 1;
        assert!(recorded_count(&baseline.module_sets["m1"], &manifest).is_err());
    }
}
