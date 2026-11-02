//! One authenticated Native call; two separately claimed owned World reads.
use super::super::{now_ms, participant_entry::OwnedOwnerReadGateway, service::cookie_value};
use super::resource_final_config::ResourceFinalConfigV2;
use super::resource_final_registry::{
    self as registry, ObservedResourceInvocationV2, ResourceFinalScopeV2,
};
use super::*;
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_client_api::world_service::{
    self as ws, SignedServiceResponse, participant_discovery::OwnerSetupObservationV1,
};
use oasis7_custody_core::provisioning::{
    OwnerSetupResourceFinalHttpV2, RESOURCE_READ_PROOF_HEADER,
};
pub(in super::super::super) struct OwnedResourcePreflightV2 {
    invocation: ObservedResourceInvocationV2,
    gateway: OwnedOwnerReadGateway,
}
pub(in super::super::super) struct CompletedResourcePreflightV2 {
    invocation: ObservedResourceInvocationV2,
    response: SignedServiceResponse<OwnerSetupObservationV1>,
    paid_reply: resource::SignedResourcePaidWorldReplyV1,
}
pub(in super::super::super) struct OwnedResourceFinalV2 {
    preflight_paid_reply: resource::SignedResourcePaidWorldReplyV1,
    invocation: ObservedResourceInvocationV2,
    gateway: OwnedOwnerReadGateway,
}
pub(in super::super::super) struct CompletedResourceFinalV2 {
    preflight_paid_reply: resource::SignedResourcePaidWorldReplyV1,
    final_paid_reply: resource::SignedResourcePaidWorldReplyV1,
    invocation: ObservedResourceInvocationV2,
}
fn unavailable() -> StoreError {
    reject("resource_final_unavailable")
}
fn one_header<'a>(headers: &'a str, name: &str) -> Result<&'a str, StoreError> {
    let mut found = None;
    for line in headers.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                if found.is_some() {
                    return Err(unavailable());
                }
                found = Some(value.trim());
            }
        }
    }
    found.filter(|v| !v.is_empty()).ok_or_else(unavailable)
}
impl OwnedResourcePreflightV2 {
    pub(in super::super::super) fn send(self) -> Result<CompletedResourcePreflightV2, StoreError> {
        if now_ms() >= self.invocation.valid_until() {
            return Err(StoreError::OutcomeUnknown);
        }
        let (response, paid_reply) = self
            .gateway
            .read_resource_owner_preflight(self.invocation.frame())?;
        Ok(CompletedResourcePreflightV2 {
            invocation: self.invocation,
            response,
            paid_reply,
        })
    }
}
impl OwnedResourceFinalV2 {
    pub(in super::super::super) fn send(self) -> Result<CompletedResourceFinalV2, StoreError> {
        if now_ms() >= self.invocation.valid_until() {
            return Err(StoreError::OutcomeUnknown);
        }
        let (_, final_paid_reply) = self.gateway.read_resource_final(self.invocation.frame())?;
        Ok(CompletedResourceFinalV2 {
            preflight_paid_reply: self.preflight_paid_reply,
            final_paid_reply,
            invocation: self.invocation,
        })
    }
}
fn plan_world(
    frame: &OwnerSetupResourceFinalHttpV2,
) -> Result<&oasis7_custody_core::approval::ApprovedWorldV1, StoreError> {
    let CustodyIntentV1::OwnerSetupV2 { plan } = &frame.input().request.intent else {
        return Err(unavailable());
    };
    Ok(&plan.world_identity)
}
fn preflight(
    frame: &OwnerSetupResourceFinalHttpV2,
    response: &SignedServiceResponse<OwnerSetupObservationV1>,
) -> Result<(), StoreError> {
    let access = &frame.request().access;
    response
        .payload
        .validate(access)
        .map_err(|_| unavailable())?;
    let ws::entry_control::EntryOwnerOperationV1::ReadResourceGoal(q) = &frame.request().operation
    else {
        return Err(unavailable());
    };
    let task = response
        .payload
        .control
        .view
        .game_task_control
        .as_ref()
        .ok_or_else(unavailable)?;
    let signer = response
        .payload
        .signer_delegation
        .as_ref()
        .ok_or_else(unavailable)?;
    let actual = resource::ResourceSignerRecordV1 {
        world_id: signer.world_id.clone(),
        branch_id: signer.branch_id.clone(),
        agent_id: signer.agent_id.clone(),
        owner_binding: signer.owner_binding.clone(),
        agent_identity_generation: DecimalU64(signer.agent_identity_generation),
        generation: DecimalU64(signer.generation),
        delegate_public_key: signer.delegate_public_key.clone(),
        revoked: signer.revoked,
        nonce: DecimalU64(signer.nonce),
    };
    if GameTaskFenceV1::from_control(task) != q.request.expected_fence
        || actual != q.request.expected_signer_delegation
    {
        return Err(unavailable());
    }
    Ok(())
}
impl IdentityService {
    pub(in super::super::super) fn prepare_owner_resource_final_v2(
        &mut self,
        method: &str,
        headers: &str,
        body: &[u8],
    ) -> Result<OwnedResourcePreflightV2, StoreError> {
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
        let frame = OwnerSetupResourceFinalHttpV2::decode_http(
            body,
            one_header(headers, RESOURCE_READ_PROOF_HEADER)?,
        )
        .map_err(|_| unavailable())?;
        let resource = ResourceFinalConfigV2::from_env()?;
        if one_header(headers, "Origin")? != resource.issuer_pin.native_audience_origin {
            return Err(unavailable());
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        resource.require_context(config, &self.config)?;
        let entry = self
            .participant_entry
            .as_ref()
            .filter(|e| e.owner_control_enabled)
            .ok_or_else(unavailable)?;
        let gateway = entry.owned_owner_read_gateway();
        let fingerprint = resource.fingerprint(&gateway.fingerprint()?)?;
        let storage_key = session_key(&self.config.session_key, sid);
        let scope = ResourceFinalScopeV2 {
            custody: config,
            identity: &self.config,
            resource: &resource,
            configuration_digest: &fingerprint,
        };
        // This first claim commits independently. Every later failure keeps it spent.
        let invocation = self
            .store
            .transaction(|tx| registry::claim_native(tx, &scope, frame, storage_key, now))?;
        gateway.require_approved_world(plan_world(invocation.frame())?)?;
        let frame = invocation.frame();
        self.validate_preflight_participant_owner(
            &frame.paid().payload.participant_native_account_id,
            &frame.request().access.owner,
        )?;
        frame
            .world_body()
            .map_err(|_| unavailable())?
            .validate_bindings(&frame.paid().payload)
            .map_err(|_| unavailable())?;
        if entry.admission_issuer_public_key() == resource.issuer_pin.public_key
            || frame.paid().payload.owner_public_key == resource.issuer_pin.public_key
            || frame.request().access.admission.subject_public_key
                != entry.admission_issuer_public_key()
        {
            return Err(unavailable());
        }
        ws::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_PARTICIPANT_ADMISSION_DOMAIN,
            &frame.request().access.admission.request,
            entry.admission_issuer_public_key(),
            &frame.request().access.admission.signature_hex,
        )
        .map_err(|_| unavailable())?;
        let observed = self
            .store
            .transaction(|tx| registry::claim_preflight(tx, &scope, invocation, now))?;
        Ok(OwnedResourcePreflightV2 {
            invocation: observed,
            gateway,
        })
    }
    pub(in super::super::super) fn finish_owner_resource_preflight_v2(
        &mut self,
        completed: CompletedResourcePreflightV2,
    ) -> Result<OwnedResourceFinalV2, StoreError> {
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let resource = ResourceFinalConfigV2::from_env()?;
        let config = self.custody.as_ref().ok_or_else(unavailable)?;
        resource.require_context(config, &self.config)?;
        let gateway = self
            .participant_entry
            .as_ref()
            .filter(|e| e.owner_control_enabled)
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        gateway.require_approved_world(plan_world(completed.invocation.frame())?)?;
        let fingerprint = resource.fingerprint(&gateway.fingerprint()?)?;
        preflight(completed.invocation.frame(), &completed.response)?;
        let scope = ResourceFinalScopeV2 {
            custody: config,
            identity: &self.config,
            resource: &resource,
            configuration_digest: &fingerprint,
        };
        let invocation = self
            .store
            .transaction(|tx| registry::claim_resource(tx, &scope, completed.invocation, now))?;
        Ok(OwnedResourceFinalV2 {
            preflight_paid_reply: completed.paid_reply,
            invocation,
            gateway,
        })
    }
    pub(in super::super::super) fn finish_owner_resource_final_v2(
        &mut self,
        completed: CompletedResourceFinalV2,
    ) -> Result<Vec<u8>, StoreError> {
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let resource = ResourceFinalConfigV2::from_env()?;
        let config = self.custody.as_ref().ok_or_else(unavailable)?;
        resource.require_context(config, &self.config)?;
        let gateway = self
            .participant_entry
            .as_ref()
            .filter(|e| e.owner_control_enabled)
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        gateway.require_approved_world(plan_world(completed.invocation.frame())?)?;
        let fingerprint = resource.fingerprint(&gateway.fingerprint()?)?;
        let scope = ResourceFinalScopeV2 {
            custody: config,
            identity: &self.config,
            resource: &resource,
            configuration_digest: &fingerprint,
        };
        let paid_proof = completed.invocation.frame().paid().clone();
        self.store
            .transaction(|tx| registry::finish(tx, &scope, completed.invocation, now))?;
        let envelope = resource::PaidOwnerSetupResourceEnvelopeV1 {
            api_version: 1,
            ok: true,
            preflight: completed.preflight_paid_reply,
            resource_final: completed.final_paid_reply,
        };
        // Preserve the exact two received Node originals, never re-sign an inner
        // response into evidence of a World ledger claim.
        oasis7_custody_core::provisioning::OwnerSetupPaidResourceResultV2 {
            api_version: 1,
            paid_proof,
            world_evidence: envelope,
        }
        .encode()
        .map_err(|_| unavailable())
    }
}
