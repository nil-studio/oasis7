use super::*;
use AuthorizationMapSetFieldV1 as Field;
use AuthorizationMembershipV1 as Membership;
struct CancelAfterPut<'a, W> {
    inner: &'a mut W,
    calls: &'a std::cell::Cell<usize>,
    at: &'a std::cell::Cell<usize>,
    cancelled: &'a std::cell::Cell<bool>,
}
impl<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter>
    oasis7_formal_protocol::incremental_value::ValueNodeWriter for CancelAfterPut<'_, W>
{
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() == self.at.get() {
            self.cancelled.set(true);
        }
        Ok(())
    }
}
pub(super) fn prove_overlay(
    path: &std::path::Path,
    compression: PageCompression,
    roots: &[Vec<u8>; 3],
) {
    let next = {
        use oasis7_formal_protocol::incremental_execution::{
            AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
            TransitionVerifier,
        };
        use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
        use oasis7_runtime::incremental_field_overlay::{
            WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1 as Error,
        };
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let mut root_set = RootSetV1::empty("authorization-mapsets".into(), [3; 32]).unwrap();
        *root_set
            .state
            .iter_mut()
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap() = StateRootV1::from_canonical_bytes(&roots[0]).unwrap();
        let parent = ExecutionParent {
            commit: [4; 32],
            roots: root_set,
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let limits = TransactionLimits {
            max_reads: 32,
            max_writes: 8,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        };
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits).unwrap();
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16384,
                },
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|_| ValueTreeError::Invalid("state page failure"))
                },
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16384,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 8,
                    record_bytes: 4096,
                },
                cancelled: || false,
            };
            for (field, subject, member, expected, after, create, count) in [
                (
                    Field::DataAccessPermissions,
                    "",
                    "",
                    Membership::Present,
                    false,
                    false,
                    1,
                ),
                (
                    Field::DataAccessPermissions,
                    "",
                    "",
                    Membership::MemberAbsent,
                    true,
                    false,
                    1,
                ),
                (
                    Field::DataAccessPermissions,
                    "new:p",
                    "new:k",
                    Membership::GroupAbsent,
                    true,
                    true,
                    2,
                ),
                (
                    Field::DataAccessPermissions,
                    "new:p",
                    "new:k",
                    Membership::Present,
                    false,
                    false,
                    1,
                ),
                (
                    Field::DataAccessPermissions,
                    "new:p",
                    "new:k",
                    Membership::MemberAbsent,
                    true,
                    false,
                    1,
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "",
                    "role-only",
                    Membership::Present,
                    false,
                    false,
                    1,
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "",
                    "role-only",
                    Membership::MemberAbsent,
                    false,
                    false,
                    0,
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "empty-group",
                    "public",
                    Membership::MemberAbsent,
                    true,
                    false,
                    1,
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "empty-group",
                    "public",
                    Membership::Present,
                    true,
                    false,
                    0,
                ),
                (
                    Field::DataAccessPermissions,
                    "empty-new",
                    "unused",
                    Membership::GroupAbsent,
                    false,
                    true,
                    1,
                ),
                (
                    Field::DataAccessPermissions,
                    "nobody",
                    "k",
                    Membership::GroupAbsent,
                    false,
                    false,
                    0,
                ),
            ] {
                let guard = read.transaction.guard();
                assert_eq!(
                    change_authorization_membership_overlay_v1(
                        AuthorizationMembershipChangeV1 {
                            field,
                            subject,
                            member,
                            expected,
                            after,
                            create_group: create
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live
                    )
                    .unwrap(),
                    count
                );
                let result =
                    read_authorization_membership_overlay_v1(field, subject, member, &mut read)
                        .unwrap();
                let expected_after = if after {
                    Membership::Present
                } else if expected == Membership::GroupAbsent && !create {
                    Membership::GroupAbsent
                } else {
                    Membership::MemberAbsent
                };
                assert_eq!(result, expected_after);
            }
        }
        let guard = transaction.guard();
        let sealed = transaction.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 14);
        assert_eq!(sealed.record.writes.len(), 5);
        let verifier = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let verified = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &verifier,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
        assert_eq!(verified.record.after, sealed.record.after);
        let mut pages = store.value_session(physical());
        for node in &verified.new_nodes {
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            pages
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
        }
        for case in 0..5 {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                    .unwrap();
            let calls = std::cell::Cell::new(0);
            let cancel_at = std::cell::Cell::new(usize::MAX);
            let cancel = std::cell::Cell::new(false);
            let mut counted = CancelAfterPut {
                inner: &mut writer,
                calls: &calls,
                at: &cancel_at,
                cancelled: &cancel,
            };
            let group_nodes = if case == 4 {
                use oasis7_runtime::incremental_field_presence::{
                    PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
                };
                let field = FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name: "data_access_permissions",
                    destination: FieldDestination::State(StateNamespaceV1::Authorization),
                    shape: FieldShape::MapSet,
                };
                write_field_group_v1(
                    field,
                    "cancel-player",
                    &mut PresenceRecordWriteContext {
                        sink: &mut counted,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192,
                        },
                        limits: PresenceRecordLimitsV1 {
                            semantic_bytes: 1024,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    },
                )
                .unwrap();
                calls.get()
            } else {
                0
            };
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut counted,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || cancel.get(),
                };
                let guard = read.transaction.guard();
                change_authorization_membership_overlay_v1(
                    AuthorizationMembershipChangeV1 {
                        field: Field::DataAccessPermissions,
                        subject: "",
                        member: "",
                        expected: Membership::Present,
                        after: false,
                        create_group: false,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap();
                let (subject, expected, create) = match case {
                    0 => ("no-create", Membership::GroupAbsent, false),
                    1 => ("missing", Membership::MemberAbsent, true),
                    2 => ("empty-group", Membership::GroupAbsent, false),
                    3 => ("partial", Membership::GroupAbsent, true),
                    _ => ("cancel-player", Membership::GroupAbsent, true),
                };
                if case == 3 {
                    write.limits.semantic_bytes = b"oasis7.iws.semantic-key.v1\0".len() as u64
                        + 1
                        + 8
                        + "partial".len() as u64;
                }
                if case == 4 {
                    calls.set(0);
                    cancel_at.set(group_nodes + 1);
                }
                let guard = read.transaction.guard();
                let result = change_authorization_membership_overlay_v1(
                    AuthorizationMembershipChangeV1 {
                        field: Field::DataAccessPermissions,
                        subject,
                        member: "k",
                        expected,
                        after: true,
                        create_group: create,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                );
                if case == 4 {
                    assert!(group_nodes > 0);
                    assert_eq!(calls.get(), group_nodes + 1);
                    assert!(cancel.get());
                }
                match (case, result) {
                    (
                        0,
                        Err(Error::Value(ValueTreeError::Invalid(
                            "authorization MapSet missing group",
                        ))),
                    )
                    | (
                        1 | 2,
                        Err(Error::Value(ValueTreeError::Invalid(
                            "authorization MapSet expected value",
                        ))),
                    )
                    | (
                        3,
                        Err(Error::Member(MemberRecordError::Value(
                            ValueTreeError::ResourceLimited(_),
                        ))),
                    ) => {}
                    (
                        4,
                        Err(Error::Member(MemberRecordError::Value(ValueTreeError::Cancelled))),
                    ) => {}
                    (case, result) => panic!("unexpected nonce failure {case}: {result:?}"),
                }
            }
            let guard = transaction.guard();
            assert!(transaction.seal(&guard, &live, 1).is_err());
        }
        verified
            .record
            .after
            .state
            .iter()
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap()
            .canonical_bytes()
            .unwrap()
    };

    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let memory = MemoryBudget::new(
            4 << 20,
            if class == MemoryClass::Recovery {
                3 << 20
            } else {
                1 << 20
            },
        )
        .unwrap();
        let store = IncrementalPageStore::open(path, memory.clone(), compression, class).unwrap();
        let _filler = if class == MemoryClass::Recovery {
            Some(
                memory
                    .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap(),
            )
        } else {
            None
        };
        for newer in [false, true] {
            let root =
                StateRootV1::from_canonical_bytes(if newer { &next } else { &roots[0] }).unwrap();
            assert_eq!(root.leaf_count, if newer { 22 } else { 19 });
            for (field, subject, member, expected) in [
                (Field::DataAccessPermissions, "", "", Membership::Present),
                (
                    Field::DataAccessPermissions,
                    "new:p",
                    "new:k",
                    if newer {
                        Membership::Present
                    } else {
                        Membership::GroupAbsent
                    },
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "",
                    "role-only",
                    if newer {
                        Membership::MemberAbsent
                    } else {
                        Membership::Present
                    },
                ),
                (
                    Field::ModuleReleaseRoleBindings,
                    "empty-group",
                    "public",
                    if newer {
                        Membership::Present
                    } else {
                        Membership::MemberAbsent
                    },
                ),
                (
                    Field::DataAccessPermissions,
                    "empty-new",
                    "unused",
                    if newer {
                        Membership::MemberAbsent
                    } else {
                        Membership::GroupAbsent
                    },
                ),
            ] {
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut read = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || false,
                };
                assert_eq!(
                    read_authorization_membership_v1(field, subject, member, &mut read).unwrap(),
                    expected
                );
            }
        }
    }
}
