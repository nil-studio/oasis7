//! Pin a file to the actual loaded image before hashing it for Frozen replay.
//! This is local artifact identity, not remote attestation or custody evidence.
use std::fs::{File, Metadata};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(test)]
#[path = "executable_image_tests.rs"]
mod replacement_tests;

pub(super) fn open_loaded_image() -> Result<File, String> {
    #[cfg(target_os = "linux")]
    let file =
        File::open("/proc/self/exe").map_err(|e| format!("loaded executable unavailable: {e}"))?;
    #[cfg(target_os = "macos")]
    let file = File::open(std::env::current_exe().map_err(|e| e.to_string())?)
        .map_err(|e| format!("loaded executable path unavailable: {e}"))?;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err("formal executable-image binding requires Linux or macOS".into());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        validate_loaded_file(&file)?;
        Ok(file)
    }
}
#[cfg(target_os = "linux")]
pub(super) fn validate_loaded_file(file: &File) -> Result<(), String> {
    let actual = std::fs::metadata("/proc/self/exe").map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || opened.dev() != actual.dev() || opened.ino() != actual.ino() {
        return Err("executable descriptor differs from loaded image".into());
    }
    Ok(())
}
#[cfg(target_os = "macos")]
pub(super) fn validate_loaded_file(file: &File) -> Result<(), String> {
    // Darwin sys/proc_info.h ABI: proc_regioninfo is 96 bytes, followed by
    // vnode_info_path. Kernel region lookup authenticates the loaded image's
    // vnode; resolving current_exe's path alone does not do that after rename.
    #[repr(C)]
    struct RegionWithPath {
        region: [u64; 12],
        vnode: libc::vnode_info_path,
    }
    let mut info: RegionWithPath = unsafe { std::mem::zeroed() };
    // libc recommends mach2 for Mach header access; only the opaque address is
    // used here, with no dependency on a header's architecture-specific layout.
    #[allow(deprecated)]
    let address = unsafe { libc::_dyld_get_image_header(0) };
    if address.is_null() {
        return Err("loaded main image header unavailable".into());
    }
    let size = std::mem::size_of::<RegionWithPath>();
    let returned = unsafe {
        libc::proc_pidinfo(
            std::process::id() as libc::pid_t,
            8,
            address as usize as u64,
            &mut info as *mut _ as *mut libc::c_void,
            size as libc::c_int,
        )
    };
    if returned != size as libc::c_int {
        return Err("loaded executable vnode unavailable".into());
    }
    let actual = &info.vnode.vip_vi.vi_stat;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file()
        || opened.dev() != u64::from(actual.vst_dev)
        || opened.ino() != actual.vst_ino
    {
        return Err("executable path was replaced after this image was loaded".into());
    }
    Ok(())
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn validate_loaded_file(_: &File) -> Result<(), String> {
    Err("formal executable-image binding unsupported".into())
}
pub(super) fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        let _ = (before, after);
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn actual_image_is_accepted_but_a_copied_same_bytes_file_is_not_loaded() {
        let actual = open_loaded_image().unwrap();
        validate_loaded_file(&actual).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-image-copy-{}", std::process::id()));
        assert!(!root.exists());
        std::fs::copy(std::env::current_exe().unwrap(), &root).unwrap();
        let copied = File::open(&root).unwrap();
        assert!(validate_loaded_file(&copied).is_err());
        std::fs::remove_file(root).unwrap();
    }
}
