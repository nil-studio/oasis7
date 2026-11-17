use super::*;

#[test]
fn owned_normal_work_can_outlive_a_callback_and_move_without_sharing_core() {
    fn assert_owned<T: Send + 'static>() {}
    assert_owned::<NormalCoreExtensionWorkV1>();
    assert_owned::<crate::world::incremental_normal_host_work::NormalHostWorkV1>();

    let common = FormalBoundedWorkV1::new(10, 20).unwrap();
    let mut work = NormalCoreExtensionWorkV1::new(&common, 3, 64);
    let identity = common.identity();
    let (common, work) = std::thread::spawn(move || {
        let view = work.view(&common).unwrap();
        assert_eq!(view.common_work().identity(), identity);
        view.common_work().charge(2, 3, &mut || false).unwrap();
        *view.pending_entries_remaining -= 1;
        (common, work)
    })
    .join()
    .unwrap();
    assert_eq!(work.common_identity(), identity);
    assert_eq!(work.pending_entries_remaining, 2);
    assert_eq!(
        (common.remaining_steps(), common.remaining_read_bytes()),
        (8, 17)
    );
}

#[test]
fn foreign_core_with_equal_allowances_cannot_rebind_owned_work() {
    let common = FormalBoundedWorkV1::new(10, 20).unwrap();
    let foreign = FormalBoundedWorkV1::new(10, 20).unwrap();
    let mut work = NormalCoreExtensionWorkV1::new(&common, 3, 64);
    assert!(matches!(
        work.view(&foreign),
        Err(BodyMutationError::Invalid("normal extension work Core"))
    ));
    assert_eq!(work.pending_entries_remaining, 3);
    assert_eq!(work.common_identity(), common.identity());
    for meter in [&common, &foreign] {
        assert_eq!(
            (meter.remaining_steps(), meter.remaining_read_bytes()),
            (10, 20)
        );
    }
    assert!(work.view(&common).is_ok());
}

#[test]
fn views_and_reborrows_continue_original_pending_and_core_balances() {
    let common = FormalBoundedWorkV1::new(10, 20).unwrap();
    let mut work = NormalCoreExtensionWorkV1::new(&common, 3, 64);
    {
        let mut view = work.view(&common).unwrap();
        assert_eq!(view.max_body_text_bytes, 64);
        *view.pending_entries_remaining -= 1;
        let loan = view.reborrow();
        assert_eq!(*loan.pending_entries_remaining, 2);
        *loan.pending_entries_remaining -= 1;
        loan.common_work().charge(2, 3, &mut || false).unwrap();
    }
    let view = work.view(&common).unwrap();
    assert_eq!(*view.pending_entries_remaining, 1);
    assert!(std::ptr::eq(view.common_work(), &common));
    assert_eq!(
        (common.remaining_steps(), common.remaining_read_bytes()),
        (8, 17)
    );
    *view.pending_entries_remaining -= 1;
    view.common_work().charge(1, 4, &mut || false).unwrap();
    assert_eq!(work.pending_entries_remaining, 0);
    assert_eq!(
        (common.remaining_steps(), common.remaining_read_bytes()),
        (7, 13)
    );
}
