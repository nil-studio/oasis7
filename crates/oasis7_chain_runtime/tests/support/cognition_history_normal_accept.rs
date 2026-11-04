use super::signed::{core_budget, facts, put};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{self, test_fixtures as fixture},
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::RootSetV1,
    incremental_history::PreparedHistoryV1,
    incremental_material::{CoreRequest, verify_commit_core, verify_fact_originals},
};
use oasis7_proto::memory_budget::ReservedValue;
pub(super) struct NormalHistoryFrame<'a> {
    pub authority: &'a formal::VerifiedAuthorityV1,
    pub parent: &'a formal::VerifiedFormalCommitV1,
    pub after: &'a RootSetV1,
    pub history: &'a PreparedHistoryV1,
    pub input: [u8; 32],
    pub expected_facts: usize,
}
pub(super) fn accept_normal_history(
    frame: NormalHistoryFrame<'_>,
    memory: &MemoryBudget,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
) -> (
    formal::VerifiedFormalCommitV1,
    ReservedValue<AcceptedCognitionHistoryViewV1>,
    ReservedValue<oasis7_formal_protocol::incremental_material::VerifiedFactHistoryExtensionV1>,
) {
    let (accepted, selected, extension, ()) =
        accept_normal_history_with_candidate(frame, memory, store, objects, io, |_, _, _, _, _| ());
    (accepted, selected, extension)
}
pub(super) fn accept_normal_history_with_candidate<T>(
    frame: NormalHistoryFrame<'_>,
    memory: &MemoryBudget,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    before_accept: impl FnOnce(
        &formal::VerifiedCandidateContextV1,
        &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
        &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedFactOriginals>,
        &ReservedValue<CheckedCognitionHistoryViewV1>,
        &mut ObjectIo<'_>,
    ) -> T,
) -> (
    formal::VerifiedFormalCommitV1,
    ReservedValue<AcceptedCognitionHistoryViewV1>,
    ReservedValue<oasis7_formal_protocol::incremental_material::VerifiedFactHistoryExtensionV1>,
    T,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let history = frame.history;
    let input = frame.input;
    use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
    let after_catalog = history
        .body
        .catalog_before
        .extend(&history.body, &state, memory, 4 << 20)
        .unwrap();
    {
        let mut no_cancel = || false;
        let mut pages = store
            .buffered_page_writer(io.physical, io.cancelled, &mut no_cancel)
            .unwrap();
        for node in &after_catalog.new_nodes {
            let charge = memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
            pages
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
        }
        pages.finish().unwrap();
    }
    for bytes in [
        history.body.canonical_bytes().unwrap(),
        history.prefix.canonical_bytes().unwrap(),
        history.segment.canonical_bytes().unwrap(),
        history.facts.canonical_bytes(memory).unwrap().to_vec(),
        frame.after.canonical_bytes().unwrap(),
        history.transition_bytes.as_ref().unwrap().to_vec(),
        after_catalog.after.canonical_bytes().unwrap(),
    ] {
        put(objects, &bytes, io);
    }
    let mut frozen = fixture::frozen(frame.authority, frame.parent, None);
    frozen.reservation.body.schema_version = formal::incremental::IWS_FROZEN_SCHEMA;
    frozen.reservation.body.runtime_manifest_digest =
        frame.parent.record().runtime_manifest_digest.clone();
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
    let mut record = fixture::execution(frame.authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(frame.after.digest().unwrap());
    record.artifact_roots = frame.parent.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (
            ArtifactRole::IncrementalRootSet,
            frame.after.digest().unwrap(),
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
        frame.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let candidate = formal::verify_formal_candidate(
        &formal::SignedCanonicalCandidateV1 {
            record: proof.decision.record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        proof.decision.frozen.as_ref(),
        frame.authority,
        Some(frame.parent),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: frame.authority,
            parent: Some(frame.parent),
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects.original_reader(io),
        &state,
        memory,
        &mut core_budget(),
        || false,
    )
    .unwrap();
    let originals = verify_fact_originals(
        &core,
        &mut objects.original_reader(io),
        memory,
        &mut facts(),
        || false,
    )
    .unwrap();
    let selected = check_candidate_cognition_history_view_v1(
        &CognitionHistorySelectionRequestV1 {
            candidate: &candidate,
            core: &core,
            originals: &originals,
        },
        &mut objects.original_reader(io),
        memory,
        &mut facts(),
        || false,
    )
    .unwrap();
    let checked_extra = before_accept(&candidate, &core, &originals, &selected, io);
    let accepted =
        formal::verify_formal_qualified(&proof, frame.authority, Some(frame.parent), None).unwrap();
    let selected =
        promote_cognition_history_view_v1(&selected, &accepted, memory, || false).unwrap();
    assert_eq!(core.facts().facts.len(), frame.expected_facts);
    let extension = oasis7_formal_protocol::incremental_material::verify_fact_history_extension_v1(
        &oasis7_formal_protocol::incremental_material::FactHistoryExtensionRequestV1 {
            parent: frame.parent,
            head: &accepted,
            core: &core,
        },
        memory,
        || false,
    )
    .unwrap();
    (accepted, selected, extension, checked_extra)
}
