#[cfg(test)]
mod artifact_pipeline_contract_tests;

#[cfg(test)]
mod builder_context_tests;

mod build_timing;
mod build_util;
#[cfg(test)]
mod env_prefix_tests;

pub use build_timing::BuildTimingSnapshot;

use build_util::{canonical_or_original, elapsed_ms, normalize_artifact_name, now_unix_ms};
#[cfg(test)]
use oasis7_wasm_build::DEFAULT_WASM_TARGET;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use wasm_encoder::{Module, RawSection};
use wasmparser::Parser;

pub const DEFAULT_TARGET: &str = "wasm32-unknown-unknown";
pub const DEFAULT_PROFILE: &str = "release";
pub const DEFAULT_OUT_DIR: &str = ".tmp/wasm-build-suite";
pub const DEFAULT_CANONICALIZER_VERSION: &str = "strip-custom-sections-v1";
pub const DEFAULT_CONTAINER_PLATFORM: &str = "linux-x86_64";
const WASM_ENV_PREFIX: &str = "OASIS7_WASM_";
const BUILD_RECEIPT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRequest {
    pub module_id: String,
    pub manifest_path: PathBuf,
    pub out_dir: PathBuf,
    pub target: String,
    pub profile: String,
    pub dry_run: bool,
}

impl BuildRequest {
    pub fn with_defaults(module_id: impl Into<String>, manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            module_id: module_id.into(),
            manifest_path: manifest_path.into(),
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            target: DEFAULT_TARGET.to_string(),
            profile: DEFAULT_PROFILE.to_string(),
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub recorded_at_unix_ms: i64,
    pub module_id: String,
    pub target: String,
    pub profile: String,
    pub wasm_toolchain: String,
    pub wasm_build_std: String,
    pub wasm_build_std_components: String,
    pub wasm_build_std_features: String,
    pub wasm_deterministic_guard: String,
    pub source_manifest_path: String,
    pub source_artifact_path: String,
    pub packaged_wasm_path: String,
    pub build_receipt_path: String,
    pub source_hash: String,
    pub build_manifest_hash: String,
    pub canonicalizer_version: String,
    pub container_platform: String,
    pub builder_image_ref: String,
    pub builder_image_digest: String,
    pub build_timing: BuildTimingSnapshot,
    pub wasm_hash_sha256: String,
    pub wasm_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildReceipt {
    pub schema_version: u32,
    pub recorded_at_unix_ms: i64,
    pub module_id: String,
    pub target: String,
    pub profile: String,
    pub wasm_toolchain: String,
    pub wasm_build_std: String,
    pub wasm_build_std_components: String,
    pub wasm_build_std_features: String,
    pub wasm_deterministic_guard: String,
    pub build_suite_version: String,
    pub source_manifest_path: String,
    pub source_artifact_path: String,
    pub packaged_wasm_path: String,
    pub source_hash: String,
    pub build_manifest_hash: String,
    pub canonicalizer_version: String,
    pub container_platform: String,
    pub builder_image_ref: String,
    pub builder_image_digest: String,
    pub build_timing: BuildTimingSnapshot,
    pub wasm_hash_sha256: String,
    pub wasm_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    pub module_id: String,
    pub source_artifact_path: PathBuf,
    pub packaged_wasm_path: PathBuf,
    pub metadata_path: PathBuf,
    pub receipt_path: PathBuf,
    pub wasm_hash_sha256: Option<String>,
    pub source_hash: Option<String>,
    pub build_manifest_hash: Option<String>,
    pub build_timing: Option<BuildTimingSnapshot>,
    pub wasm_size_bytes: Option<u64>,
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum BuildError {
    InvalidArgument(String),
    CommandFailed {
        program: String,
        args: Vec<String>,
        status_code: Option<i32>,
        stderr: String,
    },
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    Json {
        source: serde_json::Error,
        context: String,
    },
    ManifestNotFound(PathBuf),
    MetadataInvalid(String),
    ArtifactNotFound(PathBuf),
    WasmTransform {
        context: String,
        source: wasmparser::BinaryReaderError,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::InvalidArgument(msg) => write!(f, "invalid argument: {msg}"),
            BuildError::CommandFailed {
                program,
                args,
                status_code,
                stderr,
            } => {
                write!(
                    f,
                    "command failed: {} {} (status={:?}){}",
                    program,
                    args.join(" "),
                    status_code,
                    if stderr.is_empty() {
                        String::new()
                    } else {
                        format!(", stderr={stderr}")
                    }
                )
            }
            BuildError::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "io error at {}: {}", path.display(), source)
                } else {
                    write!(f, "io error: {source}")
                }
            }
            BuildError::Json { source, context } => {
                write!(f, "json error ({context}): {source}")
            }
            BuildError::ManifestNotFound(path) => {
                write!(f, "manifest not found: {}", path.display())
            }
            BuildError::MetadataInvalid(msg) => write!(f, "cargo metadata invalid: {msg}"),
            BuildError::ArtifactNotFound(path) => {
                write!(f, "wasm artifact not found: {}", path.display())
            }
            BuildError::WasmTransform { context, source } => {
                write!(f, "wasm transform error ({context}): {source}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[serde(default)]
    workspace_root: String,
    packages: Vec<CargoPackage>,
    target_directory: String,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: Option<String>,
    manifest_path: String,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildStdConfig {
    enabled: bool,
    components: String,
    features: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BuildManifest {
    profile: String,
    target: String,
    wasm_toolchain: String,
    wasm_build_std: String,
    wasm_build_std_components: String,
    wasm_build_std_features: String,
    wasm_deterministic_guard: String,
    canonicalizer_version: String,
    container_platform: String,
    builder_image_ref: String,
    builder_image_digest: String,
    build_suite_version: String,
}

impl BuildStdConfig {
    fn from_env() -> Self {
        let enabled = wasm_env_value("BUILD_STD")
            .map(|value| parse_truthy(value.as_str()))
            .unwrap_or(false);
        let components = wasm_env_value_or_default("BUILD_STD_COMPONENTS", "std,panic_abort");
        let features = wasm_env_value_or_default("BUILD_STD_FEATURES", "");
        Self {
            enabled,
            components,
            features,
        }
    }

    fn cargo_unstable_args(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }

        let mut args = Vec::new();
        if !self.components.trim().is_empty() {
            args.push("-Z".to_string());
            args.push(format!("build-std={}", self.components.trim()));
        }
        if !self.features.trim().is_empty() {
            args.push("-Z".to_string());
            args.push(format!("build-std-features={}", self.features.trim()));
        }
        args
    }
}

pub fn run_build(request: &BuildRequest) -> Result<BuildOutput, BuildError> {
    validate_request(request)?;
    let total_started = Instant::now();
    let manifest_path = canonical_or_original(&request.manifest_path);
    if !manifest_path.exists() {
        return Err(BuildError::ManifestNotFound(manifest_path));
    }

    let metadata = read_cargo_metadata(&manifest_path)?;
    validate_workspace_compile_time_determinism(&metadata, &manifest_path)?;
    let package = find_package_for_manifest(&metadata, &manifest_path)?;
    let target_name = find_wasm_target_name(package)?;
    let artifact_path = resolve_artifact_path(
        &metadata,
        request.target.as_str(),
        request.profile.as_str(),
        target_name.as_str(),
    );

    let packaged_wasm_path = request
        .out_dir
        .join(format!("{}.wasm", request.module_id.as_str()));
    let metadata_path = request
        .out_dir
        .join(format!("{}.metadata.json", request.module_id.as_str()));
    let receipt_path = request
        .out_dir
        .join(format!("{}.build-receipt.json", request.module_id.as_str()));

    if request.dry_run {
        return Ok(BuildOutput {
            module_id: request.module_id.clone(),
            source_artifact_path: artifact_path,
            packaged_wasm_path,
            metadata_path,
            receipt_path,
            wasm_hash_sha256: None,
            source_hash: None,
            build_manifest_hash: None,
            build_timing: None,
            wasm_size_bytes: None,
            dry_run: true,
        });
    }

    let cargo_started = Instant::now();
    run_cargo_build(
        manifest_path.as_path(),
        request.target.as_str(),
        request.profile.as_str(),
    )?;
    let cargo_build_ms = elapsed_ms(cargo_started);

    if !artifact_path.exists() {
        return Err(BuildError::ArtifactNotFound(artifact_path));
    }
    let wasm_bytes = fs::read(&artifact_path).map_err(|source| BuildError::Io {
        path: Some(artifact_path.clone()),
        source,
    })?;
    let canonicalize_started = Instant::now();
    let canonical_wasm_bytes = canonicalize_wasm_bytes(&wasm_bytes)?;
    let canonicalize_ms = elapsed_ms(canonicalize_started);
    let hash_started = Instant::now();
    let wasm_size_bytes = u64::try_from(canonical_wasm_bytes.len()).map_err(|_| {
        BuildError::MetadataInvalid("wasm size overflow while converting usize to u64".to_string())
    })?;
    let wasm_hash_sha256 = sha256_hex(&canonical_wasm_bytes);
    let source_manifest_path = manifest_path.to_string_lossy().to_string();
    let source_hash = compute_source_hash(&manifest_path, request.target.as_str())?;
    let wasm_toolchain = wasm_env_value_or_default("TOOLCHAIN", "");
    let wasm_build_std = wasm_env_value_or_default("BUILD_STD", "0");
    let wasm_build_std_components =
        wasm_env_value_or_default("BUILD_STD_COMPONENTS", "std,panic_abort");
    let wasm_build_std_features = wasm_env_value_or_default("BUILD_STD_FEATURES", "");
    let wasm_deterministic_guard = wasm_env_value_or_default("DETERMINISTIC_GUARD", "1");
    let canonicalizer_version =
        wasm_env_value_or_default("CANONICALIZER_VERSION", DEFAULT_CANONICALIZER_VERSION);
    let container_platform =
        wasm_env_value_or_default("CANONICAL_CONTAINER_PLATFORM", DEFAULT_CONTAINER_PLATFORM);
    let builder_image_ref = wasm_env_value_or_default("BUILDER_IMAGE_REF", "");
    let builder_image_digest = wasm_env_value_or_default("BUILDER_IMAGE_DIGEST", "");
    let build_manifest_hash = compute_build_manifest_hash(
        request.profile.as_str(),
        request.target.as_str(),
        wasm_toolchain.as_str(),
        wasm_build_std.as_str(),
        wasm_build_std_components.as_str(),
        wasm_build_std_features.as_str(),
        wasm_deterministic_guard.as_str(),
        canonicalizer_version.as_str(),
        container_platform.as_str(),
        builder_image_ref.as_str(),
        builder_image_digest.as_str(),
    )?;
    let hash_ms = elapsed_ms(hash_started);

    if let Some(parent) = packaged_wasm_path.parent() {
        fs::create_dir_all(parent).map_err(|source| BuildError::Io {
            path: Some(parent.to_path_buf()),
            source,
        })?;
    }

    fs::write(&packaged_wasm_path, &canonical_wasm_bytes).map_err(|source| BuildError::Io {
        path: Some(packaged_wasm_path.clone()),
        source,
    })?;

    let recorded_at_unix_ms = now_unix_ms();
    let build_timing = BuildTimingSnapshot {
        total_build_wall_ms: 0,
        cargo_build_ms,
        canonicalize_ms,
        hash_ms,
        receipt_write_ms: 0,
        metadata_write_ms: 0,
    };

    let receipt_started = Instant::now();
    let receipt_payload = BuildReceipt {
        schema_version: BUILD_RECEIPT_SCHEMA_VERSION,
        recorded_at_unix_ms,
        module_id: request.module_id.clone(),
        target: request.target.clone(),
        profile: request.profile.clone(),
        wasm_toolchain: wasm_toolchain.clone(),
        wasm_build_std: wasm_build_std.clone(),
        wasm_build_std_components: wasm_build_std_components.clone(),
        wasm_build_std_features: wasm_build_std_features.clone(),
        wasm_deterministic_guard: wasm_deterministic_guard.clone(),
        build_suite_version: env!("CARGO_PKG_VERSION").to_string(),
        source_manifest_path: source_manifest_path.clone(),
        source_artifact_path: artifact_path.to_string_lossy().to_string(),
        packaged_wasm_path: packaged_wasm_path.to_string_lossy().to_string(),
        source_hash: source_hash.clone(),
        build_manifest_hash: build_manifest_hash.clone(),
        canonicalizer_version: canonicalizer_version.clone(),
        container_platform: container_platform.clone(),
        builder_image_ref: builder_image_ref.clone(),
        builder_image_digest: builder_image_digest.clone(),
        build_timing: build_timing.clone(),
        wasm_hash_sha256: wasm_hash_sha256.clone(),
        wasm_size_bytes,
    };
    let receipt_json =
        serde_json::to_vec_pretty(&receipt_payload).map_err(|source| BuildError::Json {
            source,
            context: "serialize build receipt".to_string(),
        })?;
    fs::write(&receipt_path, receipt_json).map_err(|source| BuildError::Io {
        path: Some(receipt_path.clone()),
        source,
    })?;
    let receipt_write_ms = elapsed_ms(receipt_started);

    let metadata_started = Instant::now();
    let metadata_payload = BuildMetadata {
        recorded_at_unix_ms,
        module_id: request.module_id.clone(),
        target: request.target.clone(),
        profile: request.profile.clone(),
        wasm_toolchain,
        wasm_build_std,
        wasm_build_std_components,
        wasm_build_std_features,
        wasm_deterministic_guard,
        source_manifest_path,
        source_artifact_path: artifact_path.to_string_lossy().to_string(),
        packaged_wasm_path: packaged_wasm_path.to_string_lossy().to_string(),
        build_receipt_path: receipt_path.to_string_lossy().to_string(),
        source_hash: source_hash.clone(),
        build_manifest_hash: build_manifest_hash.clone(),
        canonicalizer_version,
        container_platform,
        builder_image_ref,
        builder_image_digest,
        build_timing: BuildTimingSnapshot {
            total_build_wall_ms: 0,
            cargo_build_ms,
            canonicalize_ms,
            hash_ms,
            receipt_write_ms,
            metadata_write_ms: 0,
        },
        wasm_hash_sha256: wasm_hash_sha256.clone(),
        wasm_size_bytes,
    };

    let metadata_json =
        serde_json::to_vec_pretty(&metadata_payload).map_err(|source| BuildError::Json {
            source,
            context: "serialize build metadata".to_string(),
        })?;
    fs::write(&metadata_path, metadata_json).map_err(|source| BuildError::Io {
        path: Some(metadata_path.clone()),
        source,
    })?;
    let metadata_write_ms = elapsed_ms(metadata_started);
    // Persisted timing must include the final rewrite pass as well. We budget
    // that pass using the first measured write durations, then return the
    // exact end-to-end timing in BuildOutput after the rewrites complete.
    let persisted_build_timing = BuildTimingSnapshot {
        total_build_wall_ms: elapsed_ms(total_started)
            .saturating_add(receipt_write_ms)
            .saturating_add(metadata_write_ms),
        cargo_build_ms,
        canonicalize_ms,
        hash_ms,
        receipt_write_ms: receipt_write_ms.saturating_mul(2),
        metadata_write_ms: metadata_write_ms.saturating_mul(2),
    };
    let final_receipt_payload = BuildReceipt {
        build_timing: persisted_build_timing.clone(),
        ..receipt_payload
    };
    let final_receipt_started = Instant::now();
    let final_receipt_json =
        serde_json::to_vec_pretty(&final_receipt_payload).map_err(|source| BuildError::Json {
            source,
            context: "serialize final build receipt".to_string(),
        })?;
    fs::write(&receipt_path, final_receipt_json).map_err(|source| BuildError::Io {
        path: Some(receipt_path.clone()),
        source,
    })?;
    let final_receipt_write_ms = elapsed_ms(final_receipt_started);
    let final_metadata_payload = BuildMetadata {
        build_timing: persisted_build_timing.clone(),
        ..metadata_payload
    };
    let final_metadata_started = Instant::now();
    let final_metadata_json =
        serde_json::to_vec_pretty(&final_metadata_payload).map_err(|source| BuildError::Json {
            source,
            context: "serialize final build metadata".to_string(),
        })?;
    fs::write(&metadata_path, final_metadata_json).map_err(|source| BuildError::Io {
        path: Some(metadata_path.clone()),
        source,
    })?;
    let final_metadata_write_ms = elapsed_ms(final_metadata_started);
    let build_timing = BuildTimingSnapshot {
        total_build_wall_ms: elapsed_ms(total_started),
        cargo_build_ms,
        canonicalize_ms,
        hash_ms,
        receipt_write_ms: receipt_write_ms.saturating_add(final_receipt_write_ms),
        metadata_write_ms: metadata_write_ms.saturating_add(final_metadata_write_ms),
    };

    Ok(BuildOutput {
        module_id: request.module_id.clone(),
        source_artifact_path: artifact_path,
        packaged_wasm_path,
        metadata_path,
        receipt_path,
        wasm_hash_sha256: Some(wasm_hash_sha256),
        source_hash: Some(source_hash),
        build_manifest_hash: Some(build_manifest_hash),
        build_timing: Some(build_timing),
        wasm_size_bytes: Some(wasm_size_bytes),
        dry_run: false,
    })
}

fn validate_request(request: &BuildRequest) -> Result<(), BuildError> {
    if request.module_id.trim().is_empty() {
        return Err(BuildError::InvalidArgument(
            "module_id is empty".to_string(),
        ));
    }
    if !is_path_safe_module_id(&request.module_id) {
        return Err(BuildError::InvalidArgument(format!(
            "module_id must be path-safe, got {}",
            request.module_id
        )));
    }
    if request.target.trim().is_empty() {
        return Err(BuildError::InvalidArgument("target is empty".to_string()));
    }
    if request.profile != "release" && request.profile != "dev" {
        return Err(BuildError::InvalidArgument(format!(
            "profile must be release or dev, got {}",
            request.profile
        )));
    }
    if request.manifest_path.as_os_str().is_empty() {
        return Err(BuildError::InvalidArgument(
            "manifest_path is empty".to_string(),
        ));
    }
    Ok(())
}

fn is_path_safe_module_id(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed == value
        && trimmed != "."
        && trimmed != ".."
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn read_cargo_metadata(manifest_path: &Path) -> Result<CargoMetadata, BuildError> {
    let args = vec![
        "metadata".to_string(),
        "--manifest-path".to_string(),
        manifest_path.to_string_lossy().to_string(),
        "--locked".to_string(),
        "--format-version".to_string(),
        "1".to_string(),
        "--no-deps".to_string(),
    ];
    let output = run_command_capture("cargo", args.as_slice())?;
    serde_json::from_slice(&output.stdout).map_err(|source| BuildError::Json {
        source,
        context: "parse cargo metadata output".to_string(),
    })
}

fn find_package_for_manifest<'a>(
    metadata: &'a CargoMetadata,
    manifest_path: &Path,
) -> Result<&'a CargoPackage, BuildError> {
    let canonical_manifest = canonical_or_original(manifest_path);
    metadata
        .packages
        .iter()
        .find(|package| {
            canonical_or_original(Path::new(package.manifest_path.as_str())) == canonical_manifest
        })
        .or_else(|| metadata.packages.first())
        .ok_or_else(|| {
            BuildError::MetadataInvalid("no package found in cargo metadata output".to_string())
        })
}

fn find_wasm_target_name(package: &CargoPackage) -> Result<String, BuildError> {
    let cdylib = package
        .targets
        .iter()
        .find(|target| target.kind.iter().any(|kind| kind == "cdylib"));
    if let Some(target) = cdylib {
        return Ok(normalize_artifact_name(target.name.as_str()));
    }

    let lib = package
        .targets
        .iter()
        .find(|target| target.kind.iter().any(|kind| kind == "lib"));
    if let Some(target) = lib {
        return Ok(normalize_artifact_name(target.name.as_str()));
    }

    Err(BuildError::MetadataInvalid(
        "no lib/cdylib target found in package; ensure the crate exports a library target"
            .to_string(),
    ))
}

fn resolve_artifact_path(
    metadata: &CargoMetadata,
    target: &str,
    profile: &str,
    target_name: &str,
) -> PathBuf {
    let profile_dir = match profile {
        "release" => "release",
        "dev" => "debug",
        other => other,
    };
    PathBuf::from(metadata.target_directory.as_str())
        .join(target)
        .join(profile_dir)
        .join(format!("{target_name}.wasm"))
}

fn wasm_env_key(suffix: &str) -> String {
    format!("{WASM_ENV_PREFIX}{suffix}")
}

fn wasm_env_value(suffix: &str) -> Option<String> {
    env::var(wasm_env_key(suffix)).ok()
}

fn wasm_env_value_or_default(suffix: &str, default: &str) -> String {
    wasm_env_value(suffix)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

#[expect(
    clippy::too_many_arguments,
    reason = "The explicit canonical build receipt fields preserve the existing hash schema"
)]
fn compute_build_manifest_hash(
    profile: &str,
    target: &str,
    wasm_toolchain: &str,
    wasm_build_std: &str,
    wasm_build_std_components: &str,
    wasm_build_std_features: &str,
    wasm_deterministic_guard: &str,
    canonicalizer_version: &str,
    container_platform: &str,
    builder_image_ref: &str,
    builder_image_digest: &str,
) -> Result<String, BuildError> {
    let payload = BuildManifest {
        profile: profile.to_string(),
        target: target.to_string(),
        wasm_toolchain: wasm_toolchain.to_string(),
        wasm_build_std: wasm_build_std.to_string(),
        wasm_build_std_components: wasm_build_std_components.to_string(),
        wasm_build_std_features: wasm_build_std_features.to_string(),
        wasm_deterministic_guard: wasm_deterministic_guard.to_string(),
        canonicalizer_version: canonicalizer_version.to_string(),
        container_platform: container_platform.to_string(),
        builder_image_ref: builder_image_ref.to_string(),
        builder_image_digest: builder_image_digest.to_string(),
        build_suite_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(|source| BuildError::Json {
        source,
        context: "serialize build manifest".to_string(),
    })?;
    Ok(sha256_hex(bytes.as_slice()))
}

fn compute_source_hash(manifest_path: &Path, target: &str) -> Result<String, BuildError> {
    oasis7_wasm_build::compute_source_hash(manifest_path, target).map_err(|error| {
        BuildError::MetadataInvalid(format!("source hash closure failed: {error}"))
    })
}

fn run_cargo_build(manifest_path: &Path, target: &str, profile: &str) -> Result<(), BuildError> {
    let mut args = vec![
        "build".to_string(),
        "--manifest-path".to_string(),
        manifest_path.to_string_lossy().to_string(),
        "--locked".to_string(),
        "--target".to_string(),
        target.to_string(),
        "--profile".to_string(),
        profile.to_string(),
    ];
    let build_std = BuildStdConfig::from_env();
    args.extend(build_std.cargo_unstable_args());
    run_command_capture("cargo", args.as_slice())?;
    Ok(())
}

fn validate_workspace_compile_time_determinism(
    metadata: &CargoMetadata,
    manifest_path: &Path,
) -> Result<(), BuildError> {
    let enabled = wasm_env_value("VALIDATE_WORKSPACE_COMPILETIME")
        .map(|raw| parse_truthy(raw.as_str()))
        .unwrap_or(true);
    if !enabled {
        return Ok(());
    }

    let workspace_root = if metadata.workspace_root.trim().is_empty() {
        manifest_path
            .parent()
            .map(canonical_or_original)
            .unwrap_or_else(|| canonical_or_original(manifest_path))
    } else {
        canonical_or_original(Path::new(metadata.workspace_root.as_str()))
    };

    let mut offenders = Vec::new();
    for package in &metadata.packages {
        if package.source.is_some() {
            continue;
        }
        let package_manifest = canonical_or_original(Path::new(package.manifest_path.as_str()));
        if !package_manifest.starts_with(&workspace_root) {
            continue;
        }
        let has_build_script = package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "custom-build"));
        let has_proc_macro = package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "proc-macro"));
        if !has_build_script && !has_proc_macro {
            continue;
        }

