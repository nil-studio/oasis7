use super::*;
use crate::verified_view::ReadEvidencePolicy;
const MEMBERSHIP_PATH: &str = "/v1/world/governance-membership";
use oasis7_runtime::{GovernanceValidatorAdmissionRecord, WorldState};

fn fixture() -> (ReadMembershipRequest, ReadMembershipResponse) {
    let mut state = WorldState {
        governance_finality_signer_registry: Some(
            oasis7_runtime::GovernanceFinalitySignerRegistry {
                slot_id: "finality".into(),
                threshold: 1,
                threshold_bps: 0,
                signer_bindings: std::collections::BTreeMap::from([(
                    "seed".into(),
                    "22".repeat(32),
                )]),
                validator_stakes: std::collections::BTreeMap::from([("seed".into(), 100)]),
            },
        ),
        ..Default::default()
    };
    state.governance_validator_admissions.insert(
        "candidate".into(),
        GovernanceValidatorAdmissionRecord {
            candidate_id: "candidate".into(),
            node_id: "node".into(),
            stake: 25,
            finality_signer_public_key: "11".repeat(32),
            public_manifest_hash: "manifest".into(),
            operator_owner: "PRIVATE-OWNER".into(),
            revocation_reason: Some("PRIVATE-REASON".into()),
            ..Default::default()
        },
    );
    let mut world = oasis7_runtime::World::new_with_state(state);
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    let binding = world.current_cognition_runtime_binding().unwrap();
    let commit = CommitRef {
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        binding: ExecutionBinding {
            provider_world_id: "world".into(),
            branch_id: "main".into(),
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    &binding.finality_status,
                    &binding.finality_block_hash,
                ),
            )
            .unwrap(),
            reorg_generation: 0,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: 0,
            permission_generation: world.capability_revocation_state().epoch,
        },
        position: 1,
        execution_block_hash: "block".into(),
        state_root_ref: "cas-root".into(),
    };
    let request = ReadMembershipRequest {
        contract_version: 1,
        world: commit.world.clone(),
        min_commit: None,
        fixed_commit: None,
    };
    let scope = verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE;
    let view = WorldGovernanceMembershipView::read(
        &world,
        &commit,
        &world.execution_state_identity().unwrap().state_root_hash,
        scope,
    )
    .unwrap();
    (request, public_response(view, Some(scope.into())))
}

#[test]
fn public_membership_omits_operator_and_free_form_evidence() {
    let (request, response) = fixture();
    let bytes = serde_json::to_string(&response).unwrap();
    for secret in [
        "PRIVATE-OWNER",
        "PRIVATE-REASON",
        "operator_owner",
        "revocation_reason",
    ] {
        assert!(!bytes.contains(secret));
    }
    assert!(
        authenticated_assertion(
            response,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite
        )
        .is_ok()
    );
}

#[test]
fn engineering_scope_commit_digest_and_application_claims_fail_closed() {
    let (request, response) = fixture();
    assert!(
        authenticated_assertion(response.clone(), &request, ReadEvidencePolicy::Ordinary).is_err()
    );
    let mut unknown = response.clone();
    unknown.execution_evidence_scope = Some("formal".into());
    assert!(
        authenticated_assertion(
            unknown,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite
        )
        .is_err()
    );
    let mut hidden_scope = response.clone();
    hidden_scope.execution_evidence_scope = None;
    assert!(
        authenticated_assertion(hidden_scope, &request, ReadEvidencePolicy::Ordinary,).is_err()
    );
    let mut unscoped_authority = response.clone();
    unscoped_authority.execution_evidence_scope = None;
    unscoped_authority.history_profile = "controlled_single_authority".into();
    assert!(
        authenticated_assertion(unscoped_authority, &request, ReadEvidencePolicy::Ordinary,)
            .is_err()
    );
    let mut relabeled = response.clone();
    relabeled.history_profile = "controlled_single_authority".into();
    assert!(
        authenticated_assertion(
            relabeled,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite,
        )
        .is_err()
    );
    let mut changed = response.clone();
    changed.commit.world.genesis_digest = "changed".into();
    assert!(
        authenticated_assertion(
            changed,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite
        )
        .is_err()
    );
    let mut digest = response.clone();
    digest.effective_registry_digest = "wrong".into();
    assert!(
        authenticated_assertion(
            digest,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite
        )
        .is_err()
    );
    let mut applied = response;
    applied.applied_consensus_membership = Some(applied.commit.clone());
    assert!(
        authenticated_assertion(
            applied,
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite
        )
        .is_err()
    );
}

