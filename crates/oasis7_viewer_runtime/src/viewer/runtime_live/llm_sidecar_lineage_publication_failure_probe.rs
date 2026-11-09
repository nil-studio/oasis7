//! Private fixture evidence at the actual failed rename, outside the Viewer mutex.
use std::path::{Path, PathBuf};
fn fixture(path: &Path) -> Option<(PathBuf, &'static str)> {
    let (root, prefix) = if std::env::var("PRE2_APP_ADMISSION").ok().as_deref()
        == Some("memory-ack-write-failure")
    {
        (std::env::var_os("PRE2_METADATA_DIR")?, "feedback-ack")
    } else if let Some(root) = std::env::var_os("PRE2_WAIT_CAPTURE_FS_ROOT") {
        (root, "wait-capture")
    } else if let Some(root) = std::env::var_os("PRE2_WAIT_REJECTION_FS_ROOT") {
        (root, "wait-rejection")
    } else if let Some(root) = std::env::var_os("PRE2_RESUME_HANDOFF_FS_ROOT") {
        (root, "resume-handoff")
    } else if let Some(root) = std::env::var_os("PRE2_RESUME_REJECTION_FS_ROOT") {
        (root, "resume-rejection")
    } else if let Some(root) = std::env::var_os("PRE2_FINAL_BUDGET_FS_ROOT") {
        (root, "final-budget")
    } else {
        (
            std::env::var_os("PRE2_WAIT_CLEANUP_FS_ROOT")?,
            "wait-cleanup",
        )
    };
    let root = PathBuf::from(root);
    (path.parent() == Some(root.as_path())).then_some((root, prefix))
}
use std::time::{Duration, Instant};
pub(super) fn observe_unpublished(
    path: &Path,
    temporary: &Path,
    encoded: &[u8],
) -> Result<(), String> {
    let Some((root, prefix)) = fixture(path) else {
        return Ok(());
    };
    if !root.join(format!("{prefix}-before-persist")).is_file() {
        return Ok(());
    }
    let marker = root.join(format!("{prefix}-unpublished"));
    if marker.exists() {
        return Ok(());
    }
    let actual = std::fs::read(temporary).map_err(|_| "test unpublished original unavailable")?;
    if actual != encoded {
        return Err("test unpublished original bytes changed".into());
    }
    let captured = root.join(format!("{prefix}-publication-original.json"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options
        .open(&captured)
        .and_then(|mut file| file.write_all(&actual))
        .map_err(|_| "test unpublished private evidence write failed")?;
    std::fs::write(
        &marker,
        b"actual rename IsADirectory; same original retained",
    )
    .map_err(|_| "test unpublished marker write failed")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join(format!("{prefix}-retry-release")).is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    if !root.join(format!("{prefix}-retry-release")).is_file() {
        return Err("test unpublished repair release absent".into());
    }
    if std::fs::read(temporary).map_err(|_| "test unpublished original lost")? != actual {
        return Err("test repair changed original temporary".into());
    }
    Ok(())
}

pub(super) fn observe_published(path: &Path, encoded: &[u8]) -> Result<(), String> {
    let Some((root, prefix)) = fixture(path) else {
        return Ok(());
    };
    if !root.join(format!("{prefix}-unpublished")).is_file() {
        return Ok(());
    }
    let original = std::fs::read(root.join(format!("{prefix}-publication-original.json")))
        .map_err(|_| "test original captured checkpoint missing")?;
    if original != encoded {
        return Ok(());
    }
    if std::fs::read(path).map_err(|_| "test published original missing")? != original {
        return Err("test published original bytes differ".into());
    }
    std::fs::write(
        root.join(format!("{prefix}-original-published")),
        b"exact original bytes published after repair",
    )
    .map_err(|_| "test original publication marker failed".into())
}

/// Select the exact Wait capture, not an earlier queued outcome in its batch.
pub(super) fn before_rename(path: &Path, encoded: &[u8]) -> Result<(), String> {
    let Some((root, prefix)) = fixture(path) else {
        return Ok(());
    };
    if prefix != "wait-capture"
        || !root.join("wait-capture-prepared").is_file()
        || root.join("wait-capture-before-persist").is_file()
    {
        return Ok(());
    }
    let staged: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("wait-capture-staged.json"))
            .map_err(|_| "test capture staged snapshot missing")?,
    )
    .map_err(|_| "test capture staged snapshot invalid")?;
    let actual: serde_json::Value =
        serde_json::from_slice(encoded).map_err(|_| "test capture encoded original invalid")?;
    if actual["hosted_wait"] != staged["wait"]
        || actual["hosted_wait"].is_null()
        || actual["provider_completed_decisions"] != staged["completed_queue"]
    {
        return Ok(());
    }
    let previous: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "test original queue checkpoint not published")?,
    )
    .map_err(|_| "test original queue checkpoint invalid")?;
    if previous["provider_completed_decisions"]
        .as_array()
        .map(Vec::len)
        != Some(1)
        || !previous["hosted_wait"].is_null()
        || previous["provider_completed_decisions"] != staged["completed_queue"]
    {
        return Err("test capture boundary must follow the exact published original queue".into());
    }
    std::fs::write(
        root.join("wait-capture-before-persist"),
        b"exact captured Wait after original queue publication",
    )
    .map_err(|_| "test capture boundary marker failed")?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join("wait-capture-release").is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    if !root.join("wait-capture-release").is_file() {
        return Err("test capture filesystem release absent".into());
    }
    Ok(())
}
