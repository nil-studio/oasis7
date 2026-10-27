use super::*;
pub(super) fn pairing(
    intent: &CustodyIntentV1,
) -> Result<(&str, &ClientPairingBindingV1, u64, u64, u64)> {
    let CustodyIntentV1::TestSession {
        session_id,
        client_pairing: Some(p),
        write_deadline,
        read_until,
        cleanup_until,
        ..
    } = intent
    else {
        return Err(invalid());
    };
    p.validate().map_err(|_| invalid())?;
    Ok((
        session_id,
        p,
        write_deadline.0,
        read_until.0,
        cleanup_until.0,
    ))
}
impl AccountProvisionApprovalState {
    pub fn test_sessions(&self) -> &BTreeMap<String, TestSessionAuthorityV1> {
        &self.test_sessions
    }
    pub fn test_session_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.test_sessions.get(id).and_then(|s| s.book.original(id))
    }
    pub fn test_session_world_read(
        &self,
        input: &ApprovalAdmissionInput,
    ) -> Result<ws::ReadWorldViewRequest> {
        let (id, _, _, read, cleanup) = pairing(&input.request.intent)?;
        let p = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut request = p.descriptor.owner_read.clone();
        request.observation_nonce = Some(digest(&(
            "oasis7.custody.test-session-preflight.v1",
            id,
            input.approval.digest().map_err(|_| invalid())?,
        ))?);
        request.deadline_unix_ms = Some(read.max(cleanup));
        request.fixed_commit = None;
        request.formal_anchor = None;
        Ok(request)
    }
    pub(super) fn test_session_scope(
        &self,
        input: &ApprovalAdmissionInput,
        termination: &TerminationPlanV1,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<()> {
        let (id, pair, write, _read, cleanup) = pairing(&input.request.intent)?;
        let c = current.provision.approval;
        let f = current.enrollment.fact;
        let r = &f.registration;
        let target = &r.request.target;
        current.enrollment.validate_physical()?;
        let CustodyIntentV1::TestSession {
            runtime_ref,
            skill_ref,
            source_ref,
            ..
        } = &input.request.intent
        else {
            return Err(invalid());
        };
        if runtime_ref != &r.request.descriptor.restricted_runtime_identity
            || skill_ref != &r.request.descriptor.skill_manifest_digest
            || source_ref != &r.request.descriptor.build_digest
        {
            return Err(invalid());
        }

        let CustodyIntentV1::TestSession {
            allowed_resource_refs,
            ..
        } = &input.request.intent
        else {
            return Err(invalid());
        };
        let prepared = self
            .owner_setups
            .get(&termination.setup_id)
            .ok_or_else(invalid)?;
        if allowed_resource_refs != &prepared.plan.resource_refs {
            return Err(invalid());
        }

        let CustodyIntentV1::TestSession { client_binding, .. } = &input.request.intent else {
            return Err(invalid());
        };
        let matched = match (client_binding, r.request.descriptor.transport_profile) {
            (
                crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key },
                HostTransportProfileV1::NativeBrokerCustodyV1,
            ) => client_key == &pair.client_public_key && pair.connector_public_key.is_none(),
            (
                crate::approval::ApprovedClientBindingV1::DotMcpCustodyV1 {
                    client_key,
                    connector_key,
                },
                HostTransportProfileV1::DotMcpCustodyV1,
            ) => {
                client_key == &pair.client_public_key
                    && pair.connector_public_key.as_ref() == Some(connector_key)
            }
            _ => false,
        };
        if !matched {
            return Err(invalid());
        }

        if now.get() >= write
            || self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || self.provision.stage() != ProvisionStage::Prepared
            || input.operation_id != id
            || input.request.context != c.authority
            || c.authority.security_version != self.provision.security_epoch()
            || c.authority.security_version.0 == 0
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || current.observed_at.get() > now.get()
            || now.get() - current.observed_at.get() > 5000
            || current.provision.world_identity != &self.draft.world_identity
            || current.provision.native_identity_namespace.is_empty()
            || c.authority.custody_account_id != self.draft.entry.custody_account_id
            || c.authority.environment != self.draft.entry.environment
            || current.provision.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.provision.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
            || c.source.manager_account_id != self.draft.entry.manager.native_account_id
            || c.identity_namespace != self.draft.entry.manager.identity_namespace
            || pair.target != *target
            || pair.enrollment_id != r.enrollment_id
            || pair.enrollment_digest != f.digest().map_err(|_| invalid())?
            || pair.client_key_id != r.key_id
            || pair.client_public_key != r.request.descriptor.client_public_key
            || pair.host_instance_id != r.request.descriptor.host_instance_id
            || pair.descriptor_digest != r.descriptor_digest
            || current
                .deployments
                .configured_metadata(&r.request.descriptor)
                .map_err(|_| invalid())?
                .is_none()
            || r.request.descriptor.environment != c.authority.environment
            || now.get() >= r.expires_at.get()
            || termination != &self.test_session_termination_basis(id, UnixMillis::new(cleanup))?
        {
            return Err(invalid());
        }
        let invite = self.pairing_invite(target)?;
        if now.get() >= invite.original.expires_at.get()
            || invite.original.service_epoch != c.authority.service_epoch
            || invite.original.transport_profile != r.request.descriptor.transport_profile
            || invite.original.tool_profile != r.request.descriptor.tool_profile
            || invite
                .consumed_session
                .as_ref()
                .is_some_and(|old| old != id)
        {
            return Err(invalid());
        }
        let CustodyIntentV1::TestSession {
            native_account_id,
            world_identity,
            task_fence,
            executor_key_ref,
            termination_plan_digest,
            ..
        } = &input.request.intent
        else {
            return Err(invalid());
        };
        let (_, status) = self.participant_session(current.native_scope)?;
        if status.status.receipt.native_account_id != *native_account_id {
            return Err(invalid());
        }
        if world_identity.as_ref() != &termination.world
            || task_fence.as_ref() != &termination.initial_fence
            || executor_key_ref != &termination.executor_key_ref
            || termination_plan_digest != &termination.digest()?
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn test_session_activation_scope(
        &self,
        input: &ApprovalAdmissionInput,
        termination: &TerminationPlanV1,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<()> {
        self.test_session_scope(input, termination, &current.approval, now)?;
        let (id, _, write, read, cleanup) = pairing(&input.request.intent)?;
        let c = current.approval.provision.approval;
        let (session, baseline) = self.participant_session(current.approval.native_scope)?;
        let CustodyIntentV1::TestSession {
            native_account_id, ..
        } = &input.request.intent
        else {
            return Err(invalid());
        };
        let lifetime = &current.lifetime.lifetime;
        let receipt = &lifetime.status.receipt;
        let nonce = digest(&(
            "oasis7.custody.test-session-native-lifetime.v1",
            id,
            input.approval.digest().map_err(|_| invalid())?,
        ))?;
        lifetime
            .validate_for(
                &baseline.status.receipt,
                &nonce,
                session.native_binding_generation,
                c.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        lifetime
            .covers(UnixMillis::new(write.max(read).max(cleanup)))
            .map_err(|_| invalid())?;
        if receipt.native_account_id != *native_account_id
            || receipt.custody_account_id != self.draft.entry.custody_account_id
            || receipt.provision_id != self.draft.entry.provision_id
            || lifetime.status.session_ref != session.session_ref
            || current.private_sid_generation != session.private_sid_generation
            || receipt.identity_namespace != current.approval.provision.native_identity_namespace
        {
            return Err(invalid());
        }
        let w = current.world;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        if w.request.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner {
            return Err(invalid());
        }

        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        let expected = self.test_session_world_read(input)?;
        let saved = self.test_sessions.get(id).ok_or_else(invalid)?;
        if saved.preflight_lifetime.as_ref() != Some(current.lifetime) {
            return Err(invalid());
        }

        if saved.preflight_read.as_ref().map(|r| r.as_ref()) != Some(&w.request)
            || !saved
                .preflight_steps
                .contains_key(&SessionPreflightStepV1::OwnerRead)
        {
            return Err(invalid());
        }
        if w.request.read.request != expected
            || w.request.read.subject_public_key != termination.initial_fence.owner_public_key
            || w.response.request_digest
                != ws::request_digest(
                    ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                    &w.request,
                )
                .map_err(|_| invalid())?
            || w.response
                .payload
                .control
                .view
                .game_task_control
                .as_ref()
                .map(GameTaskFenceV1::from_control)
                .as_ref()
                != Some(&termination.initial_fence)
            || w.response.payload.signer_delegation.as_ref()
                != Some(&termination.original_delegation)
        {
            return Err(invalid());
        }
        let state = self.test_sessions.get(id).ok_or_else(invalid)?;
        let source = state
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        super::source::validate_inspection_record(
            state.book.original(id).ok_or_else(invalid)?,
            source,
            source.verification.as_ref().ok_or_else(invalid)?,
            c,
            now.get(),
        )?;
        Ok(())
    }
}
