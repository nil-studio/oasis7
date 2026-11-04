use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_runtime_authorization_root::*,
    incremental_runtime_controls::*,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{ClassedPages, Pages, Writer},
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::Cell;
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 200_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 200_000,
        leaves_remaining: 10_000,
        read_bytes_remaining: 64 << 20,
    }
}
struct Counted<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    reads: Cell<usize>,
    allowed: Cell<bool>,
}
impl MerkleNodeReader for Counted<'_> {
    type Node = <Pages as MerkleNodeReader>::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        assert!(self.allowed.get(), "owner repeated generic state replay");
        self.reads.set(self.reads.get() + 1);
        MerkleNodeReader::read_node(self.pages, id, max)
    }
}
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
    controls: ReservedValue<RuntimeControlsQualificationV1>,
    auth: ReservedValue<RuntimeAuthorizationRootQualificationV1>,
    p: ExecutionParent,
    sealed: ReservedValue<PreparedTransition>,
    _source: oasis7_proto::memory_budget::MemoryReservation,
}
fn fixture() -> Fixture {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::new(&memory);
    let mut out = Writer(&pages);
    let mut vi = io();
    let mut kw = keys();
    let mut rows = Vec::with_capacity(8);
    prepare_runtime_controls_genesis_v1(
        &RuntimeControlsRefV1 {
            tick_consensus_authority_source: "authority",
            next_governance_identity_penalty_id: 7,
            governance_emergency_brake_until_tick: None,
        },
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut 6,
        &mut |row| {
            assert!(rows.len() < 8);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let field = authorization_root_descriptor_v1();
    let meta = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let row = prepare_authorization_root_value_v1(
        "before",
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    for (key, value) in [
        (meta.field_key, meta.value_root),
        (row.field_key.clone(), row.value_root),
    ] {
        crate::incremental_field_write::emit(
            key,
            value,
            &mut MemberRecordWriteContext {
                sink: &mut out,
                memory: &memory,
                io: &mut vi,
                key_work: &mut kw,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
            &mut |row| {
                assert!(rows.len() < 8);
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
    }
    let mut roots = RootSetV1::empty("shared-witness".into(), [9; 32]).unwrap();
    for ns in [StateNamespaceV1::Allocator, StateNamespaceV1::Authorization] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta =
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &pages, 8 << 20).unwrap();
        for n in &delta.new_nodes {
            let id = *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes();
            out.put_node(
                id,
                memory
                    .reserve(n.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(n.bytes.clone()),
            )
            .unwrap();
        }
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    }
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &roots.state[3],
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let controls = qualify_runtime_controls_v1(&roots, &mut ctx, &mut scan(), 1 << 20).unwrap();
    let auth =
        qualify_runtime_authorization_root_v1(&roots, &mut ctx, &mut scan(), 1 << 20).unwrap();
    let p = ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    };
    let live = p.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        p.clone(),
        [7; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 8,
            max_facts: 4,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let after = prepare_authorization_root_value_v1(
        "after",
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    tx.write(
        StateNamespaceV1::Authorization,
        after.field_key.as_bytes(),
        Some(after.value_root),
    )
    .unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    Fixture {
        memory,
        pages,
        roots,
        controls,
        auth,
        p,
        sealed,
        _source,
    }
}
#[test]
fn verified_world_transition_replays_once_for_two_real_typed_owner_receivers() {
    let f = fixture();
    let source = Counted {
        pages: &f.pages,
        memory: &f.memory,
        reads: Cell::new(0),
        allowed: Cell::new(true),
    };
    let verifier = TransitionVerifier {
        parent: &f.p,
        operation: [7; 32],
        source: &source,
        memory: &f.memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &f.sealed.record.after.state[3],
        state: &source,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let baseline = f.memory.used();
    let witness = verify_world_transition_v1(&verifier, &f.sealed.record, &mut ctx).unwrap();
    assert!(source.reads.get() > 0);
    let reads = source.reads.get();
    source.allowed.set(false);
    let controls =
        accept_runtime_controls_verified_v1(&f.controls, &witness, &mut ctx, 1 << 20).unwrap();
    let auth = accept_runtime_authorization_root_verified_v1(&f.auth, &witness, &mut ctx, 1 << 20)
        .unwrap();
    controls.validate_binding(&f.sealed.record.after).unwrap();
    auth.validate_binding(&f.sealed.record.after).unwrap();
    assert_eq!(auth.utf8_bytes(), 5);
    assert_eq!(source.reads.get(), reads);
    assert_eq!(witness.before_digest(), &f.roots.digest().unwrap());
    assert_eq!(
        witness.after_digest(),
        &f.sealed.record.after.digest().unwrap()
    );
    assert_eq!(witness.prepared().record, f.sealed.record);
    let foreign = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
    assert!(
        witness
            .check_context(&foreign, MemoryClass::Ordinary, ctx.root)
            .is_err()
    );
    assert!(
        witness
            .check_context(&f.memory, MemoryClass::Recovery, ctx.root)
            .is_err()
    );
    assert!(
        witness
            .check_context(&f.memory, MemoryClass::Ordinary, &f.roots.state[3])
            .is_err()
    );
    drop(controls);
    drop(auth);
    drop(witness);
    assert_eq!(f.memory.used(), baseline);
}
#[test]
fn verified_world_transition_bad_parent_preflight_never_reads_generic_source() {
    let f = fixture();
    let source = Counted {
        pages: &f.pages,
        memory: &f.memory,
        reads: Cell::new(0),
        allowed: Cell::new(false),
    };
    let verifier = TransitionVerifier {
        parent: &f.p,
        operation: [7; 32],
        source: &source,
        memory: &f.memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let recovery = ClassedPages::new(&f.pages, &f.memory, MemoryClass::Recovery);
    let mut vi = io();
    let mut kw = keys();
    let recovered = qualify_runtime_controls_v1(
        &f.roots,
        &mut WorldFieldReadContextV1 {
            root: &f.roots.state[3],
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &f.memory,
                class: MemoryClass::Recovery,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        1 << 20,
    )
    .unwrap();
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &f.sealed.record.after.state[3],
        state: &source,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    assert!(matches!(
        accept_runtime_controls_transition_v1(
            &recovered,
            &verifier,
            &f.sealed.record,
            &mut ctx,
            1 << 20
        ),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
    ));
    let mut other = f.roots.clone();
    other.height += 1;
    let mut vi2 = io();
    let mut kw2 = keys();
    let wrong = qualify_runtime_controls_v1(
        &other,
        &mut WorldFieldReadContextV1 {
            root: &other.state[3],
            state: &f.pages,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut vi2,
            },
            key_work: &mut kw2,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        1 << 20,
    )
    .unwrap();
    assert!(matches!(
        accept_runtime_controls_transition_v1(
            &wrong,
            &verifier,
            &f.sealed.record,
            &mut ctx,
            1 << 20
        ),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
    ));
    let used = f.memory.used();
    let fill = f
        .memory
        .reserve((128 << 20) - (16 << 20) - used - 64, MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        accept_runtime_controls_transition_v1(
            &f.controls,
            &verifier,
            &f.sealed.record,
            &mut ctx,
            1 << 20
        ),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Memory(_)))
    ));
    drop(fill);
    assert_eq!(f.memory.used(), used);
    assert_eq!(source.reads.get(), 0);
}
#[test]
fn verified_world_transition_constructor_rejects_context_cancel_and_original_error_before_io() {
    let f = fixture();
    let source = Counted {
        pages: &f.pages,
        memory: &f.memory,
        reads: Cell::new(0),
        allowed: Cell::new(false),
    };
    let verifier = TransitionVerifier {
        parent: &f.p,
        operation: [7; 32],
        source: &source,
        memory: &f.memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    for mode in 0..4 {
        let foreign = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
        let mut vi = io();
        let mut kw = keys();
        let result = verify_world_transition_v1(
            &verifier,
            &f.sealed.record,
            &mut WorldFieldReadContextV1 {
                root: if mode == 2 {
                    &f.roots.state[3]
                } else {
                    &f.sealed.record.after.state[3]
                },
                state: &source,
                values: ValueReadContext {
                    source: &f.pages,
                    memory: if mode == 1 { &foreign } else { &f.memory },
                    class: if mode == 0 {
                        MemoryClass::Recovery
                    } else {
                        MemoryClass::Ordinary
                    },
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || mode == 3,
            },
        );
        assert!(matches!(
            result,
            Err(WorldTransitionVerificationErrorV1::Value(_))
        ));
    }
    let mut vi = io();
    let mut kw = keys();
    let result = verify_world_transition_v1(
        &verifier,
        &f.sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &f.sealed.record.after.state[3],
            state: &source,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || Some(ValueTreeError::Invalid("original witness source error")),
            cancelled: || false,
        },
    );
    assert!(matches!(
        result,
        Err(WorldTransitionVerificationErrorV1::Value(
            ValueTreeError::Invalid("original witness source error")
        ))
    ));
    assert_eq!(source.reads.get(), 0);
}
