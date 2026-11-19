//! Keep the fixture's real, already paid GEN deltas as immutable page sources.
//! The finite write store retains its original 16,384-page limit. This avoids
//! copying every GEN page into it; it does not reduce the original delta fees.
use crate::incremental_typed_record_test_support::{Pages as WrittenPages, Writer as WriteHandle};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{mem, sync::Arc};

struct Genesis {
    delta: ReservedValue<StateDeltaV1>,
    _allocation: MemoryReservation,
}
const GENESIS_ALLOCATION: usize =
    mem::size_of::<Genesis>() + 2 * mem::size_of::<usize>() + mem::align_of::<Genesis>();

enum Bytes {
    Written(<WrittenPages as MerkleNodeReader>::Node),
    Genesis(Arc<Genesis>, usize),
}
pub(super) struct Node {
    bytes: Bytes,
    _metadata: MemoryReservation,
}
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        match &self.bytes {
            Bytes::Written(node) => node.as_ref(),
            Bytes::Genesis(original, index) => &original.delta.new_nodes[*index].bytes,
        }
    }
}

pub(super) struct Pages {
    written: WrittenPages,
    genesis: [Option<Arc<Genesis>>; 3],
    memory: MemoryBudget,
    _metadata: MemoryReservation,
}
impl Pages {
    pub(super) fn new(memory: &MemoryBudget) -> Self {
        let metadata = memory
            .reserve(mem::size_of::<Self>(), MemoryClass::Ordinary)
            .expect("agent GEN source metadata");
        Self {
            written: WrittenPages::new(memory),
            genesis: std::array::from_fn(|_| None),
            memory: memory.clone(),
            _metadata: metadata,
        }
    }

    pub(super) fn retain_genesis(
        &mut self,
        delta: ReservedValue<StateDeltaV1>,
    ) -> Result<(), ValueTreeError> {
        if !delta.uses_budget(&self.memory) {
            return Err(ValueTreeError::Invalid("agent GEN source pool"));
        }
        let slot = match delta.after.namespace {
            StateNamespaceV1::Business => 0,
            StateNamespaceV1::Scheduler => 1,
            StateNamespaceV1::Authorization => 2,
            _ => return Err(ValueTreeError::Invalid("agent GEN namespace")),
        };
        if self.genesis[slot].is_some()
            || delta.before.namespace != delta.after.namespace
            || delta.before.leaf_count != 0
            || delta
                .new_nodes
                .windows(2)
                .any(|pair| pair[0].digest >= pair[1].digest)
        {
            return Err(ValueTreeError::Invalid("agent GEN source order"));
        }
        let allocation = self
            .memory
            .reserve(GENESIS_ALLOCATION, MemoryClass::Ordinary)?;
        // Preserve publication precedence: a later actual GEN page replaces
        // an earlier written page with that ID, as the original put loop did.
        for page in &delta.new_nodes {
            let id = blake3::Hash::from_hex(&page.digest)
                .map_err(|_| ValueTreeError::Invalid("agent GEN page digest"))?;
            self.written.remove(id.as_bytes());
        }
        self.genesis[slot] = Some(Arc::new(Genesis {
            delta,
            _allocation: allocation,
        }));
        Ok(())
    }

    fn original(&self, digest: &str) -> Option<(&Arc<Genesis>, usize)> {
        // final_writes already supplies a sorted, unique page vector. Keep
        // that original index; there is no growing lookup map or page cache.
        self.genesis.iter().rev().flatten().find_map(|original| {
            original
                .delta
                .new_nodes
                .binary_search_by(|page| page.digest.as_str().cmp(digest))
                .ok()
                .map(|index| (original, index))
        })
    }
}

impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let metadata = self
            .memory
            .reserve(mem::size_of::<Node>(), MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let bytes = if let Some(node) = MerkleNodeReader::read_node(&self.written, id, max)? {
            Bytes::Written(node)
        } else {
            let digest = blake3::Hash::from_hex(id)
                .map_err(|error| error.to_string())?
                .to_hex();
            let Some((original, index)) = self.original(digest.as_str()) else {
                return Ok(None);
            };
            if original.delta.new_nodes[index].bytes.len() > max {
                return Err("typed fixture state page bound".into());
            }
            Bytes::Genesis(Arc::clone(original), index)
        };
        Ok(Some(Node {
            bytes,
            _metadata: metadata,
        }))
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
        match ValueNodeReader::read_node(&self.written, id, max, memory, class) {
            Err(ValueTreeError::Invalid("typed fixture missing page")) => (),
            result => return result,
        }
        let digest = blake3::Hash::from(*id).to_hex();
        let (original, index) = self
            .original(digest.as_str())
            .ok_or(ValueTreeError::Invalid("typed fixture missing page"))?;
        let bytes = &original.delta.new_nodes[index].bytes;
        if bytes.len() > max {
            return Err(ValueTreeError::Invalid("typed fixture page bound"));
        }
        // Value readers require an owned Vec carrier. Only a requested GEN
        // page is copied here, under a fresh same-pool charge before allocation.
        let allocation = memory.reserve(
            bytes.len()
                + mem::size_of::<ReservedValue<Vec<u8>>>()
                + 2 * mem::size_of::<usize>()
                + mem::align_of::<ReservedValue<Vec<u8>>>(),
            class,
        )?;
        Ok(Arc::new(allocation.retain(bytes.clone())))
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        ValueNodeWriter::put_node(&mut self.written, id, bytes)
    }
}
impl StatePageWriter for Pages {}

pub(super) struct Writer<'a>(pub(super) &'a Pages);
impl ValueNodeWriter for Writer<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        WriteHandle(&self.0.written).put_node(id, bytes)
    }
}
impl ValueNodeReader for Writer<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(self.0, id, max, memory, class)
    }
}
impl StatePageWriter for Writer<'_> {}

#[test]
fn last_genesis_page_reader_keeps_original_delta_and_carrier_paid() {
    use oasis7_formal_protocol::{
        incremental_state::{STATE_MAX_PAGE_BYTES, StateRootV1, StateWriteV1},
        incremental_value::write_value,
    };
    let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let input = b"actual GEN value";
    let mut source = &input[..];
    let value = write_value(
        &mut source,
        input.len() as u64,
        &mut Writer(&pages),
        &memory,
        &mut crate::incremental_typed_record_test_support::io(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap();
    let charge = memory
        .reserve((512 << 10) + (128 << 10), MemoryClass::Ordinary)
        .unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            vec![StateWriteV1 {
                key: b"actual GEN key".to_vec(),
                before: None,
                after: Some(value),
            }],
            &pages,
            16 << 20,
        )
        .unwrap(),
    );
    let id = blake3::Hash::from_hex(&delta.after.node_root).unwrap();
    let retained = delta.reserved_bytes() + GENESIS_ALLOCATION;
    pages.retain_genesis(delta).unwrap();
    let first = MerkleNodeReader::read_node(&pages, id.to_hex().as_str(), STATE_MAX_PAGE_BYTES)
        .unwrap()
        .unwrap();
    let last = MerkleNodeReader::read_node(&pages, id.to_hex().as_str(), STATE_MAX_PAGE_BYTES)
        .unwrap()
        .unwrap();
    drop(pages);
    assert_eq!(memory.used(), retained + 2 * mem::size_of::<Node>());
    assert_eq!(blake3::hash(first.as_ref()), id);
    drop(first);
    assert_eq!(memory.used(), retained + mem::size_of::<Node>());
    assert_eq!(blake3::hash(last.as_ref()), id);
    drop(last);
    assert_eq!(memory.used(), 0);
}
