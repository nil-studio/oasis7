use super::super::tests::{Fixture, object};
use super::*;
fn logical(root: &Path, value: &(String, Vec<u8>)) -> Vec<u8> {
    super::super::super::business_pack::read_object(&root.join(&value.0), &value.0)
        .unwrap()
        .unwrap()
}
fn finish(root: &Path) -> FormalGenerationProgress {
    let mut session = GenerationSession::default();
    for _ in 0..100 {
        let progress = advance_with_session(root, &mut session).unwrap();
        if !progress.building {
            return progress;
        }
    }
    panic!("bounded generation fixture failed to complete")
}
#[test]
fn incremental_generation_includes_late_overlay_fences_readers_and_retires_old_representation() {
    let fixture = Fixture::new();
    let mut objects = (0..3)
        .map(|i| object(vec![i + 31; 5 * 1024 * 1024]))
        .collect::<Vec<_>>();
    for (id, bytes) in &objects {
        fs::write(fixture.0.join(id), bytes).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(id)).unwrap();
    }
    assert!(advance(&fixture.0).unwrap().building);
    let late = object(b"late publication during background generation".to_vec());
    fs::write(fixture.0.join(&late.0), &late.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&late.0)).unwrap();
    objects.push(late);
    let oldreader = pin(&fixture.0, false).unwrap();
    let mut blocked = false;
    for _ in 0..10 {
        match advance(&fixture.0) {
            Ok(p) => assert!(p.building),
            Err(FormalError::Conflict(_)) => {
                blocked = true;
                break;
            }
            Err(e) => panic!("unexpected generation failure: {e:?}"),
        }
    }
    assert!(blocked);
    drop(oldreader);
    let selected = finish(&fixture.0).selected.unwrap();
    for object in &objects {
        assert_eq!(logical(&fixture.0, object), object.1);
        assert!(!fixture.0.join(&object.0).exists());
        let (length, range) = super::super::super::business_pack::read_range(
            &fixture.0.join(&object.0),
            &object.0,
            0,
            7,
        )
        .unwrap()
        .unwrap();
        assert_eq!(length, object.1.len() as u64);
        assert_eq!(range, object.1[..7.min(object.1.len())]);
    }
    let overlay = object(vec![99; 100000]);
    assert!(persist(&fixture.0, std::slice::from_ref(&overlay)).unwrap());
    roots::retain(&fixture.0, std::slice::from_ref(&overlay.0)).unwrap();
    assert_eq!(logical(&fixture.0, &overlay), overlay.1);
    let next = finish(&fixture.0).selected.unwrap();
    assert_ne!(selected, next);
    assert!(!fixture.0.join(GENERATIONS).join(selected).exists());
    for object in objects.iter().chain(std::iter::once(&overlay)) {
        assert_eq!(logical(&fixture.0, object), object.1);
    }
}
#[test]
fn generation_recovers_sealed_selected_and_interrupted_retirement_without_rewriting_world() {
    for stage in 4..=7 {
        let fixture = Fixture::new();
        let value = object(vec![21; 200000]);
        fs::write(fixture.0.join(&value.0), &value.1).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
        publication::fail_at(stage);
        let mut session = GenerationSession::default();
        let mut interrupted = false;
        for _ in 0..100 {
            if advance_with_session(&fixture.0, &mut session).is_err() {
                interrupted = true;
                break;
            }
        }
        assert!(interrupted, "fault boundary was never reached");
        drop(session);
        initialize(&fixture.0).unwrap();
        assert_eq!(logical(&fixture.0, &value), value.1);
        finish(&fixture.0);
        assert_eq!(logical(&fixture.0, &value), value.1);
        assert!(!fixture.0.join(BUILD).exists());
        assert_eq!(
            fs::read(
                fixture
                    .0
                    .parent()
                    .unwrap()
                    .join("formal-checkpoint-v1.cbor")
            )
            .unwrap(),
            b"checkpoint"
        );
    }
}
#[test]
fn corrupt_selected_generation_never_falls_back_to_stale_overlay() {
    let fixture = Fixture::new();
    let value = object(vec![42; 200000]);
    fs::write(fixture.0.join(&value.0), &value.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
    let selected = finish(&fixture.0).selected.unwrap();
    fs::write(fixture.0.join(&value.0), &value.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
    fs::write(
        fixture.0.join(GENERATIONS).join(selected).join(MANIFEST),
        b"corrupt",
    )
    .unwrap();
    assert!(
        super::super::super::business_pack::read_object(&fixture.0.join(&value.0), &value.0)
            .is_err()
    );
}

#[test]
fn interrupted_unselected_seal_accepts_later_overlay_by_reopening_its_catalog() {
    let fixture = Fixture::new();
    let first = object(vec![31; 100000]);
    fs::write(fixture.0.join(&first.0), &first.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&first.0)).unwrap();
    publication::fail_at(4);
    assert!(advance(&fixture.0).is_err());
    let late = object(vec![52; 100000]);
    fs::write(fixture.0.join(&late.0), &late.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&late.0)).unwrap();
    finish(&fixture.0);
    assert_eq!(logical(&fixture.0, &first), first.1);
    assert_eq!(logical(&fixture.0, &late), late.1);
}
#[test]
fn reopen_during_multi_page_retirement_does_not_resurrect_root_selectors() {
    let fixture = Fixture::new();
    let mut objects = (0u32..1025)
        .map(|index| object(index.to_le_bytes().repeat(64)))
        .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(persist(&fixture.0, &objects).unwrap());
    roots::retain(
        &fixture.0,
        &objects
            .iter()
            .map(|value| value.0.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut selected = false;
    let mut session = GenerationSession::default();
    for _ in 0..100 {
        let progress = advance_with_session(&fixture.0, &mut session).unwrap();
        if progress.selected.is_some() && progress.building {
            selected = true;
            break;
        }
    }
    assert!(selected);
    drop(session);
    initialize(&fixture.0).unwrap();
    let mut reopened = GenerationSession::default();
    for _ in 0..100 {
        if !advance_with_session(&fixture.0, &mut reopened)
            .unwrap()
            .building
        {
            break;
        }
    }
    assert!(!fixture.0.join(BUILD).exists());
    assert_eq!(fs::read_dir(fixture.0.join(SELECTORS)).unwrap().count(), 0);
    assert_eq!(fs::read_dir(fixture.0.join(FRAMES)).unwrap().count(), 0);
    for value in &objects {
        assert_eq!(logical(&fixture.0, value), value.1);
    }
}

#[test]
fn endpoint_lifetime_generation_pages_do_not_repeat_full_selector_recovery() {
    let fixture = Fixture::new();
    for n in 0..4u8 {
        let value = object(vec![n + 120; 5 * 1024 * 1024]);
        fs::write(fixture.0.join(&value.0), &value.1).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
    }
    publication::FULL_RECOVERY_PASSES.with(|count| count.set(0));
    let mut session = GenerationSession::default();
    assert!(
        advance_with_session(&fixture.0, &mut session)
            .unwrap()
            .building
    );
    assert!(
        advance_with_session(&fixture.0, &mut session)
            .unwrap()
            .building
    );
    assert_eq!(
        publication::FULL_RECOVERY_PASSES.with(|count| count.get()),
        1,
        "one live owner must fully recover its private build only once"
    );
}

#[test]
fn generation_session_reopens_full_recovery_after_restart_or_failed_page() {
    let fixture = Fixture::new();
    for n in 0..4u8 {
        let value = object(vec![n + 140; 5 * 1024 * 1024]);
        fs::write(fixture.0.join(&value.0), &value.1).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
    }
    let mut session = GenerationSession::default();
    publication::FULL_RECOVERY_PASSES.with(|count| count.set(0));
    publication::fail_at(3);
    assert!(advance_with_session(&fixture.0, &mut session).is_err());
    assert_eq!(
        publication::FULL_RECOVERY_PASSES.with(|count| count.get()),
        1
    );
    assert!(
        advance_with_session(&fixture.0, &mut session)
            .unwrap()
            .building
    );
    assert_eq!(
        publication::FULL_RECOVERY_PASSES.with(|count| count.get()),
        2
    );
    // A reopened owner never inherits the previous process's recovery token.
    let mut reopened = GenerationSession::default();
    assert!(
        advance_with_session(&fixture.0, &mut reopened)
            .unwrap()
            .building
    );
    assert_eq!(
        publication::FULL_RECOVERY_PASSES.with(|count| count.get()),
        3
    );
}

#[test]
fn automatic_generation_page_uses_small_payload_budget_and_explicit_mode_finishes() {
    let fixture = Fixture::new();
    let mut expected = Vec::new();
    for n in 0..4u8 {
        let value = object(vec![n + 190; 32 * 1024]);
        fs::write(fixture.0.join(&value.0), &value.1).unwrap();
        roots::retain(&fixture.0, std::slice::from_ref(&value.0)).unwrap();
        expected.push(value);
    }
    let mut session = GenerationSession::default();
    let page =
        advance_with_budget(&fixture.0, &mut session, GenerationBudget::automatic()).unwrap();
    assert!(page.building);
    assert!(
        (1..=2).contains(&page.copied_objects),
        "automatic page copied more than 64KiB of 32KiB originals"
    );
    for _ in 0..100 {
        if !advance_with_session(&fixture.0, &mut session)
            .unwrap()
            .building
        {
            for value in &expected {
                assert_eq!(logical(&fixture.0, value), value.1);
            }
            return;
        }
    }
    panic!("explicit pages did not finish automatic session's original work");
}

#[test]
fn selected_generation_retires_orphans_with_a_new_live_overlay_under_exclusive_fence() {
    let fixture = Fixture::new();
    let first = object(vec![71; 100000]);
    fs::write(fixture.0.join(&first.0), &first.1).unwrap();
    roots::retain(&fixture.0, std::slice::from_ref(&first.0)).unwrap();
    let mut session = GenerationSession::default();
    let mut selected = None;
    for _ in 0..100 {
        let progress = advance_with_session(&fixture.0, &mut session).unwrap();
        if progress.selected.is_some() && progress.building {
            selected = progress.selected;
            break;
        }
    }
    let selected = selected.expect("fixture must reach selected retirement");
    let late = object(vec![72; 100000]);
    assert!(persist(&fixture.0, std::slice::from_ref(&late)).unwrap());
    roots::retain(&fixture.0, std::slice::from_ref(&late.0)).unwrap();
    let garbage = b"orphan added with late overlay";
    let garbage_id = blake3::hash(garbage).to_hex().to_string();
    create(&fixture.0.join(FRAMES).join(&garbage_id), garbage).unwrap();
    let mut complete = false;
    for _ in 0..100 {
        let progress = advance_with_session(&fixture.0, &mut session).unwrap();
        if !progress.building {
            assert_eq!(progress.selected.as_deref(), Some(selected.as_str()));
            complete = true;
            break;
        }
    }
    assert!(
        complete,
        "nonempty overlay GC must finish under its original fence"
    );
    assert!(!fixture.0.join(FRAMES).join(garbage_id).exists());
    assert!(fixture.0.join(SELECTORS).join(&late.0).exists());
    assert_eq!(logical(&fixture.0, &first), first.1);
    assert_eq!(logical(&fixture.0, &late), late.1);
}
