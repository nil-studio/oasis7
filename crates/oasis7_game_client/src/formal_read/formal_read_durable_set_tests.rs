//! Light-consumer signature fixtures, not native custody/install evidence.
use super::historical_v2_tests::{normal_execution_proof_with_signers, profile2_initial_fixture};
use super::*;

fn fixture() -> (
    fixtures::protocol_world::ProtocolWorld,
    InitialActivationTrustV1,
    VerifiedFormalReadAnchorV1,
    FormalQualifiedCommitProofV1,
    FormalNormalDurableSetReadStepV1,
) {
    let (world, trust, anchor, _) = profile2_initial_fixture();
    let old = anchor.authority();
    let mut successor = old.definition().clone();
    successor.epoch += 1;
    successor.predecessor_authority_id = Some(old.authority_id().unwrap());
    successor.valid_from_position = anchor.head().record().position + 2;
    successor.primary.instance.authority_epoch = successor.epoch;
    successor.replica.instance.authority_epoch = successor.epoch;
    successor.replica.instance.endpoint_public_key = public(12);
    successor.replica.instance.instance_id = h(120);
    successor.replica.instance.storage_id = h(121);
    successor.replica.instance.storage_fingerprint = h(122);
    successor.replica.instance.endpoint_id = "replacement-replica".into();
    successor.replica.fault_domain_id = "replacement-replica-domain".into();
    let mut body = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: old.definition().world.clone(),
        predecessor_authority_id: old.authority_id().unwrap(),
        predecessor: anchor.head().head(),
        boundary_position: anchor.head().record().position + 1,
        operation_id: h(123),
        successor,
        independent_closure_root: h(124),
    };
    body.independent_closure_root = body.copy_contract_digest().unwrap();
    let transition = SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: sign(
            normal_durable_set_transition_signing_bytes(&body).unwrap(),
            4,
        ),
        body,
    };
    let original =
        normal_execution_proof_with_signers(old, anchor.head(), 60, h(65), &h(66), [4, 5, 6]);
    let mut record = original.decision.record;
    record.schema_version = 2;
    record.durable_set_transition_digest = Some(transition.body.digest().unwrap());
    let proof = qualified_proof(
        record,
        original.decision.frozen,
        old,
        Some(anchor.head()),
        None,
        4,
        5,
        6,
    );
    let boundary = verify_formal_qualified(&proof, old, Some(anchor.head()), None).unwrap();
    let seals = [
        epoch_seal(
            old,
            &boundary,
            &transition.body.digest().unwrap(),
            2,
            &RecoveryTailWitnessV1::Empty,
            5,
            0,
        ),
        epoch_seal(
            old,
            &boundary,
            &transition.body.digest().unwrap(),
            2,
            &RecoveryTailWitnessV1::Empty,
            6,
            1,
        ),
    ];
    let install = |role: usize, signer: u8| {
        let instance = if role == 0 {
            &transition.body.successor.primary.instance
        } else {
            &transition.body.successor.replica.instance
        };
        let checkpoint = EndpointCheckpointBindingV1 {
            revision: 6,
            commitment: h(150 + role as u8),
            head: HeadAnchor {
                position: boundary.head().position,
                decision_hash: boundary.head().record_id,
                qualified: true,
            },
        };
        let body = DurableSetInstallBodyV1 {
            schema_version: 1,
            transition_digest: transition.body.digest().unwrap(),
            successor_authority_id: transition.body.successor.authority_id().unwrap(),
            instance: instance.clone(),
            boundary: boundary.head(),
            independent_closure_root: transition.body.independent_closure_root.clone(),
            independent_inventory_digest: h(152),
            checkpoint: checkpoint.clone(),
        };
        let statement = MonotonicProviderStatementV1 {
            schema_version: 1,
            capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
            resource_binding: old.policy().authorized_fencing_resource_bindings[0].clone(),
            instance_digest: instance.digest().unwrap(),
            checkpoint,
            epoch_floor: 2,
            seal_digest: None,
        };
        DurableSetInstallReceiptV1 {
            endpoint_signature_hex: sign(
                durable_set_install_endpoint_signing_bytes(&body).unwrap(),
                signer,
            ),
            provider_signature_hex: sign(
                durable_set_install_provider_signing_bytes(&body).unwrap(),
                3,
            ),
            provider_evidence: MonotonicProviderEvidenceV1 {
                signature: domain_sign("oasis7.endpoint.monotonic-provider.v1", &statement, 3),
                statement,
            },
            body,
        }
    };
    let installs = [install(0, 5), install(1, 12)];
    (
        world,
        trust,
        anchor,
        proof,
        FormalNormalDurableSetReadStepV1 {
            transition,
            old_seals: seals,
            new_installs: installs,
        },
    )
}

