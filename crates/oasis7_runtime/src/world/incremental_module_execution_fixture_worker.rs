//! Actual Linux kernel-sealed worker image. Non-Linux only tests genuine Host
//! unavailability; it never substitutes the in-process executor for success.
use super::*;
use oasis7_proto::memory_budget::MemoryReservation;
pub(super) struct Image {
    pub file: File,
    _charge: MemoryReservation,
}
impl Image {
    pub fn new(memory: &MemoryBudget) -> Self {
        #[cfg(unix)]
        if cfg!(target_os = "linux") {
            use std::{
                io::{Read, Seek, SeekFrom, Write},
                os::fd::FromRawFd,
            };
            let path = std::env::var_os("OASIS7_WASM_WORKER_TEST_EXE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("oasis7-wasm-worker")
                });
            let mut source=File::open(&path).unwrap_or_else(|error|panic!("build real oasis7-wasm-worker with wasmtime before this component test: {}: {error}",path.display()));
            let length = source.metadata().unwrap().len();
            assert!(length > 0 && length <= 64 << 20);
            let pages = length.checked_add(4095).unwrap() / 4096 * 4096;
            let charge = memory
                .reserve(
                    usize::try_from(pages).unwrap() + 65536,
                    MemoryClass::Ordinary,
                )
                .unwrap();
            unsafe extern "C" {
                fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
                fn fcntl(fd: i32, command: i32, ...) -> i32;
            }
            // SAFETY: fixed NUL-terminated name, CLOEXEC|ALLOW_SEALING, owned FD.
            let fd = unsafe { memfd_create(c"oasis7-module-executor-fixture".as_ptr(), 3) };
            assert!(fd >= 0);
            let mut file = unsafe { File::from_raw_fd(fd) };
            let mut left = length;
            let mut scratch = [0u8; 64 << 10];
            while left != 0 {
                let take = (left as usize).min(scratch.len());
                source.read_exact(&mut scratch[..take]).unwrap();
                file.write_all(&scratch[..take]).unwrap();
                left -= take as u64;
            }
            assert_eq!(file.metadata().unwrap().len(), length);
            file.flush().unwrap();
            assert_eq!(unsafe { fcntl(fd, 1033, 15) }, 0);
            assert_eq!(unsafe { fcntl(fd, 1034) } & 15, 15);
            file.seek(SeekFrom::Start(0)).unwrap();
            return Self {
                file,
                _charge: charge,
            };
        }
        let charge = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
        Self {
            file: File::open(std::env::current_exe().unwrap()).unwrap(),
            _charge: charge,
        }
    }
}
