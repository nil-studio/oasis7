use super::*;
#[test]
fn rollback_delta_keeps_large_unchanged_text_out_of_read_allowance() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut source = RollbackNonceOutcome {
        canonical_intent_hash: "original".into(),
        rollback_checkpoint_batch_id: "c".into(),
        rollback_checkpoint_snapshot_hash: "h".into(),
        rollback_checkpoint_journal_len: 3,
        rollback_ticket: "ticket".into(),
        target_journal_commitment: "j".into(),
        target_state_root: "s".into(),
        target_journal_len: 5,
        affected_census_digest: "d".into(),
        affected_census_count: 1,
        readiness_blockers: vec!["a".into(), "".into()],
        rollback_event_id: u64::MAX,
        target_batch_id: "b".into(),
        prior_reorg_epoch: 7,
        committed_reorg_epoch: 9,
        invalidated_batch_ids: vec!["b1".into(), "b0".into()],
        dispositions: vec![RollbackEventDisposition {
            source_batch_id: "sb".into(),
            source_event_id: 11,
            status: RollbackDispositionStatus::CompensationRequired,
            compensation: Some(RollbackCompensationCaseRef {
                owner_id: "owner".into(),
                ticket_id: "t".into(),
                state: RollbackCompensationState::InProgress,
            }),
            player_id: Some(String::new()),
            action_id: None,
            system_authored: false,
        }],
        receipt: Some(RollbackReceiptProjection {
            receipt_id: "r".into(),
            canonical_intent_digest: "ci".into(),
            rollback_checkpoint_batch_id: "cb".into(),
            rollback_checkpoint_snapshot_hash: "cs".into(),
            rollback_checkpoint_journal_len: 12,
            replay_target_batch_id: "rt".into(),
            replay_target_journal_len: 13,
            replay_target_state_root: "rs".into(),
            replay_target_journal_commitment: "rj".into(),
            affected_census_digest: "ad".into(),
            affected_census_count: 14,
            readiness_blockers: vec!["blocked".into()],
            snapshot_height: 15,
            snapshot_hash: "sh".into(),
            log_cursor: 16,
            acknowledged_at_tick: 17,
        }),
    };
    let _input_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    source.canonical_intent_hash = "x".repeat(8 << 20);
    let root = prepare_rollback_record_v1(
        RollbackRecordRefV1::Outcome(&source),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    source.prior_reorg_epoch += 1;
    let after = prepare_rollback_record_v1(
        RollbackRecordRefV1::Outcome(&source),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();

    let mut logical = io();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut work = RollbackWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut cancel = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 4096,
            max_text_bytes: 16 << 20,
            max_record_bytes: 32 << 20,
        },
        cancelled: &mut cancel,
    };
    // Private helper receives the exact before body authenticated by its caller.
    // Physical page leases are an independent owner-level requirement.
    stream::qualify_outcome(&root, &mut ctx).unwrap();
    ctx.values.io.read_bytes_remaining = 2 << 20;
    stream::advance_outcome(&root, &after, &mut ctx, &mut 100000).unwrap();
    assert!(ctx.values.io.read_bytes_remaining > 0);
}

#[test]
fn rollback_delta_visits_only_changed_disposition_and_keeps_large_text_borrowed() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut source = RollbackNonceOutcome {
        canonical_intent_hash: "original".into(),
        rollback_checkpoint_batch_id: "c".into(),
        rollback_checkpoint_snapshot_hash: "h".into(),
        rollback_checkpoint_journal_len: 3,
        rollback_ticket: "ticket".into(),
        target_journal_commitment: "j".into(),
        target_state_root: "s".into(),
        target_journal_len: 5,
        affected_census_digest: "d".into(),
        affected_census_count: 1,
        readiness_blockers: vec!["a".into(), "".into()],
        rollback_event_id: u64::MAX,
        target_batch_id: "b".into(),
        prior_reorg_epoch: 7,
        committed_reorg_epoch: 9,
        invalidated_batch_ids: vec!["b1".into(), "b0".into()],
        dispositions: vec![RollbackEventDisposition {
            source_batch_id: "sb".into(),
            source_event_id: 11,
            status: RollbackDispositionStatus::CompensationRequired,
            compensation: Some(RollbackCompensationCaseRef {
                owner_id: "owner".into(),
                ticket_id: "t".into(),
                state: RollbackCompensationState::InProgress,
            }),
            player_id: Some(String::new()),
            action_id: None,
            system_authored: false,
        }],
        receipt: Some(RollbackReceiptProjection {
            receipt_id: "r".into(),
            canonical_intent_digest: "ci".into(),
            rollback_checkpoint_batch_id: "cb".into(),
            rollback_checkpoint_snapshot_hash: "cs".into(),
            rollback_checkpoint_journal_len: 12,
            replay_target_batch_id: "rt".into(),
            replay_target_journal_len: 13,
            replay_target_state_root: "rs".into(),
            replay_target_journal_commitment: "rj".into(),
            affected_census_digest: "ad".into(),
            affected_census_count: 14,
            readiness_blockers: vec!["blocked".into()],
            snapshot_height: 15,
            snapshot_hash: "sh".into(),
            log_cursor: 16,
            acknowledged_at_tick: 17,
        }),
    };
    let _input_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    source.canonical_intent_hash = "x".repeat(8 << 20);
    let root = prepare_rollback_record_v1(
        RollbackRecordRefV1::Outcome(&source),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    source.dispositions[0].status = RollbackDispositionStatus::Replayed;
    let after = prepare_rollback_record_v1(
        RollbackRecordRefV1::Outcome(&source),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();

    let mut logical = io();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut work = RollbackWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut cancel = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 4096,
            max_text_bytes: 16 << 20,
            max_record_bytes: 32 << 20,
        },
        cancelled: &mut cancel,
    };
    // Private helper receives the exact before body authenticated by its caller.
    // Physical page leases are an independent owner-level requirement.
    stream::qualify_outcome(&root, &mut ctx).unwrap();
    ctx.values.io.read_bytes_remaining = 4 << 20;
    stream::advance_outcome(&root, &after, &mut ctx, &mut 100000).unwrap();
    assert!(ctx.values.io.read_bytes_remaining > 0);
}
