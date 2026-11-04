use super::*;
#[test]
fn typed_path_epoch_updates_replay_and_failures_poison() {
    let noop_polls = std::cell::Cell::new(0);
    for case in [5, 0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11] {
        let polls = std::cell::Cell::new(0);
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _source = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let mut source = crate::CapabilityRevocationState::default();
        source.world_service_results.insert(
            "unchanged".into(),
            serde_json::Value::String("large-history".repeat(32_768)),
        );
        let (pages, roots) = seed_value(&memory, &source);
        let mut ri = io();
        let mut rk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut qualification_read = context(&pages, &roots, &memory, &mut ri, &mut rk);
        qualification_read.record_bytes = 3 << 20;
        let proof = qualify_runtime_revocation_v1(
            &roots,
            &mut qualification_read,
            &mut scan(),
            &mut read_work(),
        )
        .unwrap();
        proof.validate_binding(&roots).unwrap();
        let mut wrong = roots.clone();
        wrong.height += 1;
        assert!(proof.validate_binding(&wrong).is_err());
        assert!(readmit_runtime_revocation_ordinary_v1(&proof, &roots, &memory, || false).is_err());
        let mut sink = Sink(&pages);
        let mut after_dto = source.clone();
        after_dto.epoch = if case >= 10 { 78 } else { 77 };
        if case >= 10 {
            after_dto.system_identities.insert("_length".into(), 7);
        }
        let after = prepare_runtime_revocation_body_v1(
            &after_dto,
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
        )
        .unwrap();
        if case == 0 {
            // Component inheritance must not revisit unchanged JSON payload.
            // The production physical lease/WorldReady boundary remains separate.
            let literal = pages
                .nodes
                .borrow()
                .iter()
                .find_map(|(id, bytes)| {
                    bytes
                        .windows(b"large-history".len())
                        .any(|w| w == b"large-history")
                        .then_some(*id)
                })
                .expect("unique large JSON literal page");
            pages.nodes.borrow_mut().remove(&literal);
        }
        let parent = ExecutionParent {
            commit: [2; 32],
            roots: roots.clone(),
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 4,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        tx.write(StateNamespaceV1::Scheduler, b"outer-prefix", Some([9; 32]))
            .unwrap();
        let initial_guard = tx.guard();
        let mut read_io = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut overlay_io = io();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut write_io = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut current = begin_runtime_revocation_current_v1(
            &proof,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut overlay_io,
                },
                key_work: &mut overlay_keys,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut context(&pages, &roots, &memory, &mut read_io, &mut read_keys),
            &mut scan(),
            &mut read_work(),
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &initial_guard,
            &live,
        )
        .unwrap();
        let guard = if case == 1 { initial_guard } else { tx.guard() };
        let mut epoch_entry = None;
        StateScanContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut scan(),
        }
        .visit_entries(
            proof.body(),
            |entry| {
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == WorldFieldRecordKindV1::Entry {
                    if parsed.semantic_key
                        == Some(
                            crate::incremental_semantic_key::hash_semantic_key_v1(
                                "epoch",
                                &memory,
                                MemoryClass::Ordinary,
                                &mut SemanticKeyBudget {
                                    bytes_remaining: 1024,
                                },
                                || false,
                            )
                            .unwrap()
                            .digest,
                        )
                    {
                        epoch_entry = Some(entry.value);
                    }
                }
                Ok::<(), ValueTreeError>(())
            },
            || false,
        )
        .unwrap();
        let mut epoch_key = crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN.to_vec();
        epoch_key.push(1);
        epoch_key.extend_from_slice(&5u64.to_be_bytes());
        epoch_key.extend_from_slice(b"epoch");
        let mut unknown_key = crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN.to_vec();
        unknown_key.push(1);
        unknown_key.extend_from_slice(&7u64.to_be_bytes());
        unknown_key.extend_from_slice(b"unknown");
        let path = [if case == 9 {
            unknown_key.as_slice()
        } else {
            epoch_key.as_slice()
        }];
        let edits = [RuntimeRevocationPathEditV1 {
            path: &path,
            expected: if case == 6 {
                Some([255; 32])
            } else if case == 9 {
                None
            } else {
                epoch_entry
            },
            after: if case == 8 {
                None
            } else if case == 7 {
                Some(MemberPayloadRefV1::Bool(true))
            } else {
                Some(MemberPayloadRefV1::U64(if (3..=5).contains(&case) {
                    0
                } else {
                    77
                }))
            },
        }];
        let mut system_key = Vec::new();
        let mut length_key = Vec::new();
        let mut member_key = Vec::new();
        crate::incremental_semantic_key::write_semantic_key_v1(
            "system_identities",
            &mut system_key,
        )
        .unwrap();
        crate::incremental_semantic_key::write_semantic_key_v1("_length", &mut length_key).unwrap();
        crate::incremental_semantic_key::write_semantic_key_v1(
            &("entry", "_length"),
            &mut member_key,
        )
        .unwrap();
        let length_path = [system_key.as_slice(), length_key.as_slice()];
        let member_path = [system_key.as_slice(), member_key.as_slice()];
        let old_length = write_member_record_v1(
            field(),
            WorldFieldRecordKindV1::Entry,
            "_length",
            MemberPayloadRefV1::U64(0),
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
        )
        .unwrap()
        .value_root;
        let nested_edits = [
            RuntimeRevocationPathEditV1 {
                path: &length_path,
                expected: Some(old_length),
                after: Some(MemberPayloadRefV1::U64(1)),
            },
            RuntimeRevocationPathEditV1 {
                path: &member_path,
                expected: None,
                after: Some(MemberPayloadRefV1::U64(7)),
            },
        ];
        let batch = if case == 10 {
            &nested_edits[..]
        } else if case == 11 {
            &nested_edits[1..]
        } else {
            &edits[..]
        };
        let mut changed_work = read_work();
        changed_work.max_tree_heap = 16 << 10;
        let result = patch_runtime_revocation_overlay_v1(
            &proof,
            &mut current,
            batch,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut overlay_io,
                },
                key_work: &mut overlay_keys,
                semantic_bytes: 1024,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || {
                    polls.set(polls.get() + 1);
                    case == 2 || (case == 3 && polls.get() >= noop_polls.get())
                },
            },
            &mut WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Authorization),
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 1024,
                record_bytes: 3 << 20,
                take_state_error: || {
                    if case == 4 {
                        Some(ValueTreeError::ResourceLimited("pending source failure"))
                    } else {
                        None
                    }
                },
                cancelled: || false,
            },
            &mut scan(),
            &mut changed_work,
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            },
            &mut write_work(),
            &guard,
            &live,
        );
        if case == 5 {
            assert!(!result.unwrap());
            assert!(polls.get() > 1);
            noop_polls.set(polls.get());
            continue;
        }
        if case != 0 && case != 10 {
            assert!(result.is_err(), "case {case}");
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
            continue;
        }
        assert!(result.unwrap());
        if case == 10 {
            let second = [RuntimeRevocationPathEditV1 {
                path: &path,
                expected: epoch_entry,
                after: Some(MemberPayloadRefV1::U64(78)),
            }];
            let guard = tx.guard();
            assert!(
                patch_runtime_revocation_overlay_v1(
                    &proof,
                    &mut current,
                    &second,
                    &mut WorldFieldOverlayContextV1 {
                        transaction: &mut tx,
                        values: ValueReadContext {
                            source: &pages,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut overlay_io
                        },
                        key_work: &mut overlay_keys,
                        semantic_bytes: 1024,
                        record_bytes: 3 << 20,
                        take_state_error: || None,
                        cancelled: || false,
                    },
                    &mut WorldFieldReadContextV1 {
                        root: roots.state_root(StateNamespaceV1::Authorization),
                        state: &pages,
                        values: ValueReadContext {
                            source: &pages,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut read_io
                        },
                        key_work: &mut read_keys,
                        semantic_bytes: 1024,
                        record_bytes: 3 << 20,
                        take_state_error: || None,
                        cancelled: || false,
                    },
                    &mut scan(),
                    &mut changed_work,
                    &mut MemberRecordWriteContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut write_io,
                        key_work: &mut write_keys,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 2 << 20,
                            record_bytes: 3 << 20
                        },
                        cancelled: || false,
                    },
                    &mut write_work(),
                    &guard,
                    &live,
                )
                .unwrap()
            );
        }
        assert_eq!(current.body(), &*after);
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        for node in &sealed.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            sink.put_node(id, charge.retain(node.bytes.clone()))
                .unwrap();
        }
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let mut vi = io();
        let mut vk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let next = accept_runtime_revocation_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut context(&pages, &sealed.record.after, &memory, &mut vi, &mut vk),
            &mut scan(),
            &mut changed_work,
        )
        .unwrap();
        next.validate_binding(&sealed.record.after).unwrap();
        assert_eq!(next.body(), &*after);
    }
}
