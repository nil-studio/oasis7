//! Writer failure tests use in-memory signers, never qualify deployment durability.
use super::*;
use crate::formal_read_fixtures as fixtures;
use oasis7_distfs::controlled_authority::{formal::endpoint::*, replicated_protocol::EndpointRole};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
struct Memory {
    authority: VerifiedAuthorityV1,
    parent: VerifiedFormalCommitV1,
    role: EndpointRole,
    entries: BTreeMap<String, FormalRequestStateV1>,
    lose_decide: bool,
    closure_readback: bool,
    closure_pending_pages: usize,
}
#[derive(Clone)]
struct Endpoint(Arc<Mutex<Memory>>);
fn invalid<T>() -> Result<T, FormalError> {
    Err(FormalError::Invalid("writer-only fixture".into()))
}
impl Memory {
    fn n(&self) -> u8 {
        if self.role == EndpointRole::Primary {
            5
        } else {
            6
        }
    }
    fn candidate(&self, request: &str) -> Result<FormalCandidateMaterialV1, FormalError> {
        self.entries
            .get(request)
            .and_then(|e| e.candidate())
            .cloned()
            .ok_or(FormalError::Invalid("fixture candidate missing".into()))
    }
    fn context(
        &self,
        c: &FormalCandidateMaterialV1,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        verify_formal_candidate(
            &c.candidate,
            c.frozen.as_ref(),
            &self.authority,
            Some(&self.parent),
            None,
            c.branch,
        )
    }
}
impl FormalDurabilityEndpoint for Endpoint {
    // Fault fixtures model an explicit independent-reader result. They do not
    // claim filesystem or cross-fault-domain durability.
    fn verify_published_closure(
        &mut self,
        request: &str,
        digest: &str,
    ) -> Result<
        oasis7_distfs::controlled_authority::formal::coordinator::FormalPublishedClosureReceiptV1,
        FormalError,
    > {
        let mut memory = self.0.lock().unwrap();
        if memory.closure_pending_pages > 0 {
            memory.closure_pending_pages -= 1;
            return Err(FormalError::Conflict(
                "published closure verification pending; fixture page".into(),
            ));
        }
        if !memory.closure_readback {
            return invalid();
        }
        let Some(FormalRequestStateV1::Qualified { candidate, proof }) =
            memory.entries.get(request)
        else {
            return invalid();
        };
        verify_record_material(&candidate.candidate.record, &candidate.material)?;
        if super::super::formal_attempt_lifecycle::candidate_digest(candidate)
            .map_err(FormalError::Invalid)?
            != digest
        {
            return invalid();
        }
        Ok(oasis7_distfs::controlled_authority::formal::coordinator::FormalPublishedClosureReceiptV1 {
            candidate_digest: digest.into(), qualified_proof_digest: blake3::hash(&serde_cbor::to_vec(proof).unwrap()).to_hex().to_string(), inventory_digest: digest.into(),
            logical_objects: candidate.material.objects.len() as u64,
            logical_bytes: candidate.material.objects.iter().map(|o| o.bytes.len() as u64).sum(),
        })
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        Ok(self.0.lock().unwrap().parent.head())
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .entries
            .get(request)
            .cloned()
            .map(|e| FormalQueryOutcomeV1::Recorded(Box::new(e)))
            .unwrap_or(FormalQueryOutcomeV1::NotRecorded))
    }
    fn install_recovery_frozen(
        &mut self,
        _: FormalFrozenCertificateV1,
        _: ClosedRecord,
    ) -> Result<(), FormalError> {
        invalid()
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        let mut m = self.0.lock().unwrap();
        let request = input.body.request_digest.clone();
        verify_formal_reservation(&input, &m.authority, &m.parent)?;
        verify_frozen_material(&input.body, &material)?;
        let definition = m.authority.definition();
        let instance = if m.role == EndpointRole::Primary {
            &definition.primary.instance
        } else {
            &definition.replica.instance
        };
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: m.authority.authority_id()?,
            role: m.role,
            instance_digest: instance
                .digest()
                .map_err(|e| FormalError::Invalid(e.to_string()))?,
            reservation_id: input.body.reservation_id()?,
        };
        let receipt = FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body)?, m.n()),
            body,
        };
        if let Some(old) = m.entries.get(&request) {
            let (old_input, old_material) = match old {
                FormalRequestStateV1::Frozen {
                    input, material, ..
                } => (input, material),
                other => {
                    let c = other.candidate().unwrap();
                    (
                        &c.frozen.as_ref().unwrap().reservation,
                        c.frozen_material.as_ref().unwrap(),
                    )
                }
            };
            if old_input != &input || old_material != &material {
                return invalid();
            }
        } else {
            m.entries.insert(
                request,
                FormalRequestStateV1::Frozen {
                    input,
                    material,
                    receipt: Some(receipt.clone()),
                    certificate: None,
                },
            );
        }
        Ok(receipt)
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        let mut m = self.0.lock().unwrap();
        certificate.verify(m.authority.definition(), m.authority.activation_ref())?;
        let request = &certificate.reservation.body.request_digest;
        if let Some(FormalRequestStateV1::Frozen {
            certificate: slot, ..
        }) = m.entries.get_mut(request)
        {
            *slot = Some(certificate);
        }
        Ok(())
    }
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        let mut m = self.0.lock().unwrap();
        let request = candidate.candidate.record.request_digest.clone();
        verify_record_material(&candidate.candidate.record, &candidate.material)?;
        let context = m.context(&candidate)?;
        if let Some(old) = m.entries.get(&request).and_then(|e| e.candidate())
            && old != &candidate
        {
            return invalid();
        }
        let body = FormalPrepareReceiptBodyV1 {
            final_receipt: context.receipt_body(m.role)?,
            candidate_material_digest: formal_candidate_material_digest(&candidate)?,
        };
        let receipt = FormalPrepareReceiptV1 {
            signature_hex: fixtures::sign(formal_prepare_receipt_signing_bytes(&body)?, m.n()),
            body,
        };
        if matches!(
            m.entries.get(&request),
            Some(FormalRequestStateV1::Frozen { .. })
        ) {
            m.entries.insert(
                request,
                FormalRequestStateV1::Prepared {
                    candidate,
                    receipt: Some(receipt.clone()),
                },
            );
        }
        Ok(receipt)
    }
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        let mut m = self.0.lock().unwrap();
        let candidate = m.candidate(request)?;
        let context = m.context(&candidate)?;
        verify_formal_prepare_receipt(&primary, &candidate, &context, EndpointRole::Primary)?;
        verify_formal_prepare_receipt(&replica, &candidate, &context, EndpointRole::Replica)?;
        let body = context.receipt_body(m.role)?;
        let receipt = FormalFinalReceiptV1 {
            signature_hex: fixtures::sign(formal_final_receipt_signing_bytes(&body)?, m.n()),
            body,
        };
        let prepare = if m.role == EndpointRole::Primary {
            primary
        } else {
            replica
        };
        m.entries.insert(
            request.into(),
            FormalRequestStateV1::Decided {
                candidate,
                prepare,
                receipt: receipt.clone(),
            },
        );
        if m.lose_decide {
            m.lose_decide = false;
            return Err(FormalError::Io("fixture reply lost after decision".into()));
        }
        Ok(receipt)
    }
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        let mut m = self.0.lock().unwrap();
        let verified = verify_formal_decision(&decision, &m.authority, Some(&m.parent), None)?;
        let body = verified.complete_receipt_body(m.role)?;
        let receipt = FormalProofCompleteReceiptV1 {
            signature_hex: fixtures::sign(formal_proof_complete_signing_bytes(&body)?, m.n()),
            body,
        };
        let request = decision.record.request_digest.clone();
        let candidate = m.candidate(&request)?;
        let prepare = match m.entries.get(&request).unwrap() {
            FormalRequestStateV1::Decided { prepare, .. }
            | FormalRequestStateV1::Ready { prepare, .. } => prepare.clone(),
            _ => return invalid(),
        };
        m.entries.insert(
            request,
            FormalRequestStateV1::Ready {
                candidate,
                prepare,
                decision,
                complete: Some(receipt.clone()),
            },
        );
        Ok(receipt)
    }
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        let mut m = self.0.lock().unwrap();
        let verified = verify_formal_qualified(&proof, &m.authority, Some(&m.parent), None)?;
        let request = proof.decision.record.request_digest.clone();
        let candidate = m.candidate(&request)?;
        m.entries.insert(
            request,
            FormalRequestStateV1::Qualified { candidate, proof },
        );
        m.parent = verified;
        Ok(m.parent.head())
    }
}
fn endpoint(
    anchor: &VerifiedFormalReadAnchorV1,
    role: EndpointRole,
    lose_decide: bool,
) -> Endpoint {
    Endpoint(Arc::new(Mutex::new(Memory {
        authority: anchor.authority().clone(),
        parent: anchor.head().clone(),
        role,
        entries: BTreeMap::new(),
        lose_decide,
        closure_readback: false,
        closure_pending_pages: 0,
    })))
}
fn continuation(world: &World) -> NodeLocalExecutionContinuation {
    NodeLocalExecutionContinuation {
        schema_version: 1,
        context: oasis7_node::NodeExecutionCommitContext {
            world_id: "formal-fixture-world".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 1,
            slot: 1,
            epoch: 1,
            node_block_hash: fixtures::h(90),
            action_root: oasis7_node::compute_consensus_action_root(&[]).unwrap(),
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
fn saved_candidate_reconciles_lost_decision_without_reexecuting_world() {
    let policy = ReleaseSecurityPolicy::default();
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        formal_package::execution_policy_material(&policy)
            .unwrap()
            .2,
    );
    let original = continuation(&world);
    let primary = endpoint(&anchor, EndpointRole::Primary, false);
    let replica = endpoint(&anchor, EndpointRole::Replica, true);
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-formal-writer-{}", std::process::id()));
    assert!(!root.exists());
    let open = || {
        FormalWriter::open(
            root.clone(),
            fixtures::key(4),
            policy.clone(),
            world.clone(),
            anchor.clone(),
            primary.clone(),
            replica.clone(),
        )
    };
    let mut writer = open().unwrap();
    assert!(open().is_err(), "directory ownership must be exclusive");
    writer.prepare(&original).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.join("pending.cbor"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "pending formal protocol state must be private at creation"
        );
    }
    assert_eq!(
        writer.pending_continuation().unwrap(),
        Some(original.clone())
    );
    assert!(
        !root.join("attempts").exists(),
        "Frozen preparation must not execute World"
    );
    assert_eq!(writer.anchor().head().head(), anchor.head().head());
    drop(writer);
    let mut writer = open().unwrap();
    assert_eq!(
        writer.pending_continuation().unwrap(),
        Some(original.clone())
    );
    assert!(
        writer
            .commit(&original)
            .unwrap_err()
            .contains("OutcomeUnknown")
    );
    assert_eq!(writer.anchor().head().head(), anchor.head().head());
    assert_eq!(std::fs::read_dir(root.join("attempts")).unwrap().count(), 1);
    assert!(writer.resume_original().is_err());
    drop(writer);
    let mut writer = open().unwrap();
    let proof = writer.resume_original().unwrap();
    assert_eq!(
        std::fs::read_dir(root.join("attempts")).unwrap().count(),
        1,
        "saved candidate must take priority over reexecution"
    );
    assert_eq!(writer.commit(&original).unwrap(), proof);
    let published_path = root
        .join("published")
        .join(format!("{:020}.cbor", proof.decision.record.position));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&published_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "formal publication proof must be private at creation"
        );
    }
    assert_eq!(
        read_published_proof_at(&root, proof.decision.record.position).unwrap(),
        Some(proof.clone()),
        "a proof written by FormalWriter must pass the strict private reader"
    );
    let (published_candidate, published_proof) =
        read_published_candidate_at(&root, proof.decision.record.position)
            .unwrap()
            .expect("the exact private publication includes its committed material");
    assert_eq!(published_candidate.candidate.record, proof.decision.record);
    assert_eq!(published_proof, proof);
    let view = fixtures::projection(writer.world());
    assert_eq!(
        fixtures::commit_ref(writer.anchor(), &view).state_root_ref,
        proof.decision.record.after_state_root
    );
    let record_id = proof.decision.record.record_id().unwrap();
    writer.acknowledge(&record_id).unwrap();
    drop(writer);
    let writer = open().unwrap();
    assert_eq!(writer.anchor().head().head().record_id, record_id);
    assert_eq!(
        blake3_hex(&to_cbor(writer.world().snapshot()).unwrap()),
        proof.decision.record.after_state_root
    );
    drop(writer);
    std::fs::write(root.join("published/00000000000000000004.cbor"), [0u8]).unwrap();
    assert!(open().err().unwrap().contains("prefix has a gap"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn initial_node_bootstrap_is_bound_by_external_activation_closure() {
    let (world, mut trust, origin, _) = fixtures::actual_world_initial_fixture();
    let mut snapshot = world.snapshot();
    snapshot.state.time = 7;
    let world =
        oasis7::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap();
    trust.initial_state_root = blake3_hex(&to_cbor(world.snapshot()).unwrap());
    let policy = ReleaseSecurityPolicy::default();
    let bootstrap = super::super::formal_initial::InitialNodeBootstrapV1 {
        height: 7,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: blake3_hex(&to_cbor(world.snapshot()).unwrap()),
    };
    let draft = || {
        super::super::formal_initial::InitialWorldClosure::from_existing_world_with_node_bootstrap(
            &world,
            &origin.head().record().world,
            &policy,
            bootstrap.clone(),
        )
        .unwrap()
    };
    let body = draft()
        .activation_body(
            origin.authority().definition().clone(),
            origin.authority().policy().clone(),
        )
        .unwrap();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (authority, candidate) = draft()
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    let proof = fixtures::qualified_proof(
        candidate.candidate.record.clone(),
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    let primary = endpoint(&anchor, EndpointRole::Primary, false);
    let replica = endpoint(&anchor, EndpointRole::Replica, false);
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-formal-initial-writer-{}",
        std::process::id()
    ));
    assert!(!root.exists());
    let writer = FormalWriter::open_initial(
        root.clone(),
        fixtures::key(4),
        policy.clone(),
        anchor,
        candidate,
        proof,
        primary,
        replica,
    )
    .unwrap();
    let observed = writer.committed_initial_bootstrap().unwrap();
    assert_eq!(observed.height, 7);
    assert_eq!(
        observed.consensus_block_hash,
        bootstrap.consensus_block_hash
    );
    assert_eq!(
        observed.execution_state_root,
        bootstrap.execution_state_root
    );
    let mut changed = bootstrap;
    changed.consensus_block_hash = fixtures::h(83);
    let altered =
        super::super::formal_initial::InitialWorldClosure::from_existing_world_with_node_bootstrap(
            &world,
            &origin.head().record().world,
            &policy,
            changed,
        )
        .unwrap();
    assert!(
        altered
            .authorize(&certificate, &trust, &fixtures::key(4))
            .is_err()
    );
    drop(writer);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn exact_position_published_proof_reader_is_private_and_never_scans() {
    use std::os::unix::fs::PermissionsExt;

    let (_world, _trust, _anchor, proof) = fixtures::actual_world_initial_fixture();
    let record = proof.decision.record.clone();
    let publication = Published {
        schema_version: 1,
        candidate: FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record: record.clone(),
                writer_signature_hex: proof.decision.writer_signature_hex.clone(),
            },
            material: ClosedRecord {
                payload_digest: record.payload_digest.clone(),
                before_state_root: record.before_state_root.clone(),
                after_state_root: record.after_state_root.clone(),
                roots: record.artifact_roots.clone(),
                objects: Vec::new(),
            },
            frozen: proof.decision.frozen.clone(),
            frozen_material: None,
            branch: proof.decision.branch,
        },
        proof: proof.clone(),
    };
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-formal-published-proof-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let published = root.join("published");
    std::fs::create_dir_all(&published).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&published, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = published.join(format!("{:020}.cbor", record.position));
    std::fs::write(&path, serde_cbor::to_vec(&publication).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    assert_eq!(
        read_published_proof_at(&root, record.position).unwrap(),
        Some(proof)
    );
    assert!(
        read_published_proof_at(&root, record.position + 1)
            .unwrap()
            .is_none()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[path = "formal_writer_recovery_tests.rs"]
mod recovery_tests;

#[path = "formal_writer_saved_candidate_tests.rs"]
mod saved_candidate_tests;

#[path = "formal_writer_attempt_tests.rs"]
mod attempt_tests;
