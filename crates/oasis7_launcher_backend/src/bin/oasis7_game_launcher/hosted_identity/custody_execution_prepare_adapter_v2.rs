//! Actual managed Cookie/Origin/CSRF fixed Prepare adapter. The World work is
//! owned and sent outside the identity mutex, without any Native/Owner secret.
use super::super::{now_ms, participant_entry::OwnedOwnerReadGateway, service::cookie_value};
use super::*;
use cleanup_config::MachineConfig;
use cleanup_registry::execution_identity::{self, ExecutionScope, ObservedExecutionPrepare};
use ed25519_dalek::Signer;
use oasis7_client_api::world_service::task_context_projection::{
    TaskContextPreparedDeliveryV2, WorldContextAttestationV2,
};
use oasis7_custody_core::native_execution_prepare_v2::*;
pub(in super::super::super) struct OwnedExecutionPrepareWorkV2 {
    stage: ObservedExecutionPrepare,
    storage_key: String,
    configuration_digest: String,
    gateway: OwnedOwnerReadGateway,
}
pub(in super::super::super) struct CompletedExecutionPrepareWorkV2 {
    stage: ObservedExecutionPrepare,
    storage_key: String,
    configuration_digest: String,
    response: WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
}
impl OwnedExecutionPrepareWorkV2 {
    pub(in super::super::super) fn send(
        self,
    ) -> Result<CompletedExecutionPrepareWorkV2, StoreError> {
        let original = self.stage.request().original.as_ref();
        let response = self.gateway.execution_prepare_funded_v2(original, || {
            self.stage.require_dispatch_window(now_ms())
        })?;
        Ok(CompletedExecutionPrepareWorkV2 {
            stage: self.stage,
            storage_key: self.storage_key,
            configuration_digest: self.configuration_digest,
            response,
        })
    }
}
fn unavailable() -> StoreError {
    reject("execution_prepare_unavailable")
}
impl IdentityService {
    pub(in super::super::super) fn prepare_execution_funded_v2(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
    ) -> Result<OwnedExecutionPrepareWorkV2, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST"
            || path != NATIVE_EXECUTION_PREPARE_PATH_V2
            || body.len() > MAX_NATIVE_EXECUTION_PREPARE_WIRE_BYTES_V2
        {
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
        let raw: serde_json::Value = serde_json::from_slice(body).map_err(|_| unavailable())?;
        let request: NativeExecutionPrepareRequestV2 =
            serde_json::from_value(raw.clone()).map_err(|_| unavailable())?;
        if serde_json::to_value(&request).map_err(|_| unavailable())? != raw {
            return Err(unavailable());
        }
        request.wire_bytes().map_err(|_| unavailable())?;
        if request.proof.certificate.native_binding.native_account_id != account.account_id
            || request
                .original
                .request
                .original
                .request
                .client_operation_id
                != request.proof.certificate.stage_operation_id
        {
            return Err(unavailable());
        }
        let entry = self
            .participant_entry
            .as_ref()
            .filter(|v| v.executor_control_enabled)
            .ok_or_else(unavailable)?;
        let gateway = entry.owned_owner_read_gateway();
        gateway.require_approved_world(&request.proof.certificate.world)?;
        let machine = MachineConfig::from_env()?;
        let config = self
            .custody
            .as_ref()
            .filter(|v| v.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        let key = session_key(&self.config.session_key, sid);
        let scope = ExecutionScope {
            config,
            identity: &self.config,
            machine: &machine,
            storage_key: &key,
            configuration_digest: &fingerprint,
            now,
        };
        // Durable independent attempt transaction first. Neither a failing
        // Current read nor an unknown World reply can release these once rows.
        let fence = &request.original.access.fence;
        let resource = oasis7_client_api::world_service::request_digest(
            "entry-executor-resource",
            &(
                fence.world_id.as_str(),
                fence.genesis_digest.as_str(),
                fence.agent_id.as_str(),
                fence.task_id.as_str(),
                fence.owner_generation,
                fence.executor_epoch,
            ),
        )
        .map_err(|_| unavailable())?;
        let claimed = self
            .store
            .transaction(|tx| execution_identity::claim_execution_prepare(tx, &scope, request))?;
        let stage = self.store.transaction(|tx| {
            consume_quota(
                tx,
                "entry-executor-account",
                &account.account_id,
                now,
                60000,
                240,
            )?;
            let observed = execution_identity::observe_execution_prepare(tx, &scope, claimed)?;
            // Only the signed fixed Account original may spend this fence's
            // bucket. A caller cannot choose an arbitrary resource or actor.
            consume_quota(tx, "entry-executor-resource", &resource, now, 60000, 120)?;
            Ok(observed)
        })?;
        Ok(OwnedExecutionPrepareWorkV2 {
            stage,
            storage_key: key,
            configuration_digest: fingerprint,
            gateway,
        })
    }
    pub(in super::super::super) fn finish_execution_funded_v2(
        &mut self,
        completed: CompletedExecutionPrepareWorkV2,
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
            .filter(|v| v.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let gateway = self
            .participant_entry
            .as_ref()
            .filter(|v| v.executor_control_enabled)
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        if fingerprint != completed.configuration_digest {
            return Err(unavailable());
        }
        let request = completed.stage.request().clone();
        gateway.require_approved_world(&request.proof.certificate.world)?;
        let result_digest =
            prepared_response_digest_v2(&completed.response).map_err(|_| unavailable())?;
        let scope = ExecutionScope {
            config,
            identity: &self.config,
            machine: &machine,
            storage_key: &completed.storage_key,
            configuration_digest: &fingerprint,
            now,
        };
        self.store.transaction(|tx| {
            execution_identity::finish_execution_prepare(tx, &scope, completed.stage)
        })?;
        let c = &request.proof.certificate;
        let receipt = NativeExecutionPrepareReceiptV2 {
            api_version: 2,
            native_key_id: config.key_id.clone(),
            proof_digest: request.proof.digest().map_err(|_| unavailable())?,
            native_attempt_id: c.ports[6].attempt_id.clone(),
            current_attempt_id: c.ports[7].attempt_id.clone(),
            prepare_attempt_id: c.ports[8].attempt_id.clone(),
            result_digest,
            public_projection_digest:oasis7_client_api::world_service::task_context_projection::public_projection_digest_v2(
                &completed.response.payload.projection).map_err(|_|unavailable())?,
            completed_at: DecimalU64(now),
        };
        let signature_hex = hex::encode(
            config
                .key
                .sign(&receipt.signing_bytes(&request).map_err(|_| unavailable())?)
                .to_bytes(),
        );
        let reply = NativeExecutionPrepareReplyV2 {
            api_version: 2,
            world_response: completed.response,
            physical_receipt: SignedNativeExecutionPrepareReceiptV2 {
                receipt,
                signature_hex,
            },
        };
        reply.wire_bytes().map_err(|_| unavailable())?;
        serde_json::to_value(reply).map_err(|_| unavailable())
    }
}
