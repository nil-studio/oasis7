//! Host-side builtin WASM artifact fetching, compilation and verified caching.
//! The caller supplies the repository root; module admission remains in the runtime.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use oasis7_cas::{BlobStore, HashAlgorithm, LocalCasStore};
use sha2::{Digest, Sha256};

/// Materialization failures; the runtime maps these to its public error contract.
#[derive(Debug)]
pub enum MaterializeError {
    ModuleChangeInvalid { reason: String },
    Io(String),
    Store(oasis7_proto::world_error::WorldError),
}

impl From<std::io::Error> for MaterializeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
impl From<oasis7_proto::world_error::WorldError> for MaterializeError {
    fn from(error: oasis7_proto::world_error::WorldError) -> Self {
        Self::Store(error)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

const BUILTIN_WASM_ENV_PREFIX: &str = "OASIS7_BUILTIN_WASM_";

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_FETCH_TIMEOUT_MS: u64 = 1_500;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WASM_TOOLCHAIN: &str = "nightly-2025-12-11";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WASM_TARGET: &str = "wasm32-unknown-unknown";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WASM_BUILDER_IMAGE_REF: &str = "oasis7/wasm-builder:nightly-2025-12-11";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WASM_BUILDER_IMAGE_DIGEST: &str =
    "sha256:f3538a8039074c0b074946ae9d1803f6c74b823aee9152d52584111844ee1ed8";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_WASM_CANONICAL_CONTAINER_PLATFORM: &str = "linux-x86_64";
const BUILTIN_WASM_BUILD_PROFILE: &str = "release";
const M1_BUILTIN_MODULE_IDS_PATH: &str =
    "crates/oasis7_runtime/src/world/artifacts/m1_builtin_module_ids.txt";
const M4_BUILTIN_MODULE_IDS_PATH: &str =
    "crates/oasis7_runtime/src/world/artifacts/m4_builtin_module_ids.txt";
const M5_BUILTIN_MODULE_IDS_PATH: &str =
    "crates/oasis7_runtime/src/world/artifacts/m5_builtin_module_ids.txt";
const BUILTIN_MODULE_HASH_INDEX_PATH: &str = "module_hash_index.txt";

fn builtin_wasm_env_key(suffix: &str) -> String {
    format!("{BUILTIN_WASM_ENV_PREFIX}{suffix}")
}

pub fn builtin_wasm_env_non_empty(suffix: &str) -> Option<String> {
    env_non_empty(&builtin_wasm_env_key(suffix))
}

pub fn builtin_wasm_distfs_root(repo_root: &Path) -> PathBuf {
    if let Some(path) = builtin_wasm_env_non_empty("DISTFS_ROOT") {
        return PathBuf::from(path);
    }

    repo_root.join(".distfs").join("builtin_wasm")
}

pub fn load_builtin_wasm_with_fetch_fallback(
    module_id: &str,
    expected_hashes: &[&str],
    distfs_root: &Path,
    repo_root: &Path,
) -> Result<Vec<u8>, MaterializeError> {
    if expected_hashes.is_empty() {
        return Err(MaterializeError::ModuleChangeInvalid {
            reason: format!("builtin wasm expected hash list is empty module_id={module_id}"),
        });
    }

    let store = LocalCasStore::new_with_hash_algorithm(distfs_root, HashAlgorithm::Sha256);
    for expected_hash in expected_hashes {
        if let Ok(bytes) = store.get_verified(expected_hash) {
            return Ok(bytes);
        }
    }

    if let Some(cached_hash) =
        cached_expected_module_hash_for(module_id, expected_hashes, distfs_root)
        && let Ok(bytes) = store.get_verified(&cached_hash)
    {
        return Ok(bytes);
    }

    if let Some((actual_hash, fetched)) =
        try_fetch_builtin_wasm(module_id, expected_hashes, repo_root)?
    {
        store.put(&actual_hash, &fetched)?;
        let _ = persist_cached_module_hash(module_id, &actual_hash, distfs_root);
        return store
            .get_verified(&actual_hash)
            .map_err(MaterializeError::from);
    }

    let compiled = match compile_builtin_wasm(module_id, expected_hashes, repo_root) {
        Ok(compiled) => compiled,
        Err(compile_error) => {
            if let Some(cached_hash) = cached_module_hash_for(module_id, distfs_root)
                && let Ok(bytes) = store.get_verified(&cached_hash)
            {
                return Ok(bytes);
            }
            return Err(compile_error);
        }
    };
    let actual_hash = sha256_hex(&compiled);
    store.put(&actual_hash, &compiled)?;
    let _ = persist_cached_module_hash(module_id, &actual_hash, distfs_root);
    store
        .get_verified(&actual_hash)
        .map_err(MaterializeError::from)
}

fn try_fetch_builtin_wasm(
    module_id: &str,
    expected_hashes: &[&str],
    repo_root: &Path,
) -> Result<Option<(String, Vec<u8>)>, MaterializeError> {
    if let Some(fetched) = try_fetch_via_fetcher(module_id, expected_hashes, repo_root)? {
        return Ok(Some(fetched));
    }
    try_fetch_via_http(expected_hashes)
}

fn try_fetch_via_fetcher(
    module_id: &str,
    expected_hashes: &[&str],
    repo_root: &Path,
) -> Result<Option<(String, Vec<u8>)>, MaterializeError> {
    let Some(fetcher_path) = builtin_wasm_env_non_empty("FETCHER") else {
        return Ok(None);
    };
    let out_path = temp_artifact_path("fetched", module_id, repo_root);
    let Some(parent) = out_path.parent() else {
        return Ok(None);
    };
    fs::create_dir_all(parent)?;

    for expected_hash in expected_hashes {
        let status = match Command::new(&fetcher_path)
            .arg(module_id)
            .arg(expected_hash)
            .arg(&out_path)
            .status()
        {
            Ok(status) => status,
            Err(_) => return Ok(None),
        };
        if !status.success() {
            continue;
        }

        let bytes = match fs::read(&out_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let actual_hash = sha256_hex(&bytes);
        if is_expected_hash(expected_hashes, &actual_hash) {
            return Ok(Some((actual_hash, bytes)));
        }
    }
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
fn try_fetch_via_http(
    expected_hashes: &[&str],
) -> Result<Option<(String, Vec<u8>)>, MaterializeError> {
    let Some(fetch_urls) = builtin_wasm_env_non_empty("FETCH_URLS") else {
        return Ok(None);
    };
    let timeout = fetch_timeout();
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| MaterializeError::Io(error.to_string()))?;

    for base in fetch_urls
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        let trimmed = base.trim_end_matches('/');
        for expected_hash in expected_hashes {
            let candidates = [
                format!("{trimmed}/{expected_hash}.blob"),
                format!("{trimmed}/{expected_hash}"),
            ];
            for url in candidates {
                let Ok(response) = client.get(&url).send() else {
                    continue;
                };
                if !response.status().is_success() {
                    continue;
                }
                let Ok(bytes) = response.bytes() else {
                    continue;
                };
                let actual_hash = sha256_hex(bytes.as_ref());
                if is_expected_hash(expected_hashes, &actual_hash) {
                    return Ok(Some((actual_hash, bytes.to_vec())));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(target_arch = "wasm32")]
fn try_fetch_via_http(
    _expected_hashes: &[&str],
) -> Result<Option<(String, Vec<u8>)>, MaterializeError> {
    Ok(None)
}

fn compile_builtin_wasm(
    module_id: &str,
    expected_hashes: &[&str],
    repo_root: &Path,
) -> Result<Vec<u8>, MaterializeError> {
    if let Some(compiler_path) = builtin_wasm_env_non_empty("COMPILER") {
        return compile_via_command(
            Path::new(&compiler_path),
            module_id,
            expected_hashes,
            repo_root,
        );
    }
    compile_via_default_script(module_id, expected_hashes, repo_root)
}

fn compile_via_command(
    compiler_path: &Path,
    module_id: &str,
    expected_hashes: &[&str],
    repo_root: &Path,
) -> Result<Vec<u8>, MaterializeError> {
    let out_path = temp_artifact_path("compiled", module_id, repo_root);
    let Some(parent) = out_path.parent() else {
        return Err(MaterializeError::ModuleChangeInvalid {
            reason: "compiler output path has no parent".to_string(),
        });
    };
    fs::create_dir_all(parent)?;

    let mut failed_statuses = Vec::new();
    for expected_hash in expected_hashes {
        let status = Command::new(compiler_path)
            .arg(module_id)
            .arg(expected_hash)
            .arg(&out_path)
            .status()
            .map_err(|error| MaterializeError::ModuleChangeInvalid {
                reason: format!(
                    "failed to execute builtin wasm compiler={} err={error}",
                    compiler_path.display()
                ),
            })?;

        if !status.success() {
            failed_statuses.push(format!("{expected_hash}:{status}"));
            continue;
        }

        let bytes = fs::read(&out_path).map_err(|error| MaterializeError::ModuleChangeInvalid {
            reason: format!(
                "builtin wasm compiler output missing module_id={module_id} out={} err={error}",
                out_path.display()
            ),
        })?;

        return Ok(bytes);
    }

    Err(MaterializeError::ModuleChangeInvalid {
        reason: format!(
            "builtin wasm compiler exited non-zero for all expected hashes module_id={module_id} compiler={} expected_hashes=[{}] statuses=[{}]",
            compiler_path.display(),
            expected_hashes.join(","),
            failed_statuses.join(",")
        ),
    })
}

fn compile_via_default_script(
    module_id: &str,
    expected_hashes: &[&str],
    repo_root: &Path,
) -> Result<Vec<u8>, MaterializeError> {
    let build_script = repo_root
        .join("scripts")
        .join("build-builtin-wasm-modules.sh");
    let out_dir = temp_build_dir(module_id, repo_root);
    fs::create_dir_all(&out_dir)?;

    let artifact_path = out_dir.join(format!("{module_id}.wasm"));
    let prefer_host_native = should_prefer_host_native_builtin_wasm_build();
    run_default_builtin_wasm_build(
        &build_script,
        repo_root,
        module_id,
        &out_dir,
        prefer_host_native,
    )?;

    let mut bytes = read_compiled_artifact(module_id, &artifact_path)?;
    let mut actual_hash = sha256_hex(&bytes);
    if is_expected_hash(expected_hashes, &actual_hash) {
        let _ = fs::remove_dir_all(&out_dir);
        return Ok(bytes);
    }

    if prefer_host_native {
        let _ = fs::remove_dir_all(&out_dir);
        fs::create_dir_all(&out_dir)?;
        run_default_builtin_wasm_build(&build_script, repo_root, module_id, &out_dir, false)?;
        bytes = read_compiled_artifact(module_id, &artifact_path)?;
        actual_hash = sha256_hex(&bytes);
        if is_expected_hash(expected_hashes, &actual_hash) {
            let _ = fs::remove_dir_all(&out_dir);
            return Ok(bytes);
        }
    }

    let _ = fs::remove_dir_all(&out_dir);
    Err(MaterializeError::ModuleChangeInvalid {
        reason: format!(
            "fallback built artifact hash mismatch module_id={module_id} built_hash={actual_hash} expected_hashes=[{}]",
            expected_hashes.join(",")
        ),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn host_native_wasm_build_ready() -> bool {
    let toolchain = wasm_env_or_default("TOOLCHAIN", DEFAULT_WASM_TOOLCHAIN);
    let target = wasm_env_or_default("TARGET", DEFAULT_WASM_TARGET);

    let Ok(toolchains) = Command::new("rustup").arg("toolchain").arg("list").output() else {
        return false;
    };
    if !toolchains.status.success() {
        return false;
    }
    let toolchain_available = String::from_utf8_lossy(&toolchains.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .any(|line| {
            let candidate = line.split_whitespace().next().unwrap_or_default();
            candidate == toolchain || candidate.starts_with(&format!("{toolchain}-"))
        });
    if !toolchain_available {
        return false;
    }

    let Ok(targets) = Command::new("rustup")
        .arg("target")
        .arg("list")
        .arg("--toolchain")
        .arg(&toolchain)
        .arg("--installed")
        .output()
    else {
        return false;
    };
    if !targets.status.success() {
        return false;
    }

    String::from_utf8_lossy(&targets.stdout)
        .lines()
        .map(str::trim)
        .any(|line| line == target)
}

#[cfg(not(target_arch = "wasm32"))]
fn should_prefer_host_native_builtin_wasm_build() -> bool {
    !env_truthy("CI") && host_native_wasm_build_ready()
}

#[cfg(not(target_arch = "wasm32"))]
fn run_default_builtin_wasm_build(
    build_script: &Path,
    repo_root: &Path,
    module_id: &str,
    out_dir: &Path,
    prefer_host_native: bool,
) -> Result<(), MaterializeError> {
    let mut command = Command::new(build_script);
    // Tests may run under a versioned stable rustup alias (for example 1.96.0-...); fallback
    // build should pick the canonical wasm toolchain on its own.
    command.env_remove("RUSTUP_TOOLCHAIN");
    command.env_remove("OASIS7_WASM_BUILD_IN_CONTAINER");
    if prefer_host_native {
        command.env("OASIS7_WASM_BUILD_IN_CONTAINER", "1");
        command.env(
            "OASIS7_WASM_TOOLCHAIN",
            wasm_env_or_default("TOOLCHAIN", DEFAULT_WASM_TOOLCHAIN),
        );
        command.env(
            "OASIS7_WASM_TARGET",
            wasm_env_or_default("TARGET", DEFAULT_WASM_TARGET),
        );
        command.env(
            "OASIS7_WASM_BUILDER_IMAGE_REF",
            wasm_env_or_default("BUILDER_IMAGE_REF", DEFAULT_WASM_BUILDER_IMAGE_REF),
        );
        command.env(
            "OASIS7_WASM_BUILDER_IMAGE_DIGEST",
            wasm_env_or_default("BUILDER_IMAGE_DIGEST", DEFAULT_WASM_BUILDER_IMAGE_DIGEST),
        );
        command.env(
            "OASIS7_WASM_CANONICAL_CONTAINER_PLATFORM",
            wasm_env_or_default(
                "CANONICAL_CONTAINER_PLATFORM",
                DEFAULT_WASM_CANONICAL_CONTAINER_PLATFORM,
            ),
        );
    }
    command
        .arg("--module-id")
        .arg(module_id)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--profile")
        .arg(BUILTIN_WASM_BUILD_PROFILE);

    if let Some(module_ids_path) = builtin_module_ids_path_for(module_id, repo_root) {
        command.arg("--module-ids-path").arg(module_ids_path);
    }

    let status = command
        .status()
        .map_err(|error| MaterializeError::ModuleChangeInvalid {
            reason: format!(
                "failed to execute fallback build script={} err={error}",
                build_script.display()
            ),
        })?;

    if !status.success() {
        let mode = if prefer_host_native {
            "host-native"
        } else {
            "canonical-docker"
        };
        return Err(MaterializeError::ModuleChangeInvalid {
            reason: format!(
                "fallback build script exited non-zero script={} mode={} status={status}",
                build_script.display(),
                mode,
            ),
        });
    }

    Ok(())
}

fn read_compiled_artifact(
    module_id: &str,
    artifact_path: &Path,
) -> Result<Vec<u8>, MaterializeError> {
    fs::read(artifact_path).map_err(|error| MaterializeError::ModuleChangeInvalid {
        reason: format!(
            "fallback built artifact missing module_id={module_id} path={} err={error}",
            artifact_path.display()
        ),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn wasm_env_or_default(suffix: &str, default: &str) -> String {
    std::env::var(format!("OASIS7_WASM_{suffix}"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn builtin_module_ids_path_for(module_id: &str, repo_root: &Path) -> Option<PathBuf> {
    if module_id.starts_with("m1.") {
        return Some(repo_root.join(M1_BUILTIN_MODULE_IDS_PATH));
    }
    if module_id.starts_with("m4.") {
        return Some(repo_root.join(M4_BUILTIN_MODULE_IDS_PATH));
    }
    if module_id.starts_with("m5.") {
        return Some(repo_root.join(M5_BUILTIN_MODULE_IDS_PATH));
    }
    None
}

fn cached_module_hash_for(module_id: &str, distfs_root: &Path) -> Option<String> {
    let index = read_module_hash_index(distfs_root);
    index.get(module_id).cloned()
}

fn cached_expected_module_hash_for(
    module_id: &str,
    expected_hashes: &[&str],
    distfs_root: &Path,
) -> Option<String> {
    let cached_hash = cached_module_hash_for(module_id, distfs_root)?;
    is_expected_hash(expected_hashes, &cached_hash).then_some(cached_hash)
}

fn persist_cached_module_hash(
    module_id: &str,
    hash: &str,
    distfs_root: &Path,
) -> Result<(), MaterializeError> {
    if !is_sha256_hex(hash) {
        return Ok(());
    }

    let mut index = read_module_hash_index(distfs_root);
    index.insert(module_id.to_string(), hash.to_string());
    write_module_hash_index(distfs_root, &index)
}

fn read_module_hash_index(distfs_root: &Path) -> BTreeMap<String, String> {
    let index_path = distfs_root.join(BUILTIN_MODULE_HASH_INDEX_PATH);
    let Ok(content) = fs::read_to_string(index_path) else {
        return BTreeMap::new();
    };

    let mut index = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(module_id) = parts.next() else {
            continue;
        };
        let Some(hash) = parts.next() else {
            continue;
        };
        if module_id.is_empty() || !is_sha256_hex(hash) {
            continue;
        }
        index.insert(module_id.to_string(), hash.to_string());
    }
    index
}

fn write_module_hash_index(
    distfs_root: &Path,
    index: &BTreeMap<String, String>,
) -> Result<(), MaterializeError> {
    fs::create_dir_all(distfs_root)?;
    let index_path = distfs_root.join(BUILTIN_MODULE_HASH_INDEX_PATH);
    let mut content = String::new();
    for (module_id, hash) in index {
        content.push_str(module_id);
        content.push(' ');
        content.push_str(hash);
        content.push('\n');
    }
    fs::write(index_path, content)?;
    Ok(())
}

fn is_expected_hash(expected_hashes: &[&str], actual_hash: &str) -> bool {
    expected_hashes.contains(&actual_hash)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_timeout() -> Duration {
    let timeout_ms = builtin_wasm_env_non_empty("FETCH_TIMEOUT_MS")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_FETCH_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_truthy(key: &str) -> bool {
    matches!(
        std::env::var(key),
        Ok(value)
            if matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
    )
}

fn temp_artifact_path(kind: &str, module_id: &str, repo_root: &Path) -> PathBuf {
    temp_build_dir(module_id, repo_root).join(format!("{module_id}.{kind}.wasm"))
}

fn temp_build_dir(module_id: &str, repo_root: &Path) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    repo_root.join(".tmp").join(format!(
        "oasis7-builtin-wasm-{module_id}-{}-{now}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests;
