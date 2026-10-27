use oasis7_wasm_build::DEFAULT_WASM_TARGET;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Options {
    module_ids_path: PathBuf,
    module_manifest_map_path: PathBuf,
    hash_manifest_path: PathBuf,
    identity_manifest_path: PathBuf,
    metadata_dir: PathBuf,
    workspace_root: PathBuf,
    profile: String,
    canonical_platforms_csv: String,
    check_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildRecipe {
    profile: String,
    wasm_toolchain: String,
    wasm_target: String,
    wasm_build_std: String,
    wasm_build_std_components: String,
    wasm_build_std_features: String,
    wasm_deterministic_guard: String,
    canonicalizer_version: String,
    container_platform: String,
    builder_image_ref: String,
    builder_image_digest: String,
    build_suite_version: String,
    canonical_platforms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ModuleIdentityEntry {
    module_id: String,
    hash_tokens: Vec<String>,
    #[serde(default)]
    source_input_rule_version: Option<u32>,
    #[serde(default)]
    source_inputs: Option<Vec<oasis7_wasm_build::SourceInputRecord>>,
    source_hash: String,
    build_manifest_hash: String,
    identity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IdentityManifest {
    schema_version: u32,
    module_set: String,
    build_recipe: BuildRecipe,
    modules: Vec<ModuleIdentityEntry>,
}

#[derive(Debug, Deserialize)]
struct BuildReceipt {
    #[serde(default)]
    schema_version: Option<u32>,
    module_id: String,
    target: String,
    profile: String,
    wasm_toolchain: String,
    wasm_build_std: String,
    wasm_build_std_components: String,
    wasm_build_std_features: String,
    wasm_deterministic_guard: String,
    build_suite_version: String,
    #[serde(default)]
    source_input_rule_version: Option<u32>,
    #[serde(default)]
    source_inputs: Option<Vec<oasis7_wasm_build::SourceInputRecord>>,
    source_hash: String,
    build_manifest_hash: String,
    canonicalizer_version: String,
    container_platform: String,
    builder_image_ref: String,
    builder_image_digest: String,
    wasm_hash_sha256: String,
}

fn usage() -> &'static str {
    "Usage: cargo run -p oasis7_distfs --features sync-builtin-wasm-identity --bin sync_builtin_wasm_identity -- \\
  --module-ids-path <path> \\
  --module-manifest-map-path <path> \\
  --hash-manifest-path <path> \\
  --identity-manifest-path <path> \\
  --metadata-dir <dir> \\
  --workspace-root <dir> \\
  --profile <name> \\
  --canonical-platforms <csv> \\
  [--check]"
}

fn parse_args() -> Result<Options, String> {
    let mut module_ids_path: Option<PathBuf> = None;
    let mut module_manifest_map_path: Option<PathBuf> = None;
    let mut hash_manifest_path: Option<PathBuf> = None;
    let mut identity_manifest_path: Option<PathBuf> = None;
    let mut metadata_dir: Option<PathBuf> = None;
    let mut workspace_root: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut canonical_platforms_csv: Option<String> = None;
    let mut check_only = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--module-ids-path" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --module-ids-path".to_string());
                };
                module_ids_path = Some(PathBuf::from(value));
            }
            "--module-manifest-map-path" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --module-manifest-map-path".to_string());
                };
                module_manifest_map_path = Some(PathBuf::from(value));
            }
            "--hash-manifest-path" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --hash-manifest-path".to_string());
                };
                hash_manifest_path = Some(PathBuf::from(value));
            }
            "--identity-manifest-path" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --identity-manifest-path".to_string());
                };
                identity_manifest_path = Some(PathBuf::from(value));
            }
            "--metadata-dir" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --metadata-dir".to_string());
                };
                metadata_dir = Some(PathBuf::from(value));
            }
            "--workspace-root" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --workspace-root".to_string());
                };
                workspace_root = Some(PathBuf::from(value));
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --profile".to_string());
                };
                profile = Some(value);
            }
            "--canonical-platforms" => {
                let Some(value) = args.next() else {
                    return Err("missing value for --canonical-platforms".to_string());
                };
                canonical_platforms_csv = Some(value);
            }
            "--check" => {
                check_only = true;
            }
            "-h" | "--help" => {
                return Err(usage().to_string());
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
    }

    let Some(module_ids_path) = module_ids_path else {
        return Err("--module-ids-path is required".to_string());
    };
    let Some(module_manifest_map_path) = module_manifest_map_path else {
        return Err("--module-manifest-map-path is required".to_string());
    };
    let Some(hash_manifest_path) = hash_manifest_path else {
        return Err("--hash-manifest-path is required".to_string());
    };
    let Some(identity_manifest_path) = identity_manifest_path else {
        return Err("--identity-manifest-path is required".to_string());
    };
    let Some(metadata_dir) = metadata_dir else {
        return Err("--metadata-dir is required".to_string());
    };
    let Some(workspace_root) = workspace_root else {
        return Err("--workspace-root is required".to_string());
    };
    let Some(profile) = profile else {
        return Err("--profile is required".to_string());
    };
    let Some(canonical_platforms_csv) = canonical_platforms_csv else {
        return Err("--canonical-platforms is required".to_string());
    };

    Ok(Options {
        module_ids_path,
        module_manifest_map_path,
        hash_manifest_path,
        identity_manifest_path,
        metadata_dir,
        workspace_root,
        profile,
        canonical_platforms_csv,
        check_only,
    })
}

