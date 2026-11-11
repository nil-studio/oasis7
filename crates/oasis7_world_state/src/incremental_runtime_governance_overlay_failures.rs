use super::*;
#[test]
fn governance_incomplete_snapshot_batch_poison_and_independent_receiver_reject() {
    let fixture = seed(false, false);
    let Fixture {
        memory,
        pages,
        roots,
        ..
    } = &fixture;
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut cold = context(pages, roots, memory, &mut cold_io, &mut cold_keys);
    let family = qualify_runtime_governance_v1(roots, &mut cold, &mut scan(), 2 << 20).unwrap();
    let snapshot = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Epoch(101),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    let penalty = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Penalty(202),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    let parent = parent(roots);
    let live = parent.fence().unwrap();
    {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], pages, memory, limits()).unwrap();
        let mut logical = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut writer = Writer(pages);
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut wi,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 1 << 20,
            },
            cancelled: || false,
        };
        let guard = tx.guard();
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 1 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        change_runtime_governance_columns_v1(
            &family,
            roots,
            &penalty,
            &[RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
                expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(1)),
            }],
            &mut ctx,
            &mut write,
            &mut work(),
            &guard,
            &live,
        )
        .unwrap();
        let guard = ctx.transaction.guard();
        assert!(
            change_runtime_governance_columns_v1(
                &family,
                roots,
                &snapshot,
                &[RuntimeGovernanceColumnChangeV1 {
                    column: RuntimeGovernanceColumnV1::SignerCount,
                    expected: Some(RuntimeGovernanceValueV1::Unsigned(2)),
                    after: Some(RuntimeGovernanceValueV1::Unsigned(3))
                }],
                &mut ctx,
                &mut write,
                &mut work(),
                &guard,
                &live
            )
            .is_err()
        );
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
    // A malicious candidate bypasses the producer and changes only authenticated
    // count. Generic replay is valid; the independent owner receiver must reject.
    let mut writer = Writer(pages);
    let mut logical = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut read = ValueReadContext {
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    };
    let mut stop = || false;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut read,
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        cancelled: &mut stop,
    };
    let count =
        super::super::super::delta::key(RuntimeGovernanceColumnV1::SignerCount, &mut records)
            .unwrap();
    let previous = incremental_record_tree::lookup(snapshot.root(), &count.0, &mut records, LABELS)
        .unwrap()
        .unwrap();
    let mut wi = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 1 << 20,
        },
        cancelled: || false,
    };
    let row = write_member_record_v1(
        RuntimeGovernanceKindV1::EpochSnapshot.columns(),
        WorldFieldRecordKindV1::Entry,
        &("signer_node_ids", "count"),
        MemberPayloadRefV1::U64(3),
        &mut write,
    )
    .unwrap();
    let bad = incremental_record_tree::edit(
        snapshot.root().clone(),
        &count.0,
        (Some(previous), Some(row.value_root)),
        &mut read,
        &mut write,
        &mut stop,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 8 << 20,
            labels: LABELS,
        },
    )
    .unwrap();
    let outer = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut read,
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        cancelled: &mut stop,
    }
    .key(
        RuntimeGovernanceKindV1::EpochSnapshot.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &101u64,
    )
    .unwrap();
    let semantic = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .unwrap()
        .semantic_key
        .unwrap();
    let reference = RecordRootRefV1::new(
        RuntimeGovernanceKindV1::EpochSnapshot.schema(),
        semantic,
        bad,
    )
    .unwrap();
    let row = write_member_record_v1(
        RuntimeGovernanceKindV1::EpochSnapshot.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &101u64,
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut write,
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], pages, memory, limits()).unwrap();
    tx.write(
        StateNamespaceV1::Authorization,
        row.field_key.as_bytes(),
        Some(row.value_root),
    )
    .unwrap();
    let guard = tx.guard();
    let transition = tx.seal(&guard, &live, 1).unwrap();
    for node in &transition.new_nodes {
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        ValueNodeWriter::put_node(
            &mut Writer(pages),
            *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
            charge.retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    assert!(verifier.verify(&transition.record).is_ok());
    let mut receive_io = io();
    let mut receive_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut receive = context(
        pages,
        &transition.record.after,
        memory,
        &mut receive_io,
        &mut receive_keys,
    );
    assert!(
        accept_runtime_governance_transition_v1(
            &family,
            &verifier,
            &transition.record,
            &mut receive,
            &mut scan(),
            &mut work(),
            2 << 20
        )
        .is_err()
    );
}
#[test]
fn governance_batch_closes_option_and_snapshot_members_and_map_create_delete() {
    let fixture = seed(false, true);
    let Fixture {
        memory,
        pages,
        roots,
        ..
    } = &fixture;
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut cold = context(pages, roots, memory, &mut cold_io, &mut cold_keys);
    let family = qualify_runtime_governance_v1(roots, &mut cold, &mut scan(), 2 << 20).unwrap();
    let policy = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Policy,
        &mut cold,
    )
    .unwrap()
    .unwrap();
    assert!(
        open_runtime_governance_record_v1(
            &family,
            roots,
            RuntimeGovernanceKeyV1::Epoch(777),
            &mut cold
        )
        .unwrap()
        .is_none()
    );
    let parent = parent(roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], pages, memory, limits()).unwrap();
    let mut logical = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut wi = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut writer = Writer(pages);
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 1 << 20,
        },
        cancelled: || false,
    };
    let guard = tx.guard();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut logical,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let snapshot = GovernanceFinalityEpochSnapshot {
        epoch_id: 9,
        ..Default::default()
    };
    let record = create_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Epoch(777),
        RuntimeGovernanceRecordV1::EpochSnapshot(&snapshot),
        &mut ctx,
        &mut write,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    let record = change_runtime_governance_columns_v1(
        &family,
        roots,
        &record,
        &[
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::SignerCount,
                expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(1)),
            },
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Signer(0),
                expected: None,
                after: Some(RuntimeGovernanceValueV1::Text(" original signer ")),
            },
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::StakeCount,
                expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(1)),
            },
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Stake("count"),
                expected: None,
                after: Some(RuntimeGovernanceValueV1::Unsigned(u64::MAX)),
            },
        ],
        &mut ctx,
        &mut write,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = ctx.transaction.guard();
    delete_runtime_governance_record_v1(&family, roots, &record, &mut ctx, &guard, &live).unwrap();
    let penalty = GovernanceIdentityPenaltyRecord {
        penalty_id: 33,
        ..Default::default()
    };
    let guard = ctx.transaction.guard();
    let record = create_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Penalty(888),
        RuntimeGovernanceRecordV1::IdentityPenalty(&penalty),
        &mut ctx,
        &mut write,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    let record = change_runtime_governance_columns_v1(
        &family,
        roots,
        &record,
        &[
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::OptionPresence(GovernanceOptionV1::AppealReason),
                expected: Some(RuntimeGovernanceValueV1::Bool(false)),
                after: Some(RuntimeGovernanceValueV1::Bool(true)),
            },
            RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::OptionValue(GovernanceOptionV1::AppealReason),
                expected: None,
                after: Some(RuntimeGovernanceValueV1::Text("")),
            },
        ],
        &mut ctx,
        &mut write,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = tx.guard();
    let transition = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(transition.record.writes.len(), 1);
    for node in &transition.new_nodes {
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        ValueNodeWriter::put_node(
            &mut Writer(pages),
            *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
            charge.retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut ri = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut receive = context(pages, &transition.record.after, memory, &mut ri, &mut rk);
    let proof = accept_runtime_governance_transition_v1(
        &family,
        &verifier,
        &transition.record,
        &mut receive,
        &mut scan(),
        &mut work(),
        2 << 20,
    )
    .unwrap();
    assert_eq!(proof.record_counts(), [1, 0, 1]);
    let recovered = recover_runtime_governance_record_v1(
        RuntimeGovernanceKindV1::IdentityPenalty,
        record.root(),
        &mut receive,
        &mut scan(),
        2 << 20,
    )
    .unwrap();
    let RecoveredRuntimeGovernanceRecordV1::IdentityPenalty(value) = recovered.value() else {
        panic!("variant")
    };
    assert_eq!(value.penalty_id, 33);
    assert_eq!(value.appeal_reason.as_deref(), Some(""));
    let mut tx = ExecutionTransaction::begin(parent, [7; 32], pages, memory, limits()).unwrap();
    let guard = tx.guard();
    let mut oi = io();
    let mut ok = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    assert!(
        delete_runtime_governance_record_v1(&family, roots, &policy, &mut ctx, &guard, &live)
            .is_err()
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
