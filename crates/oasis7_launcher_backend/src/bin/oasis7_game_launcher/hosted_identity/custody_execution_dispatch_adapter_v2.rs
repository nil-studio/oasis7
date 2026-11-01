//! NativeRoot stays under the real Identity lock. Owned HTTPS work has public
//! exact typed data only and cannot issue a new query or read managed Current.
use super::execution_prepare_adapter::ExecutionWorldReentryContextV2;
use super::*;
use cleanup_config::MachineConfig;
use cleanup_registry::execution_identity::{self, ExecutionScope};
use oasis7_custody_core::{execution_dispatch_gate_v2::*, native_execution_prepare_v2::*};
use std::{io::Read, time::Duration};
fn unavailable() -> StoreError {
    reject("execution_dispatch_unavailable")
}
pub(in super::super::super) struct OwnedExecutionWorldReentryV2 {
    callback: NativeExecutionDispatchCallbackV2,
    http: reqwest::blocking::Client,
}
impl OwnedExecutionWorldReentryV2 {
    pub(in super::super::super) fn send(
        self,
    ) -> Result<
        (
            NativeExecutionDispatchCallbackV2,
            SignedExecutionDispatchClaimV2,
        ),
        StoreError,
    > {
        let raw = self.callback.wire_bytes().map_err(|_| unavailable())?;
        let q = &self.callback.query.query;
        let remaining = q
            .expires_at
            .0
            .checked_sub(super::super::now_ms())
            .filter(|v| *v > 0)
            .ok_or_else(unavailable)?;
        let response = self
            .http
            .post(format!(
                "{}{}",
                q.identity_origin, NATIVE_EXECUTION_REENTRY_PATH_V2
            ))
            .header("Content-Type", "application/json")
            .timeout(Duration::from_millis(remaining))
            .body(raw)
            .send()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if response.status().as_u16() != 200 {
            return Err(StoreError::OutcomeUnknown);
        }
        let mut raw = Vec::new();
        response
            .take(32769)
            .read_to_end(&mut raw)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if raw.len() > 32768 {
            return Err(unavailable());
        }
        let signed = serde_json::from_slice(&raw).map_err(|_| unavailable())?;
        Ok((self.callback, signed))
    }
}
impl IdentityService {
    pub(in super::super::super) fn prepare_execution_world_reentry_v2(
        &mut self,
        context: &ExecutionWorldReentryContextV2<'_>,
        previous: &SignedExecutionDispatchClaimV2,
        edge: ExecutionPreparePhysicalEdgeV2,
        body: &[u8],
    ) -> Result<OwnedExecutionWorldReentryV2, StoreError> {
        let stage = context.stage;
        let saved = (context.storage_key, context.configuration_digest);
        let now = super::super::now_ms();
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
            .owned_execution_read_gateway()?;
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        if fingerprint != saved.1 {
            return Err(unavailable());
        }
        gateway.require_approved_world(&stage.request().proof.certificate.world)?;
        let scope = ExecutionScope {
            config,
            identity: &self.config,
            machine: &machine,
            storage_key: saved.0,
            configuration_digest: &fingerprint,
            now,
        };
        // This transaction commits the physical Native→Identity attempt before
        // the token escapes the mutex, even if later HTTPS/current fails.
        let callback = self.store.transaction(|tx| {
            execution_identity::claim_execution_world_callback(
                tx,
                &scope,
                stage,
                previous,
                edge,
                oasis7_custody_core::approval::bytes_digest(body),
            )
        })?;
        let builder = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(1));
        #[cfg(test)]
        let builder = fixture_tls::extend(builder);
        let http = builder.build().map_err(|_| unavailable())?;
        Ok(OwnedExecutionWorldReentryV2 { callback, http })
    }
    pub(in super::super::super) fn finish_execution_world_reentry_v2(
        &mut self,
        context: &ExecutionWorldReentryContextV2<'_>,
        callback: &NativeExecutionDispatchCallbackV2,
        signed: &SignedExecutionDispatchClaimV2,
    ) -> Result<(), StoreError> {
        let stage = context.stage;
        let saved = (context.storage_key, context.configuration_digest);
        let now = super::super::now_ms();
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
            .owned_execution_read_gateway()?;
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        if fingerprint != saved.1 {
            return Err(unavailable());
        }
        gateway.require_approved_world(&stage.request().proof.certificate.world)?;
        let scope = ExecutionScope {
            config,
            identity: &self.config,
            machine: &machine,
            storage_key: saved.0,
            configuration_digest: &fingerprint,
            now,
        };
        self.store.transaction(|tx| {
            execution_identity::recheck_execution_prepare(tx, &scope, stage)?;
            let stored = tx
                .get::<NativeExecutionDispatchCallbackV2>(&format!(
                    "custody_execution_world_reentry_v2:{}",
                    callback.query.query.callback_attempt_ids[0]
                ))?
                .ok_or_else(unavailable)?
                .value;
            if stored.wire_bytes().map_err(|_| unavailable())?
                != callback.wire_bytes().map_err(|_| unavailable())?
            {
                return Err(unavailable());
            }
            callback
                .query
                .verify_native(
                    stage.request(),
                    &config.key_id,
                    &hex::encode(config.key.verifying_key().to_bytes()),
                    machine.execution_identity_origin()?,
                    now,
                )
                .map_err(|_| unavailable())?;
            let pin = machine.execution_dispatch_pin()?;
            let floor = cleanup_registry::admit_issuer_floor(tx, pin, &machine.issuer_floor)?;
            signed
                .verify_issuer(stage.request(), pin, &floor, now)
                .map_err(|_| unavailable())?;
            if signed.claim.leaf != callback.query.query.leaf
                || signed.claim.body_digest != callback.query.query.body_digest
                || signed.claim.claim_ordinal.0 <= callback.previous.claim.claim_ordinal.0
            {
                return Err(unavailable());
            }
            Ok(())
        })
    }
}

// Component test transport trust only. Production has no CA environment flag,
// no relaxed certificate/hostname checks and no caller-selected TLS roots.
#[cfg(test)]
pub(in super::super) mod fixture_tls {
    use std::cell::RefCell;
    thread_local! { static ROOT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) }; }
    pub(super) fn extend(
        builder: reqwest::blocking::ClientBuilder,
    ) -> reqwest::blocking::ClientBuilder {
        ROOT.with_borrow(|root| match root {
            None => builder,
            Some(pem) => {
                builder.tls_certs_only([reqwest::Certificate::from_pem(pem).expect("fixture CA")])
            }
        })
    }
    pub(in super::super::super) fn with_root<T>(pem: &[u8], run: impl FnOnce() -> T) -> T {
        struct Restore(Option<Vec<u8>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                ROOT.set(self.0.take());
            }
        }
        let _restore = Restore(ROOT.replace(Some(pem.to_vec())));
        run()
    }
}

#[cfg(test)]
impl IdentityService {
    pub(crate) fn with_execution_callback_fixture_root<T>(
        pem: &[u8],
        run: impl FnOnce() -> T,
    ) -> T {
        fixture_tls::with_root(pem, run)
    }
}
