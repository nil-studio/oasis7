use super::*;
fn build() -> Build {
    Build {
        version: 3,
        nonce: "a".repeat(64),
        previous: None,
        previous_manifest: None,
        objects: Catalogs::new(),
        completed: Catalogs::new(),
        cursor: roots::PageCursor::start(),
        sealed: None,
        unsealing: None,
        retire_after: None,
        old_checked: None,
        old_verified: false,
        catalog_retired: Vec::new(),
        overlay: None,
    }
}
fn put(root: &Path, value: u8) -> (String, Vec<u8>) {
    let object = super::super::super::tests::object(vec![value; 100000]);
    assert!(persist(root, std::slice::from_ref(&object)).unwrap());
    object
}
fn orphan(root: &Path, value: u32) -> String {
    let bytes = value.to_le_bytes();
    let id = blake3::hash(&bytes).to_hex().to_string();
    create(&root.join(FRAMES).join(&id), &bytes).unwrap();
    id
}
#[test]
fn large_overlay_revalidates_on_reopen_and_append_before_bounded_sweep() {
    let fixture = super::super::super::tests::Fixture::new();
    let objects = (1..=40)
        .map(|value| put(&fixture.0, value))
        .collect::<Vec<_>>();
    let orphans = (0..41)
        .map(|value| orphan(&fixture.0, value))
        .collect::<Vec<_>>();
    let mut state = build();
    let mut session = Session::default();
    let selected = "b".repeat(64);
    let _fence = pin(&fixture.0, true).unwrap();
    for _ in 0..3 {
        assert!(
            !advance(
                &fixture.0,
                &mut state,
                &selected,
                &mut session,
                GenerationBudget::automatic()
            )
            .unwrap()
        );
    }
    assert!(
        orphans
            .iter()
            .all(|id| fixture.0.join(FRAMES).join(id).exists())
    );
    // A saved mark is progress, never permission to skip fresh closure readback.
    state = load(&fixture.0).unwrap();
    session = Session::default();
    for _ in 0..10000 {
        if state.overlay.as_ref().is_some_and(|value| value.sealed) {
            drop(_fence);
            let appended = put(&fixture.0, 99);
            // Publication is serialized with each individual maintenance page.
            return finish_after_append(
                fixture, state, session, selected, objects, appended, orphans,
            );
        }
        let complete = advance(
            &fixture.0,
            &mut state,
            &selected,
            &mut session,
            GenerationBudget::automatic(),
        )
        .unwrap();
        assert!(!complete, "must append after mark before first sweep");
    }
    panic!("mark must eventually finish");
}
fn finish_after_append(
    fixture: super::super::super::tests::Fixture,
    mut state: Build,
    mut session: Session,
    selected: String,
    objects: Vec<(String, Vec<u8>)>,
    appended: (String, Vec<u8>),
    orphans: Vec<String>,
) {
    let _fence = pin(&fixture.0, true).unwrap();
    assert!(
        !advance(
            &fixture.0,
            &mut state,
            &selected,
            &mut session,
            GenerationBudget::automatic()
        )
        .unwrap()
    );
    assert!(
        orphans
            .iter()
            .all(|id| fixture.0.join(FRAMES).join(id).exists())
    );
    let mut complete = false;
    for _ in 0..10000 {
        complete = advance(
            &fixture.0,
            &mut state,
            &selected,
            &mut session,
            GenerationBudget::automatic(),
        )
        .unwrap();
        if complete {
            break;
        }
    }
    assert!(
        complete,
        "large overlay must converge without namespace cap"
    );
    assert!(
        orphans
            .iter()
            .all(|id| !fixture.0.join(FRAMES).join(id).exists())
    );
    for (id, bytes) in objects.into_iter().chain(std::iter::once(appended)) {
        assert_eq!(
            read_object(&fixture.0.join(&id), &id, true).unwrap(),
            Some(bytes)
        );
    }
    assert!(!directory_for(&fixture.0, &state.nonce).exists());
}
#[test]
fn malformed_overlay_blocks_frame_deletion_including_after_reopen() {
    let fixture = super::super::super::tests::Fixture::new();
    let object = put(&fixture.0, 7);
    let garbage = orphan(&fixture.0, 100);
    fs::write(fixture.0.join(SELECTORS).join(&object.0), b"unknown schema").unwrap();
    let mut state = build();
    for _ in 0..2 {
        let mut session = Session::default();
        let _fence = pin(&fixture.0, true).unwrap();
        assert!(
            advance(
                &fixture.0,
                &mut state,
                &"b".repeat(64),
                &mut session,
                GenerationBudget::explicit()
            )
            .is_err()
        );
        assert!(fixture.0.join(FRAMES).join(&garbage).exists());
    }
}
#[test]
fn replacing_partial_or_sealed_marks_cannot_authorize_live_frame_deletion() {
    for sealed in [false, true] {
        let fixture = super::super::super::tests::Fixture::new();
        let objects = (1..=20)
            .map(|value| put(&fixture.0, value))
            .collect::<Vec<_>>();
        let garbage = orphan(&fixture.0, 100);
        let mut state = build();
        let mut session = Session::default();
        let selected = "b".repeat(64);
        let _fence = pin(&fixture.0, true).unwrap();
        for _ in 0..1000 {
            advance(
                &fixture.0,
                &mut state,
                &selected,
                &mut session,
                GenerationBudget::automatic(),
            )
            .unwrap();
            if !sealed || state.overlay.as_ref().unwrap().sealed {
                break;
            }
        }
        assert_eq!(state.overlay.as_ref().unwrap().sealed, sealed);
        // This is a structurally valid replacement catalog, with unchanged
        // source stamps. Only live, page-by-page provenance rejects it.
        state.overlay.as_mut().unwrap().marks = Catalogs::new();
        save(&fixture.0, &state).unwrap();
        state = load(&fixture.0).unwrap();
        assert!(
            advance(
                &fixture.0,
                &mut state,
                &selected,
                &mut session,
                GenerationBudget::automatic()
            )
            .is_err()
        );
        assert!(fixture.0.join(FRAMES).join(&garbage).exists());
        for (id, bytes) in objects {
            assert_eq!(
                read_object(&fixture.0.join(&id), &id, true).unwrap(),
                Some(bytes)
            );
        }
    }
}
