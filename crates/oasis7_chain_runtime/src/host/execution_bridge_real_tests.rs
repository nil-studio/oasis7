use std::path::Path;

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    super::write_bytes_atomic(path, bytes)
}

#[path = "execution_bridge/mod.rs"]
pub(crate) mod real_execution_bridge;
