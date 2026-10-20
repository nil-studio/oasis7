#[cfg(test)]
use super::super::local_io as io;
use super::{LOCAL_NATIVE_PROFILE, PROFILE, PackageUpgradeV1};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, File},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
};

const MAX_BUILDINFO_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RuntimeFileSnapshot {
    pub path: PathBuf,
    pub sha256: String,
    pub device: u64,
    pub inode: u64,
}

#[derive(PartialEq, Eq)]
struct CurrentLinkSnapshot {
    target: PathBuf,
    device: u64,
    inode: u64,
    mode: libc::mode_t,
}

pub(super) fn validate(request: &PackageUpgradeV1) -> Result<(), String> {
    validated_current_runtime(request).map(|_| ())
}

/// Resolve the one package publication symlink after validating its exact
/// direct-release target. The rest of the path is inspected through the
/// captured canonical release path so `io::regular` rejects any internal
/// symlink without resolving `current` a second time.
#[cfg(test)]
pub(super) fn current_runtime_path(request: &PackageUpgradeV1) -> Result<PathBuf, String> {
    validated_current_runtime(request).map(|snapshot| snapshot.path)
}

pub(super) fn current_runtime_snapshot(
    request: &PackageUpgradeV1,
) -> Result<RuntimeFileSnapshot, String> {
    validated_current_runtime(request)
}

/// Open the selected runtime through retained directory descriptors. The only
/// allowed symlink is the `current` publication pointer; `releases`, the
/// selected release, `bin`, the executable, and BUILDINFO are opened with
/// `O_NOFOLLOW` relative to their already-open parent.
fn validated_current_runtime(request: &PackageUpgradeV1) -> Result<RuntimeFileSnapshot, String> {
    if !matches!(request.profile.as_str(), PROFILE | LOCAL_NATIVE_PROFILE) {
        return Err("unsupported package deployment profile".into());
    }
    if !request.node_root.is_absolute()
        || fs::canonicalize(&request.node_root).map_err(|error| error.to_string())?
            != request.node_root
    {
        return Err("package node root must use its canonical absolute path".into());
    }
    let node_root = open_directory_path(&request.node_root)?;
    let first_current = read_current_link(&node_root)?;
    let first_target = &first_current.target;
    let releases_path = request.node_root.join("releases");
    let target_path = if first_target.is_absolute() {
        first_target.clone()
    } else {
        request.node_root.join(first_target)
    };
    if target_path.parent() != Some(releases_path.as_path()) {
        return Err("package current must resolve to one direct prior release".into());
    }
    let release_name = target_path
        .file_name()
        .ok_or("package current release has no final path component")?;
    if release_name == "." || release_name == ".." {
        return Err("package current has an invalid prior release name".into());
    }
    let release = target_path.clone();
    let releases = open_child_directory(&node_root, std::ffi::OsStr::new("releases"))
        .map_err(|error| format!("package prior releases directory is unavailable: {error}"))?;
    let release_dir = open_child_directory(&releases, release_name)
        .map_err(|error| format!("package prior release directory is unavailable: {error}"))?;
    let bin = open_child_directory(&release_dir, std::ffi::OsStr::new("bin"))
        .map_err(|error| format!("package prior runtime bin directory is unavailable: {error}"))?;
    let mut runtime = open_child_file(&bin, std::ffi::OsStr::new("oasis7_chain_runtime"))
        .map_err(|error| format!("package prior runtime symlink or file refused: {error}"))?;
    let mut buildinfo = open_child_file(&release_dir, std::ffi::OsStr::new("BUILDINFO"))
        .map_err(|error| format!("package prior BUILDINFO symlink or file refused: {error}"))?;
    let runtime_before = runtime.metadata().map_err(|error| error.to_string())?;
    if !runtime_before.is_file() || runtime_before.mode() & 0o111 == 0 {
        return Err("package prior runtime is not a regular executable file".into());
    }
    let runtime_digest = hash_file(&mut runtime)?;
    let runtime_after = runtime.metadata().map_err(|error| error.to_string())?;
    if !same_file_state(&runtime_before, &runtime_after) {
        return Err("package prior runtime changed while hashing its open descriptor".into());
    }
    let buildinfo_before = buildinfo.metadata().map_err(|error| error.to_string())?;
    if !buildinfo_before.is_file() || buildinfo_before.len() > MAX_BUILDINFO_BYTES as u64 {
        return Err("package prior BUILDINFO is not a bounded regular file".into());
    }
    let buildinfo_bytes = read_bounded(&mut buildinfo, MAX_BUILDINFO_BYTES)?;
    let buildinfo_after = buildinfo.metadata().map_err(|error| error.to_string())?;
    if !same_file_state(&buildinfo_before, &buildinfo_after) {
        return Err("package prior BUILDINFO changed while reading its open descriptor".into());
    }
    let fields = read_buildinfo(&buildinfo_bytes)?;
    if read_current_link(&node_root)? != first_current {
        return Err("package current changed while reading the prior release".into());
    }
    named_component_matches_fd(
        &node_root,
        std::ffi::OsStr::new("releases"),
        &releases,
        "releases",
    )?;
    named_component_matches_fd(&releases, release_name, &release_dir, "prior release")?;
    named_component_matches_fd(
        &release_dir,
        std::ffi::OsStr::new("bin"),
        &bin,
        "runtime bin",
    )?;
    named_component_matches_fd(
        &bin,
        std::ffi::OsStr::new("oasis7_chain_runtime"),
        &runtime,
        "runtime",
    )?;
    named_component_matches_fd(
        &release_dir,
        std::ffi::OsStr::new("BUILDINFO"),
        &buildinfo,
        "BUILDINFO",
    )?;
    let root_after = open_directory_path(&request.node_root)?;
    let root_before_metadata = node_root.metadata().map_err(|error| error.to_string())?;
    let root_after_metadata = root_after.metadata().map_err(|error| error.to_string())?;
    if root_before_metadata.dev() != root_after_metadata.dev()
        || root_before_metadata.ino() != root_after_metadata.ino()
    {
        return Err("package node root path changed while reading the prior release".into());
    }
    let expected_platform = if request.profile == LOCAL_NATIVE_PROFILE {
        "local-native"
    } else {
        "linux-x64"
    };
    if fields.get("platform").map(String::as_str) != Some(expected_platform)
        || fields.get("runtime_sha256").map(String::as_str) != Some(runtime_digest.as_str())
        || ["package_version", "commit", "run_id"].iter().any(|name| {
            fields
                .get(*name)
                .is_none_or(|value| value.trim().is_empty())
        })
    {
        return Err("package current does not identify a valid prior release runtime".into());
    }
    Ok(RuntimeFileSnapshot {
        path: release.join("bin/oasis7_chain_runtime"),
        sha256: runtime_digest,
        device: runtime_before.dev(),
        inode: runtime_before.ino(),
    })
}

