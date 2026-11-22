use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
        TransitionVerifier,
    },
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
#[test]
fn generic_valid_changed_receipts_cannot_remove_required_outcome_relationships() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("receiver"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let outer = produce(&store, &memory, &source());
    let before = build(&store, &memory, &outer);
    let parent = ExecutionParent {
        roots: roots(&before),
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
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
    let family = qualify_diplomacy_family_v1(
        &parent.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    let field = DiplomacyFieldV1::Wars;
    let child = inspect_diplomacy_root_v1(field, KEY, &mut ctx)
        .unwrap()
        .unwrap();
    let original = bad_roots::rows(&store, &memory, &child);
    for case in 0..3 {
        let ordinal = if case == 2 { 1u64 } else { 0 };
        let mut number = vec![2];
        number.extend_from_slice(&ordinal.to_be_bytes());
        let member = fixture::pair_token(
            &fixture::pair_token(&[0], &fixture::text_token("participant_outcomes")),
            &number,
        );
        let token = if case == 1 {
            fixture::pair_token(&member, &fixture::text_token("data_delta"))
        } else {
            member
        };
        let key = fixture::key(
            field.name(),
            FieldShape::Map,
            WorldFieldRecordKindV1::Entry,
            Some(&token),
        )
        .0;
        let mut modified = original.clone();
        modified.retain(|(k, _)| k != key.as_bytes());
        if case == 2 {
            let token = fixture::pair_token(
                &fixture::pair_token(&[0], &fixture::text_token("participant_outcomes")),
                &fixture::text_token("length"),
            );
            let key = fixture::key(
                field.name(),
                FieldShape::Map,
                WorldFieldRecordKindV1::Entry,
                Some(&token),
            )
            .0;
            modified = replace(&store, &memory, &modified, &key, |bytes| {
                let n = bytes.len();
                bytes[n - 8..].copy_from_slice(&1u64.to_be_bytes());
            });
        }
        let malformed = build_from(&store, &memory, &child, &original, &modified);
        let mut writer = store.value_session(physical());
        let row = write_diplomacy_root_reference_v1(
            field,
            KEY,
            &malformed,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: limits(),
                cancelled: || false,
            },
        )
        .unwrap();
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
        let guard = tx.guard();
        tx.validate_guard(&guard, &live).unwrap();
        tx.write(Ns::Business, row.field_key.as_bytes(), Some(row.value_root))
            .unwrap();
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
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
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        verifier
            .verify(&prepared.record)
            .expect("generic Merkle receipt is valid");
        let after = prepared
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == Ns::Business)
            .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = fixed!(after, &state, &values, &memory, &mut io, &mut keys);
        assert!(
            matches!(
                accept_diplomacy_transition_v1(
                    &family,
                    &verifier,
                    &prepared.record,
                    &mut ctx,
                    &mut 100_000,
                    165
                ),
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "case {case}"
        );
    }
}
