use super::*;
use crate::incremental_artifacts::*;
use std::io::Write;
impl OriginalObjectStreamReader for Objects {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> std::io::Result<u64> {
        let bytes = self.read_original(id, max.min(usize::MAX as u64) as usize, memory)?;
        output.write_all(&bytes)?;
        Ok(bytes.len() as u64)
    }
}
struct Material {
    s: Setup,
    objects: Objects,
    pages: Pages,
    core: ReservedValue<CheckedCommitCore>,
    native: ReservedValue<CheckedNativeEnvironment>,
    manifest: IwsExecutionManifestV1,
}
fn put(objects: &mut Objects, bytes: Vec<u8>) {
    let id = *blake3::hash(&bytes).as_bytes();
    let charge = objects
        .memory
        .reserve(bytes.capacity() + 128, MemoryClass::Ordinary)
        .unwrap();
    objects.values.insert(id, Arc::new(charge.retain(bytes)));
}
fn install(pages: &mut Pages, delta: &ArtifactCatalogDeltaV1) {
    for node in &delta.new_nodes {
        let charge = pages
            .memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
}
fn material(change: impl FnOnce(&mut IwsExecutionManifestV1)) -> Material {
    material_with_policy(change, None)
}
fn material_with_policy(
    change: impl FnOnce(&mut IwsExecutionManifestV1),
    policy_override: Option<[u8; 32]>,
) -> Material {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (native, raw) = fixture::incremental::native_fixture();
    let descriptor = native.encode().unwrap();
    let native_id = *blake3::hash(&descriptor).as_bytes();
    let mut pages = Pages {
        memory: memory.clone(),
        values: BTreeMap::new(),
        reads: Cell::new(0),
        class: MemoryClass::Ordinary,
        _metadata: memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap(),
    };
    let wasm = b"\0asm\x01\0\0\0".to_vec();
    let policy = b"original-policy".to_vec();
    let entry = |kind, bytes: &[u8]| ArtifactEntryV1 {
        kind,
        original: OriginalArtifactRefV1 {
            id: *blake3::hash(bytes).as_bytes(),
            byte_len: bytes.len() as u64,
        },
    };
    let wasm_delta = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Wasm, &wasm)],
            &pages,
            &memory,
            4 << 20,
        )
        .unwrap();
    let rules_delta = ArtifactCatalogV1::empty(ArtifactKindV1::Rules)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Rules, &policy)],
            &pages,
            &memory,
            4 << 20,
        )
        .unwrap();
    install(&mut pages, &wasm_delta);
    install(&mut pages, &rules_delta);
    let config = fixture::incremental::genesis_config(native_id);
    let mut manifest = IwsExecutionManifestV1 {
        scope: config.scope().unwrap(),
        format_profile: config.format_profile,
        native: native_id,
        executable: hex::decode(&native.components[0].content_digest)
            .unwrap()
            .try_into()
            .unwrap(),
        wasm: wasm_delta.after.digest().unwrap(),
        rules: rules_delta.after.digest().unwrap(),
        policy: entry(ArtifactKindV1::Rules, &policy).original,
        mode: ExecutionManifestModeV1::Genesis,
    };
    change(&mut manifest);
    let manifest_id = manifest.digest().unwrap();
    let s = fixture::incremental::setup_with_execution_artifacts(
        memory,
        native_id,
        manifest_id,
        wasm_delta.after.digest().unwrap(),
        rules_delta.after.digest().unwrap(),
        policy_override.unwrap_or(entry(ArtifactKindV1::Rules, &policy).original.id),
    );
    let (mut objects, mut structural) = sources(&s, &s.history);
    structural.values.append(&mut pages.values);
    let pages = structural;
    for bytes in raw.into_iter().chain([
        descriptor,
        wasm,
        policy,
        manifest.canonical_bytes().unwrap(),
        wasm_delta.after.canonical_bytes().unwrap(),
        rules_delta.after.canonical_bytes().unwrap(),
    ]) {
        put(&mut objects, bytes);
    }
    let candidate = candidate(&s, s.commit.record().clone());
    let core = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    let mut quota = budget();
    quota.original_bytes_remaining = 4 << 20;
    let native =
        verify_native_environment(&core, &mut objects, &s.memory, &mut quota, || false).unwrap();
    Material {
        s,
        objects,
        pages,
        core,
        native,
        manifest,
    }
}
fn quota(m: &Material) -> ExecutionArtifactBudget {
    let refs = m.core.execution_artifacts();
    let bytes = [refs.manifest, refs.wasm, refs.rules]
        .iter()
        .map(|id| m.objects.values[id].len() as u64)
        .sum();
    ExecutionArtifactBudget {
        structural: CoreBudget {
            objects_remaining: 3,
            original_bytes_remaining: bytes,
            new_pages_remaining: 0,
            merkle_reads_remaining: 771,
        },
        originals: OriginalArtifactBudget {
            objects_remaining: 2,
            bytes_remaining: 8 + m.manifest.policy.byte_len,
            max_object_bytes: 1024,
        },
    }
}
#[test]
fn exact_manifest_catalog_and_original_allowances_bind_to_original_signed_core() {
    let mut m = material(|_| {});
    let mut quota = quota(&m);
    let before = m.s.memory.used();
    let checked = verify_execution_artifacts(
        &m.core,
        &m.native,
        ExecutionArtifactReadContext {
            objects: &mut m.objects,
            pages: &m.pages,
            memory: &m.s.memory,
        },
        &mut quota,
        || false,
    )
    .unwrap();
    assert_eq!(checked.body_id(), m.s.history.body.digest().unwrap());
    assert_eq!(checked.manifest_id(), m.core.execution_artifacts().manifest);
    assert_eq!(checked.catalog_counts(), [1, 1]);
    assert_eq!(checked.original_bytes(), 8 + m.manifest.policy.byte_len);
    assert_eq!(
        (
            quota.structural.objects_remaining,
            quota.structural.original_bytes_remaining,
            quota.structural.merkle_reads_remaining,
            quota.originals.objects_remaining,
            quota.originals.bytes_remaining
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(m.s.memory.used(), before + checked.reserved_bytes());
    drop(checked);
    assert_eq!(m.s.memory.used(), before);
}
#[test]
fn signed_manifest_mismatches_do_not_pass_opaque_original_hash_checks() {
    for mode in 0..6 {
        let mut m = material(|manifest| match mode {
            0 => manifest.wasm = [99; 32],
            1 => manifest.executable = [99; 32],
            2 => manifest.scope.world_id = "wrong".into(),
            3 => manifest.policy.byte_len += 1,
            4 => manifest.policy.id = [99; 32],
            _ => {
                manifest.mode = ExecutionManifestModeV1::Execution {
                    parent_commit: [1; 32],
                    parent_roots: [2; 32],
                    operation: [3; 32],
                    input: [4; 32],
                    continuation: OriginalArtifactRefV1 {
                        id: [5; 32],
                        byte_len: 1,
                    },
                }
            }
        });
        let mut quota = quota(&m);
        assert!(
            verify_execution_artifacts(
                &m.core,
                &m.native,
                ExecutionArtifactReadContext {
                    objects: &mut m.objects,
                    pages: &m.pages,
                    memory: &m.s.memory
                },
                &mut quota,
                || false
            )
            .is_err()
        );
    }
}
#[test]
fn missing_data_or_page_and_every_one_less_allowance_fail_closed() {
    for mode in 0..8 {
        let mut m = material(|_| {});
        let mut quota = quota(&m);
        if mode == 0 {
            quota.structural.objects_remaining -= 1;
        }
        if mode == 1 {
            quota.structural.original_bytes_remaining -= 1;
        }
        if mode == 2 {
            quota.structural.merkle_reads_remaining -= 1;
        }
        if mode == 3 {
            quota.originals.objects_remaining -= 1;
        }
        if mode == 4 {
            quota.originals.bytes_remaining -= 1;
        }
        if mode == 5 {
            let id = *blake3::hash(b"\0asm\x01\0\0\0").as_bytes();
            m.objects.values.remove(&id);
        }
        if mode == 6 {
            m.pages.values.clear();
        }
        if mode == 7 {
            let id = *blake3::hash(b"\0asm\x01\0\0\0").as_bytes();
            let charge = m.s.memory.reserve(64, MemoryClass::Ordinary).unwrap();
            m.objects
                .values
                .insert(id, Arc::new(charge.retain(vec![9; 8])));
        }
        let before = m.s.memory.used();
        assert!(
            verify_execution_artifacts(
                &m.core,
                &m.native,
                ExecutionArtifactReadContext {
                    objects: &mut m.objects,
                    pages: &m.pages,
                    memory: &m.s.memory
                },
                &mut quota,
                || false
            )
            .is_err()
        );
        assert_eq!(m.s.memory.used(), before);
    }
}
#[test]
fn nonempty_execution_manifest_binds_parent_input_and_original_continuation() {
    let mut m = material(|_| {});
    let continuation = b"original continuation".to_vec();
    let reference = OriginalArtifactRefV1 {
        id: *blake3::hash(&continuation).as_bytes(),
        byte_len: continuation.len() as u64,
    };
    put(&mut m.objects, continuation);
    let mut manifest = m.manifest.clone();
    manifest.mode = ExecutionManifestModeV1::Execution {
        parent_commit: m.s.history.body.digest().unwrap(),
        parent_roots: m.s.roots.digest().unwrap(),
        operation: [60; 32],
        input: [61; 32],
        continuation: reference,
    };
    let id = manifest.digest().unwrap();
    put(&mut m.objects, manifest.canonical_bytes().unwrap());
    let (mut current_objects, mut pages, candidate, _) =
        execution_sources_with_manifest(&m.s, [9; 32], id);
    // Preserve exact new execution structural originals over shared genesis data.
    m.objects.values.append(&mut current_objects.values);
    pages.values.append(&mut m.pages.values);
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &m.s.authority,
            parent: Some(&m.s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut m.objects,
        &pages,
        &m.s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    let native =
        verify_native_environment(&core, &mut m.objects, &m.s.memory, &mut budget(), || false)
            .unwrap();
    let mut quota = ExecutionArtifactBudget {
        structural: CoreBudget {
            objects_remaining: 3,
            original_bytes_remaining: 8192,
            new_pages_remaining: 0,
            merkle_reads_remaining: 10000,
        },
        originals: OriginalArtifactBudget {
            objects_remaining: 3,
            bytes_remaining: 1024,
            max_object_bytes: 1024,
        },
    };
    let checked = verify_execution_artifacts(
        &core,
        &native,
        ExecutionArtifactReadContext {
            objects: &mut m.objects,
            pages: &pages,
            memory: &m.s.memory,
        },
        &mut quota,
        || false,
    )
    .unwrap();
    assert_eq!(
        checked.original_bytes(),
        8 + m.manifest.policy.byte_len + reference.byte_len
    );
}

#[test]
fn approved_opaque_manifest_cannot_substitute_the_original_protocol_policy() {
    let mut m = material_with_policy(|_| {}, Some([99; 32]));
    let mut quota = quota(&m);
    assert!(matches!(
        verify_execution_artifacts(
            &m.core,
            &m.native,
            ExecutionArtifactReadContext {
                objects: &mut m.objects,
                pages: &m.pages,
                memory: &m.s.memory
            },
            &mut quota,
            || false
        ),
        Err(CoreError::Invalid("execution manifest binding"))
    ));
}

#[test]
fn cancellation_pool_class_and_caught_stream_unwind_never_return_partial_marker() {
    let mut m = material(|_| {});
    let other = material(|_| {});
    for mode in 0..3 {
        if mode == 1 {
            m.pages.class = MemoryClass::Recovery;
        } else {
            m.pages.class = MemoryClass::Ordinary;
        }
        let mut quota = quota(&m);
        let reads = m.objects.reads;
        let native = if mode == 2 { &other.native } else { &m.native };
        assert!(
            verify_execution_artifacts(
                &m.core,
                native,
                ExecutionArtifactReadContext {
                    objects: &mut m.objects,
                    pages: &m.pages,
                    memory: &m.s.memory
                },
                &mut quota,
                || mode == 0
            )
            .is_err()
        );
        assert_eq!(m.objects.reads, reads);
    }
    struct Panics<'a>(&'a mut Objects);
    impl OriginalObjectReader for Panics<'_> {
        type Error = std::io::Error;
        fn allocation_budget(&self) -> &MemoryBudget {
            &self.0.memory
        }
        fn read_original(
            &mut self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
        ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
            self.0.read_original(id, max, memory)
        }
    }
    impl OriginalObjectStreamReader for Panics<'_> {
        fn stream_original(
            &mut self,
            _: &[u8; 32],
            _: u64,
            _: &MemoryBudget,
            _: &mut dyn Write,
        ) -> std::io::Result<u64> {
            panic!("artifact stream unwind");
        }
    }
    m.pages.class = MemoryClass::Ordinary;
    let mut quota = quota(&m);
    let before = m.s.memory.used();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verify_execution_artifacts(
                &m.core,
                &m.native,
                ExecutionArtifactReadContext {
                    objects: &mut Panics(&mut m.objects),
                    pages: &m.pages,
                    memory: &m.s.memory,
                },
                &mut quota,
                || false,
            )
        }))
        .is_err()
    );
    assert_eq!(quota.originals.objects_remaining, 1);
    assert_eq!(quota.originals.bytes_remaining, m.manifest.policy.byte_len);
    assert_eq!(quota.structural.merkle_reads_remaining, 514);
    assert_eq!(m.s.memory.used(), before);
}
