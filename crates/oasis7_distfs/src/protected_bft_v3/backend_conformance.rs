//! Opt-in disposable-namespace backend contract test, independent of any HSM.
//!
//! This is NOT an attestation of protected time, nonexportable key custody,
//! power-loss durability, or anti-rollback protection. An environment owner
//! must separately qualify those properties on the actual protected device.
//! Never call this mutating diagnostic with a production namespace or lease.
use super::backend::{BftSignerAcquireOutcomeV3, BftSignerBackendSnapshotV3, BftSignerBackendV3};
use super::binding::{BftSignerErrorV3, invalid};

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_snapshot(
    snapshot: &BftSignerBackendSnapshotV3,
    binding: &str,
) -> Result<(), BftSignerErrorV3> {
    if snapshot.state.binding_digest != binding
        || !canonical_digest(&snapshot.state.state_digest)
        || snapshot.lease_generation == 0
        || snapshot.protected_time_floor_ms < 0
        || (!snapshot.lease_token_digest.is_empty()
            && !canonical_digest(&snapshot.lease_token_digest))
    {
        return Err(invalid(
            "protected backend test snapshot identity or bounds",
        ));
    }
    Ok(())
}

fn expect_acquired(
    outcome: BftSignerAcquireOutcomeV3,
    predecessor: &BftSignerBackendSnapshotV3,
    token: &str,
    binding: &str,
) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
    let BftSignerAcquireOutcomeV3::Acquired(next) = outcome else {
        return Err(invalid(
            "protected backend test namespace was not exclusively acquired",
        ));
    };
    check_snapshot(&next, binding)?;
    if next.lease_generation <= predecessor.lease_generation
        || next.lease_token_digest != token
        || next.protected_time_floor_ms < predecessor.protected_time_floor_ms
        || next.state != predecessor.state
    {
        return Err(invalid(
            "protected backend lease acquire changed original state or floor",
        ));
    }
    Ok(next)
}

fn expect_released(
    next: &BftSignerBackendSnapshotV3,
    predecessor: &BftSignerBackendSnapshotV3,
    binding: &str,
) -> Result<(), BftSignerErrorV3> {
    check_snapshot(next, binding)?;
    if next.lease_generation <= predecessor.lease_generation
        || !next.lease_token_digest.is_empty()
        || next.protected_time_floor_ms < predecessor.protected_time_floor_ms
        || next.state != predecessor.state
    {
        return Err(invalid(
            "protected backend lease release changed original state or floor",
        ));
    }
    Ok(())
}

