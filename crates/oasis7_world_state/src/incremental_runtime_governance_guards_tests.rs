use super::*;
struct ClassView<'a> {
    pages: &'a Pages,
    class: MemoryClass,
}
impl MerkleNodeReader for ClassView<'_> {
    type Node = <Pages as MerkleNodeReader>::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.pages.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        MerkleNodeReader::read_node(self.pages, id, max)
    }
}
impl ValueNodeReader for ClassView<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != self.class {
            return Err(invalid());
        }
        ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)
    }
}
#[test]
fn governance_recovery_requires_family_and_record_ordinary_readmission() {
    let fixture = seed(false, false);
    let Fixture {
        memory,
        pages,
        roots,
        ..
    } = &fixture;
    let source = ClassView {
        pages,
        class: MemoryClass::Recovery,
    };
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut cold = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &source,
        values: ValueReadContext {
            source: &source,
            memory,
            class: MemoryClass::Recovery,
            io: &mut vi,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let recovery = qualify_runtime_governance_v1(roots, &mut cold, &mut scan(), 2 << 20).unwrap();
    let record = open_runtime_governance_record_v1(
        &recovery,
        roots,
        RuntimeGovernanceKeyV1::Penalty(202),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    assert_eq!(record.allocation_class(), MemoryClass::Recovery);
    let filler = memory
        .reserve(
            (256usize << 20) - (4usize << 20) - memory.used() - (64usize << 10),
            MemoryClass::Ordinary,
        )
        .unwrap();
    assert!(matches!(
        readmit_runtime_governance_ordinary_v1(&recovery, roots, memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    assert!(matches!(
        readmit_runtime_governance_record_ordinary_v1(&record, roots, memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(filler);
    let ordinary =
        readmit_runtime_governance_ordinary_v1(&recovery, roots, memory, || false).unwrap();
    let ordinary_record =
        readmit_runtime_governance_record_ordinary_v1(&record, roots, memory, || false).unwrap();
    assert!(matches!(
        readmit_runtime_governance_record_ordinary_v1(&record, roots, memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    for (family, record, success) in [
        (&recovery, &record, false),
        (&ordinary, &record, false),
        (&ordinary, &ordinary_record, true),
    ] {
        let parent = parent(roots);
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(parent, [6; 32], pages, memory, limits()).unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut writer = Writer(pages);
        let mut output = MemberRecordWriteContext {
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
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 1 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let result = change_runtime_governance_columns_v1(
            family,
            roots,
            record,
            &[RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
                expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(1)),
            }],
            &mut overlay,
            &mut output,
            &mut work(),
            &guard,
            &live,
        );
        assert_eq!(result.is_ok(), success);
        let guard = tx.guard();
        assert_eq!(tx.seal(&guard, &live, 1).is_ok(), success);
    }
}
#[test]
fn governance_noop_final_cancel_and_invalid_option_enum_poison() {
    let fixture = seed(false, false);
    let Fixture {
        memory,
        pages,
        roots,
        ..
    } = &fixture;
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut cold = context(pages, roots, memory, &mut vi, &mut keys);
    let family = qualify_runtime_governance_v1(roots, &mut cold, &mut scan(), 2 << 20).unwrap();
    let record = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Penalty(202),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    let mut final_callback = usize::MAX;
    for cancel_late in [false, true] {
        let parent = parent(roots);
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(parent, [6; 32], pages, memory, limits()).unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut writer = Writer(pages);
        let mut output = MemberRecordWriteContext {
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
        let calls = std::cell::Cell::new(0usize);
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 1 << 20,
            take_state_error: || None,
            cancelled: || {
                let n = calls.get() + 1;
                calls.set(n);
                cancel_late && n == final_callback
            },
        };
        let result = change_runtime_governance_columns_v1(
            &family,
            roots,
            &record,
            &[RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
                expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(0)),
            }],
            &mut overlay,
            &mut output,
            &mut work(),
            &guard,
            &live,
        );
        if cancel_late {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_none());
            final_callback = calls.get();
            assert!(final_callback > 0);
        }
        let guard = tx.guard();
        assert_eq!(tx.seal(&guard, &live, 1).is_ok(), !cancel_late);
    }
    for change in [
        RuntimeGovernanceColumnChangeV1 {
            column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
            expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
            after: Some(RuntimeGovernanceValueV1::Unsigned(4)),
        },
        RuntimeGovernanceColumnChangeV1 {
            column: RuntimeGovernanceColumnV1::OptionPresence(GovernanceOptionV1::AppealReason),
            expected: Some(RuntimeGovernanceValueV1::Bool(false)),
            after: Some(RuntimeGovernanceValueV1::Bool(true)),
        },
        RuntimeGovernanceColumnChangeV1 {
            column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
            expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
            after: None,
        },
    ] {
        let parent = parent(roots);
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(parent, [6; 32], pages, memory, limits()).unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut writer = Writer(pages);
        let mut output = MemberRecordWriteContext {
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
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 1 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        assert!(
            change_runtime_governance_columns_v1(
                &family,
                roots,
                &record,
                &[change],
                &mut overlay,
                &mut output,
                &mut work(),
                &guard,
                &live
            )
            .is_err()
        );
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
