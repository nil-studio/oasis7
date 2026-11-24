//! Original E1→E3 frozen-tail authentication/recovery fixture and negative
//! checks. Kept in a child module so full protocol proof construction does not
//! overgrow the parent libtest file; the public test identity remains stable.
use super::*;

// Separate construction of the full E1→E2→E3 signed proof graph from
// the subsequent independent restart, corruption and inclusion verification.
// All fixtures, signer identities and assertions are identical; no extra
// test-thread stack, ignored tests, or reduced acceptance is introduced.
#[inline(never)]
fn build_retained_recovery_setup<T>(build: impl FnOnce() -> T) -> T {
    build()
}

// Build and independently verify the E1→E2→E3 recovery chain on a separate
// default test-thread frame before assembling the candidate and read proof.
// Keep signer inputs and all original acceptance checks unchanged.
#[inline(never)]
fn build_retained_recovery_steps<T>(build: impl FnOnce() -> T) -> T {
    build()
}

// Direct stderr survives an unrelated test's default-stack SIGABRT and
// records the last completed signed proof stage without changing assertions.
#[inline(never)]
fn retained_e3_phase(label: &'static [u8]) {
    let _ = std::io::Write::write_all(&mut std::io::stderr(), label);
}

// Keep large verifier results off the signed recovery-proof construction
// frame; original success, rejection and cold-restore checks stay intact.
#[inline(never)]
fn boxed_retained_reader<T>(make: impl FnOnce() -> T) -> Box<T> {
    Box::new(make())
}

