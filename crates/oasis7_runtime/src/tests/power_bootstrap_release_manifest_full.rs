#![cfg(all(feature = "wasmtime", feature = "test_tier_full"))]

use super::super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

const BUILTIN_WASM_DISTFS_ROOT_ENV: &str = "OASIS7_BUILTIN_WASM_DISTFS_ROOT";
const BUILTIN_WASM_COMPILER_ENV: &str = "OASIS7_BUILTIN_WASM_COMPILER";
const FAULT_SIG_MANIFEST_UNREACHABLE: &str = "fault_signature=builtin_release_manifest_unreachable";
const FAULT_SIG_MANIFEST_MISSING_OR_ROLLED_BACK: &str =
    "fault_signature=builtin_release_manifest_missing_or_rolled_back";
const FAULT_SIG_MANIFEST_IDENTITY_DRIFT: &str =
    "fault_signature=builtin_release_manifest_identity_drift";

static RELEASE_MANIFEST_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn production_policy_surfaces_fault_signature_when_online_manifest_unreachable() {
    let _env_lock = lock_release_manifest_env();
    let temp_root = temp_dir("builtin-release-manifest-unreachable");
    fs::create_dir_all(temp_root.join("blobs")).expect("create temp distfs blobs");

    let _distfs_guard = EnvVarGuard::capture(BUILTIN_WASM_DISTFS_ROOT_ENV);
    let removed_old_brand_distfs_root = removed_old_brand_builtin_wasm_env("DISTFS_ROOT");
    let _compat_old_brand_distfs_guard =
        EnvVarGuard::capture(removed_old_brand_distfs_root.as_str());
    let _compiler_guard = EnvVarGuard::capture(BUILTIN_WASM_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_builtin_wasm_env("COMPILER");
    let _compat_old_brand_compiler_guard =
        EnvVarGuard::capture(removed_old_brand_compiler.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(BUILTIN_WASM_DISTFS_ROOT_ENV, &temp_root);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_distfs_root.as_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(
            BUILTIN_WASM_COMPILER_ENV,
            temp_root.join("missing-builtin-compiler"),
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }

    let mut world = World::new();
    world.enable_production_release_policy();
    upsert_release_manifest_entry(
        &mut world,
        M1_RADIATION_POWER_MODULE_ID,
        "1111111111111111111111111111111111111111111111111111111111111111",
    );

    let err = world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("online manifest materialization should fail");
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(
        reason.contains(FAULT_SIG_MANIFEST_UNREACHABLE),
        "unexpected reason: {reason}"
    );
    assert!(
        reason.contains(M1_RADIATION_POWER_MODULE_ID),
        "unexpected reason: {reason}"
    );
}

#[test]
fn production_policy_surfaces_fault_signature_when_manifest_entry_is_rolled_back() {
    let mut world = World::new();
    world.enable_production_release_policy();
    upsert_release_manifest_entry(
        &mut world,
        M1_RADIATION_POWER_MODULE_ID,
        "2222222222222222222222222222222222222222222222222222222222222222",
    );
    upsert_release_manifest_entry(
        &mut world,
        M1_STORAGE_POWER_MODULE_ID,
        "3333333333333333333333333333333333333333333333333333333333333333",
    );
    assert!(world.remove_builtin_release_manifest_entry("m1", M1_RADIATION_POWER_MODULE_ID));
    assert!(world.remove_builtin_release_manifest_entry("m1", M1_STORAGE_POWER_MODULE_ID));

    let err = world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("rolled back manifest should fail");
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(
        reason.contains(FAULT_SIG_MANIFEST_MISSING_OR_ROLLED_BACK),
        "unexpected reason: {reason}"
    );
    assert!(
        reason.contains(M1_RADIATION_POWER_MODULE_ID),
        "unexpected reason: {reason}"
    );
}

#[test]
fn production_policy_surfaces_fault_signature_when_manifest_identity_drifts() {
    let _env_lock = lock_release_manifest_env();
    let temp_root = temp_dir("builtin-release-manifest-drift");
    fs::create_dir_all(temp_root.join("blobs")).expect("create temp distfs blobs");

    let radiation_wasm_bytes = b"manifest-identity-drift-radiation";
    let radiation_wasm_hash = util::sha256_hex(radiation_wasm_bytes);
    let storage_wasm_bytes = b"manifest-identity-drift-storage";
    let storage_wasm_hash = util::sha256_hex(storage_wasm_bytes);
    let store = LocalCasStore::new_with_hash_algorithm(&temp_root, HashAlgorithm::Sha256);
    store
        .put(radiation_wasm_hash.as_str(), radiation_wasm_bytes)
        .expect("store radiation wasm blob");
    store
        .put(storage_wasm_hash.as_str(), storage_wasm_bytes)
        .expect("store storage wasm blob");

    let drift_hash = "4444444444444444444444444444444444444444444444444444444444444444";
    let _distfs_guard = EnvVarGuard::capture(BUILTIN_WASM_DISTFS_ROOT_ENV);
    let removed_old_brand_distfs_root = removed_old_brand_builtin_wasm_env("DISTFS_ROOT");
    let _compat_old_brand_distfs_guard =
        EnvVarGuard::capture(removed_old_brand_distfs_root.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(BUILTIN_WASM_DISTFS_ROOT_ENV, &temp_root);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_distfs_root.as_str());
    }

    let mut world = World::new();
    world.enable_production_release_policy();
    upsert_release_manifest_entry(
        &mut world,
        M1_RADIATION_POWER_MODULE_ID,
        &radiation_wasm_hash,
    );
    upsert_release_manifest_entry_with_identity_map(
        &mut world,
        M1_STORAGE_POWER_MODULE_ID,
        vec![storage_wasm_hash.clone()],
        BTreeMap::from([(
            drift_hash.to_string(),
            super::signed_test_artifact_identity(drift_hash),
        )]),
    );

    let err = world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("identity drift should fail");
    let WorldError::ModuleChangeInvalid { reason } = err else {
        panic!("expected ModuleChangeInvalid");
    };
    assert!(
        reason.contains(FAULT_SIG_MANIFEST_IDENTITY_DRIFT),
        "unexpected reason: {reason}"
    );
    assert!(
        reason.contains(M1_STORAGE_POWER_MODULE_ID),
        "unexpected reason: {reason}"
    );
    assert!(reason.contains(drift_hash), "unexpected reason: {reason}");
    assert!(
        reason.contains(&storage_wasm_hash),
        "unexpected reason: {reason}"
    );
}

fn upsert_release_manifest_entry(world: &mut World, module_id: &str, wasm_hash: &str) {
    upsert_release_manifest_entry_with_identity_map(
        world,
        module_id,
        vec![wasm_hash.to_string()],
        BTreeMap::from([(
            wasm_hash.to_string(),
            super::signed_test_artifact_identity(wasm_hash),
        )]),
    );
}

fn upsert_release_manifest_entry_with_identity_map(
    world: &mut World,
    module_id: &str,
    hash_tokens: Vec<String>,
    artifact_identities: BTreeMap<String, ModuleArtifactIdentity>,
) {
    world
        .upsert_builtin_release_manifest_entry(
            "m1",
            module_id,
            BuiltinReleaseManifestEntry {
                hash_tokens,
                artifact_identities,
            },
        )
        .expect("upsert builtin release manifest entry");
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration since epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-runtime-tests-{prefix}-{}-{unique}",
        std::process::id()
    ))
}

fn lock_release_manifest_env() -> MutexGuard<'static, ()> {
    RELEASE_MANIFEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn capture(key: &str) -> Self {
        Self {
            key: key.to_string(),
            previous: std::env::var(key).ok(),
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    crate::env_mut::set_var(self.key.as_str(), value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    crate::env_mut::remove_var(self.key.as_str());
                }
            }
        }
    }
}

fn removed_old_brand_builtin_wasm_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "BUILTIN", "WASM", suffix].join("_")
}
