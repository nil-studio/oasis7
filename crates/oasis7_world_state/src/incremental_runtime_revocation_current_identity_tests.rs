use super::*;
#[test]
fn current_revocation_identity_reads_changed_body_and_real_absence_without_full_table() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = seed(&memory);
    let mut ri = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_runtime_revocation_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
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
            max_reads: 64,
            max_writes: 16,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut changed = crate::CapabilityRevocationState::default();
    changed.agent_identities.insert(
        "agent".into(),
        crate::CapabilityAgentIdentity {
            owner_binding: "owner".into(),
            generation: 7,
        },
    );
    changed
        .manufactured_identity_acquisition_sequences
        .insert("agent".into(), 11);
    let mut sink = Sink(&pages);
    let mut rows = Vec::new();
    prepare_runtime_revocation_genesis_v1(
        &changed,
        &mut MemberRecordWriteContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    for row in rows {
        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
    }
    let mut oi = io();
    let mut ok = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut pi = io();
    let mut pk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut parent_read = context(&pages, &roots, &memory, &mut pi, &mut pk);
    let mut overlay = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 1024,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut work = read_work();
    let mut selected_scan = scan();
    for agent in ["agent", "missing"] {
        let guard = overlay.transaction.guard();
        let selected = read_current_revocation_agent_identity_v1(
            agent,
            &proof,
            &mut overlay,
            &mut parent_read,
            &mut selected_scan,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        assert!(selected.uses_budget(&memory));
        if agent == "agent" {
            assert_eq!(selected.identity(), changed.agent_identities.get(agent));
            assert_eq!(selected.acquisition_sequence(), Some(11));
            assert_eq!(selected.manufactured_generation(13), Some(9));
            assert_eq!(selected.manufactured_generation(10), None);
            assert_eq!(selected.manufactured_generation(0), None);
        } else {
            assert!(selected.identity().is_none());
            assert!(selected.acquisition_sequence().is_none());
            assert_eq!(selected.manufactured_generation(13), Some(13));
        }
    }
    let guard = overlay.transaction.guard();
    drop(overlay);
    tx.seal(&guard, &live, 1).unwrap();
}

#[test]
fn current_revocation_identity_foreign_corrupt_exhausted_and_cancelled_queries_poison() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = seed(&memory);
    let mut qi = io();
    let mut qk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_runtime_revocation_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut qi, &mut qk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    let mut foreign_roots = roots.clone();
    foreign_roots.height += 1;
    let mut fi = io();
    let mut fk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let foreign_proof = qualify_runtime_revocation_v1(
        &foreign_roots,
        &mut context(&pages, &foreign_roots, &memory, &mut fi, &mut fk),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    // Reuse one small real GEN for independent transaction failure lanes.
    for mode in 0..6 {
        let parent = ExecutionParent {
            roots: if mode == 1 {
                foreign_roots.clone()
            } else {
                roots.clone()
            },
            commit: [2; 32],
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 4,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [5; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        if mode == 2 || mode == 3 {
            let field = crate::incremental_runtime_revocation::family::outer();
            let mut ti = io();
            let mut tk = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut read = context(&pages, &roots, &memory, &mut ti, &mut tk);
            let key = if mode == 2 {
                crate::incremental_world_field_key::WorldFieldKeyV1::new(
                    field,
                    crate::incremental_world_field_key::WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap()
            } else {
                read.records()
                    .key(
                        field,
                        crate::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
                        &(),
                    )
                    .unwrap()
                    .0
            };
            tx.write(
                StateNamespaceV1::Authorization,
                key.as_bytes(),
                if mode == 2 { None } else { Some([255; 32]) },
            )
            .unwrap();
        }
        let polls = std::cell::Cell::new(0usize);
        let mut oi = io();
        let mut ok = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut pi = io();
        let mut pk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut parent_read = context(&pages, &roots, &memory, &mut pi, &mut pk);
        let mut overlay = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut oi,
            },
            key_work: &mut ok,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || {
                let n = polls.get() + 1;
                polls.set(n);
                mode == 5 && n > 3
            },
        };
        let mut work = read_work();
        let mut selected_scan = scan();
        if mode == 4 {
            let guard = overlay.transaction.guard();
            read_current_revocation_agent_identity_v1(
                "missing",
                &proof,
                &mut overlay,
                &mut parent_read,
                &mut selected_scan,
                &mut work,
                &guard,
                &live,
            )
            .unwrap();
            assert!(work.json.cells_remaining < read_work().json.cells_remaining);
            work.json.cells_remaining = 0;
        }
        let guard = overlay.transaction.guard();
        let result = read_current_revocation_agent_identity_v1(
            "missing",
            if mode == 0 { &foreign_proof } else { &proof },
            &mut overlay,
            &mut parent_read,
            &mut selected_scan,
            &mut work,
            &guard,
            &live,
        );
        assert!(result.is_err(), "mode {mode}");
        if mode == 5 {
            assert!(matches!(
                result,
                Err(
                    crate::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                        ValueTreeError::Cancelled
                    )
                )
            ));
            assert!(polls.get() > 3);
        }
        let guard = overlay.transaction.guard();
        drop(overlay);
        assert!(
            matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted)),
            "mode {mode}"
        );
    }
}

#[test]
fn manufactured_generation_pure_kernel_preserves_arithmetic_boundaries() {
    let identity = crate::CapabilityAgentIdentity {
        owner_binding: "owner".into(),
        generation: u64::MAX,
    };
    assert_eq!(
        resolve_manufactured_identity_generation_v1(Some(&identity), Some(7), 8),
        None
    );
    assert_eq!(
        resolve_manufactured_identity_generation_v1(Some(&identity), Some(7), 6),
        None
    );
    assert_eq!(
        resolve_manufactured_identity_generation_v1(Some(&identity), Some(7), 7),
        Some(u64::MAX)
    );
    assert_eq!(
        resolve_manufactured_identity_generation_v1(None, Some(7), 7),
        None
    );
    assert_eq!(
        resolve_manufactured_identity_generation_v1(None, None, 0),
        None
    );
    assert_eq!(
        resolve_manufactured_identity_generation_v1(None, None, u64::MAX),
        Some(u64::MAX)
    );
}
