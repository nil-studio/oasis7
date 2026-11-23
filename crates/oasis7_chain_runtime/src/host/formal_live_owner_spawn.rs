//! Exact owner-worker command construction for the production CLI and the
//! registered test child. Lifecycle, diagnostics and spawning stay in the owner.
#[cfg(test)]
use super::{
    TEST_WORKER_CONFIG, TEST_WORKER_CONFIG_DIGEST, TEST_WORKER_OPERATION,
    TEST_WORKER_REQUEST_DIGEST,
};
use std::{path::Path, process::Command};

#[cfg(not(test))]
pub(super) fn command(
    executable: &Path,
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_read_trust_digest: Option<&str>,
) -> Result<Command, String> {
    let mut command = Command::new(executable);
    command.args([
        "op",
        "formal-owner-worker",
        "--config",
        config_path
            .to_str()
            .ok_or("non-UTF8 formal runtime config path")?,
        "--operation-id",
        operation_id,
        "--request-digest",
        request_digest,
        "--config-digest",
        expected_config_digest,
    ]);
    if let Some(digest) = expected_read_trust_digest {
        command.args(["--expected-formal-read-trust-digest", digest]);
    }
    Ok(command)
}

#[cfg(test)]
pub(super) fn command(
    executable: &Path,
    config_path: &Path,
    operation_id: &str,
    request_digest: &str,
    expected_config_digest: &str,
    expected_read_trust_digest: Option<&str>,
) -> Result<Command, String> {
    // Keep the real registered owner loop in its own test child process.
    let mut command = Command::new(executable);
    let test_name = owner_worker_test_name(executable)?;
    command
        .args(["--ignored", "--exact", &test_name, "--nocapture"])
        .env(TEST_WORKER_CONFIG, config_path)
        .env(TEST_WORKER_OPERATION, operation_id)
        .env(TEST_WORKER_REQUEST_DIGEST, request_digest)
        .env(TEST_WORKER_CONFIG_DIGEST, expected_config_digest);
    if let Some(digest) = expected_read_trust_digest {
        command.env("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST", digest);
    } else {
        command.env_remove("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST");
    }
    Ok(command)
}

#[cfg(test)]
fn owner_worker_test_name(executable: &Path) -> Result<String, String> {
    let output = Command::new(executable)
        .arg("--list")
        .output()
        .map_err(|error| format!("list formal owner test worker: {error}"))?;
    if !output.status.success() {
        return Err("formal owner test worker discovery failed".into());
    }
    let listing = String::from_utf8(output.stdout)
        .map_err(|error| format!("formal owner test worker listing is not UTF-8: {error}"))?;
    let mut matches = listing.lines().filter_map(|line| {
        line.strip_suffix(": test")
            .filter(|name| name.ends_with("::protected_handoff_owner_worker"))
    });
    let name = matches
        .next()
        .ok_or("formal owner test worker is not registered")?;
    if matches.next().is_some() {
        return Err("multiple formal owner test workers are registered".into());
    }
    Ok(name.to_owned())
}
