use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits, TransitionVerifier,
};
use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
pub(super) fn exercise() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("transitions");
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let before = build(&store, &memory, &produce_all(&store, &memory, &source()));
        let parent = ExecutionParent {
            roots: roots(&before),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let initial = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut fixed_io = logical();
        let mut fixed_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut fixed = WorldFieldReadContextV1 {
            root: &before,
            state: &initial,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut fixed_io,
            },
            key_work: &mut fixed_keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || initial.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_organization_family_v1(
            &parent.roots,
            &mut fixed,
            &mut scan(),
            &mut scan(),
            &mut 1024,
            165,
        )
        .unwrap();
        let mut case_proof = open_qualified_organization_record_v1(
            OrganizationRootKeyV1::Case(CASE),
            &parent.roots,
            &family,
            &mut fixed,
        )
        .unwrap()
        .unwrap();
        let estate_proof = open_qualified_organization_record_v1(
            OrganizationRootKeyV1::Estate(ESTATE),
            &parent.roots,
            &family,
            &mut fixed,
        )
        .unwrap()
        .unwrap();
        // A new operation gets its own original physical allowance. The raw
        // qualification above remains one finite indivisible bootstrap scope.
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 96,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut bounds = limits();
        bounds.payload_bytes = 165;
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: bounds,
            cancelled: || false,
        };
        let mut empty_grant = grant();
        empty_grant.scopes.clear();
        empty_grant.replaces = None;
        let guard = read.transaction.guard();
        case_proof = create_qualified_organization_child_empty_v1(
            OrganizationEmptyChildCreateV1 {
                case: CASE,
                record: OrganizationRelativeRecordV1::Grant("new-grant-key"),
                source: WorldRuleOrganizationScalarsRefV1::Grant(&empty_grant),
                qualification: &case_proof,
                max_pending_node_bytes: 4 << 20,
            },
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut 32,
        )
        .unwrap();
        let added_scope = scope("new-object", Right::Preserve);
        let guard = read.transaction.guard();
        case_proof = change_qualified_organization_set_v1(
            OrganizationSetChangeV1 {
                request: OrganizationSetRequestV1 {
                    case: CASE,
                    key: OrganizationRelativeSetKeyV1::Scope {
                        grant: "new-grant-key",
                        scope: &added_scope,
                    },
                    qualification: &case_proof,
                },
                expected: OrganizationSetReadV1::Absent,
                after: true,
                max_pending_node_bytes: 4 << 20,
            },
            &mut read,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let preserved = scope("", Right::Preserve);
        for (expected, after) in [
            (OrganizationSetReadV1::Present, false),
            (OrganizationSetReadV1::Absent, true),
        ] {
            let guard = read.transaction.guard();
            case_proof = change_qualified_organization_set_v1(
                OrganizationSetChangeV1 {
                    request: OrganizationSetRequestV1 {
                        case: CASE,
                        key: OrganizationRelativeSetKeyV1::Scope {
                            grant: GRANT,
                            scope: &preserved,
                        },
                        qualification: &case_proof,
                    },
                    expected,
                    after,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        }
        let request = recovery();
        let guard = read.transaction.guard();
        case_proof = match delete_qualified_organization_closed_v1(
            OrganizationClosedDeleteV1 {
                owner: OrganizationRootKeyV1::Case(CASE),
                record: OrganizationRelativeRecordV1::Recovery(REQUEST),
                expected: WorldRuleOrganizationScalarsRefV1::Recovery(&request),
                qualification: &case_proof,
                max_pending_node_bytes: 4 << 20,
            },
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut 32,
        )
        .unwrap()
        {
            OrganizationClosedDeleteResultV1::CaseUpdated(proof) => proof,
            OrganizationClosedDeleteResultV1::EstateDeleted => {
                panic!("Recovery deletion must retain Case")
            }
        };
        // Recreate at the exact original request map key in this same Tx,
        // then remove it again; neither orphan adoption nor ID normalization.
        let guard = read.transaction.guard();
        case_proof = create_qualified_organization_child_empty_v1(
            OrganizationEmptyChildCreateV1 {
                case: CASE,
                record: OrganizationRelativeRecordV1::Recovery(REQUEST),
                source: WorldRuleOrganizationScalarsRefV1::Recovery(&request),
                qualification: &case_proof,
                max_pending_node_bytes: 4 << 20,
            },
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut 32,
        )
        .unwrap();
        let guard = read.transaction.guard();
        case_proof = match delete_qualified_organization_closed_v1(
            OrganizationClosedDeleteV1 {
                owner: OrganizationRootKeyV1::Case(CASE),
                record: OrganizationRelativeRecordV1::Recovery(REQUEST),
                expected: WorldRuleOrganizationScalarsRefV1::Recovery(&request),
                qualification: &case_proof,
                max_pending_node_bytes: 4 << 20,
            },
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut 32,
        )
        .unwrap()
        {
            OrganizationClosedDeleteResultV1::CaseUpdated(proof) => proof,
            OrganizationClosedDeleteResultV1::EstateDeleted => panic!("Recovery must retain Case"),
        };
        let deposit = estate();
        let guard = read.transaction.guard();
        assert!(matches!(
            delete_qualified_organization_closed_v1(
                OrganizationClosedDeleteV1 {
                    owner: OrganizationRootKeyV1::Estate(ESTATE),
                    record: OrganizationRelativeRecordV1::Estate,
                    expected: WorldRuleOrganizationScalarsRefV1::Estate(&deposit),
                    qualification: &estate_proof,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
                &mut 32
            )
            .unwrap(),
            OrganizationClosedDeleteResultV1::EstateDeleted
        ));
        // The original nested map key and internal requester id are independent;
        // removal must target all seven original columns at the map key.
        for (object, part, _, _) in golden().into_iter().chain(marker_frames()) {
            if matches!(object, Object::Recovery(_, _)) {
                assert_eq!(
                    case_proof
                        .root()
                        .get(key(object, part).as_bytes(), &state)
                        .unwrap(),
                    None
                );
            }
        }
        let guard = read.transaction.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(prepared.record.writes.len(), 2);
        TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&prepared.record)
        .unwrap();
        for node in &prepared.new_nodes {
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let after = prepared
            .record
            .after
            .state
            .iter()
            .find(|root| root.namespace == Ns::Authorization)
            .unwrap();
        let after_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let after_values = store.value_session(physical());
        let mut after_io = logical();
        let mut after_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut after_read = WorldFieldReadContextV1 {
            root: after,
            state: &after_state,
            values: ValueReadContext {
                source: &after_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut after_io,
            },
            key_work: &mut after_keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || after_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let accepted = accept_organization_transition_v1(
            &family,
            &TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &after_state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            },
            &prepared.record,
            &mut after_read,
            &mut scan(),
            &mut 4096,
            165,
        )
        .unwrap();
        assert_eq!(accepted.record_counts(), [1, 0]);
        // Reopen through the disk provider, with a fresh root-only qualifier.
        let cold =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let cold_state = cold
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let cold_values = cold.value_session(physical());
        let mut cold_io = logical();
        let mut cold_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut cold_read = WorldFieldReadContextV1 {
            root: after,
            state: &cold_state,
            values: ValueReadContext {
                source: &cold_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut cold_io,
            },
            key_work: &mut cold_keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || cold_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let restored = qualify_organization_family_v1(
            &prepared.record.after,
            &mut cold_read,
            &mut scan(),
            &mut scan(),
            &mut 1024,
            165,
        )
        .unwrap();
        assert_eq!(restored.record_counts(), [1, 0]);
        assert!(
            open_qualified_organization_record_v1(
                OrganizationRootKeyV1::Estate(ESTATE),
                &prepared.record.after,
                &restored,
                &mut cold_read
            )
            .unwrap()
            .is_none()
        );
        let recovered_case = open_qualified_organization_record_v1(
            OrganizationRootKeyV1::Case(CASE),
            &prepared.record.after,
            &restored,
            &mut cold_read,
        )
        .unwrap()
        .unwrap();
        let mut point = WorldFieldReadContextV1 {
            root: recovered_case.root(),
            state: &cold_state,
            values: ValueReadContext {
                source: &cold_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || cold_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut id = Vec::new();
        assert_eq!(
            read_qualified_organization_field_v1(
                OrganizationFieldRequestV1 {
                    owner: OrganizationRootKeyV1::Case(CASE),
                    record: OrganizationRelativeRecordV1::Grant("new-grant-key"),
                    field: OrganizationScalarFieldV1::Id,
                    qualification: &recovered_case,
                },
                &mut point,
                128,
                &mut id
            )
            .unwrap(),
            OrganizationFieldReadV1::Value(OrganizationFieldValueV1::Text { utf8_bytes: 11 })
        );
        assert_eq!(id, b"grant-inner");
        assert_eq!(
            read_qualified_organization_set_v1(
                OrganizationSetRequestV1 {
                    case: CASE,
                    key: OrganizationRelativeSetKeyV1::Scope {
                        grant: "new-grant-key",
                        scope: &added_scope
                    },
                    qualification: &recovered_case,
                },
                &mut point
            )
            .unwrap(),
            OrganizationSetReadV1::Present
        );
    }
}
