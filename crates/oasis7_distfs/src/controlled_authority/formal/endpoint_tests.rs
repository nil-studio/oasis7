use super::*;
use crate::controlled_authority::formal::coordinator::*;
use crate::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[path = "endpoint_signed_provider_fixture.rs"]
mod signed_provider;
use signed_provider::SignedTestProvider;
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
fn h(n: u8) -> String {
    hex::encode([n; 32])
}
fn signed(bytes: Vec<u8>, n: u8) -> String {
    hex::encode(key(n).sign(&bytes).to_bytes())
}
fn material(input: Vec<u8>, snapshot: Vec<u8>, manifest: Vec<u8>) -> ClosedRecord {
    let mut roots = BTreeMap::new();
    let mut objects = BTreeMap::new();
    for role in [
        ArtifactRole::Input,
        ArtifactRole::Result,
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
    ] {
        let bytes = match role {
            ArtifactRole::Input => input.clone(),
            ArtifactRole::Snapshot => snapshot.clone(),
            ArtifactRole::ExecutionManifest => manifest.clone(),
            _ => format!("{role:?}").into_bytes(),
        };
        let digest = blake3::hash(&bytes).to_hex().to_string();
        roots.insert(role, digest.clone());
        objects.insert(
            digest.clone(),
            ArtifactObject {
                content_hash: digest,
                bytes,
                references: vec![],
            },
        );
    }
    ClosedRecord {
        payload_digest: roots[&ArtifactRole::Input].clone(),
        before_state_root: roots[&ArtifactRole::Snapshot].clone(),
        after_state_root: roots[&ArtifactRole::Snapshot].clone(),
        roots,
        objects: objects.into_values().collect(),
    }
}
struct Fixture {
    path: PathBuf,
    pdir: PathBuf,
    rdir: PathBuf,
    provider: Arc<SignedTestProvider>,
    authority: VerifiedAuthorityV1,
    initial: FormalCandidateMaterialV1,
}
impl Fixture {
    fn new() -> Self {
        Self::new_with_native(None)
    }
    fn new_with_native(
        native: Option<
            &crate::controlled_authority::formal::native_environment::NativeExecutionEnvironmentV1,
        >,
    ) -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let pdir = path.join("primary");
        let rdir = path.join("replica");
        fs::create_dir(&pdir).unwrap();
        fs::create_dir(&rdir).unwrap();
        let provider = Arc::new(SignedTestProvider::new(path.join("provider"), key(3)));
        let policy = RecoveryPolicyV1 {
            schema_version: 1,
            recovery_issuer_public_key: public(1),
            fencing_provider_public_key: public(3),
            authorized_fencing_resource_bindings: vec![provider.resource_binding().into()],
            issuer_anchor_public_key: public(2),
            allow_complete_prepared: true,
            allow_replay_frozen: true,
            successor_rule: SuccessorRuleV1::IncrementEpochDistinctBindings,
            terminates_at_profile_transition: true,
        };
        let world = FormalWorldIdentityV1 {
            world_id: "formal-real".into(),
            chain_id: "chain".into(),
            genesis_digest: h(8),
        };
        let binding = |role, n, dir: &Path| EndpointSignerBindingV1 {
            instance: fencing::EndpointInstanceBindingV1 {
                schema_version: 1,
                world_id: world.world_id.clone(),
                chain_id: world.chain_id.clone(),
                genesis_digest: world.genesis_digest.clone(),
                authority_epoch: 1,
                endpoint_role: role,
                endpoint_id: format!("endpoint-{n}"),
                endpoint_public_key: public(n),
                instance_id: h(n + 10),
                storage_id: h(n + 20),
                storage_fingerprint: fencing::directory_fingerprint(dir).unwrap(),
            },
            fault_domain_id: format!("domain-{n}"),
        };
        let definition = AuthorityDefinitionV1 {
            schema_version: 1,
            world: world.clone(),
            profile: FORMAL_PROFILE.into(),
            profile_version: 1,
            epoch: 1,
            writer_public_key: public(4),
            primary: binding(EndpointRole::Primary, 5, &pdir),
            replica: binding(EndpointRole::Replica, 6, &rdir),
            recovery_policy_digest: policy.digest().unwrap(),
            predecessor_authority_id: None,
            valid_from_position: 1,
        };
        let mut closed = material(
            b"placeholder".to_vec(),
            b"state0".to_vec(),
            b"manifest".to_vec(),
        );
        if let Some(descriptor) = native {
            native_tests::attach(&mut closed, descriptor);
        }
        let mut roots = closed.roots.clone();
        roots.remove(&ArtifactRole::Input);
        let body = InitialActivationBodyV1 {
            schema_version: if native.is_some() { 2 } else { 1 },
            authority: definition,
            recovery_policy: policy.clone(),
            initial_state_root: closed.before_state_root.clone(),
            runtime_manifest_digest: closed.roots[&ArtifactRole::ExecutionManifest].clone(),
            governing_manifest_digest: h(80),
            execution_policy_digest: h(50),
            artifact_roots: roots,
        };
        let certificate = SignedInitialActivationV1 {
            signature_hex: signed(initial_activation_signing_bytes(&body).unwrap(), 9),
            body,
        };
        let authority = verify_initial_activation(
            &certificate,
            &InitialActivationTrustV1 {
                issuer_public_key: public(9),
                governing_manifest_digest: h(80),
                world,
                initial_state_root: certificate.body.initial_state_root.clone(),
                recovery_policy_digest: policy.digest().unwrap(),
            },
        )
        .unwrap();
        closed = material(
            initial_activation_input_bytes(&certificate).unwrap(),
            b"state0".to_vec(),
            b"manifest".to_vec(),
        );
        if let Some(descriptor) = native {
            native_tests::attach(&mut closed, descriptor);
        }
        let record = CanonicalRecordV1 {
            schema_version: if native.is_some() { 5 } else { 1 },
            durable_set_transition_digest: None,
            kind: FormalRecordKindV1::InitialActivation,
            world: authority.definition().world.clone(),
            position: 1,
            parent_record_id: authority
                .definition()
                .world
                .genesis_head()
                .unwrap()
                .record_id,
            original_authority_id: authority.authority_id().unwrap(),
            original_epoch: 1,
            activation_ref: authority.activation_ref().into(),
            request_digest: h(51),
            payload_digest: closed.payload_digest.clone(),
            frozen_reservation_id: None,
            runtime_manifest_digest: certificate.body.runtime_manifest_digest.clone(),
            governing_manifest_digest: h(80),
            execution_policy_digest: h(50),
            before_state_root: closed.before_state_root.clone(),
            after_state_root: closed.after_state_root.clone(),
            action_root: h(52),
            event_root: h(53),
            result_root: closed.roots[&ArtifactRole::Result].clone(),
            projection_root: h(54),
            artifact_roots: closed.roots.clone(),
            handoff: None,
        };
        let candidate = SignedCanonicalCandidateV1 {
            writer_signature_hex: signed(canonical_candidate_signing_bytes(&record).unwrap(), 4),
            record,
        };
        Self {
            path,
            pdir,
            rdir,
            provider,
            authority,
            initial: FormalCandidateMaterialV1 {
                candidate,
                material: closed,
                frozen: None,
                frozen_material: None,
                branch: FormalCompletionBranchV1::Normal,
            },
        }
    }
    fn endpoint(&self, role: EndpointRole) -> FileFormalEndpoint {
        FileFormalEndpoint::open_inner(
            if role == EndpointRole::Primary {
                &self.pdir
            } else {
                &self.rdir
            },
            FormalEndpointAuthorizationV1 {
                authority: self.authority.clone(),
                original: self.authority.clone(),
                recovery: None,
                parent: None,
            },
            role,
            key(if role == EndpointRole::Primary { 5 } else { 6 }),
            self.provider.clone(),
            true,
            None,
        )
        .unwrap()
    }
    fn coordinator(&self) -> FormalCoordinator<FileFormalEndpoint, FileFormalEndpoint> {
        FormalCoordinator::new(
            self.endpoint(EndpointRole::Primary),
            self.endpoint(EndpointRole::Replica),
            self.authority.clone(),
            self.authority.clone(),
            None,
            None,
        )
        .unwrap()
    }
    fn frozen(
        &self,
        parent: &VerifiedFormalCommitV1,
        n: u8,
    ) -> (SignedFormalFrozenV1, ClosedRecord) {
        let closed = material(
            format!("action-{n}").into_bytes(),
            b"state0".to_vec(),
            b"manifest".to_vec(),
        );
        let body = FormalFrozenBodyV1 {
            schema_version: 1,
            world: self.authority.definition().world.clone(),
            authority_id: self.authority.authority_id().unwrap(),
            epoch: 1,
            activation_ref: self.authority.activation_ref().into(),
            parent: parent.head(),
            next_position: parent.head().position + 1,
            request_digest: h(n),
            payload_digest: closed.payload_digest.clone(),
            runtime_manifest_digest: closed.roots[&ArtifactRole::ExecutionManifest].clone(),
            governing_manifest_digest: h(80),
            execution_policy_digest: h(50),
            pre_state_root: closed.before_state_root.clone(),
            frozen_material_digest: frozen_material_digest(&closed).unwrap(),
            handoff: None,
        };
        (
            SignedFormalFrozenV1 {
                signature_hex: signed(frozen_signing_bytes(&body).unwrap(), 4),
                body,
            },
            closed,
        )
    }
    fn candidate(&self, token: &ExecutableFormalFrozenV1) -> FormalCandidateMaterialV1 {
        let frozen = token.certificate().clone();
        let body = &frozen.reservation.body;
        let original_input = token
            .material()
            .objects
            .iter()
            .find(|object| object.content_hash == body.payload_digest)
            .unwrap()
            .bytes
            .clone();
        let closed = material(original_input, b"state0".to_vec(), b"manifest".to_vec());
        // Payload identity stays the original action bytes; all roots are closed.
        let mut closed = closed;
        closed.payload_digest = body.payload_digest.clone();
        let record = CanonicalRecordV1 {
            schema_version: 1,
            durable_set_transition_digest: None,
            kind: FormalRecordKindV1::Execution,
            world: body.world.clone(),
            position: body.next_position,
            parent_record_id: body.parent.record_id.clone(),
            original_authority_id: body.authority_id.clone(),
            original_epoch: 1,
            activation_ref: body.activation_ref.clone(),
            request_digest: body.request_digest.clone(),
            payload_digest: body.payload_digest.clone(),
            frozen_reservation_id: Some(body.reservation_id().unwrap()),
            runtime_manifest_digest: body.runtime_manifest_digest.clone(),
            governing_manifest_digest: body.governing_manifest_digest.clone(),
            execution_policy_digest: body.execution_policy_digest.clone(),
            before_state_root: body.pre_state_root.clone(),
            after_state_root: closed.after_state_root.clone(),
            action_root: h(52),
            event_root: h(53),
            result_root: closed.roots[&ArtifactRole::Result].clone(),
            projection_root: h(54),
            artifact_roots: closed.roots.clone(),
            handoff: None,
        };
        FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                writer_signature_hex: signed(
                    canonical_candidate_signing_bytes(&record).unwrap(),
                    4,
                ),
                record,
            },
            material: closed,
            frozen: Some(frozen),
            frozen_material: Some(token.material().clone()),
            branch: FormalCompletionBranchV1::Normal,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn native_formal_dual_material_qualification_and_permanent_original_query() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let initial = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&initial, &fixture.authority, None, None).unwrap();
    let (input, bytes) = fixture.frozen(&parent, 60);
    let token = coordinator.freeze(input.clone(), bytes.clone()).unwrap();
    let candidate = fixture.candidate(&token);
    let proof = coordinator.submit(candidate.clone()).unwrap();
    assert_eq!(coordinator.submit(candidate.clone()).unwrap(), proof);
    assert_eq!(
        proof.decision.record.record_id().unwrap(),
        candidate.candidate.record.record_id().unwrap()
    );
    let result = coordinator.query(&input.body.request_digest).unwrap();
    assert!(
        matches!(result.0,FormalQueryOutcomeV1::Recorded(entry) if matches!(*entry,FormalRequestStateV1::Qualified {..}))
    );
    drop(coordinator);
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    assert_eq!(primary.head().unwrap().position, 2);
    assert!(
        matches!(primary.query(&input.body.request_digest).unwrap(),FormalQueryOutcomeV1::Recorded(entry) if matches!(*entry,FormalRequestStateV1::Qualified {..}))
    );
    let mut changed = input;
    changed.body.payload_digest = h(99);
    assert!(primary.freeze(changed, bytes).is_err());
}

