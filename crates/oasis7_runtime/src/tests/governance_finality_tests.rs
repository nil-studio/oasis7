use super::*;

#[test]
fn governance_apply_with_finality_uses_stake_weighted_threshold_bps() {
    let mut world = World::new();
    bind_finality_signer_with_seed(
        &mut world,
        LOCAL_FINALITY_SIGNER_1.0,
        LOCAL_FINALITY_SIGNER_1.1,
    );
    bind_finality_signer_with_seed(
        &mut world,
        LOCAL_FINALITY_SIGNER_2.0,
        LOCAL_FINALITY_SIGNER_2.1,
    );
    bind_finality_signer_with_seed(
        &mut world,
        ROTATED_FINALITY_SIGNER_3.0,
        ROTATED_FINALITY_SIGNER_3.1,
    );
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 0,
            threshold: 2,
            min_unique_signers: 2,
            threshold_bps: 6_667,
            signer_node_ids: vec![
                LOCAL_FINALITY_SIGNER_1.0.to_string(),
                LOCAL_FINALITY_SIGNER_2.0.to_string(),
                ROTATED_FINALITY_SIGNER_3.0.to_string(),
            ],
            validator_stakes: BTreeMap::from([
                (LOCAL_FINALITY_SIGNER_1.0.to_string(), 70),
                (LOCAL_FINALITY_SIGNER_2.0.to_string(), 20),
                (ROTATED_FINALITY_SIGNER_3.0.to_string(), 10),
            ]),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set weighted epoch snapshot");

    let rejected_manifest = Manifest {
        version: 2,
        content: json!({ "name": "weighted-finality-low-stake" }),
    };
    let rejected_proposal_id = world
        .propose_manifest_update(rejected_manifest, "alice")
        .unwrap();
    world.shadow_proposal(rejected_proposal_id).unwrap();
    world
        .approve_proposal(rejected_proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let low_stake_certificate = build_finality_certificate_with_signers(
        &world,
        rejected_proposal_id,
        &[LOCAL_FINALITY_SIGNER_2, ROTATED_FINALITY_SIGNER_3],
    );
    let err = world
        .apply_proposal_with_finality(rejected_proposal_id, &low_stake_certificate)
        .unwrap_err();
    let WorldError::GovernanceFinalityInvalid { reason } = err else {
        panic!("expected GovernanceFinalityInvalid");
    };
    assert!(reason.contains("signed stake below threshold_bps"));
    assert!(reason.contains("signed_stake_bps=3000"));

    let accepted_manifest = Manifest {
        version: 3,
        content: json!({ "name": "weighted-finality-high-stake" }),
    };
    let accepted_proposal_id = world
        .propose_manifest_update(accepted_manifest, "alice")
        .unwrap();
    world.shadow_proposal(accepted_proposal_id).unwrap();
    world
        .approve_proposal(accepted_proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let high_stake_certificate = build_finality_certificate_with_signers(
        &world,
        accepted_proposal_id,
        &[LOCAL_FINALITY_SIGNER_1, ROTATED_FINALITY_SIGNER_3],
    );
    world
        .apply_proposal_with_finality(accepted_proposal_id, &high_stake_certificate)
        .expect("high stake signer set should pass threshold_bps");
}
