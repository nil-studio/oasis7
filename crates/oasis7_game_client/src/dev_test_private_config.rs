//! Trusted private service configuration loading shared by native adapters.
use std::{io::Read, os::unix::fs::MetadataExt, path::Path};

pub fn load_config(path: &Path) -> Result<crate::dev_test_service::Config, String> {
    let config: crate::dev_test_service::Config =
        serde_json::from_slice(&read_private_config(path)?)
            .map_err(|_| "invalid private config")?;
    config.validate()?;
    Ok(config)
}

pub fn read_private_config(path: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    if !path.is_absolute() {
        return Err("absolute private config required".into());
    }
    crate::runtime_broker::trusted_ancestors(path)?;
    let parent = path.parent().ok_or("private config parent missing")?;
    let directory =
        std::fs::symlink_metadata(parent).map_err(|_| "private config parent unavailable")?;
    let uid = unsafe { libc::geteuid() };
    if directory.uid() != uid || directory.mode() & 0o022 != 0 {
        return Err("private config parent untrusted".into());
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "private config unavailable")?;
    let metadata = file.metadata().map_err(|_| "private config unavailable")?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
        || metadata.len() > 1048576
    {
        return Err("private config required".into());
    }
    let mut body = Vec::new();
    file.take(1048577)
        .read_to_end(&mut body)
        .map_err(|_| "private config unavailable")?;
    if body.len() > 1048576 {
        return Err("private config too large".into());
    }
    Ok(body)
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn private_config_rejects_replacement_boundaries() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-local-config-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = root.join("config");
        std::fs::write(&config, b"private-fixture").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_private_config(&config).unwrap(), b"private-fixture");
        let link = root.join("link");
        symlink(&config, &link).unwrap();
        assert!(read_private_config(&link).is_err());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(read_private_config(&config).is_err());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
