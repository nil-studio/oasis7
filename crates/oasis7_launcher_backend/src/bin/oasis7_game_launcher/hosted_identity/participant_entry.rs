//! Separate identity admission issuer. Never signs an owner or executor intent.
use super::{
    reject,
    service::{IdentityService, consume_quota, session_key, valid_session},
    store::StoreError,
};
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{
    SignedReadRequest, SignedServiceResponse, WorldIdentity, participant_discovery::*,
    request_digest, sign_read_request, verify_service_response,
};
use oasis7_client_api::{
    world_service::{LOOKUP_PATH, SUBMIT_PATH, SubmitIntentRequest, VIEW_JSON_PATH},
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntryTrust {
    pub endpoint: String,
    pub service_public_key: String,
    pub world: WorldIdentity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyOriginalRequest {
    observation_nonce: String,
}

#[cfg(test)]
#[path = "participant_entry_tests.rs"]
mod tests;
pub(super) struct ParticipantEntry {
    pub(super) trust: EntryTrust,
    pub(super) owner_control_enabled: bool,
    pub(super) executor_control_enabled: bool,
    issuer_key: Zeroizing<String>,
    issuer_public_key: String,
    http: reqwest::blocking::Client,
}
impl ParticipantEntry {
    pub(super) fn from_env() -> Result<Option<Self>, String> {
        match std::env::var("OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("0") => return Ok(None),
            Ok("1") => {}
            _ => return Err("participant entry flag must be 0 or 1".into()),
        }
        let config = std::env::var("OASIS7_HOSTED_PARTICIPANT_ENTRY_TRUST")
            .map_err(|_| "participant entry trust required")?;
        let trust: EntryTrust = oasis7_agent_api::game::parse_request(config.as_bytes())
            .map_err(|_| "participant entry trust malformed")?;
        let key = Zeroizing::new(
            std::env::var("OASIS7_HOSTED_PARTICIPANT_ISSUER_PRIVATE_KEY")
                .map_err(|_| "participant issuer key required")?,
        );
        let mut entry = Self::new(trust, key)?;
        entry.owner_control_enabled =
            match std::env::var("OASIS7_HOSTED_OWNER_CONTROL_ENABLED").as_deref() {
                Err(std::env::VarError::NotPresent) | Ok("0") => false,
                Ok("1") => true,
                _ => return Err("owner control flag must be 0 or 1".into()),
            };
        entry.executor_control_enabled =
            match std::env::var("OASIS7_HOSTED_EXECUTOR_CONTROL_ENABLED").as_deref() {
                Err(std::env::VarError::NotPresent) | Ok("0") => false,
                Ok("1") => true,
                _ => return Err("executor control flag must be 0 or 1".into()),
            };
        Ok(Some(entry))
    }
    pub(super) fn new(trust: EntryTrust, issuer_key: Zeroizing<String>) -> Result<Self, String> {
        let url = url::Url::parse(&trust.endpoint).map_err(|_| "invalid participant service")?;
        if url.scheme() != "http"
            || url.origin().ascii_serialization() != trust.endpoint
            || !url.host_str().is_some_and(|h| {
                h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            })
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("participant upstream must be an exact loopback origin".into());
        }
        trust.world.validate().map_err(|e| e.to_string())?;
        super::native_key::public_key(&json!({"public_key":trust.service_public_key}))
            .map_err(|_| "invalid participant service key")?;
        let signed = sign_read_request("participant-issuer-selfcheck", (), &issuer_key)?;
        super::native_key::public_key(&json!({"public_key":signed.subject_public_key}))
            .map_err(|_| "invalid participant issuer key")?;
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(1))
            .build()
            .map_err(|_| "participant transport unavailable")?;
        Ok(Self {
            trust,
            owner_control_enabled: false,
            executor_control_enabled: false,
            issuer_key,
            issuer_public_key: signed.subject_public_key,
            http,
        })
    }
    fn policy(&self) -> Result<Value, StoreError> {
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| reject("participant_unavailable"))?;
        Ok(self.policy_original(hex::encode(nonce))?.1.payload)
    }
    fn policy_original(
        &self,
        observation_nonce: String,
    ) -> Result<
        (
            SignedReadRequest<ParticipantPolicyRequestV1>,
            SignedServiceResponse<Value>,
        ),
        StoreError,
    > {
        if observation_nonce.len() != 64
            || !observation_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(reject("invalid_request"));
        }
        let request = sign_read_request(
            PARTICIPANT_POLICY_PATH,
            ParticipantPolicyRequestV1 {
                contract_version: 1,
                expected_world: self.trust.world.clone(),
                trust_config_ref: self.trust.service_public_key.clone(),
                observation_nonce,
            },
            &self.issuer_key,
        )
        .map_err(|_| reject("participant_unavailable"))?;
        let response = self
            .http
            .post(format!("{}{PARTICIPANT_POLICY_PATH}", self.trust.endpoint))
            .json(&request)
            .send()
            .map_err(|_| reject("participant_unavailable"))?;
        if response.status().as_u16() != 200 {
            return Err(reject("participant_unavailable"));
        }
        let mut bytes = Vec::new();
        response
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| reject("participant_unavailable"))?;
        if bytes.len() > 65536 {
            return Err(reject("participant_unavailable"));
        }
        let original: Value =
            parse_request(&bytes).map_err(|_| reject("participant_unavailable"))?;
        let response: SignedServiceResponse<Value> = serde_json::from_value(original.clone())
            .map_err(|_| reject("participant_unavailable"))?;
        if serde_json::to_value(&response).map_err(|_| reject("participant_unavailable"))?
            != original
        {
            return Err(reject("participant_unavailable"));
        }
        verify_service_response(
            PARTICIPANT_POLICY_PATH,
            &request_digest(PARTICIPANT_POLICY_PATH, &request)
                .map_err(|_| reject("participant_unavailable"))?,
            &response,
            &self.trust.service_public_key,
        )
        .map_err(|_| reject("participant_unavailable"))?;
        Ok((request, response))
    }
    pub(super) fn forward_owner_view(
        &self,
        request: &ParticipantOwnerViewRequestV1,
    ) -> Result<Value, StoreError> {
        match request.response_profile {
            None => self.forward_bound(
                PARTICIPANT_OWNER_VIEW_PATH,
                VIEW_JSON_PATH,
                request,
                &request.read,
            ),
            Some(ParticipantOwnerViewProfileV1::OwnerControlV1) => {
                let reply = self.forward_bound(
                    PARTICIPANT_OWNER_VIEW_PATH,
                    PARTICIPANT_OWNER_VIEW_PATH,
                    request,
                    request,
                )?;
                let response: SignedServiceResponse<ParticipantOwnerControlResponseV1> =
                    serde_json::from_value(reply["world_response"].clone())
                        .map_err(|_| StoreError::OutcomeUnknown)?;
                response
                    .payload
                    .validate(request)
                    .map_err(|_| StoreError::OutcomeUnknown)?;
                Ok(reply)
            }
            Some(ParticipantOwnerViewProfileV1::OwnerSetupV1) => {
                let reply = self.forward_bound(
                    PARTICIPANT_OWNER_VIEW_PATH,
                    PARTICIPANT_OWNER_VIEW_PATH,
                    request,
                    request,
                )?;
                let response: SignedServiceResponse<OwnerSetupObservationV1> =
                    serde_json::from_value(reply["world_response"].clone())
                        .map_err(|_| StoreError::OutcomeUnknown)?;
                response
                    .payload
                    .validate(request)
                    .map_err(|_| StoreError::OutcomeUnknown)?;
                Ok(reply)
            }
        }
    }
    fn forward<Q: Serialize>(&self, path: &str, request: &Q) -> Result<Value, StoreError> {
        if ![SUBMIT_PATH, LOOKUP_PATH].contains(&path) {
            return Err(reject("invalid_request"));
        }
        self.forward_bound(path, path, request, request)
    }
    pub(super) fn forward_bound<Q: Serialize, R: Serialize>(
        &self,
        path: &str,
        response_path: &str,
        request: &Q,
        response_request: &R,
    ) -> Result<Value, StoreError> {
        self.forward_bounded(path, response_path, request, response_request, 262144)
    }
    pub(super) fn forward_bounded<Q: Serialize, R: Serialize>(
        &self,
        path: &str,
        response_path: &str,
        request: &Q,
        response_request: &R,
        maximum: usize,
    ) -> Result<Value, StoreError> {
        let bytes = serde_json::to_vec(request).map_err(|_| reject("invalid_request"))?;
        if bytes.len() > 65536 {
            return Err(reject("invalid_request"));
        }
        let response = self
            .http
            .post(format!("{}{path}", self.trust.endpoint))
            .header("Content-Type", "application/json")
            .body(bytes)
            .send()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if response.status().as_u16() != 200 {
            let status = response.status().as_u16();
            let mut error = Vec::new();
            response
                .take(4097)
                .read_to_end(&mut error)
                .map_err(|_| StoreError::OutcomeUnknown)?;
            if error.len() <= 4096
                && status == 409
                && let Ok(value) = parse_request::<Value>(&error)
            {
                let code = value["error"].as_str().unwrap_or("");
                let quote=path==oasis7_client_api::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH || path==oasis7_client_api::world_service::task_factory_quote::TASK_FACTORY_QUOTE_PATH;
                let allowed = (quote
                    && ["task_quote_base_drift", "task_quote_delivery_drift"].contains(&code))
                    || (path
                        == oasis7_client_api::world_service::task_context::TASK_CONTEXT_READ_PATH
                        && code == "task_context_read_delivery_drift");
                if allowed {
                    return Ok(
                        json!({"api_version":2,"ok":true,"upstream_read_error":{"status":409,"code":code}}),
                    );
                }
            }
            return Err(StoreError::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        response
            .take(maximum as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if bytes.len() > maximum {
            return Err(StoreError::OutcomeUnknown);
        }
        let digest = request_digest(response_path, response_request)
            .map_err(|_| reject("invalid_request"))?;
        macro_rules! verified {
            ($kind:ty) => {{
                let original: Value =
                    parse_bounded_json(&bytes, maximum).map_err(|_| StoreError::OutcomeUnknown)?;
                let response: SignedServiceResponse<$kind> =
                    serde_json::from_value(original.clone())
                        .map_err(|_| StoreError::OutcomeUnknown)?;
                let wire =
                    serde_json::to_value(&response).map_err(|_| StoreError::OutcomeUnknown)?;
                if wire != original {
                    return Err(StoreError::OutcomeUnknown);
                }
                verify_service_response(
                    response_path,
                    &digest,
                    &response,
                    &self.trust.service_public_key,
                )
                .map_err(|_| StoreError::OutcomeUnknown)?;
                wire
            }};
        }
        use oasis7_client_api::world_service::{
            private_goal::*, private_goal_read::*, task_context::*, task_factory_quote::*,
            task_recipe_quote::*,
        };
        // Private signatures cover typed CBOR, including typed map keys.
        // Never reinterpret that proof as the normalized JSON view profile.
        let response = match response_path {
            PARTICIPANT_OWNER_VIEW_PATH => verified!(ParticipantOwnerControlResponseV1),
            GOAL_PREPARE_PATH => verified!(PreparedGoalReceiptV1),
            GOAL_CURRENT_PATH => verified!(CurrentGoalResponse),
            TASK_CONTEXT_PREPARE_PATH => verified!(PreparedTaskContextV1),
            TASK_CONTEXT_READ_PATH => verified!(CapturedTaskContextV1),
            TASK_SNAPSHOT_PATH => verified!(GoalExecutionSnapshotV1),
            TASK_RECIPE_QUOTE_PATH => verified!(TaskRecipeQuoteV1),
            TASK_FACTORY_QUOTE_PATH => verified!(TaskFactoryQuoteV1),
            _ => verified!(Value),
        };
        Ok(json!({"api_version":2,"ok":true,"world_response":response}))
    }
}

#[derive(Serialize, Deserialize)]
struct OriginalBinding {
    business_digest: String,
    owner_public_key: String,
}

impl IdentityService {
    pub(super) fn entry_discovery(&mut self, source: &str, now: u64) -> Result<Value, StoreError> {
        let subject = super::digest(&self.config.session_key, "entry-discovery-source", source);
        self.store
            .transaction(|tx| consume_quota(tx, "entry-discovery", &subject, now, 60_000, 60))?;
        let entry = self.participant_entry.as_ref();
        let policy = entry.map(ParticipantEntry::policy).transpose()?;
        let view = policy.as_ref().unwrap_or(&Value::Null);
        let ready = entry.is_some_and(|e| {
            let p = &view["policy"];
            p["registry_complete"] == true
                && p["issuer_public_key"] == e.issuer_public_key
                && p["world_id"] == e.trust.world.world_id
                && p["genesis_digest"] == e.trust.world.genesis_digest
                && p["identity_namespace"]["service_id"]
                    == self.config.namespace.identity_service_id
                && p["identity_namespace"]["environment_id"] == self.config.namespace.environment_id
                && serde_json::from_value::<DecimalU64>(
                    p["identity_namespace"]["instance_epoch"].clone(),
                )
                .is_ok_and(|epoch| epoch.0 == self.config.rp.identity_instance_epoch)
        });
        Ok(json!({"api_version":2,"ok":true,"schema_revision":1,
            "identity":{"service_id":self.config.namespace.identity_service_id,"environment_id":self.config.namespace.environment_id,"instance_epoch":self.config.rp.identity_instance_epoch.to_string()},
            "world":entry.map(|e|&e.trust.world),"world_service_public_key":entry.map(|e|&e.trust.service_public_key),
            "phase_capabilities":{"machine_registration":self.config.registration_enabled&&self.config.machine_registration_enabled,
                "participant_registration":ready&&view["available"]==true,"participant_recovery":ready,
                "owner_control":ready&&entry.is_some_and(|e|e.owner_control_enabled),
                "executor_control":ready&&entry.is_some_and(|e|e.executor_control_enabled),"complete_cold_entry":false},
            "participant_policy":view,"runtime_profiles":[],"registration_fee":{"state":"no_additional_identity_registration_fee","world_costs":"check_each_authorized_operation"},
            "next_action":"use_only_enabled_phases_and_explicit_local_owner_confirmation"}))
    }
    pub(super) fn participant_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        payload: &Value,
        now: u64,
    ) -> Option<Result<Value, StoreError>> {
        if ![
            "/participant/policy-original",
            "/participant/authorize",
            "/participant/submit",
            "/participant/lookup",
            "/participant/owner-view",
            "/participant/starter-eligibility",
        ]
        .contains(&path)
        {
            return None;
        }
        let result = (|| {
            if method != "POST" {
                return Err(reject("method_not_allowed"));
            }
            let (account, _) = self.session(sid, now)?;
            self.store.transaction(|tx| {
                consume_quota(
                    tx,
                    "participant-request",
                    &account.account_id,
                    now,
                    60_000,
                    120,
                )
            })?;
            let entry = self
                .participant_entry
                .as_ref()
                .ok_or_else(|| reject("participant_unavailable"))?;
            if path == "/participant/policy-original" {
                let request: PolicyOriginalRequest = parse_request(
                    &serde_json::to_vec(payload).map_err(|_| reject("invalid_request"))?,
                )
                .map_err(|_| reject("invalid_request"))?;
                let (request, response) = entry.policy_original(request.observation_nonce)?;
                self.store.transaction(|tx| {
                    valid_session(
                        tx,
                        &session_key(
                            &self.config.session_key,
                            sid.ok_or_else(|| reject("session_expired"))?,
                        ),
                        self.config.rp.identity_instance_epoch,
                        super::now_ms(),
                    )
                    .map(|_| ())
                })?;
                return serde_json::to_value(ParticipantPolicyOriginalV1 { request, response })
                    .map_err(|_| reject("participant_unavailable"));
            }
            if path == "/participant/owner-view" || path == "/participant/starter-eligibility" {
                let request: ParticipantOwnerViewRequestV1 = parse_request(
                    &serde_json::to_vec(payload).map_err(|_| reject("invalid_request"))?,
                )
                .map_err(|_| reject("invalid_request"))?;
                self.check_participant_subject(&request.owner.request, &account.account_id, entry)?;
                self.store.transaction(|tx| {
                    valid_session(
                        tx,
                        &session_key(
                            &self.config.session_key,
                            sid.ok_or_else(|| reject("session_expired"))?,
                        ),
                        self.config.rp.identity_instance_epoch,
                        super::now_ms(),
                    )
                    .map(|_| ())
                })?;
                if path == "/participant/starter-eligibility" {
                    return entry.forward_bound(
                        PARTICIPANT_STARTER_ELIGIBILITY_PATH,
                        PARTICIPANT_STARTER_ELIGIBILITY_PATH,
                        &request,
                        &request,
                    );
                }
                return entry.forward_owner_view(&request);
            }
            if path != "/participant/authorize" {
                let bytes = serde_json::to_vec(payload).map_err(|_| reject("invalid_request"))?;
                if path == "/participant/submit" {
                    let request: SubmitIntentRequest<WorldServicePayloadV1> =
                        parse_request(&bytes).map_err(|_| reject("invalid_request"))?;
                    let WorldServicePayloadV1::GameParticipantRegister { owner, .. } =
                        &request.signed_payload
                    else {
                        return Err(reject("participant_context_mismatch"));
                    };
                    self.check_participant_subject(&owner.request, &account.account_id, entry)?;
                    request.validate().map_err(|_| reject("invalid_request"))?;
                    self.store.transaction(|tx| {
                        valid_session(
                            tx,
                            &session_key(
                                &self.config.session_key,
                                sid.ok_or_else(|| reject("session_expired"))?,
                            ),
                            self.config.rp.identity_instance_epoch,
                            super::now_ms(),
                        )
                        .map(|_| ())
                    })?;
                    return entry.forward(SUBMIT_PATH, &request);
                }
                let request: AuthenticatedLookup =
                    parse_request(&bytes).map_err(|_| reject("invalid_request"))?;
                let WorldServicePayloadV1::GameParticipantRegister { owner, .. } =
                    &request.original
                else {
                    return Err(reject("participant_context_mismatch"));
                };
                self.check_participant_subject(&owner.request, &account.account_id, entry)?;
                request
                    .request
                    .validate()
                    .map_err(|_| reject("invalid_request"))?;
                self.store.transaction(|tx| {
                    valid_session(
                        tx,
                        &session_key(
                            &self.config.session_key,
                            sid.ok_or_else(|| reject("session_expired"))?,
                        ),
                        self.config.rp.identity_instance_epoch,
                        super::now_ms(),
                    )
                    .map(|_| ())
                })?;
                return entry.forward(LOOKUP_PATH, &request);
            }
            let owner: SignedReadRequest<GameParticipantRegisterV1> =
                parse_request(&serde_json::to_vec(payload).map_err(|_| reject("invalid_request"))?)
                    .map_err(|_| reject("invalid_request"))?;
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
                || r.account_id != account.account_id
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
            let view = entry.policy()?;
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
                    number(&view["reorg_epoch"])
                        .ok_or_else(|| reject("participant_unavailable"))?,
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
            let token = session_key(
                &self.config.session_key,
                sid.ok_or_else(|| reject("session_expired"))?,
            );
            let checked_at = super::now_ms();
            self.store.transaction(|tx| {
                let current = valid_session(
                    tx,
                    &token,
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
        })();
        Some(result.and_then(|reply| {
            self.session(sid, super::now_ms())?;
            Ok(reply)
        }))
    }
    pub(super) fn check_participant_subject(
        &self,
        r: &GameParticipantRegisterV1,
        account: &str,
        entry: &ParticipantEntry,
    ) -> Result<(), StoreError> {
        r.validate().map_err(|_| reject("invalid_request"))?;
        if r.account_id != account
            || r.identity_namespace.service_id != self.config.namespace.identity_service_id
            || r.identity_namespace.environment_id != self.config.namespace.environment_id
            || r.identity_namespace.instance_epoch.0 > self.config.rp.identity_instance_epoch
            || r.world_id != entry.trust.world.world_id
            || r.genesis_digest != entry.trust.world.genesis_digest
        {
            return Err(reject("participant_context_mismatch"));
        }
        Ok(())
    }
}
