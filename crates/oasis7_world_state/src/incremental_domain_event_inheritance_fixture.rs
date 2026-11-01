//! Actual signed Normal successor, not a caller-constructed provenance token.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, ExecutionTransaction, TransactionLimits},
    incremental_history::PreparedHistoryV1,
    incremental_material::{
        EventHistoryExtensionRequestV1, inherit_event_fact_occurrence_v1,
        verify_event_history_extension_v1,
    },
};

pub(super) fn extend(
    s: &incremental::Setup,
    old: &PreparedHistoryV1,
    parent: &formal::VerifiedFormalCommitV1,
    source: &mut Source,
    pages: &mut Pages,
    raw: &ReservedValue<VerifiedEventFactOccurrenceV1>,
    memory: &MemoryBudget,
) -> ReservedValue<VerifiedEventFactOccurrenceV1> {
    let catalog = old
        .body
        .catalog_before
        .extend(&old.body, pages, memory, 4 << 20)
        .unwrap();
    let before = ExecutionParent {
        roots: s.roots.clone(),
        commit: old.body.digest().unwrap(),
        authority: old.body.authority,
    };
    let fence = before.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        before,
        [60; 32],
        pages,
        memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 4,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let input_bytes = b"next original input".to_vec();
    let input = *blake3::hash(&input_bytes).as_bytes();
    txn.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    let guard = txn.guard();
    let sealed = txn.seal(&guard, &fence, 1).unwrap();
    let next = HeaderPreparation {
        config: &s.config,
        catalog: &catalog.after,
        roots: &sealed.record.after,
        parent: Some(&old.body),
        parent_history: Some(&old.prefix),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority: old.body.authority,
        execution_environment: s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(memory)
    .unwrap();
    let mut frozen = fixture::frozen(&s.authority, parent, None);
    frozen.reservation.body.schema_version = formal::incremental::IWS_FROZEN_SCHEMA;
    frozen.reservation.body.runtime_manifest_digest =
        parent.record().runtime_manifest_digest.clone();
    frozen.reservation.body.payload_digest = hex::encode(input);
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
    let mut record = fixture::execution(&s.authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(sealed.record.after.digest().unwrap());
    record.artifact_roots = parent.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (
            ArtifactRole::IncrementalRootSet,
            sealed.record.after.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            next.body.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            next.prefix.digest().unwrap(),
        ),
    ] {
        record.artifact_roots.insert(role, hex::encode(id));
    }
    let proof = fixture::proof(
        record,
        &s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let head = formal::verify_formal_qualified(&proof, &s.authority, Some(parent), None).unwrap();
    let signed = formal::SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    let candidate = formal::verify_formal_candidate(
        &signed,
        proof.decision.frozen.as_ref(),
        &s.authority,
        Some(parent),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let after_catalog = next
        .body
        .catalog_before
        .extend(&next.body, pages, memory, 4 << 20)
        .unwrap();
    let _metadata = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    for node in catalog.new_nodes.iter().chain(&after_catalog.new_nodes) {
        let charge = memory
            .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.nodes.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    for bytes in [
        next.body.canonical_bytes().unwrap(),
        next.prefix.canonical_bytes().unwrap(),
        next.segment.canonical_bytes().unwrap(),
        next.facts.canonical_bytes(memory).unwrap().to_vec(),
        sealed.record.after.canonical_bytes().unwrap(),
        next.transition_bytes.as_ref().unwrap().to_vec(),
        after_catalog.after.canonical_bytes().unwrap(),
        input_bytes,
    ] {
        let id = *blake3::hash(&bytes).as_bytes();
        let charge = memory
            .reserve(bytes.capacity(), MemoryClass::Ordinary)
            .unwrap();
        source.headers.insert(id, Arc::new(charge.retain(bytes)));
    }
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: Some(parent),
            max_pending_node_bytes: 4 << 20,
        },
        source,
        pages,
        memory,
        &mut CoreBudget {
            objects_remaining: 64,
            original_bytes_remaining: 1 << 20,
            new_pages_remaining: 4096,
            merkle_reads_remaining: 10000,
        },
        || false,
    )
    .unwrap();
    let extension = verify_event_history_extension_v1(
        &EventHistoryExtensionRequestV1 {
            parent,
            head: &head,
            core: &core,
        },
        memory,
        || false,
    )
    .unwrap();
    inherit_event_fact_occurrence_v1(raw, &extension, memory, || false).unwrap()
}
