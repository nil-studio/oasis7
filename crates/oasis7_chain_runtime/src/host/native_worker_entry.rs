//! Explicit native worker entry points; dispatch does not grant world writes.
#[cfg(target_os = "linux")]
use super::native_environment_capture;

/// Process entry for an IWS worker which verifies its own loaded native closure.
/// This only dispatches the bounded worker protocol; it grants no world writes.
pub fn native_bound_worker_main_v1() -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    {
        native_environment_capture::native_bound_worker_main_v1()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("native-bound worker platform unsupported")
    }
}

#[cfg(feature = "test-support")]
pub fn native_worker_fixture_capture_v1(path: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "linux")]
    {
        native_environment_capture::fixture_capture_v1(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err("native fixture platform unsupported".into())
    }
}

#[cfg(feature = "test-support")]
pub fn native_worker_fixture_main_v1() -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    {
        native_environment_capture::fixture_worker_main_v1()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("native fixture platform unsupported")
    }
}
