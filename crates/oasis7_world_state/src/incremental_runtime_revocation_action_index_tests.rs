use super::*;

#[test]
fn empty_action_index_readmit_checks_cancellation() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let (pages, roots) = seed_value(&memory, &crate::CapabilityRevocationState::default());
    let mut ri = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_runtime_revocation_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    for cancel_on in [1, 2] {
        let polls = std::cell::Cell::new(0);
        let result = proof
            .safe_alternative_actions
            .readmit(&memory, MemoryClass::Ordinary, || {
                polls.set(polls.get() + 1);
                polls.get() >= cancel_on
            });
        assert!(matches!(result, Err(ValueTreeError::Cancelled)));
        assert_eq!(polls.get(), cancel_on);
    }
}

#[test]
fn authenticated_action_index_tracks_member_delta_and_rejects_duplicates() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let mut source = crate::CapabilityRevocationState::default();
    for (key, id) in [("first", 7), ("second", 9)] {
        source.safe_alternatives.insert(
            key.into(),
            serde_json::json!({"authorization":{"action_id":id}}),
        );
    }
    let (pages, roots) = seed_value(&memory, &source);
    let mut ri = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_runtime_revocation_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(proof.safe_alternative_lineage_for_action(7), Some("first"));
    assert_eq!(proof.safe_alternative_lineage_for_action(9), Some("second"));

    source.safe_alternatives.remove("second");
    source.safe_alternatives.insert(
        "first".into(),
        serde_json::json!({"authorization":{"action_id":17}}),
    );
    source.safe_alternatives.insert(
        "third".into(),
        serde_json::json!({"authorization":{"action_id":21}}),
    );
    let prepare = |value: &crate::CapabilityRevocationState| {
        let mut sink = Sink(&pages);
        prepare_runtime_revocation_body_v1(
            value,
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            },
            &mut write_work(),
        )
        .unwrap()
    };
    let after = prepare(&source);
    let index = crate::incremental_runtime_revocation::action_index::advance(
        Some(&proof.safe_alternative_actions),
        &after,
        &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(index.lineage(7), None);
    assert_eq!(index.lineage(9), None);
    assert_eq!(index.lineage(17), Some("first"));
    assert_eq!(index.lineage(21), Some("third"));
    // The parent lookup is immutable while the changed member index is built.
    assert_eq!(proof.safe_alternative_lineage_for_action(7), Some("first"));
    source.safe_alternatives.insert(
        "duplicate".into(),
        serde_json::json!({"authorization":{"action_id":21}}),
    );
    let duplicate = prepare(&source);
    assert!(
        crate::incremental_runtime_revocation::action_index::advance(
            Some(&index),
            &duplicate,
            &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
            &mut scan(),
            &mut read_work()
        )
        .is_err()
    );
}
