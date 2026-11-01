//! Explicit human-cookie registration adapter. Owned World work is executed
//! outside the identity mutex; neither this token nor its gateway carries keys.
use super::super::{now_ms, participant_entry::OwnedOwnerReadGateway, service::cookie_value};
use super::*;
use cleanup_config::MachineConfig;
use cleanup_registry::PreparedRegistration;
use oasis7_custody_core::native_cleanup_v2::*;
pub(in super::super::super) struct OwnedRegistrationWork {
    prepared: PreparedRegistration,
    gateway: OwnedOwnerReadGateway,
    configuration_digest: String,
}
pub(in super::super::super) struct CompletedRegistrationWork {
    prepared: PreparedRegistration,
    configuration_digest: String,
    response: oasis7_client_api::world_service::SignedServiceResponse<
        oasis7_client_api::world_service::participant_discovery::OwnerSetupObservationV1,
    >,
}
impl OwnedRegistrationWork {
    pub(in super::super::super) fn send(self) -> Result<CompletedRegistrationWork, StoreError> {
        let response = self.gateway.read_owner_setup(self.prepared.owner_read())?;
        Ok(CompletedRegistrationWork {
            prepared: self.prepared,
            configuration_digest: self.configuration_digest,
            response,
        })
    }
}
fn unavailable() -> StoreError {
    StoreError::Rejected("cleanup_registration_unavailable".into())
}
impl IdentityService {
    pub(in super::super::super) fn prepare_cleanup_registration_v2(
        &mut self,
        method: &str,
        headers: &str,
        body: &[u8],
    ) -> Result<OwnedRegistrationWork, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" || body.len() > 12_288 {
            return Err(unavailable());
        }
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let sid = cookie_value(headers, "__Host-oasis7_sid").ok_or_else(unavailable)?;
        let csrf = digest(&self.config.session_key, "csrf", sid);
        self.validate_json_write(headers, &csrf)?;
        let (account, _) = self.session(Some(sid), now)?;
        // The replacement read uses the existing participant-request abuse
        // limiter; it does not mint another business/session transport cap.
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
        let request: NativeReadAndRegisterRequestV2 =
            serde_json::from_slice(body).map_err(|_| unavailable())?;
        request.validate().map_err(|_| unavailable())?;
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
        let storage_key = session_key(&self.config.session_key, sid);
        let paid = self.store.transaction(|tx| {
            cleanup_registry::claim_registration_native(
                tx,
                config,
                &self.config,
                request,
                &machine.pending_pin,
                &machine.issuer_floor,
                now,
            )
        })?;
        let prepared = self.store.transaction(|tx| {
            cleanup_registry::claim(
                tx,
                config,
                &self.config,
                &storage_key,
                &fingerprint,
                paid,
                &machine.pending_pin,
                &machine.issuer_floor,
                now,
            )
        })?;
        Ok(OwnedRegistrationWork {
            prepared,
            gateway,
            configuration_digest: fingerprint,
        })
    }
    pub(in super::super::super) fn finish_cleanup_registration_v2(
        &mut self,
        completed: CompletedRegistrationWork,
    ) -> Result<NativeReadAndRegisterReplyV2, StoreError> {
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
        self.store.transaction(|tx| {
            cleanup_registry::finish(
                tx,
                config,
                &self.config,
                &fingerprint,
                completed.prepared,
                completed.response,
                now,
            )
        })
    }
}

impl IdentityService {
    /// Dedicated Account certificate authentication, never a Cookie exemption
    /// applied to any human/browser route. It sends no World request.
    pub(in super::super::super) fn activate_cleanup_registration_v2(
        &mut self,
        method: &str,
        body: &[u8],
    ) -> Result<SignedNativeActivationAckV2, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" || body.len() > 12_288 {
            return Err(unavailable());
        }
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let request: NativeActivateRequestV2 =
            serde_json::from_slice(body).map_err(|_| unavailable())?;
        request.validate_shape().map_err(|_| unavailable())?;
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
        let paid = self.store.transaction(|tx| {
            cleanup_registry::claim_activation_native(
                tx,
                &fingerprint,
                request,
                &machine.active_pin,
                &machine.issuer_floor,
                now,
            )
        })?;
        let ack = self.store.transaction(|tx| {
            cleanup_registry::activate_registered(
                tx,
                config,
                &self.config,
                &fingerprint,
                paid,
                &machine.active_pin,
                &machine.issuer_floor,
                now,
            )
        })?;
        // Signal extraction is a separate local read after the first actual
        // Active commit. Any scheduling/extraction failure leaves Active intact.
        let signal = self
            .store
            .transaction(|tx| cleanup_registry::active_model_signal(tx, config, &self.config, &ack))
            .ok()
            .flatten();
        // The transaction above has committed. This callback may only enqueue
        // local work; the Native handler still owns the identity mutex.
        if let (Some(authority), Some(signal)) = (&self.model_proxy_basis, signal) {
            authority.active_committed(signal);
        }
        Ok(ack)
    }
}

impl IdentityService {
    /// Machine proof is exact Account-issuer method/path/body/nonce authority;
    /// no browser cookie is read and no ordinary route gains an exemption.
    pub(in super::super::super) fn observe_cleanup_current_v2(
        &mut self,
        method: &str,
        body: &[u8],
    ) -> Result<SignedNativeMachineCurrentFactV2, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" || body.len() > 12_288 {
            return Err(unavailable());
        }
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let request: NativeCurrentEnvelopeV2 =
            serde_json::from_slice(body).map_err(|_| unavailable())?;
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
        let prepared = self.store.transaction(|tx| {
            cleanup_registry::claim_current_registered(tx, &machine, &fingerprint, request, now)
        })?;
        self.store.transaction(|tx| {
            cleanup_registry::observe_current_registered(
                tx,
                config,
                &self.config,
                &machine,
                &fingerprint,
                prepared,
                now,
            )
        })
    }
}

impl IdentityService {
    /// Machine proof is exact Account-issuer method/path/body/nonce authority;
    /// no browser cookie is read and no ordinary route gains an exemption.
    pub(in super::super::super) fn observe_cleanup_term_current_v2(
        &mut self,
        method: &str,
        body: &[u8],
    ) -> Result<SignedNativeTermCurrentFactV2, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" || body.len() > 12_288 {
            return Err(unavailable());
        }
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let request: NativeTermCurrentEnvelopeV2 =
            serde_json::from_slice(body).map_err(|_| unavailable())?;
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
        let prepared = self.store.transaction(|tx| {
            cleanup_registry::claim_term_current_registered(
                tx,
                &machine,
                &fingerprint,
                request,
                now,
            )
        })?;
        self.store.transaction(|tx| {
            cleanup_registry::observe_term_current_registered(
                tx,
                config,
                &self.config,
                &machine,
                &fingerprint,
                prepared,
                now,
            )
        })
    }
}