        let mut reasons = Vec::new();
        if has_build_script {
            reasons.push("build.rs");
        }
        if has_proc_macro {
            reasons.push("proc-macro");
        }
        offenders.push(format!(
            "{} ({}) uses disallowed {}",
            package.name,
            package_manifest.display(),
            reasons.join("+")
        ));
    }

    if offenders.is_empty() {
        return Ok(());
    }

    Err(BuildError::MetadataInvalid(format!(
        "workspace compile-time nondeterminism guard blocked packages: {}",
        offenders.join("; ")
    )))
}

fn parse_truthy(value: &str) -> bool {
    matches!(
        value,
        "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes" | "on" | "ON" | "On"
    )
}

fn run_command_capture(program: &str, args: &[String]) -> Result<std::process::Output, BuildError> {
    let mut command = Command::new(program);
    command.env_remove("RUSTC_WRAPPER");
    for arg in args {
        command.arg(OsStr::new(arg));
    }
    let output = command
        .output()
        .map_err(|source| BuildError::Io { path: None, source })?;

    if !output.status.success() {
        return Err(BuildError::CommandFailed {
            program: program.to_string(),
            args: args.to_vec(),
            status_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output)
}

// Strip custom sections (debug/producers/name/path metadata) so hash checks track
// executable Wasm semantics instead of host/toolchain metadata drift.
fn canonicalize_wasm_bytes(wasm_bytes: &[u8]) -> Result<Vec<u8>, BuildError> {
    let mut module = Module::new();
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload.map_err(|source| BuildError::WasmTransform {
            context: "parse wasm payload".to_string(),
            source,
        })?;
        let Some((section_id, section_range)) = payload.as_section() else {
            continue;
        };

        if section_id == 0 {
            continue;
        }

        let section_bytes = wasm_bytes
            .get(section_range.start..section_range.end)
            .ok_or_else(|| {
                BuildError::MetadataInvalid(format!(
                    "invalid wasm section range start={} end={} len={}",
                    section_range.start,
                    section_range.end,
                    wasm_bytes.len()
                ))
            })?;
        module.section(&RawSection {
            id: section_id,
            data: section_bytes,
        });
    }
    Ok(module.finish())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use wasm_encoder::CustomSection;
    use wasmparser::Payload;

    fn sample_request() -> BuildRequest {
        BuildRequest {
            module_id: "test.module".to_string(),
            manifest_path: PathBuf::from("/tmp/module/Cargo.toml"),
            out_dir: PathBuf::from("/tmp/out"),
            target: DEFAULT_TARGET.to_string(),
            profile: DEFAULT_PROFILE.to_string(),
            dry_run: false,
        }
    }

    #[test]
    fn validate_request_rejects_empty_module_id() {
        let mut request = sample_request();
        request.module_id = "  ".to_string();
        let error = validate_request(&request).expect_err("expected empty module id to fail");
        assert!(matches!(error, BuildError::InvalidArgument(_)));
    }

    #[test]
    fn validate_request_rejects_invalid_profile() {
        let mut request = sample_request();
        request.profile = "staging".to_string();
        let error = validate_request(&request).expect_err("expected invalid profile to fail");
        assert!(matches!(error, BuildError::InvalidArgument(_)));
    }

    #[test]
    fn validate_request_rejects_empty_manifest_path() {
        let mut request = sample_request();
        request.manifest_path = PathBuf::new();
        let error = validate_request(&request).expect_err("expected empty manifest path to fail");
        assert!(matches!(error, BuildError::InvalidArgument(_)));
    }

    #[test]
    fn resolve_artifact_path_maps_profile_directory() {
        let metadata = CargoMetadata {
            workspace_root: "/tmp".to_string(),
            packages: Vec::new(),
            target_directory: "/tmp/target".to_string(),
        };
        let release = resolve_artifact_path(&metadata, DEFAULT_TARGET, "release", "demo_module");
        let dev = resolve_artifact_path(&metadata, DEFAULT_TARGET, "dev", "demo_module");

        assert_eq!(
            release,
            std::path::Path::new("/tmp/target")
                .join(DEFAULT_TARGET)
                .join("release")
                .join("demo_module.wasm")
        );
        assert_eq!(
            dev,
            std::path::Path::new("/tmp/target")
                .join(DEFAULT_TARGET)
                .join("debug")
                .join("demo_module.wasm")
        );
    }

    #[test]
    fn find_wasm_target_prefers_cdylib_and_normalizes_name() {
        let package = CargoPackage {
            name: "demo".to_string(),
            source: None,
            manifest_path: "/tmp/module/Cargo.toml".to_string(),
            targets: vec![
                CargoTarget {
                    name: "demo-lib".to_string(),
                    kind: vec!["lib".to_string()],
                },
                CargoTarget {
                    name: "demo-cdylib".to_string(),
                    kind: vec!["cdylib".to_string()],
                },
            ],
        };

        let target_name =
            find_wasm_target_name(&package).expect("expected cdylib target to be selected");
        assert_eq!(target_name, "demo_cdylib");
    }

    #[test]
    fn normalize_artifact_name_replaces_hyphen() {
        assert_eq!(normalize_artifact_name("alpha-beta"), "alpha_beta");
    }

    #[test]
    fn canonicalize_wasm_bytes_drops_all_custom_sections() {
        let mut module = Module::new();
        module.section(&CustomSection {
            name: Cow::Borrowed("name"),
            data: Cow::Borrowed(b"debug-name-bytes"),
        });
        module.section(&CustomSection {
            name: Cow::Borrowed("producers"),
            data: Cow::Borrowed(b"debug-producers-bytes"),
        });
        let input = module.finish();

        let canonical = canonicalize_wasm_bytes(&input).expect("canonicalize wasm");
        let has_custom = Parser::new(0)
            .parse_all(&canonical)
            .filter_map(Result::ok)
            .any(|payload| matches!(payload, Payload::CustomSection(_)));
        assert!(
            !has_custom,
            "canonicalized wasm should not keep custom sections"
        );
    }

    #[test]
    fn build_std_config_disabled_emits_no_unstable_args() {
        let config = BuildStdConfig {
            enabled: false,
            components: "std,panic_abort".to_string(),
            features: "panic_immediate_abort".to_string(),
        };
        assert!(config.cargo_unstable_args().is_empty());
    }

    #[test]
    fn build_std_config_enabled_emits_expected_unstable_args() {
        let config = BuildStdConfig {
            enabled: true,
            components: "core,std".to_string(),
            features: "panic_immediate_abort".to_string(),
        };
        assert_eq!(
            config.cargo_unstable_args(),
            vec![
                "-Z".to_string(),
                "build-std=core,std".to_string(),
                "-Z".to_string(),
                "build-std-features=panic_immediate_abort".to_string(),
            ]
        );
    }

    #[test]
    fn parse_truthy_accepts_expected_values() {
        for value in ["1", "true", "TRUE", "yes", "On"] {
            assert!(parse_truthy(value), "value should be truthy: {value}");
        }
        for value in ["0", "false", "off", "", "random"] {
            assert!(!parse_truthy(value), "value should be falsey: {value}");
        }
    }

    #[test]
    fn source_hash_tracks_local_path_dependency_changes() {
        let root = std::env::temp_dir().join(format!(
            "wasm-build-suite-source-hash-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("module/src")).expect("create module dir");
        fs::create_dir_all(root.join("shared/src")).expect("create shared dir");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"module\", \"shared\"]\nresolver = \"2\"\n",
        )
        .expect("write workspace manifest");
        fs::write(
            root.join("module/Cargo.toml"),
            "[package]\nname = \"fixture_module\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nfixture_shared = { path = \"../shared\" }\n",
        )
        .expect("write module manifest");
        fs::write(
            root.join("module/src/lib.rs"),
            "pub fn call() -> u32 { fixture_shared::value() }\n",
        )
        .expect("write module source");
        fs::write(
            root.join("shared/Cargo.toml"),
            "[package]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write shared manifest");
        fs::write(
            root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        )
        .expect("write shared source");

        let manifest_path = root.join("module/Cargo.toml");
        let initial =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("initial source hash");
        fs::write(
            root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 2 }\n",
        )
        .expect("rewrite shared source");
        let changed =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("changed source hash");
        assert_ne!(initial, changed);
        let _ = fs::remove_dir_all(root);
    }
}
