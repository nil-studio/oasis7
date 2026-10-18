use super::*;
#[path = "formal_checkpoint_attempt_tests.rs"]
mod checkpoint_attempt_tests;
#[path = "formal_frozen_snapshot_tests.rs"]
mod frozen_snapshot_tests;
use crate::formal_read_fixtures as fixtures;
#[path = "formal_wasm_fixture.rs"]
mod wasm_fixture;
use oasis7_distfs::controlled_authority::{
    formal::{
        coordinator::{FormalCoordinator, FormalDurabilityEndpoint},
        endpoint::*,
    },
    replicated_protocol::EndpointRole,
};
use oasis7_node::NodeExecutionCommitContext;

#[path = "formal_snapshot_checkpoint_fixture.rs"]
mod snapshot_checkpoint_fixture;
use snapshot_checkpoint_fixture::frozen_at_snapshot_depth;

// In-memory test receivers exercise the runtime handoff only. Native durable
// endpoint and independent-provider qualification have separate storage tests.
struct FrozenReceiver {
    authority: VerifiedAuthorityV1,
    parent: VerifiedFormalCommitV1,
    role: EndpointRole,
    retained: Option<FormalRequestStateV1>,
}
fn unsupported<T>() -> Result<T, FormalError> {
    Err(FormalError::Invalid("runtime-only test receiver".into()))
}
impl FormalDurabilityEndpoint for FrozenReceiver {
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        Ok(self.parent.head())
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        Ok(match &self.retained {
            Some(retained) if retained.request_digest() == request => {
                FormalQueryOutcomeV1::Recorded(Box::new(retained.clone()))
            }
            _ => FormalQueryOutcomeV1::NotRecorded,
        })
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        verify_formal_reservation(&input, &self.authority, &self.parent)?;
        verify_frozen_material(&input.body, &material)?;
        let (instance, n) = match self.role {
            EndpointRole::Primary => (&self.authority.definition().primary.instance, 5),
            EndpointRole::Replica => (&self.authority.definition().replica.instance, 6),
        };
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: self.authority.authority_id()?,
            role: self.role,
            instance_digest: instance
                .digest()
                .map_err(|e| FormalError::Invalid(e.to_string()))?,
            reservation_id: input.body.reservation_id()?,
        };
        let receipt = FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body)?, n),
            body,
        };
        if let Some(old) = &self.retained {
            if let FormalRequestStateV1::Frozen {
                input: old_input,
                material: old_material,
                ..
            } = old
            {
                if old_input != &input || old_material != &material {
                    return Err(FormalError::Conflict("original test receiver input".into()));
                }
            } else {
                return unsupported();
            }
        }
        self.retained = Some(FormalRequestStateV1::Frozen {
            input,
            material,
            receipt: Some(receipt.clone()),
            certificate: None,
        });
        Ok(receipt)
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        certificate.verify(self.authority.definition(), self.authority.activation_ref())?;
        if let Some(FormalRequestStateV1::Frozen {
            input,
            certificate: slot,
            ..
        }) = &mut self.retained
        {
            if input != &certificate.reservation {
                return Err(FormalError::Conflict("original test certificate".into()));
            }
            *slot = Some(certificate);
            Ok(())
        } else {
            unsupported()
        }
    }
    fn install_recovery_frozen(
        &mut self,
        _: FormalFrozenCertificateV1,
        _: ClosedRecord,
    ) -> Result<(), FormalError> {
        unsupported()
    }
    fn prepare(
        &mut self,
        _: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        unsupported()
    }
    fn decide(
        &mut self,
        _: &str,
        _: FormalPrepareReceiptV1,
        _: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        unsupported()
    }
    fn install_proof(
        &mut self,
        _: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        unsupported()
    }
    fn publish_qualified(
        &mut self,
        _: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        unsupported()
    }
}

