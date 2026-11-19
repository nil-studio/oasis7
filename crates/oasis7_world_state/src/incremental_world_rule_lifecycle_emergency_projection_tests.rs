use super::*;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io},
    incremental_world_rule_composite_family::test_support::{read_work, scan},
};
use oasis7_formal_protocol::{
    incremental_state::StateEntryV1,
    incremental_value::{ValueIoBudget, ValueReadContext},
};

fn metadata(field: FieldDescriptor, pages: &mut Pages, memory: &MemoryBudget) -> StateEntryV1 {
    let row = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 4096,
                record_bytes: 64 << 10,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let FieldDestination::State(namespace) = field.destination else {
        panic!("state fixture");
    };
    StateEntryV1 {
        namespace,
        key: row.field_key.as_bytes().to_vec(),
        value: row.value_root,
    }
}

fn read_key(
    before: Option<&StateEntryV1>,
    after: Option<&StateEntryV1>,
    pages: &Pages,
    memory: &MemoryBudget,
    quota: &mut ValueIoBudget,
    entry_only: bool,
) -> Result<bool, ValueTreeError> {
    let p = emergency_profile();
    let root = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &root,
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 64 << 10,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut work = read_work();
    let mut scan = scan();
    let trees_remaining = work.max_retained_trees;
    let mut decoder = ChangeDecoder {
        ctx: &mut ctx,
        scan: &mut scan,
        work: &mut work,
        profile: p,
        trees_remaining,
        tree_depth: 0,
    };
    let before = before.map(TypedCellRefV1::Entry);
    let after = after.map(TypedCellRefV1::Entry);
    let result = if entry_only {
        key(
            &p,
            after.as_ref().or(before.as_ref()).unwrap(),
            &mut decoder,
        )
        .map(Some)
    } else {
        selector_key(&p, before.as_ref(), after.as_ref(), &mut decoder)
    };
    result.map(|value| value.is_none())
}

#[test]
fn emergency_metadata_uses_presence_reads_and_keeps_generic_entry_gate() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::with_limit(&memory, 128);
    let valid = metadata(emergency_profile().field, &mut pages, &memory);
    let retained = memory.used();
    for (before, after) in [
        (None, Some(&valid)),
        (Some(&valid), None),
        (Some(&valid), Some(&valid)),
    ] {
        let mut quota = io();
        let start = quota.nodes_remaining;
        assert!(read_key(before, after, &pages, &memory, &mut quota, false).unwrap());
        assert!(
            quota.nodes_remaining < start,
            "Metadata payload must be read"
        );
        assert_eq!(memory.used(), retained);
    }
    assert!(matches!(
        read_key(None, Some(&valid), &pages, &memory, &mut io(), true),
        Err(ValueTreeError::Invalid(_))
    ));
    let mut no_reads = io();
    no_reads.nodes_remaining = 0;
    assert!(matches!(
        read_key(None, Some(&valid), &pages, &memory, &mut no_reads, false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert_eq!(memory.used(), retained);
}

#[test]
fn emergency_metadata_rejects_foreign_keys_and_non_presence_on_either_edge() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::with_limit(&memory, 128);
    let p = emergency_profile();
    let valid = metadata(p.field, &mut pages, &memory);
    let mut invalid = Vec::new();
    let mut field = p.field;
    field.owner = FieldOwner::Runtime;
    invalid.push(metadata(field, &mut pages, &memory));
    field = p.field;
    field.name = "world_rules.foreign.columns";
    invalid.push(metadata(field, &mut pages, &memory));
    field = p.field;
    field.shape = FieldShape::Value;
    invalid.push(metadata(field, &mut pages, &memory));
    field = p.field;
    field.destination = FieldDestination::State(StateNamespaceV1::Business);
    invalid.push(metadata(field, &mut pages, &memory));
    let mut wrong_namespace = valid.clone();
    wrong_namespace.namespace = StateNamespaceV1::Business;
    invalid.push(wrong_namespace);
    let mut malformed_key = valid.clone();
    malformed_key.key.push(0);
    invalid.push(malformed_key);
    // Keep the correct Metadata key but bind a real member record instead of
    // a presence record. Authenticating the key alone must not accept it.
    let member = write_member_record_v1(
        p.field,
        WorldFieldRecordKindV1::Entry,
        "not-presence",
        MemberPayloadRefV1::U64(7),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 64 << 10,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let mut wrong_payload = valid.clone();
    wrong_payload.value = member.value_root;
    invalid.push(wrong_payload);
    let retained = memory.used();
    for invalid in &invalid {
        for (before, after) in [
            (None, Some(invalid)),
            (Some(invalid), None),
            (Some(invalid), Some(&valid)),
            (Some(&valid), Some(invalid)),
        ] {
            assert!(matches!(
                read_key(before, after, &pages, &memory, &mut io(), false),
                Err(ValueTreeError::Invalid(_))
            ));
            assert_eq!(memory.used(), retained);
        }
    }
}
