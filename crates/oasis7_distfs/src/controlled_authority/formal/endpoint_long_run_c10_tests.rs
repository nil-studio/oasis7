//! C10 uses actual immutable records/dual native stores, not inflated counters.
//! World execution/effect semantics are covered separately by runtime tests.
use super::*;
use crate::controlled_authority::formal::business_index::*;
fn replace_journal(material: &mut ClosedRecord, bytes: Vec<u8>) {
    let old = material.roots[&ArtifactRole::Journal].clone();
    let hash = blake3::hash(&bytes).to_hex().to_string();
    material.roots.insert(ArtifactRole::Journal, hash.clone());
    material.objects.retain(|object| object.content_hash != old);
    material.objects.push(ArtifactObject {
        content_hash: hash,
        bytes,
        references: vec![],
    });
    material
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
}
#[test]
#[ignore = "long native durability acceptance; run by formal-live-local-tests.sh in required CI"]
fn native_profile2_c10_over_64_records_and_16mib_reopens_and_retries_original_once() {
    let started = std::time::Instant::now();
    let mut freeze_us = 0u128;
    let mut delta_us = 0u128;
    let mut material_us = 0u128;
    let mut submit_us = 0u128;
    let mut verify_us = 0u128;
    let mut reopen_us = 0u128;
    let (fixture, initial_envelope) = super::business_tests::fixture();
    let mut coordinator = fixture.coordinator();
    let initial = coordinator.submit(fixture.initial.clone()).unwrap();
    let initial_commit = verify_formal_qualified(&initial, &fixture.authority, None, None).unwrap();
    let mut parent = initial_commit.clone();
    let mut previous_proof = initial.clone();
    let mut index = initial_envelope.delta.new;
    let mut cumulative_record_bytes = serde_cbor::to_vec(&fixture.initial.material).unwrap().len();
    let mut first_original = None;
    let mut reopen_count = 0usize;
    for request in 130..200u8 {
        let phase = std::time::Instant::now();
        let (input, material) = fixture.frozen(&parent, request);
        let executable = coordinator.freeze(input, material).unwrap();
        freeze_us += phase.elapsed().as_micros();
        let phase = std::time::Instant::now();
        let mut candidate = fixture.candidate(&executable);
        // Distinct, hash-closed bytes actually travel to and persist in both
        // endpoints. No one frame exceeds the established material bounds.
        let mut journal = vec![request; 320 * 1024];
        journal[..8].copy_from_slice(&parent.head().position.to_be_bytes());
        replace_journal(&mut candidate.material, journal);
        let mut updates = vec![
            BusinessIndexLeafV1 {
                key_digest: metadata_key_digest(),
                kind: BusinessIndexLeafKindV1::CommitMetadata,
                canonical_value: vec![request],
            },
            BusinessIndexLeafV1 {
                key_digest: h(request),
                kind: BusinessIndexLeafKindV1::Outcome,
                canonical_value: serde_cbor::to_vec(&(request, parent.head().position + 1))
                    .unwrap(),
            },
            FormalHistoricalCutV1::from_proof(&previous_proof)
                .unwrap()
                .leaf()
                .unwrap(),
        ];
        updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
        material_us += phase.elapsed().as_micros();
        let phase = std::time::Instant::now();
        let delta =
            BusinessIndexDeltaV1::build(index, updates, &coordinator.endpoints_mut().0.store)
                .unwrap();
        delta_us += phase.elapsed().as_micros();
        let phase = std::time::Instant::now();
        index = delta.new.clone();
        let envelope = super::business_tests::envelope(delta);
        super::business_tests::replace_result(&mut candidate.material, envelope.encode().unwrap());
        candidate.candidate.record.artifact_roots = candidate.material.roots.clone();
        candidate.candidate.record.result_root = envelope.delta.new_root;
        candidate.candidate.writer_signature_hex = signed(
            canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
            4,
        );
        verify_record_material(&candidate.candidate.record, &candidate.material).unwrap();
        let bytes = serde_cbor::to_vec(&candidate.material).unwrap().len();
        assert!(bytes < 64 * 1024 * 1024);
        cumulative_record_bytes = cumulative_record_bytes.checked_add(bytes).unwrap();
        material_us += phase.elapsed().as_micros();
        let phase = std::time::Instant::now();
        let proof = coordinator.submit(candidate.clone()).unwrap();
        submit_us += phase.elapsed().as_micros();
        let phase = std::time::Instant::now();
        parent = verify_formal_qualified(&proof, &fixture.authority, Some(&parent), None).unwrap();
        if first_original.is_none() {
            first_original = Some((candidate, proof.clone()));
        }
        previous_proof = proof;
        let (primary, replica) = coordinator.endpoints_mut();
        assert!(primary.store.body.pending.is_none() && replica.store.body.pending.is_none());
        assert!(serde_cbor::to_vec(&primary.store.body).unwrap().len() < 16 * 1024);
        assert!(serde_cbor::to_vec(&replica.store.body).unwrap().len() < 16 * 1024);
        verify_us += phase.elapsed().as_micros();
        // Prune all writer-side per-record temporaries and reopen the genuine
        // bounded checkpoints repeatedly while immutable evidence stays in CAS.
        if (request - 129) % 17 == 0 {
            let phase = std::time::Instant::now();
            drop(coordinator);
            coordinator = FormalCoordinator::new(
                fixture.endpoint(EndpointRole::Primary),
                fixture.endpoint(EndpointRole::Replica),
                fixture.authority.clone(),
                fixture.authority.clone(),
                None,
                Some(parent.clone()),
            )
            .unwrap();
            reopen_count += 1;
            assert_eq!(coordinator.endpoints_mut().0.head().unwrap(), parent.head());
            reopen_us += phase.elapsed().as_micros();
        }
        if (request - 129) % 10 == 0 {
            eprintln!(
                "C10 native timing: requests={} freeze_us={freeze_us} delta_us={delta_us} material_us={material_us} submit_us={submit_us} verify_us={verify_us} reopen_us={reopen_us} total_us={}",
                request - 129,
                started.elapsed().as_micros()
            );
        }
    }
    assert_eq!(parent.head().position, 71);
    assert!(parent.head().position > 64);
    assert!(cumulative_record_bytes > 16 * 1024 * 1024);
    assert!(reopen_count >= 4);
    let (original, original_proof) = first_original.unwrap();
    let phase = std::time::Instant::now();
    drop(coordinator);
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    let mut replica = fixture.endpoint(EndpointRole::Replica);
    primary.verify_minimum(&initial_commit.head()).unwrap();
    replica.verify_minimum(&initial_commit.head()).unwrap();
    for endpoint in [&mut primary, &mut replica] {
        let FormalQueryOutcomeV1::Recorded(state) = endpoint
            .query(&original.candidate.record.request_digest)
            .unwrap()
        else {
            panic!("permanent original missing");
        };
        let FormalRequestStateV1::Qualified { candidate, proof } = *state else {
            panic!("original no longer Qualified");
        };
        assert_eq!(proof, original_proof);
        assert!(candidate == original);
        verify_record_material(&proof.decision.record, &candidate.material).unwrap();
        assert_eq!(
            verify_formal_qualified(&proof, &fixture.authority, Some(&initial_commit), None)
                .unwrap()
                .head()
                .position,
            2
        );
    }
    let head = parent.head();
    let primary_checkpoint = primary.store.checkpoint.clone();
    let replica_checkpoint = replica.store.checkpoint.clone();
    let mut coordinator = FormalCoordinator::new(
        primary,
        replica,
        fixture.authority.clone(),
        fixture.authority.clone(),
        None,
        Some(parent),
    )
    .unwrap();
    assert_eq!(
        coordinator.submit(original.clone()).unwrap(),
        original_proof
    );
    assert_eq!(coordinator.submit(original).unwrap(), original_proof);
    let (primary, replica) = coordinator.endpoints_mut();
    assert_eq!(primary.head().unwrap(), head);
    assert_eq!(replica.head().unwrap(), head);
    assert_eq!(primary.store.checkpoint, primary_checkpoint);
    assert_eq!(replica.store.checkpoint, replica_checkpoint);
    assert!(primary.store.body.pending.is_none() && replica.store.body.pending.is_none());
    eprintln!(
        "C10 native evidence: QualifiedRecords=71 cumulativeRecordBytes={cumulative_record_bytes} reopens={} originalPosition=2 currentPosition=71",
        reopen_count + 1
    );
    eprintln!(
        "C10 native timing: requests=70 freeze_us={freeze_us} delta_us={delta_us} material_us={material_us} submit_us={submit_us} verify_us={verify_us} reopen_us={reopen_us} original_us={} total_us={}",
        phase.elapsed().as_micros(),
        started.elapsed().as_micros()
    );
}