pub(super) fn file_snapshot(path: &Path) -> Result<RuntimeFileSnapshot, String> {
    let canonical = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let mut file = open_absolute_file(&canonical)?;
    let before = file.metadata().map_err(|error| error.to_string())?;
    if !before.is_file() {
        return Err(format!(
            "package executable is not a regular file: {}",
            path.display()
        ));
    }
    let sha256 = hash_file(&mut file)?;
    let after = file.metadata().map_err(|error| error.to_string())?;
    if !same_file_state(&before, &after) {
        return Err(format!(
            "package executable changed while hashing: {}",
            path.display()
        ));
    }
    Ok(RuntimeFileSnapshot {
        path: canonical,
        sha256,
        device: before.dev(),
        inode: before.ino(),
    })
}

pub(super) fn loaded_image_snapshot() -> Result<RuntimeFileSnapshot, String> {
    let mut image = crate::executable_image::open_loaded_image()?;
    crate::executable_image::validate_loaded_file(&image)?;
    let before = image.metadata().map_err(|error| error.to_string())?;
    let path = fs::canonicalize(std::env::current_exe().map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let named_before = file_snapshot(&path)?;
    if named_before.device != before.dev() || named_before.inode != before.ino() {
        return Err("loaded executable path differs from the actual process image".into());
    }
    let sha256 = hash_file(&mut image)?;
    let after = image.metadata().map_err(|error| error.to_string())?;
    crate::executable_image::validate_loaded_file(&image)?;
    if !same_file_state(&before, &after) {
        return Err("loaded executable changed while its image descriptor was hashed".into());
    }
    if file_snapshot(&path)? != named_before {
        return Err("loaded executable pathname changed while the image was hashed".into());
    }
    Ok(RuntimeFileSnapshot {
        path,
        sha256,
        device: before.dev(),
        inode: before.ino(),
    })
}

fn named_component_matches_fd(
    parent: &File,
    name: &std::ffi::OsStr,
    opened: &File,
    description: &str,
) -> Result<(), String> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| format!("invalid package {description} path component"))?;
    // SAFETY: parent is a live directory fd and name is a NUL-terminated
    // single path component. AT_SYMLINK_NOFOLLOW checks the directory entry.
    let named = unsafe {
        let mut value = std::mem::MaybeUninit::<libc::stat>::zeroed();
        if libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) < 0
        {
            return Err(format!(
                "package {description} pathname changed after it was opened: {}",
                std::io::Error::last_os_error()
            ));
        }
        value.assume_init()
    };
    let opened = opened.metadata().map_err(|error| error.to_string())?;
    if named.st_dev as u64 != opened.dev()
        || named.st_ino as u64 != opened.ino()
        || named.st_mode & libc::S_IFMT != opened.mode() as libc::mode_t & libc::S_IFMT
    {
        return Err(format!(
            "package {description} pathname no longer names the opened file"
        ));
    }
    Ok(())
}

