#![cfg(feature = "test_tier_full")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::{
    BlobStore, HashAlgorithm, LocalCasStore, load_builtin_wasm_with_fetch_fallback, util,
};

const FETCHER_ENV: &str = "OASIS7_BUILTIN_WASM_FETCHER";
const COMPILER_ENV: &str = "OASIS7_BUILTIN_WASM_COMPILER";

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn materializer_fetch_miss_falls_back_to_compile_and_caches_blob() {
    let _env_guard = ENV_LOCK.lock().expect("lock env");
    let temp_root = temp_dir("builtin-materializer");
    fs::create_dir_all(&temp_root).expect("create temp root");

    let distfs_root = temp_root.join("distfs");
    fs::create_dir_all(distfs_root.join("blobs")).expect("create distfs blobs");

    let fetch_log = temp_root.join("fetch.log");
    let fetcher = temp_root.join("fetcher.sh");
    write_fetcher_script(&fetcher, &fetch_log);
    let compiler = temp_root.join("compiler.sh");
    let compiled_bytes = b"\0asmfallback-test-builtin".to_vec();
    let compiled_hash = util::sha256_hex(&compiled_bytes);
    write_compiler_script(&compiler, compiled_bytes.as_slice());

    let module_id = "m9.test.fallback";
    let expected_hashes = [compiled_hash.clone()];
    let expected_hash_refs: Vec<&str> = expected_hashes.iter().map(String::as_str).collect();

    let _fetcher_guard = EnvVarGuard::capture(FETCHER_ENV);
    let removed_old_brand_fetcher = removed_old_brand_builtin_wasm_env("FETCHER");
    let _removed_old_brand_fetcher_guard = EnvVarGuard::capture(removed_old_brand_fetcher.as_str());
    let _compiler_guard = EnvVarGuard::capture(COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_builtin_wasm_env("COMPILER");
    let _removed_old_brand_compiler_guard =
        EnvVarGuard::capture(removed_old_brand_compiler.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(FETCHER_ENV, &fetcher);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(COMPILER_ENV, &compiler);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_fetcher.as_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }

    let load_result =
        load_builtin_wasm_with_fetch_fallback(module_id, &expected_hash_refs, &distfs_root);

    let bytes = load_result.expect("load builtin wasm");
    let actual_hash = util::sha256_hex(&bytes);
    assert!(
        expected_hashes.iter().any(|hash| hash == &actual_hash),
        "loaded hash should be listed in manifest"
    );

    let fetched_log = fs::read_to_string(&fetch_log).expect("read fetch log");
    assert!(
        fetched_log.contains(module_id),
        "fetcher log should contain module_id"
    );
    assert!(
        expected_hashes
            .iter()
            .any(|hash| fetched_log.contains(hash)),
        "fetcher log should contain one of expected hashes"
    );

    let store = LocalCasStore::new_with_hash_algorithm(&distfs_root, HashAlgorithm::Sha256);
    assert!(store.has(&actual_hash).expect("distfs has expected hash"));
    let cached = store
        .get_verified(&actual_hash)
        .expect("verified distfs blob");
    assert_eq!(cached, bytes);

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn materializer_reuses_cached_module_hash_when_manifest_hashes_do_not_match() {
    let _env_guard = ENV_LOCK.lock().expect("lock env");
    let temp_root = temp_dir("builtin-materializer-cache");
    fs::create_dir_all(&temp_root).expect("create temp root");

    let distfs_root = temp_root.join("distfs");
    fs::create_dir_all(distfs_root.join("blobs")).expect("create distfs blobs");

    let module_id = "m9.experimental.cross_platform";
    let wasm_bytes = b"\0asmcached-cross-platform".to_vec();
    let cached_hash = util::sha256_hex(&wasm_bytes);
    let stale_manifest_hash =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();

    let store = LocalCasStore::new_with_hash_algorithm(&distfs_root, HashAlgorithm::Sha256);
    store
        .put(&cached_hash, &wasm_bytes)
        .expect("store cached wasm");
    fs::write(
        distfs_root.join("module_hash_index.txt"),
        format!("{module_id} {cached_hash}\n"),
    )
    .expect("write module hash index");

    let expected_hashes = [stale_manifest_hash];
    let expected_hash_refs: Vec<&str> = expected_hashes.iter().map(String::as_str).collect();
    let loaded =
        load_builtin_wasm_with_fetch_fallback(module_id, &expected_hash_refs, &distfs_root)
            .expect("load cached wasm by module hash index");

    assert_eq!(loaded, wasm_bytes);

    let _ = fs::remove_dir_all(&temp_root);
}

fn write_fetcher_script(script_path: &Path, fetch_log: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\necho \"$1 $2\" >> \"{}\"\nexit 1\n",
        fetch_log.display()
    );
    fs::write(script_path, script).expect("write fetcher script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(script_path, permissions).expect("chmod fetcher script");
    }
}

fn write_compiler_script(script_path: &Path, wasm_bytes: &[u8]) {
    let artifact_path = script_path.with_extension("wasm");
    fs::write(&artifact_path, wasm_bytes).expect("write compiler artifact");
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\ncp \"{}\" \"$3\"\n",
        artifact_path.display()
    );
    fs::write(script_path, script).expect("write compiler script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(script_path, permissions).expect("chmod compiler script");
    }
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
                    crate::env_mut::set_var(&self.key, value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    crate::env_mut::remove_var(&self.key);
                }
            }
        }
    }
}

fn removed_old_brand_builtin_wasm_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "BUILTIN", "WASM", suffix].join("_")
}
