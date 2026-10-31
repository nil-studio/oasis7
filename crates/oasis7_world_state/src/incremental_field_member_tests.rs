use super::*;
use crate::incremental_projection::{FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1 as Ns,
    incremental_value::{ValueNodeReader, ValueReadContext, write_value},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
};

type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    nodes: ReservedValue<RefCell<PageMap>>,
    reads: Cell<usize>,
    fail: bool,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            nodes: memory
                .reserve(16 << 10, MemoryClass::Ordinary)
                .unwrap()
                .retain(RefCell::new(BTreeMap::new())),
            reads: Cell::new(0),
            fail: false,
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail {
            return Err(ValueTreeError::Invalid("injected page failure"));
        }
        assert!(self.nodes.borrow().len() < 64);
        self.nodes.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        let nodes = self.nodes.borrow();
        let bytes = nodes
            .get(id)
            .ok_or(ValueTreeError::Invalid("test missing"))?;
        if class != MemoryClass::Ordinary || !bytes.uses_budget(memory) || bytes.len() > max {
            return Err(ValueTreeError::Invalid("test source pool/class/limit"));
        }
        Ok(bytes.clone())
    }
}
fn quota() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        write_bytes_remaining: 1 << 20,
    }
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 1024,
        record_bytes: 4096,
    }
}
fn descriptor(kind: WorldFieldRecordKindV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: "flags",
        destination: FieldDestination::State(Ns::Business),
        shape: if kind == WorldFieldRecordKindV1::Bytes {
            FieldShape::ByteMap
        } else {
            FieldShape::Map
        },
    }
}
fn write(
    pages: &mut Pages,
    memory: &MemoryBudget,
    payload: MemberPayloadRefV1<'_>,
    kind: WorldFieldRecordKindV1,
) -> MemberRecordV1 {
    write_member_record_v1(
        descriptor(kind),
        kind,
        "é",
        payload,
        &mut MemberRecordWriteContext {
            sink: pages,
            memory,
            io: &mut quota(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: limits(),
            cancelled: || false,
        },
    )
    .unwrap()
}
fn raw(pages: &Pages, memory: &MemoryBudget, root: &[u8; 32]) -> Vec<u8> {
    let mut io = quota();
    let mut context = ValueReadContext {
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let root = context.load_root(root).unwrap();
    let mut out = Vec::new();
    context
        .read_range(&root, 0, root.0.logical_len, &mut out, || false)
        .unwrap();
    out
}
fn store(pages: &mut Pages, memory: &MemoryBudget, mut bytes: &[u8]) -> [u8; 32] {
    let length = bytes.len() as u64;
    write_value(&mut bytes, length, pages, memory, &mut quota(), || false)
        .unwrap()
        .digest()
        .unwrap()
}
fn read(
    record: &MemberRecordV1,
    pages: &Pages,
    memory: &MemoryBudget,
    limits: MemberRecordLimitsV1,
    key: &mut impl Write,
    payload: &mut impl Write,
    cancel: impl FnMut() -> bool,
) -> Result<MemberRecordInfoV1, ValueTreeError> {
    read_member_record_v1(
        &record.value_root,
        &record.field_key,
        &mut ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut quota(),
        },
        limits,
        key,
        payload,
        cancel,
    )
}
#[test]
fn exact_frame_contains_original_key_and_all_closed_payload_profiles() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let _output = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let ledger = MaterialLedgerId::Factory("甲".into());
    let cases = [
        (MemberPayloadRefV1::Unit, 1, Vec::new()),
        (
            MemberPayloadRefV1::U64(u64::MAX),
            2,
            u64::MAX.to_be_bytes().to_vec(),
        ),
        (
            MemberPayloadRefV1::I64(i64::MIN),
            3,
            i64::MIN.to_be_bytes().to_vec(),
        ),
        (MemberPayloadRefV1::Bool(true), 4, vec![1]),
        (MemberPayloadRefV1::Text("甲"), 5, "甲".as_bytes().to_vec()),
        (
            MemberPayloadRefV1::Ledger(&ledger),
            6,
            [
                b"oasis7.iws.ledger-value.v1\0".as_slice(),
                &[3],
                &3_u64.to_be_bytes(),
                "甲".as_bytes(),
            ]
            .concat(),
        ),
        (MemberPayloadRefV1::Bytes(&[0, 255]), 7, vec![0, 255]),
    ];
    let key = [
        b"oasis7.iws.semantic-key.v1\0".as_slice(),
        &[1],
        &2_u64.to_be_bytes(),
        "é".as_bytes(),
    ]
    .concat();
    for (payload, codec, payload_bytes) in cases {
        let mut pages = Pages::new(&memory);
        let kind = if codec == 7 {
            WorldFieldRecordKindV1::Bytes
        } else {
            WorldFieldRecordKindV1::Entry
        };
        let record = write(&mut pages, &memory, payload, kind);
        let shape = if codec == 7 { 7 } else { 2 };
        let field = [
            b"oasis7.iws.world-field-key.v1\0".as_slice(),
            &[1, 0, 1, shape, kind as u8, 0, 5],
            b"flags",
            blake3::hash(&key).as_bytes(),
        ]
        .concat();
        let expected = [
            b"oasis7.iws.field-member.v1\0".as_slice(),
            &(field.len() as u16).to_be_bytes(),
            &[codec],
            &(key.len() as u64).to_be_bytes(),
            &(payload_bytes.len() as u64).to_be_bytes(),
            &field,
            &key,
            &payload_bytes,
        ]
        .concat();
        assert_eq!(raw(&pages, &memory, &record.value_root), expected);
        let mut key_out = Vec::new();
        let mut payload_out = Vec::new();
        assert_eq!(
            read(
                &record,
                &pages,
                &memory,
                limits(),
                &mut key_out,
                &mut payload_out,
                || false
            )
            .unwrap(),
            record.info
        );
        assert_eq!(key_out, key);
        assert_eq!(payload_out, payload_bytes);
    }
}
#[test]
fn framing_limits_wrong_fields_and_key_hash_fail_before_payload_output() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let _output = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    for mode in 0..8 {
        let mut pages = Pages::new(&memory);
        let mut record = write(
            &mut pages,
            &memory,
            MemberPayloadRefV1::U64(17),
            WorldFieldRecordKindV1::Entry,
        );
        let mut bytes = raw(&pages, &memory, &record.value_root);
        let p = FIELD_MEMBER_DOMAIN.len();
        match mode {
            0 => bytes[0] ^= 1,
            1 => bytes[p + 2] = 0,
            2 => bytes[p..p + 2].copy_from_slice(&u16::MAX.to_be_bytes()),
            3 => bytes[p + 3..p + 11].copy_from_slice(&u64::MAX.to_be_bytes()),
            4 => {
                let n = bytes.len();
                bytes[n - 9] ^= 1;
            } // original key tail
            5 => bytes.push(0),
            6 => {
                record.field_key = WorldFieldKeyV1::new(
                    FieldDescriptor {
                        name: "flogs",
                        ..descriptor(WorldFieldRecordKindV1::Entry)
                    },
                    WorldFieldRecordKindV1::Entry,
                    WorldFieldKeyV1::from_bytes(record.field_key.as_bytes())
                        .unwrap()
                        .semantic_key,
                )
                .unwrap()
            }
            7 => {}
            _ => unreachable!(),
        }
        record.value_root = store(&mut pages, &memory, &bytes);
        let mut key = Vec::new();
        let mut payload = Vec::new();
        let mut bounds = limits();
        if mode == 7 {
            bounds.semantic_bytes = 1;
        }
        assert!(
            read(
                &record,
                &pages,
                &memory,
                bounds,
                &mut key,
                &mut payload,
                || false
            )
            .is_err()
        );
        assert!(payload.is_empty());
    }
}
#[test]
fn source_limits_cancel_io_and_sink_failures_never_return_a_root() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    for mode in 0..8 {
        let mut pages = Pages::new(&memory);
        pages.fail = mode == 7;
        let mut io = quota();
        if mode == 4 {
            io.nodes_remaining = 0;
        }
        let mut bounds = limits();
        match mode {
            0 => bounds.semantic_bytes = 1,
            1 => bounds.payload_bytes = 1,
            2 => bounds.record_bytes = 1,
            _ => {}
        }
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: if mode == 3 { 0 } else { 4096 },
        };
        let kind = if mode == 6 {
            WorldFieldRecordKindV1::Group
        } else {
            WorldFieldRecordKindV1::Entry
        };
        assert!(
            write_member_record_v1(
                descriptor(kind),
                kind,
                "é",
                MemberPayloadRefV1::Text("abc"),
                &mut MemberRecordWriteContext {
                    sink: &mut pages,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut key_work,
                    limits: bounds,
                    cancelled: || mode == 5
                }
            )
            .is_err()
        );
    }
}
#[test]
fn provisional_outputs_and_final_cancellation_do_not_return_completion() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Text("abc"),
        WorldFieldRecordKindV1::Entry,
    );
    struct Stop;
    impl Write for Stop {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stop"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Stop,
            &mut Vec::new(),
            || false
        )
        .is_err()
    );
    assert!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut Stop,
            || false
        )
        .is_err()
    );
    let cancelled = Cell::new(false);
    struct Cancel<'a>(&'a Cell<bool>);
    impl Write for Cancel<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.set(true);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut Cancel(&cancelled),
            || cancelled.get()
        ),
        Err(ValueTreeError::Cancelled)
    ));
    let before = pages.reads.get();
    let _filler = memory
        .reserve((2 << 20) - 4096 - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut Vec::new(),
            || true
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(before, pages.reads.get());
}