fn read_module_ids(path: &Path) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read module ids {}: {error}", path.display()))?;
    let ids: Vec<String> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    if ids.is_empty() {
        return Err(format!("module ids is empty: {}", path.display()));
    }
    Ok(ids)
}

fn read_module_manifest_map(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let content = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read module manifest map {}: {error}",
            path.display()
        )
    })?;

    let mut map = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(module_id) = parts.next() else {
            continue;
        };
        let Some(manifest_path) = parts.next() else {
            return Err(format!("invalid module manifest map line: {line}"));
        };
        map.insert(module_id.to_string(), manifest_path.to_string());
    }

    Ok(map)
}

fn read_hash_manifest(path: &Path) -> Result<BTreeMap<String, Vec<String>>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read hash manifest {}: {error}", path.display()))?;

    let mut map = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(module_id) = parts.next() else {
            continue;
        };
        let tokens: Vec<String> = parts.map(ToString::to_string).collect();
        if tokens.is_empty() {
            return Err(format!(
                "hash manifest line has no tokens for module_id={module_id}"
            ));
        }
        map.insert(module_id.to_string(), tokens);
    }

    Ok(map)
}

fn parse_hash_value(token: &str) -> &str {
    token.split_once('=').map(|(_, hash)| hash).unwrap_or(token)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn parse_canonical_platforms(raw_csv: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for raw in raw_csv.split(',') {
        let candidate = raw.trim();
        if candidate.is_empty() {
            continue;
        }
        if seen.insert(candidate.to_string()) {
            items.push(candidate.to_string());
        }
    }
    items
}

#[cfg(test)]
fn compute_source_hash(module_manifest_path: &Path) -> Result<String, String> {
    oasis7_wasm_build::compute_source_hash(module_manifest_path, DEFAULT_WASM_TARGET)
        .map_err(|error| format!("source hash closure failed: {error}"))
}

fn module_set_from_ids(module_ids: &[String]) -> String {
    let mut set = BTreeSet::new();
    for module_id in module_ids {
        let prefix = module_id.split('.').next().unwrap_or(module_id.as_str());
        set.insert(prefix.to_string());
    }

    if set.len() == 1 {
        set.into_iter()
            .next()
            .unwrap_or_else(|| "builtin".to_string())
    } else {
        "mixed".to_string()
    }
}

fn receipt_build_manifest_hash(receipt: &BuildReceipt) -> Result<String, String> {
    let manifest = oasis7_wasm_build::BuildManifest {
        source_input_rule_version: receipt.source_input_rule_version.unwrap_or_default(),
        profile: receipt.profile.clone(),
        target: receipt.target.clone(),
        wasm_toolchain: receipt.wasm_toolchain.clone(),
        wasm_build_std: receipt.wasm_build_std.clone(),
        wasm_build_std_components: receipt.wasm_build_std_components.clone(),
        wasm_build_std_features: receipt.wasm_build_std_features.clone(),
        wasm_deterministic_guard: receipt.wasm_deterministic_guard.clone(),
        canonicalizer_version: receipt.canonicalizer_version.clone(),
        container_platform: receipt.container_platform.clone(),
        builder_image_ref: receipt.builder_image_ref.clone(),
        builder_image_digest: receipt.builder_image_digest.clone(),
        build_suite_version: receipt.build_suite_version.clone(),
    };
    oasis7_wasm_build::compute_build_manifest_hash(&manifest).map_err(|error| error.to_string())
}

fn validate_receipt_source(
    receipt: &BuildReceipt,
    expected: &oasis7_wasm_build::SourceInputs,
) -> Result<(), String> {
    oasis7_wasm_build::validate_source_evidence(
        receipt.source_input_rule_version,
        receipt.source_inputs.as_deref(),
        &receipt.source_hash,
        expected,
    )
    .map_err(|e| e.to_string())?;
    if receipt.schema_version != Some(3) {
        return Err("legacy/unknown build receipt schema; canonical rebuild required".into());
    }
    let expected_build_hash = receipt_build_manifest_hash(receipt)?;
    if receipt.build_manifest_hash != expected_build_hash {
        return Err(format!(
            "build manifest hash does not bind current rule/tooling expected={expected_build_hash} actual={}",
            receipt.build_manifest_hash
        ));
    }
    if receipt.target != DEFAULT_WASM_TARGET {
        return Err("build receipt target mismatch".into());
    }
    Ok(())
}

fn expected_manifest(options: &Options) -> Result<IdentityManifest, String> {
    let module_ids = read_module_ids(&options.module_ids_path)?;
    let module_manifest_map = read_module_manifest_map(&options.module_manifest_map_path)?;
    let hash_manifest = read_hash_manifest(&options.hash_manifest_path)?;

    let canonical_platforms = parse_canonical_platforms(&options.canonical_platforms_csv);
    if canonical_platforms.is_empty() {
        return Err("canonical platforms list is empty".to_string());
    }

    let mut modules = Vec::new();
    let mut expected_build_recipe: Option<BuildRecipe> = None;
    let mut expected_build_manifest_hash: Option<String> = None;

    for module_id in &module_ids {
        let source_manifest_rel = module_manifest_map
            .get(module_id)
            .ok_or_else(|| format!("module manifest map missing module_id={module_id}"))?;

        let module_manifest_path = if Path::new(source_manifest_rel).is_absolute() {
            PathBuf::from(source_manifest_rel)
        } else {
            options.workspace_root.join(source_manifest_rel)
        };

        if !module_manifest_path.exists() {
            return Err(format!(
                "module manifest path not found module_id={} path={}",
                module_id,
                module_manifest_path.display()
            ));
        }

        let hash_tokens = hash_manifest
            .get(module_id)
            .cloned()
            .ok_or_else(|| format!("hash manifest missing module_id={module_id}"))?;

        for token in &hash_tokens {
            let hash_value = parse_hash_value(token);
            if !is_sha256_hex(hash_value) {
                return Err(format!(
                    "invalid hash token module_id={} token={}",
                    module_id, token
                ));
            }
        }

        let receipt_path = options
            .metadata_dir
            .join(format!("{module_id}.build-receipt.json"));
        let receipt_content = fs::read(&receipt_path).map_err(|error| {
            format!(
                "failed to read build receipt module_id={} path={} err={error}",
                module_id,
                receipt_path.display()
            )
        })?;

        let receipt: BuildReceipt = serde_json::from_slice(&receipt_content).map_err(|error| {
            format!(
                "failed to parse build receipt module_id={} path={} err={error}",
                module_id,
                receipt_path.display()
            )
        })?;

        if receipt.module_id != *module_id {
            return Err(format!(
                "build receipt module id mismatch expected={} actual={} path={}",
                module_id,
                receipt.module_id,
                receipt_path.display()
            ));
        }
        if receipt.profile != options.profile {
            return Err(format!(
                "build receipt profile mismatch module_id={} expected={} actual={} path={}",
                module_id,
                options.profile,
                receipt.profile,
                receipt_path.display()
            ));
        }

        if !hash_tokens
            .iter()
            .map(|token| parse_hash_value(token))
            .any(|value| value == receipt.wasm_hash_sha256)
        {
            return Err(format!(
                "build receipt hash missing from hash manifest module_id={} built_hash={} hash_tokens=[{}]",
                module_id,
                receipt.wasm_hash_sha256,
                hash_tokens.join(",")
            ));
        }

        let source_evidence =
            oasis7_wasm_build::compute_source_inputs(&module_manifest_path, DEFAULT_WASM_TARGET)
                .map_err(|e| e.to_string())?;
        validate_receipt_source(&receipt, &source_evidence)
            .map_err(|e| format!("build receipt module_id={module_id}: {e}"))?;
        let source_hash = source_evidence.source_hash;
        if source_hash != receipt.source_hash {
            return Err(format!(
                "build receipt source_hash mismatch module_id={} expected={} actual={} path={}",
                module_id,
                source_hash,
                receipt.source_hash,
                receipt_path.display()
            ));
        }
        if !canonical_platforms
            .iter()
            .any(|platform| platform == &receipt.container_platform)
        {
            return Err(format!(
                "build receipt container platform is not canonical module_id={} platform={} canonical_platforms=[{}]",
                module_id,
                receipt.container_platform,
                canonical_platforms.join(",")
            ));
        }

        let current_build_recipe = BuildRecipe {
            profile: receipt.profile.clone(),
            wasm_toolchain: receipt.wasm_toolchain.clone(),
            wasm_target: receipt.target.clone(),
            wasm_build_std: receipt.wasm_build_std.clone(),
            wasm_build_std_components: receipt.wasm_build_std_components.clone(),
            wasm_build_std_features: receipt.wasm_build_std_features.clone(),
            wasm_deterministic_guard: receipt.wasm_deterministic_guard.clone(),
            canonicalizer_version: receipt.canonicalizer_version.clone(),
            container_platform: receipt.container_platform.clone(),
            builder_image_ref: receipt.builder_image_ref.clone(),
            builder_image_digest: receipt.builder_image_digest.clone(),
            build_suite_version: receipt.build_suite_version.clone(),
            canonical_platforms: canonical_platforms.clone(),
        };

        match &expected_build_recipe {
            Some(existing) if existing != &current_build_recipe => {
                return Err(format!(
                    "build recipe mismatch across modules baseline_module={} current_module={}",
                    modules
                        .first()
                        .map(|entry: &ModuleIdentityEntry| entry.module_id.as_str())
                        .unwrap_or("unknown"),
                    module_id
                ));
            }
            None => expected_build_recipe = Some(current_build_recipe),
            _ => {}
        }

        match &expected_build_manifest_hash {
            Some(existing) if existing != &receipt.build_manifest_hash => {
                return Err(format!(
                    "build_manifest_hash mismatch across modules baseline={} current={} module_id={}",
                    existing, receipt.build_manifest_hash, module_id
                ));
            }
            None => expected_build_manifest_hash = Some(receipt.build_manifest_hash.clone()),
            _ => {}
        }

        let identity_hash = sha256_hex(
            format!("{module_id}:{source_hash}:{}", receipt.build_manifest_hash).as_bytes(),
        );

        modules.push(ModuleIdentityEntry {
            module_id: module_id.clone(),
            hash_tokens,
            source_input_rule_version: Some(source_evidence.source_input_rule_version),
            source_inputs: Some(source_evidence.source_inputs),
            source_hash,
            build_manifest_hash: receipt.build_manifest_hash,
            identity_hash,
        });
    }

    let build_recipe = expected_build_recipe
        .ok_or_else(|| "failed to derive build recipe from receipts".to_string())?;

    Ok(IdentityManifest {
        schema_version: 2,
        module_set: module_set_from_ids(&module_ids),
        build_recipe,
        modules,
    })
}

fn sort_manifest(mut manifest: IdentityManifest) -> IdentityManifest {
    manifest
        .modules
        .sort_by(|left, right| left.module_id.cmp(&right.module_id));
    manifest
}

fn diff_manifest(expected: &IdentityManifest, actual: &IdentityManifest) -> String {
    if expected.schema_version != actual.schema_version {
        return format!(
            "schema_version mismatch expected={} actual={}",
            expected.schema_version, actual.schema_version
        );
    }

    if expected.module_set != actual.module_set {
        return format!(
            "module_set mismatch expected={} actual={}",
            expected.module_set, actual.module_set
        );
    }

    if expected.build_recipe != actual.build_recipe {
        return "build_recipe mismatch".to_string();
    }

    if expected.modules.len() != actual.modules.len() {
        return format!(
            "module entry count mismatch expected={} actual={}",
            expected.modules.len(),
            actual.modules.len()
        );
    }

    for (expected_entry, actual_entry) in expected.modules.iter().zip(actual.modules.iter()) {
        if expected_entry.module_id != actual_entry.module_id {
            return format!(
                "module_id mismatch expected={} actual={}",
                expected_entry.module_id, actual_entry.module_id
            );
        }
        if expected_entry.hash_tokens != actual_entry.hash_tokens {
            return format!(
                "hash_tokens mismatch module_id={} expected=[{}] actual=[{}]",
                expected_entry.module_id,
                expected_entry.hash_tokens.join(","),
                actual_entry.hash_tokens.join(",")
            );
        }
        if expected_entry.source_input_rule_version != actual_entry.source_input_rule_version
            || expected_entry.source_inputs != actual_entry.source_inputs
        {
            return format!(
                "source input version/details mismatch module_id={}; canonical rebuild required",
                expected_entry.module_id
            );
        }
        if expected_entry.source_hash != actual_entry.source_hash {
            return format!(
                "source_hash mismatch module_id={} expected={} actual={}",
                expected_entry.module_id, expected_entry.source_hash, actual_entry.source_hash
            );
        }
        if expected_entry.build_manifest_hash != actual_entry.build_manifest_hash {
            return format!(
                "build_manifest_hash mismatch module_id={} expected={} actual={}",
                expected_entry.module_id,
                expected_entry.build_manifest_hash,
                actual_entry.build_manifest_hash
            );
        }
        if expected_entry.identity_hash != actual_entry.identity_hash {
            return format!(
                "identity_hash mismatch module_id={} expected={} actual={}",
                expected_entry.module_id, expected_entry.identity_hash, actual_entry.identity_hash
            );
        }
    }

    "unknown mismatch".to_string()
}

#[path = "sync_builtin_wasm_identity/candidate.rs"]
mod candidate;

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg.starts_with("--candidate-baseline-"))
    {
        return candidate::run(&args);
    }
    let options = parse_args()?;
    let expected = sort_manifest(expected_manifest(&options)?);

    if options.check_only {
        let manifest_bytes = fs::read(&options.identity_manifest_path).map_err(|error| {
            format!(
                "failed to read identity manifest {}: {error}",
                options.identity_manifest_path.display()
            )
        })?;
        let actual: IdentityManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                format!(
                    "failed to parse identity manifest {}: {error}",
                    options.identity_manifest_path.display()
                )
            })?;

        let actual = sort_manifest(actual);

        if expected != actual {
            return Err(format!(
                "identity manifest mismatch: {}",
                diff_manifest(&expected, &actual)
            ));
        }

        println!(
            "check ok: identity manifest is in sync module_count={} path={}",
            expected.modules.len(),
            options.identity_manifest_path.display()
        );
        return Ok(());
    }

    let payload = serde_json::to_vec_pretty(&expected)
        .map_err(|error| format!("failed to serialize identity manifest: {error}"))?;

    if let Some(parent) = options.identity_manifest_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create identity manifest dir {}: {error}",
                parent.display()
            )
        })?;
    }

    let mut with_newline = payload;
    with_newline.push(b'\n');
    fs::write(&options.identity_manifest_path, &with_newline).map_err(|error| {
        format!(
            "failed to write identity manifest {}: {error}",
            options.identity_manifest_path.display()
        )
    })?;

    println!(
        "synced builtin wasm identity manifest module_count={} path={}",
        expected.modules.len(),
        options.identity_manifest_path.display()
    );

    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        eprintln!("{}", usage());
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, contents).expect("write file");
    }

    #[test]
    fn sync_source_hash_tracks_local_path_dependency_changes() {
        let root = unique_temp_dir("oasis7-distfs-source-hash");
        write_file(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"module\", \"shared\"]\nresolver = \"2\"\n",
        );
        write_file(
            &root.join("module/Cargo.toml"),
            "[package]\nname = \"fixture_module\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nfixture_shared = { path = \"../shared\" }\n",
        );
        write_file(
            &root.join("module/src/lib.rs"),
            "pub fn call() -> u32 { fixture_shared::value() }\n",
        );
        write_file(
            &root.join("shared/Cargo.toml"),
            "[package]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        write_file(
            &root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        );

        let manifest_path = root.join("module/Cargo.toml");
        let initial = compute_source_hash(&manifest_path).expect("initial source hash");
        write_file(
            &root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 2 }\n",
        );
        let changed = compute_source_hash(&manifest_path).expect("changed source hash");
        assert_ne!(initial, changed);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn identity_consumer_rejects_legacy_version_patches_and_stale_inputs() {
        let root = unique_temp_dir("identity-source-rule-v2");
        write_file(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers=[\"module\"]\nresolver=\"2\"\n",
        );
        write_file(
            &root.join("module/Cargo.toml"),
            "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        );
        write_file(&root.join("module/src/lib.rs"), "pub fn value(){}\n");
        let evidence = oasis7_wasm_build::compute_source_inputs(
            &root.join("module/Cargo.toml"),
            DEFAULT_WASM_TARGET,
        )
        .unwrap();
        write_file(&root.join("ids"), "fixture.module\n");
        write_file(&root.join("map"), "fixture.module module/Cargo.toml\n");
        write_file(
            &root.join("hashes"),
            "fixture.module linux-x86_64=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        );
        let options = Options {
            module_ids_path: root.join("ids"),
            module_manifest_map_path: root.join("map"),
            hash_manifest_path: root.join("hashes"),
            identity_manifest_path: root.join("identity.json"),
            metadata_dir: root.join("metadata"),
            workspace_root: root.clone(),
            profile: "release".into(),
            canonical_platforms_csv: "linux-x86_64".into(),
            check_only: true,
        };
        let mut receipt = serde_json::json!({"schema_version":3,"module_id":"fixture.module","target":DEFAULT_WASM_TARGET,"profile":"release","wasm_toolchain":"fixture","wasm_build_std":"0","wasm_build_std_components":"","wasm_build_std_features":"","wasm_deterministic_guard":"1","build_suite_version":"fixture","source_input_rule_version":2,"source_inputs":evidence.source_inputs,"source_hash":evidence.source_hash,"build_manifest_hash":"build-hash","canonicalizer_version":"fixture","container_platform":"linux-x86_64","builder_image_ref":"fixture","builder_image_digest":"fixture","wasm_hash_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"});
        let decoded: BuildReceipt = serde_json::from_value(receipt.clone()).unwrap();
        receipt["build_manifest_hash"] = receipt_build_manifest_hash(&decoded).unwrap().into();
        let path = root.join("metadata/fixture.module.build-receipt.json");
        let set = |value: &serde_json::Value| write_file(&path, &value.to_string());
        set(&receipt);
        let identity = expected_manifest(&options).unwrap();
        assert_eq!(identity.schema_version, 2);
        assert_eq!(identity.modules[0].source_input_rule_version, Some(2));
        assert_eq!(
            identity.modules[0].source_inputs.as_ref(),
            Some(&evidence.source_inputs)
        );
        let mut legacy = receipt.clone();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("source_input_rule_version");
        legacy.as_object_mut().unwrap().remove("source_inputs");
        legacy["schema_version"] = 2.into();
        set(&legacy);
        assert!(expected_manifest(&options).unwrap_err().contains("legacy"));
        legacy["source_input_rule_version"] = 2.into();
        set(&legacy);
        assert!(
            expected_manifest(&options)
                .unwrap_err()
                .contains("details mismatch")
        );
        let mut unknown = receipt.clone();
        unknown["source_input_rule_version"] = 99.into();
        set(&unknown);
        assert!(
            expected_manifest(&options)
                .unwrap_err()
                .contains("unknown/mismatched")
        );
        let mut schema = receipt.clone();
        schema["schema_version"] = 99.into();
        set(&schema);
        assert!(
            expected_manifest(&options)
                .unwrap_err()
                .contains("unknown build receipt schema")
        );
        let mut wasm = receipt.clone();
        wasm["wasm_hash_sha256"] = "other-wasm".into();
        set(&wasm);
        assert!(
            expected_manifest(&options)
                .unwrap_err()
                .contains("hash missing from hash manifest")
        );
        set(&receipt);
        write_file(
            &root.join("module/src/lib.rs"),
            "pub fn value(){let _=1;}\n",
        );
        assert!(
            expected_manifest(&options)
                .unwrap_err()
                .contains("source input details mismatch")
        );
        write_file(&root.join("module/src/lib.rs"), "pub fn value(){}\n");
        let keys = [
            "profile",
            "target",
            "wasm_toolchain",
            "wasm_build_std",
            "wasm_build_std_components",
            "wasm_build_std_features",
            "wasm_deterministic_guard",
            "canonicalizer_version",
            "container_platform",
            "builder_image_ref",
            "builder_image_digest",
            "build_suite_version",
        ];
        let old_payload = format!(
            "{{{}}}",
            keys.iter()
                .map(|key| format!("{}:{}", serde_json::to_string(key).unwrap(), receipt[*key]))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut old_build = receipt.clone();
        old_build["build_manifest_hash"] = sha256_hex(old_payload.as_bytes()).into();
        set(&old_build);
        assert!(
            expected_manifest(&options).is_err(),
            "v2 source fields must not accept an unchanged legacy build-manifest hash"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
