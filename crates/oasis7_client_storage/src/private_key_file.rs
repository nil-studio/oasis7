//! Private operator/device keys share the same bounded, no-link Unix loader.
//! Ancestor directories and local listening processes must be trusted by the
//! operator; ownership checks do not isolate a compromised same-UID process.
use std::path::Path;
use zeroize::Zeroizing;

#[cfg(unix)]
pub fn read_private_key_file(path: &Path) -> Result<Zeroizing<[u8; 32]>, String> {
    use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::{Dir, MetadataExt, OpenOptions, OpenOptionsExt, PermissionsExt};
    use std::io::Read;
    let components = path.components().collect::<Vec<_>>();
    if !path.is_absolute()
        || components.len() < 2
        || components
            .iter()
            .skip(1)
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("invalid private key path".into());
    }
    let mut parent = Dir::open_ambient_dir("/", cap_std::ambient_authority())
        .map_err(|_| "private key unavailable")?;
    for component in &components[1..components.len() - 1] {
        parent = parent
            .open_dir_nofollow(component.as_os_str())
            .map_err(|_| "private key unavailable")?;
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_NONBLOCK);
    let mut file = parent
        .open_with(components.last().unwrap().as_os_str(), &options)
        .map_err(|_| "private key unavailable")?;
    let metadata = file.metadata().map_err(|_| "private key unavailable")?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o7777 != 0o600
        || metadata.len() != 32
    {
        return Err("private key requires operator-owned regular mode-0600 exact 32 bytes".into());
    }
    let mut key = Zeroizing::new([0; 32]);
    file.read_exact(&mut *key)
        .map_err(|_| "private key unavailable")?;
    let mut extra = [0; 1];
    if file
        .read(&mut extra)
        .map_err(|_| "private key unavailable")?
        != 0
    {
        return Err("private key size changed".into());
    }
    Ok(key)
}
#[cfg(not(unix))]
pub fn read_private_key_file(_: &Path) -> Result<Zeroizing<[u8; 32]>, String> {
    Err("private key loading unsupported on this platform".into())
}
