use super::*;
use std::env;
use std::ffi::OsStr;
use std::sync::{Mutex, MutexGuard};

static ENV_MUTATION_LOCK: Mutex<()> = Mutex::new(());

fn lock_env_mutations() -> MutexGuard<'static, ()> {
    ENV_MUTATION_LOCK
        .lock()
        .expect("env mutation test lock should not be poisoned")
}

fn set_env_var<K, V>(key: K, value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    // SAFETY: These tests mutate process environment in a scoped guard setup.
    // Each test holds ENV_MUTATION_LOCK before capture, mutation, validation,
    // and restoration.
    unsafe {
        env::set_var(key, value);
    }
}

fn remove_env_var<K>(key: K)
where
    K: AsRef<OsStr>,
{
    // SAFETY: These tests mutate process environment in a scoped guard setup.
    // Each test holds ENV_MUTATION_LOCK before capture, mutation, validation,
    // and restoration.
    unsafe {
        env::remove_var(key);
    }
}

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn capture(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            previous: env::var(&key).ok(),
            key,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.take() {
            set_env_var(&self.key, value);
        } else {
            remove_env_var(&self.key);
        }
    }
}

#[test]
fn compile_time_guard_rejects_workspace_build_script_target() {
    let _env_lock = lock_env_mutations();
    let _guard = EnvVarGuard::capture(wasm_env_key("VALIDATE_WORKSPACE_COMPILETIME"));
    let removed_old_brand_key = removed_old_brand_wasm_env("VALIDATE_WORKSPACE_COMPILETIME");
    let _removed_old_brand_guard = EnvVarGuard::capture(removed_old_brand_key.as_str());
    set_env_var(wasm_env_key("VALIDATE_WORKSPACE_COMPILETIME"), "1");
    remove_env_var(removed_old_brand_key.as_str());

    let metadata = CargoMetadata {
        workspace_root: "/workspace".to_string(),
        target_directory: "/workspace/target".to_string(),
        packages: vec![CargoPackage {
            name: "m1_rule_move".to_string(),
            source: None,
            manifest_path: "/workspace/crates/m1_rule_move/Cargo.toml".to_string(),
            targets: vec![
                CargoTarget {
                    name: "m1_rule_move".to_string(),
                    kind: vec!["cdylib".to_string()],
                },
                CargoTarget {
                    name: "build_script_build".to_string(),
                    kind: vec!["custom-build".to_string()],
                },
            ],
        }],
    };
    let manifest_path = std::path::Path::new("/workspace/crates/m1_rule_move/Cargo.toml");

    let error = validate_workspace_compile_time_determinism(&metadata, manifest_path)
        .expect_err("expected workspace build.rs target to be rejected");
    assert!(matches!(error, BuildError::MetadataInvalid(_)));
}

#[test]
fn compile_time_guard_allows_external_proc_macro_package() {
    let _env_lock = lock_env_mutations();
    let _guard = EnvVarGuard::capture(wasm_env_key("VALIDATE_WORKSPACE_COMPILETIME"));
    let removed_old_brand_key = removed_old_brand_wasm_env("VALIDATE_WORKSPACE_COMPILETIME");
    let _removed_old_brand_guard = EnvVarGuard::capture(removed_old_brand_key.as_str());
    set_env_var(wasm_env_key("VALIDATE_WORKSPACE_COMPILETIME"), "1");
    remove_env_var(removed_old_brand_key.as_str());

    let metadata = CargoMetadata {
        workspace_root: "/workspace".to_string(),
        target_directory: "/workspace/target".to_string(),
        packages: vec![CargoPackage {
            name: "serde_derive".to_string(),
            source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            manifest_path: "/cargo/registry/src/serde_derive/Cargo.toml".to_string(),
            targets: vec![CargoTarget {
                name: "serde_derive".to_string(),
                kind: vec!["proc-macro".to_string()],
            }],
        }],
    };
    let manifest_path = std::path::Path::new("/workspace/crates/m1_rule_move/Cargo.toml");

    validate_workspace_compile_time_determinism(&metadata, manifest_path)
        .expect("external dependency proc-macro should be ignored by workspace guard");
}

#[test]
fn wasm_env_value_or_default_reads_oasis7_prefix() {
    let _env_lock = lock_env_mutations();
    let _primary = EnvVarGuard::capture(wasm_env_key("BUILD_STD"));
    let removed_old_brand_key = removed_old_brand_wasm_env("BUILD_STD");
    let _removed_old_brand = EnvVarGuard::capture(removed_old_brand_key.as_str());
    set_env_var(wasm_env_key("BUILD_STD"), "1");
    set_env_var(removed_old_brand_key.as_str(), "0");

    assert_eq!(wasm_env_value_or_default("BUILD_STD", "missing"), "1");
}

#[test]
fn wasm_env_value_or_default_rejects_removed_old_brand_prefix() {
    let _env_lock = lock_env_mutations();
    let _primary = EnvVarGuard::capture(wasm_env_key("BUILD_STD_COMPONENTS"));
    let removed_old_brand_key = removed_old_brand_wasm_env("BUILD_STD_COMPONENTS");
    let _removed_old_brand = EnvVarGuard::capture(removed_old_brand_key.as_str());
    remove_env_var(wasm_env_key("BUILD_STD_COMPONENTS"));
    set_env_var(removed_old_brand_key.as_str(), "std,panic_abort");

    assert_eq!(
        wasm_env_value_or_default("BUILD_STD_COMPONENTS", "missing"),
        "missing"
    );
}

fn removed_old_brand_wasm_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "WASM", suffix].join("_")
}

#[test]
fn candidate_recipe_requires_the_actual_fixed_guest_job_environment() {
    const CHILD: &str = "OASIS7_TEST_CANDIDATE_JOBS_CHILD";
    if env::var_os(CHILD).is_some() {
        let request = BuildRequest::with_defaults("fixture.module", "unused/Cargo.toml");
        let error = read_candidate_builder(&request).unwrap_err();
        assert!(error.to_string().contains("CARGO_BUILD_JOBS=2"), "{error}");
        return;
    }
    for jobs in [None, Some("7")] {
        let mut command = std::process::Command::new(env::current_exe().unwrap());
        command.args(["--exact", "env_prefix_tests::candidate_recipe_requires_the_actual_fixed_guest_job_environment", "--nocapture"])
            .env(CHILD, "1")
            .env("OASIS7_WASM_CANDIDATE_BUILDER_EVIDENCE", "/missing-candidate-evidence.json")
            .env_remove("OASIS7_WASM_BUILDER_IMAGE_REF")
            .env_remove("OASIS7_WASM_BUILDER_IMAGE_DIGEST");
        if let Some(jobs) = jobs {
            command.env("CARGO_BUILD_JOBS", jobs);
        } else {
            command.env_remove("CARGO_BUILD_JOBS");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
