//! Fixed structural checkpoints, shared by cold and changed closure.
use super::super::super::RuntimeArtifactRecordV1 as Record;
#[derive(Clone, Copy)]
pub(crate) enum CheckpointKey {
    Simple(&'static str),
    Pair(&'static str, &'static str),
}
pub(crate) fn counts(record: Record) -> [Option<CheckpointKey>; 3] {
    use CheckpointKey::{Pair as P, Simple as S};
    use Record as R;
    match record {
        R::ChainResourceManifest => [
            Some(S("resource_balances.count")),
            Some(S("material_ledgers.count")),
            Some(S("generated_chunks.count")),
        ],
        R::MaterialLedger => [Some(S("materials.count")), None, None],
        R::ChunkManifest => [
            Some(S("fragment_refs.count")),
            Some(P("total_by_element_g", "count")),
            Some(P("remaining_by_element_g", "count")),
        ],
        R::ChainResourceDelta => [Some(S("entries.count")), None, None],
        R::BuiltinReleaseManifest => [Some(S("module_sets.count")), None, None],
        R::BuiltinModuleSet => [Some(S("modules.count")), None, None],
        R::BuiltinReleaseEntry => [
            Some(S("hash_tokens.count")),
            Some(S("artifact_identities.count")),
            None,
        ],
        _ => [None; 3],
    }
}
/// Original adjacent present/value columns retain their original names and slots.
pub(crate) fn options(record: Record) -> [Option<(&'static str, u8)>; 3] {
    use Record as R;
    match record {
        R::ChainResourceManifest => [
            Some(("genesis_ref", 12)),
            Some(("created_at_block_hash", 14)),
            None,
        ],
        R::CommitRef => [
            Some(("block_hash", 2)),
            Some(("event_id", 4)),
            Some(("action_id", 6)),
        ],
        R::ChainResourceDelta => [
            Some(("action_id", 7)),
            Some(("event_id", 9)),
            Some(("commit_block_hash", 12)),
        ],
        _ => [None; 3],
    }
}
pub(crate) fn exact_variant(record: Record, key: super::super::key::ParsedKey) -> Option<u64> {
    if key.value != super::super::key::KeyValue::Simple || key.label().ok() != Some("variant") {
        return None;
    }
    match record {
        Record::DeltaRuntimeResource => Some(0),
        Record::DeltaMaterialLedger => Some(1),
        Record::DeltaChunkResource => Some(2),
        _ => None,
    }
}

pub(crate) fn coordinate_i32(record: Record, key: super::super::key::ParsedKey) -> bool {
    matches!(record, Record::ChunkManifest | Record::DeltaChunkResource)
        && key.value == super::super::key::KeyValue::Simple
        && matches!(key.label().ok(), Some("coord.x" | "coord.y" | "coord.z"))
}
