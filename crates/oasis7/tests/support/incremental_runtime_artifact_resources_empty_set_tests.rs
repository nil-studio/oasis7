use super::*;

#[test]
fn builtin_remove_missing_entry_prunes_existing_empty_set_but_absent_set_is_noop() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, delta, mut builtins) = rich_sources();
        builtins.insert("empty-set".into(), BTreeMap::new());

        // Use the actual Runtime removal API as the semantic oracle. It returns
        // false because the entry was absent, while still pruning a present
        // empty set. A wholly absent set remains an actual no-op.
        let mut runtime_json = serde_json::to_value(World::new()).unwrap();
        runtime_json["builtin_release_manifest"]["module_sets"] =
            serde_json::to_value(&builtins).unwrap();
        let mut runtime_world: World = serde_json::from_value(runtime_json).unwrap();
        assert!(
            runtime_world
                .builtin_release_manifest()
                .module_sets
                .contains_key("empty-set")
        );
        assert!(
            !runtime_world.remove_builtin_release_manifest_entry("empty-set", "missing-module")
        );
        assert!(
            !runtime_world
                .builtin_release_manifest()
                .module_sets
                .contains_key("empty-set")
        );
        let mut expected_runtime_sets = builtins.clone();
        expected_runtime_sets.remove("empty-set");
        assert_eq!(
            runtime_world.builtin_release_manifest().module_sets,
            expected_runtime_sets
        );
        let after_prune = runtime_world.builtin_release_manifest().module_sets.clone();
        assert!(
            !runtime_world.remove_builtin_release_manifest_entry("absent-set", "missing-module")
        );
        assert_eq!(
            runtime_world.builtin_release_manifest().module_sets,
            after_prune
        );

        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin_source,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());
        let qualified = qualify(&store, &memory, &roots).unwrap();
        assert_eq!(qualified.record_counts(), [1, 1, 1]);
        let empty_set = builtin_module_set_root(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
            "empty-set",
        )
        .expect("genesis must preserve the empty module set");
        assert_eq!(
            unsigned_cell(
                &store,
                &memory,
                &empty_set,
                RuntimeArtifactRecordV1::BuiltinModuleSet,
                "modules.count",
            ),
            RuntimeArtifactValueV1::Unsigned(0)
        );
        let manifest_root = field_root(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        )
        .expect("required builtin manifest");
        assert_eq!(
            unsigned_cell(
                &store,
                &memory,
                &manifest_root,
                RuntimeArtifactRecordV1::BuiltinReleaseManifest,
                "module_sets.count",
            ),
            RuntimeArtifactValueV1::Unsigned(2)
        );

        let mut work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 4 << 20,
        };
        let absent_set_noop = execute_change(
            &store,
            &memory,
            &roots,
            &qualified,
            [0x93; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "absent-set",
                module_id: "missing-module",
                expected_entry_root: None,
                after: None,
            }),
            &mut work,
            false,
        );
        assert!(absent_set_noop.transition().record.writes.is_empty());
        assert_eq!(
            absent_set_noop
                .transition()
                .record
                .before
                .state_root(StateNamespaceV1::Artifacts),
            absent_set_noop
                .transition()
                .record
                .after
                .state_root(StateNamespaceV1::Artifacts),
            "removing from a wholly absent set must leave the artifacts root unchanged"
        );
        assert_eq!(absent_set_noop.qualification().record_counts(), [1, 1, 1]);
        let noop_roots = absent_set_noop.transition().record.after.clone();

        let cancelled_prune = failed_change(
            &store,
            &memory,
            &noop_roots,
            absent_set_noop.qualification(),
            [0x95; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "empty-set",
                module_id: "missing-module",
                expected_entry_root: None,
                after: None,
            }),
        );
        assert!(matches!(
            cancelled_prune.error,
            oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                ValueTreeError::Cancelled
            ) | oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(
                MemberRecordError::Value(ValueTreeError::Cancelled)
            )
        ));
        assert!(cancelled_prune.value_puts > 0);
        assert!(cancelled_prune.cancelled_after_put);
        assert!(cancelled_prune.aborted);

        let pruned = execute_change(
            &store,
            &memory,
            &noop_roots,
            absent_set_noop.qualification(),
            [0x94; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "empty-set",
                module_id: "missing-module",
                expected_entry_root: None,
                after: None,
            }),
            &mut work,
            true,
        );
        assert_eq!(pruned.qualification().record_counts(), [1, 1, 1]);
        let after_roots = pruned.transition().record.after.clone();
        assert!(
            builtin_module_set_root(
                &store,
                &memory,
                after_roots.state_root(StateNamespaceV1::Artifacts),
                "empty-set",
            )
            .is_none()
        );
        assert!(
            builtin_module_set_root(
                &store,
                &memory,
                after_roots.state_root(StateNamespaceV1::Artifacts),
                "set\0key",
            )
            .is_some()
        );
        let after_manifest = field_root(
            &store,
            &memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        )
        .expect("required manifest after pruning one empty set");
        assert_eq!(
            unsigned_cell(
                &store,
                &memory,
                &after_manifest,
                RuntimeArtifactRecordV1::BuiltinReleaseManifest,
                "module_sets.count",
            ),
            RuntimeArtifactValueV1::Unsigned(1)
        );
        let after_digest = after_roots.digest().unwrap();
        drop(pruned);
        drop(absent_set_noop);
        drop(qualified);
        drop(store);

        let cold_memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
        let cold_store = IncrementalPageStore::open(
            &path,
            cold_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cold_qualification = qualify(&cold_store, &cold_memory, &after_roots).unwrap();
        cold_qualification
            .validate_binding(&after_digest, MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(cold_qualification.record_counts(), [1, 1, 1]);
        assert!(
            builtin_module_set_root(
                &cold_store,
                &cold_memory,
                after_roots.state_root(StateNamespaceV1::Artifacts),
                "empty-set",
            )
            .is_none()
        );
        assert!(
            builtin_module_set_root(
                &cold_store,
                &cold_memory,
                after_roots.state_root(StateNamespaceV1::Artifacts),
                "set\0key",
            )
            .is_some()
        );
        let cold_manifest = field_root(
            &cold_store,
            &cold_memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        )
        .expect("cold required manifest after pruning");
        assert_eq!(
            unsigned_cell(
                &cold_store,
                &cold_memory,
                &cold_manifest,
                RuntimeArtifactRecordV1::BuiltinReleaseManifest,
                "module_sets.count",
            ),
            RuntimeArtifactValueV1::Unsigned(1)
        );
    }
}
