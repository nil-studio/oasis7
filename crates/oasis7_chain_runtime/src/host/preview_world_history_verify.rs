//! Explicit operator verification; only scratch and an optional private error are writable.
use super::*;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone)]
pub(crate) struct PrivateDiagnostic {
    destination: PathBuf,
}
impl PrivateDiagnostic {
    pub(super) fn new(root: &Path, destination: PathBuf) -> Result<Self, String> {
        let value = Self { destination };
        value.validate(root)?;
        Ok(value)
    }
    #[cfg(unix)]
    fn validate(&self, root: &Path) -> Result<fs::File, String> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let parent = self
            .destination
            .parent()
            .ok_or("private diagnostic parent missing")?;
        let canonical = parent
            .canonicalize()
            .map_err(|_| "private diagnostic parent unavailable")?;
        if !self.destination.is_absolute()
            || self.destination.file_name().is_none()
            || canonical != parent
            || canonical.starts_with(
                root.canonicalize()
                    .map_err(|_| "history root unavailable")?,
            )
        {
            return Err("private diagnostic path rejected".into());
        }
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(parent)
            .map_err(|_| "private diagnostic parent rejected")?;
        let metadata = directory
            .metadata()
            .map_err(|_| "private diagnostic metadata unavailable")?;
        // SAFETY: geteuid has no inputs or memory effects.
        if !metadata.is_dir()
            || metadata.permissions().mode() & 0o777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("private diagnostic owner/mode rejected".into());
        }
        match fs::symlink_metadata(&self.destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(directory),
            _ => Err("private diagnostic destination must be absent".into()),
        }
    }
    #[cfg(not(unix))]
    fn validate(&self, _root: &Path) -> Result<fs::File, String> {
        Err("private diagnostics require Unix".into())
    }
    pub(super) fn record(
        &self,
        root: &Path,
        stage: &'static str,
        height: u64,
        error: impl std::fmt::Display,
    ) {
        #[cfg(unix)]
        let _ = (|| -> Result<(), String> {
            use std::os::{
                fd::{AsRawFd, FromRawFd},
                unix::ffi::OsStrExt,
            };
            let directory = self.validate(root)?;
            let name = std::ffi::CString::new(self.destination.file_name().unwrap().as_bytes())
                .map_err(|_| "private diagnostic name invalid")?;
            // SAFETY: directory owns a valid descriptor; name is NUL-terminated.
            // openat confines creation to the validated directory even if its pathname changes.
            let descriptor = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if descriptor < 0 {
                return Err("private diagnostic creation failed".into());
            }
            // SAFETY: the successful new descriptor is transferred exactly once.
            let mut file = unsafe { fs::File::from_raw_fd(descriptor) };
            {
                let mut bounded = BoundedWriter {
                    file: &mut file,
                    remaining: 16 * 1024,
                };
                write!(&mut bounded, "stage={stage} height={height}\n{error}")
                    .map_err(|_| "private diagnostic write failed")?;
            }
            file.sync_all()
                .map_err(|_| "private diagnostic sync failed".to_owned())
        })();
        #[cfg(not(unix))]
        let _ = (root, stage, height, error);
    }
}

#[cfg(unix)]
struct BoundedWriter<'a> {
    file: &'a mut fs::File,
    remaining: usize,
}
#[cfg(unix)]
impl Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let accepted = bytes.len().min(self.remaining);
        self.file.write_all(&bytes[..accepted])?;
        self.remaining -= accepted;
        // Formatting may continue, but no bytes beyond the fixed private cap are retained.
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

// Metadata-only admission before acquiring a writer lock. Origin contents are
// authenticated exactly once by reconstruct, inside its original replay budget.
fn validate_before_lock(root: &Path, replay: &ReplayBudget) -> Result<(), String> {
    if Instant::now() >= replay.deadline {
        return Err("preview history deadline exhausted".into());
    }
    if replay.max_total_bytes < ORIGIN_READ_RESERVATION {
        return Err("preview history byte budget exhausted".into());
    }
    if !root.is_absolute()
        || root
            .canonicalize()
            .map_err(|_| "history root unavailable")?
            != root
    {
        return Err("history root must be canonical and absolute".into());
    }
    let world = root.join("world");
    let metadata =
        fs::symlink_metadata(&world).map_err(|_| "history World directory unavailable")?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || world
            .canonicalize()
            .map_err(|_| "history World directory unavailable")?
            != world
    {
        return Err("history World directory rejected".into());
    }
    Ok(())
}

