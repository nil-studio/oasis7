use super::super::super::tests::{Fixture, object as logical_object};
use super::*;
fn finish(root: &Path) -> String {
    let mut session = GenerationSession::default();
    for _ in 0..500 {
        let progress = advance_with_session(root, &mut session).unwrap();
        if !progress.building {
            return progress.selected.unwrap();
        }
    }
    panic!("generation import did not converge");
}
fn frame_names(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root.join(FRAMES))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}
#[test]
#[cfg(unix)]
fn importing_original_frames_preserves_sharing_destroyed_by_hash_order_rebatch() {
    let source = Fixture::new();
    let regrouped = Fixture::new();
    let mut random = 13u64;
    let prefix = (0..65536)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect::<Vec<_>>();
    let mut objects = Vec::new();
    for value in 0..32u8 {
        let mut bytes = prefix.clone();
        bytes.extend(std::iter::repeat_n(value, 8192));
        let object = logical_object(bytes);
        assert!(persist(&source.0, std::slice::from_ref(&object)).unwrap());
        roots::retain(&source.0, std::slice::from_ref(&object.0)).unwrap();
        objects.push(object);
    }
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(persist(&regrouped.0, &objects).unwrap());
    let before = crate::storage_inventory::inventory_storage(&source.0);
    let regrouped_cost = crate::storage_inventory::inventory_storage(&regrouped.0);
    assert!(before.complete && regrouped_cost.complete);
    assert!(
        regrouped_cost.allocated_bytes.unwrap() > before.allocated_bytes.unwrap() * 2,
        "fixture must reproduce actual filesystem growth from re-chunking"
    );
    let original_frames = frame_names(&source.0);
    let identities = original_frames
        .iter()
        .map(|id| {
            let metadata = fs::symlink_metadata(source.0.join(FRAMES).join(id)).unwrap();
            #[cfg(unix)]
            let identity = (metadata.dev(), metadata.ino());
            #[cfg(not(unix))]
            let identity = (metadata.len(), metadata.len());
            (id.clone(), identity)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let selected = finish(&source.0);
    let target = source.0.join(GENERATIONS).join(selected);
    assert_eq!(frame_names(&target), original_frames);
    assert!(frame_names(&source.0).is_empty());
    for id in original_frames {
        let metadata = fs::symlink_metadata(target.join(FRAMES).join(&id)).unwrap();
        #[cfg(unix)]
        assert_eq!((metadata.dev(), metadata.ino()), identities[&id]);
        #[cfg(not(unix))]
        assert_eq!((metadata.len(), metadata.len()), identities[&id]);
    }
    for (id, bytes) in objects {
        assert_eq!(
            super::super::super::super::business_pack::read_object_durable(
                &source.0.join(&id),
                &id
            )
            .unwrap(),
            Some(bytes)
        );
    }
}
#[test]
fn reopened_subset_import_keeps_only_captured_recipe_member_without_extra_alias() {
    let source = Fixture::new();
    let mut objects = vec![
        logical_object(vec![1; 100000]),
        logical_object(vec![2; 100000]),
    ];
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(persist(&source.0, &objects).unwrap());
    super::super::super::initialize(&source.0).unwrap();
    // Physical publication permits a missing optional root alias. Build a
    // complete shared batch explicitly, then orphan precisely one member.
    for (id, _) in &objects {
        let selected = source.0.join(SELECTORS).join(id);
        assert!(selected.exists());
        let (_, recipe) = selector(&source.0.join(id), id).unwrap().unwrap();
        assert_eq!(recipe.objects.len(), 2);
        for (member, _) in &objects {
            assert!(recipe.objects.iter().any(|span| &span.id == member));
        }
        assert!(!source.0.join(id).exists());
        fs::hard_link(&selected, source.0.join(id)).unwrap();
        #[cfg(unix)]
        {
            let a = fs::symlink_metadata(&selected).unwrap();
            let b = fs::symlink_metadata(source.0.join(id)).unwrap();
            assert_eq!((a.dev(), a.ino()), (b.dev(), b.ino()));
        }
    }
    let kept = &objects[0];
    let omitted = &objects[1];
    roots::retain(&source.0, std::slice::from_ref(&kept.0)).unwrap();
    fs::remove_file(source.0.join(SELECTORS).join(&omitted.0)).unwrap();
    fs::remove_file(source.0.join(&omitted.0)).unwrap();
    publication::fail_at(2);
    assert!(advance_with_session(&source.0, &mut GenerationSession::default()).is_err());
    let state = load(&source.0).unwrap();
    let target = building(&source.0, &state);
    assert!(target.join(SELECTORS).join(&kept.0).exists());
    assert!(!target.join(SELECTORS).join(&omitted.0).exists());
    assert!(!target.join(&kept.0).exists());
    // A valid logical payload in a different inode is still the wrong alias
    // representation and must not be silently accepted as a recovery repair.
    create(&target.join(&kept.0), &kept.1).unwrap();
    assert!(super::super::super::initialize_generation(&target).is_err());
    fs::remove_file(target.join(&kept.0)).unwrap();
    super::super::super::initialize_generation(&target).unwrap();
    assert!(!target.join(&kept.0).exists());
    assert!(!target.join(&omitted.0).exists());
    assert!(!target.join(SELECTORS).join(&omitted.0).exists());
    assert_eq!(roots::count(&target).unwrap(), 0);
    let selected = finish(&source.0);
    let target = source.0.join(GENERATIONS).join(selected);
    assert!(!target.join(SELECTORS).join(&omitted.0).exists());
    assert_eq!(roots::count(&target).unwrap(), 0);
    assert_eq!(
        read_object(&target.join(&kept.0), &kept.0, true).unwrap(),
        Some(kept.1.clone())
    );
}
#[test]
fn retained_frame_corruption_before_activation_blocks_import_recovery() {
    let source = Fixture::new();
    let object = logical_object(vec![5; 100000]);
    assert!(persist(&source.0, std::slice::from_ref(&object)).unwrap());
    roots::retain(&source.0, std::slice::from_ref(&object.0)).unwrap();
    publication::fail_at(3);
    assert!(advance_with_session(&source.0, &mut GenerationSession::default()).is_err());
    let target = building(&source.0, &load(&source.0).unwrap());
    let frame = frame_names(&target).into_iter().next().unwrap();
    let damaged = target.join(FRAMES).join(frame);
    fs::remove_file(&damaged).unwrap();
    create(&damaged, b"corrupt retained frame").unwrap();
    assert!(advance_with_session(&source.0, &mut GenerationSession::default()).is_err());
    assert!(current(&source.0).unwrap().is_none());
    assert!(source.0.join(BUILD).exists());
    assert_eq!(
        read_object(&source.0.join(&object.0), &object.0, true).unwrap(),
        Some(object.1)
    );
}
#[test]
fn legacy_shared_pack_import_retains_original_inode_layout() {
    let source = Fixture::new();
    let mut random = 29u64;
    let mut objects = (0..3)
        .map(|_| {
            logical_object(
                (0..4096)
                    .map(|_| {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        random as u8
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    let writes = objects
        .iter()
        .map(|(digest, bytes)| {
            super::super::super::super::business_index::BusinessIndexNodeWriteV1 {
                digest: digest.clone(),
                bytes: bytes.clone(),
            }
        })
        .collect::<Vec<_>>();
    super::super::super::super::business_pack::persist(&source.0, &writes).unwrap();
    for (id, _) in &objects {
        assert!(selector(&source.0.join(id), id).unwrap().is_none());
    }
    #[cfg(unix)]
    let original = {
        let meta = fs::symlink_metadata(source.0.join(&objects[0].0)).unwrap();
        for (id, _) in &objects {
            let other = fs::symlink_metadata(source.0.join(id)).unwrap();
            assert_eq!((other.dev(), other.ino()), (meta.dev(), meta.ino()));
        }
        (meta.dev(), meta.ino())
    };
    let target = source.0.join(GENERATIONS).join(finish(&source.0));
    for (id, bytes) in objects {
        #[cfg(unix)]
        {
            let meta = fs::symlink_metadata(target.join(&id)).unwrap();
            assert_eq!((meta.dev(), meta.ino()), original);
        }
        assert_eq!(
            super::super::super::super::business_pack::read_object_durable(&target.join(&id), &id)
                .unwrap(),
            Some(bytes)
        );
    }
}
#[test]
fn selector_only_generation_reopens_and_rebuilds_from_top_level_retention() {
    let source = Fixture::new();
    let first = logical_object(vec![11; 100000]);
    assert!(persist(&source.0, std::slice::from_ref(&first)).unwrap());
    roots::retain(&source.0, std::slice::from_ref(&first.0)).unwrap();
    let first_selected = finish(&source.0);
    let first_target = source.0.join(GENERATIONS).join(&first_selected);
    assert!(first_target.join(SELECTORS).join(&first.0).exists());
    assert!(!first_target.join(&first.0).exists());
    assert_eq!(roots::count(&first_target).unwrap(), 0);
    super::super::super::initialize_generation(&first_target).unwrap();
    assert!(!first_target.join(&first.0).exists());
    let second = logical_object(vec![12; 100000]);
    assert!(persist(&source.0, std::slice::from_ref(&second)).unwrap());
    roots::retain(&source.0, std::slice::from_ref(&second.0)).unwrap();
    let mut session = GenerationSession::default();
    assert!(
        advance_with_session(&source.0, &mut session)
            .unwrap()
            .building
    );
    drop(session);
    let second_selected = finish(&source.0);
    assert_ne!(second_selected, first_selected);
    assert!(!first_target.exists());
    let second_target = source.0.join(GENERATIONS).join(second_selected);
    super::super::super::initialize_generation(&second_target).unwrap();
    assert_eq!(roots::count(&second_target).unwrap(), 0);
    assert_eq!(roots::count(&source.0).unwrap(), 2);
    for (id, bytes) in [first, second] {
        assert!(!second_target.join(&id).exists());
        assert!(second_target.join(SELECTORS).join(&id).exists());
        assert_eq!(
            super::super::super::super::business_pack::read_object_durable(
                &source.0.join(&id),
                &id
            )
            .unwrap(),
            Some(bytes)
        );
    }
}
