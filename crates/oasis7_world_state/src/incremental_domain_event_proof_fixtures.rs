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
        CoreBudget, CoreRequest, CurrentEventFactRequestV1, EventFactLocationV1,
        EventFactReadContextV1, EventFactRequestV1, FactBudget, OriginalObjectReader,
        OriginalObjectStreamReader, bind_checked_current_event_fact_v1, verify_commit_core,
        verify_event_fact_occurrence_v1, verify_fact_originals,
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
        let bytes = if id == &self.id {
            &self.original
        } else {
            self.headers.get(id).ok_or(io::ErrorKind::NotFound)?
        };
        if bytes.len() as u64 > max {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        for part in bytes.chunks(8192) {
            out.write_all(part)?;
        }
        Ok(bytes.len() as u64)
    }
}
struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Pages {
    memory: MemoryBudget,
    nodes: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
}
impl MerkleNodeReader for Pages {
    type Node = Page;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let node = self.nodes.get(id).ok_or("fixture catalog page missing")?;
        if node.len() > max {
            return Err("fixture catalog page allowance".into());
        }
        Ok(Some(Page(node.clone())))
    }
}

pub fn provenance(
    event: &WorldEvent,
    memory: &MemoryBudget,
) -> ReservedValue<VerifiedEventFactOccurrenceV1> {
    provenance_with_current(event, memory, false)
}
pub fn current_provenance(
    event: &WorldEvent,
    memory: &MemoryBudget,
) -> ReservedValue<VerifiedEventFactOccurrenceV1> {
    provenance_with_current(event, memory, true)
}
fn provenance_with_current(
    event: &WorldEvent,
    memory: &MemoryBudget,
    current: bool,
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
    let (objects, catalog) = incremental::core_objects(&s, &history);
    let metadata = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        memory: memory.clone(),
        nodes: BTreeMap::new(),
    };
    for node in &catalog.new_nodes {
        let charge = memory
            .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.nodes.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
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
    if current {
        let signed = formal::SignedCanonicalCandidateV1 {
            record: head.record().clone(),
            writer_signature_hex: fixture::sign(
                formal::canonical_candidate_signing_bytes(head.record()).unwrap(),
                4,
            ),
        };
        let candidate = formal::verify_formal_candidate(
            &signed,
            None,
            &s.authority,
            None,
            None,
            formal::FormalCompletionBranchV1::Normal,
        )
        .unwrap();
        let core = verify_commit_core(
            &CoreRequest {
                candidate: &candidate,
                original_authority: &s.authority,
                parent: None,
                max_pending_node_bytes: 4 << 20,
            },
            &mut source,
            &pages,
            memory,
            &mut CoreBudget {
                objects_remaining: 32,
                original_bytes_remaining: 1 << 20,
                new_pages_remaining: 4096,
                merkle_reads_remaining: 10000,
            },
            || false,
        )
        .unwrap();
        let observed = verify_fact_originals(
            &core,
            &mut source,
            memory,
            &mut FactBudget {
                facts_remaining: 2,
                original_bytes_remaining: 1 << 20,
                max_fact_bytes: 1 << 20,
            },
            || false,
        )
        .unwrap();
        return bind_checked_current_event_fact_v1(
            &CurrentEventFactRequestV1 {
                head: &head,
                core: &core,
                originals: &observed,
                ordinal: 1,
            },
            memory,
            || false,
        )
        .unwrap();
    }
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
            catalog_pages: &pages,
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