fn read_current_link(node_root: &File) -> Result<CurrentLinkSnapshot, String> {
    let current = CString::new("current").expect("static C string");
    let mut buffer = vec![0u8; 4096];
    // SAFETY: current is NUL-terminated and node_root is a live directory fd.
    let metadata = unsafe {
        let mut value = std::mem::MaybeUninit::<libc::stat>::zeroed();
        if libc::fstatat(
            node_root.as_raw_fd(),
            current.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) < 0
        {
            return Err(format!(
                "package stage requires current to be a symlink to a valid prior release: {}",
                std::io::Error::last_os_error()
            ));
        }
        value.assume_init()
    };
    if metadata.st_mode & libc::S_IFMT != libc::S_IFLNK {
        return Err(
            "package stage requires current to be a symlink to a valid prior release".into(),
        );
    }
    // SAFETY: current is NUL-terminated, node_root is an open directory, and
    // buffer is writable for its full declared length.
    let size = unsafe {
        libc::readlinkat(
            node_root.as_raw_fd(),
            current.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if size < 0 {
        return Err(format!(
            "package stage requires current to be a symlink to a valid prior release: {}",
            std::io::Error::last_os_error()
        ));
    }
    if size as usize == buffer.len() {
        return Err("package current symlink target exceeds its bound".into());
    }
    buffer.truncate(size as usize);
    // Re-read the link inode after readlinkat so a replaced publication link
    // cannot pair the original metadata with a different target.
    let metadata_after = unsafe {
        let mut value = std::mem::MaybeUninit::<libc::stat>::zeroed();
        if libc::fstatat(
            node_root.as_raw_fd(),
            current.as_ptr(),
            value.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) < 0
        {
            return Err(format!(
                "package current symlink changed while reading: {}",
                std::io::Error::last_os_error()
            ));
        }
        value.assume_init()
    };
    if metadata.st_dev != metadata_after.st_dev
        || metadata.st_ino != metadata_after.st_ino
        || metadata.st_mode != metadata_after.st_mode
    {
        return Err("package current changed while reading its publication symlink".into());
    }
    Ok(CurrentLinkSnapshot {
        target: PathBuf::from(std::ffi::OsStr::from_bytes(&buffer)),
        device: metadata.st_dev as u64,
        inode: metadata.st_ino as u64,
        mode: metadata.st_mode,
    })
}

fn open_directory_path(path: &Path) -> Result<File, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err("package path must be absolute and normalized".into());
    }
    let mut current = File::open("/").map_err(|error| error.to_string())?;
    for component in path.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        current = open_child_directory(&current, name).map_err(|error| {
            format!(
                "package path component {} is not a real directory: {error}",
                name.to_string_lossy()
            )
        })?;
    }
    Ok(current)
}

