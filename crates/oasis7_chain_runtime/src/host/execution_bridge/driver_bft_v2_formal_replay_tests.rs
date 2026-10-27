use super::super::{FormalCandidateContext, execute_formal_candidate};
use super::*;
use oasis7_formal_protocol::bft::{FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1};

#[test]
fn formal_prefix_replay_reexecutes_all_heights_and_rejects_signed_false_outputs() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(replay_case)
        .unwrap()
        .join()
        .unwrap();
}

fn replay_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let execution = open_formal_read(&h, &scratch, h.reader.clone(), true).unwrap();
    let before = execution.parent_world.snapshot();
    let block = execution.build_candidate(Vec::new()).unwrap();
    let mut executed = execution.execute_candidate(&block).unwrap();
    for node in &executed.result_delta.node_writes {
        execution
            .business_reader
            .cache_content_node(&node.digest, &node.bytes)
            .unwrap();
    }
    let mut prefix = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![FormalBftCertifiedEntryV1 {
            certificate: certificate(block, execution.schedule()),
            commitment: executed.commitment.clone(),
        }],
    };
    // Build three actual World transitions. No Snapshot supplied by a provider
    // is used by the replay consumer below.
    for _ in 0..2 {
        let proof = prefix.verify(&execution.handoff).unwrap();
        let schedule = proof.next_height_schedule();
        let mut block = executed.block.clone();
        block.height = schedule.height();
        block.parent_commit_ref = schedule.parent_commit_ref().into();
        block.authority_epoch = schedule.authority_epoch();
        block.active_validator_set_hash = schedule.active_set().set_hash().into();
        block.manifest_hash = schedule.parent_manifest_hash().into();
        executed = execute_formal_candidate(
            FormalCandidateContext {
                handoff: &execution.handoff,
                schedule,
                parent_world: &executed.world,
                parent_result_tree: &executed.result_tree,
                identity: &execution.parent_record.world,
                business_reader: &execution.business_reader,
                prefix: Some(&proof),
            },
            &block,
        )
        .unwrap();
        for node in &executed.result_delta.node_writes {
            execution
                .business_reader
                .cache_content_node(&node.digest, &node.bytes)
                .unwrap();
        }
        prefix.entries.push(FormalBftCertifiedEntryV1 {
            certificate: certificate(executed.block.clone(), schedule),
            commitment: executed.commitment.clone(),
        });
    }
    // Use a separately opened consumer so builder-side cached BFT result
    // nodes cannot mask missing per-height delta retention during replay.
    let replay_scratch = Scratch::new();
    let probe_bytes = b"replay reader isolation probe".to_vec();
    let probe_digest = blake3_hex(&probe_bytes);
    let mut source = h.reader.0.clone();
    source.insert(probe_digest.clone(), probe_bytes.clone());
    let replayer =
        open_formal_read(&h, &replay_scratch, Arc::new(IndexReader(source)), true).unwrap();
    let original_refs = replayer.retained_nodes.recorded_refs().unwrap();
    let id = executed.block.block_id().unwrap();
    let height = executed.block.height;
    let replayed = replayer
        .replay_certified_prefix(&prefix, height, &id)
        .unwrap();
    assert_eq!(replayed.proof.certificate().block_id(), id);
    assert_eq!(replayed.tip.snapshot_bytes, executed.snapshot_bytes);
    assert_eq!(replayed.tip.journal_bytes, executed.journal_bytes);
    assert_eq!(replayed.tip.commitment, executed.commitment);
    assert_eq!(
        replayed.tip.result_tree.root().unwrap(),
        executed.result_tree.root().unwrap()
    );
    assert_eq!(replayed.tip.world.state().time, 5);
    assert!(!original_refs.contains(&probe_digest));
    assert_eq!(
        replayed.dependencies.read_node(&probe_digest).unwrap(),
        Some(probe_bytes)
    );
    assert_eq!(
        replayer.retained_nodes.recorded_refs().unwrap(),
        original_refs,
        "a replay overlay miss must not grow the original H+1 recorder"
    );
    let retained = replayed.dependencies.recorded_nodes().unwrap();
    for node in &executed.result_delta.node_writes {
        assert!(
            retained
                .iter()
                .any(|(digest, bytes)| digest == &node.digest && bytes == &node.bytes)
        );
    }
    assert_eq!(replayer.parent_world.snapshot(), before);
    assert!(replayer.ledger.record().is_none());
    assert_eq!(
        replayer.retained_nodes.recorded_refs().unwrap(),
        original_refs
    );
    assert_eq!(execution.parent_world.snapshot(), before);
    assert!(
        execution.ledger.record().is_none(),
        "replay must not publish H+1 or successors"
    );
    assert!(
        execution
            .replay_certified_prefix(&prefix, height - 1, &id)
            .is_err()
    );
    assert!(
        execution
            .replay_certified_prefix(&prefix, height, &"ff".repeat(32))
            .is_err()
    );

    // A valid final QC can certify a fabricated result tree. Protocol proof
    // accepts it, but full replay must recompute the actual result root.
    let preceding = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: prefix.entries[..2].to_vec(),
    }
    .verify(&execution.handoff)
    .unwrap();
    let mut forged = prefix.clone();
    let last = forged.entries.last_mut().unwrap();
    last.commitment.result_root = "a1".repeat(32);
    let mut block = last.certificate.proposal.block.clone();
    block.execution_block_hash = last.commitment.digest().unwrap();
    last.certificate = certificate(block, preceding.next_height_schedule());
    let forged_tip = forged.verify(&execution.handoff).unwrap();
    let error = replayer
        .replay_certified_prefix(&forged, height, forged_tip.certificate().block_id())
        .err()
        .expect("valid signatures cannot stand in for execution");
    assert!(error.contains("does not reproduce"), "{error}");
    assert_eq!(execution.parent_world.snapshot(), before);
    assert!(execution.ledger.record().is_none());

    // Persist/reopen the old H+1 ledger, whose dependency reader is strict.
    // New replay deltas must remain in their own overlay and must not change
    // that existing durable cut or its exact dependency set.
    drop(replayer);
    let mut installer = open_formal_read(&h, &replay_scratch, h.reader.clone(), false).unwrap();
    let cut = installer
        .commit_certificate(prefix.entries[0].certificate.clone())
        .unwrap();
    let original_digest = cut.durable_record_digest().to_owned();
    drop(installer);
    let reopened =
        open_formal_read(&h, &replay_scratch, Arc::new(EmptyIndexReader), false).unwrap();
    let replayed = reopened
        .replay_certified_prefix(&prefix, height, &id)
        .unwrap();
    assert_eq!(replayed.tip.commitment, executed.commitment);
    assert_eq!(
        reopened.verified_cut(3).unwrap().durable_record_digest(),
        original_digest
    );
}