fn continuation(world: &World, parent: &VerifiedFormalCommitV1) -> NodeLocalExecutionContinuation {
    NodeLocalExecutionContinuation {
        schema_version: 1,
        context: NodeExecutionCommitContext {
            world_id: parent.record().world.world_id.clone(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 1,
            slot: 1,
            epoch: 1,
            node_block_hash: fixtures::h(90),
            action_root: compute_consensus_action_root(&[]).unwrap(),
            committed_actions: vec![],
            committed_at_unix_ms: 1,
        },
        predecessor_committed_height: 0,
        predecessor_node_block_hash: Some(fixtures::h(91)),
        predecessor_execution_height: 0,
        predecessor_execution_block_hash: Some(fixtures::h(92)),
        predecessor_execution_state_root: Some(blake3_hex(&to_cbor(world.snapshot()).unwrap())),
        next_height: 2,
        next_slot: 2,
        reserved_action_bytes: 0,
        approved_stake: 1,
        rejected_stake: 0,
        required_stake: 1,
        total_stake: 1,
    }
}

#[test]
fn formal_actual_input_closure_is_repeatable_and_binds_exact_decision() {
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        execution_policy_material(&ReleaseSecurityPolicy::default())
            .unwrap()
            .2,
    );
    let original = continuation(&world, anchor.head());
    let policy = ReleaseSecurityPolicy::default();
    let (body, material) = freeze_material(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
    )
    .unwrap();
    let repeated = freeze_material(
        anchor.authority(),
        anchor.head(),
        &original,
        &world,
        &policy,
    )
    .unwrap();
    assert_eq!((body.clone(), material.clone()), repeated);
    verify_frozen_material(&body, &material).unwrap();
    let manifest: FormalExecutionManifestV1 =
        capture::decode_role(&material, ArtifactRole::ExecutionManifest).unwrap();
    assert_eq!(manifest.continuation, original);
    assert_eq!(manifest.parent, anchor.head().head());
    assert_eq!(material.after_state_root, material.before_state_root);
    assert!(!manifest.domain.contains("engineering"));
    let mut changed = original.clone();
    changed.context.committed_at_unix_ms += 1;
    let (different, _) =
        freeze_material(anchor.authority(), anchor.head(), &changed, &world, &policy).unwrap();
    assert_ne!(body.request_digest, different.request_digest);
    assert_ne!(
        body.runtime_manifest_digest,
        different.runtime_manifest_digest
    );
    let mut wrong_prestate = original;
    wrong_prestate.predecessor_execution_state_root = Some(fixtures::h(93));
    assert!(
        freeze_material(
            anchor.authority(),
            anchor.head(),
            &wrong_prestate,
            &world,
            &policy
        )
        .is_err()
    );
    let mut tampered = body;
    tampered.runtime_manifest_digest = fixtures::h(94);
    assert!(verify_frozen_material(&tampered, &material).is_err());
}

