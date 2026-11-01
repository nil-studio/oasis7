use super::task_stop_record_control_v2::*;
use crate::formal_read_service::LocalStopPublication;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{task_stop_record_control_v2::*, task_stop_v2::*, *};
#[path = "task_cleanup_v2_formal_fixture.rs"]
mod actual_fixture;
fn stopped() -> (LocalStopPublication, StopRecordControlRequestV2) {
    let (world, close, owner) = actual_fixture::fixture();
    let (mut world, _, _, _) = crate::formal_read_fixtures::actual_world_initial_fixture_from_world(
        world,
        crate::formal_read_fixtures::h(50),
    );
    let p = close.request;
    let stop = sign_read_request(
        GAME_TASK_STOP_DOMAIN_V2,
        GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: "combined-stop".into(),
            custody_account_id: p.custody_account_id,
            termination_plan_digest: p.termination_plan_digest,
            initial_fence: p.permitted_branches.exact_initial_active,
            target_executor_public_key: p.signer_public_key,
            target_executor_generation: p.signer_generation,
            target_executor_nonce: p.signer_nonce,
            execute_until_ms: DecimalU64(1500),
            observation_grant: None,
        },
        &owner,
    )
    .unwrap();
    let mut original = SignedGameTaskStopV2 {
        request: stop.request,
        subject_public_key: stop.subject_public_key,
        signature_hex: stop.signature_hex,
    };
    original.request.observation_grant = Some(
        StopObservationGrantV2::known_control_for_exact_target(&original.request, 5000, 4500, 4000)
            .unwrap(),
    );
    let signed =
        sign_read_request(GAME_TASK_STOP_DOMAIN_V2, original.request.clone(), &owner).unwrap();
    original.signature_hex = signed.signature_hex;
    let context = actual_fixture::begin(&mut world, 1000);
    let record = world.apply_guarded_task_stop_v2(&original).unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    assert_eq!(
        record.after_control.task_status,
        GameTaskStatusV1::Suspended
    );
    assert_eq!(world.guarded_task_stop_v2(&original).unwrap(), Some(record));
    let (world, _, anchor, _) =
        crate::formal_read_fixtures::actual_world_initial_fixture_from_world(
            world,
            crate::formal_read_fixtures::h(50),
        );
    let publication = LocalStopPublication::from_publication(
        crate::formal_read_service::FormalReadPublication::new(anchor, world).unwrap(),
    )
    .unwrap();
    let q = StopRecordControlRequestV2 {
        api_revision: 2,
        original: TaskStopSubmitV2 {
            api_revision: 2,
            expected_world: publication.commit().world.clone(),
            original,
            observation_nonce: "11".repeat(32),
        },
        source: StopRecordSourceV2 {
            approved_input_digest: "22".repeat(32),
            session_id: "session".into(),
            invocation_id: "invocation".into(),
            source_operation_id: "source".into(),
            source_nonce: "33".repeat(32),
            world_attempt_id: "paid-world".into(),
        },
    };
    (publication, q)
}

#[test]
fn stop_record_control_v2_actual_qualified_snapshot_two_domains_and_delivery_expiry() {
    let (publication, q) = stopped();
    let raw = serde_json::to_vec(&q).unwrap();
    let before = publication.world().snapshot();
    let response = prepare(&raw, &publication, 2000)
        .unwrap()
        .deliver(&publication, &"44".repeat(32), 2001)
        .unwrap();
    let node = sign_read_request("key", (), &"44".repeat(32))
        .unwrap()
        .subject_public_key;
    verify_stop_record_control_v2(&q, &response, &node).unwrap();
    let StopRecordFactV2::Known(fact) = &response.fact.payload.fact else {
        panic!("actual record expected")
    };
    assert_eq!(fact.witnessed_in_commit, *publication.commit());
    assert!(matches!(
        response.current.payload.current,
        StopControlDeliveryV2::Exact(_)
    ));
    let late = prepare(&raw, &publication, 2000)
        .unwrap()
        .deliver(&publication, &"44".repeat(32), 4000)
        .unwrap();
    verify_stop_record_control_v2(&q, &late, &node).unwrap();
    assert!(matches!(
        late.current.payload.current,
        StopControlDeliveryV2::Drift
    ));
    assert_eq!(publication.world().snapshot(), before);
    let mut stripped = q.clone();
    stripped.original.original.request.observation_grant = None;
    assert!(prepare(&serde_json::to_vec(&stripped).unwrap(), &publication, 2000).is_err());
    let mut bad = response.clone();
    bad.fact.payload.observed_at_ms.0 += 1;
    assert!(verify_stop_record_control_v2(&q, &bad, &node).is_err());
    let mut swapped = response.clone();
    swapped.current.signature_hex = response.fact.signature_hex.clone();
    assert!(verify_stop_record_control_v2(&q, &swapped, &node).is_err());
}