pub(super) fn exercise() {
    let (
        world,
        trust,
        anchor,
        initial_proof,
        origin_checkpoint,
        origin_digest,
        request,
        reply,
        recovery3,
        step2,
        step3,
        completion,
    retained_e3_phase(b"oasis7 native diagnostic game_outer_stage_enter\n");
    ) = build_retained_recovery_setup(|| {
        let (
            world,
            trust,
            anchor,
            initial_proof,
            origin_checkpoint,
            origin_digest,
            request,
            mut reply,
            view,
            frozen_body,
            frozen,
            step2,
            step3,
            recovery3,
        retained_e3_phase(b"oasis7 native diagnostic game_recovery_steps_enter\n");
        ) = build_retained_recovery_steps(|| {
            retained_e3_phase(b"oasis7 native diagnostic game_initial_fixture_begin\n");
            let (world, trust, anchor, initial_proof) = protocol_initial_fixture();
            let origin_checkpoint = anchor.checkpoint();
            let origin_digest = origin_checkpoint.digest().unwrap();
            let (request, reply) = response(
                &world,
                &anchor,
                FormalReadHistoryV1 {
                    schema_version: 1,
                    steps: vec![],
                },
            );
            let original = anchor.authority();
            let mut view = reply.view.clone();
            view.runtime_binding.as_mut().unwrap().runtime_manifest_hash =
                serde_json::from_value(serde_json::json!(format!("blake3:{}", h(91)))).unwrap();
            view.formal_commit_header
                .as_mut()
                .unwrap()
                .permission_generation += 1;
            let frozen_body = FormalFrozenBodyV1 {
                schema_version: 1,
                world: original.definition().world.clone(),
                authority_id: original.authority_id().unwrap(),
                epoch: original.definition().epoch,
                activation_ref: original.activation_ref().into(),
                parent: anchor.head().head(),
                next_position: anchor.head().record().position + 1,
                request_digest: h(60),
                payload_digest: h(61),
                runtime_manifest_digest: h(62),
                governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
                execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
                pre_state_root: anchor.head().record().after_state_root.clone(),
                frozen_material_digest: h(64),
                handoff: None,
            };
            let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
                let body = FormalFrozenReceiptBodyV1 {
                    authority_id: original.authority_id().unwrap(),
                    role,
                    instance_digest: binding.instance.digest().unwrap(),
                    reservation_id: frozen_body.reservation_id().unwrap(),
                };
                FormalFrozenReceiptV1 {
                    signature_hex: sign(frozen_receipt_signing_bytes(&body).unwrap(), key_id),
                    body,
                }
            };
            retained_e3_phase(b"oasis7 native diagnostic game_frozen_cert_begin\n");
            let frozen = FormalFrozenCertificateV1 {
                reservation: SignedFormalFrozenV1 {
                    signature_hex: sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
                    body: frozen_body.clone(),
                },
                primary: frozen_receipt(EndpointRole::Primary, &original.definition().primary, 5),
                replica: frozen_receipt(EndpointRole::Replica, &original.definition().replica, 6),
            };
            let witness = RecoveryTailWitnessV1::Frozen(frozen.clone());
            let next2 = successor(original, 2, 13, 14, 15, 2);
            let minimum = issuer::IssuerAnchorV1::initial(original.policy()).unwrap();
            retained_e3_phase(b"oasis7 native diagnostic game_recovery_step2_begin\n");
            let (step2, issuer_anchor2) = recovery_step(
                original,
                anchor.head(),
                next2,
                witness.clone(),
                None,
                1,
                minimum,
                [5, 6],
            );
            retained_e3_phase(b"oasis7 native diagnostic game_recovery_step2_created\n");
            let recovery2 = history::verify_recovery_step(&step2, &anchor, None)
                .unwrap()
                .recovery;
            let context3 = VerifiedActiveRecoveryContextV2::from_previous(
                &recovery2,
                original,
                anchor.head(),
                &witness,
            )
            .unwrap();
            let next3 = successor(recovery2.successor(), 3, 23, 24, 25, 2);
            retained_e3_phase(b"oasis7 native diagnostic game_recovery_step3_begin\n");
            let (step3, _) = recovery_step(
                recovery2.successor(),
                anchor.head(),
                next3,
                witness.clone(),
                Some(&context3),
                2,
                issuer_anchor2,
                [14, 15],
            );
            retained_e3_phase(b"oasis7 native diagnostic game_recovery_step3_created\n");
            let recovery3 = history::verify_recovery_step(&step3, &anchor, Some(&recovery2))
                .unwrap()
                .recovery;
            (
                world,
                trust,
                anchor,
                initial_proof,
                origin_checkpoint,
                origin_digest,
                request,
                reply,
                view,
                frozen_body,
                frozen,
                step2,
                step3,
                recovery3,
            )
        });
        let original = anchor.authority();

        let audience = ProjectionAudienceV1::public();
        let mut record = anchor.head().record().clone();
        record.kind = FormalRecordKindV1::Execution;
        record.position = frozen_body.next_position;
        record.parent_record_id = frozen_body.parent.record_id.clone();
        record.request_digest = frozen_body.request_digest.clone();
        record.payload_digest = frozen_body.payload_digest.clone();
        record.frozen_reservation_id = Some(frozen_body.reservation_id().unwrap());
        record.runtime_manifest_digest = frozen_body.runtime_manifest_digest.clone();
        record.governing_manifest_digest = h(91);
        record.before_state_root = frozen_body.pre_state_root.clone();
        record.after_state_root = h(65);
        record.action_root = h(66);
        record.event_root = h(67);
        record.result_root = h(68);
        record
            .artifact_roots
            .insert(ArtifactRole::ExecutionManifest, h(62));
        let tree = ProjectionCommitmentsV1::from_leaves(vec![(
            audience.clone(),
            oasis7_client_api::formal_projection::projection_leaf(&audience, &view).unwrap(),
        )])
        .unwrap();
        record.projection_root = tree.root().unwrap();
        retained_e3_phase(b"oasis7 native diagnostic game_candidate_begin\n");
        let candidate = SignedCanonicalCandidateV1 {
            writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 23),
            record: record.clone(),
        };
        let candidate_context = verify_formal_candidate(
            &candidate,
            Some(&frozen),
            original,
            Some(anchor.head()),
            Some(&recovery3),
            FormalCompletionBranchV1::ReplayFrozen,
        )
        .unwrap();
        let final_receipt = |role, key_id| {
            let body = candidate_context.receipt_body(role).unwrap();
            FormalFinalReceiptV1 {
                signature_hex: sign(formal_final_receipt_signing_bytes(&body).unwrap(), key_id),
                body,
            }
        };
        let decision = FormalDecisionProofV1 {
            schema_version: 1,
            record: record.clone(),
            branch: FormalCompletionBranchV1::ReplayFrozen,
            writer_signature_hex: candidate.writer_signature_hex,
            frozen: Some(frozen.clone()),
            primary: final_receipt(EndpointRole::Primary, 24),
            replica: final_receipt(EndpointRole::Replica, 25),
        };
        retained_e3_phase(b"oasis7 native diagnostic game_verified_decision_begin\n");
        let verified_decision =
            verify_formal_decision(&decision, original, Some(anchor.head()), Some(&recovery3))
                .unwrap();
        let complete_receipt = |role, key_id| {
            let body = verified_decision.complete_receipt_body(role).unwrap();
            FormalProofCompleteReceiptV1 {
                signature_hex: sign(formal_proof_complete_signing_bytes(&body).unwrap(), key_id),
                body,
            }
        };
        retained_e3_phase(b"oasis7 native diagnostic game_completion_begin\n");
        let completion = FormalQualifiedCommitProofV1 {
            schema_version: 1,
            decision,
            primary_complete: complete_receipt(EndpointRole::Primary, 24),
            replica_complete: complete_receipt(EndpointRole::Replica, 25),
        };

        let binding = view.runtime_binding.as_ref().unwrap();
        let header = view.formal_commit_header.as_ref().unwrap();
        let era = header.event_id_era;
        let sequence = header.journal_high_watermark;
        let commit = CommitRef {
            world: reply.version.commit.world.clone(),
            position: record.position,
            execution_block_hash: record.record_id().unwrap(),
            state_root_ref: record.after_state_root.clone(),
            binding: ExecutionBinding {
                provider_world_id: binding.world_id.clone(),
                branch_id: binding.branch_id.clone(),
                finality_ref: authority::request_digest(
                    "finality",
                    &(
                        binding.finality_epoch,
                        &binding.finality_status,
                        &binding.finality_block_hash,
                    ),
                )
                .unwrap(),
                reorg_generation: binding.reorg_epoch,
                governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
                authority_generation: header.authority_generation,
                permission_generation: header.permission_generation,
            },
        };
        reply.view = view;
        reply.logical_tick = reply.view.state.time;
        reply.version.commit = commit.clone();
        reply.version.projection_revision =
            authority::request_digest("projection", &reply.view).unwrap();
        reply.continuation.commit = commit.clone();
        reply.continuation.era = era;
        reply.continuation.sequence = sequence;
        let inclusion = tree.inclusion(&audience).unwrap();
        let sidecar = reply.formal_proof.as_mut().unwrap();
        sidecar.history_cbor_hex = FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![
                FormalReadStepV1::Recovery(Box::new(step2.clone())),
                FormalReadStepV1::Recovery(Box::new(step3.clone())),
                FormalReadStepV1::Commit(Box::new(completion.clone())),
            ],
        }
        .encode_hex()
        .unwrap();
        sidecar.projection = FormalProjectionInclusionV1 {
            schema_version: inclusion.schema_version,
            audience: FormalProjectionAudienceV1 {
                scope_id: inclusion.audience.scope_id,
                reader_public_key: inclusion.audience.reader_public_key,
            },
            leaf_index: inclusion.leaf_index,
            leaf_count: inclusion.leaf_count,
            siblings: inclusion.siblings,
        };
        (
            world,
            trust,
            anchor,
            initial_proof,
            origin_checkpoint,
            origin_digest,
            request,
            reply,
            recovery3,
            step2,
            step3,
            completion,
        )
    });
    retained_e3_phase(b"oasis7 native diagnostic game_setup_returned\n");
    let mut without_previous =
        boxed_retained_reader(|| FormalReadVerifier::new(&trust, anchor.clone()).unwrap());
    let before = without_previous.anchor_ref().unwrap();
    let mut skipped = reply.clone();
    skipped.formal_proof.as_mut().unwrap().history_cbor_hex = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![FormalReadStepV1::Recovery(Box::new(step3.clone()))],
    }
    .encode_hex()
    .unwrap();
    assert!(
        without_previous
            .verify_view(&skipped, &request, &public(7))
            .unwrap_err()
            .starts_with("HistoryUnavailable")
    );
    assert_eq!(without_previous.anchor_ref().unwrap(), before);

    retained_e3_phase(b"oasis7 native diagnostic game_first_reader_begin\n");
    let mut verifier =
        boxed_retained_reader(|| FormalReadVerifier::new(&trust, anchor.clone()).unwrap());
    let observed = boxed_retained_reader(|| {
        verifier.verify_view(&reply, &request, &public(7)).unwrap()
    });
    assert_eq!(observed.commit().record().position, 2);
    assert_eq!(
        observed.commit().finalizing_authority_id(),
        recovery3.successor().authority_id().unwrap()
    );
    let completed_history =
        FormalReadHistoryV1::decode_hex(&reply.formal_proof.as_ref().unwrap().history_cbor_hex)
            .unwrap();
    let advanced = verifier.anchor_ref().unwrap();
    assert_eq!(advanced.position, 2);
    assert!(verifier.checkpoint().is_err());
    let checkpoint_v2 = boxed_retained_reader(|| verifier.checkpoint_v2(&origin_digest).unwrap());
    let pinned_v2_digest = checkpoint_v2.digest().unwrap();
    let decoded = FormalReadCheckpointV2::decode_hex(&checkpoint_v2.encode_hex().unwrap()).unwrap();
    retained_e3_phase(b"oasis7 native diagnostic game_checkpoint_reopen_begin\n");
    let restored = boxed_retained_reader(|| {
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &decoded,
            &pinned_v2_digest,
            &origin_digest,
        )
        .unwrap()
    });
    assert_eq!(restored.anchor_ref().unwrap(), advanced);
    assert_eq!(
        restored.qualified_commits.len(),
        verifier.qualified_commits.len()
    );
    assert_eq!(restored.verified_authorities.len(), 3);
    let mut tampered = decoded.clone();
    tampered.current.head_record.result_root = h(97);
    assert!(
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &tampered,
            &pinned_v2_digest,
            &origin_digest,
        )
        .is_err()
    );

    retained_e3_phase(b"oasis7 native diagnostic game_origin_reader_begin\n");
    let (origin_request, mut origin_reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let origin_history = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Recovery(Box::new(step2)),
            FormalReadStepV1::Recovery(Box::new(step3)),
        ],
    };
    origin_reply.formal_proof.as_mut().unwrap().history_cbor_hex =
        origin_history.encode_hex().unwrap();
    let mut schema2_origin_reader =
        boxed_retained_reader(|| FormalReadVerifier::new(&trust, anchor.clone()).unwrap());
    schema2_origin_reader
        .verify_view(&origin_reply, &origin_request, &public(7))
        .unwrap();
    let schema2_origin = schema2_origin_reader.current.checkpoint();
    // A schema-2 active E3 checkpoint cannot be promoted by the legacy V1
    // import path; the V2 origin qualification below carries its E1→E2→E3
    // verified recovery lineage instead.
    assert!(
        VerifiedFormalReadAnchorV1::from_fixed_pin(
            &schema2_origin,
            &schema2_origin.digest().unwrap(),
            &trust,
        )
        .is_err()
    );
    let origin_proof_digest = formal_qualified_proof_digest(&initial_proof).unwrap();
    let origin_qualification = FormalReadOriginQualificationV2 {
        base: origin_checkpoint.clone(),
        base_digest: origin_digest.clone(),
        history: origin_history,
        origin_record: anchor.head().record().clone(),
        finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        qualified_proof_digest: origin_proof_digest.clone(),
        qualified_proof: initial_proof,
    };
    let schema2_origin_digest = schema2_origin.digest().unwrap();
    let rebased = FormalReadCheckpointV2 {
        schema_version: 2,
        origin: schema2_origin.clone(),
        origin_digest: schema2_origin_digest.clone(),
        origin_qualification: Some(origin_qualification),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
        current: schema2_origin,
    };
    let rebased_digest = rebased.digest().unwrap();
    retained_e3_phase(b"oasis7 native diagnostic game_rebase_reader_begin\n");
    let rebased_reader = boxed_retained_reader(|| {
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &rebased,
            &rebased_digest,
            &schema2_origin_digest,
        )
        .unwrap()
    });
    assert_eq!(
        rebased_reader.anchor_ref().unwrap().active_authority_id,
        recovery3.successor().authority_id().unwrap()
    );
    let initial_qualification_key = QualifiedCommitKey {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id.clone(),
        proof_digest: origin_proof_digest,
    };
    assert!(
        rebased_reader
            .qualified_commits
            .contains_key(&initial_qualification_key)
    );
    assert_eq!(rebased_reader.verified_authorities.len(), 3);

    let completed_origin = verifier.current.checkpoint();
    let completed_origin_digest = completed_origin.digest().unwrap();
    let replayed_non_normal_proof_digest = formal_qualified_proof_digest(&completion).unwrap();
    let replayed_non_normal_qualification = FormalReadOriginQualificationV2 {
        base: origin_checkpoint.clone(),
        base_digest: origin_digest.clone(),
        history: completed_history,
        origin_record: observed.commit().record().clone(),
        finalizing_authority_id: observed.commit().finalizing_authority_id().to_owned(),
        qualified_proof_digest: replayed_non_normal_proof_digest.clone(),
        qualified_proof: completion.clone(),
    };
    let replayed_non_normal_checkpoint = FormalReadCheckpointV2 {
        schema_version: 2,
        origin: completed_origin.clone(),
        origin_digest: completed_origin_digest.clone(),
        origin_qualification: Some(replayed_non_normal_qualification),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
        current: completed_origin,
    };
    let replayed_non_normal_digest = replayed_non_normal_checkpoint.digest().unwrap();
    retained_e3_phase(b"oasis7 native diagnostic game_non_normal_reader_begin\n");
    let replayed_non_normal_reader = boxed_retained_reader(|| {
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &replayed_non_normal_checkpoint,
            &replayed_non_normal_digest,
            &completed_origin_digest,
        )
        .unwrap()
    });
    let replayed_non_normal_key = QualifiedCommitKey {
        position: observed.commit().record().position,
        record_id: observed.commit().head().record_id.clone(),
        proof_digest: replayed_non_normal_proof_digest,
    };
    let replayed_non_normal_token = replayed_non_normal_reader
        .qualified_commits
        .get(&replayed_non_normal_key)
        .expect("origin replay retains exact non-Normal proof context");
    assert!(replayed_non_normal_token.recovery.is_some());
    assert!(replayed_non_normal_token.retained_context.is_some());

    let without_origin_qualification = FormalReadCheckpointV2 {
        origin_qualification: None,
        ..rebased
    };
    let without_origin_digest = without_origin_qualification.digest().unwrap();
    assert!(
        FormalReadVerifier::from_fixed_checkpoint_v2(
            &trust,
            &without_origin_qualification,
            &without_origin_digest,
            &schema2_origin_digest,
        )
        .is_err()
    );
    assert!(verifier.verify_view(&reply, &request, &public(7)).is_err());
    assert_eq!(verifier.anchor_ref().unwrap(), advanced);
}
