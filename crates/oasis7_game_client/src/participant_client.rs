//! Trusted local owner control. Executor clients never receive this object.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::{
    world_service::{self as ws, *},
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use oasis7_client_storage::identity_store::OwnerControlStore;
#[path = "entry_owner_client.rs"]
mod entry_owner_client;
pub use entry_owner_client::EntryOwnerProgress;
#[cfg(test)]
#[path = "participant_client_tests.rs"]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantClientConfig {
    pub identity: IdentityClientConfig,
    pub identity_instance_epoch: DecimalU64,
    pub world: WorldIdentity,
    pub trusted_world_service_key: String,
    pub owner_state_directory: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticipantProgress {
    pub stage: String,
    pub state: String,
    pub operation_id: String,
    pub account_id: String,
    pub agent_id: Option<String>,
    pub next_action: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ParticipantOwnerObservation {
    pub receipt: GameParticipantReceiptV1,
    pub owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
    pub version: ProjectionVersion,
    pub logical_tick: u64,
    pub task: Option<GameTaskControlV1>,
}
pub struct ParticipantClient {
    identity: IdentityClient,
    owner: OwnerControlStore,
    key: SigningKey,
    config: ParticipantClientConfig,
    scope: String,
}
impl ParticipantClient {
    pub(crate) fn validate(config: &ParticipantClientConfig) -> Result<(), String> {
        http::validate_config(&config.identity)?;
        config.world.validate().map_err(|e| e.to_string())?;
        let bytes: [u8; 32] = hex::decode(&config.trusted_world_service_key)
            .map_err(|_| "invalid service key")?
            .try_into()
            .map_err(|_| "invalid service key")?;
        let key =
            ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid service key")?;
        if key.is_weak()
            || config.identity_instance_epoch.0 == 0
            || config.owner_state_directory == config.identity.state_directory
        {
            return Err("invalid participant trust or key isolation".into());
        }
        Ok(())
    }
    /// Explicit local creation only; separate random owner material and directory.
    pub fn initialize_owner(
        config: &ParticipantClientConfig,
        passphrase: &[u8],
    ) -> Result<String, String> {
        Self::validate(config)?;
        let owner = OwnerControlStore::create(&config.owner_state_directory, passphrase)?;
        Ok(hex::encode(
            SigningKey::from_bytes(owner.owner_seed())
                .verifying_key()
                .to_bytes(),
        ))
    }
    pub fn new(config: ParticipantClientConfig, passphrase: &[u8]) -> Result<Self, String> {
        Self::validate(&config)?;
        let identity = IdentityClient::new(config.identity.clone(), passphrase)?;
        let owner = OwnerControlStore::open(&config.owner_state_directory, passphrase)?;
        let key = SigningKey::from_bytes(owner.owner_seed());
        if key.verifying_key() == identity.signing_key.verifying_key() {
            return Err("owner must differ from login key".into());
        }
        let scope = ws::request_digest(
            "participant-client-trust",
            &(
                &identity.scope,
                &config.world,
                &config.trusted_world_service_key,
            ),
        )?;
        Ok(Self {
            identity,
            owner,
            key,
            config,
            scope,
        })
    }
    fn authorized_call(
        &self,
        login_operation: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, String> {
        validate_operation(login_operation)?;
        let session = self
            .identity
            .store
            .read_original::<IdentitySession>(
                &self.identity.scope,
                login_operation,
                "login-session",
            )?
            .ok_or("action_required: explicit fresh identity login")?;
        let mut original = OriginalRequest {
            path: "/session".into(),
            cookie: session.cookie,
            csrf: String::new(),
            body: String::new(),
        };
        let (current, _) = self.identity.call("GET", "/session", Some(&original))?;
        if current["account"]["account_id"] != session.account_id {
            return Err("identity session subject changed".into());
        }
        original.path = "/capabilities".into();
        let (caps, _) = self
            .identity
            .call("GET", "/capabilities", Some(&original))?;
        original.csrf = caps["csrf_token"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("identity CSRF unavailable")?
            .into();
        original.path = path.into();
        original.body =
            serde_json::to_string(body).map_err(|_| "participant request encoding failed")?;
        self.identity
            .call("POST", path, Some(&original))
            .map(|(v, _)| v)
    }
    /// Creates one immutable business original before any possible submission.
    /// Every invocation queries its original key before an exact retry.
    pub fn register(
        &self,
        operation: &str,
        login_operation: &str,
    ) -> Result<ParticipantProgress, String> {
        validate_operation(operation)?;
        let session = self
            .identity
            .store
            .read_original::<IdentitySession>(
                &self.identity.scope,
                login_operation,
                "login-session",
            )?
            .ok_or("action_required: explicit fresh identity login")?;
        let request = GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            identity_namespace: GameIdentityNamespaceV1 {
                service_id: self.config.identity.identity_service_id.clone(),
                environment_id: self.config.identity.environment_id.clone(),
                instance_epoch: self.config.identity_instance_epoch,
            },
            account_id: session.account_id,
            world_id: self.config.world.world_id.clone(),
            genesis_digest: self.config.world.genesis_digest.clone(),
            owner_public_key: hex::encode(self.key.verifying_key().to_bytes()),
            owner_key_version: DecimalU64(1),
        };
        request.validate()?;
        let owner = if let Some(old) = self
            .owner
            .read_original::<SignedReadRequest<GameParticipantRegisterV1>>(
                &self.scope,
                operation,
                "participant-business-original",
            )? {
            let mut expected = request.clone();
            expected.identity_namespace.instance_epoch =
                old.request.identity_namespace.instance_epoch;
            if old.request != expected
                || old.subject_public_key != request.owner_public_key
                || old.request.identity_namespace.instance_epoch.0
                    > self.config.identity_instance_epoch.0
            {
                return Err("participant original content conflict".into());
            }
            ws::private_goal::verify_owner_signature_strict(
                GAME_PARTICIPANT_REGISTER_DOMAIN,
                &old.request,
                &old.subject_public_key,
                &old.signature_hex,
            )?;
            old
        } else {
            let private = zeroize::Zeroizing::new(hex::encode(self.key.to_bytes()));
            let signed =
                ws::sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, request, &private)?;
            let winner: SignedReadRequest<GameParticipantRegisterV1> = self.owner.put_original(
                &self.scope,
                operation,
                "participant-business-original",
                &signed,
            )?;
            if winner != signed {
                return Err("participant original content conflict".into());
            }
            winner
        };
        let reply = self.authorized_call(
            login_operation,
            "/participant/authorize",
            &serde_json::to_value(&owner).map_err(|_| "participant original encoding failed")?,
        )?;
        if reply["phase"] != "participant_authorized" {
            return Err("participant authorization unavailable".into());
        }
        let registration_enabled = reply["new_registration_enabled"]
            .as_bool()
            .ok_or("participant policy availability missing")?;
        let admission: SignedReadRequest<GameParticipantAdmissionV1> =
            serde_json::from_value(reply["admission"].clone())
                .map_err(|_| "participant admission malformed")?;
        let a = &admission.request;
        let r = &owner.request;
        if a.identity_namespace.service_id != r.identity_namespace.service_id
            || a.identity_namespace.environment_id != r.identity_namespace.environment_id
            || a.identity_namespace.instance_epoch != self.config.identity_instance_epoch
            || a.account_id != r.account_id
            || a.world_id != r.world_id
            || a.genesis_digest != r.genesis_digest
            || a.owner_public_key != r.owner_public_key
            || a.business_digest
                != oasis7_client_api::world_service_correlation::participant_business_digest(r)?
        {
            return Err("participant authorization binding mismatch".into());
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            a,
            &admission.subject_public_key,
            &admission.signature_hex,
        )?;
        let payload = WorldServicePayloadV1::GameParticipantRegister {
            owner: owner.clone(),
            admission,
        };
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            self.config.world.clone(),
            &payload,
        )?;
        let stage = format!(
            "participant-attempt-{}",
            ws::request_digest("admission-original", &payload)?.trim_start_matches("blake3:")
        );
        let original: WorldServicePayloadV1 =
            self.owner
                .put_original(&self.scope, operation, &stage, &payload)?;
        if original != payload {
            return Err("participant transport original conflict".into());
        }
        let lookup = AuthenticatedLookup {
            request: LookupIntentRequest {
                contract_version: 1,
                key: correlation.key.clone(),
            },
            original: payload.clone(),
        };
        let submit = SubmitIntentRequest {
            contract_version: 1,
            correlation: correlation.clone(),
            deadline_unix_ms: None,
            signed_payload: payload,
        };
        let originals: (
            AuthenticatedLookup,
            SubmitIntentRequest<WorldServicePayloadV1>,
        ) = self.owner.put_original(
            &self.scope,
            operation,
            &format!("wire-{}", stage),
            &(lookup, submit),
        )?;
        let observed = self.world_call(
            login_operation,
            "/participant/lookup",
            LOOKUP_PATH,
            &originals.0,
            &correlation,
        )?;
        if !matches!(observed.outcome, IntentOutcome::Unknown) {
            return self.progress(operation, r, &observed.outcome);
        }
        if !registration_enabled {
            return Ok(ParticipantProgress {
                stage: "participant_binding".into(),
                state: "blocked".into(),
                operation_id: operation.into(),
                account_id: r.account_id.clone(),
                agent_id: None,
                next_action: "await_authorized_registration".into(),
            });
        }
        let response = self.world_call(
            login_operation,
            "/participant/submit",
            SUBMIT_PATH,
            &originals.1,
            &correlation,
        )?;
        self.progress(operation, r, &response.outcome)
    }
    /// Historical local evidence only; never current authorization or ownership.
    pub fn status(&self, operation: &str) -> Result<ParticipantProgress, String> {
        validate_operation(operation)?;
        let original = self
            .owner
            .read_original::<SignedReadRequest<GameParticipantRegisterV1>>(
                &self.scope,
                operation,
                "participant-business-original",
            )?;
        let settled = self.owner.read_original::<GameParticipantReceiptV1>(
            &self.scope,
            operation,
            "participant-settlement",
        )?;
        Ok(ParticipantProgress {
            stage: "participant_binding".into(),
            state: if original.is_some() {
                "unknown"
            } else {
                "not_started"
            }
            .into(),
            operation_id: operation.into(),
            account_id: original.map(|r| r.request.account_id).unwrap_or_default(),
            agent_id: settled.map(|r| r.agent_id),
            next_action: "revalidate_session_and_query_original".into(),
        })
    }
    fn access(
        &self,
        operation: &str,
        login: &str,
    ) -> Result<
        (
            ws::participant_discovery::ParticipantOwnerViewRequestV1,
            GameParticipantReceiptV1,
        ),
        String,
    > {
        validate_operation(operation)?;
        let owner = self
            .owner
            .read_original::<SignedReadRequest<GameParticipantRegisterV1>>(
                &self.scope,
                operation,
                "participant-business-original",
            )?
            .ok_or("participant original missing; reconcile original identity")?;
        let receipt = self
            .owner
            .read_original::<GameParticipantReceiptV1>(
                &self.scope,
                operation,
                "participant-settlement",
            )?
            .ok_or("participant settlement unknown; query original")?;
        let reply = self.authorized_call(
            login,
            "/participant/authorize",
            &serde_json::to_value(&owner).map_err(|_| "participant original malformed")?,
        )?;
        let admission: SignedReadRequest<GameParticipantAdmissionV1> =
            serde_json::from_value(reply["admission"].clone())
                .map_err(|_| "participant admission missing")?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| "owner observation randomness unavailable")?;
        let read_request = ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some(hex::encode(nonce)),
            formal_anchor: None,
            world: self.config.world.clone(),
            scope_id: format!("agent:{}", receipt.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let private = zeroize::Zeroizing::new(hex::encode(self.key.to_bytes()));
        let read = ws::sign_read_request(VIEW_JSON_PATH, read_request, &private)?;
        let request = ws::participant_discovery::ParticipantOwnerViewRequestV1 {
            owner,
            admission,
            read,
            response_profile: None,
        };
        Ok((request, receipt))
    }
    pub(super) fn owner_control_call(
        &self,
        participant_operation: &str,
        login: &str,
        operation: ws::entry_control::EntryOwnerOperationV1,
    ) -> Result<Value, String> {
        let (mut access, _) = self.access(participant_operation, login)?;
        access.response_profile =
            Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerControlV1);
        let request = ws::entry_control::EntryOwnerControlRequestV1 { access, operation };
        let digest = ws::request_digest("entry-owner-transport-original", &request)?;
        let request: ws::entry_control::EntryOwnerControlRequestV1 = self.owner.put_original(
            &self.scope,
            participant_operation,
            &format!("owner-control-{}", digest.trim_start_matches("blake3:")),
            &request,
        )?;
        let reply = self.authorized_call(
            login,
            "/entry-owner/control",
            &serde_json::to_value(request).map_err(|_| "owner request encoding failed")?,
        )?;
        if reply["world_response"].is_null() {
            return Err("owner control outcome unknown; retain original".into());
        }
        Ok(reply["world_response"].clone())
    }
    pub fn current_owner(
        &self,
        operation: &str,
        login: &str,
    ) -> Result<ParticipantOwnerObservation, String> {
        let (mut request, receipt) = self.access(operation, login)?;
        request.response_profile =
            Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerControlV1);
        let reply = self.authorized_call(
            login,
            "/participant/owner-view",
            &serde_json::to_value(&request).map_err(|_| "owner view request encoding failed")?,
        )?;
        let response: SignedServiceResponse<
            ws::participant_discovery::ParticipantOwnerControlResponseV1,
        > = serde_json::from_value(reply["world_response"].clone())
            .map_err(|_| "owner view response malformed")?;
        ws::verify_service_response(
            ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
            &ws::request_digest(
                ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                &request,
            )?,
            &response,
            &self.config.trusted_world_service_key,
        )?;
        response.payload.validate(&request)?;
        let fence = response.payload.view.canonical_agent_owner;
        let binding = &response.payload.version.commit.binding;
        if fence.agent_id != receipt.agent_id
            || fence.public_key != receipt.owner_public_key
            || fence.world_id != receipt.world_id
            || fence.player_id != receipt.owner_binding
            || fence.canonical_authority.agent_identity_generation != receipt.owner_generation.0
            || fence.canonical_authority.branch_id != binding.branch_id
            || fence.reorg_epoch != binding.reorg_generation
        {
            return Err("canonical owner observation differs from binding".into());
        }
        let task = response.payload.view.game_task_control;
        if task.as_ref().is_some_and(|t| {
            t.agent_id != fence.agent_id
                || t.owner_public_key != fence.public_key
                || t.owner_binding != receipt.owner_binding
                || t.world_id != receipt.world_id
                || t.genesis_digest != receipt.genesis_digest
                || t.owner_generation.0 != receipt.owner_generation.0
                || t.branch_id != binding.branch_id
                || t.reorg_epoch.0 != binding.reorg_generation
        }) {
            return Err("task control differs from owner observation".into());
        }
        Ok(ParticipantOwnerObservation {
            receipt,
            owner: fence,
            version: response.payload.version,
            logical_tick: response.payload.logical_tick,
            task,
        })
    }
    pub fn starter_eligibility(&self, operation: &str, login: &str) -> Result<Value, String> {
        let (request, receipt) = self.access(operation, login)?;
        let reply = self.authorized_call(
            login,
            "/participant/starter-eligibility",
            &serde_json::to_value(&request).map_err(|_| "starter request encoding failed")?,
        )?;
        let response: SignedServiceResponse<Value> =
            serde_json::from_value(reply["world_response"].clone())
                .map_err(|_| "starter eligibility response malformed")?;
        let path = ws::participant_discovery::PARTICIPANT_STARTER_ELIGIBILITY_PATH;
        ws::verify_service_response(
            path,
            &ws::request_digest(path, &request)?,
            &response,
            &self.config.trusted_world_service_key,
        )?;
        let commit: CommitRef = serde_json::from_value(response.payload["commit"].clone())
            .map_err(|_| "starter eligibility commit malformed")?;
        commit.validate().map_err(|e| e.to_string())?;
        if commit.world != self.config.world
            || response.payload["world"]
                != serde_json::to_value(&self.config.world)
                    .map_err(|_| "starter world malformed")?
            || response.payload["agent_id"] != receipt.agent_id
            || !matches!(
                response.payload["eligibility"]["status"].as_str(),
                Some("claimed" | "eligible" | "blocked")
            )
        {
            return Err("starter eligibility resource mismatch".into());
        }
        Ok(response.payload)
    }
    fn world_call<Q: Serialize>(
        &self,
        login: &str,
        route: &str,
        path: &str,
        request: &Q,
        correlation: &RequestCorrelation,
    ) -> Result<IntentResponse<Value>, String> {
        let reply = self.authorized_call(
            login,
            route,
            &serde_json::to_value(request).map_err(|_| "participant encoding failed")?,
        )?;
        let signed: SignedServiceResponse<IntentResponse<Value>> =
            serde_json::from_value(reply["world_response"].clone())
                .map_err(|_| "world outcome unknown; retain original")?;
        ws::verify_service_response(
            path,
            &ws::request_digest(path, request)?,
            &signed,
            &self.config.trusted_world_service_key,
        )?;
        signed
            .payload
            .validate(correlation)
            .map_err(|e| e.to_string())?;
        Ok(signed.payload)
    }
    fn progress(
        &self,
        operation: &str,
        r: &GameParticipantRegisterV1,
        outcome: &IntentOutcome<Value>,
    ) -> Result<ParticipantProgress, String> {
        let mut result = ParticipantProgress {
            stage: "participant_binding".into(),
            state: "unknown".into(),
            operation_id: operation.into(),
            account_id: r.account_id.clone(),
            agent_id: None,
            next_action: "query_original".into(),
        };
        match outcome {
            IntentOutcome::Committed { commit, receipt } => {
                let receipt: GameParticipantReceiptV1 = serde_json::from_value(receipt.clone())
                    .map_err(|_| "participant receipt malformed")?;
                let mut original = r.clone();
                original.client_operation_id = receipt.client_operation_id.clone();
                if commit.world!=self.config.world || receipt.identity_namespace!=r.identity_namespace || receipt.account_id!=r.account_id
                    || receipt.world_id!=r.world_id || receipt.genesis_digest!=r.genesis_digest || receipt.owner_public_key!=r.owner_public_key
                    || receipt.business_digest!=oasis7_client_api::world_service_correlation::participant_business_digest(&original)?
                    || receipt.agent_id.is_empty() || receipt.owner_generation.0==0 {return Err("participant receipt binding mismatch".into());}
                // Signed service settlement is a prerequisite observation. It is
                // not an independently proved world/owner view or task authority.
                self.owner.put_original(
                    &self.scope,
                    operation,
                    "participant-settlement",
                    &receipt,
                )?;
                result.state = "pending".into();
                result.agent_id = Some(receipt.agent_id);
                result.next_action = "verify_canonical_owner_view".into();
            }
            IntentOutcome::Received { .. } => {
                result.state = "pending".into();
            }
            IntentOutcome::Rejected { .. } | IntentOutcome::Expired => {
                result.state = "blocked".into();
                result.next_action = "inspect_original_rejection".into();
            }
            _ => {}
        }
        Ok(result)
    }
}
