//! Environment cases run in separate test processes, never mutate shared test env.
use super::IdentityConfig;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path =
            std::env::temp_dir().join(format!("oasis7-identity-config-{}", hex::encode(nonce)));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn probe(flag: Option<&str>, sqlite: Option<&Path>, table: Option<&str>, reject: bool) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .args([
            "--ignored",
            "--exact",
            "hosted_identity::config::config_tests::identity_v2_disabled_config_probe",
            "--nocapture",
        ])
        .env(
            "OASIS7_CONFIG_PROBE_EXPECT_REJECT",
            if reject { "1" } else { "0" },
        );
    if let Some(flag) = flag {
        command.env("OASIS7_HOSTED_IDENTITY_V2_ENABLED", flag);
    }
    if let Some(path) = sqlite {
        command.env("OASIS7_HOSTED_IDENTITY_SQLITE_PATH", path);
    }
    if let Some(table) = table {
        command.env("OASIS7_HOSTED_IDENTITY_TABLE_V2", table);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "isolated config probe failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
#[ignore = "only invoked by isolated config regression parent"]
fn identity_v2_disabled_config_probe() {
    let expect_reject = std::env::var("OASIS7_CONFIG_PROBE_EXPECT_REJECT").unwrap() == "1";
    match IdentityConfig::from_env() {
        Err(error) if expect_reject => {
            assert!(error.contains("keep identity v2 enabled"));
            assert!(error.contains("close registration"));
        }
        Ok(None) if !expect_reject => {}
        _ => panic!("disabled v2 startup did not preserve the expected storage boundary"),
    }
}

#[test]
fn disabled_v2_rejects_existing_sqlite_for_absent_and_off_flags() {
    let root = TestDirectory::new();
    let db = root.path().join("identity.sqlite");
    std::fs::write(&db, b"existing identity authority").unwrap();
    for flag in [None, Some("0"), Some("false")] {
        probe(flag, Some(&db), None, true);
    }
}

#[test]
fn disabled_v2_rejects_configured_table_without_backend_or_credentials() {
    for flag in [None, Some("0")] {
        probe(flag, None, Some("identity_v2"), true);
    }
}

#[test]
fn disabled_v2_preserves_entirely_unconfigured_legacy() {
    for flag in [None, Some("0"), Some("false"), Some("")] {
        probe(flag, None, None, false);
    }
    probe(Some("0"), None, Some("  "), false);
}

#[test]
fn disabled_v2_allows_explicit_path_that_does_not_exist() {
    let root = TestDirectory::new();
    probe(
        None,
        Some(&root.path().join("not-created.sqlite")),
        None,
        false,
    );
}

#[cfg(unix)]
#[test]
fn disabled_v2_rejects_sqlite_existence_check_errors() {
    let root = TestDirectory::new();
    let path = root.path().join("symlink-loop");
    std::os::unix::fs::symlink(&path, &path).unwrap();
    assert!(path.try_exists().is_err());
    probe(None, Some(&path), None, true);
}

#[test]
fn enabled_v2_with_closed_registration_keeps_existing_sqlite() {
    let root = TestDirectory::new();
    let path = root.path().join("identity.sqlite");
    std::fs::write(&path, b"existing identity authority").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .env_clear()
        .args([
            "--ignored",
            "--exact",
            "hosted_identity::config::config_tests::identity_v2_enabled_config_probe",
            "--nocapture",
        ])
        .env("OASIS7_HOSTED_IDENTITY_V2_ENABLED", "1")
        .env("OASIS7_HOSTED_IDENTITY_SQLITE_PATH", path)
        .env("OASIS7_HOSTED_WEBAUTHN_RP_ID", "localhost")
        .env("OASIS7_HOSTED_WEBAUTHN_ORIGINS", "https://localhost")
        .env("OASIS7_HOSTED_IDENTITY_SESSION_KEY", "00".repeat(32))
        .env("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH", "1")
        .env("OASIS7_HOSTED_IDENTITY_SERVICE_ID", "config-test")
        .env("OASIS7_HOSTED_IDENTITY_ENVIRONMENT", "test")
        .env("OASIS7_HOSTED_WORLD_IDENTITY_SCOPE", "fixture-world")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
#[ignore = "only invoked by isolated config regression parent"]
fn identity_v2_enabled_config_probe() {
    let config = IdentityConfig::from_env().unwrap().unwrap();
    assert!(!config.registration_enabled);
    assert!(config.sqlite_path.try_exists().unwrap());
}

#[test]
fn invalid_v2_flags_fail_closed_before_legacy_startup() {
    for flag in ["true", "TRUE", " 1 ", "01", "yes", "FALSE", "  "] {
        let output = Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .args([
                "--ignored",
                "--exact",
                "hosted_identity::config::config_tests::identity_v2_invalid_flag_probe",
                "--nocapture",
            ])
            .env("OASIS7_HOSTED_IDENTITY_V2_ENABLED", flag)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "invalid flag {flag:?}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}

#[test]
#[ignore = "only invoked by isolated config regression parent"]
fn identity_v2_invalid_flag_probe() {
    assert!(
        oasis7_client_api::viewer_auth::hosted_identity_v2_enabled(),
        "standalone runtime must retain fail-closed v2 authorization for invalid configuration"
    );
    let error = IdentityConfig::from_env()
        .err()
        .expect("invalid v2 flag must reject startup before legacy endpoints become available");
    assert!(error.contains("OASIS7_HOSTED_IDENTITY_V2_ENABLED"));
    assert!(error.contains("invalid identity v2 flag"));
}