fn open_absolute_file(path: &Path) -> Result<File, String> {
    let name = path
        .file_name()
        .ok_or("package executable path has no filename")?;
    let parent = path
        .parent()
        .ok_or("package executable path has no parent")?;
    let directory = open_directory_path(parent)?;
    open_child_file(&directory, name).map_err(|error| {
        format!("package executable cannot be opened without following symlinks: {error}")
    })
}

fn open_child_directory(parent: &File, name: &std::ffi::OsStr) -> std::io::Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: parent is a live directory fd and name is a NUL-terminated
    // single component. O_NOFOLLOW rejects a substituted symlink.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a uniquely owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_child_file(parent: &File, name: &std::ffi::OsStr) -> std::io::Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: parent is a live directory fd and name is a NUL-terminated
    // single component. O_NOFOLLOW rejects a substituted symlink.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a uniquely owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn hash_file(file: &mut File) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn read_bounded(file: &mut File, maximum: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum {
        return Err("package prior BUILDINFO exceeds its byte bound".into());
    }
    Ok(bytes)
}

fn same_file_state(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.mode() == right.mode()
        && left.len() == right.len()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

fn read_buildinfo(contents: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let contents = std::str::from_utf8(contents).map_err(|error| error.to_string())?;
    let mut fields = BTreeMap::new();
    for (line_number, line) in contents.lines().enumerate() {
        let (name, value) = line.split_once('=').ok_or_else(|| {
            format!(
                "prior release BUILDINFO line {} is malformed",
                line_number + 1
            )
        })?;
        if name.trim().is_empty() || fields.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err("prior release BUILDINFO has an empty or duplicate field".into());
        }
    }
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::{
        LOCAL_NATIVE_PROFILE, PackageUpgradeV1, current_runtime_path, current_runtime_snapshot, io,
        named_component_matches_fd, open_directory_path,
    };
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        path::{Path, PathBuf},
    };

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "oasis7-package-current-link-{}-{}",
                std::process::id(),
                io::random_id().unwrap()
            ));
            io::private_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(node_root: PathBuf) -> PackageUpgradeV1 {
        PackageUpgradeV1 {
            api_version: "oasis7.local-package-upgrade-request/v1".into(),
            profile: LOCAL_NATIVE_PROFILE.into(),
            node_root,
            helper_script: Some(PathBuf::from("/bin/true")),
            helper_sha256: Some("a".repeat(64)),
            package_deb: None,
            package_deb_sha256: None,
            ops_tools_tar: None,
            ops_tools_tar_sha256: None,
            native_artifact: None,
            candidate_chain_config: None,
            package_version: "test".into(),
            commit: "test".into(),
            run_id: "test".into(),
            artifact_ref: None,
        }
    }

    fn write_release(release: &Path, runtime_bytes: &[u8]) -> PathBuf {
        let runtime = release.join("bin/oasis7_chain_runtime");
        fs::create_dir_all(runtime.parent().unwrap()).unwrap();
        fs::write(&runtime, runtime_bytes).unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
        let runtime_sha256 = io::sha256(&runtime).unwrap();
        fs::write(
            release.join("BUILDINFO"),
            format!(
                "platform=local-native\npackage_version=prior\ncommit=prior-commit\nrun_id=prior-run\nruntime_sha256={runtime_sha256}\n"
            ),
        )
        .unwrap();
        runtime
    }

    #[test]
    fn current_publication_symlink_resolves_only_after_direct_release_validation() {
        let root = TestDir::new();
        let node_root = root.0.join("native-package");
        io::private_dir(&node_root).unwrap();
        let release = node_root.join("releases/prior");
        let runtime = write_release(&release, b"prior runtime");
        symlink(Path::new("releases/prior"), node_root.join("current")).unwrap();

        let resolved = current_runtime_path(&request(node_root)).unwrap();

        assert_eq!(resolved, fs::canonicalize(runtime).unwrap());
    }

    #[test]
    fn current_runtime_snapshot_binds_the_open_executable_identity() {
        use std::os::unix::fs::MetadataExt;

        let root = TestDir::new();
        let node_root = root.0.join("native-package");
        io::private_dir(&node_root).unwrap();
        let release = node_root.join("releases/prior");
        let runtime = write_release(&release, b"prior runtime");
        symlink(Path::new("releases/prior"), node_root.join("current")).unwrap();

        let snapshot = current_runtime_snapshot(&request(node_root)).unwrap();
        let canonical_runtime = fs::canonicalize(&runtime).unwrap();
        let metadata = fs::metadata(&canonical_runtime).unwrap();

        assert_eq!(snapshot.path, canonical_runtime);
        assert_eq!(snapshot.sha256, io::sha256(&runtime).unwrap());
        assert_eq!(snapshot.device, metadata.dev());
        assert_eq!(snapshot.inode, metadata.ino());
    }

    #[test]
    fn current_publication_must_not_traverse_release_path_components() {
        let root = TestDir::new();
        let node_root = root.0.join("native-package");
        io::private_dir(&node_root).unwrap();
        let release = node_root.join("releases/prior");
        write_release(&release, b"prior runtime");
        symlink(
            Path::new("releases/../releases/prior"),
            node_root.join("current"),
        )
        .unwrap();

        let error = current_runtime_path(&request(node_root)).unwrap_err();

        assert!(error.contains("direct prior release"), "{error}");
    }

    #[test]
    fn opened_component_is_rejected_when_its_name_is_replaced() {
        let root = TestDir::new();
        let parent = root.0.join("parent");
        io::private_dir(&parent).unwrap();
        let path = parent.join("runtime");
        fs::write(&path, b"opened runtime").unwrap();
        let opened = fs::File::open(&path).unwrap();
        let replacement = parent.join("replacement");
        fs::write(&replacement, b"replacement runtime").unwrap();
        fs::rename(&replacement, &path).unwrap();
        let parent_fd = open_directory_path(&parent).unwrap();

        let error = named_component_matches_fd(
            &parent_fd,
            std::ffi::OsStr::new("runtime"),
            &opened,
            "runtime",
        )
        .unwrap_err();

        assert!(error.contains("pathname no longer names"), "{error}");
    }

    #[test]
    fn current_release_internal_runtime_symlink_remains_forbidden() {
        let root = TestDir::new();
        let node_root = root.0.join("native-package");
        io::private_dir(&node_root).unwrap();
        let release = node_root.join("releases/prior");
        let external_runtime = root.0.join("external-runtime");
        fs::write(&external_runtime, b"prior runtime").unwrap();
        fs::set_permissions(&external_runtime, fs::Permissions::from_mode(0o755)).unwrap();
        let runtime = release.join("bin/oasis7_chain_runtime");
        fs::create_dir_all(runtime.parent().unwrap()).unwrap();
        symlink(&external_runtime, &runtime).unwrap();
        let runtime_sha256 = io::sha256(&external_runtime).unwrap();
        fs::write(
            release.join("BUILDINFO"),
            format!(
                "platform=local-native\npackage_version=prior\ncommit=prior-commit\nrun_id=prior-run\nruntime_sha256={runtime_sha256}\n"
            ),
        )
        .unwrap();
        symlink(Path::new("releases/prior"), node_root.join("current")).unwrap();

        let error = current_runtime_path(&request(node_root)).unwrap_err();

        assert!(
            error.contains("package prior runtime symlink or file refused"),
            "{error}"
        );
        assert!(
            fs::symlink_metadata(&runtime)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the rejected runtime component must be the selected release's symlink"
        );
    }

    #[test]
    fn current_release_directory_symlink_is_not_followed() {
        let root = TestDir::new();
        let node_root = root.0.join("native-package");
        io::private_dir(&node_root).unwrap();
        let real_release = node_root.join("releases/prior-real");
        write_release(&real_release, b"prior runtime");
        symlink("prior-real", node_root.join("releases/prior-link")).unwrap();
        symlink(Path::new("releases/prior-link"), node_root.join("current")).unwrap();

        let error = current_runtime_path(&request(node_root)).unwrap_err();

        assert!(error.contains("prior release directory"), "{error}");
    }
}
