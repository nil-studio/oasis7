//! Account-local TestSession authority; no caller JSON supplies Current.
use super::*;
use oasis7_custody_core::{
    client_enrollment::HostDeploymentAllowlist, provisioning::*, time_units::UnixMillis,
};
#[path = "test_session_store_approval.rs"]
mod approval;
#[path = "test_session_store_preflight.rs"]
mod preflight;

pub(crate) struct TestSessionApprovalOwned {
    pub native: NativeStageCurrent,
    pub enrollment: crate::client_enrollment_reader::FactView,
    pub expected_actor_id: String,
    pub expected_realm: String,
    pub expected_origin: String,
    pub deployments: HostDeploymentAllowlist,
    pub observed_at: u64,
}
impl TestSessionApprovalOwned {
    fn with_core<R>(&self, f: impl FnOnce(&TestSessionApprovalCurrent<'_, '_>) -> R) -> R {
        let enrollment = ProvedEnrollmentObservation {
            actual_actor_id: &self.enrollment.actual_actor_id,
            expected_actor_id: &self.expected_actor_id,
            actual_realm: &self.enrollment.realm,
            expected_realm: &self.expected_realm,
            expected_public_origin: &self.expected_origin,
            fact: &self.enrollment.fact,
        };
        let provision = self.native.provision.as_core();
        let scope = self.native.scope.as_core();
        f(&TestSessionApprovalCurrent {
            provision: &provision,
            native_scope: &scope,
            enrollment: &enrollment,
            deployments: &self.deployments,
            observed_at: UnixMillis::new(self.observed_at),
        })
    }
}
pub(crate) struct TestSessionPreflightOwned {
    pub native: NativeStageCurrent,
    pub source_operation_id: String,
    pub source_ticket: u8,
    pub oracle_ticket: u8,
    pub observed_at: u64,
}
impl TestSessionPreflightOwned {
    fn with_core<R>(&self, f: impl FnOnce(&TestSessionPreflightCurrent<'_, '_>) -> R) -> R {
        let provision = self.native.provision.as_core();
        let scope = self.native.scope.as_core();
        f(&TestSessionPreflightCurrent {
            provision: &provision,
            native_scope: &scope,
            source_operation_id: &self.source_operation_id,
            source_ticket: self.source_ticket,
            oracle_ticket: self.oracle_ticket,
            observed_at: UnixMillis::new(self.observed_at),
        })
    }
}
impl OwnerProvisionApprovalStore {
    pub(crate) fn test_session_local<R>(
        &self,
        id: &str,
        transition: impl FnMut(&mut AccountProvisionApprovalState, UnixMillis) -> Result<R>,
    ) -> Result<R> {
        let mut transition = transition;
        self.local_reservation("test_session_reserved", id, |state, time| {
            transition(state, UnixMillis::new(time))
        })
    }
}