#[test]
fn native_formal_long_history_minimum_and_corruption_fail_closed() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let initial = coordinator.submit(fixture.initial.clone()).unwrap();
    let mut parent = verify_formal_qualified(&initial, &fixture.authority, None, None).unwrap();
    let initial_head = parent.head();
    for request in 60..126 {
        let (input, bytes) = fixture.frozen(&parent, request);
        let token = coordinator.freeze(input, bytes).unwrap();
        let proof = coordinator.submit(fixture.candidate(&token)).unwrap();
        parent = verify_formal_qualified(&proof, &fixture.authority, Some(&parent), None).unwrap();
    }
    drop(coordinator);
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    assert_eq!(primary.head().unwrap().position, 67);
    primary.verify_minimum(&initial_head).unwrap();
    let mut fork = initial_head;
    fork.record_id = h(200);
    assert!(primary.verify_minimum(&fork).is_err());
    assert!(
        matches!(primary.query(&h(60)).unwrap(), FormalQueryOutcomeV1::Recorded(entry) if matches!(*entry,FormalRequestStateV1::Qualified {..}))
    );
    assert!(serde_cbor::to_vec(&primary.store.body).unwrap().len() < 16 * 1024);
    let id = primary.store.lookup(&h(60)).unwrap().unwrap();
    fs::write(
        primary.store.path.join("formal-objects-v1").join(id),
        b"corrupt",
    )
    .unwrap();
    assert!(primary.query(&h(60)).is_err());
    assert!(primary.head().is_err());
}

