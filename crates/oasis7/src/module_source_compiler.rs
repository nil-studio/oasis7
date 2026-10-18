use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use oasis7_agent_api::module_source::{
    ModuleSourceCompileError as WorldError, ModuleSourceCompiler,
};
use oasis7_agent_api::world_action::ModuleSourcePackage;

pub(crate) struct HostModuleSourceCompiler;
impl ModuleSourceCompiler for HostModuleSourceCompiler {
    fn compile(
        &self,
        module_id: &str,
        package: &ModuleSourcePackage,
    ) -> Result<Vec<u8>, WorldError> {
        compile_module_artifact_from_source(module_id, package)
    }
}

const MODULE_SOURCE_ENV_PREFIX: &str = "OASIS7_MODULE_SOURCE_";
const MODULE_SOURCE_BUILD_PROFILE: &str = "release";

const DEFAULT_MODULE_SOURCE_MAX_FILES: usize = 128;
const DEFAULT_MODULE_SOURCE_MAX_FILE_BYTES: usize = 512 * 1024;
const DEFAULT_MODULE_SOURCE_MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MODULE_SOURCE_COMPILE_TIMEOUT_MS: u64 = 120_000;

const COMPILER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "RUSTFLAGS",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "GIT_SSL_CAINFO",
];

pub(crate) fn compile_module_artifact_from_source(
    module_id: &str,
    source_package: &ModuleSourcePackage,
) -> Result<Vec<u8>, WorldError> {
    if module_id.trim().is_empty() {
        return Err(WorldError::ModuleChangeInvalid {
            reason: "compile module source rejected: module_id is empty".to_string(),
        });
    }
    if source_package.manifest_path.trim().is_empty() {
        return Err(WorldError::ModuleChangeInvalid {
            reason: "compile module source rejected: manifest_path is empty".to_string(),
        });
    }
    if source_package.files.is_empty() {
        return Err(WorldError::ModuleChangeInvalid {
            reason: "compile module source rejected: source files are empty".to_string(),
        });
    }

    validate_source_package_limits(source_package)?;

    let workspace_dir = temp_source_workspace(module_id);
    fs::create_dir_all(&workspace_dir).map_err(WorldError::from)?;
    let sandbox_tmp_dir = workspace_dir.join("tmp");
    fs::create_dir_all(&sandbox_tmp_dir).map_err(WorldError::from)?;

    let result = (|| {
        let manifest_rel = validate_relative_source_path(source_package.manifest_path.as_str())?;

        for (file_path, file_bytes) in &source_package.files {
            let rel_path = validate_relative_source_path(file_path.as_str())?;
            let full_path = workspace_dir.join(rel_path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(WorldError::from)?;
            }
            fs::write(full_path, file_bytes).map_err(WorldError::from)?;
        }

        let manifest_abs = workspace_dir.join(manifest_rel.as_path());
        if !manifest_abs.exists() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: manifest path missing {}",
                    source_package.manifest_path
                ),
            });
        }

        let out_dir = workspace_dir.join("out");
        fs::create_dir_all(&out_dir).map_err(WorldError::from)?;
        let out_path = out_dir.join(format!("{module_id}.wasm"));

        if let Some(compiler_path) = module_source_env_non_empty("COMPILER") {
            compile_via_custom_command(
                Path::new(compiler_path.as_str()),
                module_id,
                workspace_dir.as_path(),
                source_package.manifest_path.as_str(),
                out_path.as_path(),
                sandbox_tmp_dir.as_path(),
            )?;
        } else {
            compile_via_default_script(
                module_id,
                workspace_dir.as_path(),
                source_package.manifest_path.as_str(),
                out_dir.as_path(),
                sandbox_tmp_dir.as_path(),
            )?;
        }

        let output_bytes =
            fs::read(out_path.as_path()).map_err(|error| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: output missing path={} err={error}",
                    out_path.display()
                ),
            })?;
        Ok(output_bytes)
    })();

    let _ = fs::remove_dir_all(&workspace_dir);
    result
}

