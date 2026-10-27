//! Engineering expectations only. This module never selects a Runtime/release identity.
use super::*;

const ARTIFACTS: &str = "crates/oasis7_runtime/src/world/artifacts";
const DESCRIPTOR: &str = "builtin_wasm_candidate_baseline.v1.json";
const PURPOSE: &str = "candidate-build";
const LEGACY: [(&str, &str); 6] = [
    (
        "m1_builtin_modules.identity.json",
        "bc4402578935a14381228b2b66d3fe077f4a770fef897cc3927ce08a1528ebf9",
    ),
    (
        "m1_builtin_modules.sha256",
        "f92378e5ad249ce3ce46c71dd2ebb32986734600549adb6385f8600b0d50be2d",
    ),
    (
        "m4_builtin_modules.identity.json",
        "2624f8bbb13d1711138df143298063501a1fbee1be4f338eb1d0cfcf12b3f994",
    ),
    (
        "m4_builtin_modules.sha256",
        "2e9cc243483acd9d72b77ccb09d6fb5857e51f977285ce5ce8c9b23709c147d1",
    ),
    (
        "m5_builtin_modules.identity.json",
        "ebb07fa6f6a545e71cfcff121c9b46e4b10201a20bf3733a4d7d3c5fb699be8d",
    ),
    (
        "m5_builtin_modules.sha256",
        "154fd149e18b22312a34e611e7e7b24c35314d89e05ee70af5adf2cdc47aeec9",
    ),
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateSet {
    identity_file: String,
    identity_sha256: String,
    hash_file: String,
    hash_sha256: String,
    module_count: usize,
    build_manifest_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    descriptor_schema_version: u32,
    purpose: String,
    source_input_rule_version: u32,
    receipt_schema_version: u32,
    identity_schema_version: u32,
    target: String,
    profile: String,
    build_recipe: BuildRecipe,
    #[serde(deserialize_with = "unique_map")]
    frozen_release_sha256: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    module_sets: BTreeMap<String, CandidateSet>,
}

fn unique_map<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unique object keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if result.insert(key.clone(), value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate candidate key {key}"
                    )));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

fn legacy_map() -> BTreeMap<String, String> {
    LEGACY
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect()
}

fn bytes(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    if Path::new(name).components().count() != 1 || name.contains(['/', '\\']) {
        return Err("candidate baseline path must be an exact filename".into());
    }
    // Check every component before reading, including the directory itself.
    let path = dir.join(name);
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("candidate baseline symlink is unsupported".into());
        }
    }
    fs::read(path).map_err(|e| e.to_string())
}

fn frozen(root: &Path) -> Result<(), String> {
    for (name, expected) in LEGACY {
        if sha256_hex(&bytes(&root.join(ARTIFACTS), name)?) != expected {
            return Err(format!("frozen release bytes changed: {name}"));
        }
    }
    Ok(())
}

fn names(set: &str) -> (String, String) {
    (
        format!("{set}_builtin_modules.source-rule2-candidate.identity.json"),
        format!("{set}_builtin_modules.source-rule2-candidate.sha256"),
    )
}

fn options(root: &Path, metadata: &Path, set: &str, dir: &Path) -> Options {
    let (identity, hash) = names(set);
    Options {
        module_ids_path: root
            .join(ARTIFACTS)
            .join(format!("{set}_builtin_module_ids.txt")),
        module_manifest_map_path: root.join(ARTIFACTS).join("builtin_module_manifest_map.txt"),
        hash_manifest_path: dir.join(hash),
        identity_manifest_path: dir.join(identity),
        metadata_dir: metadata.into(),
        workspace_root: root.into(),
        profile: "release".into(),
        canonical_platforms_csv: "linux-x86_64".into(),
        check_only: true,
    }
}

fn membership(root: &Path, set: &str, manifest: &IdentityManifest) -> Result<(), String> {
    let count = match set {
        "m1" => 10,
        "m4" => 22,
        "m5" => 5,
        _ => return Err("unknown candidate module set".into()),
    };
    let released: IdentityManifest = serde_json::from_slice(&bytes(
        &root.join(ARTIFACTS),
        &format!("{set}_builtin_modules.identity.json"),
    )?)
    .map_err(|e| e.to_string())?;
    let ids = |m: &IdentityManifest| {
        m.modules
            .iter()
            .map(|v| v.module_id.clone())
            .collect::<BTreeSet<_>>()
    };
    if manifest.modules.len() != count
        || ids(manifest).len() != count
        || ids(manifest) != ids(&released)
    {
        return Err("candidate module membership mismatch".into());
    }
    Ok(())
}

