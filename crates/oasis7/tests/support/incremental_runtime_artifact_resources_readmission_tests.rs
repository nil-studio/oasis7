use super::change_tests::execute_change;
use super::*;

#[test]
fn recovery_resources_readmission_bridges_to_ordinary_delta_tick_and_cold_requalifies() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, initial_delta, builtins) = rich_sources();
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&initial_delta),
            builtin_release_manifest: &builtin_source,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());

        let recovery_store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let recovery_qualification =
            qualify_with_class(&recovery_store, &memory, &roots, MemoryClass::Recovery).unwrap();
        recovery_qualification
            .validate_binding(&roots.digest().unwrap(), MemoryClass::Recovery)
            .unwrap();
        assert_eq!(recovery_qualification.record_counts(), [1, 1, 1]);

        let ordinary_store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let ordinary_source = qualify(&ordinary_store, &memory, &roots).unwrap();
        let before_ordinary_rejection = memory.used();
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &ordinary_source,
                &roots,
                &memory,
                || false,
            ),
            Err(ValueTreeError::Invalid(_))
        ));
        assert_eq!(memory.used(), before_ordinary_rejection);
        drop(ordinary_source);
        drop(ordinary_store);
        drop(recovery_store);

        let hidden = directory.0.join("pages-hidden-during-readmission");
        std::fs::rename(&path, &hidden).unwrap();
        assert!(!path.exists());

        let foreign = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let foreign_before = foreign.used();
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &roots,
                &foreign,
                || false,
            ),
            Err(ValueTreeError::Invalid(_))
        ));
        assert_eq!(foreign.used(), foreign_before);

        let mut wrong_roots = roots.clone();
        wrong_roots.height = wrong_roots.height.saturating_add(1);
        let before_wrong_root = memory.used();
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &wrong_roots,
                &memory,
                || false,
            ),
            Err(ValueTreeError::Invalid(_))
        ));
        assert_eq!(memory.used(), before_wrong_root);

        let before_start_cancel = memory.used();
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &roots,
                &memory,
                || true,
            ),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(memory.used(), before_start_cancel);

        let mut cancellation_checks = 0;
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &roots,
                &memory,
                || {
                    cancellation_checks += 1;
                    cancellation_checks == 2
                },
            ),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(cancellation_checks, 2);
        assert_eq!(memory.used(), before_start_cancel);

        let ordinary_limit = (64 << 20) - (16 << 20);
        let used_before_pressure = memory.used();
        assert!(used_before_pressure < ordinary_limit);
        let pressure = memory
            .reserve(ordinary_limit - used_before_pressure, MemoryClass::Ordinary)
            .unwrap();
        let before_failed_admission = memory.used();
        assert!(matches!(
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &roots,
                &memory,
                || false,
            ),
            Err(ValueTreeError::Memory(_))
        ));
        assert_eq!(memory.used(), before_failed_admission);
        drop(pressure);
        assert_eq!(memory.used(), used_before_pressure);

        let before_readmission = memory.used();
        let ordinary_qualification =
            oasis7_runtime::incremental_runtime_artifact_resources::readmit_runtime_artifact_resources_ordinary_v1(
                &recovery_qualification,
                &roots,
                &memory,
                || false,
            )
            .unwrap();
        assert_eq!(
            ordinary_qualification.reserved_bytes(),
            std::mem::size_of::<oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>()
        );
        assert_eq!(
            memory.used() - before_readmission,
            ordinary_qualification.reserved_bytes()
        );
        ordinary_qualification
            .validate_binding(&roots.digest().unwrap(), MemoryClass::Ordinary)
            .unwrap();

        std::fs::rename(&hidden, &path).unwrap();
        assert!(path.exists());
        let ordinary_store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let mut work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 4 << 20,
        };
        let after_tick = initial_delta.tick.checked_add(1).unwrap();
        let prepared = execute_change(
            &ordinary_store,
            &memory,
            &roots,
            &ordinary_qualification,
            [0xb7; 32],
            RuntimeArtifactResourcesChangeV1::DeltaTick(RuntimeArtifactDeltaTickChangeV1 {
                expected: initial_delta.tick,
                after: after_tick,
            }),
            &mut work,
            true,
        );
        assert_eq!(prepared.qualification().record_counts(), [1, 1, 1]);
        let after_roots = prepared.transition().record.after.clone();
        drop(prepared);
        drop(ordinary_qualification);
        drop(recovery_qualification);
        drop(ordinary_store);

        let cold_memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let cold_store = IncrementalPageStore::open(
            &path,
            cold_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cold = qualify(&cold_store, &cold_memory, &after_roots).unwrap();
        cold.validate_binding(&after_roots.digest().unwrap(), MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(cold.record_counts(), [1, 1, 1]);
        let delta_root = field_root(
            &cold_store,
            &cold_memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .expect("persisted delta remains present");
        assert_eq!(
            unsigned_cell(
                &cold_store,
                &cold_memory,
                &delta_root,
                RuntimeArtifactRecordV1::ChainResourceDelta,
                "tick",
            ),
            RuntimeArtifactValueV1::Unsigned(after_tick)
        );
    }
}
