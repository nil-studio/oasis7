//! Nonempty raw Agent authorization keys coexist with the real rollback owner.
use super::*;
use crate::incremental_agent_fields::{
    AgentFieldKeyV1, AgentFieldV1, AgentScalarRefV1, write_agent_scalar_stream_v1,
};
use crate::incremental_value_mutation::ValueMutationContextV1;

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 1000,
    }
}
fn fixture(memory: &MemoryBudget, malformed: bool) -> (Pages, RootSetV1) {
    let mut pages = Pages::new(memory);
    let registry = RollbackAuthorityRegistry::default();
    let nonces = Default::default();
    let outcomes = Default::default();
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::new();
    prepare_rollback_controls_genesis_v1(
        &RollbackControlsRefV1 {
            authority_registry: &registry,
            consumed_nonces: &nonces,
            nonce_outcomes: &outcomes,
        },
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    // Real independently registered raw field and its original scalar wire.
    assert_eq!(
        AgentFieldV1::IntentPresent.namespace(),
        StateNamespaceV1::Authorization
    );
    let key = AgentFieldKeyV1::new("alice", AgentFieldV1::IntentPresent, 0).unwrap();
    let value = write_agent_scalar_stream_v1(
        AgentScalarRefV1::Bool(false),
        &mut ValueMutationContextV1 {
            sink: &mut pages,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.iter().map(|row| row.mutation.clone()).collect();
    writes.push(StateWriteV1 {
        key: key.as_bytes().to_vec(),
        before: None,
        after: Some(value.digest().unwrap()),
    });
    if malformed {
        writes.push(StateWriteV1 {
            key: b"oasis7.iws.world-field-key.v1Xbad".to_vec(),
            before: None,
            after: Some(value.digest().unwrap()),
        });
    }
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let _delta = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    for node in delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        pages
            .put_node(
                id,
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes),
            )
            .unwrap();
    }
    let mut roots = RootSetV1::empty("rollback-with-agent".into(), [77; 32]).unwrap();
    roots.state[3] = delta.after;
    (pages, roots)
}
#[test]
fn rollback_qualification_and_recovery_keep_foreign_agent_domain_separate() {
    for malformed in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
        let (pages, roots) = fixture(&memory, malformed);
        let mut value_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut work = RollbackWorkV1 {
            cells_remaining: 10000,
            max_pending_node_bytes: 8 << 20,
        };
        let mut values = ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        };
        let mut cancelled = || false;
        let mut ctx = RollbackDecodeContextV1 {
            values: &mut values,
            keys: &mut keys,
            work: &mut work,
            limits: RollbackDecodeLimitsV1 {
                max_output_bytes: 65536,
                max_text_bytes: 4096,
                max_record_bytes: 2 << 20,
            },
            cancelled: &mut cancelled,
        };
        let qualified = qualify_rollback_family_v1(&roots, &pages, &mut scan(), &mut ctx, || None);
        let recovered = crate::incremental_rollback_family_recovery::recover_rollback_controls_v1(
            &roots,
            &pages,
            &mut scan(),
            &mut ctx,
        );
        if malformed {
            assert!(matches!(
                qualified,
                Err(ValueTreeError::Invalid("world field key domain/length"))
            ));
            assert!(matches!(
                recovered,
                Err(ValueTreeError::Invalid("world field key domain/length"))
            ));
        } else {
            let qualified = qualified.unwrap();
            qualified.validate_binding(&roots).unwrap();
            assert_eq!(qualified.consumed_nonce_count(), 0);
            assert_eq!(qualified.outcome_count(), 0);
            let recovered = recovered.unwrap();
            assert!(recovered.consumed_nonces.is_empty());
            assert!(recovered.nonce_outcomes.is_empty());
        }
    }
}