fn actual_wasm(metadata: &Path, manifest: &IdentityManifest) -> Result<(), String> {
    for entry in &manifest.modules {
        let actual = sha256_hex(&bytes(metadata, &format!("{}.wasm", entry.module_id))?);
        if entry.hash_tokens != [format!("linux-x86_64={actual}")] {
            return Err(format!(
                "candidate packaged WASM mismatch: {}",
                entry.module_id
            ));
        }
    }
    Ok(())
}

fn known_receipts(root: &Path, metadata: &Path, complete: bool) -> Result<(), String> {
    let mut expected = BTreeSet::new();
    for set in ["m1", "m4", "m5"] {
        for id in read_module_ids(
            &root
                .join(ARTIFACTS)
                .join(format!("{set}_builtin_module_ids.txt")),
        )? {
            expected.insert(format!("{id}.build-receipt.json"));
        }
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(metadata).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non UTF8 metadata filename")?;
        if name.ends_with(".build-receipt.json") {
            actual.insert(name);
        }
    }
    if !actual.is_subset(&expected) || (complete && actual != expected) {
        return Err("missing/unknown candidate receipt membership".into());
    }
    Ok(())
}

fn descriptor(dir: &Path) -> Result<Descriptor, String> {
    let value: Descriptor =
        serde_json::from_slice(&bytes(dir, DESCRIPTOR)?).map_err(|e| e.to_string())?;
    if value.descriptor_schema_version != 1
        || value.purpose != PURPOSE
        || value.source_input_rule_version != 2
        || value.receipt_schema_version != 3
        || value.identity_schema_version != 2
        || value.target != DEFAULT_WASM_TARGET
        || value.profile != "release"
        || value.frozen_release_sha256 != legacy_map()
        || value
            .module_sets
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != ["m1", "m4", "m5"]
    {
        return Err("unknown/mixed candidate baseline contract".into());
    }
    for (set, record) in &value.module_sets {
        let (identity, hash) = names(set);
        if record.identity_file != identity
            || record.hash_file != hash
            || sha256_hex(&bytes(dir, &identity)?) != record.identity_sha256
            || sha256_hex(&bytes(dir, &hash)?) != record.hash_sha256
        {
            return Err("candidate baseline path/digest mismatch".into());
        }
    }
    Ok(value)
}

fn check(root: &Path, metadata: &Path, set: &str) -> Result<(), String> {
    frozen(root)?;
    known_receipts(root, metadata, false)?;
    let dir = root.join(ARTIFACTS);
    let baseline = descriptor(&dir)?;
    let record = baseline
        .module_sets
        .get(set)
        .ok_or("unknown candidate module set")?;
    let expected = sort_manifest(expected_manifest(&options(root, metadata, set, &dir))?);
    let actual: IdentityManifest =
        serde_json::from_slice(&bytes(&dir, &record.identity_file)?).map_err(|e| e.to_string())?;
    membership(root, set, &expected)?;
    actual_wasm(metadata, &expected)?;
    if expected != sort_manifest(actual)
        || expected.build_recipe != baseline.build_recipe
        || expected.modules.len() != record.module_count
        || expected
            .modules
            .iter()
            .any(|entry| entry.build_manifest_hash != record.build_manifest_hash)
    {
        return Err("candidate source/build/identity freshness mismatch".into());
    }
    println!("candidate baseline check ok: {set}");
    Ok(())
}

fn ci_active(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    })
}

