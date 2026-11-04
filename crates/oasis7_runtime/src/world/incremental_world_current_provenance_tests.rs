//! Genuine signed GEN and ordinary Core scopes for current-owner tests.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as fixture, incremental},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_history::*,
    incremental_material::*,
};
pub(crate) struct Frame {
    _catalog_charge: oasis7_proto::memory_budget::MemoryReservation,
    pub config: GenesisConfigV1,
    pub roots: RootSetV1,
    pub history: ReservedValue<PreparedHistoryV1>,
    pub catalog: CatalogRootV1,
    pub authority: formal::VerifiedAuthorityV1,
    pub accepted: formal::VerifiedFormalCommitV1,
    pub candidate: formal::VerifiedCandidateContextV1,
    pub core: ReservedValue<CheckedCommitCore>,
    pub originals: ReservedValue<CheckedFactOriginals>,
}
impl Frame {
    pub(crate) fn candidate_scope(&self) -> MailboxProvenanceScopeV1<'_> {
        MailboxProvenanceScopeV1::Candidate {
            candidate: &self.candidate,
            core: &self.core,
            originals: &self.originals,
        }
    }
    pub(crate) fn accepted_scope(&self) -> MailboxProvenanceScopeV1<'_> {
        MailboxProvenanceScopeV1::Accepted {
            head: &self.accepted,
            roots: &self.roots,
        }
    }
    pub(crate) fn parent(&self) -> ExecutionParent {
        ExecutionParent {
            roots: self.roots.clone(),
            commit: self.history.body.digest().unwrap(),
            authority: self.history.body.authority,
        }
    }
}
fn put(
    memory: &MemoryBudget,
    objects: &mut BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
    bytes: &[u8],
) {
    assert!(objects.len() < 1024);
    objects.insert(
        *blake3::hash(bytes).as_bytes(),
        memory
            .reserve(bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(bytes.to_vec()),
    );
}
fn nodes(memory: &MemoryBudget, pages: &mut Pages, delta: &CatalogDeltaV1) {
    for node in &delta.new_nodes {
        pages
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn core_work() -> CoreBudget {
    CoreBudget {
        objects_remaining: 256,
        original_bytes_remaining: 32 << 20,
        new_pages_remaining: 8192,
        merkle_reads_remaining: 1_000_000,
    }
}
pub(crate) fn fact_work() -> FactBudget {
    FactBudget {
        facts_remaining: 256,
        original_bytes_remaining: 32 << 20,
        max_fact_bytes: 4 << 20,
    }
}
fn checked(
    memory: &MemoryBudget,
    pages: &Pages,
    objects: &BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
    proof: &formal::FormalQualifiedCommitProofV1,
    authority: &formal::VerifiedAuthorityV1,
    parent: Option<&formal::VerifiedFormalCommitV1>,
) -> (
    formal::VerifiedCandidateContextV1,
    ReservedValue<CheckedCommitCore>,
    ReservedValue<CheckedFactOriginals>,
) {
    let candidate = formal::verify_formal_candidate(
        &formal::SignedCanonicalCandidateV1 {
            record: proof.decision.record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        proof.decision.frozen.as_ref(),
        authority,
        parent,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let reader = Reader {
        pages,
        memory,
        class: MemoryClass::Ordinary,
    };
    let mut objects = Originals {
        values: objects,
        memory,
        class: MemoryClass::Ordinary,
        calls: 0,
    };
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: authority,
            parent,
            max_pending_node_bytes: 16 << 20,
        },
        &mut objects,
        &reader,
        memory,
        &mut core_work(),
        || false,
    )
    .unwrap();
    let originals =
        verify_fact_originals(&core, &mut objects, memory, &mut fact_work(), || false).unwrap();
    (candidate, core, originals)
}
pub(crate) fn genesis(
    memory: &MemoryBudget,
    pages: &mut Pages,
    state: [StateRootV1; 6],
    events: impl IntoIterator<Item = [u8; 32]>,
    objects: &mut BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
) -> Frame {
    let extras: Vec<_> = events
        .into_iter()
        .map(|id| ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id,
        })
        .collect();
    let s = incremental::setup_with_config_state_and_facts(
        memory.clone(),
        incremental::genesis_config([4; 32]),
        state,
        incremental::FixtureExecutionArtifactRefsV1 {
            manifest: [5; 32],
            wasm: [6; 32],
            rules: [7; 32],
            policy: [8; 32],
        },
        &extras,
    );
    let (blobs, catalog) = incremental::core_objects(&s, &s.history);
    for bytes in blobs.iter() {
        put(memory, objects, bytes.as_slice());
    }
    nodes(memory, pages, &catalog);
    let (candidate, core, originals) =
        checked(memory, pages, objects, &s.proof, &s.authority, None);
    let catalog_charge = memory
        .reserve(
            std::mem::size_of::<CatalogRootV1>()
                .checked_add(catalog.after.scope.world_id.len())
                .unwrap()
                .checked_add(catalog.after.node_root.len())
                .unwrap()
                .checked_add(128)
                .unwrap(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let catalog_after = catalog.after.clone();
    Frame {
        _catalog_charge: catalog_charge,
        config: s.config,
        roots: s.roots,
        history: s.history,
        catalog: catalog_after,
        authority: s.authority,
        accepted: s.commit,
        candidate,
        core,
        originals,
    }
}
pub(crate) fn normal(
    memory: &MemoryBudget,
    pages: &mut Pages,
    parent: &Frame,
    transition: &TransitionRecordV1,
    input: [u8; 32],
    objects: &mut BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
) -> Frame {
    let history = HeaderPreparation {
        config: &parent.config,
        catalog: &parent.catalog,
        roots: &transition.after,
        parent: Some(&parent.history.body),
        parent_history: Some(&parent.history.prefix),
        transition: Some(transition),
        genesis_facts: &[],
        operation: transition.operation,
        authority: parent.history.body.authority,
        execution_environment: parent.config.execution_environment,
        checkpoint: false,
    }
    .prepare(memory)
    .unwrap();
    let catalog = {
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        parent
            .catalog
            .extend(&history.body, &reader, memory, 16 << 20)
            .unwrap()
    };
    nodes(memory, pages, &catalog);
    for bytes in [
        history.body.canonical_bytes().unwrap(),
        history.prefix.canonical_bytes().unwrap(),
        history.segment.canonical_bytes().unwrap(),
        history.facts.canonical_bytes(memory).unwrap().to_vec(),
        transition.after.canonical_bytes().unwrap(),
        history.transition_bytes.as_ref().unwrap().to_vec(),
        catalog.after.canonical_bytes().unwrap(),
    ] {
        put(memory, objects, &bytes);
    }
    let mut frozen = fixture::frozen(&parent.authority, &parent.accepted, None);
    frozen.reservation.body.schema_version = formal::incremental::IWS_FROZEN_SCHEMA;
    frozen.reservation.body.runtime_manifest_digest =
        parent.accepted.record().runtime_manifest_digest.clone();
    frozen.reservation.body.payload_digest = hex::encode(input);
    frozen.reservation.body.request_digest = hex::encode(history.body.operation);
    frozen.reservation.signature_hex = fixture::sign(
        formal::frozen_signing_bytes(&frozen.reservation.body).unwrap(),
        4,
    );
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation.clone();
        receipt.signature_hex = fixture::sign(
            formal::frozen_receipt_signing_bytes(&receipt.body).unwrap(),
            key,
        );
    }
    let mut record = fixture::execution(&parent.authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(transition.after.digest().unwrap());
    record.artifact_roots = parent.accepted.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (
            ArtifactRole::IncrementalRootSet,
            transition.after.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            history.body.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            history.prefix.digest().unwrap(),
        ),
    ] {
        record.artifact_roots.insert(role, hex::encode(id));
    }
    let proof = fixture::proof(
        record,
        &parent.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let (candidate, core, originals) = checked(
        memory,
        pages,
        objects,
        &proof,
        &parent.authority,
        Some(&parent.accepted),
    );
    let accepted =
        formal::verify_formal_qualified(&proof, &parent.authority, Some(&parent.accepted), None)
            .unwrap();
    let catalog_charge = memory
        .reserve(
            std::mem::size_of::<CatalogRootV1>()
                .checked_add(catalog.after.scope.world_id.len())
                .unwrap()
                .checked_add(catalog.after.node_root.len())
                .unwrap()
                .checked_add(128)
                .unwrap(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let catalog_after = catalog.after.clone();
    Frame {
        _catalog_charge: catalog_charge,
        config: parent.config.clone(),
        roots: transition.after.clone(),
        history,
        catalog: catalog_after,
        authority: parent.authority.clone(),
        accepted,
        candidate,
        core,
        originals,
    }
}
