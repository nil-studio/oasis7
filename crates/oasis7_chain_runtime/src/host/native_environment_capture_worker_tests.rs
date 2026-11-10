//! Actual native mappings, not caller-supplied environment labels.
use super::*;
static CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[test]
fn actual_descriptor_is_checked_before_and_after_in_same_process() {
    let _lock = CAPTURE_LOCK.lock().unwrap();
    let descriptor = super::super::capture(|_, _| Ok(()))
        .unwrap()
        .encode()
        .unwrap();
    let expected = NativeExecutionEnvironmentV1::decode(&descriptor).unwrap();
    let captured = capture_pinned_v1(CaptureModeV1::Ordinary, |_, _| Ok(())).unwrap();
    captured.validate_descriptor(&expected).unwrap();
    captured.validate_pins(CaptureModeV1::Ordinary).unwrap();
    let after = capture_pinned_v1(CaptureModeV1::Ordinary, |_, _| Ok(())).unwrap();
    after.validate_descriptor(&expected).unwrap();
}
#[test]
fn valid_format_with_wrong_actual_component_digest_is_rejected() {
    let _lock = CAPTURE_LOCK.lock().unwrap();
    let mut descriptor = super::super::capture(|_, _| Ok(())).unwrap();
    let component = descriptor.components.first_mut().unwrap();
    component.content_digest = "00".repeat(32);
    let encoded = descriptor.encode().unwrap();
    let expected = NativeExecutionEnvironmentV1::decode(&encoded).unwrap();
    let actual = capture_pinned_v1(CaptureModeV1::Ordinary, |_, _| Ok(())).unwrap();
    assert!(actual.validate_descriptor(&expected).is_err());
}
#[test]
fn changed_actual_mapped_dependency_cannot_pass_post_call_check() {
    let _lock = CAPTURE_LOCK.lock().unwrap();
    use std::{
        fs::OpenOptions,
        io::{Seek, SeekFrom, Write},
        os::fd::AsRawFd,
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "oasis7-native-mapped-dependency-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(&[17; 4096]).unwrap();
    file.sync_all().unwrap();
    // SAFETY: a live page-sized regular file is mapped read-only and held until
    // munmap below. The pointer is never dereferenced by this test.
    let address = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            4096,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    assert_ne!(address, libc::MAP_FAILED);
    {
        let descriptor = super::super::capture(|_, _| Ok(())).unwrap();
        assert!(
            descriptor
                .components
                .iter()
                .any(|component| component.name.ends_with(path.to_str().unwrap()))
        );
        let expected = NativeExecutionEnvironmentV1::decode(&descriptor.encode().unwrap()).unwrap();
        let captured = capture_pinned_v1(CaptureModeV1::Ordinary, |_, _| Ok(())).unwrap();
        captured.validate_descriptor(&expected).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&[29; 4096]).unwrap();
        file.sync_all().unwrap();
        assert!(captured.validate_pins(CaptureModeV1::Ordinary).is_err());
        let actual = capture_pinned_v1(CaptureModeV1::Ordinary, |_, _| Ok(())).unwrap();
        assert!(actual.validate_descriptor(&expected).is_err());
    }
    // SAFETY: this is precisely the still-live mapping returned above.
    assert_eq!(unsafe { libc::munmap(address, 4096) }, 0);
    drop(file);
    std::fs::remove_file(path).unwrap();
}