#[test]
fn native_formal_monotonic_intent_reopen_and_single_slot() {
    for phase in [1, 2] {
        let fixture = Fixture::new();
        let mut primary = fixture.endpoint(EndpointRole::Primary);
        primary.store.fault = Some(phase);
        assert!(primary.prepare(fixture.initial.clone()).is_err());
        assert!(primary.head().is_err());
        drop(primary);
        let mut primary = fixture.endpoint(EndpointRole::Primary);
        let receipt = primary.prepare(fixture.initial.clone()).unwrap();
        assert_eq!(
            receipt.body.final_receipt.record_id,
            fixture.initial.candidate.record.record_id().unwrap()
        );
        let mut competing = fixture.initial.clone();
        competing.candidate.record.request_digest = h(88);
        competing.candidate.writer_signature_hex = signed(
            canonical_candidate_signing_bytes(&competing.candidate.record).unwrap(),
            4,
        );
        assert!(primary.prepare(competing).is_err());
    }
}

#[test]
fn native_formal_controlled_provider_cannot_open_production_writer() {
    let fixture = Fixture::new();
    assert!(matches!(
        FileFormalEndpoint::open(
            &fixture.pdir,
            FormalEndpointAuthorizationV1 {
                authority: fixture.authority.clone(),
                original: fixture.authority.clone(),
                recovery: None,
                parent: None
            },
            EndpointRole::Primary,
            key(5),
            fixture.provider.clone()
        ),
        Err(FormalError::FencingUnproven)
    ));
}
#[test]
fn native_formal_decision_is_not_qualified_until_dual_proof_complete() {
    let fixture = Fixture::new();
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    let mut replica = fixture.endpoint(EndpointRole::Replica);
    let p = primary.prepare(fixture.initial.clone()).unwrap();
    let r = replica.prepare(fixture.initial.clone()).unwrap();
    let request = &fixture.initial.candidate.record.request_digest;
    let primary_receipt = primary.decide(request, p.clone(), r.clone()).unwrap();
    let replica_receipt = replica.decide(request, p, r).unwrap();
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: fixture.initial.candidate.record.clone(),
        branch: FormalCompletionBranchV1::Normal,
        writer_signature_hex: fixture.initial.candidate.writer_signature_hex.clone(),
        frozen: None,
        primary: primary_receipt,
        replica: replica_receipt,
    };
    verify_formal_decision(&decision, &fixture.authority, None, None).unwrap();
    let p = primary.install_proof(decision.clone()).unwrap();
    assert_eq!(primary.head().unwrap().position, 0);
    assert_eq!(replica.head().unwrap().position, 0);
    assert!(
        matches!(primary.query(request).unwrap(),FormalQueryOutcomeV1::Recorded(entry) if matches!(*entry,FormalRequestStateV1::Ready {..}))
    );
    let mut qualified = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision: decision.clone(),
        primary_complete: p.clone(),
        replica_complete: p,
    };
    assert!(verify_formal_qualified(&qualified, &fixture.authority, None, None).is_err());
    assert!(primary.publish_qualified(qualified.clone()).is_err());
    assert_eq!(primary.head().unwrap().position, 0);
    qualified.replica_complete = replica.install_proof(decision).unwrap();
    let verified = verify_formal_qualified(&qualified, &fixture.authority, None, None).unwrap();
    assert_eq!(
        primary.publish_qualified(qualified.clone()).unwrap(),
        verified.head()
    );
    assert_eq!(
        replica.publish_qualified(qualified).unwrap(),
        verified.head()
    );
}

