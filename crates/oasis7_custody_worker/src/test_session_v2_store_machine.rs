//! New paid OwnerRead replaces the original RecordLifetime + ClaimStep pair.
use super::*;
use crate::account_service_issuer::AccountServiceIssuer;
use oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1;
use oasis7_custody_core::native_cleanup_v2::NativeReadAndRegisterRequestV2;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn record_lifetime_and_claim_machine_read_v2<F>(
        &self,
        id: &str,
        original: SignedManagedNativeSessionLifetimeV1,
        issuer: &AccountServiceIssuer,
        current: F,
    ) -> Result<NativeReadAndRegisterRequestV2>
    where
        F: Fn() -> Result<TestSessionPreflightOwned>,
    {
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_machine_read",
            id,
            |s, _, _, t| {
                current()?.with_core(|c| {
                    Ok(vec![
                        s.test_session_v2_preflight_lifetime_checks(
                            id,
                            &original,
                            c,
                            UnixMillis::new(t),
                        )
                        .map_err(|_| rejected())?,
                        s.test_session_v2_preflight_checks(id, c, UnixMillis::new(t))
                            .map_err(|_| rejected())?,
                    ])
                })
            },
            |s, _, _, t, v| {
                current()?.with_core(|c| {
                    let now = UnixMillis::new(t);
                    s.record_test_session_v2_preflight_lifetime(id, original.clone(), c, now, v)
                        .map_err(|_| rejected())?;
                    s.claim_test_session_v2_preflight_step(
                        id,
                        SessionPreflightStepV1::OwnerRead,
                        now,
                    )
                    .map_err(|_| rejected())?;
                    let prepared = s
                        .prepare_test_session_v2_machine_pending(
                            id,
                            NativeMachinePendingIssuerV2 {
                                issuer: issuer.issuer().clone(),
                                pin: issuer.native_pending_pin()?,
                                floor: &issuer.native_pending_floor(),
                            },
                            c,
                            now,
                            v,
                        )
                        .map_err(|_| rejected())?;
                    let signature = issuer.sign_native_pending(&prepared, t)?;
                    s.finish_test_session_v2_machine_pending(prepared, signature, c, now, v)
                        .map_err(|_| rejected())?;
                    s.test_session_v2_machine_read_original(id)
                        .map_err(|_| rejected())
                })
            },
        )
        .await
    }
}

struct MachineActivationMaterialV2 {
    lifetime: SignedManagedNativeSessionLifetimeV1,
    world: OwnerSetupWorldEvidenceV1,
    private_sid_generation: DecimalU64,
    source: String,
}
impl MachineActivationMaterialV2 {
    fn with_core<R>(
        &self,
        owned: &TestSessionApprovalOwned,
        f: impl FnOnce(&TestSessionCurrent<'_, '_>) -> R,
    ) -> R {
        owned.with_core(|a| {
            f(&TestSessionCurrent {
                approval: TestSessionApprovalCurrent {
                    provision: a.provision,
                    native_scope: a.native_scope,
                    enrollment: a.enrollment,
                    deployments: a.deployments,
                    observed_at: a.observed_at,
                },
                lifetime: &self.lifetime,
                private_sid_generation: self.private_sid_generation,
                world: &self.world,
                source_operation_id: &self.source,
            })
        })
    }
}
impl OwnerProvisionApprovalStore {
    pub(crate) async fn claim_registered_native_activation_v2<F>(
        &self,
        id: &str,
        reply: oasis7_custody_core::native_cleanup_v2::NativeReadAndRegisterReplyV2,
        lifetime: SignedManagedNativeSessionLifetimeV1,
        world: OwnerSetupWorldEvidenceV1,
        private_sid_generation: DecimalU64,
        source: String,
        issuer: &AccountServiceIssuer,
        current: F,
    ) -> Result<oasis7_custody_core::provisioning::NativeMachineActivationDispatchV2>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let material = MachineActivationMaterialV2 {
            lifetime,
            world,
            private_sid_generation,
            source,
        };
        let saved = self.snapshot()?;
        let session = saved
            .state
            .test_session_v2_snapshot(id)
            .map_err(|_| rejected())?;
        let deadline = super::super::alarm::write_deadline(session.input())?;
        self.native_verified_with_alarm_traced(
            &|| Ok(current()?.native),
            ("test_session_native_activation", id),
            deadline,
            |s, _, _, t| {
                material.with_core(&current()?, |c| {
                    s.activate_test_session_v2_checks(id, c, UnixMillis::new(t))
                        .map_err(|_| rejected())
                })
            },
            |s, _, _, t, v| {
                material.with_core(&current()?, |c| {
                    let now = UnixMillis::new(t);
                    s.activate_test_session_v2_registered_machine(id, reply.clone(), c, now, v)
                        .map_err(|_| rejected())?;
                    let prepared = s
                        .prepare_test_session_v2_machine_active(
                            id,
                            issuer.native_active_pin()?,
                            &issuer.native_pending_floor(),
                            c,
                            now,
                            v,
                        )
                        .map_err(|_| rejected())?;
                    let signature = issuer.sign_native_active(&prepared, t)?;
                    s.finish_test_session_v2_machine_active(prepared, signature, now)
                        .map_err(|_| rejected())
                })
            },
            #[cfg(feature = "approval-harness")]
            None,
        )
        .await
    }
    pub(crate) async fn admit_native_activation_v2<F>(
        &self,
        id: &str,
        ack: oasis7_custody_core::native_cleanup_v2::SignedNativeActivationAckV2,
        lifetime: SignedManagedNativeSessionLifetimeV1,
        world: OwnerSetupWorldEvidenceV1,
        private_sid_generation: DecimalU64,
        source: String,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let material = MachineActivationMaterialV2 {
            lifetime,
            world,
            private_sid_generation,
            source,
        };
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_native_ready",
            id,
            |s, _, _, t| {
                material.with_core(&current()?, |c| {
                    s.activate_test_session_v2_checks(id, c, UnixMillis::new(t))
                        .map_err(|_| rejected())
                })
            },
            |s, _, _, t, v| {
                material.with_core(&current()?, |c| {
                    s.admit_test_session_v2_native_activation(
                        id,
                        ack.clone(),
                        c,
                        UnixMillis::new(t),
                        v,
                    )
                    .map_err(|_| rejected())
                })
            },
        )
        .await
    }
}
