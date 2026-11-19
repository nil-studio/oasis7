use super::super::super::tests::with_store_for_bounded_index_fixture;
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(2_000_000, 256 << 20).unwrap()
}
fn inputs() -> (String, String) {
    ("12".repeat(32), "34".repeat(32))
}

#[test]
fn real_store_stages_original_leaf_without_publishing_and_holds_retirement() {
    with_store_for_bounded_index_fixture(|store, _| {
        let (request, entry) = inputs();
        let expected = serde_cbor::to_vec(&Index::Leaf {
            request: request.clone(),
            entry: entry.clone(),
        })
        .unwrap();
        let hot_before = serde_cbor::to_vec(&store.body).unwrap();
        let memory = memory();
        let meter = work();
        let stage = store
            .stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        assert_eq!(stage.bytes(), expected);
        assert_eq!(stage.id(), blake3::hash(&expected).to_hex().as_str());
        assert!(stage.uses_budget(&memory));
        assert_eq!(stage.reserved_bytes(), memory.used());
        assert!(store.lookup(&request).unwrap().is_none());
        assert_eq!(serde_cbor::to_vec(&store.body).unwrap(), hot_before);
        let directory = store.path.join("formal-objects-v1");
        assert!(physical::pin(&directory, true).is_err());
        let on_disk: Index = store.get(stage.id()).unwrap();
        assert_eq!(serde_cbor::to_vec(&on_disk).unwrap(), expected);
        drop(stage);
        assert_eq!(memory.used(), 0);
        drop(physical::pin(&directory, true).unwrap());
        // A retry uses the actual durable local reader and preserves old bytes.
        let stage = store
            .stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        assert_eq!(stage.bytes(), expected);
        assert_eq!(fs::read(directory.join(stage.id())).unwrap(), expected);
        drop(stage);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn damaged_existing_content_is_not_overwritten_or_published() {
    with_store_for_bounded_index_fixture(|store, _| {
        let (request, entry) = inputs();
        let expected = serde_cbor::to_vec(&Index::Leaf {
            request: request.clone(),
            entry: entry.clone(),
        })
        .unwrap();
        let id = blake3::hash(&expected).to_hex().to_string();
        let path = store.path.join("formal-objects-v1").join(&id);
        fs::write(&path, b"damaged original").unwrap();
        let memory = memory();
        let meter = work();
        assert!(
            store
                .stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || false)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"damaged original");
        assert!(store.lookup(&request).unwrap().is_none());
        assert_eq!(memory.used(), 0);
        assert!(meter.remaining_steps() < 2_000_000);
    });
}

#[test]
fn caller_budget_and_cancellation_fail_closed_and_release_fees() {
    for mode in 0..5 {
        with_store_for_bounded_index_fixture(|store, _| {
            let (request, entry) = inputs();
            let memory = if mode == 0 {
                MemoryBudget::new(1, 0).unwrap()
            } else {
                memory()
            };
            let meter = FormalArchiveReadWorkV1::new(
                if mode == 1 { 0 } else { 2_000_000 },
                if mode == 2 { 0 } else { 256 << 20 },
            )
            .unwrap();
            let mut calls = 0;
            let result =
                store.stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || {
                    calls += 1;
                    mode == 3 || (mode == 4 && calls >= 15)
                });
            assert!(result.is_err(), "failure mode {mode}");
            drop(result);
            assert_eq!(memory.used(), 0, "failure mode {mode}");
            assert!(store.lookup(&request).unwrap().is_none());
        });
    }
}

#[test]
fn original_two_hash_admission_and_missing_guard_are_enforced() {
    with_store_for_bounded_index_fixture(|store, _| {
        let (request, entry) = inputs();
        let memory = memory();
        for (r, e) in [("bad", entry.as_str()), (request.as_str(), "bad")] {
            assert!(
                store
                    .stage_index_leaf_bounded(r, e, &memory, &work(), &mut || false)
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
        }
        // Removing only this isolated data-fixture's publication guard must not
        // cause the staging API to repair/initialize its namespace.
        let directory = store.path.join("formal-objects-v1");
        let guard = directory.join(".physical-publication-guard-v1");
        assert!(guard.is_file());
        fs::remove_file(&guard).unwrap();
        assert!(
            store
                .stage_index_leaf_bounded(&request, &entry, &memory, &work(), &mut || false)
                .is_err()
        );
        assert!(!guard.exists());
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn actual_pack_physical_and_generation_representations_are_reused() {
    for mode in 0..3 {
        with_store_for_bounded_index_fixture(|store, _| {
            let (request, entry) = inputs();
            let bytes = serde_cbor::to_vec(&Index::Leaf {
                request: request.clone(),
                entry: entry.clone(),
            })
            .unwrap();
            let id = blake3::hash(&bytes).to_hex().to_string();
            let directory = store.path.join("formal-objects-v1");
            if mode == 0 {
                super::super::super::super::business_pack::persist(
                    &directory,
                    &[
                        super::super::super::super::business_index::BusinessIndexNodeWriteV1 {
                            digest: id.clone(),
                            bytes: bytes.clone(),
                        },
                    ],
                )
                .unwrap();
                assert!(
                    fs::read(directory.join(&id))
                        .unwrap()
                        .starts_with(b"oasis7.formal.business-cas.pack.v1\0")
                );
            } else {
                let padding = vec![92; 128 << 10];
                let mut objects = vec![
                    (id.clone(), bytes.clone()),
                    (blake3::hash(&padding).to_hex().to_string(), padding),
                ];
                objects.sort_by(|a, b| a.0.cmp(&b.0));
                assert!(physical::persist(&directory, &objects).unwrap());
                assert!(
                    directory.join(".representation-v1").join(&id).is_file(),
                    "actual physical selector"
                );
                assert_ne!(
                    fs::read(directory.join(".representation-v1").join(&id)).unwrap(),
                    bytes,
                    "selector is not raw payload"
                );
                if mode == 2 {
                    physical::roots::retain(&directory, std::slice::from_ref(&id)).unwrap();
                    let mut session = physical::GenerationSession::default();
                    let mut selected = None;
                    for _ in 0..100 {
                        let progress =
                            physical::advance_with_session(&directory, &mut session).unwrap();
                        if !progress.building {
                            selected = progress.selected;
                            break;
                        }
                    }
                    assert!(selected.is_some(), "real generation finished");
                }
            }
            let memory = memory();
            let meter = work();
            let stage = store
                .stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || false)
                .unwrap();
            assert_eq!(stage.bytes(), bytes, "mode {mode}");
            assert_eq!(stage.id(), id);
            assert!(store.lookup(&request).unwrap().is_none());
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn retired_namespace_and_occupied_publication_guard_are_rejected() {
    for retired in [false, true] {
        with_store_for_bounded_index_fixture(|store, _| {
            let (request, entry) = inputs();
            let directory = store.path.join("formal-objects-v1");
            let guard = if retired {
                None
            } else {
                Some(physical::publication_guard(&directory).unwrap())
            };
            if retired {
                fs::write(
                    directory.join(".physical-retire-v1.cbor"),
                    b"not an admitted transition",
                )
                .unwrap();
            }
            let memory = memory();
            assert!(
                store
                    .stage_index_leaf_bounded(&request, &entry, &memory, &work(), &mut || false)
                    .is_err()
            );
            assert_eq!(memory.used(), 0);
            assert!(store.lookup(&request).unwrap().is_none());
            drop(guard);
        });
    }
}

#[test]
fn private_writer_short_interrupted_zero_and_first_error_are_metered() {
    struct Writer {
        calls: usize,
        output: Vec<u8>,
        mode: u8,
    }
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls == 1 {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if self.mode == 1 {
                return Ok(0);
            }
            if self.mode == 2 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "typed stage sentinel",
                ));
            }
            let n = bytes.len().min(3);
            self.output.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for mode in 0..3 {
        let meter = FormalArchiveReadWorkV1::new(100, 100).unwrap();
        let mut writer = Writer {
            calls: 0,
            output: Vec::new(),
            mode,
        };
        let result = write_payload_bounded(&mut writer, b"abcdefgh", &meter, &mut || false);
        assert!(meter.remaining_steps() < 100);
        assert!(meter.remaining_read_bytes() < 100);
        match mode {
            0 => {
                result.unwrap();
                assert_eq!(writer.output, b"abcdefgh");
                assert_eq!(writer.calls, 4);
            }
            1 => {
                let error = result.unwrap_err();
                let expected = io(std::io::Error::from(std::io::ErrorKind::WriteZero));
                assert_eq!(
                    std::mem::discriminant(&error),
                    std::mem::discriminant(&expected)
                );
                assert_eq!(error.to_string(), expected.to_string());
            }
            _ => assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("typed stage sentinel")
            ),
        }
    }
    let meter = FormalArchiveReadWorkV1::new(100, 7).unwrap();
    let mut writer = Writer {
        calls: 0,
        output: Vec::new(),
        mode: 0,
    };
    assert!(write_payload_bounded(&mut writer, b"abcdefgh", &meter, &mut || false).is_err());
    assert_eq!(writer.calls, 0, "budget is charged before actual write");
}

#[test]
fn raw_publish_never_clobbers_and_late_cancel_returns_no_owner() {
    for late_cancel in [false, true] {
        with_store_for_bounded_index_fixture(|store, _| {
            let (request, entry) = inputs();
            let bytes = serde_cbor::to_vec(&Index::Leaf {
                request: request.clone(),
                entry: entry.clone(),
            })
            .unwrap();
            let id = blake3::hash(&bytes).to_hex().to_string();
            let directory = store.path.join("formal-objects-v1");
            let target = directory.join(&id);
            let memory = memory();
            let mut injected = false;
            let result =
                store.stage_index_leaf_bounded(&request, &entry, &memory, &work(), &mut || {
                    if late_cancel {
                        return target.exists();
                    }
                    if !injected
                        && fs::read_dir(&directory).unwrap().any(|e| {
                            e.unwrap()
                                .file_name()
                                .to_string_lossy()
                                .starts_with(".formal-stage-")
                        })
                    {
                        fs::write(&target, b"competing immutable original").unwrap();
                        injected = true;
                    }
                    false
                });
            assert!(result.is_err());
            drop(result);
            assert_eq!(memory.used(), 0);
            assert_eq!(
                fs::read(&target).unwrap(),
                if late_cancel {
                    bytes.as_slice()
                } else {
                    b"competing immutable original"
                }
            );
            assert!(store.lookup(&request).unwrap().is_none());
        });
    }
}

#[test]
fn encoder_child_lease_prevents_alias_charge_and_same_meter_reentry() {
    with_store_for_bounded_index_fixture(|store, _| {
        let (request, entry) = inputs();
        let memory = memory();
        let meter = work();
        let mut observed = false;
        let stage = store
            .stage_index_leaf_bounded(&request, &entry, &memory, &meter, &mut || {
                if meter.consume_steps(0, &mut || false).is_err() {
                    observed = true;
                    let before = (meter.remaining_steps(), meter.remaining_read_bytes());
                    assert!(
                        store
                            .stage_index_leaf_bounded(
                                &request,
                                &entry,
                                &memory,
                                &meter,
                                &mut || false
                            )
                            .is_err()
                    );
                    assert_eq!(
                        before,
                        (meter.remaining_steps(), meter.remaining_read_bytes())
                    );
                }
                false
            })
            .unwrap();
        assert!(observed, "real encoder/local child lease observed");
        drop(stage);
        assert_eq!(memory.used(), 0);
    });
}

#[cfg(unix)]
#[test]
fn invalid_guard_mode_symlink_and_marker_are_not_admitted() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for mode in 0..3 {
        with_store_for_bounded_index_fixture(|store, _| {
            let (request, entry) = inputs();
            let directory = store.path.join("formal-objects-v1");
            let guard = directory.join(".physical-publication-guard-v1");
            match mode {
                0 => fs::set_permissions(&guard, fs::Permissions::from_mode(0o666)).unwrap(),
                1 => {
                    fs::remove_file(&guard).unwrap();
                    symlink(".physical-guard-v1", &guard).unwrap();
                }
                _ => fs::write(
                    directory.join(".physical-write-v1"),
                    b"invalid initialized marker",
                )
                .unwrap(),
            }
            let memory = memory();
            assert!(
                store
                    .stage_index_leaf_bounded(&request, &entry, &memory, &work(), &mut || false)
                    .is_err(),
                "mode {mode}"
            );
            assert_eq!(memory.used(), 0);
            assert!(store.lookup(&request).unwrap().is_none());
        });
    }
}

#[cfg(unix)]
#[test]
fn escaped_long_path_busy_guard_diagnostics_and_admission_are_paid() {
    use std::os::unix::ffi::OsStringExt;
    with_store_for_bounded_index_fixture(|_, parent| {
        let mut root = parent.to_path_buf();
        for i in 0..6u8 {
            let mut component = vec![b'x'; 110];
            // APFS requires valid UTF-8 filenames. Linux additionally exercises
            // an actual invalid UTF-8 component; Mac still uses escaped control bytes.
            #[cfg(target_os = "linux")]
            component.push(0xff);
            component.extend_from_slice(&[1, b'\n', i + b'0']);
            root.push(std::ffi::OsString::from_vec(component));
        }
        fs::create_dir_all(&root).unwrap();
        physical::initialize(&root).unwrap();
        let busy = physical::publication_guard(&root).unwrap();
        let actual_memory = memory();
        let meter = work();
        let error = match super::super::stage_io::guard_initialized(
            &root,
            &actual_memory,
            &meter,
            &mut || false,
        ) {
            Ok(_) => panic!("busy publication guard admitted"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, FormalError::Conflict(message) if message == "physical publication busy")
        );
        assert_eq!(actual_memory.used(), 0);
        let tiny = MemoryBudget::new(0, 0).unwrap();
        let tiny_meter = work();
        let error =
            match super::super::stage_io::guard_initialized(&root, &tiny, &tiny_meter, &mut || {
                false
            }) {
                Ok(_) => panic!("unpaid guard admitted"),
                Err(error) => error,
            };
        assert!(error.to_string().contains("admission"));
        assert_eq!(
            tiny_meter.remaining_steps(),
            2_000_000 - 1,
            "no filesystem lock attempt before memory admission"
        );
        assert_eq!(tiny.used(), 0);
        drop(busy);
        // The original retirement-pin conflict, unlike publication-mutex
        // contention, contains both escaped paths and opened-FD identity.
        let retirement = physical::pin(&root, true).unwrap();
        assert!(retirement.is_some());
        let error = match super::super::stage_io::guard_initialized(
            &root,
            &actual_memory,
            &work(),
            &mut || false,
        ) {
            Ok(_) => panic!("exclusive retirement fence admitted a publisher"),
            Err(error) => error,
        };
        assert!(matches!(&error, FormalError::Conflict(_)));
        assert!(error.to_string().contains("root="));
        assert!(error.to_string().contains("guard="));
        assert!(error.to_string().contains("mode=shared"));
        assert_eq!(actual_memory.used(), 0);
        drop(retirement);
        let guard =
            super::super::stage_io::guard_initialized(&root, &actual_memory, &work(), &mut || {
                false
            })
            .unwrap();
        assert_eq!(guard.reserved_bytes(), actual_memory.used());
        assert!(physical::pin(&root, true).is_err());
        drop(guard);
        assert_eq!(actual_memory.used(), 0);
        drop(physical::pin(&root, true).unwrap());
    });
}
