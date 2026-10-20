//! Offline-only entry point. Does not construct or start a node/world service.
#[cfg(not(test))]
use super::execution_bridge::controlled_history::{TrustedHistoryConfiguration, verify_history};
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::controlled_history::{
    TrustedHistoryConfiguration, verify_history,
};
use oasis7_distfs::controlled_authority::replicated_protocol::DurabilityEvidence;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut trust = None;
    let mut evidence = None;
    while let Some(arg) = args.next() {
        let destination = match arg {
            "--trusted-config" => &mut trust,
            "--evidence" => &mut evidence,
            _ => return Err(format!("unknown history verifier argument: {arg}")),
        };
        if destination.is_some() {
            return Err(format!("duplicate history verifier argument: {arg}"));
        }
        *destination = Some(PathBuf::from(
            args.next()
                .ok_or("missing history verifier argument value")?,
        ));
    }
    let config: TrustedHistoryConfiguration = serde_json::from_slice(&read_bounded(
        &trust.ok_or("required --trusted-config")?,
        64 * 1024,
    )?)
    .map_err(|e| format!("trusted configuration: {e}"))?;
    let bytes = read_bounded(&evidence.ok_or("required --evidence")?, 64 * 1024 * 1024)?;
    let input: EvidenceHistory =
        serde_json::from_slice(&bytes).map_err(|e| format!("bounded evidence history: {e}"))?;
    let result = verify_history(&config, &input.0)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
struct EvidenceHistory(Vec<DurabilityEvidence>);
impl<'de> Deserialize<'de> for EvidenceHistory {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct History;
        impl<'de> serde::de::Visitor<'de> for History {
            type Value = EvidenceHistory;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("at most 64 dual-receipt history records")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut items: A,
            ) -> Result<EvidenceHistory, A::Error> {
                if items.size_hint().is_some_and(|n| n > 64) {
                    return Err(serde::de::Error::custom("history capacity"));
                }
                let mut result = Vec::new();
                while let Some(item) = items.next_element()? {
                    if result.len() == 64 {
                        return Err(serde::de::Error::custom("history capacity"));
                    }
                    result.push(item);
                }
                Ok(EvidenceHistory(result))
            }
        }
        decoder.deserialize_seq(History)
    }
}
pub(crate) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    #[cfg(not(unix))]
    {
        let _ = (path, maximum);
        Err("offline history file verification is unsupported on this platform".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Operator-controlled directory boundary: reject every observed symlink
        // component and use no-follow for the final open; no hostile same-UID
        // ancestor replacement guarantee is claimed.
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        for component in absolute.ancestors() {
            let metadata = std::fs::symlink_metadata(component).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("history input symlink refused".into());
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&absolute)
            .map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > maximum {
            return Err("history input file type/size refused".into());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(maximum + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > maximum {
            return Err("history input file grew beyond budget".into());
        }
        Ok(bytes)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn offline_inputs_reject_symlinks_nonfiles_and_oversize_without_writes() {
        let unique = format!(
            "controlled-history-input-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        std::fs::create_dir(&directory).unwrap();
        let directory = std::fs::canonicalize(directory).unwrap();
        let file = directory.join("input");
        std::fs::write(&file, b"bounded").unwrap();
        assert_eq!(read_bounded(&file, 7).unwrap(), b"bounded");
        assert!(read_bounded(&file, 6).is_err());
        assert!(read_bounded(&directory, 1024).is_err());
        let link = directory.join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(read_bounded(&link, 1024).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"bounded");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn offline_arguments_fail_closed_without_starting_services() {
        assert!(run(["--trusted-config"].into_iter()).is_err());
        assert!(run(["--evidence", "a", "--evidence", "b"].into_iter()).is_err());
        assert!(run(["--world", "a"].into_iter()).is_err());
    }
}
