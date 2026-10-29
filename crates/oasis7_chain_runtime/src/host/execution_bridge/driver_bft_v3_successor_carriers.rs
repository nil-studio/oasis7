//! Read-only installed successor evidence bound to the original ledger carriers.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_prefix_files as files;
use super::successor_archive::{MAX_METADATA, decode, err, live};
use super::successor_install::{DIRECTORY, InstalledPointerV1};
use super::*;
use oasis7_distfs::protected_bft_v3::{BftSignerStateV3, BftSuccessorWindowStoredRequestV1};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Component;
use std::time::Instant;

pub(super) struct InstallationRootV1 {
    path: PathBuf,
    root: File,
    prefix: File,
}
impl InstallationRootV1 {
    pub(super) fn new(path: PathBuf) -> Result<Self, String> {
        let root = open_absolute_directory(&path)?;
        private_directory(&root)?;
        let prefix = open_child(&root, storage::DIRECTORY, true)?;
        private_directory(&prefix)?;
        let pin = Self { path, root, prefix };
        pin.verify_directories()?;
        Ok(pin)
    }
    fn verify_directories(&self) -> Result<(), String> {
        let current = open_absolute_directory(&self.path)?;
        private_directory(&current)?;
        same_directory(&self.root, &current)?;
        let prefix = open_child(&current, storage::DIRECTORY, true)?;
        private_directory(&prefix)?;
        same_directory(&self.prefix, &prefix)
    }
    pub(super) fn read_installed(
        &self,
        origin: &storage::Origin,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<InstalledPointerV1, String> {
        live(deadline)?;
        self.verify_directories()?;
        let format = open_child(&self.root, "bft-format.lock", false)?;
        let writer = open_child(&self.prefix, "writer.lock", false)?;
        for (file, path, expected) in [
            (
                &format,
                self.path.join("bft-format.lock"),
                origin.format_lock(),
            ),
            (
                &writer,
                self.path.join(storage::DIRECTORY).join("writer.lock"),
                origin.writer_lock(),
            ),
        ] {
            common_storage::verify_lock_carrier(file, &path)?;
            if &common_storage::lock_carrier_identity(file)? != expected {
                return Err("successor original ledger carrier identity changed".into());
            }
        }
        let directory = open_child(&self.root, DIRECTORY, true)?;
        private_directory(&directory)?;
        let expected = InstalledPointerV1::from_original(original, installed)?;
        let digest = original
            .request
            .intent
            .body
            .checkpoint
            .digest()
            .map_err(err)?;
        let intent = format!("{digest}.intent.cbor");
        let pointer = format!("{digest}.installed.cbor");
        // Enumerate this exact descriptor, never an absolute path whose parent
        // may have been replaced. An original intent/stage is never ignored.
        for name in entry_names(&directory)? {
            if name.as_os_str() == std::ffi::OsStr::new(&intent)
                || name
                    .to_str()
                    .and_then(files::stage_target)
                    .is_some_and(|target| target == intent || target == pointer)
            {
                return Err("successor private install WAL is unresolved".into());
            }
        }
        let mut file = open_child(&directory, &pointer, false)?;
        private_file(&file)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_METADATA as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() > MAX_METADATA {
            return Err("successor installed pointer capacity".into());
        }
        let actual: InstalledPointerV1 = decode(&bytes, MAX_METADATA)?;
        if actual != expected {
            return Err("successor installed pointer differs from original minimum".into());
        }
        private_file(&file)?;
        let current_pointer = open_child(&directory, &pointer, false)?;
        same_file(&file, &current_pointer)?;
        let current_directory = open_child(&self.root, DIRECTORY, true)?;
        same_directory(&directory, &current_directory)?;
        for name in entry_names(&directory)? {
            if name.as_os_str() == std::ffi::OsStr::new(&intent)
                || name
                    .to_str()
                    .and_then(files::stage_target)
                    .is_some_and(|target| target == intent || target == pointer)
            {
                return Err("successor private install WAL appeared during readback".into());
            }
        }
        // Recheck carrier bytes after the bounded pointer read, and reject
        // root/prefix replacement instead of following a new absolute parent.
        for (file, path, expected) in [
            (
                &format,
                self.path.join("bft-format.lock"),
                origin.format_lock(),
            ),
            (
                &writer,
                self.path.join(storage::DIRECTORY).join("writer.lock"),
                origin.writer_lock(),
            ),
        ] {
            common_storage::verify_lock_carrier(file, &path)?;
            if &common_storage::lock_carrier_identity(file)? != expected {
                return Err("successor original ledger carrier changed during readback".into());
            }
        }
        self.verify_directories()?;
        live(deadline)?;
        Ok(actual)
    }
}
fn open_child(parent: &File, name: &str, directory: bool) -> Result<File, String> {
    open_component(parent, std::ffi::OsStr::new(name), directory)
}
fn open_component(parent: &File, name: &std::ffi::OsStr, directory: bool) -> Result<File, String> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.contains(&b'/') || bytes == b"." || bytes == b".." {
        return Err("successor invalid child name".into());
    }
    let name = CString::new(bytes).map_err(err)?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: parent and CString remain alive throughout openat; ownership of a
    // successful descriptor transfers exactly once into File.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn open_absolute_directory(path: &Path) -> Result<File, String> {
    if !path.is_absolute() {
        return Err("successor installation root must be absolute".into());
    }
    let mut directory = File::open("/").map_err(err)?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = open_component(&directory, name, true)?;
            }
            _ => return Err("successor noncanonical installation root".into()),
        }
    }
    Ok(directory)
}
fn private_directory(file: &File) -> Result<(), String> {
    let m = file.metadata().map_err(err)?;
    // SAFETY: geteuid has no arguments or memory access.
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("successor installation directory is not private and owned".into());
    }
    Ok(())
}
fn private_file(file: &File) -> Result<(), String> {
    let m = file.metadata().map_err(err)?;
    // SAFETY: geteuid has no arguments or memory access.
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || m.len() > MAX_METADATA as u64
    {
        return Err("successor installed pointer is not a private single original".into());
    }
    Ok(())
}
fn same_file(a: &File, b: &File) -> Result<(), String> {
    let a = a.metadata().map_err(err)?;
    let b = b.metadata().map_err(err)?;
    if a.dev() != b.dev() || a.ino() != b.ino() {
        return Err("successor installation carrier replaced".into());
    }
    Ok(())
}
fn same_directory(a: &File, b: &File) -> Result<(), String> {
    private_directory(a)?;
    private_directory(b)?;
    same_file(a, b)
}
fn entry_names(directory: &File) -> Result<Vec<std::ffi::OsString>, String> {
    let duplicate = directory.try_clone().map_err(err)?;
    // SAFETY: fdopendir assumes ownership only on success; File owns it until
    // then. Its DIR stream is confined to this call and closed by the guard.
    let raw = unsafe { libc::fdopendir(duplicate.as_raw_fd()) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let _ = duplicate.into_raw_fd();
    struct Dir(*mut libc::DIR);
    impl Drop for Dir {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    let stream = Dir(raw);
    // A cloned directory descriptor shares its directory offset. Always rewind
    // this private readback descriptor before each fresh enumeration.
    unsafe {
        libc::rewinddir(stream.0);
    }
    let mut names = Vec::new();
    loop {
        // SAFETY: the stream is live; d_name is NUL-terminated by readdir.
        clear_errno()?;
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(0) {
                return Err(error.to_string());
            }
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if names.len() == 256 {
            return Err("successor install directory capacity".into());
        }
        names.push(std::ffi::OsStr::from_bytes(name).to_owned());
    }
    Ok(names)
}

fn clear_errno() -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // SAFETY: libc returns this thread's errno location.
        unsafe {
            *libc::__error() = 0;
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: libc returns this thread's errno location.
        unsafe {
            *libc::__errno_location() = 0;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
    Err("successor directory readback is unsupported on this platform".into())
}
