//! Profile2 epoch recovery uses the actual protected endpoint executables.
//! Loopback custody and issuer keys are isolated software fixtures.
use super::*;
#[path = "endpoint_recovery_issuer_seam_tests.rs"]
mod issuer_seams;
pub(super) use issuer_seams::issue_after_interruptions;
#[path = "endpoint_recovery_writer_seam_tests.rs"]
mod writer_seams;
pub(super) use writer_seams::WriterProcess;
#[path = "endpoint_recovery_control_tests.rs"]
mod control_seams;
pub(super) use control_seams::{control_candidate, publish_one};

pub(super) fn persist_plan(
    fixture: &Fixture,
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    tail: &RecoveryTailWitnessV1,
    body: &RecoveryCertificateBodyV1,
    pending: &RecoveryPendingHandoffV1,
    authorization: RecoveryPlanAuthorityV1,
) -> SignedRecoveryPlanV1 {
    use crate::controlled_authority::formal::issuer::*;
    let checked = VerifiedRecoveryPlanV1::verify(
        RecoveryPlanBodyV1::new(body.clone(), authority, cut, tail, pending, authorization)
            .unwrap(),
        authority,
        cut,
        tail,
        pending,
    )
    .unwrap();
    let path = fixture.path.join("original-recovery-issuer");
    let minimum = IssuerAnchorV1::initial(authority.policy()).unwrap();
    let mut issuer =
        RecoveryIssuer::open(&path, authority.policy().clone(), key(1), minimum.clone()).unwrap();
    let plan = issuer.prepare_plan(&checked).unwrap();
    drop(issuer);
    let mut cold =
        RecoveryIssuer::open(&path, authority.policy().clone(), key(1), minimum).unwrap();
    assert_eq!(cold.prepare_plan(&checked).unwrap(), plan);
    plan
}

pub(super) fn business_candidate(
    fixture: &Fixture,
    candidate: &mut FormalCandidateMaterialV1,
    parent: &FormalQualifiedCommitProofV1,
    base: &FormalBusinessResultEnvelopeV1,
    sources: &[&Path],
    signer: u8,
) -> FormalBusinessResultEnvelopeV1 {
    struct Reader(Vec<FileFormalArchiveSourceV1>);
    impl BusinessIndexNodeReader for Reader {
        fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
            for source in &self.0 {
                if let Some(bytes) = source.read_object(id).map_err(|error| error.to_string())? {
                    return Ok(Some(bytes));
                }
            }
            Ok(None)
        }
    }
    assert_eq!(fixture.authority.definition().profile_version, 2);
    let reader = Reader(
        sources
            .iter()
            .map(|path| FileFormalArchiveSourceV1::new(path.to_path_buf()).unwrap())
            .collect(),
    );
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: serde_cbor::to_vec(&candidate.candidate.record.position).unwrap(),
        },
        BusinessIndexLeafV1 {
            key_digest: candidate.candidate.record.request_digest.clone(),
            kind: BusinessIndexLeafKindV1::Outcome,
            canonical_value: serde_cbor::to_vec(&(
                &candidate.candidate.record.request_digest,
                candidate.candidate.record.position,
            ))
            .unwrap(),
        },
        FormalHistoricalCutV1::from_proof(parent)
            .unwrap()
            .leaf()
            .unwrap(),
    ];
    updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    let delta = BusinessIndexDeltaV1::build(base.delta.new.clone(), updates, &reader).unwrap();
    let envelope = super::super::super::business_tests::envelope(delta);
    super::super::super::business_tests::replace_result(
        &mut candidate.material,
        envelope.encode().unwrap(),
    );
    candidate.candidate.record.artifact_roots = candidate.material.roots.clone();
    candidate.candidate.record.result_root = envelope.delta.new_root.clone();
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        signer,
    );
    envelope
}