#[test]
fn formal_initial_existing_world_has_complete_approved_material_and_stable_identity() {
    let (world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let release = ReleaseSecurityPolicy::default();
    let draft = || {
        super::super::formal_initial::InitialWorldClosure::from_existing_world(
            &world,
            &anchor.authority().definition().world,
            &release,
        )
        .unwrap()
    };
    let body = draft()
        .activation_body(
            anchor.authority().definition().clone(),
            anchor.authority().policy().clone(),
        )
        .unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (authority, first) = draft()
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    let (_, second) = draft()
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.material.roots.len(), 9);
    verify_record_material(&first.candidate.record, &first.material).unwrap();
    let proof = fixtures::qualified_proof(
        first.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let qualified = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    assert_eq!(
        qualified.head().record().after_state_root,
        blake3_hex(&to_cbor(world.snapshot()).unwrap())
    );
    assert!(
        draft()
            .authorize(&certificate, &trust, &fixtures::key(7))
            .is_err()
    );
    let mut wrong = certificate.clone();
    wrong.body.execution_policy_digest = fixtures::h(97);
    wrong.signature_hex = fixtures::sign(initial_activation_signing_bytes(&wrong.body).unwrap(), 9);
    assert!(
        draft()
            .authorize(&wrong, &trust, &fixtures::key(4))
            .is_err()
    );
    let mut different_identity = anchor.authority().definition().world.clone();
    different_identity.world_id = "different-world".into();
    assert!(
        super::super::formal_initial::InitialWorldClosure::from_existing_world(
            &world,
            &different_identity,
            &release
        )
        .is_err()
    );
    use oasis7::world_service::*;
    let identity = WorldIdentity {
        world_id: anchor.head().record().world.world_id.clone(),
        genesis_digest: anchor.head().record().world.genesis_digest.clone(),
    };
    let signed_payload = WorldServicePayloadV1::Delegation(
        authority::sign_read_request(
            "delegation",
            AgentSignerDelegationChangeV1 {
                world_id: identity.world_id.clone(),
                branch_id: "existing".into(),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                agent_identity_generation: 1,
                generation: 1,
                delegate_public_key: fixtures::public(7),
                revoked: false,
                nonce: 7,
            },
            &hex::encode([7u8; 32]),
        )
        .unwrap(),
    );
    let mut legacy_world =
        oasis7::runtime_host::world::from_snapshot(world.snapshot(), world.journal().clone())
            .unwrap();
    legacy_world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: SubmitIntentRequest {
                contract_version: 1,
                correlation: derive_correlation(identity, &signed_payload).unwrap(),
                deadline_unix_ms: None,
                signed_payload,
            },
            action_id: 1,
            committed_height: 1,
            receipt: serde_json::json!({"legacy": true}),
            rejected: None,
        })
        .unwrap();
    assert!(
        super::super::formal_initial::InitialWorldClosure::from_existing_world(
            &legacy_world,
            &anchor.authority().definition().world,
            &release,
        )
        .err()
        .unwrap()
        .contains("legacy result migration")
    );
}

#[test]
fn native_initial_approval_binds_external_descriptor_without_claiming_missing_bytes() {
    use oasis7_formal_protocol::native_environment::{
        NativeEnvironmentChunkV1, NativeEnvironmentComponentV1, NativeEnvironmentRole,
        NativeExecutionEnvironmentV1,
    };
    let (world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
    let release = ReleaseSecurityPolicy::default();
    let executable_digest = crate::controlled_execution_policy::executable_digest().unwrap();
    // A declared external image is deliberately absent. These are protocol
    // fixture commitments, not a captured Linux/macOS execution environment.
    let native = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "protocol-fixture".into(),
        os: "fixture".into(),
        arch: "fixture".into(),
        components: [
            NativeEnvironmentRole::Executable,
            NativeEnvironmentRole::BuildMetadata,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, role)| NativeEnvironmentComponentV1 {
            role,
            name: format!("fixture-{index}"),
            byte_len: 1,
            content_digest: if index == 0 {
                executable_digest.clone()
            } else {
                fixtures::h(44)
            },
            chunks: vec![NativeEnvironmentChunkV1 {
                digest: fixtures::h(44),
                byte_len: 1,
            }],
        })
        .collect(),
    };
    let draft = || {
        super::super::formal_initial::InitialWorldClosure::from_existing_world(
            &world,
            &anchor.authority().definition().world,
            &release,
        )
        .unwrap()
        .with_native_environment(&native)
        .unwrap()
    };
    let body = draft()
        .activation_body(
            anchor.authority().definition().clone(),
            anchor.authority().policy().clone(),
        )
        .unwrap();
    assert_eq!(body.schema_version, 2);
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (_, candidate) = draft()
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    assert_eq!(candidate.candidate.record.schema_version, 5);
    assert_eq!(candidate.material.roots.len(), 10);
    verify_record_material(&candidate.candidate.record, &candidate.material).unwrap();
    assert_eq!(
        super::super::formal_native_environment::descriptor(&candidate.material).unwrap(),
        Some(native.clone())
    );
    assert!(
        native
            .verify(|_| Err("fixture external bytes unavailable".into()))
            .is_err()
    );
}

