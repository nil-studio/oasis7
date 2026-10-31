use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
        TransitionVerifier,
    },
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn member_delta_skips_unchanged_text_larger_than_the_cold_pool() {
    const TEXT: usize = 32 << 20;
    let directory = Directory::new();
    let path = directory.0.join("large");
    let canonical = {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _source = memory
            .reserve(TEXT + (1 << 20), MemoryClass::Ordinary)
            .unwrap();
        let store =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Zstd).unwrap();
        let mut source = source();
        source.wars.clear();
        source.crises.clear();
        source.economic_contracts.clear();
        source.alliances.get_mut(KEY).unwrap().charter = "x".repeat(TEXT);
        let rows = produce_with_limits(
            &store,
            &memory,
            &source,
            MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: TEXT as u64,
                record_bytes: (TEXT + 8192) as u64,
            },
        );
        roots(&build(&store, &memory, &rows))
            .canonical_bytes()
            .unwrap()
    };
    // The original World/store/pool have all been dropped. Recovery4MiB and the
    // subsequent normal24MiB pool are independently smaller than the32MiB Text.
    let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Zstd,
        MemoryClass::Recovery,
    )
    .unwrap();
    let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
    let before = roots
        .state
        .iter()
        .find(|r| r.namespace == Ns::Business)
        .unwrap();
    let (family, proof) = {
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: before,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: (TEXT + 8192) as u64,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_diplomacy_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            TEXT as u64,
        )
        .unwrap();
        let proof = open_qualified_diplomacy_record_v1(
            DiplomacyFieldV1::Alliances,
            KEY,
            &roots,
            &family,
            &mut ctx,
        )
        .unwrap()
        .unwrap();
        (family, proof)
    };
    let parent = ExecutionParent {
        roots,
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 4,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: (TEXT + 8192) as u64,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut writer = store.value_session(physical());
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: limits(),
        cancelled: || false,
    };
    let guard = ctx.transaction.guard();
    change_qualified_diplomacy_member_v1(
        DiplomacyMemberChangeV1 {
            field: DiplomacyFieldV1::Alliances,
            id: KEY,
            qualification: &proof,
            expected_length: 3,
            ordinal: 3,
            expected: None,
            after: Some(DiplomacyMemberV1::Alliance("")),
            max_current_payload_bytes: 165,
            max_pending_node_bytes: 4 << 20,
        },
        &mut ctx,
        &mut write,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = ctx.transaction.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(prepared.record.writes.len(), 1);
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
        .find(|r| r.namespace == Ns::Business)
        .unwrap();
    let mut io = logical();
    let initial_bytes = io.read_bytes_remaining;
    let initial_nodes = io.nodes_remaining;
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(after, &state, &values, &memory, &mut io, &mut keys);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    };
    let accepted = accept_diplomacy_transition_v1(
        &family,
        &verifier,
        &prepared.record,
        &mut ctx,
        &mut 100_000,
        165,
    )
    .unwrap();
    assert_eq!(accepted.record_counts(), [1, 0, 0, 0]);
    let bytes = initial_bytes - ctx.values.io.read_bytes_remaining;
    let nodes = initial_nodes - ctx.values.io.nodes_remaining;
    eprintln!(
        "fixed member delta receiver logical reads: {nodes}nodes {bytes}bytes; unchanged Text={TEXT}"
    );
    assert!(bytes < 512 << 10);
    assert!(nodes < 4096);
}
