//! Read the actual GEN-backed grant under one paid Current transaction.  This
//! reader returns typed DATA, not an issuance authorization or WorldReady claim.
use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};

#[test]
fn current_grant_reads_all_original_fields_and_rejects_stale_guard() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("current-grant"), memory.clone(), compression)
                .unwrap();
        let original = source();
        let roots = seed(&store, &memory, &original);
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let family = qualify_main_token_policy_family_v1(
            &roots,
            &mut ctx!(
                &roots.state[0],
                &state,
                &values,
                &memory,
                MemoryClass::Ordinary,
                &mut logical(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                }
            ),
            &mut scan(),
            &mut scan(),
            &mut 8192,
            4096,
        )
        .unwrap();
        assert_eq!(family.record_counts()[3], 2);

        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [0x41; 32],
            authority: AuthorityBinding {
                token: [0x42; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut transaction = ExecutionTransaction::begin(
            parent,
            [0x43; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 200,
                max_writes: 32,
                max_facts: 0,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut rio = logical();
        let mut rkeys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut rio,
            },
            key_work: &mut rkeys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut family_scan = scan();
        let mut cells = 8192;
        let first_guard = overlay.transaction.guard();

        for id in [" raw grant ", "none"] {
            let guard = overlay.transaction.guard();
            let selected = read_current_restricted_claim_grant_v1(
                id,
                &family,
                &roots,
                &state,
                &mut overlay,
                &mut family_scan,
                &mut cells,
                4096,
                &guard,
                &live,
            )
            .unwrap();
            let actual = selected.value().expect("original GEN grant present");
            let expected = original
                .restricted_starter_claim_grants
                .get(id)
                .expect("original independent expected record");
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap(),
                "Current grant must preserve every typed source field"
            );
            assert!(selected.qualification().is_some());
        }

        let guard = overlay.transaction.guard();
        let absent = read_current_restricted_claim_grant_v1(
            "missing-original-grant",
            &family,
            &roots,
            &state,
            &mut overlay,
            &mut family_scan,
            &mut cells,
            4096,
            &guard,
            &live,
        )
        .unwrap();
        assert!(absent.value().is_none());
        assert!(absent.qualification().is_none());

        let stale = read_current_restricted_claim_grant_v1(
            " raw grant ",
            &family,
            &roots,
            &state,
            &mut overlay,
            &mut family_scan,
            &mut cells,
            4096,
            &first_guard,
            &live,
        );
        assert!(
            matches!(stale, Err(WorldFieldOverlayErrorV1::Execution(_))),
            "stale original read-set guard must fail closed"
        );
        let final_guard = overlay.transaction.guard();
        assert!(matches!(
            transaction.seal(&final_guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn cancelled_current_grant_rejects_before_original_io_and_poisons_transaction() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("cancelled-current-grant"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory, &source());
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let family = qualify_main_token_policy_family_v1(
        &roots,
        &mut ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut logical(),
            &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            }
        ),
        &mut scan(),
        &mut scan(),
        &mut 8192,
        4096,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [0x51; 32],
        authority: AuthorityBinding {
            token: [0x52; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [0x53; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let mut read_io = logical();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let before_nodes = read_io.nodes_remaining;
    let before_bytes = read_io.read_bytes_remaining;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || true,
    };
    let guard = overlay.transaction.guard();
    let result = read_current_restricted_claim_grant_v1(
        " raw grant ",
        &family,
        &roots,
        &state,
        &mut overlay,
        &mut scan(),
        &mut 8192,
        4096,
        &guard,
        &live,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    ));
    let final_guard = tx.guard();
    assert!(matches!(
        tx.seal(&final_guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
    assert_eq!(read_io.nodes_remaining, before_nodes);
    assert_eq!(read_io.read_bytes_remaining, before_bytes);
}
