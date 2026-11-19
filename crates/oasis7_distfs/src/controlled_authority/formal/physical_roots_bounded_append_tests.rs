use super::*;
use crate::controlled_authority::formal::endpoint::with_store_for_bounded_retention_fixture;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> FormalArchiveReadWorkV1 {
    FormalArchiveReadWorkV1::new(2_000_000, 256 << 20).unwrap()
}
fn inputs() -> (String, String) {
    ("56".repeat(32), "78".repeat(32))
}

#[test]
fn actual_store_stage_retained_has_original_log_record_without_index_publication() {
    with_store_for_bounded_retention_fixture(|fixture, _| {
        let root = fixture.object_root();
        let before = fixture.checkpoint_bytes().unwrap();
        let old_count = count(&root).unwrap();
        let (request, entry) = inputs();
        let memory = memory();
        let meter = work();
        let owner = fixture
            .stage_retained(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        assert_eq!(blake3::hash(owner.bytes()).to_hex().as_str(), owner.id());
        assert!(owner.uses_budget(&memory));
        assert_eq!(owner.reserved_bytes(), memory.used());
        assert_eq!(count(&root).unwrap(), old_count + 1);
        let mut cursor = start();
        let mut found = None;
        loop {
            let (ids, next, done) = page(&root, &cursor, 1024).unwrap();
            if let Some(id) = ids.last() {
                found = Some(id.clone());
            }
            cursor = next;
            if done {
                break;
            }
        }
        assert_eq!(found.as_deref(), Some(owner.id()));
        assert!(fixture.lookup(&request).unwrap().is_none());
        assert_eq!(fixture.checkpoint_bytes().unwrap(), before);
        assert!(crate::controlled_authority::formal::physical::pin(&root, true).is_err());
        drop(owner);
        assert_eq!(memory.used(), 0);
        drop(crate::controlled_authority::formal::physical::pin(&root, true).unwrap());
        // The original registry permits repeated IDs: retry is another record.
        let owner = fixture
            .stage_retained(&request, &entry, &memory, &meter, &mut || false)
            .unwrap();
        assert_eq!(count(&root).unwrap(), old_count + 2);
        drop(owner);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn true_1023_1024_boundary_and_uncommitted_suffix_recovery() {
    with_store_for_bounded_retention_fixture(|fixture, _| {
        let root = fixture.object_root();
        let (request, entry) = inputs();
        let memory = memory();
        let stage = fixture
            .stage(&request, &entry, &memory, &work(), &mut || false)
            .unwrap();
        let prior = count(&root).unwrap();
        assert!(prior <= 1023);
        retain(
            &root,
            &vec![stage.retention_source(&memory).unwrap().1.to_owned(); (1023 - prior) as usize],
        )
        .unwrap();
        assert_eq!(count(&root).unwrap(), 1023);
        let first = append_one_bounded_v1(&stage, &memory, &work(), &mut || false).unwrap();
        assert_eq!(count(&root).unwrap(), 1024);
        drop(first);
        let next_segment = segment(&root, 1);
        fs::write(&next_segment, b"uncommitted suffix").unwrap();
        let second = append_one_bounded_v1(&stage, &memory, &work(), &mut || false).unwrap();
        assert_eq!(count(&root).unwrap(), 1025);
        assert_eq!(fs::metadata(next_segment).unwrap().len(), 64);
        drop(second);
        drop(stage);
        assert_eq!(memory.used(), 0);
    });
}

#[test]
fn missing_head_and_corrupt_committed_prefix_are_rejected_without_reinitialization() {
    for mode in 0..3 {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            let first = append_one_bounded_v1(&stage, &memory, &work(), &mut || false).unwrap();
            drop(first);
            let head_path = root.join(ROOTS).join(HEAD);
            let before = fs::read(&head_path).unwrap();
            match mode {
                0 => fs::remove_file(&head_path).unwrap(),
                1 => {
                    let file = OpenOptions::new()
                        .write(true)
                        .open(segment(&root, 0))
                        .unwrap();
                    file.set_len(1).unwrap();
                }
                _ => {
                    let mut bytes = fs::read(segment(&root, 0)).unwrap();
                    let last = bytes.len() - 1;
                    bytes[last] ^= 1;
                    fs::write(segment(&root, 0), bytes).unwrap();
                }
            }
            assert!(append_one_bounded_v1(&stage, &memory, &work(), &mut || false).is_err());
            if mode == 0 {
                assert!(!head_path.exists());
            } else {
                assert_eq!(fs::read(head_path).unwrap(), before);
            }
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn zero_budgets_and_cancellation_preserve_first_error_and_release_actual_pool() {
    for mode in 0..4 {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            let live = memory.used();
            let before = head(&root).unwrap().count;
            let meter = FormalArchiveReadWorkV1::new(
                if mode == 0 { 0 } else { 2_000_000 },
                if mode == 1 { 0 } else { 256 << 20 },
            )
            .unwrap();
            let mut calls = 0;
            let result = append_one_bounded_v1(&stage, &memory, &meter, &mut || {
                calls += 1;
                mode == 2 || (mode == 3 && calls >= 12)
            });
            assert!(result.is_err(), "mode {mode}");
            drop(result);
            assert_eq!(memory.used(), live);
            assert_eq!(head(&root).unwrap().count, before);
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[cfg(unix)]
#[test]
fn fifo_head_and_segment_are_rejected_without_blocking() {
    use std::os::unix::ffi::OsStrExt;
    for segment_fifo in [false, true] {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            let path = if segment_fifo {
                segment(&root, 0)
            } else {
                root.join(ROOTS).join(HEAD)
            };
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: name is a live NUL-terminated path and mode has no pointers.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            let live = memory.used();
            assert!(append_one_bounded_v1(&stage, &memory, &work(), &mut || false).is_err());
            assert_eq!(memory.used(), live);
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn exact_phase_and_actual_head_workspace_minus_one_are_rejected() {
    for head_phase in [false, true] {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            let baseline = memory.used();
            let raw = fs::read(root.join(ROOTS).join(HEAD)).unwrap();
            let needed = phase_layout(&root).unwrap()
                + if head_phase {
                    head_workspace(raw.len()).unwrap()
                } else {
                    0
                };
            let pressure = memory
                .reserve((256 << 20) - baseline - (needed - 1), MemoryClass::Ordinary)
                .unwrap();
            let live = memory.used();
            let error = match append_one_bounded_v1(&stage, &memory, &work(), &mut || false) {
                Ok(_) => panic!("one-byte-short budget admitted"),
                Err(e) => e,
            };
            assert!(error.to_string().contains(if head_phase {
                "head admission"
            } else {
                "phase admission"
            }));
            assert_eq!(memory.used(), live);
            drop(pressure);
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn hostile_head_codec_uses_original_decoder_and_preserves_errors() {
    let unknown = serde_cbor::to_vec(&serde_cbor::Value::Map(std::collections::BTreeMap::from([
        (
            serde_cbor::Value::Text("unknown\nfield".into()),
            serde_cbor::Value::Integer(1),
        ),
    ])))
    .unwrap();
    let oversized_hint = vec![
        0xa1, 0x68, b'c', b'h', b'e', b'c', b'k', b's', b'u', b'm', 0x7a, 0xff, 0xff, 0xff, 0xff,
    ];
    for raw in [unknown, oversized_hint, vec![0xff], vec![0xa0], vec![0xf6]] {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            fs::write(root.join(ROOTS).join(HEAD), &raw).unwrap();
            let expected = match decode_head(&raw) {
                Ok(_) => panic!("invalid fixture head"),
                Err(e) => e,
            };
            let live = memory.used();
            let actual = match append_one_bounded_v1(&stage, &memory, &work(), &mut || false) {
                Ok(_) => panic!("invalid head admitted"),
                Err(e) => e,
            };
            assert_eq!(
                std::mem::discriminant(&actual),
                std::mem::discriminant(&expected)
            );
            assert_eq!(actual.to_string(), expected.to_string());
            assert_eq!(memory.used(), live);
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn late_cancellation_preserves_real_segment_then_head_commit_order() {
    for after_head in [false, true] {
        with_store_for_bounded_retention_fixture(|fixture, _| {
            let root = fixture.object_root();
            let (request, entry) = inputs();
            let memory = memory();
            let stage = fixture
                .stage(&request, &entry, &memory, &work(), &mut || false)
                .unwrap();
            let head_path = root.join(ROOTS).join(HEAD);
            let original = fs::read(&head_path).unwrap();
            let old = head(&root).unwrap().count;
            let live = memory.used();
            let result = append_one_bounded_v1(&stage, &memory, &work(), &mut || {
                if after_head {
                    fs::read(&head_path).unwrap() != original
                } else {
                    fs::metadata(segment(&root, old / RECORDS))
                        .map(|m| m.len() > (old % RECORDS) * 64)
                        .unwrap_or(false)
                }
            });
            assert!(result.is_err());
            drop(result);
            assert_eq!(memory.used(), live);
            assert_eq!(head(&root).unwrap().count, old + u64::from(after_head));
            // A cancelled append returns no receipt even when atomic head
            // replacement already committed. Retry follows the genuine log.
            let receipt = append_one_bounded_v1(&stage, &memory, &work(), &mut || false).unwrap();
            assert_eq!(count(&root).unwrap(), old + u64::from(after_head) + 1);
            drop(receipt);
            drop(stage);
            assert_eq!(memory.used(), 0);
        });
    }
}

#[test]
fn production_write_loop_handles_short_interrupted_zero_and_typed_source_error() {
    struct Writer {
        calls: usize,
        bytes: Vec<u8>,
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
                    "retention sentinel",
                ));
            }
            let n = bytes.len().min(3);
            self.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for mode in 0..3 {
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut io = BoundedRetainIo {
            memory: &memory,
            work: &meter,
            stop: &mut stop,
        };
        let mut writer = Writer {
            calls: 0,
            bytes: Vec::new(),
            mode,
        };
        let result = io.write_loop(&mut writer, b"abcdefgh");
        match mode {
            0 => {
                result.unwrap();
                assert_eq!(writer.bytes, b"abcdefgh");
                assert_eq!(writer.calls, 4);
            }
            1 => assert_eq!(
                result.unwrap_err().to_string(),
                super::io(std::io::Error::from(std::io::ErrorKind::WriteZero)).to_string()
            ),
            _ => assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("retention sentinel")
            ),
        }
        assert!(meter.remaining_read_bytes() < 256 << 20);
    }
}

#[cfg(unix)]
#[test]
fn segment_open_rechecks_symlink_parent_and_never_writes_outside() {
    with_store_for_bounded_retention_fixture(|fixture, parent| {
        let root = fixture.object_root();
        let outside = parent.join("outside-retention-data");
        fs::create_dir(&outside).unwrap();
        let log = root.join(ROOTS);
        let saved = root.join("saved-retention-data");
        fs::rename(&log, &saved).unwrap();
        std::os::unix::fs::symlink(&outside, &log).unwrap();
        let memory = memory();
        let meter = work();
        let mut stop = || false;
        let mut io = BoundedRetainIo {
            memory: &memory,
            work: &meter,
            stop: &mut stop,
        };
        assert!(io.open_segment(&segment(&root, 0)).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        assert!(meter.remaining_steps() < 2_000_000);
    });
}