/// Exercises only the original lease/floor/reopen API of a preprovisioned,
/// **disposable and otherwise unused** namespace, using two distinct backend
/// instances. Mutation is deliberate; never invoke this in production.
///
/// The factory must reopen independently (e.g. a new process/session), not
/// return the first handle. This function cannot verify that the factory
/// truly provides physical anti-rollback or key nonexportability. The tests
/// below use software state *only* to test this harness's refusal behavior.
///
/// Other original Fresh/Reserve, journal CAS, receipt, time-origin and signed
/// cold-operation requirements remain independent acceptance obligations.
pub fn exercise_disposable_backend_lease_handoff_v3(
    first: &mut dyn BftSignerBackendV3,
    reopen: impl FnOnce() -> Result<Box<dyn BftSignerBackendV3>, BftSignerErrorV3>,
    binding_digest: &str,
    first_token_digest: &str,
    second_token_digest: &str,
) -> Result<(), BftSignerErrorV3> {
    if !canonical_digest(binding_digest)
        || !canonical_digest(first_token_digest)
        || !canonical_digest(second_token_digest)
        || first_token_digest == second_token_digest
    {
        return Err(invalid(
            "protected backend test requires distinct canonical identities",
        ));
    }

    let before = first.load_provisioned(binding_digest)?;
    check_snapshot(&before, binding_digest)?;
    if !before.lease_token_digest.is_empty() {
        return Err(invalid("protected backend test requires an idle namespace"));
    }

    let first_lease = expect_acquired(
        first.acquire_lease(binding_digest, before.lease_generation, first_token_digest)?,
        &before,
        first_token_digest,
        binding_digest,
    )?;
    if first.read_for_lease(first_lease.lease_generation, first_token_digest)? != first_lease {
        return Err(invalid("protected backend first lease readback differs"));
    }

    let sampled_time = first.protected_time_ms(first_lease.lease_generation, first_token_digest)?;
    let timed = first.read_for_lease(first_lease.lease_generation, first_token_digest)?;
    if sampled_time < first_lease.protected_time_floor_ms
        || timed.protected_time_floor_ms < sampled_time
        || timed.state != first_lease.state
        || timed.lease_generation != first_lease.lease_generation
        || timed.lease_token_digest != first_token_digest
    {
        return Err(invalid(
            "protected backend time floor did not survive exact readback",
        ));
    }

    let released = first.release_lease(first_lease.lease_generation, first_token_digest)?;
    expect_released(&released, &timed, binding_digest)?;
    // Reopening is provided by the actual backend owner, not by this harness.
    let mut second = reopen()?;
    let cold = second.load_provisioned(binding_digest)?;
    if cold != released {
        return Err(invalid(
            "protected backend cold reopening changed exact original state",
        ));
    }

    let second_lease = expect_acquired(
        second.acquire_lease(binding_digest, cold.lease_generation, second_token_digest)?,
        &cold,
        second_token_digest,
        binding_digest,
    )?;
    // A fenced owner must not commit even an otherwise well-shaped state.
    // The test deliberately supplies the exact current revision and digest;
    // a backend that checks only these values, but not the lease, is unsafe.
    if first
        .read_for_lease(first_lease.lease_generation, first_token_digest)
        .is_ok()
        || first
            .release_lease(first_lease.lease_generation, first_token_digest)
            .is_ok()
        || first
            .compare_and_swap(
                first_lease.lease_generation,
                first_token_digest,
                second_lease.state.revision,
                &second_lease.state.state_digest,
                None,
                second_lease.state.clone(),
            )
            .is_ok()
    {
        return Err(invalid("protected backend stale owner remains authorized"));
    }
    if second.read_for_lease(second_lease.lease_generation, second_token_digest)? != second_lease {
        return Err(invalid(
            "protected backend new owner lost exact lease readback",
        ));
    }
    let final_release = second.release_lease(second_lease.lease_generation, second_token_digest)?;
    expect_released(&final_release, &second_lease, binding_digest)?;
    if second.load_provisioned(binding_digest)? != final_release {
        return Err(invalid(
            "protected backend final release is not independently readable",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::state::BftSignerStateV3;
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct SoftwareOnlyFixture {
        shared: Arc<Mutex<BftSignerBackendSnapshotV3>>,
        permit_stale_read: bool,
        permit_stale_cas: bool,
    }

    impl SoftwareOnlyFixture {
        fn load(&self) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            Ok(self
                .shared
                .lock()
                .map_err(|_| invalid("fixture lock"))?
                .clone())
        }
    }

    impl BftSignerBackendV3 for SoftwareOnlyFixture {
        fn load_provisioned(
            &mut self,
            binding_digest: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            let value = self.load()?;
            if value.state.binding_digest != binding_digest {
                return Err(invalid("wrong namespace"));
            }
            Ok(value)
        }

        fn acquire_lease(
            &mut self,
            binding_digest: &str,
            expected_generation: u64,
            new_lease_token_digest: &str,
        ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
            let mut state = self.shared.lock().map_err(|_| invalid("fixture lock"))?;
            if state.state.binding_digest != binding_digest {
                return Err(invalid("wrong namespace"));
            }
            if state.lease_generation != expected_generation || !state.lease_token_digest.is_empty()
            {
                return Ok(BftSignerAcquireOutcomeV3::Conflict(state.clone()));
            }
            state.lease_generation += 1;
            state.lease_token_digest = new_lease_token_digest.into();
            Ok(BftSignerAcquireOutcomeV3::Acquired(state.clone()))
        }

        fn release_lease(
            &mut self,
            expected_generation: u64,
            expected_lease_token_digest: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            let mut state = self.shared.lock().map_err(|_| invalid("fixture lock"))?;
            if state.lease_generation != expected_generation
                || state.lease_token_digest != expected_lease_token_digest
            {
                return Err(invalid("stale owner"));
            }
            state.lease_generation += 1;
            state.lease_token_digest.clear();
            Ok(state.clone())
        }

        fn read_for_lease(
            &mut self,
            generation: u64,
            token: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            let state = self.load()?;
            if !self.permit_stale_read
                && (state.lease_generation != generation || state.lease_token_digest != token)
            {
                return Err(invalid("stale owner"));
            }
            Ok(state)
        }

        fn protected_time_ms(
            &mut self,
            generation: u64,
            token: &str,
        ) -> Result<i64, BftSignerErrorV3> {
            let mut state = self.shared.lock().map_err(|_| invalid("fixture lock"))?;
            if state.lease_generation != generation || state.lease_token_digest != token {
                return Err(invalid("stale owner"));
            }
            state.protected_time_floor_ms += 1;
            Ok(state.protected_time_floor_ms)
        }

        fn compare_and_swap(
            &mut self,
            _generation: u64,
            _token: &str,
            _revision: u64,
            _digest: &str,
            _minimum: Option<i64>,
            _next: BftSignerStateV3,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            if self.permit_stale_cas {
                // Deliberately defective adapter: reports a stale write as
                // accepted, so the conformance verifier must reject it.
                return self.load();
            }
            Err(invalid("fixture does not implement protected CAS"))
        }
    }

    fn fixture() -> SoftwareOnlyFixture {
        let binding = "11".repeat(32);
        let state: BftSignerStateV3 = serde_json::from_value(serde_json::json!({
            "schema_version":3, "binding_digest":binding, "revision":0,
            "history":[], "journal":null, "completed_journals":[],
            "pending_driver_apply":null, "applied_installs":[],
            "state_digest":"22".repeat(32)
        }))
        .unwrap();
        SoftwareOnlyFixture {
            shared: Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
                state,
                lease_generation: 1,
                lease_token_digest: String::new(),
                protected_time_floor_ms: 100,
            })),
            permit_stale_read: false,
            permit_stale_cas: false,
        }
    }

    #[test]
    fn disposable_contract_exercises_two_leases_and_cold_readback() {
        let mut first = fixture();
        let factory = first.clone();
        let result = exercise_disposable_backend_lease_handoff_v3(
            &mut first,
            move || Ok(Box::new(factory)),
            &"11".repeat(32),
            &"33".repeat(32),
            &"44".repeat(32),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(first.load().unwrap().lease_generation, 5);
    }

    #[test]
    fn contract_refuses_a_backend_that_keeps_old_owner_readable() {
        let mut first = fixture();
        first.permit_stale_read = true;
        let factory = first.clone();
        assert!(
            exercise_disposable_backend_lease_handoff_v3(
                &mut first,
                move || Ok(Box::new(factory)),
                &"11".repeat(32),
                &"33".repeat(32),
                &"44".repeat(32),
            )
            .is_err()
        );
    }

    #[test]
    fn contract_refuses_stale_compare_and_swap_even_when_revision_matches() {
        let mut first = fixture();
        first.permit_stale_cas = true;
        let factory = first.clone();
        assert!(
            exercise_disposable_backend_lease_handoff_v3(
                &mut first,
                move || Ok(Box::new(factory)),
                &"11".repeat(32),
                &"33".repeat(32),
                &"44".repeat(32),
            )
            .is_err()
        );
    }

    #[test]
    fn contract_refuses_reopened_state_rollback_and_invalid_tokens() {
        let mut first = fixture();
        let rolled_back = fixture();
        assert!(
            exercise_disposable_backend_lease_handoff_v3(
                &mut first,
                move || Ok(Box::new(rolled_back)),
                &"11".repeat(32),
                &"33".repeat(32),
                &"44".repeat(32),
            )
            .is_err()
        );
        let mut fresh = fixture();
        assert!(
            exercise_disposable_backend_lease_handoff_v3(
                &mut fresh,
                || Err(invalid("should not open")),
                &"11".repeat(32),
                &"33".repeat(32),
                &"33".repeat(32),
            )
            .is_err()
        );
        assert_eq!(fresh.load().unwrap().lease_generation, 1);
    }
}