struct LostQualification(FileFormalEndpoint);
impl FormalDurabilityEndpoint for LostQualification {
    fn install_recovery_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<(), FormalError> {
        self.0.install_recovery_frozen(certificate, material)
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        self.0.head()
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        self.0.query(request)
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        self.0.freeze(input, material)
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        self.0.complete_frozen(certificate)
    }
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        self.0.prepare(candidate)
    }
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        self.0.decide(request, primary, replica)
    }
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        self.0.install_proof(decision)
    }
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        self.0.publish_qualified(proof)?;
        Err(FormalError::Io("lost replica qualification reply".into()))
    }
}
#[test]
fn native_formal_dual_qualification_lost_reply_returns_unknown_and_original_retry() {
    let fixture = Fixture::new();
    let mut coordinator = FormalCoordinator::new(
        fixture.endpoint(EndpointRole::Primary),
        LostQualification(fixture.endpoint(EndpointRole::Replica)),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        coordinator.submit(fixture.initial.clone()),
        Err(FormalCoordinatorErrorV1::OutcomeUnknown { .. })
    ));
    assert!(matches!(
        coordinator.submit(fixture.initial.clone()),
        Err(FormalCoordinatorErrorV1::OutcomeUnknown { .. })
    ));
    drop(coordinator);
    let mut coordinator = fixture.coordinator();
    let original = coordinator.submit(fixture.initial.clone()).unwrap();
    assert_eq!(original.decision.record, fixture.initial.candidate.record);
    assert_eq!(
        coordinator.endpoints_mut().0.head().unwrap(),
        coordinator.endpoints_mut().1.head().unwrap()
    );
}
#[test]
fn native_formal_seal_rejects_append_and_native_snapshot_rollback() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let initial = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&initial, &fixture.authority, None, None).unwrap();
    let (input, material) = fixture.frozen(&parent, 60);
    let token = coordinator.freeze(input.clone(), material.clone()).unwrap();
    let candidate = fixture.candidate(&token);
    let primary = coordinator.endpoints_mut().0;
    primary.prepare(candidate.clone()).unwrap();
    let old = fs::read(fixture.pdir.join("formal-checkpoint-v1.cbor")).unwrap();
    let checkpoint = primary.checkpoint().unwrap();
    let seal = primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    assert_eq!(
        seal.body.highest_complete.decision_hash,
        parent.head().record_id
    );
    assert_eq!(
        seal.body.tail.proposal_digest.as_deref(),
        Some(candidate.candidate.record.record_id().unwrap().as_str())
    );
    assert_eq!(primary.seal_epoch(&h(210), 2, &checkpoint).unwrap(), seal);
    assert!(
        primary
            .decide(
                &input.body.request_digest,
                FormalPrepareReceiptV1 {
                    body: FormalPrepareReceiptBodyV1 {
                        final_receipt: FormalFinalReceiptBodyV1 {
                            schema_version: 1,
                            record_id: h(0),
                            position: 2,
                            finalizing_authority_id: h(0),
                            role: EndpointRole::Primary,
                            instance_digest: h(0),
                            branch: FormalCompletionBranchV1::Normal,
                            recovery_certificate_digest: None
                        },
                        candidate_material_digest: h(0)
                    },
                    signature_hex: h(0)
                },
                FormalPrepareReceiptV1 {
                    body: FormalPrepareReceiptBodyV1 {
                        final_receipt: FormalFinalReceiptBodyV1 {
                            schema_version: 1,
                            record_id: h(0),
                            position: 2,
                            finalizing_authority_id: h(0),
                            role: EndpointRole::Replica,
                            instance_digest: h(0),
                            branch: FormalCompletionBranchV1::Normal,
                            recovery_certificate_digest: None
                        },
                        candidate_material_digest: h(0)
                    },
                    signature_hex: h(0)
                }
            )
            .is_err()
    );
    assert!(matches!(
        fencing::verify_epoch_seal(
            &seal,
            &fencing::TrustedFencingPolicyV1 {
                provider_public_key: public(3),
                authorized_resource_bindings: vec![fixture.provider.resource_binding().into()]
            }
        ),
        Err(fencing::FencingError::FencingUnproven)
    ));
    drop(coordinator);
    fs::write(fixture.pdir.join("formal-checkpoint-v1.cbor"), old).unwrap();
    assert!(
        FileFormalEndpoint::open_inner(
            &fixture.pdir,
            FormalEndpointAuthorizationV1 {
                authority: fixture.authority.clone(),
                original: fixture.authority.clone(),
                recovery: None,
                parent: None
            },
            EndpointRole::Primary,
            key(5),
            fixture.provider.clone(),
            true,
            None,
        )
        .is_err()
    );
}