#[test]
fn membership_fixed_minimum_and_selected_node_bindings_are_validated() {
    let (mut request, response) = fixture();
    let mut minimum = response.commit.clone();
    minimum.position += 1;
    request.min_commit = Some(minimum);
    assert!(response.validate(&request).is_err());
    request.min_commit = None;
    let mut fixed = response.commit.clone();
    fixed.execution_block_hash = "another-block".into();
    request.fixed_commit = Some(fixed);
    assert!(response.validate(&request).is_err());
    request.fixed_commit = None;
    let mut selected = response;
    selected
        .selected_candidates
        .insert("wrong-node".into(), "candidate".into());
    assert!(selected.validate(&request).is_err());
}

#[test]
fn signed_membership_response_cannot_replay_for_a_fresh_observation() {
    let (query, response) = fixture();
    let old = MembershipReadEnvelope {
        query: query.clone(),
        observation_nonce: "11".repeat(32),
    };
    let fresh = MembershipReadEnvelope {
        query,
        observation_nonce: "22".repeat(32),
    };
    old.validate().unwrap();
    fresh.validate().unwrap();
    let reader = hex::encode([7; 32]);
    let service = hex::encode([9; 32]);
    let old = authority::sign_read_request(MEMBERSHIP_PATH, old, &reader).unwrap();
    let fresh = authority::sign_read_request(MEMBERSHIP_PATH, fresh, &reader).unwrap();
    let old_digest = authority::request_digest(MEMBERSHIP_PATH, &old).unwrap();
    let fresh_digest = authority::request_digest(MEMBERSHIP_PATH, &fresh).unwrap();
    let signed =
        authority::sign_service_response(MEMBERSHIP_PATH, old_digest.clone(), response, &service)
            .unwrap();
    let public_key = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[9; 32])
            .verifying_key()
            .to_bytes(),
    );
    assert!(
        authority::verify_service_response(MEMBERSHIP_PATH, &old_digest, &signed, &public_key)
            .is_ok()
    );
    assert!(
        authority::verify_service_response(MEMBERSHIP_PATH, &fresh_digest, &signed, &public_key)
            .is_err()
    );
}

// Backend fixtures must cross the same authenticated client boundary as a
// remote reader; a freely constructed response cannot mint an opaque value.
fn authenticated_assertion(
    response: ReadMembershipResponse,
    request: &ReadMembershipRequest,
    policy: ReadEvidencePolicy,
) -> Result<AuthenticatedMembershipObservation, String> {
    let signed_request = authority::sign_read_request(
        MEMBERSHIP_PATH,
        MembershipReadEnvelope {
            query: request.clone(),
            observation_nonce: "11".repeat(32),
        },
        &hex::encode([7; 32]),
    )?;
    let digest = authority::request_digest(MEMBERSHIP_PATH, &signed_request)?;
    let signed_response =
        authority::sign_service_response(MEMBERSHIP_PATH, digest, response, &hex::encode([9; 32]))?;
    let key = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[9; 32])
            .verifying_key()
            .to_bytes(),
    );
    AuthenticatedMembershipObservation::verify_service_assertion(
        signed_response,
        &signed_request,
        &key,
        policy,
    )
}
