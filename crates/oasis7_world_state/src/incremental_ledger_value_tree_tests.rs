use super::*;
use oasis7_formal_protocol::incremental_value::{
    ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError, write_value,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
};

type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;

struct Pages {
    nodes: ReservedValue<RefCell<PageMap>>,
    reads: Cell<usize>,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            nodes: memory
                .reserve(64 << 10, MemoryClass::Ordinary)
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
        assert!(self.nodes.borrow().len() < 32, "test metadata bound");
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
            return Err(ValueTreeError::Invalid("test source pool/class/bound"));
        }
        Ok(bytes.clone())
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        write_bytes_remaining: 1 << 20,
    }
}
fn store(mut bytes: &[u8], memory: &MemoryBudget) -> (Pages, [u8; 32]) {
    let mut pages = Pages::new(memory);
    let length = bytes.len() as u64;
    let root = write_value(&mut bytes, length, &mut pages, memory, &mut io(), || false).unwrap();
    (pages, root.digest().unwrap())
}

#[test]
fn tree_reads_enforce_length_utf8_output_and_cancellation_without_success_metadata() {
    let memory = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
    let mut bytes = Vec::new();
    write_ledger_value_v1(&MaterialLedgerId::Agent("甲".into()), &mut bytes).unwrap();
    let (pages, root) = store(&bytes, &memory);
    for mode in 0..3 {
        let mut budget = io();
        let mut output = Vec::new();
        let mut context = ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut budget,
        };
        let result = read_ledger_value_tree_v1(
            &root,
            &mut context,
            if mode == 0 { 2 } else { 3 },
            &mut output,
            || mode == 1,
        );
        if mode < 2 {
            assert!(result.is_err());
            assert!(output.is_empty());
        } else {
            assert_eq!(
                result.unwrap(),
                LedgerValueInfoV1 {
                    kind: LedgerValueKindV1::Agent,
                    id_bytes: 3
                }
            );
            assert_eq!(output, "甲".as_bytes());
        }
    }
    for payload in [&b"\xe7\x94"[..], &b"\xff"[..]] {
        let mut invalid = bytes[..LEDGER_VALUE_HEADER_BYTES].to_vec();
        invalid[LEDGER_VALUE_DOMAIN.len() + 1..]
            .copy_from_slice(&(payload.len() as u64).to_be_bytes());
        invalid.extend_from_slice(payload);
        let (pages, root) = store(&invalid, &memory);
        let mut budget = io();
        let mut context = ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut budget,
        };
        assert!(matches!(
            read_ledger_value_tree_v1(&root, &mut context, 3, &mut Vec::new(), || false),
            Err(ValueTreeError::Invalid("ledger UTF-8"))
        ));
    }
    struct Stop;
    impl Write for Stop {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("output stop"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut budget = io();
    let mut context = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    };
    assert!(matches!(
        read_ledger_value_tree_v1(&root, &mut context, 3, &mut Stop, || false),
        Err(ValueTreeError::Io(_))
    ));
}

#[test]
fn root_load_cancellation_precedes_an_unavailable_range_workspace() {
    let memory = MemoryBudget::new(1 << 20, 4096).unwrap();
    let mut bytes = Vec::new();
    write_ledger_value_v1(&MaterialLedgerId::World, &mut bytes).unwrap();
    let (pages, root) = store(&bytes, &memory);
    let cancelled = Cell::new(false);
    struct CancelRoot<'a> {
        pages: &'a Pages,
        cancelled: &'a Cell<bool>,
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
            self.cancelled.set(true);
            result
        }
    }
    let source = CancelRoot {
        pages: &pages,
        cancelled: &cancelled,
    };
    let _filler = memory
        .reserve(
            (1 << 20) - 4096 - memory.used() - 1024,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut budget = io();
    let mut output = Vec::new();
    let mut context = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    };
    assert!(matches!(
        read_ledger_value_tree_v1(&root, &mut context, 0, &mut output, || cancelled.get()),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(pages.reads.get(), 1);
    assert_eq!(budget.nodes_remaining, 999);
    assert!(output.is_empty());
}

#[test]
fn output_cancellation_and_exhausted_io_do_not_complete_a_value() {
    let memory = MemoryBudget::new(2 << 20, 4096).unwrap();
    let mut bytes = Vec::new();
    write_ledger_value_v1(&MaterialLedgerId::Site("site".into()), &mut bytes).unwrap();
    let (pages, root) = store(&bytes, &memory);
    let cancelled = Cell::new(false);
    struct CancelOutput<'a>(&'a Cell<bool>);
    impl Write for CancelOutput<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.set(true);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut budget = io();
    let mut context = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    };
    assert!(matches!(
        read_ledger_value_tree_v1(
            &root,
            &mut context,
            4,
            &mut CancelOutput(&cancelled),
            || cancelled.get()
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(budget.nodes_remaining < 1000);
    let mut budget = io();
    budget.write_bytes_remaining = 0;
    let mut output = Vec::new();
    let mut context = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    };
    assert!(matches!(
        read_ledger_value_tree_v1(&root, &mut context, 4, &mut output, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(output.is_empty());
}
