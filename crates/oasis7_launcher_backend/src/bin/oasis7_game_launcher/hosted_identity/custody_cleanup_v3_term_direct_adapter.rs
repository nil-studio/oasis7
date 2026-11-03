//! Local-only Direct5 receiver. Each HTTP attempt is paid by the original frame.
use super::*;
use oasis7_custody_core::native_cleanup_v2::{
    NativeTermDirectFrameV3, SignedNativeTermDirectFactV3,
};
impl IdentityService {
    pub(in super::super::super) fn observe_cleanup_term_direct_v3(
        &mut self,
        method: &str,
        body: &[u8],
    ) -> Result<SignedNativeTermDirectFactV3, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST" {
            return Err(unavailable());
        }
        let request = NativeTermDirectFrameV3::decode(body).map_err(|_| unavailable())?;
        #[cfg(test)]
        let fixture_until = request.actor.binding.expires_at.0;
        #[cfg(test)]
        let fixture_pause = request.body.source_operation_id == "source-expires-after-claim";
        let now = super::super::now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let machine = cleanup_config::MachineConfig::from_env()?;
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
        // A failed Current check cannot refund this independently committed claim.
        let prepared = self.store.transaction(|tx| {
            cleanup_registry::claim_term_direct_registered_v3(
                tx,
                config,
                &self.config,
                &machine,
                &fingerprint,
                request,
                now,
            )
        })?;
        #[cfg(test)]
        if guarded_consent::term_direct_v3_fixture_enabled() && fixture_pause {
            let hard_started = std::time::Instant::now();
            while super::super::now_ms() <= fixture_until.saturating_add(20) {
                assert!(
                    hard_started.elapsed() < std::time::Duration::from_secs(6),
                    "fixture wall clock did not reach the original expiry"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        // TX1 is already committed. Fresh time may reject TX2 but cannot
        // release or replay the original physical attempt.
        let now = super::super::now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        self.store.transaction(|tx| {
            cleanup_registry::observe_term_direct_registered_v3(
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
fn unavailable() -> StoreError {
    StoreError::Rejected("term_direct_current_unavailable".into())
}