pub(crate) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut values = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "verify-preview-history --storage-profile dev_local --root <original> --scratch <absent> --world-id <id> --node-id <id> --service-public-key <independent-pin> --height <height> --max-replay-heights <n> --max-replay-bytes <n> --deadline-ms <n> [--operator-public-key <independent-local-pin>] [--private-diagnostic <absent-private-file>]"
            );
            return Ok(());
        }
        if !matches!(
            key,
            "--storage-profile"
                | "--root"
                | "--scratch"
                | "--world-id"
                | "--node-id"
                | "--service-public-key"
                | "--height"
                | "--max-replay-heights"
                | "--max-replay-bytes"
                | "--deadline-ms"
                | "--operator-public-key"
                | "--private-diagnostic"
        ) || values
            .insert(key, args.next().ok_or("history argument value missing")?)
            .is_some()
        {
            return Err("unknown or duplicate history verification argument".into());
        }
    }
    let value = |key| {
        values
            .get(key)
            .copied()
            .ok_or("explicit history verification argument missing")
    };
    let number = |key| {
        value(key)?
            .parse::<u64>()
            .map_err(|_| "history numeric argument invalid")
    };
    if value("--storage-profile")? != "dev_local" {
        return Err("history verification requires explicit DevLocal profile".into());
    }
    let root = PathBuf::from(value("--root")?);
    let scratch = PathBuf::from(value("--scratch")?);
    let world = value("--world-id")?;
    let node = value("--node-id")?;
    let pin = value("--service-public-key")?;
    let target = number("--height")?;
    let diagnostic = values
        .get("--private-diagnostic")
        .map(|path| PrivateDiagnostic::new(&root, PathBuf::from(*path)))
        .transpose()?;
    let local = values
        .get("--operator-public-key")
        .map(|key| {
            oasis7_runtime::DevLocalParticipantAuthority::new(
                oasis7_proto::storage_profile::StorageProfile::DevLocal,
                &ReleaseSecurityPolicy::default(),
                (*key).to_owned(),
            )
            .map_err(|_| "history local authority invalid")
        })
        .transpose()?;
    let replay = ReplayBudget {
        max_heights: number("--max-replay-heights")?,
        max_total_bytes: usize::try_from(number("--max-replay-bytes")?)
            .map_err(|_| "history byte budget invalid")?,
        deadline: Instant::now()
            .checked_add(std::time::Duration::from_millis(number("--deadline-ms")?))
            .ok_or("history deadline invalid")?,
        diagnostic_sink: diagnostic.clone(),
    };
    let outcome: Result<(), String> = (|| {
        validate_before_lock(&root, &replay)?;
        let _writer =
            super::super::world_writer_lock::acquire_live_world_writer_lock(&root.join("world"))?;
        let cut = reconstruct(&root, &scratch, world, node, pin, target, replay, local)?;
        if cut.commit.position != target || cut.commit.world.world_id != world {
            return Err("preview history verified world/height mismatch".into());
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        if let Some(sink) = diagnostic {
            sink.record(&root, "verification", target, &error);
        }
        return Err("preview history verification rejected; inspect explicit private diagnostic if supplied".into());
    }
    println!("preview_history_verified_height={target}");
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "preview-verify-private-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn private_diagnostic_is_bounded_private_and_never_overwrites() {
        let directory = Directory::new();
        let source = directory.0.join("source");
        fs::create_dir(&source).unwrap();
        let path = directory.0.join("error.txt");
        let sink = PrivateDiagnostic::new(&source, path.clone()).unwrap();
        assert!(!path.exists());
        sink.record(
            &source,
            "deterministic_execution",
            17,
            &"sensitive-local-detail".repeat(2000),
        );
        let original = fs::read(&path).unwrap();
        assert_eq!(original.len(), 16 * 1024);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        sink.record(&source, "verification", 18, "replacement forbidden");
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(PrivateDiagnostic::new(&source, path).is_err());
    }
    #[test]
    fn private_diagnostic_refuses_source_and_redirected_or_public_parents() {
        let directory = Directory::new();
        let source = directory.0.join("source");
        fs::DirBuilder::new().mode(0o700).create(&source).unwrap();
        assert!(PrivateDiagnostic::new(&source, source.join("error.txt")).is_err());
        let alias = directory.0.join("alias");
        symlink(&directory.0, &alias).unwrap();
        assert!(PrivateDiagnostic::new(&source, alias.join("error.txt")).is_err());
        let public = directory.0.join("public");
        fs::create_dir(&public).unwrap();
        fs::set_permissions(&public, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(PrivateDiagnostic::new(&source, public.join("error.txt")).is_err());
        let target = directory.0.join("existing");
        fs::write(&target, b"original").unwrap();
        let linked = directory.0.join("linked");
        symlink(&target, &linked).unwrap();
        assert!(PrivateDiagnostic::new(&source, linked).is_err());
        assert_eq!(fs::read(target).unwrap(), b"original");
    }
    #[test]
    fn verify_cli_requires_explicit_replay_inputs_and_rejects_publication_options() {
        for args in [
            vec![],
            vec!["--root", "/unused"],
            vec!["--max-store-bytes", "1"],
            vec!["--root", "/unused", "--root", "/different"],
        ] {
            assert!(run(args.into_iter()).is_err());
        }
    }
}

#[cfg(test)]
mod budget_admission_tests {
    use super::*;
    #[test]
    fn exhausted_budget_precedes_origin_io_and_lock_creation() {
        // Nonexistent origin cannot be read or locked. Exact budget failures
        // prove admission stops before even path metadata, not just generic CLI Err.
        let root =
            std::env::temp_dir().join(format!("absent-preview-origin-{}", std::process::id()));
        assert!(!root.exists());
        let mut replay = ReplayBudget {
            max_heights: 1,
            max_total_bytes: 0,
            deadline: Instant::now() + std::time::Duration::from_secs(1),
            diagnostic_sink: None,
        };
        assert_eq!(
            validate_before_lock(&root, &replay).unwrap_err(),
            "preview history byte budget exhausted"
        );
        replay.max_total_bytes = ORIGIN_READ_RESERVATION;
        replay.deadline = Instant::now();
        assert_eq!(
            validate_before_lock(&root, &replay).unwrap_err(),
            "preview history deadline exhausted"
        );
        assert!(!root.exists());
        assert!(!root.join("world.lock.guard").exists());
    }
}