#[test]
fn root_loading_cancellation_precedes_range_workspace_and_preserves_spent_io() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Unit,
        WorldFieldRecordKindV1::Entry,
    );
    let cancelled = Cell::new(false);
    struct CancelRoot<'a> {
        pages: &'a Pages,
        flag: &'a Cell<bool>,
    }
    impl ValueNodeReader for CancelRoot<'_> {
        fn read_node(
            &self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
            class: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            let result = self.pages.read_node(id, max, memory, class);
            self.flag.set(true);
            result
        }
    }
    let source = CancelRoot {
        pages: &pages,
        flag: &cancelled,
    };
    let _filler = memory
        .reserve(
            (2 << 20) - 4096 - memory.used() - 6144,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut io = quota();
    let mut context = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    assert!(matches!(
        read_member_record_v1(
            &record.value_root,
            &record.field_key,
            &mut context,
            limits(),
            &mut Vec::new(),
            &mut Vec::new(),
            || cancelled.get()
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(pages.reads.get(), 1);
    assert_eq!(io.nodes_remaining, 999);
}
#[test]
fn raw_reads_forward_class_and_fail_on_exhausted_io_without_output() {
    let memory = MemoryBudget::new(2 << 20, 16 << 10).unwrap();
    let mut pages = Pages::new(&memory);
    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Bool(false),
        WorldFieldRecordKindV1::Entry,
    );
    for mode in 0..2 {
        let mut io = quota();
        if mode == 0 {
            io.nodes_remaining = 0;
        }
        let mut key = Vec::new();
        let mut payload = Vec::new();
        let mut context = ValueReadContext {
            source: &pages,
            memory: &memory,
            class: if mode == 0 {
                MemoryClass::Ordinary
            } else {
                MemoryClass::Recovery
            },
            io: &mut io,
        };
        assert!(
            read_member_record_v1(
                &record.value_root,
                &record.field_key,
                &mut context,
                limits(),
                &mut key,
                &mut payload,
                || false
            )
            .is_err()
        );
        assert!(key.is_empty() && payload.is_empty());
    }
}

#[test]
fn inconsistent_declared_lengths_are_invalid_before_operation_limits() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let _outputs = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let mut record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::U64(17),
        WorldFieldRecordKindV1::Entry,
    );
    let mut limited = limits();
    limited.semantic_bytes = 1;
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limited,
            &mut Vec::new(),
            &mut Vec::new(),
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let mut bytes = raw(&pages, &memory, &record.value_root);
    let p = FIELD_MEMBER_DOMAIN.len();
    bytes[p + 3..p + 11].copy_from_slice(&2048_u64.to_be_bytes());
    record.value_root = store(&mut pages, &memory, &bytes);
    assert!(matches!(
        read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut Vec::new(),
            &mut Vec::new(),
            || false
        ),
        Err(ValueTreeError::Invalid("member record length"))
    ));
}

