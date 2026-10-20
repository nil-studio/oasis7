//! Whole-attempt provider failover for formal full-state sync.

use super::*;
use std::sync::atomic::Ordering;

impl FormalFullSyncSession {
    pub(super) fn sync_across_providers(
        &mut self,
    ) -> Result<RestoredFormalFullSyncWorldV1, String> {
        let provider_count = 1 + self.config.alternate_endpoints.len();
        let starting_provider = self.active_provider_index;
        let mut failures = Vec::with_capacity(provider_count);
        for offset in 0..provider_count {
            let provider_index = (starting_provider + offset) % provider_count;
            if offset > 0
                && let Err(error) = self.activate_provider(provider_index)
            {
                // Replaying a saved proof chain is local verification, not
                // a provider read. A failure here is global to this fixed
                // C0 history and must not be bypassed by another endpoint.
                self.history.poison();
                return Err(format!(
                    "HistoryUnavailable: cannot replay the fixed external history for provider {provider_index}: {error}"
                ));
            }
            match self.sync_latest_inner() {
                Ok(restored) => return Ok(restored),
                Err(error) => {
                    failures.push(format!("provider[{provider_index}]: {error}"));
                    if self.history.is_poisoned()
                        || self.cas_publication_poisoned
                        || self.cas_integrity_poisoned.load(Ordering::Acquire)
                    {
                        return Err(failures.join("; "));
                    }
                }
            }
        }

        // A failed provider may have advanced only its in-memory verifier. Put
        // the session back on a fresh verifier replayed from durable history
        // before the next poll, even when there was only one provider.
        if let Err(error) = self.activate_provider(starting_provider) {
            self.history.poison();
            failures.push(format!(
                "HistoryUnavailable: could not restore the durable read cursor: {error}"
            ));
        }
        Err(failures.join("; "))
    }

    fn activate_provider(&mut self, provider_index: usize) -> Result<(), String> {
        let providers = self.config.providers();
        let provider = providers
            .get(provider_index)
            .ok_or("full-sync provider index is outside the configured list")?;
        let client = providers::build_read_client(
            &self.config,
            provider,
            &self.node_private_key_hex,
            Duration::from_millis(self.config.request_timeout_ms),
        )?;
        self.history
            .replay_into_fresh_client(&self.config, &client)?;
        self.client = client;
        self.active_provider_index = provider_index;
        Ok(())
    }
}
