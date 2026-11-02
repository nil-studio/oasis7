//! Existing FreshAccess Actor RPC, no additional observation or service call.
use super::*;
use crate::account_service_issuer::AccountServiceIssuer;
use oasis7_custody_core::native_cleanup_v2::SignedAccountPreflightAccessV2;
use oasis7_custody_core::provisioning::NativePreflightAccessIssuerV2;
impl OwnerProvisionApprovalStore {
    pub(crate) fn claim_test_session_v2_preflight_world(
        &self,
        id: &str,
        issuer: &AccountServiceIssuer,
    ) -> Result<SignedAccountPreflightAccessV2> {
        self.local_reservation(
            "test_session_v2_world_preflight_claimed",
            id,
            |state, time| {
                let now = oasis7_custody_core::time_units::UnixMillis::new(time);
                let prepared = state
                    .claim_test_session_v2_preflight_world(
                        id,
                        NativePreflightAccessIssuerV2 {
                            issuer: issuer.issuer().clone(),
                            pin: issuer.native_preflight_access_pin()?,
                            floor: &issuer.native_preflight_access_floor(),
                        },
                        &issuer.native_scope(),
                        now,
                    )
                    .map_err(|_| rejected())?;
                let signature = issuer.sign_native_preflight_access(&prepared, time)?;
                state
                    .finish_test_session_v2_preflight_world(prepared, signature, now)
                    .map_err(|_| rejected())
            },
        )
    }
}