#[test]
fn native_formal_sealed_index_transfer_preserves_old_context_and_rejects_second_effect() {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let cut = verify_formal_qualified(&proof, &fixture.authority, None, None).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    let receipt = primary.seal_epoch(&h(210), 2, &checkpoint).unwrap();
    // Simulate the external provider's signed custody/floor contract, then
    // obtain opaque evidence through the same verifier used in production.
    let resource_binding = fixture.provider.resource_binding().to_owned();
    let signed_seal = oasis7_formal_protocol::controlled_authority::formal::test_fixtures::signed_epoch_seal_receipt(
        receipt.body, &key(5), &key(3), resource_binding.clone(),
    );
    let seal = fencing::verify_epoch_seal(
        &signed_seal,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: public(3),
            authorized_resource_bindings: vec![resource_binding],
        },
    )
    .unwrap();
    let mut transfer = primary.export_sealed_archive(&seal).unwrap();
    let pdir = fixture.path.join("successor-primary");
    let rdir = fixture.path.join("successor-replica");
    fs::create_dir(&pdir).unwrap();
    fs::create_dir(&rdir).unwrap();
    let mut definition = fixture.authority.definition().clone();
    definition.epoch = 2;
    definition.writer_public_key = public(10);
    definition.predecessor_authority_id = Some(fixture.authority.authority_id().unwrap());
    definition.valid_from_position = 2;
    for (binding, dir, n) in [
        (&mut definition.primary, &pdir, 11),
        (&mut definition.replica, &rdir, 12),
    ] {
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("successor-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = fencing::directory_fingerprint(dir).unwrap();
        binding.fault_domain_id = format!("successor-domain-{n}");
    }
    definition.validate().unwrap();
    let coverage = VerifiedAppendFencingCoverageV1::derive_from_necessary_endpoint_seal(
        &fixture.authority,
        &cut,
        &seal,
    )
    .unwrap();
    let intent = VerifiedRecoveryIntent::verify_single_survivor(
        RecoveryCertificateBodyV1 {
            retained_context: None,
            schema_version: 1,
            world: definition.world.clone(),
            operation_id: h(210),
            predecessor_authority_id: fixture.authority.authority_id().unwrap(),
            predecessor_epoch: 1,
            sealed_cut: cut.head(),
            tail: RecoveryTailV1::Empty,
            fencing_evidence_digest: coverage.evidence_digest().unwrap(),
            successor: definition,
            first_normal_position: Some(2),
        },
        &fixture.authority,
        &cut,
        &coverage,
        &RecoveryTailWitnessV1::Empty,
    )
    .unwrap();
    let recovery =
        oasis7_formal_protocol::controlled_authority::formal::test_fixtures::verified_recovery(
            &intent,
            &fixture.authority,
        );
    let successor = recovery.successor().clone();
    // Candidate import must reject a different cut before installing any state.
    let exact_cut = transfer.cut.clone();
    transfer.cut.record_id = h(249);
    assert!(
        FileFormalEndpoint::open_inner(
            &pdir,
            FormalEndpointAuthorizationV1 {
                authority: successor.clone(),
                original: fixture.authority.clone(),
                recovery: Some(recovery.clone()),
                parent: Some(cut.clone()),
            },
            EndpointRole::Primary,
            key(11),
            fixture.provider.clone(),
            true,
            Some(&transfer),
        )
        .is_err()
    );
    transfer.cut = exact_cut;
    assert_eq!(primary.head().unwrap(), cut.head());

    struct UnavailableArchive;
    impl FormalArchiveSourceV1 for UnavailableArchive {
        fn read_object(&self, _: &str) -> Result<Option<Vec<u8>>, FormalError> {
            Ok(None)
        }
    }
    let original_source = transfer.source.clone();
    transfer.source = Arc::new(UnavailableArchive);
    assert!(
        FileFormalEndpoint::open_inner(
            &pdir,
            FormalEndpointAuthorizationV1 {
                authority: successor.clone(),
                original: fixture.authority.clone(),
                recovery: Some(recovery.clone()),
                parent: Some(cut.clone()),
            },
            EndpointRole::Primary,
            key(11),
            fixture.provider.clone(),
            true,
            Some(&transfer),
        )
        .is_err()
    );
    transfer.source = original_source;

    let mut new_primary = FileFormalEndpoint::open_inner(
        &pdir,
        FormalEndpointAuthorizationV1 {
            authority: successor.clone(),
            original: fixture.authority.clone(),
            recovery: Some(recovery.clone()),
            parent: Some(cut.clone()),
        },
        EndpointRole::Primary,
        key(11),
        fixture.provider.clone(),
        true,
        Some(&transfer),
    )
    .unwrap();
    new_primary.verify_minimum(&cut.head()).unwrap();
    let FormalQueryOutcomeV1::Recorded(old) = new_primary
        .query(&fixture.initial.candidate.record.request_digest)
        .unwrap()
    else {
        panic!("lost permanent request");
    };
    let FormalRequestStateV1::Qualified { proof: old, .. } = *old else {
        panic!("lost original qualification");
    };
    assert_eq!(old, proof);
    assert_eq!(
        old.decision.primary.body.finalizing_authority_id,
        fixture.authority.authority_id().unwrap()
    );
    let (mut input, material) = fixture.frozen(&cut, 51);
    input.body.authority_id = successor.authority_id().unwrap();
    input.body.epoch = 2;
    input.body.activation_ref = successor.activation_ref().into();
    input.signature_hex = signed(frozen_signing_bytes(&input.body).unwrap(), 10);
    assert!(matches!(
        new_primary.freeze(input, material),
        Err(FormalError::Conflict(_))
    ));
    let expected = new_primary.head().unwrap();
    drop(new_primary);
    let mut reopened = FileFormalEndpoint::open_inner(
        &pdir,
        FormalEndpointAuthorizationV1 {
            authority: successor,
            original: fixture.authority.clone(),
            recovery: Some(recovery),
            parent: Some(cut),
        },
        EndpointRole::Primary,
        key(11),
        fixture.provider.clone(),
        true,
        Some(&transfer),
    )
    .unwrap();
    assert_eq!(reopened.head().unwrap(), expected);
    // Removing a sealed source object cannot turn an old request into absence.
    let id = reopened.store.lookup(&h(51)).unwrap().unwrap();
    let canonical = fixture.pdir.join("formal-objects-v1").join(&id);
    let actual = if canonical.exists() {
        canonical
    } else {
        fixture
            .pdir
            .join("formal-objects-v1")
            .join(".representation-v1")
            .join(&id)
    };
    fs::remove_file(actual).unwrap();
    assert!(reopened.query(&h(51)).is_err());
}

