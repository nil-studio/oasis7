//! Bounded authenticated CAS used by the combined real-executor fixture.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::*, incremental_schedule::SchedulePageWriter, incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, io::Write, rc::Rc, sync::Arc};
struct State {
    memory: MemoryBudget,
    nodes: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    fault: Option<ValueTreeError>,
    _metadata: MemoryReservation,
}
#[derive(Clone)]
pub(super) struct Cas(Rc<RefCell<State>>);
impl Cas {
    pub fn new(memory: &MemoryBudget) -> Self {
        // Fixed key/Arc BTree nodes, including sparse occupancy and Arc owner.
        let per = 4
            * (std::mem::size_of::<[u8; 32]>()
                + std::mem::size_of::<Arc<ReservedValue<Vec<u8>>>>())
            + std::mem::size_of::<ReservedValue<Vec<u8>>>()
            + 64;
        let fee = memory
            .reserve(262144 * per + 8192, MemoryClass::Ordinary)
            .unwrap();
        Self(Rc::new(RefCell::new(State {
            memory: memory.clone(),
            nodes: BTreeMap::new(),
            fault: None,
            _metadata: fee,
        })))
    }
    pub fn error(&self) -> Option<ValueTreeError> {
        self.0.borrow_mut().fault.take()
    }
}
impl ValueNodeWriter for Cas {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let mut state = self.0.borrow_mut();
        if !bytes.uses_budget(&state.memory) || *blake3::hash(&bytes).as_bytes() != id {
            return Err(ValueTreeError::Invalid("combined fixture CAS hash/pool"));
        }
        if let Some(old) = state.nodes.get(&id) {
            if old.as_slice() != bytes.as_slice() {
                return Err(ValueTreeError::Invalid(
                    "combined fixture CAS immutable collision",
                ));
            }
            return Ok(());
        }
        if state.nodes.len() >= 262144 {
            return Err(ValueTreeError::ResourceLimited(
                "combined fixture CAS nodes",
            ));
        }
        state.nodes.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Cas {}
impl SchedulePageWriter for Cas {}
impl ValueNodeReader for Cas {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let state = self.0.borrow();
        if class != MemoryClass::Ordinary || !memory.same_pool(&state.memory) {
            return Err(ValueTreeError::Invalid(
                "combined fixture CAS read class/pool",
            ));
        }
        let value = state
            .nodes
            .get(id)
            .ok_or(ValueTreeError::Invalid("combined fixture missing CAS node"))?;
        if value.len() > max {
            return Err(ValueTreeError::ResourceLimited(
                "combined fixture CAS read bound",
            ));
        }
        Ok(Arc::clone(value))
    }
}
// A stable reference to the actual pool is required by Formal reader identity.
// Cas itself only owns immutable Rc state; the wrapper borrows the genuine pool.
pub(super) struct Reader<'a> {
    pub cas: Cas,
    pub memory: &'a MemoryBudget,
}
impl ValueNodeReader for Reader<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.cas.read_node(id, max, memory, class)
    }
}
impl MerkleNodeReader for Reader<'_> {
    type Node = Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id =
            blake3::Hash::from_hex(id).map_err(|_| "combined fixture state digest".to_owned())?;
        match self
            .cas
            .read_node(id.as_bytes(), max, self.memory, MemoryClass::Ordinary)
        {
            Ok(bytes) => Ok(Some(Node(bytes))),
            Err(error) => {
                self.cas.0.borrow_mut().fault = Some(error);
                Err("combined fixture typed state failure".to_owned())
            }
        }
    }
}
pub(super) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct ObjectState {
    memory: MemoryBudget,
    values: BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
    _metadata: MemoryReservation,
}
#[derive(Clone)]
pub(super) struct Objects(Rc<RefCell<ObjectState>>);
impl Objects {
    pub fn new(memory: &MemoryBudget) -> Self {
        let fee = memory.reserve(1024 * 512, MemoryClass::Ordinary).unwrap();
        Self(Rc::new(RefCell::new(ObjectState {
            memory: memory.clone(),
            values: BTreeMap::new(),
            _metadata: fee,
        })))
    }
    pub fn put(&self, bytes: &[u8]) -> [u8; 32] {
        assert!(bytes.len() <= 8 << 20);
        let id = *blake3::hash(bytes).as_bytes();
        let mut store = self.0.borrow_mut();
        if let Some(old) = store.values.get(&id) {
            assert_eq!(old.as_slice(), bytes);
            return id;
        }
        assert!(store.values.len() < 1024);
        let fee = store
            .memory
            .reserve(bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        store.values.insert(id, fee.retain(bytes.to_vec()));
        id
    }
    pub fn original(&self, id: &[u8; 32]) -> Vec<u8> {
        self.0.borrow().values.get(id).unwrap().to_vec()
    }
}
pub(super) struct Originals<'a> {
    pub objects: Objects,
    pub memory: &'a MemoryBudget,
}
impl OriginalObjectReader for Originals<'_> {
    type Error = &'static str;
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        let store = self.objects.0.borrow();
        let bytes = store.values.get(id).ok_or("missing combined original")?;
        if !memory.same_pool(self.memory)
            || bytes.len() > max
            || max > oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_RECORD_BYTES
            || !bytes.starts_with(b"oasis7.iws.")
        {
            return Err("combined bounded Formal header route");
        }
        let fee = memory
            .reserve(bytes.len() + 128, MemoryClass::Ordinary)
            .map_err(|_| "combined Formal header admission")?;
        Ok(Arc::new(fee.retain(bytes.to_vec())))
    }
}
impl OriginalObjectStreamReader for Originals<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<u64, Self::Error> {
        let store = self.objects.0.borrow();
        let bytes = store
            .values
            .get(id)
            .ok_or("missing combined stream original")?;
        if !memory.same_pool(self.memory) || bytes.len() as u64 > max {
            return Err("combined original stream bound/pool");
        }
        for chunk in bytes.chunks(64 << 10) {
            if cancelled() {
                return Err("combined original cancelled");
            }
            output
                .write_all(chunk)
                .map_err(|_| "combined original sink")?;
        }
        if cancelled() {
            return Err("combined original final cancelled");
        }
        Ok(bytes.len() as u64)
    }
}
