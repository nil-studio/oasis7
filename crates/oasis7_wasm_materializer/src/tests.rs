use super::*;
use std::sync::Mutex;

static BUILTIN_WASM_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn builtin_module_manifests_follow_runtime_artifact_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (module_id, relative_path, source) in [
        (
            "m1.rule.move",
            M1_BUILTIN_MODULE_IDS_PATH,
            include_str!("../../oasis7_runtime/src/world/artifacts/m1_builtin_module_ids.txt"),
        ),
        (
            "m4.economy.production",
            M4_BUILTIN_MODULE_IDS_PATH,
            include_str!("../../oasis7_runtime/src/world/artifacts/m4_builtin_module_ids.txt"),
        ),
        (
            "m5.gameplay.war.core",
            M5_BUILTIN_MODULE_IDS_PATH,
            include_str!("../../oasis7_runtime/src/world/artifacts/m5_builtin_module_ids.txt"),
        ),
    ] {
        let actual = builtin_module_ids_path_for(module_id, &root).expect("builtin owner path");
        assert_eq!(actual, root.join(relative_path));
        assert_eq!(
            fs::read_to_string(actual).expect("runtime manifest"),
            source
        );
        assert!(source.lines().any(|line| !line.trim().is_empty()));
    }
}

#[test]
fn builtin_module_ids_path_supports_m5_prefix() {
    let root = Path::new("/tmp/workspace");
    let path = builtin_module_ids_path_for("m5.gameplay.war.core", root).expect("m5 path");
    assert_eq!(path, root.join(M5_BUILTIN_MODULE_IDS_PATH));
}

