/// Operator-controlled immutable executable identity; not remote attestation.
/// A different binary cannot replay an unresolved frozen input. Already saved
/// candidates can be verified and completed without rerunning old business code.
pub(super) fn executable_digest() -> Result<String, String> {
    use std::io::Read;
    static DIGEST: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    DIGEST
        .get_or_init(|| {
            let mut file = crate::open_loaded_image()?;
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_file() {
                return Err("execution artifact is not a regular file".into());
            }
            let mut hasher = blake3::Hasher::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            crate::validate_loaded_file(&file)?;
            if !crate::unchanged(&metadata, &file.metadata().map_err(|e| e.to_string())?) {
                return Err("execution artifact changed while hashing".into());
            }
            Ok(hasher.finalize().to_hex().to_string())
        })
        .clone()
}