#[test]
fn raw_authenticated_key_bytes_are_not_typed_semantic_validation() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let _outputs = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let mut record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Bool(true),
        WorldFieldRecordKindV1::Entry,
    );
    let mut bytes = raw(&pages, &memory, &record.value_root);
    let field_start = FIELD_MEMBER_HEADER_BYTES;
    let key_start = field_start + record.field_key.as_bytes().len();
    // Change the canonical token into an unknown tag while preserving length.
    bytes[key_start + crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN.len()] = 255;
    let key_end = key_start + record.info.semantic_bytes as usize;
    let digest = *blake3::hash(&bytes[key_start..key_end]).as_bytes();
    record.field_key = WorldFieldKeyV1::new(
        descriptor(WorldFieldRecordKindV1::Entry),
        WorldFieldRecordKindV1::Entry,
        Some(digest),
    )
    .unwrap();
    bytes[field_start..key_start].copy_from_slice(record.field_key.as_bytes());
    record.value_root = store(&mut pages, &memory, &bytes);
    let mut key = Vec::new();
    let mut payload = Vec::new();
    read(
        &record,
        &pages,
        &memory,
        limits(),
        &mut key,
        &mut payload,
        || false,
    )
    .unwrap();
    assert_eq!(payload, [1]);
    assert!(crate::incremental_semantic_key::read_semantic_key_v1(&key).is_err());
}

