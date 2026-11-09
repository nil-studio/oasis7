//! Closed cell grammar for every Runtime Artifacts record.
use super::super::{RuntimeArtifactRecordV1 as Record, key::ParsedKey};
use crate::incremental_field_member::MemberPayloadCodecV1 as Codec;

#[derive(Clone, Copy)]
pub(super) enum Payload {
    U64 {
        maximum: Option<u64>,
    },
    I64,
    Bool,
    Text {
        labels: Option<&'static [&'static str]>,
    },
    Ledger,
    Root(Child),
}

#[derive(Clone, Copy)]
pub(super) enum Child {
    Exact(Record),
    DeltaEntry,
}

impl Payload {
    pub fn codec(self) -> Option<Codec> {
        Some(match self {
            Self::U64 { .. } => Codec::U64,
            Self::I64 => Codec::I64,
            Self::Bool => Codec::Bool,
            Self::Text { .. } => Codec::Text,
            Self::Ledger => Codec::Ledger,
            Self::Root(_) => return None,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum Cell {
    Group(u8),
    Fixed {
        slot: u8,
        payload: Payload,
        count_group: Option<u8>,
        option: Option<(u8, OptionPart)>,
    },
    Dynamic {
        group: u8,
        payload: Payload,
        ordinal: bool,
    },
}

#[derive(Clone, Copy)]
pub(super) enum OptionPart {
    Present,
    Value,
}

#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub fixed_fields: u8,
    pub groups: u8,
    pub dynamic_groups: u8,
    /// Bit i is set when dynamic group i is an ordinal sequence rather than
    /// a keyed map. Count/max ordinal closure applies only to these groups.
    pub ordinal_groups: u8,
}

pub(super) fn layout(record: Record) -> Layout {
    use Record as R;
    match record {
        R::ChainResourceManifest => Layout {
            fixed_fields: 18,
            groups: 0b111,
            dynamic_groups: 3,
            ordinal_groups: 0,
        },
        R::MaterialLedger => Layout {
            fixed_fields: 1,
            groups: 0b001,
            dynamic_groups: 1,
            ordinal_groups: 0,
        },
        R::ChunkManifest => Layout {
            fixed_fields: 19,
            groups: 0b111,
            dynamic_groups: 3,
            ordinal_groups: 0b001,
        },
        R::FragmentRef => Layout {
            fixed_fields: 5,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::CommitRef => Layout {
            fixed_fields: 7,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::ChainResourceDelta => Layout {
            fixed_fields: 18,
            groups: 0b001,
            dynamic_groups: 1,
            ordinal_groups: 0b001,
        },
        R::OrderingKey => Layout {
            fixed_fields: 3,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::DeltaRuntimeResource => Layout {
            fixed_fields: 4,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::DeltaMaterialLedger => Layout {
            fixed_fields: 5,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::DeltaChunkResource => Layout {
            fixed_fields: 10,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
        R::BuiltinReleaseManifest => Layout {
            fixed_fields: 1,
            groups: 0b001,
            dynamic_groups: 1,
            ordinal_groups: 0,
        },
        R::BuiltinModuleSet => Layout {
            fixed_fields: 1,
            groups: 0b001,
            dynamic_groups: 1,
            ordinal_groups: 0,
        },
        R::BuiltinReleaseEntry => Layout {
            fixed_fields: 2,
            groups: 0b011,
            dynamic_groups: 2,
            ordinal_groups: 0b001,
        },
        R::ModuleArtifactIdentity => Layout {
            fixed_fields: 5,
            groups: 0,
            dynamic_groups: 0,
            ordinal_groups: 0,
        },
    }
}

fn simple(key: ParsedKey, label: &str) -> bool {
    key.value == super::key::KeyValue::Simple && key.label().ok() == Some(label)
}
fn pair(key: ParsedKey, label: &str, value: super::key::KeyValue) -> bool {
    let token_matches = match (key.value, value) {
        (super::key::KeyValue::Unsigned(_), super::key::KeyValue::Unsigned(0))
        | (super::key::KeyValue::Resource(_), super::key::KeyValue::Resource(0))
        | (super::key::KeyValue::Ledger(_), super::key::KeyValue::Ledger(0)) => true,
        (actual, expected) => actual == expected,
    };
    token_matches && key.label().ok() == Some(label)
}
fn fixed(slot: u8, payload: Payload) -> Cell {
    Cell::Fixed {
        slot,
        payload,
        count_group: None,
        option: None,
    }
}
fn count(slot: u8, group: u8) -> Cell {
    Cell::Fixed {
        slot,
        payload: Payload::U64 { maximum: None },
        count_group: Some(group),
        option: None,
    }
}
fn option(slot: u8, id: u8, part: OptionPart, payload: Payload) -> Cell {
    Cell::Fixed {
        slot,
        payload,
        count_group: None,
        option: Some((id, part)),
    }
}
fn dynamic(group: u8, payload: Payload, ordinal: bool) -> Cell {
    Cell::Dynamic {
        group,
        payload,
        ordinal,
    }
}
fn group(id: u8) -> Cell {
    Cell::Group(id)
}

const ELEMENTS: &[&str] = &[
    "oxygen",
    "silicon",
    "magnesium",
    "aluminum",
    "calcium",
    "iron",
    "nickel",
    "cobalt",
    "titanium",
    "chromium",
    "hydrogen",
    "carbon",
    "nitrogen",
    "sulfur",
    "copper",
    "zinc",
    "lithium",
    "neodymium",
    "uranium",
    "thorium",
];

pub(super) fn classify(record: Record, kind: u8, key: ParsedKey) -> Option<Cell> {
    use super::key::KeyValue as K;
    use OptionPart as O;
    use Payload as P;
    use Record as R;

    if kind == 2 {
        return match record {
            R::ChainResourceManifest if pair(key, "resource_balances", K::Unit) => Some(group(0)),
            R::ChainResourceManifest if pair(key, "material_ledgers", K::Unit) => Some(group(1)),
            R::ChainResourceManifest if pair(key, "generated_chunks", K::Unit) => Some(group(2)),
            R::MaterialLedger if pair(key, "materials", K::Unit) => Some(group(0)),
            R::ChunkManifest if pair(key, "fragment_refs", K::Unit) => Some(group(0)),
            R::ChunkManifest if pair(key, "total_by_element_g", K::Unit) => Some(group(1)),
            R::ChunkManifest if pair(key, "remaining_by_element_g", K::Unit) => Some(group(2)),
            R::ChainResourceDelta if pair(key, "entries", K::Unit) => Some(group(0)),
            R::BuiltinReleaseManifest if pair(key, "module_sets", K::Unit) => Some(group(0)),
            R::BuiltinModuleSet if pair(key, "modules", K::Unit) => Some(group(0)),
            R::BuiltinReleaseEntry if pair(key, "hash_tokens", K::Unit) => Some(group(0)),
            R::BuiltinReleaseEntry if pair(key, "artifact_identities", K::Unit) => Some(group(1)),
            _ => None,
        };
    }
    if kind != 3 {
        return None;
    }

    let text = P::Text { labels: None };
    let u64v = P::U64 { maximum: None };
    let i64v = P::I64;
    let root = |record| P::Root(Child::Exact(record));
    match record {
        R::ChainResourceManifest => {
            let fields = [
                ("schema_version", P::Text { labels: None }),
                ("world_id", text),
                ("chain_id", text),
                ("chunk_generation_schema_version", text),
                ("world_config_hash", text),
                ("generation_algorithm_id", text),
                ("generation_algorithm_hash", text),
                ("manifest_hash", text),
                ("world_seed", u64v),
                ("created_at_height", u64v),
                ("manifest_height", u64v),
            ];
            for (index, (name, payload)) in fields.into_iter().enumerate() {
                if simple(key, name) {
                    return Some(fixed(index as u8, payload));
                }
            }
            if simple(key, "resource_balances.count") {
                return Some(count(15, 0));
            }
            if simple(key, "material_ledgers.count") {
                return Some(count(16, 1));
            }
            if simple(key, "generated_chunks.count") {
                return Some(count(17, 2));
            }
            if pair(key, "genesis_ref", K::Text) && key.right_text() == Some("present") {
                return Some(option(11, 0, O::Present, P::Bool));
            }
            if pair(key, "genesis_ref", K::Text) && key.right_text() == Some("value") {
                return Some(option(12, 0, O::Value, text));
            }
            if pair(key, "created_at_block_hash", K::Text) && key.right_text() == Some("present") {
                return Some(option(13, 1, O::Present, P::Bool));
            }
            if pair(key, "created_at_block_hash", K::Text) && key.right_text() == Some("value") {
                return Some(option(14, 1, O::Value, text));
            }
            if pair(key, "resource_balances", K::Resource(0)) {
                return Some(dynamic(0, i64v, false));
            }
            if pair(key, "material_ledgers", K::Ledger(0)) {
                return Some(dynamic(1, root(R::MaterialLedger), false));
            }
            if pair(key, "generated_chunks", K::Text) {
                return Some(dynamic(2, root(R::ChunkManifest), false));
            }
            None
        }
        R::MaterialLedger => {
            if simple(key, "materials.count") {
                return Some(count(0, 0));
            }
            if pair(key, "materials", K::Text) {
                return Some(dynamic(0, i64v, false));
            }
            None
        }
        R::ChunkManifest => {
            let fields = [
                ("schema_version", text),
                ("world_id", text),
                ("chain_id", text),
                ("chunk_generation_schema_version", text),
                ("chunk_budget_total_hash", text),
                ("chunk_budget_remaining_hash", text),
                ("manifest_hash", text),
                ("world_seed", u64v),
                ("seed", u64v),
                ("fragment_count", u64v),
                ("block_count", u64v),
                ("coord.x", i64v),
                ("coord.y", i64v),
                ("coord.z", i64v),
                ("chunk_status", P::U64 { maximum: Some(3) }),
            ];
            for (index, (name, payload)) in fields.into_iter().enumerate() {
                if simple(key, name) {
                    return Some(fixed(index as u8, payload));
                }
            }
            if simple(key, "fragment_refs.count") {
                return Some(count(15, 0));
            }
            if pair(key, "total_by_element_g", K::Text) && key.right_text() == Some("count") {
                return Some(count(16, 1));
            }
            if pair(key, "remaining_by_element_g", K::Text) && key.right_text() == Some("count") {
                return Some(count(17, 2));
            }
            if simple(key, "commit_ref") {
                return Some(fixed(18, root(R::CommitRef)));
            }
            if pair(key, "fragment_refs", K::Unsigned(0)) {
                return Some(dynamic(0, root(R::FragmentRef), true));
            }
            if pair(key, "total_by_element_g", K::Text)
                && key.right_text().is_some_and(|s| ELEMENTS.contains(&s))
            {
                return Some(dynamic(1, i64v, false));
            }
            if pair(key, "remaining_by_element_g", K::Text)
                && key.right_text().is_some_and(|s| ELEMENTS.contains(&s))
            {
                return Some(dynamic(2, i64v, false));
            }
            None
        }
        R::FragmentRef => {
            let fields = [
                "fragment_id",
                "location_id",
                "profile_hash",
                "budget_total_hash",
                "budget_remaining_hash",
            ];
            fields
                .iter()
                .position(|name| simple(key, name))
                .map(|i| fixed(i as u8, text))
        }
        R::CommitRef => {
            if simple(key, "height") {
                return Some(fixed(0, u64v));
            }
            for (label, base, option_id, value_profile) in [
                ("block_hash", 1, 0, text),
                ("event_id", 3, 1, u64v),
                ("action_id", 5, 2, u64v),
            ] {
                if pair(key, label, K::Text) && key.right_text() == Some("present") {
                    return Some(option(base, option_id, O::Present, P::Bool));
                }
                if pair(key, label, K::Text) && key.right_text() == Some("value") {
                    return Some(option(base + 1, option_id, O::Value, value_profile));
                }
            }
            None
        }
        R::ChainResourceDelta => {
            for (index, name) in [
                "schema_version",
                "world_id",
                "chain_id",
                "delta_id",
                "base_manifest_hash",
                "resulting_manifest_hash",
            ]
            .iter()
            .enumerate()
            {
                if simple(key, name) {
                    return Some(fixed(index as u8, text));
                }
            }
            if simple(key, "block_height") {
                return Some(fixed(10, u64v));
            }
            if simple(key, "tick") {
                return Some(fixed(13, u64v));
            }
            if simple(key, "source") {
                return Some(fixed(14, P::U64 { maximum: Some(4) }));
            }
            if simple(key, "replay_status") {
                return Some(fixed(15, P::U64 { maximum: Some(2) }));
            }
            if simple(key, "entries.count") {
                return Some(count(16, 0));
            }
            if simple(key, "ordering_key") {
                return Some(fixed(17, root(R::OrderingKey)));
            }
            for (label, base, option_id) in [("action_id", 6, 0), ("event_id", 8, 1)] {
                if pair(key, label, K::Text) && key.right_text() == Some("present") {
                    return Some(option(base, option_id, O::Present, P::Bool));
                }
                if pair(key, label, K::Text) && key.right_text() == Some("value") {
                    return Some(option(base + 1, option_id, O::Value, u64v));
                }
            }
            if pair(key, "commit_block_hash", K::Text) && key.right_text() == Some("present") {
                return Some(option(11, 2, O::Present, P::Bool));
            }
            if pair(key, "commit_block_hash", K::Text) && key.right_text() == Some("value") {
                return Some(option(12, 2, O::Value, text));
            }
            if pair(key, "entries", K::Unsigned(0)) {
                return Some(dynamic(0, P::Root(Child::DeltaEntry), true));
            }
            None
        }
        R::OrderingKey => {
            for (index, name) in ["height", "event_sequence", "action_sequence"]
                .iter()
                .enumerate()
            {
                if simple(key, name) {
                    return Some(fixed(index as u8, u64v));
                }
            }
            None
        }
        R::DeltaRuntimeResource => {
            if simple(key, "variant") {
                return Some(fixed(0, P::U64 { maximum: Some(0) }));
            }
            if simple(key, "kind") {
                return Some(fixed(1, P::U64 { maximum: Some(1) }));
            }
            if simple(key, "delta") {
                return Some(fixed(2, i64v));
            }
            if simple(key, "resulting_balance") {
                return Some(fixed(3, i64v));
            }
            None
        }
        R::DeltaMaterialLedger => {
            if simple(key, "variant") {
                return Some(fixed(0, P::U64 { maximum: Some(1) }));
            }
            if simple(key, "ledger_id") {
                return Some(fixed(1, P::Ledger));
            }
            if simple(key, "material_kind") {
                return Some(fixed(2, text));
            }
            if simple(key, "delta") {
                return Some(fixed(3, i64v));
            }
            if simple(key, "resulting_balance") {
                return Some(fixed(4, i64v));
            }
            None
        }
        R::DeltaChunkResource => {
            let fields = [
                ("variant", P::U64 { maximum: Some(2) }),
                ("coord.x", i64v),
                ("coord.y", i64v),
                ("coord.z", i64v),
                (
                    "element",
                    P::Text {
                        labels: Some(ELEMENTS),
                    },
                ),
                ("total_delta_g", i64v),
                ("remaining_delta_g", i64v),
                ("resulting_remaining_g", i64v),
                ("remaining_after_hash", text),
                ("chunk_remaining_after_hash", text),
            ];
            for (index, (name, payload)) in fields.into_iter().enumerate() {
                if simple(key, name) {
                    return Some(fixed(index as u8, payload));
                }
            }
            None
        }
        R::BuiltinReleaseManifest => {
            if simple(key, "module_sets.count") {
                return Some(count(0, 0));
            }
            if pair(key, "module_sets", K::Text) {
                return Some(dynamic(0, root(R::BuiltinModuleSet), false));
            }
            None
        }
        R::BuiltinModuleSet => {
            if simple(key, "modules.count") {
                return Some(count(0, 0));
            }
            if pair(key, "modules", K::Text) {
                return Some(dynamic(0, root(R::BuiltinReleaseEntry), false));
            }
            None
        }
        R::BuiltinReleaseEntry => {
            if simple(key, "hash_tokens.count") {
                return Some(count(0, 0));
            }
            if simple(key, "artifact_identities.count") {
                return Some(count(1, 1));
            }
            if pair(key, "hash_tokens", K::Unsigned(0)) {
                return Some(dynamic(0, text, true));
            }
            if pair(key, "artifact_identities", K::Text) {
                return Some(dynamic(1, root(R::ModuleArtifactIdentity), false));
            }
            None
        }
        R::ModuleArtifactIdentity => {
            let fields = [
                "source_hash",
                "build_manifest_hash",
                "signer_node_id",
                "signature_scheme",
                "artifact_signature",
            ];
            fields
                .iter()
                .position(|name| simple(key, name))
                .map(|i| fixed(i as u8, text))
        }
    }
}
