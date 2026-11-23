//! The production Current reader must consume genuine GEN bytes and the one
//! transaction's original parent/read-set, not a caller supplied profile DTO.
use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use oasis7_runtime::incremental_material_records::{
    MaterialProfileCurrentReadWorkV1, read_current_material_default_priority_overlay_v1,
    read_current_material_transport_loss_overlay_v1,
};
use oasis7_wasm_abi::{MaterialDefaultPriority, MaterialTransportLossClass};

fn state_error(error: impl std::fmt::Display) -> ValueTreeError {
    ValueTreeError::Io(std::io::Error::other(error.to_string()))
}

#[test]
fn current_profile_reads_original_gen_and_rejects_stale_guard_without_new_authority() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("material-current"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let roots = seed(&store, &memory, &source());
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical_scope());
        let family = qualify_material_family_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Business),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut scan(),
            &mut scan(),
            &mut 8192,
            1024,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 2]);

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
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut work = MaterialProfileCurrentReadWorkV1 {
            scan: scan(),
            cells_remaining: 8192,
            max_payload_bytes: 1024,
        };

        let first_guard = overlay.transaction.guard();
        assert_eq!(
            read_current_material_default_priority_overlay_v1(
                PROFILE,
                &family,
                &state,
                &mut overlay,
                &mut work,
                &first_guard,
                &live,
            )
            .unwrap(),
            Some(MaterialDefaultPriority::Urgent),
        );

        let refreshed = overlay.transaction.guard();
        assert_eq!(
            read_current_material_transport_loss_overlay_v1(
                PROFILE,
                &family,
                &state,
                &mut overlay,
                &mut work,
                &refreshed,
                &live,
            )
            .unwrap(),
            Some(MaterialTransportLossClass::High),
        );

        let refreshed = overlay.transaction.guard();
        assert_eq!(
            read_current_material_default_priority_overlay_v1(
                "not-an-original-profile",
                &family,
                &state,
                &mut overlay,
                &mut work,
                &refreshed,
                &live,
            )
            .unwrap(),
            None,
        );

        // A previous read has advanced the single real transaction's read set.
        // Using the original guard must fail closed and poison this transaction.
        let stale = read_current_material_default_priority_overlay_v1(
            PROFILE,
            &family,
            &state,
            &mut overlay,
            &mut work,
            &first_guard,
            &live,
        );
        assert!(
            matches!(stale, Err(WorldFieldOverlayErrorV1::Execution(_))),
            "stale Current guard must be rejected: {stale:?}"
        );
        let tail = overlay.transaction.guard();
        assert!(matches!(
            transaction.seal(&tail, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