#[test]
fn key_output_cancellation_stops_before_retry_or_payload_output() {
    struct CancelKey<'a> {
        flag: &'a Cell<bool>,
        interrupted: bool,
        calls: usize,
    }
    impl Write for CancelKey<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls == 1 {
                self.flag.set(true);
                if self.interrupted {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                return Ok(bytes.len().min(1));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Text("payload"),
        WorldFieldRecordKindV1::Entry,
    );
    for interrupted in [true, false] {
        let cancelled = Cell::new(false);
        let mut key = CancelKey {
            flag: &cancelled,
            interrupted,
            calls: 0,
        };
        let mut payload = Vec::new();
        assert!(matches!(
            read(
                &record,
                &pages,
                &memory,
                limits(),
                &mut key,
                &mut payload,
                || cancelled.get()
            ),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(key.calls, 1, "interrupted={interrupted}");
        assert!(payload.is_empty());
    }
}

#[test]
fn key_digest_covers_each_accepted_byte_once_across_short_and_interrupted_writes() {
    struct ShortKey {
        bytes: Vec<u8>,
        width: usize,
        interrupt_next: bool,
    }
    impl Write for ShortKey {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.interrupt_next {
                self.interrupt_next = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.interrupt_next = true;
            let count = bytes.len().min(self.width);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::Text("payload"),
        WorldFieldRecordKindV1::Entry,
    );
    let mut expected_key = Vec::new();
    let mut expected_payload = Vec::new();
    let expected = read(
        &record,
        &pages,
        &memory,
        limits(),
        &mut expected_key,
        &mut expected_payload,
        || false,
    )
    .unwrap();
    for width in 1..=expected_key.len() {
        let mut key = ShortKey {
            bytes: Vec::new(),
            width,
            interrupt_next: true,
        };
        let mut payload = Vec::new();
        let actual = read(
            &record,
            &pages,
            &memory,
            limits(),
            &mut key,
            &mut payload,
            || false,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(key.bytes, expected_key);
        assert_eq!(payload, expected_payload);
    }
}

#[test]
fn expected_i64_profile_rejects_malformed_fixed_length_before_payload_limits() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let mut record = write(
        &mut pages,
        &memory,
        MemberPayloadRefV1::I64(7),
        WorldFieldRecordKindV1::Entry,
    );
    let mut bytes = raw(&pages, &memory, &record.value_root);
    let p = FIELD_MEMBER_DOMAIN.len();
    bytes[p + 11..p + 19].copy_from_slice(&9u64.to_be_bytes());
    bytes.push(0);
    record.value_root = store(&mut pages, &memory, &bytes);
    let mut key = Vec::new();
    let mut payload = Vec::new();
    let result = read_member_record_profile_v1(
        &record.value_root,
        &record.field_key,
        &mut ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota(),
        },
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::I64,
            semantic_bytes: record.info.semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 8,
                record_bytes: 4096,
            },
        },
        &mut key,
        &mut payload,
        || false,
    );
    assert!(matches!(
        result,
        Err(ValueTreeError::Invalid("member fixed payload length"))
    ));
    assert!(key.is_empty());
    assert!(payload.is_empty());
}