#[test]
#[cfg(unix)]
fn formal_actual_wasm_execution_replays_the_identical_closed_candidate() {
    let (world, anchor) = wasm_fixture::world_and_anchor();
    let mut original = continuation(&world, anchor.head());
    let action = oasis7_node::NodeConsensusAction::from_payload(
        1,
        "node-a",
        oasis7::consensus_action_payload::encode_consensus_action_payload(
            &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_runtime_action(
                oasis7_runtime::Action::RegisterAgent {
                    agent_id: "formal-wasm-agent".into(),
                    pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
                },
            ),
        )
        .unwrap(),
    )
    .unwrap();
    original.context.committed_actions = vec![action];
    original.context.action_root =
        compute_consensus_action_root(&original.context.committed_actions).unwrap();
    original.reserved_action_bytes = original
        .context
        .committed_actions
        .iter()
        .map(|action| action.payload_cbor.len())
        .sum();
    let (body, material) =
        frozen_at_snapshot_depth(anchor.authority(), anchor.head(), &original, &world, 8);
    let input = SignedFormalFrozenV1 {
        signature_hex: fixtures::sign(frozen_signing_bytes(&body).unwrap(), 4),
        body,
    };
    let receiver = |role| FrozenReceiver {
        authority: anchor.authority().clone(),
        parent: anchor.head().clone(),
        role,
        retained: None,
    };
    let mut coordinator = FormalCoordinator::new(
        receiver(EndpointRole::Primary),
        receiver(EndpointRole::Replica),
        anchor.authority().clone(),
        anchor.authority().clone(),
        None,
        Some(anchor.head().clone()),
    )
    .unwrap();
    let executable = coordinator.freeze(input, material).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-formal-wasm-replay-{}", std::process::id()));
    assert!(!root.exists());
    super::frozen_snapshot::reset_counts();
    let first = super::super::formal_executor::execute_normal(
        &executable,
        anchor.authority(),
        anchor.head(),
        &root,
        &fixtures::key(4),
    )
    .unwrap();
    // These counts cover only the Node token, not endpoint verification or
    // candidate after-state validation. Both typed restores share one expansion.
    assert_eq!(super::frozen_snapshot::counts(), (1, 2));
    super::frozen_snapshot::reset_counts();
    let second = super::super::formal_executor::execute_normal(
        &executable,
        anchor.authority(),
        anchor.head(),
        &root,
        &fixtures::key(4),
    )
    .unwrap();
    assert_eq!(super::frozen_snapshot::counts(), (1, 2));
    assert_eq!(first, second);
    let checkpoint = &first.material.roots[&ArtifactRole::Snapshot];
    assert_eq!(checkpoint, &first.candidate.record.after_state_root);
    assert_eq!(
        blake3_hex(&first.material.snapshot_bytes().unwrap()),
        first.candidate.record.after_state_root,
    );
    let old_prestate = &executable.material().roots[&ArtifactRole::Snapshot];
    assert!(
        first
            .material
            .objects
            .iter()
            .any(|object| &object.content_hash == old_prestate)
    );
    assert_eq!(first.frozen_material.as_ref(), Some(executable.material()));
    let mut next_frozen_snapshot = BTreeMap::new();
    let inherited_root = super::super::formal_snapshot::inherit(
        &mut next_frozen_snapshot,
        &first.material,
        &first.material.snapshot_bytes().unwrap(),
    )
    .unwrap();
    assert_eq!(&inherited_root, checkpoint);
    assert_eq!(
        next_frozen_snapshot.len(),
        1,
        "next Frozen inherits only the new complete Snapshot"
    );
    let journal: oasis7_runtime::Journal =
        capture::decode_role(&first.material, ArtifactRole::Journal).unwrap();
    assert!(
        journal
            .events
            .iter()
            .any(|event| matches!(event.body, oasis7_runtime::WorldEventBody::ModuleEmitted(_)))
    );
    first.candidate.verify_original(anchor.authority()).unwrap();
    verify_record_material(&first.candidate.record, &first.material).unwrap();
    verify_formal_candidate(
        &first.candidate,
        first.frozen.as_ref(),
        anchor.authority(),
        Some(anchor.head()),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    assert_eq!(
        first.material.roots[&ArtifactRole::ExecutionManifest],
        executable.material().roots[&ArtifactRole::ExecutionManifest]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[path = "formal_storage_benchmark.rs"]
mod storage_benchmark;
