use super::*;
use oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1;
use oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1;
impl OwnerProvisionApprovalStore {
    pub(crate) fn claim_test_session_preflight(
        &self,
        id: &str,
    ) -> Result<TestSessionPreflightDescriptorV1> {
        self.test_session_local(id, |s, t| {
            s.claim_test_session_preflight(id, t)
                .map_err(|_| rejected())
        })
    }
    pub(crate) fn claim_test_session_preflight_step(
        &self,
        id: &str,
        step: SessionPreflightStepV1,
    ) -> Result<TestSessionPreflightDescriptorV1> {
        self.test_session_local(id, |s, t| {
            s.claim_test_session_preflight_step(id, step, t)
                .map_err(|_| rejected())
        })
    }
    /// Two local credits are consumed atomically before any remote observation.
    pub(crate) fn reserve_test_session_preflight_oracle(
        &self,
        id: &str,
        source: &str,
        role: Role,
    ) -> Result<(u8, u8)> {
        let purpose = match role {
            Role::Identity => OwnerSetupReadPurposeV1::Session,
            Role::Owner => OwnerSetupReadPurposeV1::Sign,
            _ => return Err(rejected()),
        };
        self.test_session_local(id, |s, t| {
            let source_ticket = s
                .reserve_test_session_source_read(id, source, t)
                .map_err(|_| rejected())?;
            let oracle_ticket = s
                .reserve_test_session_preflight_read(id, purpose, role, t)
                .map_err(|_| rejected())?;
            Ok((source_ticket, oracle_ticket))
        })
    }
    pub(crate) async fn test_session_preflight_permit<F>(
        &self,
        id: &str,
        role: Role,
        access: Option<OwnerSetupAccessEvidenceV1>,
        current: F,
    ) -> Result<(
        TestSessionPreflightPermitV1,
        Option<TestSessionPreflightSigningBundleV1>,
    )>
    where
        F: Fn() -> Result<TestSessionPreflightOwned>,
    {
        let purpose = match role {
            Role::Identity => OwnerSetupReadPurposeV1::Session,
            Role::Owner => OwnerSetupReadPurposeV1::Sign,
            _ => return Err(rejected()),
        };
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_oracle",
            id,
            |s, _, _, t| {
                current()?.with_core(|c| {
                    let mut jobs = vec![
                        s.test_session_preflight_checks(id, c, UnixMillis::new(t))
                            .map_err(|_| rejected())?,
                    ];
                    if c.oracle_ticket == 3 {
                        let a = access.as_ref().ok_or_else(rejected)?;
                        jobs.push(
                            s.test_session_preflight_access_checks(id, a, c, UnixMillis::new(t))
                                .map_err(|_| rejected())?,
                        );
                    } else if access.is_some() {
                        return Err(rejected());
                    } else if c.oracle_ticket == 4 {
                        let stored = s
                            .test_sessions()
                            .get(id)
                            .and_then(|s| s.preflight_access.as_ref())
                            .ok_or_else(rejected)?;
                        jobs.push(
                            s.test_session_preflight_access_checks(
                                id,
                                stored,
                                c,
                                UnixMillis::new(t),
                            )
                            .map_err(|_| rejected())?,
                        );
                    }
                    Ok(jobs)
                })
            },
            |s, _, _, t, v| {
                current()?.with_core(|c| {
                    if c.oracle_ticket == 3 {
                        s.record_test_session_preflight_access(
                            id,
                            access.clone().ok_or_else(rejected)?,
                            c,
                            UnixMillis::new(t),
                            v,
                        )
                        .map_err(|_| rejected())?;
                    }
                    let permit = s
                        .test_session_preflight_permit(id, role, purpose, c, UnixMillis::new(t), v)
                        .map_err(|_| rejected())?;
                    let signing = if role == Role::Owner {
                        Some(
                            s.test_session_preflight_signing_bundle(id, c, UnixMillis::new(t), v)
                                .map_err(|_| rejected())?,
                        )
                    } else {
                        None
                    };
                    Ok((permit, signing))
                })
            },
        )
        .await
    }
    pub(crate) async fn publish_test_session_read<F>(
        &self,
        id: &str,
        original: ParticipantOwnerViewRequestV1,
        current: F,
    ) -> Result<crate::owner_test_session_rpc::TestSessionPublishedRead>
    where
        F: Fn() -> Result<TestSessionPreflightOwned>,
    {
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_read_published",
            id,
            |s, _, _, t| {
                current()?.with_core(|c| {
                    Ok(vec![
                        s.test_session_preflight_checks(id, c, UnixMillis::new(t))
                            .map_err(|_| rejected())?,
                        s.test_session_preflight_publish_checks(
                            id,
                            &original,
                            c,
                            UnixMillis::new(t),
                        )
                        .map_err(|_| rejected())?,
                    ])
                })
            },
            |s, _, _, t, v| {
                current()?.with_core(|c| {
                    s.publish_test_session_preflight_read(
                        id,
                        original.clone(),
                        c,
                        UnixMillis::new(t),
                        v,
                    )
                    .map_err(|_| rejected())?;
                    let permit = s
                        .test_session_preflight_permit(
                            id,
                            Role::Owner,
                            OwnerSetupReadPurposeV1::Sign,
                            c,
                            UnixMillis::new(t),
                            v,
                        )
                        .map_err(|_| rejected())?;
                    let original = s
                        .test_sessions()
                        .get(id)
                        .and_then(|s| s.preflight_read.as_deref())
                        .cloned()
                        .ok_or_else(rejected)?;
                    Ok(
                        crate::owner_test_session_rpc::TestSessionPublishedRead {
                            permit,
                            original,
                        },
                    )
                })
            },
        )
        .await
    }
    pub(crate) async fn record_test_session_lifetime<F>(
        &self,
        id: &str,
        original: SignedManagedNativeSessionLifetimeV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<TestSessionPreflightOwned>,
    {
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_lifetime",
            id,
            |s, _, _, t| {
                current()?.with_core(|c| {
                    Ok(vec![
                        s.test_session_preflight_lifetime_checks(
                            id,
                            &original,
                            c,
                            UnixMillis::new(t),
                        )
                        .map_err(|_| rejected())?,
                    ])
                })
            },
            |s, _, _, t, v| {
                current()?.with_core(|c| {
                    s.record_test_session_preflight_lifetime(
                        id,
                        original.clone(),
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
    pub(crate) async fn activate_test_session<F>(
        &self,
        id: &str,
        lifetime: SignedManagedNativeSessionLifetimeV1,
        world: OwnerSetupWorldEvidenceV1,
        private_sid_generation: DecimalU64,
        source: String,
        current: F,
    ) -> Result<TestSessionAuthorityV1>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_activated",
            id,
            |s, _, _, t| {
                current()?.with_core(|approval| {
                    s.activate_test_session_checks(
                        id,
                        &TestSessionCurrent {
                            approval: TestSessionApprovalCurrent {
                                provision: approval.provision,
                                native_scope: approval.native_scope,
                                enrollment: approval.enrollment,
                                deployments: approval.deployments,
                                observed_at: approval.observed_at,
                            },
                            lifetime: &lifetime,
                            private_sid_generation,
                            world: &world,
                            source_operation_id: &source,
                        },
                        UnixMillis::new(t),
                    )
                    .map_err(|_| rejected())
                })
            },
            |s, _, _, t, v| {
                current()?.with_core(|approval| {
                    s.activate_test_session(
                        id,
                        &TestSessionCurrent {
                            approval: TestSessionApprovalCurrent {
                                provision: approval.provision,
                                native_scope: approval.native_scope,
                                enrollment: approval.enrollment,
                                deployments: approval.deployments,
                                observed_at: approval.observed_at,
                            },
                            lifetime: &lifetime,
                            private_sid_generation,
                            world: &world,
                            source_operation_id: &source,
                        },
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