#[path = "endpoint_protected_tests.rs"]
mod protected_tests;

#[test]
fn known_qualified_restores_exact_current_parent_and_never_regresses_historical_retry() {
    let fixture = Fixture::new();
    let first = fixture
        .coordinator()
        .submit(fixture.initial.clone())
        .unwrap();
    let first_verified = verify_formal_qualified(&first, &fixture.authority, None, None).unwrap();
    let mut restarted = fixture.coordinator();
    assert_eq!(restarted.submit(fixture.initial.clone()).unwrap(), first);
    let (frozen, material) = fixture.frozen(&first_verified, 41);
    let executable = restarted.freeze(frozen, material).unwrap();
    let second = restarted.submit(fixture.candidate(&executable)).unwrap();
    // This historical query succeeds, without resetting H+2 session to H+1.
    assert_eq!(restarted.submit(fixture.initial.clone()).unwrap(), first);
    let second_verified =
        verify_formal_qualified(&second, &fixture.authority, Some(&first_verified), None).unwrap();
    let (frozen, material) = fixture.frozen(&second_verified, 42);
    assert!(restarted.freeze(frozen, material).is_ok());
}

#[test]
fn thin_parent_cannot_initialize_historyless_nonzero_native_head() {
    let fixture = Fixture::new();
    let first = fixture
        .coordinator()
        .submit(fixture.initial.clone())
        .unwrap();
    let verified = verify_formal_qualified(&first, &fixture.authority, None, None).unwrap();
    let (initial, _) = protected_signer::initial_hot(
        &fixture.authority,
        &fixture.authority,
        &None,
        &Some(verified),
        &fixture.authority.definition().primary.instance,
        None,
    )
    .unwrap();
    assert_eq!(
        initial.head,
        fixture.authority.definition().world.genesis_head().unwrap()
    );
    assert!(initial.base_parent.is_none());
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    let before = primary.checkpoint().unwrap();
    let mut guard = primary.guard().unwrap();
    let mut historyless = primary.store.body.clone();
    historyless.index = None;
    historyless.head_node = None;
    assert!(primary.publish_native(historyless, guard.as_mut()).is_err());
    assert_eq!(primary.store.checkpoint, before);
}

