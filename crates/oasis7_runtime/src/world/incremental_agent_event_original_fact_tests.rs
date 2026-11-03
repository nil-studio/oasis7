//! A consumed, self-reported sink is not a durable-original/acceptance proof.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as signing, incremental as fixture},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_history::{CatalogRootV1, HeaderPreparation},
    incremental_material::{
        CoreBudget, CoreError, CoreRequest, FactBudget, OriginalObjectReader,
        OriginalObjectStreamReader, verify_commit_core, verify_fact_originals,
    },
    incremental_state::MerkleNodeReader,
};
use std::io::{self, Write};
struct HeaderSource<'a> {
    memory: &'a MemoryBudget,
    originals: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
}
impl OriginalObjectReader for HeaderSource<'_> {
    type Error = io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
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
        if !memory.same_pool(self.memory) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let bytes = self.originals.get(id).ok_or(io::ErrorKind::NotFound)?;
        if bytes.len() > max {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(bytes.clone())
    }
}
impl OriginalObjectStreamReader for HeaderSource<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        out: &mut dyn Write,
    ) -> io::Result<u64> {
        let max = usize::try_from(max).map_err(|_| io::ErrorKind::InvalidInput)?;
        let bytes = self.read_original(id, max, memory)?;
        for chunk in bytes.chunks(8192) {
            out.write_all(chunk)?;
        }
        Ok(bytes.len() as u64)
    }
}
struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
struct CatalogPages<'a> {
    memory: &'a MemoryBudget,
    pages: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
}
impl MerkleNodeReader for CatalogPages<'_> {
    type Node = Page;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Page>, String> {
        let Some(page) = self.pages.get(id) else {
            return Ok(None);
        };
        if page.len() > max {
            return Err("catalog fixture bound".into());
        }
        Ok(Some(Page(page.clone())))
    }
}
#[test]
fn fake_sink_consumption_does_not_pass_real_fact_original_verification() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let _metadata = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let (world, mut parent, mut pages) = super::fixture(&memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut pages,
        &mut values,
        &memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    world
        .contribute_iws_agent_event(
            &event(&world, 1),
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                persist_original: |producer: &mut dyn AgentEventOriginalV1| {
                    // Deliberately violate persistence: consume bytes but create no
                    // original source entry. Private producer completion is provisional.
                    producer.persist_to(
                        &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              _: &mut dyn FnMut() -> bool| {
                            stream.write_to(&mut io::sink())
                        },
                    )
                },
                max_event_bytes: 4096,
                original_work: original_work(),
            },
        )
        .unwrap();
    let guard = txn.guard();
    let provisional = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(provisional.record.facts.len(), 1);
    let missing = provisional.record.facts[0];

    // Use real signed test-authority header/candidate and real material/fact
    // verifiers. These structural fixtures are not a World acceptance receipt.
    let s = fixture::setup_with_memory(memory.clone());
    let facts = [s.history.facts.facts[0], missing];
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
    .prepare(&memory)
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
    let signed = formal::SignedCanonicalCandidateV1 {
        writer_signature_hex: signing::sign(
            formal::canonical_candidate_signing_bytes(&record).unwrap(),
            4,
        ),
        record,
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
    let (blobs, catalog) = fixture::core_objects(&s, &history);
    let mut originals = HeaderSource {
        memory: &memory,
        originals: blobs
            .iter()
            .map(|b| (*blake3::hash(b).as_bytes(), b.clone()))
            .collect(),
    };
    assert!(!originals.originals.contains_key(&missing.id));
    let mut pages = CatalogPages {
        memory: &memory,
        pages: BTreeMap::new(),
    };
    for node in &catalog.new_nodes {
        let charge = memory
            .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.pages.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 4 << 20,
        },
        &mut originals,
        &pages,
        &memory,
        &mut CoreBudget {
            objects_remaining: 32,
            original_bytes_remaining: 1 << 20,
            new_pages_remaining: 4096,
            merkle_reads_remaining: 10_000,
        },
        || false,
    )
    .unwrap();
    let checked = verify_fact_originals(
        &core,
        &mut originals,
        &memory,
        &mut FactBudget {
            facts_remaining: 2,
            original_bytes_remaining: 1 << 20,
            max_fact_bytes: 1 << 20,
        },
        || false,
    );
    assert!(matches!(checked,Err(CoreError::Object(ref e)) if e.kind()==io::ErrorKind::NotFound));
}
