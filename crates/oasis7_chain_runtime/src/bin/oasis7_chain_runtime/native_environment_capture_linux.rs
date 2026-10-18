//! Linux mapped backing files are pinned by kernel map-file handles or verified vnode identity.
use std::{
    collections::BTreeMap,
    fs::{File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mapping {
    range: String,
    major: u32,
    minor: u32,
    inode: u64,
    path: String,
}
pub(super) fn inventory() -> Result<Vec<Mapping>, String> {
    let mut bytes = Vec::new();
    File::open("/proc/self/maps")
        .map_err(|e| e.to_string())?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Unavailable: native mapping inventory budget".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let mut files = BTreeMap::new();
    for line in text.lines() {
        let mut rest = line;
        let mut fields = Vec::new();
        for _ in 0..5 {
            rest = rest.trim_start();
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            fields.push(&rest[..end]);
            rest = &rest[end..];
        }
        let inode: u64 = fields[4]
            .parse()
            .map_err(|_| "Unavailable: malformed map inode")?;
        let path = rest.trim_start();
        if inode == 0 {
            continue;
        }
        if !path.starts_with('/') || path.ends_with(" (deleted)") {
            return Err("Unavailable: mapped backing file is deleted or unnamed".into());
        }
        let (major, minor) = fields[3].split_once(':').ok_or("malformed map device")?;
        let major = u32::from_str_radix(major, 16).map_err(|e| e.to_string())?;
        let minor = u32::from_str_radix(minor, 16).map_err(|e| e.to_string())?;
        let mapping = Mapping {
            range: fields[0].into(),
            major,
            minor,
            inode,
            path: path.into(),
        };
        // One file may be mapped through multiple loader aliases. Choose a
        // stable logical path rather than whichever ASLR address sorts first.
        files
            .entry((major, minor, inode))
            .and_modify(|current: &mut Mapping| {
                if mapping.path < current.path {
                    *current = mapping.clone();
                }
            })
            .or_insert(mapping);
    }
    if files.len() > 253 {
        return Err("Unavailable: mapped component budget".into());
    }
    Ok(files.into_values().collect())
}
fn matches(mapping: &Mapping, metadata: &Metadata) -> bool {
    metadata.is_file()
        && metadata.ino() == mapping.inode
        && libc::major(metadata.dev()) == mapping.major
        && libc::minor(metadata.dev()) == mapping.minor
}
pub(super) fn pin(
    mappings: &[Mapping],
    executable: &Metadata,
) -> Result<Vec<(String, File, Metadata)>, String> {
    let mut result = Vec::new();
    for mapping in mappings {
        if matches(mapping, executable) {
            continue;
        }
        // map_files pins the actual backing vnode even if its pathname was replaced.
        // Some kernels restrict map_files, so a pathname is allowed only with identical inode/device.
        let file = File::open(format!("/proc/self/map_files/{}", mapping.range))
            .or_else(|_| File::open(&mapping.path))
            .map_err(|e| format!("Unavailable: mapped dependency {}: {e}", mapping.path))?;
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !matches(mapping, &before) {
            return Err("Unavailable: mapped dependency path was replaced".into());
        }
        // Vnode identity proves this capture, but is not execution identity:
        // restoring identical original files necessarily creates new inodes.
        result.push((format!("mapped{}", mapping.path), file, before));
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    if result.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("Unavailable: one loader path names multiple mapped backing files".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_backing_file_identity_is_independent_of_inode() {
        let directory =
            std::env::temp_dir().join(format!("oasis-native-inode-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("library.so");
        let bytes = b"original native dependency";
        std::fs::write(&path, bytes).unwrap();
        let executable = File::open("/proc/self/exe").unwrap().metadata().unwrap();
        let mapped = |metadata: &Metadata| Mapping {
            range: "invalid-map-file-range".into(),
            major: libc::major(metadata.dev()),
            minor: libc::minor(metadata.dev()),
            inode: metadata.ino(),
            path: path.to_str().unwrap().into(),
        };
        let first_metadata = std::fs::metadata(&path).unwrap();
        let mut first = pin(&[mapped(&first_metadata)], &executable)
            .unwrap()
            .remove(0);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let second_metadata = std::fs::metadata(&path).unwrap();
        assert_ne!(first_metadata.ino(), second_metadata.ino());
        let mut second = pin(&[mapped(&second_metadata)], &executable)
            .unwrap()
            .remove(0);
        let capture = |entry: &mut (String, File, Metadata)| {
            super::super::component(
                oasis7_formal_protocol::native_environment::NativeEnvironmentRole::LoadedLibrary,
                &entry.0,
                &mut entry.1,
                &mut |_, _| Ok(()),
            )
            .unwrap()
        };
        let first = serde_cbor::to_vec(&capture(&mut first)).unwrap();
        let second = serde_cbor::to_vec(&capture(&mut second)).unwrap();
        assert_eq!(first, second);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