fn compile_via_custom_command(
    compiler_path: &Path,
    module_id: &str,
    workspace_dir: &Path,
    manifest_path: &str,
    out_path: &Path,
    sandbox_tmp_dir: &Path,
) -> Result<(), WorldError> {
    let mut command = Command::new(compiler_path);
    command
        .current_dir(workspace_dir)
        .arg(module_id)
        .arg(workspace_dir)
        .arg(manifest_path)
        .arg(out_path);
    apply_compiler_environment(&mut command, sandbox_tmp_dir);

    let status = run_command_with_timeout(
        &mut command,
        compile_timeout(),
        format!("compiler={}", compiler_path.display()).as_str(),
    )?;

    if status.success() {
        return Ok(());
    }

    Err(WorldError::ModuleChangeInvalid {
        reason: format!(
            "compile module source rejected: compiler exited non-zero compiler={} status={status}",
            compiler_path.display()
        ),
    })
}

fn compile_via_default_script(
    module_id: &str,
    workspace_dir: &Path,
    manifest_path: &str,
    out_dir: &Path,
    sandbox_tmp_dir: &Path,
) -> Result<(), WorldError> {
    let script_path = repo_root().join("scripts").join("build-wasm-module.sh");
    let manifest_abs = workspace_dir.join(manifest_path);

    let mut command = Command::new(script_path.as_path());
    command
        .current_dir(workspace_dir)
        .arg("--module-id")
        .arg(module_id)
        .arg("--manifest-path")
        .arg(manifest_abs.as_path())
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--profile")
        .arg(MODULE_SOURCE_BUILD_PROFILE);
    apply_compiler_environment(&mut command, sandbox_tmp_dir);

    let status = run_command_with_timeout(
        &mut command,
        compile_timeout(),
        format!("script={}", script_path.display()).as_str(),
    )?;

    if status.success() {
        return Ok(());
    }

    Err(WorldError::ModuleChangeInvalid {
        reason: format!(
            "compile module source rejected: default builder exited non-zero script={} status={status}",
            script_path.display()
        ),
    })
}

fn validate_relative_source_path(raw: &str) -> Result<PathBuf, WorldError> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty() {
        return Err(WorldError::ModuleChangeInvalid {
            reason: "compile module source rejected: source path is empty".to_string(),
        });
    }
    if path.is_absolute() {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "compile module source rejected: absolute source path is not allowed {}",
                raw
            ),
        });
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "compile module source rejected: invalid source path {}",
                        raw
                    ),
                });
            }
        }
    }
    Ok(path.to_path_buf())
}

fn validate_source_package_limits(source_package: &ModuleSourcePackage) -> Result<(), WorldError> {
    let max_files = module_source_env_limit_usize("MAX_FILES", DEFAULT_MODULE_SOURCE_MAX_FILES);
    let max_file_bytes =
        module_source_env_limit_usize("MAX_FILE_BYTES", DEFAULT_MODULE_SOURCE_MAX_FILE_BYTES);
    let max_total_bytes =
        module_source_env_limit_usize("MAX_TOTAL_BYTES", DEFAULT_MODULE_SOURCE_MAX_TOTAL_BYTES);

    if source_package.files.len() > max_files {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "compile module source rejected: source file count exceeds limit count={} limit={max_files}",
                source_package.files.len()
            ),
        });
    }

    let mut total_bytes = 0usize;
    for (path, file_bytes) in &source_package.files {
        if file_bytes.len() > max_file_bytes {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: source file exceeds size limit path={} bytes={} limit={max_file_bytes}",
                    path,
                    file_bytes.len()
                ),
            });
        }
        total_bytes = total_bytes.saturating_add(file_bytes.len());
        if total_bytes > max_total_bytes {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: source package exceeds total size limit bytes={total_bytes} limit={max_total_bytes}",
                ),
            });
        }
    }

    Ok(())
}

fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
    command_label: &str,
) -> Result<ExitStatus, WorldError> {
    let mut child = command
        .spawn()
        .map_err(|error| WorldError::ModuleChangeInvalid {
            reason: format!(
                "compile module source rejected: execute compiler failed {} err={error}",
                command_label
            ),
        })?;

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: wait compiler failed {} err={error}",
                    command_label
                ),
            })?
        {
            return Ok(status);
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "compile module source rejected: compiler timed out {} timeout_ms={}",
                    command_label,
                    timeout.as_millis(),
                ),
            });
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

fn apply_compiler_environment(command: &mut Command, sandbox_tmp_dir: &Path) {
    command.env_clear();
    for key in COMPILER_ENV_ALLOWLIST {
        if let Some(value) = env_non_empty(key) {
            command.env(key, value);
        }
    }
    command.env("TMPDIR", sandbox_tmp_dir);
    command.env("TMP", sandbox_tmp_dir);
    command.env("TEMP", sandbox_tmp_dir);
}

fn compile_timeout() -> Duration {
    let timeout_ms = module_source_env_non_empty("COMPILE_TIMEOUT_MS")
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MODULE_SOURCE_COMPILE_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}

fn module_source_env_key(suffix: &str) -> String {
    format!("{MODULE_SOURCE_ENV_PREFIX}{suffix}")
}

fn module_source_env_non_empty(suffix: &str) -> Option<String> {
    env_non_empty(module_source_env_key(suffix).as_str())
}

fn module_source_env_limit_usize(suffix: &str, default: usize) -> usize {
    module_source_env_non_empty(suffix)
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn temp_source_workspace(module_id: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-module-source-{module_id}-{}-{now}",
        std::process::id()
    ))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[cfg(test)]
mod tests {
    use super::{module_source_env_key, module_source_env_non_empty};
    use std::sync::{Mutex, OnceLock};

    #[test]
    fn module_source_env_non_empty_reads_oasis7_value() {
        let _guard = module_source_env_lock().lock().expect("env lock");
        let key = module_source_env_key("COMPILER");
        let _env_guard = TestEnvGuard::capture(key.as_str());
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::set_var(key.as_str(), "/tmp/oasis7-module-source-compiler");
        }

        assert_eq!(
            module_source_env_non_empty("COMPILER").as_deref(),
            Some("/tmp/oasis7-module-source-compiler")
        );
    }

    #[test]
    fn module_source_env_non_empty_rejects_removed_old_brand_value() {
        let _guard = module_source_env_lock().lock().expect("env lock");
        let key = module_source_env_key("COMPILER");
        let _env_guard = TestEnvGuard::capture(key.as_str());
        let removed_old_brand_key = removed_old_brand_module_source_env("COMPILER");
        let _removed_old_brand_guard = TestEnvGuard::capture(removed_old_brand_key.as_str());
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::remove_var(key.as_str());
        }
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::set_var(
                removed_old_brand_key.as_str(),
                "/tmp/removed-old-brand-module-source-compiler",
            );
        }

        assert!(module_source_env_non_empty("COMPILER").is_none());
    }

    fn module_source_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn removed_old_brand_module_source_env(suffix: &str) -> String {
        ["AGENT", "WORLD", "MODULE", "SOURCE", suffix].join("_")
    }

    struct TestEnvGuard {
        key: String,
        previous: Option<String>,
    }

    impl TestEnvGuard {
        fn capture(key: &str) -> Self {
            Self {
                key: key.to_string(),
                previous: std::env::var(key).ok(),
            }
        }
    }

    impl Drop for TestEnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.as_ref() {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::set_var(self.key.as_str(), value);
                }
            } else {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::remove_var(self.key.as_str());
                }
            }
        }
    }
}
