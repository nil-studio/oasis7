use super::super::super::tests::{Fixture, object};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> GenerationSelectedReadWorkV1 {
    GenerationSelectedReadWorkV1 {
        remaining_steps: 1_000_000,
        remaining_read_bytes: 128 << 20,
    }
}
fn finish(root: &Path) -> String {
    let mut session = GenerationSession::default();
    for _ in 0..100 {
        let p = advance_with_session(root, &mut session).unwrap();
        if !p.building {
            return p.selected.unwrap();
        }
    }
    panic!("generation did not finish")
}
fn consume(
    path: &Path,
    id: &str,
    memory: &MemoryBudget,
    w: &mut GenerationSelectedReadWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
    let mut physical = super::super::PhysicalReadWorkV1 {
        remaining_steps: w.remaining_steps,
        remaining_read_bytes: w.remaining_read_bytes,
        remaining_metadata_bytes: w.remaining_read_bytes,
    };
    let result = super::super::read_unknown_length_durable_v1(
        path,
        id,
        1 << 20,
        memory,
        &mut physical,
        stop,
    );
    w.remaining_steps = physical.remaining_steps;
    w.remaining_read_bytes = physical.remaining_read_bytes;
    if let Some(out) = result? {
        return Ok(Some(out));
    }
    let mut legacy = super::super::super::super::business_pack::LegacyReadWorkV1 {
        remaining_steps: w.remaining_steps,
        remaining_read_bytes: w.remaining_read_bytes,
    };
    let result = super::super::super::super::business_pack::read_legacy_unknown_length_durable_v1(
        path,
        id,
        1 << 20,
        memory,
        &mut legacy,
        stop,
    );
    w.remaining_steps = legacy.remaining_steps;
    w.remaining_read_bytes = legacy.remaining_read_bytes;
    result
}
#[test]
fn actual_raw_pack_and_physical_overlay_consume_while_root_is_pinned() {
    for mode in 0..3 {
        let f = Fixture::new();
        let value = object(vec![43; 2000]);
        if mode == 0 {
            fs::write(f.0.join(&value.0), &value.1).unwrap();
        } else if mode == 1 {
            let legacy = f.0.join(".legacy-test");
            fs::create_dir(&legacy).unwrap();
            super::super::super::super::business_pack::persist(
                &legacy,
                &[
                    super::super::super::super::business_index::BusinessIndexNodeWriteV1 {
                        digest: value.0.clone(),
                        bytes: value.1.clone(),
                    },
                ],
            )
            .unwrap();
            fs::hard_link(legacy.join(&value.0), f.0.join(&value.0)).unwrap();
        } else {
            let padding = object(vec![91; 128 * 1024]);
            let mut batch = vec![value.clone(), padding];
            batch.sort_by(|left, right| left.0.cmp(&right.0));
            assert!(persist(&f.0, &batch).unwrap());
            assert!(f.0.join(SELECTORS).join(&value.0).is_file());
        }
        let memory = memory();
        let mut w = work();
        let out = with_selected_path_bounded_v1(
            &f.0.join(&value.0),
            &value.0,
            &memory,
            &mut w,
            &mut || false,
            |selected, w, stop| {
                assert!(
                    pin(&f.0, true).is_err(),
                    "actual shared pin must outlive callback"
                );
                consume(selected, &value.0, &memory, w, stop)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(&**out, &value.1);
        assert_eq!(memory.used(), out.reserved_bytes());
        drop(out);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn actual_generation_member_and_late_overlay_match_original_payload() {
    let f = Fixture::new();
    let selected = object(vec![54; 4000]);
    fs::write(f.0.join(&selected.0), &selected.1).unwrap();
    roots::retain(&f.0, std::slice::from_ref(&selected.0)).unwrap();
    let generation = finish(&f.0);
    let overlay = object(b"late overlay".to_vec());
    fs::write(f.0.join(&overlay.0), &overlay.1).unwrap();
    let memory = memory();
    let mut w = work();
    for (value, in_generation) in [(&selected, true), (&overlay, false)] {
        let out = with_selected_path_bounded_v1(
            &f.0.join(&value.0),
            &value.0,
            &memory,
            &mut w,
            &mut || false,
            |path, w, stop| {
                assert_eq!(
                    path.parent().unwrap(),
                    if in_generation {
                        f.0.join(GENERATIONS).join(&generation)
                    } else {
                        f.0.clone()
                    }
                );
                assert!(pin(&f.0, true).is_err());
                consume(path, &value.0, &memory, w, stop)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(&**out, &value.1);
        drop(out);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn selected_missing_never_falls_back_to_existing_stale_overlay() {
    let f = Fixture::new();
    let value = object(vec![62; 4000]);
    fs::write(f.0.join(&value.0), &value.1).unwrap();
    roots::retain(&f.0, std::slice::from_ref(&value.0)).unwrap();
    let generation = finish(&f.0);
    let dir = f.0.join(GENERATIONS).join(generation);
    for p in [dir.join(&value.0), dir.join(SELECTORS).join(&value.0)] {
        match fs::remove_file(p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("{e}"),
        }
    }
    fs::write(f.0.join(&value.0), &value.1).unwrap();
    let memory = memory();
    let mut called = false;
    let error = with_selected_path_bounded_v1(
        &f.0.join(&value.0),
        &value.0,
        &memory,
        &mut work(),
        &mut || false,
        |_, _, _| {
            called = true;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("selected generation object missing"),
        "{error}"
    );
    assert!(!called);
    assert_eq!(memory.used(), 0);
}
#[test]
fn fresh_current_manifest_and_catalog_corruption_are_errors_not_overlay() {
    for mode in 0..3 {
        let f = Fixture::new();
        let value = object(vec![63; 4000]);
        fs::write(f.0.join(&value.0), &value.1).unwrap();
        roots::retain(&f.0, std::slice::from_ref(&value.0)).unwrap();
        let generation = finish(&f.0);
        let memory = memory();
        with_selected_path_bounded_v1(
            &f.0.join(&value.0),
            &value.0,
            &memory,
            &mut work(),
            &mut || false,
            |_, _, _| Ok(()),
        )
        .unwrap();
        let dir = f.0.join(GENERATIONS).join(&generation);
        let path = match mode {
            0 => f.0.join(CURRENT),
            1 => dir.join(MANIFEST),
            _ => fs::read_dir(dir.join(catalog::CATALOG))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        };
        fs::write(path, b"corrupt metadata").unwrap();
        let mut called = false;
        assert!(
            with_selected_path_bounded_v1(
                &f.0.join(&value.0),
                &value.0,
                &memory,
                &mut work(),
                &mut || false,
                |_, _, _| {
                    called = true;
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!called);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn continuous_budget_cancel_and_memory_failure_never_enter_consumer() {
    let f = Fixture::new();
    let value = object(vec![64; 4000]);
    fs::write(f.0.join(&value.0), &value.1).unwrap();
    roots::retain(&f.0, std::slice::from_ref(&value.0)).unwrap();
    finish(&f.0);
    for mode in 0..4 {
        let memory = if mode == 2 {
            MemoryBudget::new(0, 0).unwrap()
        } else {
            memory()
        };
        let mut w = work();
        if mode == 0 {
            w.remaining_steps = 0;
        }
        if mode == 1 {
            w.remaining_read_bytes = 0;
        }
        let mut called = false;
        let mut observations = 0;
        assert!(
            with_selected_path_bounded_v1(
                &f.0.join(&value.0),
                &value.0,
                &memory,
                &mut w,
                &mut || {
                    observations += 1;
                    mode == 3 && observations >= 4
                },
                |_, _, _| {
                    called = true;
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!called);
        assert_eq!(memory.used(), 0);
    }
    let memory = memory();
    let mut w = work();
    let initial = w.remaining_read_bytes;
    with_selected_path_bounded_v1(
        &f.0.join(&value.0),
        &value.0,
        &memory,
        &mut w,
        &mut || false,
        |_, _, _| Ok(()),
    )
    .unwrap();
    let spent = initial - w.remaining_read_bytes;
    assert!(spent > 0);
    w.remaining_read_bytes = spent - 1;
    assert!(
        with_selected_path_bounded_v1(
            &f.0.join(&value.0),
            &value.0,
            &memory,
            &mut w,
            &mut || false,
            |_, _, _| Ok(())
        )
        .is_err()
    );
    assert_eq!(w.remaining_read_bytes, 0);
    assert_eq!(memory.used(), 0);
}
