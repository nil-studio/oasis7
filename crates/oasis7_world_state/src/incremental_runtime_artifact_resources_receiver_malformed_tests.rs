use super::*;
fn delete<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    scope: &Scope<'_, Pages>,
    root: &StateRootV1,
    record: RuntimeArtifactRecordV1,
    kind: WorldFieldRecordKindV1,
    key: &K,
) -> StateRootV1 {
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root,
        state: scope.pages,
        values: ValueReadContext {
            source: scope.pages,
            memory: scope.memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let path = if kind == WorldFieldRecordKindV1::Metadata {
        WorldFieldKeyV1::new(record.descriptor(), kind, None).unwrap()
    } else {
        ctx.key(record.descriptor(), kind, key).unwrap().0
    };
    let old = ctx.lookup(&path).unwrap().unwrap();
    let delta = StateDeltaV1::build(
        root.clone(),
        vec![StateWriteV1 {
            key: path.as_bytes().to_vec(),
            before: Some(old),
            after: None,
        }],
        scope.pages,
        8 << 20,
    )
    .unwrap();
    let mut out = Writer(scope.pages);
    for n in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes();
        out.put_node(
            id,
            scope
                .memory
                .reserve(n.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(n.bytes.clone()),
        )
        .unwrap();
    }
    delta.after
}
fn malformed(mode: u8) {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut manifest = ChainResourceManifest::default();
    let mut delta = ChainResourceDelta::default();
    delta.entries = vec![
        ChainResourceDeltaEntry::RuntimeResource {
            kind: ResourceKind::Electricity,
            delta: 1,
            resulting_balance: 1,
        },
        ChainResourceDeltaEntry::MaterialLedger {
            ledger_id: MaterialLedgerId::World,
            material_kind: "iron".into(),
            delta: 1,
            resulting_balance: 1,
        },
        ChainResourceDeltaEntry::RuntimeResource {
            kind: ResourceKind::Data,
            delta: 1,
            resulting_balance: 1,
        },
    ];
    if matches!(mode, 13 | 14) {
        manifest
            .generated_chunks
            .insert("chunk".into(), fixture_chunk());
    }
    if mode == 15 {
        delta.entries[2] = ChainResourceDeltaEntry::ChunkResource {
            coord: ChunkCoord { x: 1, y: 2, z: 3 },
            element: FragmentElementKind::Iron,
            total_delta_g: 1,
            remaining_delta_g: 1,
            resulting_remaining_g: 1,
            remaining_after_hash: "remaining".into(),
            chunk_remaining_after_hash: "chunk".into(),
        };
    }
    let builtin = Builtins(Default::default());
    let (pages, roots) = seed(&memory, &manifest, Some(&delta), &builtin);
    let family = qualify(&pages, &roots, &memory).unwrap();
    let scope = Scope {
        pages: &pages,
        memory: &memory,
        values: &pages,
    };
    let field = if matches!(mode, 7 | 8 | 15) {
        RuntimeArtifactFieldV1::LatestChainResourceDelta
    } else {
        RuntimeArtifactFieldV1::ChainResourceManifest
    };
    let record = if field == RuntimeArtifactFieldV1::LatestChainResourceDelta {
        RuntimeArtifactRecordV1::ChainResourceDelta
    } else {
        RuntimeArtifactRecordV1::ChainResourceManifest
    };
    let original = scope.field(&roots, field).unwrap();
    let mut body = original.root().clone();
    match mode {
        0 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &"unknown",
                MemberPayloadRefV1::U64(1),
            )
        }
        1 => {
            body = delete(
                &scope,
                &body,
                record,
                WorldFieldRecordKindV1::Entry,
                &"world_id",
            )
        }
        2 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("genesis_ref", "value"),
                MemberPayloadRefV1::Text("orphan"),
            )
        }
        3 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &"manifest_height",
                MemberPayloadRefV1::Bool(true),
            )
        }
        4 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("resource_balances", ResourceKind::Electricity),
                MemberPayloadRefV1::I64(1),
            )
        }
        5 => {}
        6 => body = StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        7 => {
            let material = scope.child(
                record,
                RuntimeArtifactRecordV1::DeltaMaterialLedger,
                &original,
                &("entries", 1u64),
            );
            let bad = rewrite(
                &memory,
                &pages,
                material.root(),
                RuntimeArtifactRecordV1::DeltaMaterialLedger,
                &"variant",
                MemberPayloadRefV1::U64(0),
            );
            let reference = root_ref(
                RuntimeArtifactRecordV1::DeltaMaterialLedger.schema(),
                &("entries", 1u64),
                bad,
                &memory,
            );
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("entries", 1u64),
                MemberPayloadRefV1::RecordRoot(&reference),
            );
        }
        8 => {
            body = delete(
                &scope,
                &body,
                record,
                WorldFieldRecordKindV1::Entry,
                &("entries", 1u64),
            );
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &"entries.count",
                MemberPayloadRefV1::U64(2),
            );
        }
        9 => body = delete(&scope, &body, record, WorldFieldRecordKindV1::Metadata, &()),
        10 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &"unknown",
                MemberPayloadRefV1::U64(1),
            );
            body = delete(&scope, &body, record, WorldFieldRecordKindV1::Metadata, &());
        }
        11 => {
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("genesis_ref", "present"),
                MemberPayloadRefV1::Bool(true),
            )
        }
        12 => {}
        13 | 14 => {
            let chunk = scope.child(
                record,
                RuntimeArtifactRecordV1::ChunkManifest,
                &original,
                &("generated_chunks", "chunk"),
            );
            let (name, payload) = if mode == 13 {
                (
                    "fragment_count",
                    MemberPayloadRefV1::U64(u64::from(u32::MAX) + 1),
                )
            } else {
                ("coord.x", MemberPayloadRefV1::I64(i64::from(i32::MAX) + 1))
            };
            let bad = rewrite(
                &memory,
                &pages,
                chunk.root(),
                RuntimeArtifactRecordV1::ChunkManifest,
                &name,
                payload,
            );
            let reference = root_ref(
                RuntimeArtifactRecordV1::ChunkManifest.schema(),
                &("generated_chunks", "chunk"),
                bad,
                &memory,
            );
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("generated_chunks", "chunk"),
                MemberPayloadRefV1::RecordRoot(&reference),
            );
        }
        15 => {
            let chunk = scope.child(
                record,
                RuntimeArtifactRecordV1::DeltaChunkResource,
                &original,
                &("entries", 2u64),
            );
            let bad = rewrite(
                &memory,
                &pages,
                chunk.root(),
                RuntimeArtifactRecordV1::DeltaChunkResource,
                &"coord.x",
                MemberPayloadRefV1::I64(i64::from(i32::MAX) + 1),
            );
            let reference = root_ref(
                RuntimeArtifactRecordV1::DeltaChunkResource.schema(),
                &("entries", 2u64),
                bad,
                &memory,
            );
            body = rewrite(
                &memory,
                &pages,
                &body,
                record,
                &("entries", 2u64),
                MemberPayloadRefV1::RecordRoot(&reference),
            );
        }
        _ => unreachable!(),
    }
    let p = parent(&roots);
    let live = p.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(p.clone(), [7; 32], &pages, &memory, limits()).unwrap();
    if mode == 5 {
        let key = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .unwrap();
        tx.write(StateNamespaceV1::Artifacts, key.as_bytes(), None)
            .unwrap();
    } else if mode != 12 {
        let reference = root_ref(field.schema(), &(), body, &memory);
        let mut out = Writer(&pages);
        let mut vi = io();
        let mut kw = keys();
        let row = write_member_record_v1(
            field.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut out,
                memory: &memory,
                io: &mut vi,
                key_work: &mut kw,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 2 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 4 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        tx.write(
            StateNamespaceV1::Artifacts,
            row.field_key.as_bytes(),
            Some(row.value_root),
        )
        .unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    TransitionVerifier {
        parent: &p,
        operation: [7; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    }
    .verify(&sealed.record)
    .expect("malformed data must be a valid generic transition");
    let mut other = roots.clone();
    other.height += 1;
    let wrong = if mode == 12 {
        Some(qualify(&pages, &other, &memory).unwrap())
    } else {
        None
    };
    let result = scope.accept(
        wrong.as_ref().unwrap_or(&family),
        &p,
        &sealed.record,
        &mut scans(),
        4096,
    );
    assert!(
        matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
        ),
        "mode {mode} must fail typed closure, not source IO or quota"
    );
    if mode >= 13 {
        let after = scope.publish(&p, &sealed.record);
        assert!(matches!(
            qualify(&pages, &after, &memory),
            Err(ValueTreeError::Invalid(_))
        ));
    }
}
#[test]
fn resource_receiver_rejects_unknown_mandatory_codec_and_count_changes() {
    for mode in [0, 1, 3, 4, 9, 10] {
        malformed(mode)
    }
}
#[test]
fn resource_receiver_rejects_option_shape_and_metadata_changes() {
    for mode in [2, 5, 6, 11, 12] {
        malformed(mode)
    }
}
#[test]
fn resource_receiver_rejects_wrong_variant_and_ordinal_hole() {
    for mode in [7, 8] {
        malformed(mode)
    }
}
#[test]
fn resource_receiver_preserves_scan_limit_cancel_original_error_and_phase_budget() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut manifest = ChainResourceManifest::default();
    let builtin = Builtins(Default::default());
    let (pages, roots) = seed(&memory, &manifest, None, &builtin);
    let family = qualify(&pages, &roots, &memory).unwrap();
    let scope = Scope {
        pages: &pages,
        memory: &memory,
        values: &pages,
    };
    let current = scope
        .field(&roots, RuntimeArtifactFieldV1::ChainResourceManifest)
        .unwrap();
    manifest
        .resource_balances
        .insert(ResourceKind::Electricity, 1);
    let (p, sealed) = scope.body(
        &roots,
        &family,
        RuntimeArtifactBodyChangeV1::ReplaceManifest {
            expected_root: digest(current.root()),
            after: &manifest,
        },
    );
    let verifier = TransitionVerifier {
        parent: &p,
        operation: [7; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    verifier.verify(&sealed.record).unwrap();
    let mut zero = std::array::from_fn(|_| StateScanBudget {
        nodes_remaining: 0,
        leaves_remaining: 0,
        read_bytes_remaining: 0,
    });
    assert!(matches!(
        scope.accept(&family, &p, &sealed.record, &mut zero, 4096),
        Err(WorldFieldOverlayErrorV1::Value(
            ValueTreeError::ResourceLimited(_)
        ))
    ));
    let mut vi = io();
    let mut kw = keys();
    let result = accept_runtime_artifact_resources_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[5],
            state: &pages,
            values: ValueReadContext {
                source: &Unreadable,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || true,
        },
        &mut scans(),
        4096,
        &mut work(),
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    ));
    let mut vi = io();
    let mut kw = keys();
    let result = accept_runtime_artifact_resources_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[5],
            state: &pages,
            values: ValueReadContext {
                source: &Unreadable,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || Some(ValueTreeError::Invalid("original resource source failure")),
            cancelled: || false,
        },
        &mut scans(),
        4096,
        &mut work(),
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
            "original resource source failure"
        )))
    ));
    let before = memory.used();
    let fill = memory
        .reserve(
            (192 << 20) - (16 << 20) - before - 64,
            MemoryClass::Ordinary,
        )
        .unwrap();
    assert!(matches!(
        scope.accept(&family, &p, &sealed.record, &mut scans(), 4096),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Memory(_)))
    ));
    drop(fill);
    assert_eq!(memory.used(), before);
}

#[test]
fn resource_receiver_and_cold_reject_original_numeric_type_overflows() {
    for mode in 13..16 {
        malformed(mode)
    }
}
