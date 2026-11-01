use super::*;
#[test]
fn custody_metadata_is_exact_owner_local_and_never_a_key_permit() {
    let f = Fixture::new(limits());
    let c = f.create();
    let metadata = f
        .manager
        .custody_connection_metadata("account", "session", &c.id)
        .unwrap();
    assert_eq!(metadata.connection_id, c.id);
    assert_eq!(metadata.preset.id, preset().id);
    for (account, session) in [("other", "session"), ("account", "other")] {
        assert!(
            f.manager
                .custody_connection_metadata(account, session, &c.id)
                .is_err()
        );
    }
    // Local owner metadata cannot call the authority callback while Native's
    // identity mutex is held; Native authenticates the owner before this call.
    f.live.store(false, Ordering::SeqCst);
    assert!(
        f.manager
            .custody_connection_metadata("account", "session", &c.id)
            .is_ok()
    );
    assert!(f.manager.snapshot(&f.principal, &c.id).is_err());
}
#[test]
fn custody_metadata_rejects_active_and_expired_keys_and_weak_lifetime() {
    let f = Fixture::new(limits());
    let c = f.active();
    assert!(
        f.manager
            .custody_connection_metadata("account", "session", &c.id)
            .is_err()
    );
    let fresh = Fixture::new(limits());
    let c = fresh.create();
    fresh.clock.0.store(1300, Ordering::SeqCst);
    assert!(
        fresh
            .manager
            .custody_connection_metadata("account", "session", &c.id)
            .is_err()
    );
    let weak = fresh.manager.downgrade();
    drop(fresh);
    assert!(weak.upgrade().is_none());
}
#[test]
fn custody_preset_selection_rejects_ambiguity_without_owner_fallback() {
    let f = Fixture::new(limits());
    f.create();
    assert!(
        f.manager
            .custody_connection_for_preset("account", "session", "deepseek-fixture")
            .is_ok()
    );
    f.create();
    assert!(matches!(
        f.manager
            .custody_connection_for_preset("account", "session", "deepseek-fixture"),
        Err(ConnectionError::Conflict)
    ));
    assert!(
        f.manager
            .custody_connection_for_preset("other", "session", "deepseek-fixture")
            .is_err()
    );
}
