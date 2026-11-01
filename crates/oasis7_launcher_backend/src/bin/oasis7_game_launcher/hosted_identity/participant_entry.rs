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

#[path = "participant_entry_owned_read.rs"]
mod owned_read;
pub(super) use owned_read::OwnedOwnerReadGateway;

#[derive(Clone, Serialize, Deserialize)]
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
#[path = "participant_read_diagnostics_tests.rs"]
mod read_diagnostics_tests;
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
                        .map_err(|_| reject("participant_read_response_schema"))?;
                response
                    .payload
                    .validate(request)
                    .map_err(|_| reject("participant_read_response_context"))?;
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
                        .map_err(|_| reject("participant_read_response_schema"))?;
                response
                    .payload
                    .validate(request)
                    .map_err(|_| reject("participant_read_response_context"))?;
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
        owned_read::forward_bounded(
            &self.trust,
            &self.http,
            path,
            response_path,
            request,
            response_request,
            maximum,
        )
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
        #[cfg(test)]
        if path == "/_fixture/resource-basis/owner-view" {
            return Some((|| {
                if method != "POST" {
                    return Err(reject("method_not_allowed"));
                }
                let q: oasis7_client_api::world_service::resource_basis::ParticipantOwnerResourceViewRequestV1 = parse_request(&serde_json::to_vec(payload).map_err(|_| reject("invalid_request"))?).map_err(|_| reject("invalid_request"))?;
                let bytes = self.owner_resource_component(sid, &q, now)?;
                let reply: oasis7_client_api::world_service::resource_basis::NativeResourceEnvelopeV1<oasis7_client_api::world_service::resource_basis::ParticipantOwnerResourceObservationV1> = parse_request(&bytes).map_err(|_| reject("invalid_request"))?;
                serde_json::to_value(reply).map_err(|_| reject("invalid_request"))
            })());
        }
        if path == "/participant/owner-resource-view" {
            // No approved V2 scope/prepaid-once verifier: no environment-only enablement.
            return Some(Err(reject("participant_unavailable")));
        }
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
            self.validate_preflight_participant_owner(&account.account_id, &owner)?;
            let view = entry.policy()?;
            self.participant_authorize_with_policy(sid, owner, view)
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

#[cfg(test)]
#[path = "participant_resource_component.rs"]
mod resource_component;

#[path = "participant_entry_authorize_policy.rs"]
mod authorize_policy;
