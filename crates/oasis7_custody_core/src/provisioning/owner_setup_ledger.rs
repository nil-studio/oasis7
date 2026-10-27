use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_setup_outbox(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Option<&OwnerSetupOutboxV1> {
        self.owner_setups.get(id).and_then(|v| v.stages.get(&stage))
    }
    /// Local reservation only; does not assert remote source, Fence or World authority.
    pub fn reserve_owner_setup_stage(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        session_nonce: &str,
        now: u64,
    ) -> Result<OwnerSetupOutboxV1> {
        if stage != OwnerSetupStageV1::CreateTask
            && self.provision.stage() != ProvisionStage::ReadyForSetup
        {
            return Err(invalid());
        }
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        if v.source.as_ref().is_none_or(|s| s.verification.is_none()) {
            return Err(invalid());
        }
        let consumed = v
            .book
            .original(id)
            .and_then(|r| r.consumed_at())
            .ok_or_else(invalid)?;
        if self.frozen()
            || now < consumed
            || now >= v.plan.deadline.0
            || !hex64(session_nonce)
            || self.provision.security_epoch()
                != v.book
                    .original(id)
                    .ok_or_else(invalid)?
                    .input()
                    .request
                    .context
                    .security_version
        {
            return Err(invalid());
        }
        if let Some(original) = v.stages.get(&stage) {
            return Ok(original.clone());
        }
        if stage.previous().is_some_and(|p| {
            v.stages
                .get(&p)
                .and_then(|v| v.result.as_ref())
                .is_none_or(|v| !v.succeeded())
        }) {
            return Err(invalid());
        }
        if v.stages.values().any(|v| v.session_nonce == session_nonce) {
            return Err(invalid());
        }
        let operation_id = stage_operation_id(&v.plan, stage)?;
        let outbox = OwnerSetupOutboxV1 {
            stage,
            source: ProvisionOriginInspectionV1 {
                operation_id: digest(&("oasis7.custody.owner-setup-stage-source.v1", id, stage))?,
                verify_nonce: digest(&(
                    "oasis7.custody.owner-setup-stage-source-nonce.v1",
                    id,
                    stage,
                    session_nonce,
                ))?,
                created_at: DecimalU64(now),
                expires_at: DecimalU64(
                    now.checked_add(30_000)
                        .ok_or_else(invalid)?
                        .min(v.plan.deadline.0)
                        .min(
                            v.book
                                .original(id)
                                .and_then(|r| r.verification())
                                .ok_or_else(invalid)?
                                .verification
                                .source_session_idle_expires_at
                                .0,
                        )
                        .min(
                            v.book
                                .original(id)
                                .and_then(|r| r.verification())
                                .ok_or_else(invalid)?
                                .verification
                                .source_session_expires_at
                                .0,
                        ),
                ),
                claimed_at: None,
                request_bytes: None,
                verification: None,
                verified_at: None,
            },
            operation: Operation::admit(
                operation_id,
                digest(&(
                    "oasis7.custody.owner-setup-stage.v1",
                    v.plan.digest()?,
                    stage,
                ))?,
            )
            .map_err(|_| invalid())?,
            original: None,
            world: None,
            access_claimed_at: None,
            access: None,
            access_recorded_at: None,
            observation: None,
            observation_bytes: None,
            candidate_bytes: None,
            admission_session: None,
            admitted_at: None,
            admission_callback_claimed_at: None,
            session: None,
            original_bytes: None,
            published_at: None,
            claimed_at: None,
            result: None,
            last_observation: None,
            last_observation_session: None,
            last_observed_at: None,
            canonical_history: BTreeMap::new(),
            reserved_at: DecimalU64(now),
            oracle_reads: 0,
            callback_claimed_at: None,
            lookup_claimed_at: None,
            lookup_reserved_at: None,
            completed_at: None,
            session_nonce: session_nonce.into(),
        };
        let mut next = self.clone();
        let v = next.owner_setups.get_mut(id).ok_or_else(invalid)?;
        v.transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(OWNER_SETUP_STAGE_TRANSPORT_COST)
            .map_err(|_| invalid())?;
        v.stages.insert(stage, outbox.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(outbox)
    }
    pub fn reserve_owner_setup_read(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        purpose: OwnerSetupReadPurposeV1,
        static_role: Role,
        now: u64,
    ) -> Result<OwnerSetupReadTicket> {
        self.owner_setup_local(id, now)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.result.is_some() || o.oracle_reads >= OWNER_SETUP_MAX_ORACLE_READS {
            return Err(invalid());
        }
        if o.oracle_reads == 2 && o.access_claimed_at.is_none()
            || o.oracle_reads >= 3 && o.access.is_none()
        {
            return Err(invalid());
        }
        let expected = if o.oracle_reads < 2 {
            OwnerSetupReadPurposeV1::Session
        } else {
            OwnerSetupReadPurposeV1::Sign
        };
        if purpose != expected {
            return Err(invalid());
        }
        OwnerSetupReadTicket {
            stage,
            ticket: o.oracle_reads + 1,
            purpose,
        }
        .validate_role(static_role)?;
        o.oracle_reads += 1;
        let ticket = o.oracle_reads;
        next.validate_persistence()?;
        *self = next;
        Ok(OwnerSetupReadTicket {
            stage,
            ticket,
            purpose,
        })
    }
    pub fn reserve_owner_setup_callback(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        now: u64,
    ) -> Result<()> {
        self.owner_setup_local(id, now)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.claimed_at.is_none() || o.callback_claimed_at.is_some() || o.result.is_some() {
            return Err(invalid());
        }
        o.callback_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn owner_setup_local(&self, id: &str, now: u64) -> Result<()> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        if self.frozen()
            || now >= v.plan.deadline.0
            || now < r.consumed_at().ok_or_else(invalid)?
            || self.provision.security_epoch() != r.input().request.context.security_version
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn authorize_owner_setup_read(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        static_role: Role,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupPermitViewV1> {
        read.validate_role(static_role)?;
        let checks = if read.purpose == OwnerSetupReadPurposeV1::Sign {
            self.owner_setup_access_current_checks(id, read.stage, current, now)?
        } else {
            self.owner_setup_stage_checks(id, read.stage, current.provision, now)?
        };
        verifier.verify(&checks)?;
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = v.stages.get(&read.stage).ok_or_else(invalid)?;
        if read.ticket == 0 || read.ticket > o.oracle_reads || o.result.is_some() {
            return Err(invalid());
        }
        self.owner_setup_view(id, read, current.provision, now)
    }
    pub(super) fn owner_setup_view(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<OwnerSetupPermitViewV1> {
        let stage = read.stage;
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = v.stages.get(&stage).ok_or_else(invalid)?;
        let source = o.source.verification.as_ref().ok_or_else(invalid)?;
        let expires = v.plan.deadline.0.min(
            source
                .verification
                .observed_at
                .0
                .checked_add(5000)
                .ok_or_else(invalid)?,
        );
        if now >= expires {
            return Err(invalid());
        }
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let source_revision = digest(&(
            "oasis7.custody.owner-setup-source-revision.v1",
            &current.approval.source,
        ))?;
        let mut descriptor = participant.descriptor.clone();
        descriptor.session = v.session.clone().ok_or_else(invalid)?;
        Ok(OwnerSetupPermitViewV1 {
            setup_id: v.plan.setup_id.clone(),
            kind: if read.purpose == OwnerSetupReadPurposeV1::Session {
                OwnerSetupPermitKindV1::Session
            } else {
                OwnerSetupPermitKindV1::Sign
            },
            stage,
            read_purpose: read.purpose,
            plan: v.plan.clone(),
            participant: descriptor,
            participant_access: if read.purpose == OwnerSetupReadPurposeV1::Session {
                self.owner_setup_participant_access(id)?
            } else {
                self.owner_setup_stage_access(id, stage)?
            },
            original: o.original.clone(),
            observation: o.observation.clone(),
            access_nonces: self.owner_setup_access_nonces(id, stage)?,
            preflight_session_observation_nonce: preflight_nonce(id, stage, &o.session_nonce)?,
            session_observation_nonce: o.session_nonce.clone(),
            lookup_session_observation_nonce: if matches!(
                stage,
                OwnerSetupStageV1::CreateTask
                    | OwnerSetupStageV1::PublishGoal
                    | OwnerSetupStageV1::Delegation
                    | OwnerSetupStageV1::Bind
            ) {
                Some(lookup_nonce(id, stage, &o.session_nonce)?)
            } else {
                None
            },
            source_revision: source_revision.clone(),
            authority_revision: digest(&(
                "oasis7.custody.owner-setup-authority.v1",
                &v.plan,
                &current.approval.authority,
                &source_revision,
            ))?,
            service_epoch: current.approval.authority.service_epoch,
            target_security_epoch: current.approval.authority.security_version,
            observed_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
        })
    }
}

pub(super) fn stage_operation_id(
    plan: &OwnerSetupPlanV1,
    stage: OwnerSetupStageV1,
) -> Result<String> {
    Ok(match stage {
        OwnerSetupStageV1::CreateTask => plan.create_operation_id.clone(),
        OwnerSetupStageV1::PrepareGoal => plan.prepare_operation_id.clone(),
        OwnerSetupStageV1::PublishGoal => plan.publish_operation_id.clone(),
        OwnerSetupStageV1::Delegation => plan.delegation_operation_id.clone(),
        OwnerSetupStageV1::Bind => plan.bind_operation_id.clone(),
        OwnerSetupStageV1::FinalObservation => digest(&(
            "oasis7.custody.owner-setup-final.v1",
            &plan.setup_id,
            &plan.provision_id,
        ))?,
    })
}

pub(super) fn lookup_nonce(id: &str, stage: OwnerSetupStageV1, nonce: &str) -> Result<String> {
    digest(&(
        "oasis7.custody.owner-setup-lookup-session.v1",
        id,
        stage,
        nonce,
    ))
}

pub(super) fn preflight_nonce(id: &str, stage: OwnerSetupStageV1, nonce: &str) -> Result<String> {
    digest(&(
        "oasis7.custody.owner-setup-preflight-session.v1",
        id,
        stage,
        nonce,
    ))
}
