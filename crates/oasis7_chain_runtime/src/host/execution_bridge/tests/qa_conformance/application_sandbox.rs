//! Real OS filesystem isolation for application subprocesses; unsupported hosts fail closed.
use std::{path::Path, process::Command};

pub(super) fn mechanism() -> &'static str {
    if cfg!(target_os = "macos") {
        "sandbox-exec"
    } else {
        "landlock"
    }
}

pub(super) fn profile_digest(
    executable: &Path,
    application_dir: &Path,
    profile: &str,
) -> blake3::Hash {
    #[cfg(target_os = "linux")]
    {
        let _ = profile;
        blake3::hash(linux::profile(executable, application_dir).as_bytes())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (executable, application_dir);
        blake3::hash(profile.as_bytes())
    }
}

pub(super) fn command(executable: &Path, application_dir: &Path, profile: &str) -> Command {
    #[cfg(target_os = "macos")]
    {
        let _ = application_dir;
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args(["-p", profile]).arg(executable);
        command
    }
    #[cfg(target_os = "linux")]
    {
        let _ = profile;
        linux::command(executable, application_dir)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (executable, application_dir, profile);
        panic!("real application OS isolation requires Darwin sandbox-exec or Linux Landlock");
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{fs::OpenOptionsExt, process::CommandExt},
    };
    use std::{fs::OpenOptions, io};

    // ABI v3 is required: TRUNCATE closes the file mutation gap of ABI v1.
    const ACCESS: u64 = (1 << 15) - 1;
    const FILE_ACCESS: u64 = (1 << 0) | (1 << 2);
    const SYSTEM_ACCESS: u64 = FILE_ACCESS | (1 << 3);
    const SYSTEM_PATHS: [&str; 7] = ["/usr", "/lib", "/lib64", "/etc", "/dev", "/proc", "/sys"];

    pub(super) fn profile(executable: &Path, application_dir: &Path) -> String {
        let paths: Vec<_> = SYSTEM_PATHS
            .into_iter()
            .filter(|path| Path::new(path).exists())
            .collect();
        format!(
            "landlock min_abi=3 handled={ACCESS} system={SYSTEM_ACCESS}:{paths:?} executable={FILE_ACCESS}:{} application={ACCESS}:{}",
            executable.display(),
            application_dir.display()
        )
    }
    #[repr(C, packed)]
    struct PathBeneath {
        allowed_access: u64,
        parent_fd: i32,
    }

    fn add_path(ruleset: &OwnedFd, path: &Path, rights: u64) -> io::Result<()> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
            .open(path)?;
        let rule = PathBeneath {
            allowed_access: rights,
            parent_fd: file.as_raw_fd(),
        };
        let result = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                1,
                &rule,
                0,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn command(executable: &Path, application_dir: &Path) -> Command {
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<u8>(),
                0,
                1,
            )
        };
        assert!(
            abi >= 3,
            "Landlock ABI3 required for truncate denial: {abi}"
        );
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &ACCESS,
                std::mem::size_of::<u64>(),
                0,
            )
        };
        assert!(
            fd >= 0,
            "Landlock ruleset unavailable: {}",
            io::Error::last_os_error()
        );
        let ruleset = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        // Node storage in the parent's temporary directory is never admitted.
        // Runtime libraries/configuration and the child's private application directory are explicit.
        for path in SYSTEM_PATHS {
            let path = Path::new(path);
            if path.exists() {
                add_path(&ruleset, path, SYSTEM_ACCESS).expect("system sandbox rule");
            }
        }
        add_path(&ruleset, executable, FILE_ACCESS).expect("application executable rule");
        add_path(&ruleset, application_dir, ACCESS).expect("private application directory rule");
        let mut command = Command::new(executable);
        // Only async-signal-safe syscalls execute between fork and exec. The parent stays unrestricted.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command
    }
}