#[path = "endpoint_business_tests.rs"]
mod business_tests;

#[path = "endpoint_business_history_tests.rs"]
mod business_history_tests;
#[path = "endpoint_committed_history_tests.rs"]
mod committed_history_tests;
#[path = "endpoint_long_run_c10_tests.rs"]
mod long_run_c10_tests;

#[path = "endpoint_artifact_cas_tests.rs"]
mod artifact_cas_tests;

#[path = "endpoint_durable_set_copy_graph_tests.rs"]
mod durable_set_copy_graph_tests;

#[test]
fn typed_store_reads_packed_objects_and_reuses_their_logical_identity() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let value = vec![42u64; 128];
    let bytes = serde_cbor::to_vec(&value).unwrap();
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let writes = vec![super::super::business_index::BusinessIndexNodeWriteV1 {
        digest: digest.clone(),
        bytes: bytes.clone(),
    }];
    endpoint.store.persist_business_nodes(&writes).unwrap();
    assert_eq!(endpoint.store.get::<Vec<u64>>(&digest).unwrap(), value);
    assert_eq!(endpoint.store.put(&value).unwrap(), digest);
    assert_eq!(endpoint.store.raw_object(&digest).unwrap().unwrap(), bytes);
    let mut physical =
        fs::read(endpoint.store.path.join("formal-objects-v1").join(&digest)).unwrap();
    *physical.last_mut().unwrap() ^= 1;
    fs::write(
        endpoint.store.path.join("formal-objects-v1").join(&digest),
        physical,
    )
    .unwrap();
    assert!(endpoint.store.get::<Vec<u64>>(&digest).is_err());
}

