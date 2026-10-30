use super::*;
use crate::incremental_projection::{FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeReader, ValueReadContext, write_value},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io,
    sync::Arc,
};

struct Pages {
    nodes: ReservedValue<RefCell<BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>>>,
    reads: Cell<usize>,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            nodes: memory
                .reserve(16 << 10, MemoryClass::Ordinary)
                .unwrap()
                .retain(RefCell::new(BTreeMap::new())),
            reads: Cell::new(0),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 64);
        self.nodes.borrow_mut().insert(id, bytes);
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
        if bytes.len() > max {
            return Err(ValueTreeError::Invalid("test length"));
        }
        Ok(Arc::new(
            memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
        ))
    }
}
fn quota() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        write_bytes_remaining: 1 << 20,
    }
}
fn limits() -> PresenceRecordLimitsV1 {
    PresenceRecordLimitsV1 {
        semantic_bytes: 1024,
        record_bytes: 4096,
    }
}
fn field(shape: FieldShape) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: "ledgers",
        destination: FieldDestination::State(StateNamespaceV1::Business),
        shape,
    }
}
fn produce(pages: &mut Pages, memory: &MemoryBudget, group: bool) -> PresenceRecordV1 {
    let mut context = PresenceRecordWriteContext {
        sink: pages,
        memory,
        io: &mut quota(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 4096,
        },
        limits: limits(),
        cancelled: || false,
    };
    if group {
        write_field_group_v1(field(FieldShape::NestedMap), "é", &mut context)
    } else {
        write_field_metadata_v1(field(FieldShape::NestedMap), &mut context)
    }
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
    let mut bytes = Vec::new();
    context
        .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
        .unwrap();
    bytes
}
fn store(pages: &mut Pages, memory: &MemoryBudget, bytes: &[u8]) -> [u8; 32] {
    write_value(
        &mut &bytes[..],
        bytes.len() as u64,
        pages,
        memory,
        &mut quota(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap()
}
fn read(
    record: &PresenceRecordV1,
    pages: &Pages,
    memory: &MemoryBudget,
    output: &mut impl Write,
    cancel: impl FnMut() -> bool,
) -> Result<PresenceRecordInfoV1, ValueTreeError> {
    read_field_presence_v1(
        &record.value_root,
        &record.field_key,
        &mut ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut quota(),
        },
        limits(),
        output,
        cancel,
    )
}

#[test]
fn independent_metadata_and_group_vectors_keep_empty_existence_separate() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    for group in [false, true] {
        let record = produce(&mut pages, &memory, group);
        // Independent token and structural field-key bytes, not the encoder.
        let key = if group {
            b"oasis7.iws.semantic-key.v1\0\x01\0\0\0\0\0\0\0\x02\xc3\xa9".to_vec()
        } else {
            Vec::new()
        };
        let mut field_key = b"oasis7.iws.world-field-key.v1\0".to_vec();
        field_key.extend_from_slice(&[1, 0, 1, 3, if group { 2 } else { 1 }, 0, 7]);
        field_key.extend_from_slice(b"ledgers");
        let digest = if group {
            *blake3::hash(&key).as_bytes()
        } else {
            [0; 32]
        };
        field_key.extend_from_slice(&digest);
        let mut expected = b"oasis7.iws.field-presence.v1\0".to_vec();
        expected.extend_from_slice(&(field_key.len() as u16).to_be_bytes());
        expected.extend_from_slice(&(key.len() as u64).to_be_bytes());
        expected.extend_from_slice(&field_key);
        expected.extend_from_slice(&key);
        assert_eq!(record.field_key.as_bytes(), field_key);
        assert_eq!(raw(&pages, &memory, &record.value_root), expected);
        let mut actual_key = Vec::new();
        assert_eq!(
            read(&record, &pages, &memory, &mut actual_key, || false).unwrap(),
            record.info
        );
        assert_eq!(actual_key, key);
    }
}

