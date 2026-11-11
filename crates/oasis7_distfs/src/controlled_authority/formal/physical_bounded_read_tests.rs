use super::super::tests::{Fixture, object};
use super::*;
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 << 20, 0).unwrap()
}
fn work() -> PhysicalReadWorkV1 {
    PhysicalReadWorkV1 {
        remaining_steps: 1_000_000,
        remaining_read_bytes: 64 << 20,
        remaining_metadata_bytes: 64 << 20,
    }
}
fn random_bytes(n: usize) -> Vec<u8> {
    let mut seed = 0x193fea18u64;
    (0..n)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect()
}
fn publish_with_compressible_padding(root: &Path, original: &(String, Vec<u8>)) {
    let mut objects = vec![original.clone(), object(vec![13; 360_000])];
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(persist(root, &objects).unwrap());
}
#[test]
fn actual_unknown_length_compressed_and_raw_wire_hash_and_fee() {
    for (data, compressed) in [(vec![79; 180_000], true), (random_bytes(180_000), false)] {
        let fixture = Fixture::new();
        let original = object(data);
        publish_with_compressible_padding(&fixture.0, &original);
        let memory = memory();
        let mut w = work();
        let path = fixture.0.join(&original.0);
        let (_, recipe) = selector(&path, &original.0).unwrap().unwrap();
        let span = recipe.objects.iter().find(|v| v.id == original.0).unwrap();
        assert!(
            recipe
                .frames
                .iter()
                .scan(0u64, |position, frame| {
                    let start = *position;
                    *position += u64::from(frame.length);
                    Some((start, *position, frame))
                })
                .any(|(start, end, frame)| start < span.start + span.length
                    && end > span.start
                    && frame.compressed == compressed),
            "target object must overlap an actual requested codec branch"
        );
        let out = read_unknown_length_durable_v1(
            &path,
            &original.0,
            original.1.len(),
            &memory,
            &mut w,
            &mut || false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(&**out, &original.1);
        assert!(out.uses_budget(&memory));
        assert_eq!(memory.used(), out.reserved_bytes());
        assert_eq!(blake3::hash(&out).to_hex().as_str(), original.0);
        assert!(w.remaining_steps < 1_000_000);
        assert!(w.remaining_metadata_bytes < 64 << 20);
        assert!(w.remaining_read_bytes < 64 << 20);
        drop(out);
        assert_eq!(memory.used(), 0);
        FRAME_DECODES.with(|v| v.set(0));
        assert!(
            read_unknown_length_durable_v1(
                &path,
                &original.0,
                original.1.len() - 1,
                &memory,
                &mut work(),
                &mut || false
            )
            .is_err()
        );
        FRAME_DECODES.with(|v| assert_eq!(v.get(), 0, "cap must reject before payload codec"));
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn only_missing_selector_returns_none_and_bad_present_selector_is_error() {
    let fixture = Fixture::new();
    let memory = memory();
    let id = "00".repeat(32);
    let path = fixture.0.join(&id);
    assert!(
        read_unknown_length_durable_v1(&path, &id, 100, &memory, &mut work(), &mut || false)
            .unwrap()
            .is_none()
    );
    fs::write(
        fixture.0.join(SELECTORS).join(&id),
        b"invalid-present-selector",
    )
    .unwrap();
    assert!(
        read_unknown_length_durable_v1(&path, &id, 100, &memory, &mut work(), &mut || false)
            .is_err()
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn fresh_second_call_rejects_frame_corruption_after_success() {
    let fixture = Fixture::new();
    let original = object(vec![41; 100_000]);
    persist(&fixture.0, std::slice::from_ref(&original)).unwrap();
    let memory = memory();
    let path = fixture.0.join(&original.0);
    drop(
        read_unknown_length_durable_v1(
            &path,
            &original.0,
            original.1.len(),
            &memory,
            &mut work(),
            &mut || false,
        )
        .unwrap()
        .unwrap(),
    );
    let (_, recipe) = selector(&path, &original.0).unwrap().unwrap();
    fs::write(
        fixture.0.join(FRAMES).join(&recipe.frames[0].stored),
        b"broken",
    )
    .unwrap();
    assert!(
        read_unknown_length_durable_v1(
            &path,
            &original.0,
            original.1.len(),
            &memory,
            &mut work(),
            &mut || false
        )
        .is_err()
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn fresh_second_call_rejects_selector_mutation_and_alias_conflict() {
    for alias in [false, true] {
        let fixture = Fixture::new();
        let original = object(vec![11; 4096]);
        persist(&fixture.0, std::slice::from_ref(&original)).unwrap();
        let memory = memory();
        let path = fixture.0.join(&original.0);
        drop(
            read_unknown_length_durable_v1(
                &path,
                &original.0,
                4096,
                &memory,
                &mut work(),
                &mut || false,
            )
            .unwrap()
            .unwrap(),
        );
        if alias {
            if path.exists() {
                fs::remove_file(&path).unwrap();
            }
            fs::write(&path, b"unrelated-alias").unwrap();
        } else {
            fs::write(
                fixture.0.join(SELECTORS).join(&original.0),
                b"changed-selector",
            )
            .unwrap();
        }
        assert!(
            read_unknown_length_durable_v1(
                &path,
                &original.0,
                4096,
                &memory,
                &mut work(),
                &mut || false
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn continuous_steps_io_metadata_and_memory_exhaustion_return_no_owner() {
    let fixture = Fixture::new();
    let original = object(vec![12; 10_000]);
    persist(&fixture.0, std::slice::from_ref(&original)).unwrap();
    let path = fixture.0.join(&original.0);
    for mode in 0..4 {
        let memory = if mode == 3 {
            MemoryBudget::new(0, 0).unwrap()
        } else {
            memory()
        };
        let mut w = work();
        match mode {
            0 => w.remaining_steps = 0,
            1 => w.remaining_read_bytes = 0,
            2 => w.remaining_metadata_bytes = 0,
            _ => {}
        }
        assert!(
            read_unknown_length_durable_v1(
                &path,
                &original.0,
                10_000,
                &memory,
                &mut w,
                &mut || false
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
    // A real callback trips after entry, during the genuine bounded read pipeline.
    let memory = memory();
    let mut w = work();
    let mut observations = 0;
    assert!(
        read_unknown_length_durable_v1(&path, &original.0, 10_000, &memory, &mut w, &mut || {
            observations += 1;
            observations >= 6
        })
        .is_err()
    );
    assert!(observations >= 6);
    assert!(w.remaining_steps < 1_000_000);
    assert_eq!(memory.used(), 0);
}
#[test]
fn continuous_work_is_not_reset_between_reads() {
    let fixture = Fixture::new();
    let original = object(random_bytes(2000));
    publish_with_compressible_padding(&fixture.0, &original);
    let path = fixture.0.join(&original.0);
    let memory = memory();
    let mut w = work();
    let initial = w.remaining_read_bytes;
    drop(
        read_unknown_length_durable_v1(&path, &original.0, 2000, &memory, &mut w, &mut || false)
            .unwrap()
            .unwrap(),
    );
    let spent = initial - w.remaining_read_bytes;
    assert!(spent > 0);
    w.remaining_read_bytes = spent - 1;
    assert!(
        read_unknown_length_durable_v1(&path, &original.0, 2000, &memory, &mut w, &mut || false)
            .is_err()
    );
    assert_eq!(w.remaining_read_bytes, 0);
    assert_eq!(memory.used(), 0);
}
