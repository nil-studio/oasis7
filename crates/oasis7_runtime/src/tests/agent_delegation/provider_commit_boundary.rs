use super::*;
use crate::tests::agent_cognition_runtime_hardening::{
    continuous_commit_request, continuous_response_artifact,
};
#[test]
fn combined_provider_commit_rejects_wrong_artifact_identity_without_executing_action() {
    let mut world = fixture();
    world.install_agent_delegation_grant(grant()).unwrap();
    accepted(&mut world);
    world
        .set_agent_resource_balance(
            AGENT_ID,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            100,
        )
        .unwrap();
    let request = continuous_commit_request(&world);
    let mut artifact = continuous_response_artifact(&request);
    artifact.request_digest =
        "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into();
    artifact.refresh_artifact_digest();
    let before = world.state().agents[AGENT_ID].state.pos;
    let result = world.commit_provider_cognition_action(
        request,
        movement(1),
        artifact,
        ProviderDecisionEvidenceV1::default(),
    );
    assert!(
        matches!(result, Err(ProviderCognitionCommitError::Commit(_))),
        "{result:?}"
    );
    assert_eq!(world.state().agents[AGENT_ID].state.pos, before);
    assert!(
        world.cognition()["commit_records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn provider_evidence_cannot_turn_revoked_delegation_into_an_owner_override() {
    let mut world = fixture();
    let mut g = grant();
    g.revoked = true;
    world.install_agent_delegation_grant(g).unwrap();
    accepted(&mut world);
    let request = continuous_commit_request(&world);
    let artifact = continuous_response_artifact(&request);
    let before = world.state().agents[AGENT_ID].state.pos;
    let evidence = ProviderDecisionEvidenceV1 {
        expected_consequence: json!({"override_actor":"player","owner_control_refs":["forged-control"],"grant_revision":1}),
        stakes: json!({"authority":"owner"}),
        ..Default::default()
    };
    let result = world.commit_provider_cognition_action(request, movement(1), artifact, evidence);
    assert!(
        matches!(result, Err(ProviderCognitionCommitError::AgencyBinding(_))),
        "{result:?}"
    );
    assert_eq!(world.state().agents[AGENT_ID].state.pos, before);
    assert!(world.agent_causal_receipts(AGENT_ID).unwrap().is_empty());
}