#[test]
fn marked_shadow_cannot_initialize_a_serving_endpoint() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let initial = endpoint.store.body.clone();
    drop(endpoint);
    let shadow = fixture.path.join("shadow-objects");
    super::super::build_formal_storage_shadow(&fixture.pdir, &shadow).unwrap();
    let result = Store::open(&shadow, initial, &key(5), vec![]);
    assert!(matches!(result, Err(FormalError::Invalid(ref message)) if message.contains("shadow")));
    assert!(!shadow.join("formal-checkpoint-v1.cbor").exists());
}
#[path = "endpoint_durable_set_copy_source_tests.rs"]
mod durable_set_copy_source_tests;

#[test]
fn native_verification_pin_is_consumed_and_missing_chunks_require_fresh_readback() {
    use crate::controlled_authority::formal::native_environment::*;
    let fixture = Fixture::new();
    let mut endpoint = fixture.endpoint(EndpointRole::Primary);
    let bytes = vec![17; 32 * 1024];
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: "test-linux".into(),
        os: "linux".into(),
        arch: "test".into(),
        components: [
            NativeEnvironmentRole::Executable,
            NativeEnvironmentRole::BuildMetadata,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, role)| NativeEnvironmentComponentV1 {
            role,
            name: format!("part-{index}"),
            byte_len: bytes.len() as u64,
            content_digest: digest.clone(),
            chunks: vec![NativeEnvironmentChunkV1 {
                digest: digest.clone(),
                byte_len: bytes.len() as u32,
            }],
        })
        .collect(),
    };
    endpoint
        .stage_native_environment(
            &descriptor,
            &[NativeEnvironmentChunkBytesV1 {
                digest: digest.clone(),
                bytes,
            }],
        )
        .unwrap();
    assert!(
        endpoint
            .advance_native_environment_verification(&descriptor)
            .unwrap()
            .complete
    );
    let directory = fixture.pdir.join("formal-objects-v1");
    assert!(crate::controlled_authority::formal::physical::pin(&directory, true).is_err());
    let mut material = fixture.initial.material.clone();
    let encoded = descriptor.encode().unwrap();
    let id = descriptor.digest().unwrap();
    material
        .roots
        .insert(ArtifactRole::NativeExecutionEnvironment, id.clone());
    material.objects.push(ArtifactObject {
        content_hash: id,
        bytes: encoded,
        references: vec![],
    });
    drop(endpoint.take_native_environment(&material).unwrap());
    assert!(
        crate::controlled_authority::formal::physical::pin(&directory, true)
            .unwrap()
            .is_some()
    );
    assert!(endpoint.take_native_environment(&material).is_err());
    for path in [
        directory.join(&digest),
        directory.join(".representation-v1").join(&digest),
    ] {
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    assert!(
        endpoint
            .advance_native_environment_verification(&descriptor)
            .is_err()
    );
}

#[path = "endpoint_native_environment_tests.rs"]
mod native_tests;