#[test]
fn module_hash_index_roundtrip_keeps_latest_hash_per_module() {
    let temp_root = std::env::temp_dir().join(format!(
        "oasis7-materializer-index-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp root");

    persist_cached_module_hash(
        "m1.rule.move",
        "1111111111111111111111111111111111111111111111111111111111111111",
        &temp_root,
    )
    .expect("persist first hash");
    persist_cached_module_hash(
        "m1.rule.move",
        "2222222222222222222222222222222222222222222222222222222222222222",
        &temp_root,
    )
    .expect("persist updated hash");

    let cached = cached_module_hash_for("m1.rule.move", &temp_root).expect("cached hash");
    assert_eq!(
        cached,
        "2222222222222222222222222222222222222222222222222222222222222222"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn cached_expected_module_hash_ignores_stale_hashes() {
    let temp_root = std::env::temp_dir().join(format!(
        "oasis7-materializer-expected-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp root");

    let stale_hash = "1111111111111111111111111111111111111111111111111111111111111111";
    let expected_hash = "2222222222222222222222222222222222222222222222222222222222222222";
    persist_cached_module_hash("m5.gameplay.war.core", stale_hash, &temp_root)
        .expect("persist stale hash");

    assert!(
        cached_expected_module_hash_for("m5.gameplay.war.core", &[expected_hash], &temp_root)
            .is_none(),
        "stale cached hash should not bypass expected hash manifest"
    );
    assert_eq!(
        cached_expected_module_hash_for("m5.gameplay.war.core", &[stale_hash], &temp_root)
            .as_deref(),
        Some(stale_hash)
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn temp_build_dir_stays_under_repo_tmp_root() {
    let repo_root = temporary_repo("root");
    let temp_dir = temp_build_dir("m1.rule.move", &repo_root).expect("reserve build directory");
    assert!(
        temp_dir.starts_with(repo_root.join(".tmp")),
        "temp build dir should stay under repo .tmp, got {}",
        temp_dir.display()
    );
    assert!(temp_dir.is_dir());
    fs::remove_dir_all(repo_root).expect("remove fixture");
}

#[test]
fn same_clock_builds_keep_artifacts_when_another_call_cleans_up() {
    let repo_root = temporary_repo("same-clock");
    let sequence = AtomicU64::new(0);
    let first = temp_build_dir_at("m4.recipe.smelter.polymer_resin", &repo_root, 42, &sequence)
        .expect("reserve first build");
    let second = temp_build_dir_at("m4.recipe.smelter.polymer_resin", &repo_root, 42, &sequence)
        .expect("reserve second build with the same clock reading");

    fs::write(first.join("module.wasm"), b"first").expect("write first build");
    fs::write(second.join("module.wasm"), b"second").expect("write second build");
    fs::remove_dir_all(first).expect("clean completed first build");
    assert_eq!(
        fs::read(second.join("module.wasm")).expect("other build still owns its artifact"),
        b"second"
    );
    fs::remove_dir_all(repo_root).expect("remove fixture");
}

#[test]
fn existing_build_directory_is_not_claimed_or_cleaned() {
    let repo_root = temporary_repo("occupied");
    let occupied = temp_build_dir_at("m1.rule.move", &repo_root, 42, &AtomicU64::new(0))
        .expect("reserve existing build");
    fs::write(occupied.join("sentinel"), b"existing owner").expect("write existing artifact");

    let next = temp_build_dir_at("m1.rule.move", &repo_root, 42, &AtomicU64::new(0))
        .expect("reserve build after a counter restart with an occupied candidate");
    fs::write(next.join("module.wasm"), b"new build").expect("write new artifact");
    fs::remove_dir_all(next).expect("clean new build");
    assert_eq!(
        fs::read(occupied.join("sentinel")).expect("existing owner keeps its artifact"),
        b"existing owner"
    );
    fs::remove_dir_all(repo_root).expect("remove fixture");
}

#[test]
fn allocation_errors_reach_fetcher_and_both_compiler_entry_points() {
    let _env_lock = BUILTIN_WASM_ENV_LOCK.lock().expect("lock builtin wasm env");
    let repo_root = temporary_repo("blocked");
    fs::write(repo_root.join(".tmp"), b"not a directory").expect("block temporary root");
    let unavailable_command = repo_root.join("must-not-run");
    let fetcher_key = builtin_wasm_env_key("FETCHER");
    let _fetcher_guard = TestEnvGuard::capture(&fetcher_key);
    // SAFETY: All tests that change builtin WASM environment hold the same lock.
    unsafe {
        std::env::set_var(&fetcher_key, &unavailable_command);
    }

    assert!(matches!(
        try_fetch_via_fetcher("m1.rule.move", &["expected"], &repo_root),
        Err(MaterializeError::Io(_))
    ));
    assert!(matches!(
        compile_via_command(
            &unavailable_command,
            "m1.rule.move",
            &["expected"],
            &repo_root
        ),
        Err(MaterializeError::Io(_))
    ));
    assert!(matches!(
        compile_via_default_script("m1.rule.move", &["expected"], &repo_root),
        Err(MaterializeError::Io(_))
    ));
    assert_eq!(
        fs::read(repo_root.join(".tmp")).expect("preserve blocked root"),
        b"not a directory"
    );
    fs::remove_dir_all(repo_root).expect("remove fixture");
}

fn temporary_repo(label: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "oasis7-materializer-build-dir-{label}-{}-{now}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("create fixture repository");
    root
}

#[test]
fn builtin_wasm_env_non_empty_reads_oasis7_prefix() {
    let _env_lock = BUILTIN_WASM_ENV_LOCK.lock().expect("lock builtin wasm env");
    let primary_key = builtin_wasm_env_key("FETCHER");
    let _primary_guard = TestEnvGuard::capture(primary_key.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var(primary_key.as_str(), "/tmp/oasis7-fetcher");
    }

    assert_eq!(
        builtin_wasm_env_non_empty("FETCHER").as_deref(),
        Some("/tmp/oasis7-fetcher")
    );
}

#[test]
fn builtin_wasm_env_non_empty_rejects_removed_old_brand_prefix() {
    let _env_lock = BUILTIN_WASM_ENV_LOCK.lock().expect("lock builtin wasm env");
    let primary_key = builtin_wasm_env_key("FETCHER");
    let _primary_guard = TestEnvGuard::capture(primary_key.as_str());
    let removed_old_brand_key = removed_old_brand_builtin_wasm_env("FETCHER");
    let _removed_old_brand_guard = TestEnvGuard::capture(removed_old_brand_key.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::remove_var(primary_key.as_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var(
            removed_old_brand_key.as_str(),
            "/tmp/removed-old-brand-fetcher",
        );
    }

    assert!(builtin_wasm_env_non_empty("FETCHER").is_none());
}

#[test]
fn env_truthy_recognizes_ci_style_values() {
    let _env_lock = BUILTIN_WASM_ENV_LOCK.lock().expect("lock builtin wasm env");
    let _guard = TestEnvGuard::capture("CI");

    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var("CI", "true");
    }
    assert!(env_truthy("CI"));

    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var("CI", "0");
    }
    assert!(!env_truthy("CI"));
}

struct TestEnvGuard {
    key: String,
    previous: Option<String>,
}

fn removed_old_brand_builtin_wasm_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "BUILTIN", "WASM", suffix].join("_")
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
        match self.previous.take() {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    std::env::set_var(&self.key, value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    std::env::remove_var(&self.key);
                }
            }
        }
    }
}

#[test]
fn empty_expected_hashes_preserves_invalid_module_reason() {
    let error = load_builtin_wasm_with_fetch_fallback(
        "m9.test.empty",
        &[],
        Path::new("/unused/cas"),
        Path::new("/unused/repo"),
    )
    .expect_err("empty manifest rejected before any IO");
    assert!(
        matches!(error, MaterializeError::ModuleChangeInvalid { reason }
        if reason == "builtin wasm expected hash list is empty module_id=m9.test.empty")
    );
}

#[test]
fn verified_expected_cas_artifact_needs_no_fetch_or_compiler() {
    let root = std::env::temp_dir().join(format!(
        "oasis7-materializer-cas-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bytes = b"\0asmverified-artifact";
    let hash = sha256_hex(bytes);
    let store = LocalCasStore::new_with_hash_algorithm(&root, HashAlgorithm::Sha256);
    store.put(&hash, bytes).expect("store verified bytes");
    let loaded = load_builtin_wasm_with_fetch_fallback(
        "m9.test.cas",
        &[&hash],
        &root,
        Path::new("/unused/repo"),
    )
    .expect("verified CAS hit bypasses host IO");
    assert_eq!(loaded, bytes);
    fs::remove_dir_all(root).expect("remove fixture");
}
