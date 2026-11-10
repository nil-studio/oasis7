//! Trusted offline operator entry. Never expose this command to a model.
use oasis7_cas::LocalCasStore;
use std::{error::Error, io::Read, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 || args[1] != "--restore-approved" {
        return Err("usage: restore_verified_blob --restore-approved CAS_ROOT EXPECTED_HASH VERIFIED_SOURCE_FILE (stop services first)".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(Path::new(&args[4]))?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("operator source exceeds 64MiB bound".into());
    }
    let evidence = LocalCasStore::new(&args[2])
        .restore_verified_blob(&args[3], &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    println!(
        "{}",
        serde_json::json!({"status":"verified", "content_hash":args[3], "byte_length":bytes.len(), "quarantine":evidence})
    );
    Ok(())
}
