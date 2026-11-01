//! Test-authority signed original-history fixture; never a production authority.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as fixture, incremental},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1},
    incremental_history::{CatalogRootV1, HeaderPreparation},
    incremental_material::{
        CoreBudget, EventFactLocationV1, EventFactReadContextV1, EventFactRequestV1, FactBudget,
        OriginalObjectReader, OriginalObjectStreamReader, verify_event_fact_occurrence_v1,
    },
    incremental_state::MerkleNodeReader,
};
use std::{collections::BTreeMap, sync::Arc};

struct Source {
    memory: MemoryBudget,
    headers: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    original: Arc<ReservedValue<Vec<u8>>>,
    id: [u8; 32],
    _metadata: MemoryReservation,
}
impl OriginalObjectReader for Source {
    type Error = io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> io::Result<Arc<ReservedValue<Vec<u8>>>> {
        if !memory.same_pool(&self.memory) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let value = self.headers.get(id).ok_or(io::ErrorKind::NotFound)?;
        if value.len() > max {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(value.clone())
    }
}
impl OriginalObjectStreamReader for Source {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        _: &MemoryBudget,
        out: &mut dyn Write,
    ) -> io::Result<u64> {
        if id != &self.id || self.original.len() as u64 > max {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for part in self.original.chunks(8192) {
            out.write_all(part)?;
        }
        Ok(self.original.len() as u64)
    }
}
struct NoPages(MemoryBudget);
impl MerkleNodeReader for NoPages {
    type Node = Vec<u8>;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.0)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Err("genesis occurrence must not read ancestors".into())
    }
}

pub fn provenance(
    event: &WorldEvent,
    memory: &MemoryBudget,
) -> ReservedValue<VerifiedEventFactOccurrenceV1> {
    let s = incremental::setup_with_memory(memory.clone());
    // The original, unchanged encoder is the independent byte oracle.
    let mut oracle = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
    let id = *blake3::hash(&bytes).as_bytes();
    let original = Arc::new(oracle.split(bytes.capacity()).unwrap().retain(bytes));
    let facts = [
        s.history.facts.facts[0],
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id,
        },
    ];
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &CatalogRootV1::empty(s.config.scope().unwrap()).unwrap(),
        roots: &s.roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &facts,
        operation: s.history.body.operation,
        authority: s.history.body.authority,
        execution_environment: s.config.execution_environment,
        checkpoint: true,
    }
    .prepare(memory)
    .unwrap();
    let mut record = s.commit.record().clone();
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let proof = fixture::proof(
        record,
        &s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let head = formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
    let (objects, _) = incremental::core_objects(&s, &history);
    let metadata = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut source = Source {
        memory: memory.clone(),
        headers: objects
            .iter()
            .map(|v| (*blake3::hash(v).as_bytes(), v.clone()))
            .collect(),
        original,
        id,
        _metadata: metadata,
    };
    verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head: &head,
            roots: &s.roots,
            location: EventFactLocationV1 {
                height: 0,
                ordinal: 1,
                original: id,
            },
        },
        &mut EventFactReadContextV1 {
            originals: &mut source,
            catalog_pages: &NoPages(memory.clone()),
            memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut CoreBudget {
                objects_remaining: 32,
                original_bytes_remaining: 1 << 20,
                new_pages_remaining: 4096,
                merkle_reads_remaining: 10000,
            },
            fact_budget: &mut FactBudget {
                facts_remaining: 1,
                original_bytes_remaining: 1 << 20,
                max_fact_bytes: 1 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap()
}
