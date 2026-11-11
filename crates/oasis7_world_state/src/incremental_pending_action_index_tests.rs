use super::*;
fn meta(i: u64) -> PendingActionMetaV1 {
    PendingActionMetaV1 {
        position: Position { era: 0, ordinal: i },
        action_id: 7,
        original: OriginalArtifactRefV1 {
            id: [0; 32],
            byte_len: 0,
        },
        value_root: [0; 32],
    }
}
#[test]
fn persistent_ordered_snapshots_and_balanced_deletion() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
    let mut w = PendingActionIndexWorkV1 {
        steps_remaining: 100000,
    };
    let mut c = || false;
    let mut index = PendingActionIndexV1::new(&memory, MemoryClass::Ordinary);
    for i in 0..128 {
        index = index.insert(meta(i), &mut w, &mut c).unwrap();
    }
    let snapshot = index.clone();
    for i in [0, 64, 127] {
        index = index.remove(meta(i).position, &mut w, &mut c).unwrap();
    }
    assert_eq!(snapshot.len(), 128);
    assert_eq!(index.len(), 125);
    assert!(height(&index.root) < 10);
    assert_eq!(
        index
            .first(&mut w, &mut c)
            .unwrap()
            .unwrap()
            .position
            .ordinal,
        1
    );
    assert_eq!(
        index
            .last(&mut w, &mut c)
            .unwrap()
            .unwrap()
            .position
            .ordinal,
        126
    );
    let mut ids = Vec::new();
    index
        .visit_page(Some(meta(62).position), 3, &mut w, &mut c, |m| {
            ids.push(m.position.ordinal);
            Ok(())
        })
        .unwrap();
    assert_eq!(ids, [63, 65, 66]);
    assert!(
        snapshot
            .get(meta(64).position, &mut w, &mut c)
            .unwrap()
            .is_some()
    );
    assert!(
        index
            .get(meta(64).position, &mut w, &mut c)
            .unwrap()
            .is_none()
    );
}
#[test]
fn work_cancellation_and_heap_failure_leave_parent_usable() {
    let memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let index = PendingActionIndexV1::new(&memory, MemoryClass::Ordinary);
    assert!(
        index
            .insert(
                meta(1),
                &mut PendingActionIndexWorkV1 { steps_remaining: 0 },
                &mut || false
            )
            .is_err()
    );
    assert!(
        index
            .insert(
                meta(1),
                &mut PendingActionIndexWorkV1 {
                    steps_remaining: 100
                },
                &mut || true
            )
            .is_err()
    );
    let tiny = MemoryBudget::new(1, 1).unwrap();
    assert!(
        PendingActionIndexV1::new(&tiny, MemoryClass::Ordinary)
            .insert(
                meta(1),
                &mut PendingActionIndexWorkV1 {
                    steps_remaining: 100
                },
                &mut || false
            )
            .is_err()
    );
    assert_eq!(index.len(), 0);
}
#[test]
fn recovery_readmission_copies_every_node_and_rejects_wrong_pool() {
    let memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let mut w = PendingActionIndexWorkV1 {
        steps_remaining: 10000,
    };
    let mut c = || false;
    let recovery = PendingActionIndexV1::new(&memory, MemoryClass::Recovery)
        .insert(meta(1), &mut w, &mut c)
        .unwrap();
    let ordinary = recovery
        .readmit(&memory, MemoryClass::Ordinary, &mut w, &mut c)
        .unwrap();
    assert!(!Arc::ptr_eq(
        recovery.root.as_ref().unwrap(),
        ordinary.root.as_ref().unwrap()
    ));
    assert_eq!(ordinary.class(), MemoryClass::Ordinary);
    assert!(
        recovery
            .readmit(
                &MemoryBudget::new(2 << 20, 1 << 20).unwrap(),
                MemoryClass::Ordinary,
                &mut w,
                &mut c
            )
            .is_err()
    );
    assert!(
        recovery
            .readmit(&memory, MemoryClass::Recovery, &mut w, &mut c)
            .is_err()
    );
    let weak = Arc::downgrade(ordinary.root.as_ref().unwrap());
    let clone = ordinary.clone();
    drop(ordinary);
    assert!(weak.upgrade().is_some());
    drop(clone);
    assert!(weak.upgrade().is_none());
}
