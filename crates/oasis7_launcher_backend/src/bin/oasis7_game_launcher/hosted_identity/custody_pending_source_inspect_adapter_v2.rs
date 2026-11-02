//! Identity-only fixed Source receiver. No ordinary approval verification fallback.
use super::super::now_ms;
use super::*;
use cleanup_config::MachineConfig;
use cleanup_registry::pending_source_inspect::{self, SourceInspectScope};
use oasis7_custody_core::execution_pending_source_inspect_v2::*;
fn unavailable() -> StoreError {
    reject("pending_source_inspect_unavailable")
}
fn unique_header<'a>(headers: &'a str, name: &str) -> Result<Option<&'a str>, StoreError> {
    let mut values = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim());
    let first = values.next();
    if values.next().is_some() {
        return Err(unavailable());
    }
    Ok(first)
}
impl IdentityService {
    pub(in super::super::super) fn pending_source_inspect_route_v2(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        raw: &[u8],
    ) -> Result<serde_json::Value, StoreError> {
        self.store.require_guarded_machine_storage()?;
        if method != "POST"
            || path != SOURCE_NATIVE_PATH_V2
            || unique_header(headers, "origin")?.is_some()
            || unique_header(headers, "cookie")?.is_some()
            || unique_header(headers, "content-type")? != Some("application/json")
        {
            return Err(unavailable());
        }
        let config = self
            .custody
            .as_ref()
            .filter(|c| c.managed_binding.is_some())
            .ok_or_else(unavailable)?;
        let auth = unique_header(headers, "authorization")?
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|v| hex::decode(v).ok())
            .filter(|v| v.len() == 32)
            .ok_or_else(unavailable)?;
        if blake3::keyed_hash(&self.config.session_key, &auth)
            != blake3::keyed_hash(&self.config.session_key, &config.adapter_secret)
        {
            return Err(unavailable());
        }
        // Typed-first decoder also checks duplicate keys in preserved Value subtrees.
        let request = PendingSourceNativeRequestV2::decode(raw).map_err(|_| unavailable())?;
        let machine = MachineConfig::from_env()?;
        machine.source_inspect_pin()?;
        let gateway = self
            .participant_entry
            .as_ref()
            .ok_or_else(unavailable)?
            .owned_owner_read_gateway();
        let fingerprint = machine.fingerprint(config, &self.config, &gateway.fingerprint()?)?;
        let claimed_at = now_ms();
        if !self.clock.observe(claimed_at) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let scope = SourceInspectScope {
            config,
            identity: &self.config,
            machine: &machine,
            configuration_digest: &fingerprint,
            now: claimed_at,
        };
        // First permanent claim has its own commit. A fallible Current/sign step
        // afterwards cannot refund or recreate its first-return handle.
        let claimed = self.store.transaction(|tx| {
            pending_source_inspect::claim_pending_source_inspect(tx, &scope, request)
        })?;
        let completed_at = now_ms();
        if completed_at < claimed_at || !self.clock.observe(completed_at) {
            self.email.clock_healthy = false;
            return Err(unavailable());
        }
        let scope = SourceInspectScope {
            now: completed_at,
            ..scope
        };
        self.store.transaction(|tx| {
            pending_source_inspect::finish_pending_source_inspect(tx, &scope, claimed)
        })
    }
}
