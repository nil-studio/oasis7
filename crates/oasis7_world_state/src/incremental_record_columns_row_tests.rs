//! Inheriting an old large Text authenticates its key within covered chunks only.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
#[test]
fn inherited_column_key_does_not_scan_or_hydrate_old_text_payload() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let text: String = (0..(1 << 20))
        .map(|i| char::from(b'a' + ((i / 65536 + i % 29) % 26) as u8))
        .collect();
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "key.only.columns",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    };
    let row = {
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &"old",
            MemberPayloadRefV1::Text(&text),
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap()
    };
    drop(text);
    let entry = StateEntryV1 {
        namespace: StateNamespaceV1::Authorization,
        key: row.field_key.as_bytes().to_vec(),
        value: row.value_root,
    };
    let profile = ColumnReadProfileV1 {
        field,
        schema: [3; 32],
        groups: &[],
        reference_schema: None,
    };
    // Establish the old value's full codec/UTF8 semantics before taking the
    // deliberately smaller exact-key-only budget for an ordinary delta.
    let mut full_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut stop = || false;
    {
        let mut records = InventoryValueReader {
            values: &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut full_io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            cancelled: &mut stop,
        };
        let full = read_column_row_v1(profile, &entry, &mut records, 2 << 20).unwrap();
        assert!(matches!(&full.cell,ColumnCellV1::Text(value)if value.len()==1<<20));
    }
    let mut tiny_io = ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 512 << 10,
        write_bytes_remaining: 512 << 10,
    };
    let mut records = InventoryValueReader {
        values: &mut ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut tiny_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        cancelled: &mut stop,
    };
    let key = read_column_key_v1(profile, &entry, &mut records, 32 << 10).unwrap();
    assert_eq!(&*key, &ColumnKeyV1::text("old"));
    assert!(key.reserved_bytes() < 32 << 10);
    assert!(tiny_io.read_bytes_remaining > 0);
}
