//! Two fixed paid World reads. No history resolver, P/R call, retry or caller
//! callback supplies authorization; every cut comes from the live publisher.
use super::*;
#[cfg(unix)]
pub(super) use oasis7::world_service::resource_world_read_service_v1::ResourceWorldReadServiceV1;
#[cfg(unix)]
use oasis7_client_api::world_service::{
    participant_discovery::PARTICIPANT_OWNER_VIEW_PATH, resource_basis::RESOURCE_GOAL_PATH_V2,
};
#[cfg(unix)]
fn handles(path: &str) -> bool {
    matches!(path, PARTICIPANT_OWNER_VIEW_PATH | RESOURCE_GOAL_PATH_V2)
}
#[cfg(unix)]
fn one_proof_header<'a>(headers: &'a str) -> Result<&'a str, String> {
    let name = oasis7::world_service::resource_world_paid_ledger_v1::RESOURCE_READ_PROOF_HEADER;
    let mut value = None;
    for line in headers.split("\r\n").skip(1) {
        if let Some((key, field)) = line.split_once(':') {
            if key.eq_ignore_ascii_case(name) {
                if value.is_some() {
                    return Err("duplicate Resource paid header".into());
                }
                value = Some(field.trim());
            }
        }
    }
    value
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "Resource paid header required".into())
}
impl FormalLiveService {
    pub(super) fn handle_with_resource_headers(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        headers: &str,
    ) -> HttpReply {
        #[cfg(unix)]
        {
            let proof_name =
                oasis7::world_service::resource_world_paid_ledger_v1::RESOURCE_READ_PROOF_HEADER;
            let has_paid_header = headers.split("\r\n").skip(1).any(|line| {
                line.split_once(':')
                    .is_some_and(|(key, _)| key.eq_ignore_ascii_case(proof_name))
            });
            if method != "POST" || !handles(path) || !has_paid_header {
                return self.handle(method, path, body);
            }
            let result = (|| {
                let resource = self
                    .resource_world_reads
                    .as_ref()
                    .ok_or_else(|| "paid Resource profile unavailable".to_string())?;
                let header = one_proof_header(headers)?;
                // Dedicated World ledger owns its own short commit lock. No
                // publisher/Node/admission/cache lock has been acquired yet.
                let consumed = resource
                    .claim_http(path, body, header)
                    .map_err(|e| e.to_string())?;
                let prepared = if path == PARTICIPANT_OWNER_VIEW_PATH {
                    let held = self.current_resource_publication()?;
                    let local = held
                        .lock()
                        .map_err(|_| "qualified Resource publication unavailable")?;
                    local
                        .qualified_cut()
                        .prepare_paid_resource_read(consumed, None, None)
                        .map_err(|e| e.to_string())?
                } else {
                    let authorized = {
                        let held = self.current_resource_publication()?;
                        let local = held
                            .lock()
                            .map_err(|_| "qualified Resource publication unavailable")?;
                        local
                            .qualified_cut()
                            .authorize_resource_archive_read(&consumed)
                            .map_err(|e| e.to_string())?
                    };
                    // Full cold proof replay and private Goal I/O occur after the
                    // initial publication/cache guard has been released.
                    let (archive, goal) = resource
                        .load_original_goal(&authorized, &consumed)
                        .map_err(|e| e.to_string())?;
                    let held = self.current_resource_publication()?;
                    let local = held
                        .lock()
                        .map_err(|_| "qualified Resource publication unavailable")?;
                    // Recompute quote in the actual fresh immutable cut, not the
                    // older authorization cut used before potentially slow I/O.
                    local
                        .qualified_cut()
                        .prepare_paid_resource_read(consumed, Some(&archive), Some(goal))
                        .map_err(|e| e.to_string())?
                };
                let delivery = {
                    let held = self.current_resource_publication()?;
                    let local = held
                        .lock()
                        .map_err(|_| "qualified Resource delivery unavailable")?;
                    local.qualified_cut().resource_delivery_cut()
                };
                // No publication/cache lock remains across floor verification or
                // signing. HTTP write is later, outside all these guards.
                resource
                    .finish(prepared, &delivery, &self.signer)
                    .map_err(|e| e.to_string())
            })();
            match result {
                Ok(body) => HttpReply { status: 200, body },
                Err(_) => raw_error(
                    path,
                    body,
                    "resource_unavailable",
                    "paid Resource read unavailable",
                    &self.signer,
                ),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = headers;
            self.handle(method, path, body)
        }
    }
    #[cfg(unix)]
    fn current_resource_publication(
        &self,
    ) -> Result<Arc<Mutex<oasis7::world_service::formal_read_service::LocalStopPublication>>, String>
    {
        // The old qualified cache is historical while the actual Node is not
        // ready. This runs only after the independent physical claim, with no
        // floor/publication/cache lock held while checking Node readiness.
        if !self.runtime_status().ready {
            return Err("formal Resource runtime is not ready".into());
        }
        self.published.observe(|cut| {
            cut.stop_publication
                .clone()
                .ok_or_else(|| "qualified local Resource publication unavailable".into())
        })
    }
    /// Trusted bootstrap only. This does not enable a profile through request
    /// JSON or initialize missing floor/once state; the actual stores are already
    /// configured and opened before this builder is called.
    #[cfg(unix)]
    pub(crate) fn with_resource_world_reads(
        mut self,
        resource: Arc<ResourceWorldReadServiceV1>,
    ) -> Self {
        self.resource_world_reads = Some(resource);
        self
    }
}

#[path = "formal_live_resource_read_config.rs"]
mod config;
