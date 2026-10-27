use super::*;
use oasis7_agent_api::game::GameIdentityNamespaceV1;
fn namespace(binding: &NativeBindingReceiptV1) -> Result<GameIdentityNamespaceV1> {
    let (length, rest) = binding.identity_namespace.split_once(':').ok_or_else(bad)?;
    let count = length.parse::<usize>().map_err(|_| bad())?;
    if count.to_string() != length {
        return Err(bad());
    }
    let service = rest.get(..count).ok_or_else(bad)?;
    let environment = rest
        .get(count..)
        .and_then(|v| v.strip_prefix(':'))
        .ok_or_else(bad)?;
    if service.is_empty() || environment.is_empty() {
        return Err(bad());
    }
    Ok(GameIdentityNamespaceV1 {
        service_id: service.into(),
        environment_id: environment.into(),
        instance_epoch: binding.identity_instance_epoch,
    })
}
impl AccountProvisionApprovalState {
    fn participant_session_candidates(
        &self,
        http_origin: &str,
    ) -> Result<
        Vec<(
            DecimalU64,
            DecimalU64,
            ParticipantSessionRefV1,
            SignedManagedNativeSessionStatusV1,
        )>,
    > {
        let descriptor = self.native_descriptor_with_origin(http_origin)?;
        let history_scope = NativeHistoryScopeV1 {
            role_scope: self
                .role_key_results()
                .get("identity")
                .ok_or_else(bad)?
                .scope
                .clone(),
            identity_namespace: descriptor.registration.identity_namespace.clone(),
            identity_instance_epoch: descriptor.registration.identity_instance_epoch,
            registration_operation_id: descriptor.registration.registration_operation_id.clone(),
            initial_login_operation_id: descriptor.login_request_id.clone(),
        };
        let mut choices = Vec::new();
        for (id, original) in self.native_stages() {
            if let (Some(NativeStageResultV1::SessionStatus(s)), Some(generation)) =
                (&original.result, original.private_sid_generation)
            {
                choices.push((
                    generation,
                    s.status.observed_at,
                    ParticipantSessionRefV1 {
                        scope: history_scope.clone(),
                        phase: NativeHistoryPhaseV1::InitialLogin,
                        source_permit_id: id.clone(),
                        native_operation_id: original.descriptor.login_request_id.clone(),
                        session_ref: s.status.session_ref.clone(),
                        private_sid_generation: generation,
                        native_binding_generation: s.status.generation,
                    },
                    s.clone(),
                ));
            }
        }
        for id in self.managed_logins().keys() {
            if let Some(task) = self.managed_login_task(id) {
                if let (Some(NativeStageResultV1::SessionStatus(s)), Some(generation)) =
                    (&task.result, task.private_sid_generation)
                {
                    choices.push((
                        generation,
                        s.status.observed_at,
                        ParticipantSessionRefV1 {
                            scope: history_scope.clone(),
                            phase: NativeHistoryPhaseV1::ManagedLogin,
                            source_permit_id: id.clone(),
                            native_operation_id: task.login_operation_id.clone(),
                            session_ref: s.status.session_ref.clone(),
                            private_sid_generation: generation,
                            native_binding_generation: s.status.generation,
                        },
                        s.clone(),
                    ));
                }
            }
        }
        for r in self.session_status_recoveries().values() {
            if let (Some(s), Some(generation)) = (&r.result, r.result_private_sid_generation) {
                let h = self
                    .native_history()
                    .get(&r.request.history_digest)
                    .ok_or_else(bad)?;
                choices.push((
                    generation,
                    s.status.observed_at,
                    ParticipantSessionRefV1 {
                        scope: h.scope.clone(),
                        phase: h.phase,
                        source_permit_id: h.result_permit_id.clone(),
                        native_operation_id: h.native_operation_id.clone(),
                        session_ref: s.status.session_ref.clone(),
                        private_sid_generation: generation,
                        native_binding_generation: s.status.generation,
                    },
                    s.clone(),
                ));
            }
        }
        Ok(choices)
    }
    pub(super) fn participant_session(
        &self,
        scope: &NativeStageScope<'_>,
    ) -> Result<(ParticipantSessionRefV1, SignedManagedNativeSessionStatusV1)> {
        let mut choices = self.participant_session_candidates(scope.http_origin)?;
        choices.sort_by_key(|v| (v.0.0, v.1.0));
        let (_, _, reference, status) = choices.pop().ok_or_else(bad)?;
        if !status.status.active
            || status.status.frozen
            || status.status.pending
            || reference.private_sid_generation.0 == 0
        {
            return Err(bad());
        }
        Ok((reference, status))
    }
    pub(super) fn participant_descriptor(
        &self,
        request: &ParticipantProvisionRequestV1,
        nonces: &ParticipantNoncesV1,
        scope: &NativeStageScope<'_>,
    ) -> Result<ParticipantDescriptorV1> {
        let (session, status) = self.participant_session(scope)?;
        self.participant_descriptor_bound(request, nonces, session, status)
    }
    fn participant_descriptor_bound(
        &self,
        request: &ParticipantProvisionRequestV1,
        nonces: &ParticipantNoncesV1,
        session: ParticipantSessionRefV1,
        status: SignedManagedNativeSessionStatusV1,
    ) -> Result<ParticipantDescriptorV1> {
        let binding = status.status.receipt.clone();
        let owner = self.role_key_results().get("owner").ok_or_else(bad)?;
        let identity = self.role_key_results().get("identity").ok_or_else(bad)?;
        let native_namespace = namespace(&binding)?;
        let register = GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: digest(&(
                "oasis7.custody.participant-business.v1",
                &self.draft().entry,
                &binding,
                &owner.public_ref,
            ))?,
            identity_namespace: native_namespace,
            account_id: binding.native_account_id.clone(),
            world_id: self.draft().world_identity.world_id.clone(),
            genesis_digest: self.draft().world_identity.genesis_digest.clone(),
            owner_public_key: owner.public_ref.public_key.clone(),
            owner_key_version: owner.public_ref.key_version,
        };
        register.validate().map_err(|_| bad())?;
        let account = ws::request_digest(
            "participant-account-world",
            &(
                &register.identity_namespace.service_id,
                &register.identity_namespace.environment_id,
                &register.account_id,
                &register.world_id,
                &register.genesis_digest,
            ),
        )
        .map_err(|_| bad())?;
        let agent = format!(
            "participant:{}",
            account.strip_prefix("blake3:").unwrap_or(&account)
        );
        let owner_read = ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some(nonces.owner_view.clone()),
            formal_anchor: None,
            world: ws::WorldIdentity {
                world_id: register.world_id.clone(),
                genesis_digest: register.genesis_digest.clone(),
            },
            scope_id: format!("agent:{agent}"),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: Some(self.draft().plan.expires_at.0),
        };
        owner_read.validate().map_err(|_| bad())?;
        if request.custody_account_id != binding.custody_account_id
            || request.provision_id != binding.provision_id
            || owner.public_ref.key_version.0 != 1
        {
            return Err(bad());
        }
        Ok(ParticipantDescriptorV1 {
            world: self.draft().world_identity.clone(),
            native_binding: binding,
            session,
            owner_scope: owner.scope.clone(),
            owner_key: owner.public_ref.clone(),
            identity_scope: identity.scope.clone(),
            register,
            owner_read,
            policy_nonce: nonces.policy.clone(),
            owner_view_nonce: nonces.owner_view.clone(),
        })
    }
    pub(super) fn validate_participant_descriptor(
        &self,
        original: &ParticipantOriginalV1,
    ) -> Result<()> {
        let reference = &original.descriptor.session;
        let origin = &self
            .native_stages()
            .values()
            .find(|v| {
                v.descriptor.registration.registration_operation_id
                    == reference.scope.registration_operation_id
                    && v.descriptor.login_request_id == reference.scope.initial_login_operation_id
            })
            .ok_or_else(bad)?
            .descriptor
            .http_origin;
        let (_, _, session, status) = self
            .participant_session_candidates(origin)?
            .into_iter()
            .find(|v| &v.2 == reference)
            .ok_or_else(bad)?;
        let nonces = ParticipantNoncesV1 {
            source: original.source_inspection.verify_nonce.clone(),
            policy: original.descriptor.policy_nonce.clone(),
            owner_view: original.descriptor.owner_view_nonce.clone(),
            session: original.session_nonce.clone(),
        };
        if self.participant_descriptor_bound(&original.request, &nonces, session, status)?
            != original.descriptor
        {
            return Err(bad());
        }
        Ok(())
    }
}
