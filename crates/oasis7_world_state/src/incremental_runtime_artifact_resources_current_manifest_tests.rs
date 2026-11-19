//! Child of resource receiver cold tests; reuses its actual seed/qualification.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
#[test]
fn current_manifest_opener_preserves_original_field_and_rejects_deleted_current_metadata() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let manifest = ChainResourceManifest::empty_at_height("actual-world", 7, 0);
    let builtin = Builtins(Default::default());
    let (pages, roots) = seed(&memory, &manifest, None, &builtin);
    let qualification = qualify(&pages, &roots, &memory).unwrap();
    assert_eq!(
        qualification.reserved_bytes(),
        std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>()
    );
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [4; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 4,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut vi = io();
    let mut kw = keys();
    let guard = {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        let pointer =
            crate::incremental_runtime_artifact_resources::open_current_chain_resource_manifest_v1(
                &qualification,
                &mut overlay,
                &guard,
                &live,
            )
            .unwrap();
        let mut read_io = io();
        let mut read_keys = keys();
        let mut read = WorldFieldReadContextV1 {
            root: &roots.state[5],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let original_pointer = inspect_runtime_artifact_record_root_v1(
            RuntimeArtifactFieldV1::ChainResourceManifest,
            &mut read,
        )
        .unwrap()
        .unwrap();
        assert_eq!(pointer.root(), original_pointer.root());
        assert_eq!(pointer.binding(), original_pointer.binding());
        read.root = pointer.root();
        let mut text = Vec::new();
        read_runtime_artifact_member_v1(
            RuntimeArtifactRecordV1::ChainResourceManifest,
            &pointer,
            "world_id",
            RuntimeArtifactMemberProfileV1::Text,
            &mut read,
            4096,
            &mut text,
        )
        .unwrap();
        assert_eq!(text, manifest.world_id.as_bytes());
        let metadata = WorldFieldKeyV1::new(
            RuntimeArtifactFieldV1::ChainResourceManifest.descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .unwrap();
        overlay
            .transaction
            .write(StateNamespaceV1::Artifacts, metadata.as_bytes(), None)
            .unwrap();
        let guard = overlay.transaction.guard();
        assert!(
            crate::incremental_runtime_artifact_resources::open_current_chain_resource_manifest_v1(
                &qualification,
                &mut overlay,
                &guard,
                &live
            )
            .is_err()
        );
        overlay.transaction.guard()
    };
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
