//! Real acceptance/cold Result routing on the candidate fixture's same bytes.
use super::*;
use oasis7_world_state::incremental_world_result_router_accepted::*;

pub(super) fn exercise(
    mode: u8,
    objects: &IncrementalObjectStore<'_>,
    store: &IncrementalPageStore,
    io: &mut ObjectIo<'_>,
    memory: &MemoryBudget,
    proof: &formal::FormalQualifiedCommitProofV1,
    authority: &formal::VerifiedAuthorityV1,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    facts_list: &[ExecutionFactRefV1],
    runtime_id: [u8; 32],
    wrong_head: &formal::VerifiedFormalCommitV1,
) {
    let accepted = formal::verify_formal_qualified(proof, authority, None, None).unwrap();
    io.class = MemoryClass::Recovery;
    let state = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let (ordinal, fact) = facts_list
        .iter()
        .enumerate()
        .find(|(_, f)| f.kind == ExecutionFactKindV1::Result && f.id == runtime_id)
        .or_else(|| {
            facts_list
                .iter()
                .enumerate()
                .find(|(_, f)| f.kind == ExecutionFactKindV1::Result)
        })
        .unwrap();
    let anchor = verify_fact_occurrence_v1(
        &FactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head: &accepted,
            roots,
            location: FactLocationV1 {
                height: roots.height,
                ordinal: ordinal as u64,
                original: fact.id,
            },
        },
        &mut FactReadContextV1 {
            originals: &mut objects.recovery_original_reader(io).unwrap(),
            catalog_pages: &state,
            memory,
            class: MemoryClass::Recovery,
            core_budget: &mut core_budget(),
            fact_budget: &mut facts(),
            cancelled: || false,
        },
    )
    .unwrap();
    let request = AcceptedWorldResultRouterRequestV1 {
        head: &accepted,
        roots,
        anchor: &anchor,
    };
    let routed = route_accepted_world_results_v1(
        &request,
        &mut objects.recovery_original_reader(io).unwrap(),
        &state,
        memory,
        MemoryClass::Recovery,
        &mut core_budget(),
        &mut router_work(),
        || false,
    );
    if mode >= 2 {
        assert!(
            routed.is_err(),
            "actual accepted FactSet mode={mode} must be rejected"
        );
        io.class = MemoryClass::Ordinary;
        return;
    }
    let routed = routed.unwrap();
    assert_eq!(routed.rows().len(), 3);
    assert_eq!(routed.record_id(), accepted.verified_record_id());
    assert_eq!(routed.body_id(), anchor.head_body_id());
    assert_eq!(routed.body().roots, roots.digest().unwrap());
    assert_eq!(
        routed.cognition().profile(),
        WorldResultProfileV1::CognitionHistoryV3
    );
    assert_eq!(
        routed.runtime_history().profile(),
        WorldResultProfileV1::RuntimeHistoryV1
    );
    for row in routed.rows() {
        assert_eq!(row.occurrence().allocation_class(), MemoryClass::Recovery);
        assert_eq!(
            row.occurrence().head_record_id(),
            accepted.verified_record_id()
        );
    }
    assert!(
        route_accepted_world_results_v1(
            &AcceptedWorldResultRouterRequestV1 {
                head: wrong_head,
                roots,
                anchor: &anchor
            },
            &mut objects.recovery_original_reader(io).unwrap(),
            &state,
            memory,
            MemoryClass::Recovery,
            &mut core_budget(),
            &mut router_work(),
            || false
        )
        .is_err()
    );
    assert!(
        route_accepted_world_results_v1(
            &request,
            &mut objects.recovery_original_reader(io).unwrap(),
            &state,
            memory,
            MemoryClass::Recovery,
            &mut core_budget(),
            &mut router_work(),
            || true
        )
        .is_err()
    );
    let mut no_headers = core_budget();
    no_headers.objects_remaining = 0;
    assert!(
        route_accepted_world_results_v1(
            &request,
            &mut objects.recovery_original_reader(io).unwrap(),
            &state,
            memory,
            MemoryClass::Recovery,
            &mut no_headers,
            &mut router_work(),
            || false
        )
        .is_err()
    );
    let mut no_facts = router_work();
    no_facts.facts.original_bytes_remaining = 0;
    assert!(
        route_accepted_world_results_v1(
            &request,
            &mut objects.recovery_original_reader(io).unwrap(),
            &state,
            memory,
            MemoryClass::Recovery,
            &mut core_budget(),
            &mut no_facts,
            || false
        )
        .is_err()
    );
    assert!(
        readmit_accepted_world_result_routing_ordinary_v1(
            &routed,
            wrong_head,
            roots,
            memory,
            || false
        )
        .is_err()
    );
    assert!(
        readmit_accepted_world_result_routing_ordinary_v1(
            &routed,
            &accepted,
            roots,
            memory,
            || true
        )
        .is_err()
    );
    let ordinary = readmit_accepted_world_result_routing_ordinary_v1(
        &routed,
        &accepted,
        roots,
        memory,
        || false,
    )
    .unwrap();
    drop(routed);
    drop(anchor);
    drop(state);
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    for row in ordinary.rows() {
        assert_eq!(row.occurrence().allocation_class(), MemoryClass::Ordinary);
    }
    assert_eq!(ordinary.body().roots, roots.digest().unwrap());
    io.class = MemoryClass::Ordinary;
}
