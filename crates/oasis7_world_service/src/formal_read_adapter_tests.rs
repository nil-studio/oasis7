use super::super::projection::WorldServiceProjection;
use super::super::{authority, formal_read_fixtures::*};
use super::*;
use crate::formal_read_response_fixture::response;
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::*;
#[test]
fn externally_pinned_actual_world_read_is_independent_of_service_assertion() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let checkpoint = anchor.checkpoint();
    let pin = checkpoint.digest().unwrap();
    let restored = VerifiedFormalReadAnchorV1::from_fixed_pin(&checkpoint, &pin, &trust).unwrap();
    let (request, response) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    assert!(service_assertion(response.clone(), &request).is_err());
    let mut verifier = FormalReadVerifier::new(&trust, restored).unwrap();
    let observed = verifier
        .verify_view(&response, &request, &public(7))
        .unwrap();
    let view = super::super::verified_view::VerifiedWorldView::from_verified_projection(
        response.clone(),
        &request,
        observed,
    )
    .unwrap();
    assert_eq!(view.formal_commit().unwrap().head(), anchor.head().head());
    let membership_request = ReadMembershipRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world: request.world.clone(),
        min_commit: Some(view.version().commit.clone()),
        fixed_commit: Some(view.version().commit.clone()),
    };
    let membership =
        super::super::membership::AuthenticatedMembershipObservation::read_verified_view(
            &view,
            &membership_request,
        )
        .unwrap();
    assert_eq!(membership.response().commit, view.version().commit);
    assert!(membership.response().applied_consensus_membership.is_none());
    assert!(!membership.response().supports_safe_runtime_update);
    let mut malicious = response;
    malicious.view.state.time += 1;
    malicious.logical_tick += 1;
    malicious.view.runtime_binding.as_mut().unwrap().base_tick = malicious.logical_tick;
    malicious.version.projection_revision =
        authority::request_digest("projection", &malicious.view).unwrap();
    let before = verifier.anchor_ref().unwrap();
    assert!(
        verifier
            .verify_view(&malicious, &request, &public(7))
            .is_err()
    );
    assert_eq!(verifier.anchor_ref().unwrap(), before);
    let mut wrong_checkpoint = checkpoint;
    wrong_checkpoint.active_authority.writer_public_key = public(99);
    assert!(VerifiedFormalReadAnchorV1::from_fixed_pin(&wrong_checkpoint, &pin, &trust).is_err());
    let mut wrong_trust = trust;
    wrong_trust.issuer_public_key = public(98);
    assert!(FormalReadVerifier::new(&wrong_trust, anchor).is_err());
}

#[test]
fn formal_projection_trait_preserves_world_tick_binding_and_scope_validation() {
    let (world, _, anchor, _) = actual_world_initial_fixture();
    let (request, response) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let validate = |reply: &ReadWorldViewResponse<
        super::super::projection::WorldServiceProjection,
    >| {
        <super::super::projection::WorldServiceProjection as FormalReadProjection>::validate_contents(reply, &request)
    };
    validate(&response).unwrap();
    let mut invalid_tick = response.clone();
    invalid_tick.view.state.time += 1;
    assert_eq!(
        validate(&invalid_tick).unwrap_err(),
        "projection tick differs from declared logical tick"
    );
    let mut invalid_binding = response.clone();
    invalid_binding
        .view
        .runtime_binding
        .as_mut()
        .unwrap()
        .base_tick += 1;
    assert_eq!(
        validate(&invalid_binding).unwrap_err(),
        "projection runtime binding differs from committed view"
    );
    let mut private_context = response;
    private_context.view.feedback_history =
        Some(super::super::projection::WorldServiceFeedbackHistory {
            agent_id: "private-agent".into(),
            records: vec![],
        });
    assert_eq!(
        validate(&private_context).unwrap_err(),
        "Agent observation differs from authorized visibility scope"
    );
}

// A raw formal response cannot obtain assurance through a service assertion,
// even when that assertion is correctly signed for this exact observation.
fn service_assertion(
    response: ReadWorldViewResponse<WorldServiceProjection>,
    request: &ReadWorldViewRequest,
) -> Result<crate::verified_view::VerifiedWorldView, String> {
    let signed_request = authority::sign_read_request(VIEW_PATH, request.clone(), &h(7))?;
    let digest = authority::request_digest(VIEW_PATH, &signed_request)?;
    let signed_response = authority::sign_service_response(VIEW_PATH, digest, response, &h(9))?;
    crate::verified_view::VerifiedWorldView::verify_service_assertion(
        signed_response,
        &signed_request,
        &public(9),
        crate::verified_view::ReadEvidencePolicy::Ordinary,
    )
}
