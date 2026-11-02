//! These are SQLite once-consumption tests, not MFA/Account-prepayment proof.
use super::super::super::{
    model::{IdentityNamespace, RpConfig},
    store::IdentityStore,
};
use super::*;
fn open(path: &std::path::Path) -> IdentityStore {
    IdentityStore::open(
        path,
        IdentityNamespace {
            identity_service_id: "machine-test".into(),
            environment_id: "isolated".into(),
        },
        RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec!["http://localhost:4173".into()],
            version: 1,
            identity_instance_epoch: 1,
        },
    )
    .unwrap()
}
fn issuer() -> AccountMachineIssuerV2 {
    AccountMachineIssuerV2 {
        namespace: "account-test".into(),
        service_id: "account".into(),
        environment: "isolated".into(),
        key_id: "issuer-one".into(),
        issuer_epoch: DecimalU64(1),
        config_generation: DecimalU64(1),
        native_audience_origin: "https://native.invalid".into(),
    }
}
#[test]
fn native_physical_attempt_survives_restart_rotation_and_stage_change() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-cleanup-once-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let mut store = open(&path);
    store
        .transaction(|tx| {
            claim_physical_attempt(
                tx,
                &issuer(),
                "physical-one",
                &"a".repeat(64),
                "registration_world",
            )
        })
        .unwrap();
    drop(store);
    let mut store = open(&path);
    let mut changed = issuer();
    changed.key_id = "issuer-two".into();
    changed.issuer_epoch = DecimalU64(2);
    assert!(
        store
            .transaction(|tx| claim_physical_attempt(
                tx,
                &changed,
                "physical-one",
                &"b".repeat(64),
                "activation_native"
            ))
            .is_err()
    );
    store
        .transaction(|tx| {
            claim_physical_attempt(
                tx,
                &changed,
                "physical-two",
                &"b".repeat(64),
                "activation_native",
            )
        })
        .unwrap();
}
#[test]
fn native_physical_attempt_independent_connections_have_exactly_one_claim() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-cleanup-once-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    drop(open(&path));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = open(&path);
                barrier.wait();
                store
                    .transaction(|tx| {
                        claim_physical_attempt(
                            tx,
                            &issuer(),
                            "physical-once",
                            &"a".repeat(64),
                            "registration_native",
                        )
                    })
                    .is_ok()
            })
        })
        .collect();
    let successes = workers
        .into_iter()
        .map(|w| usize::from(w.join().unwrap()))
        .sum::<usize>();
    assert_eq!(successes, 1);
}

#[test]
fn native_paid_claim_is_not_rolled_back_by_later_observation_failure() {
    // This proves only SQLite attempt durability, not FullMFA or prepaid authority.
    let path = std::env::temp_dir().join(format!(
        "oasis7-cleanup-paid-failure-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let mut store = open(&path);
    store
        .transaction(|tx| {
            claim_physical_attempt(
                tx,
                &issuer(),
                "physical-failure",
                &"a".repeat(64),
                "term_current_native",
            )
        })
        .unwrap();
    let failed: Result<(), StoreError> = store.transaction(|_| Err(unavailable()));
    assert!(failed.is_err());
    drop(store);
    let mut store = open(&path);
    assert!(
        store
            .transaction(|tx| claim_physical_attempt(
                tx,
                &issuer(),
                "physical-failure",
                &"a".repeat(64),
                "term_current_native"
            ))
            .is_err()
    );
    store
        .transaction(|tx| {
            claim_physical_attempt(
                tx,
                &issuer(),
                "physical-fresh",
                &"a".repeat(64),
                "term_current_native",
            )
        })
        .unwrap();
}
