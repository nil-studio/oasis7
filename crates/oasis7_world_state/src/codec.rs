//! Concrete persistence and consensus codecs. Format distinctions are intentional.
use crate::{Journal, Snapshot, WorldError, WorldStateProjection};
use serde::Serialize;
#[derive(Serialize)]
pub struct StateRootProjection<'a, 's> {
    pub state: &'a WorldStateProjection<'s>,
    pub manifest_hash: &'a str,
    pub policy_hash: &'a str,
}
pub fn state_root_hash(projection: &StateRootProjection<'_, '_>) -> Result<String, WorldError> {
    crate::util::hash_json(projection)
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
