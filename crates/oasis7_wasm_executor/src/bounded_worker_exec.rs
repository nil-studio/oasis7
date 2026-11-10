//! Kernel immutability mechanism only. Native identity/authorization comes from
//! the host's real native material handle, never this module's File parameter.
use std::{fs::File, path::Path};
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::{io, path::PathBuf};
pub enum WorkerExecutableV1<'a> {
    /// Component fixtures only; a replaceable path grants no native permission.
    FixturePath(&'a Path),
    /// Production must borrow this FD from its actual opaque native baseline.
    SealedFd(&'a File),
    /// Production IWS obtains both borrows from one authenticated native lease.
    NativeBoundSealedFd {
        file: &'a File,
        descriptor: &'a [u8],
    },
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(super) fn path(executable: &WorkerExecutableV1<'_>) -> io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    match executable {
        WorkerExecutableV1::FixturePath(path) => Ok(path.to_path_buf()),
        WorkerExecutableV1::SealedFd(file)
        | WorkerExecutableV1::NativeBoundSealedFd { file, .. } => {
            unsafe extern "C" {
                fn fcntl(fd: i32, command: i32, ...) -> i32;
            }
            let fd = file.as_raw_fd();
            // SAFETY: only interrogates the borrowed live file descriptor.
            let seals = unsafe { fcntl(fd, 1034) };
            // SAFETY: only interrogates the same borrowed live descriptor.
            let flags = unsafe { fcntl(fd, 1) };
            if seals < 0 || flags < 0 || seals & 15 != 15 || flags & 1 != 1 {
                return Err(io::Error::other(
                    "worker executable requires all immutable seals and CLOEXEC",
                ));
            }
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(io::Error::other("worker executable file"));
            }
            // The descriptor remains live in the caller until spawn/wait. The
            // kernel resolves this exact sealed inode before exec closes FD.
            Ok(PathBuf::from(format!("/proc/self/fd/{fd}")))
        }
    }
}