pub(super) fn first_seal_binds_original_successor() {
    let binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN").unwrap();
    let (fixture, _) = super::super::super::business_tests::fixture();
    assert_eq!(fixture.authority.definition().profile_version, 2);
    let po = Oracle::start(&fixture, EndpointRole::Primary);
    let ro = Oracle::start(&fixture, EndpointRole::Replica);
    let auth = fixture.path.join("oracle-auth");
    let (_, _, configs) = original_configs(&fixture, &po, &ro, &auth);
    let (mut pchild, paddr) = start(&binary, &configs[0]);
    let (mut rchild, raddr) = start(&binary, &configs[1]);
    let mut writer = WriterProcess::start(&fixture, &binary, &fixture.authority, 4, None);
    assert_eq!(
        writer
            .normal
            .sign_candidate(&fixture.initial.candidate.record, None)
            .unwrap(),
        fixture.initial.candidate.writer_signature_hex
    );
    let genesis = fixture.authority.definition().world.genesis_head().unwrap();
    let mut coordinator = FormalCoordinator::new(
        writer.normal_endpoint(paddr, EndpointRole::Primary, genesis.clone()),
        writer.normal_endpoint(raddr, EndpointRole::Replica, genesis),
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        None,
    )
    .unwrap();
    let proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let cut = verify_formal_qualified(&proof, &fixture.authority, None, None).unwrap();
    drop(coordinator);
    let operation = h(240);
    let mut successor = fixture.authority.definition().clone();
    successor.epoch = 2;
    successor.writer_public_key = public(10);
    successor.predecessor_authority_id = Some(fixture.authority.authority_id().unwrap());
    successor.valid_from_position = cut.head().position + 1;
    for (binding, n) in [(&mut successor.primary, 11), (&mut successor.replica, 12)] {
        let directory = fixture.path.join(format!("planned-successor-{n}"));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("planned-successor-{n}");
        binding.instance.endpoint_public_key = public(n);
        binding.instance.instance_id = h(n + 20);
        binding.instance.storage_id = h(n + 30);
        binding.instance.storage_fingerprint = directory_fingerprint(&directory).unwrap();
        binding.fault_domain_id = format!("planned-successor-domain-{n}");
    }
    let template = RecoveryCertificateBodyV1 {
        schema_version: 1,
        retained_context: None,
        world: successor.world.clone(),
        operation_id: operation.clone(),
        predecessor_authority_id: fixture.authority.authority_id().unwrap(),
        predecessor_epoch: 1,
        sealed_cut: cut.head(),
        tail: RecoveryTailV1::Empty,
        fencing_evidence_digest: h(0),
        successor,
        first_normal_position: Some(cut.head().position + 1),
    };
    let pending = RecoveryPendingHandoffV1::empty();
    let fixed = RecoveryTailWitnessV1::Empty;
    let plan = persist_plan(
        &fixture,
        &fixture.authority,
        &cut,
        &fixed,
        &template,
        &pending,
        writer.plan_authorization(),
    );
    let ack = writer.persist_plan(&plan, &fixed, None, &pending);
    assert!(
        writer
            .normal
            .sign_candidate(&fixture.initial.candidate.record, None)
            .is_err(),
        "old normal credential must reach the actual writer custody refusal"
    );
    let mut p = writer.endpoint(paddr, EndpointRole::Primary, cut.head());
    let mut r = writer.endpoint(raddr, EndpointRole::Replica, cut.head());
    let pc = p.checkpoint().unwrap();
    let mut raw = remote(
        paddr,
        &fixture.authority,
        EndpointRole::Primary,
        4,
        cut.head(),
    );
    assert!(
        raw.seal_epoch(operation.clone(), 2, pc.clone()).is_err(),
        "new profile2 seal cannot infer an absent plan"
    );
    p.prepare_recovery_plan(plan.clone(), pending.clone(), ack.clone(), pc)
        .unwrap();
    let rc = r.checkpoint().unwrap();
    r.prepare_recovery_plan(plan.clone(), pending.clone(), ack, rc)
        .unwrap();
    recovery_credential_configs(&fixture, &configs);
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    rchild.kill().unwrap();
    rchild.wait().unwrap();
    let (mut pchild, paddr) = start(&binary, &configs[0]);
    let (_rchild, raddr) = start(&binary, &configs[1]);
    let mut p = writer.endpoint(paddr, EndpointRole::Primary, cut.head());
    let mut r = writer.endpoint(raddr, EndpointRole::Replica, cut.head());
    assert_eq!(p.recovery_pending_handoff().unwrap(), pending);
    let drained = p.recovery_drained_proof().unwrap();
    assert_eq!(drained, r.recovery_drained_proof().unwrap());
    assert_eq!(drained.qualified, proof);
    let writer_seal = writer.persist_seal(&plan, &drained);
    p = writer.endpoint(paddr, EndpointRole::Primary, cut.head());
    r = writer.endpoint(raddr, EndpointRole::Replica, cut.head());
    let pc = p.checkpoint().unwrap();
    let first_seal = p.seal_planned_epoch(writer_seal.clone(), pc).unwrap();
    pchild.kill().unwrap();
    pchild.wait().unwrap();
    let (_cold, cold_address) = start(&binary, &configs[0]);
    p = writer.endpoint(cold_address, EndpointRole::Primary, cut.head());
    let cold_checkpoint = p.checkpoint().unwrap();
    assert_eq!(
        p.seal_planned_epoch(writer_seal.clone(), cold_checkpoint)
            .unwrap()
            .body,
        first_seal.body
    );
    let rc = r.checkpoint().unwrap();
    let second_seal = r.seal_planned_epoch(writer_seal, rc).unwrap();
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let primary = verify_epoch_seal(&first_seal, &policy).unwrap();
    let replica = verify_epoch_seal(&second_seal, &policy).unwrap();
    let mut body = plan.body.intent.clone();
    body.schema_version = 3;
    body.fencing_evidence_digest = recovery_fencing_digest([&primary, &replica]).unwrap();
    let original = VerifiedRecoveryIntent::verify(
        body.clone(),
        &fixture.authority,
        &cut,
        [&primary, &replica],
        &fixed,
    )
    .unwrap();
    assert_eq!(original.body(), &body);
    let mut replacement = body;
    replacement.successor.writer_public_key = public(14);
    replacement.successor.primary.instance.endpoint_public_key = public(15);
    replacement.successor.primary.instance.instance_id = h(46);
    assert!(
        VerifiedRecoveryIntent::verify(
            replacement,
            &fixture.authority,
            &cut,
            [&primary, &replica],
            &fixed
        )
        .is_err(),
        "CA-F2: a real profile2 first seal and cold reopen must not authorize replacement successor bindings under the original operation"
    );
}