fn validate_staging_output(output: &Path) -> Result<(), String> {
    let absolute = std::path::absolute(output).map_err(|e| e.to_string())?;
    if absolute
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("candidate staging path must not escape through parent traversal".into());
    }
    for ancestor in absolute.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("candidate staging ancestor is a symlink".into());
            }
            Ok(metadata) if ancestor != absolute && !metadata.is_dir() => {
                return Err("candidate staging ancestor is not a directory".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    if fs::symlink_metadata(&absolute).is_ok() {
        return Err("candidate staging output already exists".into());
    }
    Ok(())
}

fn prepare(root: &Path, metadata: &Path, output: &Path) -> Result<(), String> {
    if ci_active(env::var("CI").ok().as_deref())
        || ci_active(env::var("GITHUB_ACTIONS").ok().as_deref())
        || env::var("OASIS7_WASM_SYNC_WRITE_ALLOW").as_deref() != Ok("local-dev")
    {
        return Err("candidate prepare requires explicit local-dev opt-in outside CI".into());
    }
    frozen(root)?;
    known_receipts(root, metadata, true)?;
    if output.exists() {
        return Err("candidate staging output already exists".into());
    }
    let stage = output.with_extension(format!("prepare-{}", std::process::id()));
    validate_staging_output(output)?;
    validate_staging_output(&stage)?;
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut sets = BTreeMap::new();
        let mut recipe = None;
        for set in ["m1", "m4", "m5"] {
            let options = options(root, metadata, set, &stage);
            let ids = read_module_ids(&options.module_ids_path)?;
            let mut hashes = String::new();
            for id in &ids {
                let receipt: BuildReceipt =
                    serde_json::from_slice(&bytes(metadata, &format!("{id}.build-receipt.json"))?)
                        .map_err(|e| e.to_string())?;
                hashes.push_str(&format!("{id} linux-x86_64={}\n", receipt.wasm_hash_sha256));
            }
            fs::write(&options.hash_manifest_path, &hashes).map_err(|e| e.to_string())?;
            let manifest = sort_manifest(expected_manifest(&options)?);
            membership(root, set, &manifest)?;
            actual_wasm(metadata, &manifest)?;
            if recipe.as_ref().is_some_and(|r| r != &manifest.build_recipe) {
                return Err("candidate recipe differs across module sets".into());
            }
            recipe = Some(manifest.build_recipe.clone());
            let (identity_file, hash_file) = names(set);
            let mut identity = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
            identity.push(b'\n');
            fs::write(stage.join(&identity_file), &identity).map_err(|e| e.to_string())?;
            sets.insert(
                set.into(),
                CandidateSet {
                    identity_file,
                    identity_sha256: sha256_hex(&identity),
                    hash_file,
                    hash_sha256: sha256_hex(hashes.as_bytes()),
                    module_count: manifest.modules.len(),
                    build_manifest_hash: manifest.modules[0].build_manifest_hash.clone(),
                },
            );
        }
        let baseline = Descriptor {
            descriptor_schema_version: 1,
            purpose: PURPOSE.into(),
            source_input_rule_version: 2,
            receipt_schema_version: 3,
            identity_schema_version: 2,
            target: DEFAULT_WASM_TARGET.into(),
            profile: "release".into(),
            build_recipe: recipe.ok_or("missing recipe")?,
            frozen_release_sha256: legacy_map(),
            module_sets: sets,
        };
        let mut encoded = serde_json::to_vec_pretty(&baseline).map_err(|e| e.to_string())?;
        encoded.push(b'\n');
        fs::write(stage.join(DESCRIPTOR), encoded).map_err(|e| e.to_string())?;
        descriptor(&stage)?;
        fs::rename(&stage, output).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err(
            "candidate mode requires <workspace-root> <metadata-dir> <set-or-new-staging-dir>"
                .into(),
        );
    }
    let root = Path::new(&args[1]);
    let metadata = Path::new(&args[2]);
    match args[0].as_str() {
        "--candidate-baseline-check" => check(root, metadata, &args[3]),
        "--candidate-baseline-prepare" => prepare(root, metadata, Path::new(&args[3])),
        _ => Err("unknown candidate mode".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, Descriptor) {
        let dir = env::temp_dir().join(format!(
            "candidate-baseline-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let released: IdentityManifest = serde_json::from_slice(
            &fs::read(
                root.join(ARTIFACTS)
                    .join("m1_builtin_modules.identity.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut module_sets = BTreeMap::new();
        for (set, count) in [("m1", 10), ("m4", 22), ("m5", 5)] {
            let (identity_file, hash_file) = names(set);
            fs::write(dir.join(&identity_file), b"identity").unwrap();
            fs::write(dir.join(&hash_file), b"hash").unwrap();
            module_sets.insert(
                set.into(),
                CandidateSet {
                    identity_file,
                    hash_file,
                    identity_sha256: sha256_hex(b"identity"),
                    hash_sha256: sha256_hex(b"hash"),
                    module_count: count,
                    build_manifest_hash: "0".repeat(64),
                },
            );
        }
        let descriptor = Descriptor {
            descriptor_schema_version: 1,
            purpose: PURPOSE.into(),
            source_input_rule_version: 2,
            receipt_schema_version: 3,
            identity_schema_version: 2,
            target: DEFAULT_WASM_TARGET.into(),
            profile: "release".into(),
            build_recipe: released.build_recipe,
            frozen_release_sha256: legacy_map(),
            module_sets,
        };
        (dir, descriptor)
    }

    #[test]
    fn ci_truthy_markers_and_staging_ancestors_fail_before_writes() {
        for value in ["true", "1", "YES", "on", "unknown"] {
            assert!(ci_active(Some(value)));
        }
        for value in ["false", "0", "off", ""] {
            assert!(!ci_active(Some(value)));
        }
        assert!(!ci_active(None));
        let (dir, _) = fixture();
        let outside = dir.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("marker"), b"unchanged").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
            assert!(validate_staging_output(&dir.join("link/new-output")).is_err());
            assert!(!outside.join("new-output").exists());
        }
        assert!(validate_staging_output(&dir.join("outside/../escaped")).is_err());
        assert_eq!(fs::read(outside.join("marker")).unwrap(), b"unchanged");
        assert!(validate_staging_output(&dir.join("safe-new-output")).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn strict_descriptor_rejects_versions_paths_digests_and_duplicate_keys() {
        let (dir, baseline) = fixture();
        let original = serde_json::to_value(&baseline).unwrap();
        fs::write(dir.join(DESCRIPTOR), original.to_string()).unwrap();
        descriptor(&dir).unwrap();
        for (field, value) in [
            ("descriptor_schema_version", serde_json::json!(99)),
            ("purpose", serde_json::json!("release-build")),
            ("source_input_rule_version", serde_json::json!(1)),
            ("receipt_schema_version", serde_json::json!(2)),
            ("identity_schema_version", serde_json::json!(1)),
            ("profile", serde_json::json!("dev")),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut invalid = original.clone();
            invalid[field] = value;
            fs::write(dir.join(DESCRIPTOR), invalid.to_string()).unwrap();
            assert!(descriptor(&dir).is_err(), "{field}");
        }
        for field in [
            "identity_file",
            "hash_file",
            "identity_sha256",
            "hash_sha256",
        ] {
            let mut invalid = original.clone();
            invalid["module_sets"]["m1"][field] = serde_json::json!("../escape");
            fs::write(dir.join(DESCRIPTOR), invalid.to_string()).unwrap();
            assert!(descriptor(&dir).is_err(), "{field}");
        }
        for field in ["purpose", "module_sets", "m1"] {
            let value = if field == "m1" {
                &original["module_sets"][field]
            } else {
                &original[field]
            };
            let token = format!("\"{field}\":{value}");
            let raw = original
                .to_string()
                .replacen(&token, &format!("{token},{token}"), 1);
            fs::write(dir.join(DESCRIPTOR), raw).unwrap();
            assert!(descriptor(&dir).is_err(), "duplicate {field}");
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn frozen_release_and_staging_failure_cannot_rewrite_or_promote() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        frozen(&root).unwrap();
        let (dir, _) = fixture();
        let output = dir.join("must-not-promote");
        // No receipts: failure occurs before a staging directory or baseline is published.
        assert!(prepare(&root, &dir, &output).is_err());
        assert!(!output.exists());
        assert!(fs::read_dir(&dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("prepare-")
        }));
        fs::write(dir.join("unknown.module.build-receipt.json"), b"{}").unwrap();
        assert!(known_receipts(&root, &dir, false).is_err());
        // Frozen release checks cannot accept a freshly rehashed changed release file.
        let fake = dir.join("fake-root");
        fs::create_dir_all(fake.join(ARTIFACTS)).unwrap();
        for (name, _) in LEGACY {
            fs::copy(
                root.join(ARTIFACTS).join(name),
                fake.join(ARTIFACTS).join(name),
            )
            .unwrap();
        }
        frozen(&fake).unwrap();
        fs::write(fake.join(ARTIFACTS).join(LEGACY[0].0), b"changed").unwrap();
        assert!(frozen(&fake).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
