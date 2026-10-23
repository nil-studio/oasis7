//! Restore may perform recovery writes. Keep every such write in our own copy.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(super) struct CopyGuard {
    pub path: PathBuf,
}
impl Drop for CopyGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
pub(super) fn isolated_copy(source: &Path, private_parent: &Path) -> Result<CopyGuard, String> {
    oasis7_game_client::private_storage::open_root(private_parent)?;
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| "offline scratch randomness unavailable")?;
    let path = private_parent.join(format!("participant-restore-{}", hex::encode(random)));
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .map_err(|_| "private scratch unavailable")?;
    let guard = CopyGuard { path };
    let mut budget = (512 * 1024 * 1024u64, 50_000usize);
    copy(source, &guard.path, &mut budget)?;
    Ok(guard)
}
fn copy(source: &Path, target: &Path, budget: &mut (u64, usize)) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    for item in fs::read_dir(source).map_err(|_| "offline world enumeration failed")? {
        let item = item.map_err(|_| "offline world enumeration failed")?;
        budget.1 = budget
            .1
            .checked_sub(1)
            .ok_or("offline world file count exceeds bound")?;
        let from = item.path();
        let to = target.join(item.file_name());
        let m = fs::symlink_metadata(&from).map_err(|_| "offline world metadata failed")?;
        if m.is_dir() {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&to)
                .map_err(|_| "offline directory copy failed")?;
            copy(&from, &to, budget)?;
        } else if m.is_file() {
            let mut input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&from)
                .map_err(|_| "offline world input failed")?;
            let opened = input.metadata().map_err(|_| "offline world input failed")?;
            if !opened.is_file() || opened.len() != m.len() {
                return Err("offline world changed during copy".into());
            }
            budget.0 = budget.0.checked_sub(opened.len()).ok_or(
                "offline restore exceeds 512 MiB; use an explicitly verified dedicated backup",
            )?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&to)
                .map_err(|_| "offline world copy failed")?;
            let copied = std::io::copy(&mut (&mut input).take(opened.len() + 1), &mut output)
                .map_err(|_| "offline world copy failed")?;
            if copied != opened.len() {
                return Err("offline world changed during copy".into());
            }
            output.flush().map_err(|_| "offline copy flush failed")?;
        } else {
            return Err("offline restore rejects symbolic or special files".into());
        }
    }
    Ok(())
}