#[test]
fn malformed_framing_precedes_limits_and_never_returns_presence() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = produce(&mut pages, &memory, true);
    let valid = raw(&pages, &memory, &record.value_root);
    for mode in 0..7 {
        let mut bytes = valid.clone();
        let p = FIELD_PRESENCE_DOMAIN.len();
        match mode {
            0 => bytes[0] ^= 1,
            1 => bytes[p..p + 2].copy_from_slice(&u16::MAX.to_be_bytes()),
            2 => bytes.push(0),
            3 => bytes[p + 2..p + 10].copy_from_slice(&0u64.to_be_bytes()),
            4 => bytes[p + 2..p + 10].copy_from_slice(&2048u64.to_be_bytes()),
            5 => bytes[FIELD_PRESENCE_HEADER_BYTES] ^= 1,
            _ => *bytes.last_mut().unwrap() ^= 1,
        }
        let root = store(&mut pages, &memory, &bytes);
        let mut output = Vec::new();
        let result = read_field_presence_v1(
            &root,
            &record.field_key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota(),
            },
            limits(),
            &mut output,
            || false,
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "mode {mode}: {result:?}"
        );
    }
    let metadata = produce(&mut pages, &memory, false);
    let mut bytes = raw(&pages, &memory, &metadata.value_root);
    bytes[FIELD_PRESENCE_DOMAIN.len() + 2..FIELD_PRESENCE_HEADER_BYTES]
        .copy_from_slice(&1u64.to_be_bytes());
    let root = store(&mut pages, &memory, &bytes);
    assert!(matches!(
        read_field_presence_v1(
            &root,
            &metadata.field_key,
            &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota()
            },
            limits(),
            &mut Vec::new(),
            || false
        ),
        Err(ValueTreeError::Invalid(_))
    ));
}

#[test]
fn recovery_reads_use_their_class_when_ordinary_is_full() {
    let source_memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&source_memory);
    let group = produce(&mut pages, &source_memory, true);
    let metadata = produce(&mut pages, &source_memory, false);
    let memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let _output = memory.reserve(8192, MemoryClass::Recovery).unwrap();
    let _full = memory
        .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
    let before = memory.used();
    for record in [&group, &metadata] {
        let mut key = Vec::new();
        assert_eq!(
            read_field_presence_v1(
                &record.value_root,
                &record.field_key,
                &mut ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Recovery,
                    io: &mut quota()
                },
                limits(),
                &mut key,
                || false
            )
            .unwrap(),
            record.info
        );
        assert_eq!(key.len() as u64, record.info.semantic_bytes);
        assert_eq!(memory.used(), before);
    }
}

#[test]
fn cancelled_key_output_is_not_retried_and_short_writes_hash_only_accepted_bytes() {
    struct Output<'a> {
        bytes: Vec<u8>,
        flag: &'a Cell<bool>,
        cancel: bool,
        calls: usize,
        interrupt: bool,
    }
    impl Write for Output<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.cancel && self.calls == 1 {
                self.flag.set(true);
            }
            if self.interrupt {
                self.interrupt = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = bytes.len().min(1);
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut pages = Pages::new(&memory);
    let record = produce(&mut pages, &memory, true);
    for interrupt in [false, true] {
        let flag = Cell::new(false);
        let mut output = Output {
            bytes: Vec::new(),
            flag: &flag,
            cancel: true,
            calls: 0,
            interrupt,
        };
        assert!(matches!(
            read(&record, &pages, &memory, &mut output, || flag.get()),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(output.calls, 1);
    }
    let flag = Cell::new(false);
    let mut output = Output {
        bytes: Vec::new(),
        flag: &flag,
        cancel: false,
        calls: 0,
        interrupt: true,
    };
    read(&record, &pages, &memory, &mut output, || false).unwrap();
    assert_eq!(
        output.bytes,
        b"oasis7.iws.semantic-key.v1\0\x01\0\0\0\0\0\0\0\x02\xc3\xa9"
    );
    let metadata = produce(&mut pages, &memory, false);
    output.calls = 0;
    read(&metadata, &pages, &memory, &mut output, || false).unwrap();
    assert_eq!(output.calls, 0);
}

#[test]
fn pre_cancel_and_exhausted_work_or_io_cannot_return_a_root() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    for mode in 0..4 {
        let mut pages = Pages::new(&memory);
        let mut io = quota();
        let mut work = SemanticKeyBudget {
            bytes_remaining: if mode == 1 { 0 } else { 4096 },
        };
        if mode == 2 {
            io.nodes_remaining = 0;
        }
        let mut context = PresenceRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io,
            key_work: &mut work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: if mode == 3 { 0 } else { 1024 },
                record_bytes: 4096,
            },
            cancelled: || mode == 0,
        };
        assert!(write_field_group_v1(field(FieldShape::NestedMap), "é", &mut context).is_err());
        assert!(pages.nodes.borrow().is_empty());
    }
    let mut pages = Pages::new(&memory);
    let mut context = PresenceRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut quota(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 4096,
        },
        limits: limits(),
        cancelled: || false,
    };
    assert!(write_field_group_v1(field(FieldShape::Map), "é", &mut context).is_err());
    assert!(pages.nodes.borrow().is_empty());
}