#[test]
fn world_u64_genesis_preserves_namespace_unit_and_full_unsigned_range() {
    use crate::{incremental_world_u64::*, state::WorldState};
    let source = WorldState {
        time: 0,
        next_industry_settlement_order: u64::MAX,
        next_module_release_request_id: 2,
        next_module_instance_id: 3,
        next_module_market_order_id: 4,
        next_module_market_sale_id: 5,
        agent_claim_last_processed_epoch: 6,
        ..WorldState::default()
    };
    let memory = MemoryBudget::new(4 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let mut writes = Vec::new();
    let mut remaining = 14;
    prepare_world_u64_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut quota(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut remaining,
        &mut |write| {
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(writes.len(), 14);
    let expected = [
        ("time", Ns::Allocator, 0),
        ("next_industry_settlement_order", Ns::Allocator, u64::MAX),
        ("next_module_release_request_id", Ns::Allocator, 2),
        ("next_module_instance_id", Ns::Allocator, 3),
        ("next_module_market_order_id", Ns::Allocator, 4),
        ("next_module_market_sale_id", Ns::Allocator, 5),
        ("agent_claim_last_processed_epoch", Ns::Scheduler, 6),
    ];
    for (pair, (name, namespace, value)) in writes.chunks_exact(2).zip(expected) {
        let descriptor = crate::incremental_projection::WORLD_STATE_FIELDS
            .iter()
            .find(|field| field.name == name)
            .copied()
            .unwrap();
        for write in pair {
            assert_eq!(write.namespace, namespace);
            assert_eq!(write.mutation.before, None);
        }
        let metadata_key =
            WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None).unwrap();
        assert_eq!(pair[0].mutation.key, metadata_key.as_bytes());
        let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
            &(),
            &memory,
            MemoryClass::Ordinary,
            &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            || false,
        )
        .unwrap();
        let entry_key = WorldFieldKeyV1::new(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            Some(semantic.digest),
        )
        .unwrap();
        assert_eq!(pair[1].mutation.key, entry_key.as_bytes());
        let mut payload = [0; 8];
        read_member_record_profile_v1(
            &pair[1].mutation.after.unwrap(),
            &entry_key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota(),
            },
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::U64,
                semantic_bytes: (crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN.len() + 1)
                    as u64,
                limits: limits(),
            },
            &mut std::io::sink(),
            &mut &mut payload[..],
            || false,
        )
        .unwrap();
        assert_eq!(u64::from_be_bytes(payload), value);
        let bytes = raw(&pages, &memory, &pair[1].mutation.after.unwrap());
        assert_eq!(&bytes[bytes.len() - 8..], &value.to_be_bytes());
        // Unit is one zero token, not an empty semantic key.
        assert_eq!(bytes[bytes.len() - 9], 0);
    }
}

#[test]
fn world_u64_genesis_stops_on_record_limit_or_sink_rejection() {
    use crate::{incremental_world_u64::*, state::WorldState};
    for reject in [false, true] {
        let memory = MemoryBudget::new(4 << 20, 4096).unwrap();
        let mut pages = Pages::new(&memory);
        let mut accepted = 0;
        let result = prepare_world_u64_genesis_v1(
            &WorldState::default(),
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut quota(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                limits: limits(),
                cancelled: || false,
            },
            &mut if reject { 14 } else { 3 },
            &mut |_| {
                if reject && accepted == 2 {
                    return Err(ValueTreeError::Invalid("sink rejected"));
                }
                accepted += 1;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(accepted, if reject { 2 } else { 3 });
    }
}

#[path = "incremental_delivery_cursor_tests.rs"]
mod delivery_cursor;
