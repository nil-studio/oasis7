//! Shared authorizer completion after a fixed, strong World policy response.
//! The caller must have paid and validated before obtaining that response.
use super::*;
impl IdentityService {
    pub(in super::super) fn validate_preflight_participant_owner(
        &self,
        account_id: &str,
        owner: &SignedReadRequest<GameParticipantRegisterV1>,
    ) -> Result<(), StoreError> {
        let entry = self
            .participant_entry
            .as_ref()
            .ok_or_else(|| reject("participant_unavailable"))?;
        let r = &owner.request;
        r.validate().map_err(|_| reject("invalid_request"))?;
        let namespace = GameIdentityNamespaceV1 {
            service_id: self.config.namespace.identity_service_id.clone(),
            environment_id: self.config.namespace.environment_id.clone(),
            instance_epoch: DecimalU64(self.config.rp.identity_instance_epoch),
        };
        if r.identity_namespace.service_id != namespace.service_id
            || r.identity_namespace.environment_id != namespace.environment_id
            || r.identity_namespace.instance_epoch.0 > namespace.instance_epoch.0
            || r.account_id != account_id
            || r.world_id != entry.trust.world.world_id
            || r.genesis_digest != entry.trust.world.genesis_digest
            || owner.subject_public_key != r.owner_public_key
        {
            return Err(reject("participant_context_mismatch"));
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_PARTICIPANT_REGISTER_DOMAIN,
            r,
            &owner.subject_public_key,
            &owner.signature_hex,
        )
        .map_err(|_| reject("authentication_failed"))?;
        let business_digest =
            oasis7_client_api::world_service_correlation::participant_business_digest(r)
                .map_err(|_| reject("invalid_request"))?;
        let _ = business_digest;
        Ok(())
    }
    pub(super) fn participant_authorize_with_policy(
        &mut self,
        sid: Option<&str>,
        owner: SignedReadRequest<GameParticipantRegisterV1>,
        view: Value,
    ) -> Result<Value, StoreError> {
        let storage_key = session_key(
            &self.config.session_key,
            sid.ok_or_else(|| reject("session_expired"))?,
        );
        self.participant_authorize_at_storage_key(&storage_key, owner, view)
    }
    pub(in super::super) fn participant_authorize_at_storage_key(
        &mut self,
        storage_key: &str,
        owner: SignedReadRequest<GameParticipantRegisterV1>,
        view: Value,
    ) -> Result<Value, StoreError> {
        let (account, _) = self.store.transaction(|tx| {
            valid_session(
                tx,
                storage_key,
                self.config.rp.identity_instance_epoch,
                super::super::now_ms(),
            )
        })?;
        self.validate_preflight_participant_owner(&account.account_id, &owner)?;
        let entry = self
            .participant_entry
            .as_ref()
            .ok_or_else(|| reject("participant_unavailable"))?;
        let r = &owner.request;
        let namespace = GameIdentityNamespaceV1 {
            service_id: self.config.namespace.identity_service_id.clone(),
            environment_id: self.config.namespace.environment_id.clone(),
            instance_epoch: DecimalU64(self.config.rp.identity_instance_epoch),
        };
        let business_digest =
            oasis7_client_api::world_service_correlation::participant_business_digest(r)
                .map_err(|_| reject("invalid_request"))?;
        let p = &view["policy"];
        let number = |v: &Value| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        };
        let tick =
            number(&view["logical_tick"]).ok_or_else(|| reject("participant_unavailable"))?;
        let window = number(&p["max_grant_ticks"])
            .filter(|v| *v > 0 && *v <= 300)
            .ok_or_else(|| reject("participant_unavailable"))?;
        // A closed policy still authorizes exact existing-binding recovery;
        // canonical validation decides whether a binding already exists.
        if !p["enabled"].is_boolean()
            || p["registry_complete"] != true
            || view["world_id"] != r.world_id
            || view["genesis_digest"] != r.genesis_digest
            || p["world_id"] != r.world_id
            || p["genesis_digest"] != r.genesis_digest
            || p["identity_namespace"]
                != serde_json::to_value(&namespace)
                    .map_err(|_| reject("participant_unavailable"))?
            || p["issuer_public_key"] != entry.issuer_public_key
        {
            return Err(reject("participant_unavailable"));
        }
        let admission = GameParticipantAdmissionV1 {
            api_rev: 1,
            business_digest: business_digest.clone(),
            identity_namespace: namespace,
            account_id: r.account_id.clone(),
            world_id: r.world_id.clone(),
            genesis_digest: r.genesis_digest.clone(),
            owner_public_key: r.owner_public_key.clone(),
            branch_id: view["branch_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| reject("participant_unavailable"))?
                .into(),
            reorg_epoch: DecimalU64(
                number(&view["reorg_epoch"]).ok_or_else(|| reject("participant_unavailable"))?,
            ),
            policy_generation: DecimalU64(
                number(&p["generation"])
                    .filter(|g| *g > 0)
                    .ok_or_else(|| reject("participant_unavailable"))?,
            ),
            valid_from_tick: DecimalU64(tick),
            valid_until_tick: DecimalU64(
                tick.checked_add(window)
                    .ok_or_else(|| reject("participant_unavailable"))?,
            ),
        };
        let binding_key = format!(
            "participant-operation:{}",
            request_digest(
                "participant-identity-operation",
                &(
                    &r.identity_namespace.service_id,
                    &r.identity_namespace.environment_id,
                    &r.account_id,
                    &r.world_id,
                    &r.genesis_digest,
                    &r.client_operation_id
                )
            )
            .map_err(|_| reject("invalid_request"))?
        );
        let token = storage_key;
        let checked_at = super::super::now_ms();
        self.store.transaction(|tx| {
            let current = valid_session(
                tx,
                token,
                self.config.rp.identity_instance_epoch,
                checked_at,
            )?;
            if current.0.account_id != r.account_id {
                return Err(reject("authentication_failed"));
            }
            if let Some(old) = tx.get::<OriginalBinding>(&binding_key)? {
                if old.value.business_digest != business_digest
                    || old.value.owner_public_key != r.owner_public_key
                {
                    return Err(reject("idempotency_conflict"));
                }
            } else {
                tx.create(
                    &binding_key,
                    "participant_original",
                    Some(&r.account_id),
                    None,
                    &OriginalBinding {
                        business_digest: business_digest.clone(),
                        owner_public_key: r.owner_public_key.clone(),
                    },
                )?;
            }
            consume_quota(
                tx,
                "participant-grant",
                &r.account_id,
                checked_at,
                60_000,
                30,
            )?;
            Ok(())
        })?;
        let signed = sign_read_request(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            admission,
            &entry.issuer_key,
        )
        .map_err(|_| reject("participant_unavailable"))?;
        Ok(
            json!({"api_version":2,"ok":true,"phase":"participant_authorized","state":"pending","new_registration_enabled":p["enabled"],"admission":signed,"next_action":"query_original_participant_request"}),
        )
    }
}
