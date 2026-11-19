//! Feature-gated process fixture; never part of the default production entry.
use super::*;
use oasis7_wasm_executor::bounded_worker::NativeEnvironmentVerifierV1;
use std::{
    ffi::CString,
    path::{Path, PathBuf},
};
const STABLE: &str = "oasis7-native-bound-test-stable.so";
const CHANGING: &str = "oasis7-native-bound-test-changing.so";
struct LoadedDependency {
    handle: *mut libc::c_void,
    path: PathBuf,
}
impl LoadedDependency {
    fn open(path: &Path) -> Result<Self, &'static str> {
        if !matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(STABLE | CHANGING)
        ) {
            return Err("fixture dependency name");
        }
        let path = path.canonicalize().map_err(|_| "fixture dependency path")?;
        let name = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| "fixture dependency nul")?;
        // SAFETY: the fixture supplies a live canonical ELF shared library;
        // its handle is retained until the matching dlclose in Drop.
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err("fixture actual dlopen");
        }
        Ok(Self { handle, path })
    }
}
impl Drop for LoadedDependency {
    fn drop(&mut self) {
        // SAFETY: this is the owned successful dlopen handle, closed once.
        unsafe {
            libc::dlclose(self.handle);
        }
    }
}
pub(crate) fn fixture_capture_v1(dependency: Option<&Path>) -> Result<Vec<u8>, String> {
    let _loaded = dependency
        .map(LoadedDependency::open)
        .transpose()
        .map_err(str::to_owned)?;
    super::capture(|_, _| Ok(()))?.encode()
}
struct FixtureVerifier {
    actual: worker::ActualNativeVerifierV1,
}
struct FixtureGuard {
    actual: worker::NativeInvocationGuardV1,
    loaded: Option<LoadedDependency>,
}
impl NativeEnvironmentVerifierV1 for FixtureVerifier {
    type Guard = FixtureGuard;
    fn before(&mut self, bytes: &[u8]) -> Result<Self::Guard, &'static str> {
        let descriptor =
            NativeExecutionEnvironmentV1::decode(bytes).map_err(|_| "fixture descriptor")?;
        let mut selected = None;
        for component in &descriptor.components {
            if component.role != NativeEnvironmentRole::LoadedLibrary {
                continue;
            }
            let Some(path) = component.name.strip_prefix("mapped") else {
                continue;
            };
            let path = Path::new(path);
            if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(STABLE | CHANGING)
            ) {
                if selected.is_some() {
                    return Err("fixture duplicate dependencies");
                }
                selected = Some(LoadedDependency::open(path)?);
            }
        }
        let actual = self.actual.before(bytes)?;
        Ok(FixtureGuard {
            actual,
            loaded: selected,
        })
    }
    fn after(&mut self, bytes: &[u8], guard: &Self::Guard) -> Result<(), &'static str> {
        if let Some(loaded) = &guard.loaded
            && loaded.path.file_name().and_then(|name| name.to_str()) == Some(CHANGING)
        {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&loaded.path)
                .map_err(|_| "fixture DSO open")?;
            let length = file.metadata().map_err(|_| "fixture DSO metadata")?.len();
            file.set_len(length.checked_add(1).ok_or("fixture DSO length")?)
                .map_err(|_| "fixture actual DSO mutation")?;
            file.sync_all().map_err(|_| "fixture DSO sync")?;
        }
        self.actual.after(bytes, &guard.actual)
    }
}
pub(crate) fn fixture_worker_main_v1() -> Result<(), &'static str> {
    oasis7_wasm_executor::bounded_worker::worker_main_with_native_verifier_v1(
        &mut FixtureVerifier {
            actual: worker::ActualNativeVerifierV1::new(),
        },
    )
}
