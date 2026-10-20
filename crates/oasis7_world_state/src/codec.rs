//! Concrete persistence and consensus codecs. Format distinctions are intentional.
use crate::{Journal, Snapshot, WorldError, WorldStateProjection};
use serde::Serialize;
use std::path::Path;
#[derive(Serialize)]
pub struct StateRootProjection<'a, 's> {
    pub state: &'a WorldStateProjection<'s>,
    pub manifest_hash: &'a str,
    pub policy_hash: &'a str,
}
pub fn state_root_hash(projection: &StateRootProjection<'_, '_>) -> Result<String, WorldError> {
    crate::util::hash_json(projection)
}
/// Save the concrete Snapshot through the existing atomic pretty-JSON writer.
pub fn snapshot_save_json(snapshot: &Snapshot, path: &Path) -> Result<(), WorldError> {
    crate::util::atomic_write_json_to_path(snapshot, path)
}
/// Load the concrete Snapshot with the existing file reader and error conversion.
pub fn snapshot_load_json(path: &Path) -> Result<Snapshot, WorldError> {
    crate::util::read_json_from_path(path)
}
/// Save the concrete Journal through the existing atomic pretty-JSON writer.
pub fn journal_save_json(journal: &Journal, path: &Path) -> Result<(), WorldError> {
    crate::util::atomic_write_json_to_path(journal, path)
}
/// Load the concrete Journal with the existing file reader and error conversion.
pub fn journal_load_json(path: &Path) -> Result<Journal, WorldError> {
    crate::util::read_json_from_path(path)
}
pub fn snapshot_json(snapshot: &Snapshot) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(snapshot)
}
pub fn snapshot_json_pretty(snapshot: &Snapshot) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(snapshot)
}
pub fn snapshot_from_json(bytes: &[u8]) -> Result<Snapshot, serde_json::Error> {
    serde_json::from_slice(bytes)
}
pub fn snapshot_cbor(snapshot: &Snapshot) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(snapshot)
}
pub fn snapshot_from_cbor(bytes: &[u8]) -> Result<Snapshot, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}
pub fn snapshot_canonical_cbor(snapshot: &Snapshot) -> Result<Vec<u8>, WorldError> {
    crate::util::to_canonical_cbor(snapshot)
}
pub fn journal_json(journal: &Journal) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(journal)
}
pub fn journal_json_pretty(journal: &Journal) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(journal)
}
pub fn journal_from_json(bytes: &[u8]) -> Result<Journal, serde_json::Error> {
    serde_json::from_slice(bytes)
}
pub fn journal_cbor(journal: &Journal) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(journal)
}
pub fn journal_from_cbor(bytes: &[u8]) -> Result<Journal, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}
pub fn journal_canonical_cbor(journal: &Journal) -> Result<Vec<u8>, WorldError> {
    crate::util::to_canonical_cbor(journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_path_wrappers_preserve_atomic_replacement_file_bytes_and_errors() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("oasis7-state-json-{}-{unique}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone());
        let snapshot = snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let journal = journal_from_json(include_bytes!("../fixtures/journal.json")).unwrap();
        let snapshot_path = dir.join("snapshot.json");
        let journal_path = dir.join("journal.json");
        std::fs::write(&snapshot_path, b"previous snapshot").unwrap();
        std::fs::write(&journal_path, b"previous journal").unwrap();
        snapshot.save_json(snapshot_path.clone()).unwrap();
        journal.save_json(journal_path.as_path()).unwrap();
        assert_eq!(
            Snapshot::load_json(snapshot_path.as_path()).unwrap(),
            snapshot
        );
        assert_eq!(Journal::load_json(journal_path.clone()).unwrap(), journal);
        assert_eq!(
            std::fs::read(&snapshot_path).unwrap(),
            include_bytes!("../fixtures/snapshot.pretty.json")
        );
        assert_eq!(
            std::fs::read(&journal_path).unwrap(),
            include_bytes!("../fixtures/journal.pretty.json")
        );

        let missing = dir.join("missing.json");
        assert_eq!(
            snapshot_load_json(&missing),
            crate::util::read_json_from_path::<Snapshot>(&missing)
        );
        assert_eq!(
            journal_load_json(&missing),
            crate::util::read_json_from_path::<Journal>(&missing)
        );
        std::fs::write(&snapshot_path, b"{").unwrap();
        std::fs::write(&journal_path, b"{").unwrap();
        assert_eq!(
            snapshot_load_json(&snapshot_path),
            crate::util::read_json_from_path::<Snapshot>(&snapshot_path)
        );
        assert_eq!(
            journal_load_json(&journal_path),
            crate::util::read_json_from_path::<Journal>(&journal_path)
        );
    }

    #[test]
    fn pre_extraction_snapshot_and_journal_codecs_preserve_bytes() {
        let snapshot = snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let journal = journal_from_json(include_bytes!("../fixtures/journal.json")).unwrap();
        assert!(!journal.events.is_empty());
        assert_eq!(
            snapshot_json(&snapshot).unwrap(),
            include_bytes!("../fixtures/snapshot.json")
        );
        assert_eq!(
            snapshot_json_pretty(&snapshot).unwrap(),
            include_bytes!("../fixtures/snapshot.pretty.json")
        );
        assert_eq!(
            snapshot_cbor(&snapshot).unwrap(),
            include_bytes!("../fixtures/snapshot.cbor")
        );
        assert_eq!(
            snapshot_from_cbor(include_bytes!("../fixtures/snapshot.cbor")).unwrap(),
            snapshot
        );
        assert_eq!(
            journal_json(&journal).unwrap(),
            include_bytes!("../fixtures/journal.json")
        );
        assert_eq!(
            journal_json_pretty(&journal).unwrap(),
            include_bytes!("../fixtures/journal.pretty.json")
        );
        assert_eq!(
            journal_cbor(&journal).unwrap(),
            include_bytes!("../fixtures/journal.cbor")
        );
        assert_eq!(
            journal_from_cbor(include_bytes!("../fixtures/journal.cbor")).unwrap(),
            journal
        );
    }
    #[test]
    fn concrete_state_root_preserves_plain_and_sparse_encoding() {
        let snapshot = snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let projected = WorldStateProjection::borrowed(&snapshot.state);
        assert_eq!(
            serde_json::to_vec(&projected).unwrap(),
            serde_json::to_vec(&snapshot.state).unwrap()
        );
        let policy_hash = crate::util::hash_json(&snapshot.policies).unwrap();
        let manifest_hash = crate::util::hash_json(&snapshot.manifest).unwrap();
        let concrete = StateRootProjection {
            state: &projected,
            manifest_hash: &manifest_hash,
            policy_hash: &policy_hash,
        };
        #[derive(Serialize)]
        struct Previous<'a> {
            state: &'a crate::WorldState,
            manifest_hash: &'a str,
            policy_hash: &'a str,
        }
        assert_eq!(
            state_root_hash(&concrete).unwrap(),
            crate::util::hash_json(&Previous {
                state: &snapshot.state,
                manifest_hash: &manifest_hash,
                policy_hash: &policy_hash
            })
            .unwrap()
        );
    }
    #[test]
    fn malformed_codecs_preserve_parser_errors() {
        assert!(snapshot_from_json(b"{}").is_err());
        assert!(journal_from_json(b"{\"events\":[] ,\"events\":[]}").is_err());
        assert!(snapshot_from_cbor(&[0xff]).is_err());
        assert!(journal_from_cbor(&[0xff]).is_err());
    }
}