#[test]
fn healthy_durable_set_read_replays_dual_activation_and_first_normal_from_fixed_origin() {
    let (_, trust, origin, boundary_proof, step) = fixture();
    let mut reader = FormalReadVerifier::new(&trust, origin.clone()).unwrap();
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(boundary_proof))])
        .unwrap();
    let old = reader.current.authority().clone();
    reader
        .replay_history_steps(&[FormalReadStepV1::DurableSetActivation(Box::new(
            step.clone(),
        ))])
        .unwrap();
    assert!(reader.checkpoint().is_err());
    assert_eq!(reader.current.authority().definition().epoch, 2);
    assert_eq!(reader.current.head().head().position, 2);
    let first = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        70,
        h(75),
        &h(76),
        [4, 5, 12],
    );
    assert!(verify_formal_qualified(&first, &old, Some(reader.current.head()), None).is_err());
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(first))])
        .unwrap();
    assert_eq!(reader.current.head().head().position, 3);
    let origin_digest = origin.checkpoint().digest().unwrap();
    let checkpoint = reader.checkpoint_v2(&origin_digest).unwrap();
    let reopened = FormalReadVerifier::from_fixed_checkpoint_v2(
        &trust,
        &checkpoint,
        &checkpoint.digest().unwrap(),
        &origin_digest,
    )
    .unwrap();
    assert_eq!(
        reader.replay_cursor().unwrap(),
        reopened.replay_cursor().unwrap()
    );
    let mut corrupt = checkpoint.clone();
    let FormalReadStepV1::DurableSetActivation(activation) = &mut corrupt.history.steps[1] else {
        panic!("expected activation")
    };
    activation.new_installs[1].body.independent_closure_root = h(200);
    assert!(
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &corrupt,
            &corrupt.digest().unwrap(),
            &origin_digest
        )
        .is_err()
    );
}

#[test]
fn healthy_durable_set_read_rejects_missing_boundary_predecessor_or_dual_install() {
    let (_, trust, origin, proof, step) = fixture();
    let mut reader = FormalReadVerifier::new(&trust, origin.clone()).unwrap();
    assert!(
        reader
            .clone()
            .replay_history_steps(&[FormalReadStepV1::DurableSetActivation(Box::new(
                step.clone()
            ))])
            .is_err()
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(proof))])
        .unwrap();
    let before = reader.replay_cursor().unwrap();
    let mut corrupt = step.clone();
    corrupt.new_installs[1] = corrupt.new_installs[0].clone();
    assert!(
        reader
            .clone()
            .replay_history_steps(&[FormalReadStepV1::DurableSetActivation(Box::new(corrupt))])
            .is_err()
    );
    let mut divergent = step.clone();
    let body = &mut divergent.new_installs[1].body;
    body.independent_inventory_digest = h(199);
    divergent.new_installs[1].endpoint_signature_hex = sign(
        durable_set_install_endpoint_signing_bytes(body).unwrap(),
        12,
    );
    divergent.new_installs[1].provider_signature_hex =
        sign(durable_set_install_provider_signing_bytes(body).unwrap(), 3);
    assert!(
        reader
            .clone()
            .replay_history_steps(&[FormalReadStepV1::DurableSetActivation(Box::new(divergent))])
            .is_err()
    );
    let mut missing = reader.clone();
    missing.history.remove(&origin.head().record().position);
    assert!(
        missing
            .replay_history_steps(&[FormalReadStepV1::DurableSetActivation(Box::new(step))])
            .unwrap_err()
            .contains("predecessor unavailable")
    );
    assert_eq!(before, reader.replay_cursor().unwrap());
}

#[test]
fn healthy_durable_set_read_retains_original_result_after_epoch_change_without_old_cache() {
    use super::historical_v2_tests::{EmptyIndexNodes, memory_index_nodes};
    use oasis7_formal_protocol::controlled_authority::formal::{
        business_history::FormalHistoricalCutV1,
        business_index::{BusinessIndexDeltaV1, BusinessIndexRootV1},
    };
    let (world, trust, origin, boundary_proof, step) = fixture();
    let original_proof = qualified_proof(
        origin.head().record().clone(),
        None,
        origin.authority(),
        None,
        None,
        4,
        5,
        6,
    );
    let original_cut = FormalHistoricalCutV1::from_proof(&original_proof).unwrap();
    let leaf = original_cut.leaf().unwrap();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf.clone()],
        &EmptyIndexNodes,
    )
    .unwrap();
    let descriptor = delta.new;
    let nodes = memory_index_nodes(delta.node_writes);
    let mut reader = FormalReadVerifier::new(&trust, origin.clone()).unwrap();
    reader
        .replay_history_steps(&[
            FormalReadStepV1::Commit(Box::new(boundary_proof)),
            FormalReadStepV1::DurableSetActivation(Box::new(step)),
        ])
        .unwrap();
    assert_eq!(
        reader.current.authority().activation_ref(),
        origin.authority().activation_ref()
    );
    let first = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        80,
        descriptor.canonical_result_root().unwrap(),
        &h(85),
        [4, 5, 12],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(first))])
        .unwrap();
    reader.qualified_commits.clear();
    let inclusion = descriptor.prove(&leaf.key_digest, &nodes).unwrap();
    let historical =
        VerifiedHistoricalCutInclusionV2::verify(&reader.current, &descriptor, &inclusion, 1)
            .unwrap();
    let (request, response) = super::response(
        &world,
        &origin,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: Vec::new(),
        },
    );
    let before = reader.replay_cursor().unwrap();
    let restored = reader
        .verify_committed_historical_view(&historical, &response, &request, &public(7))
        .unwrap();
    assert_eq!(restored.commit().record(), origin.head().record());
    assert_eq!(before, reader.replay_cursor().unwrap());
}
