//! Human managed Cookie + Origin + CSRF, not the Account machine-cookie mode.
//! Each unique paid World operation is sent outside the global service mutex.
use super::super::{now_ms, participant_entry::OwnedOwnerReadGateway, service::cookie_value};
use super::*;
use cleanup_config::MachineConfig;
use oasis7_client_api::world_service::{
    SignedReadRequest, SignedServiceResponse,
    participant_discovery::{ParticipantPolicyOriginalV1, ParticipantPolicyRequestV1},
};
use oasis7_custody_core::native_cleanup_v2::*;
use preflight_world_registry::{ObservedWorldStage, WorldStageScope};
pub(in super::super::super) struct OwnedPreflightWorldWorkV2 {
    stage: ObservedWorldStage,
    storage_key: String,
    configuration_digest: String,
    request: SignedReadRequest<ParticipantPolicyRequestV1>,
    gateway: OwnedOwnerReadGateway,
}
pub(in super::super::super) struct CompletedPreflightWorldWorkV2 {
    stage: ObservedWorldStage,
    storage_key: String,
    configuration_digest: String,
    policy: ParticipantPolicyOriginalV1,
}
impl OwnedPreflightWorldWorkV2 {
    pub(in super::super::super) fn send(self) -> Result<CompletedPreflightWorldWorkV2, StoreError> {
        let response: SignedServiceResponse<serde_json::Value> =
            self.gateway.read_participant_policy(&self.request)?;
        Ok(CompletedPreflightWorldWorkV2 {
            stage: self.stage,
            storage_key: self.storage_key,
            configuration_digest: self.configuration_digest,
            policy: ParticipantPolicyOriginalV1 {
                request: self.request,
                response,
            },
        })
    }
}
fn unavailable() -> StoreError {
    reject("preflight_world_unavailable")
}
fn exact_request<T: serde::de::DeserializeOwned + serde::Serialize>(
    body: &[u8],
) -> Result<T, StoreError> {
    let raw: serde_json::Value = serde_json::from_slice(body).map_err(|_| unavailable())?;
    let value: T = serde_json::from_value(raw.clone()).map_err(|_| unavailable())?;
    if serde_json::to_value(&value).map_err(|_| unavailable())? != raw {
        return Err(unavailable());
    }
    Ok(value)
}
impl IdentityService {
    pub(in super::super::super) fn prepare_preflight_world_v2(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
    ) -> Result<OwnedPreflightWorldWorkV2, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" || body.len() > 65536 {
            return Err(unavailable());
        }
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let sid = cookie_value(headers, "__Host-oasis7_sid").ok_or_else(unavailable)?;
        self.validate_json_write(headers, &digest(&self.config.session_key, "csrf", sid))?;
        let (account, _) = self.session(Some(sid), now)?;
        let (proof, operation, policy_nonce) = match path {
            NATIVE_PREFLIGHT_POLICY_V2_PATH => {
                let request: NativePreflightPolicyRequestV2 = exact_request(body)?;
                if AccountPreflightAccessV2::policy_body_digest(&request.body)
                    .map_err(|_| unavailable())?
                    != request.proof.certificate.ports[0].body_digest
                {
                    return Err(unavailable());
                }
                if request.body.observation_nonce
                    != request.proof.certificate.descriptor.access_nonces.policy
                {
                    return Err(unavailable());
                }
                (
                    *request.proof,
                    NativePreflightWorldOperationV2::PolicyOriginal,
                    request.body.observation_nonce,
                )
            }
            NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH => {
                let request: NativePreflightAuthorizeRequestV2 = exact_request(body)?;
                if AccountPreflightAccessV2::authorize_body_digest(&request.body)
                    .map_err(|_| unavailable())?
                    != request.proof.certificate.ports[1].body_digest
                {
                    return Err(unavailable());
                }
                if request.body != request.proof.certificate.descriptor.participant_owner {
                    return Err(unavailable());
                }
                self.validate_preflight_participant_owner(&account.account_id, &request.body)?;
                let nonce = request.proof.certificate.ports[1]
                    .physical_attempt_id
                    .clone();
                (
                    *request.proof,
                    NativePreflightWorldOperationV2::AuthorizePolicy,
                    nonce,
                )
            }
            _ => return Err(unavailable()),
        };
        proof
            .certificate
            .validate_shape()
            .map_err(|_| unavailable())?;
        if proof
            .certificate
            .descriptor
            .native_binding
            .native_account_id
            != account.account_id
        {
            return Err(unavailable());
        }
        self.validate_preflight_participant_owner(
            &account.account_id,
            &proof.certificate.descriptor.participant_owner,
        )?;
        let entry = self.participant_entry.as_ref().ok_or_else(unavailable)?;
        let gateway = entry.owned_owner_read_gateway();
        gateway.require_approved_world(&proof.certificate.descriptor.world)?;
        let request = entry.prepare_participant_policy_read(policy_nonce)?;
        let machine = MachineConfig::from_env()?;
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        let key = session_key(&self.config.session_key, sid);
        let scope = WorldStageScope {
            config,
            identity: &self.config,
            storage_key: &key,
            configuration_digest: &fingerprint,
            pin: machine.preflight_access_pin()?,
            floor: &machine.issuer_floor,
            now,
        };
        // Its own committed transaction: a later current/network failure cannot refund it.
        let claimed = self
            .store
            .transaction(|tx| preflight_world_registry::claim(tx, &scope, proof, operation))?;
        let stage = self.store.transaction(|tx| {
            consume_quota(
                tx,
                "participant-request",
                &account.account_id,
                now,
                60000,
                120,
            )?;
            preflight_world_registry::observe(tx, &scope, claimed)
        })?;
        Ok(OwnedPreflightWorldWorkV2 {
            stage,
            storage_key: key,
            configuration_digest: fingerprint,
            request,
            gateway,
        })
    }
    pub(in super::super::super) fn finish_preflight_world_v2(
        &mut self,
        completed: CompletedPreflightWorldWorkV2,
    ) -> Result<serde_json::Value, StoreError> {
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let machine = MachineConfig::from_env()?;
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let gateway = self
            .participant_entry
            .as_ref()
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        if fingerprint != completed.configuration_digest {
            return Err(unavailable());
        }
        let original = completed.stage.original().clone();
        let operation = completed.stage.operation();
        // This exact Node-signed policy was produced by the owned fixed gateway.
        // The authorizer completion uses its existing pure policy/admission kernel,
        // and never makes another policy request.
        let admission = if operation == NativePreflightWorldOperationV2::AuthorizePolicy {
            let reply = self.participant_authorize_at_storage_key(
                &completed.storage_key,
                original.certificate.descriptor.participant_owner.clone(),
                completed.policy.response.payload.clone(),
            )?;
            Some((
                serde_json::from_value::<
                    oasis7_client_api::world_service::SignedReadRequest<
                        oasis7_agent_api::game::GameParticipantAdmissionV1,
                    >,
                >(reply["admission"].clone())
                .map_err(|_| unavailable())?,
                reply["new_registration_enabled"].clone(),
            ))
        } else {
            None
        };
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let result_digest = match &admission {
            Some((admission, _)) => {
                NativePreflightWorldReceiptV2::admission_result_digest(admission)
            }
            None => NativePreflightWorldReceiptV2::policy_result_digest(&completed.policy),
        }
        .map_err(|_| unavailable())?;
        let scope = WorldStageScope {
            config,
            identity: &self.config,
            storage_key: &completed.storage_key,
            configuration_digest: &fingerprint,
            pin: machine.preflight_access_pin()?,
            floor: &machine.issuer_floor,
            now,
        };
        let receipt = self.store.transaction(|tx| {
            preflight_world_registry::finish(tx, &scope, completed.stage, &result_digest)
        })?;
        match admission {
            Some((admission, enabled)) => serde_json::to_value(NativePreflightAuthorizeReplyV2 {
                receipt,
                admission,
                new_registration_enabled: enabled.as_bool().ok_or_else(unavailable)?,
            })
            .map_err(|_| unavailable()),
            None => serde_json::to_value(NativePreflightPolicyReplyV2 {
                receipt,
                policy: completed.policy,
            })
            .map_err(|_| unavailable()),
        }
    }
}
