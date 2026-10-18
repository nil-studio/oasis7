use super::*;
use crate::viewer::runtime_live::agent_service_io::*;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn prepare_provider_metadata_probe(
        &mut self,
    ) -> Result<Option<AgentServiceIoJob>, String> {
        let Some(settings) = provider_settings_from_env()? else {
            self.provider_check_snapshot = None;
            self.provider_metadata_inflight = None;
            self.provider_metadata_generation = self.provider_metadata_generation.saturating_add(1);
            return Ok(None);
        };
        let key = runtime_provider_check_cache_key(&settings);
        let now = runtime_provider_check_now_unix_ms();
        if self
            .provider_metadata_inflight
            .as_ref()
            .is_some_and(|(original, _)| original == &key)
        {
            return Ok(None);
        }
        if self
            .provider_check_snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.cache_key == key
                    && now.saturating_sub(snapshot.checked_at_unix_ms)
                        < RUNTIME_PROVIDER_CHECK_CACHE_MS
            })
        {
            return Ok(None);
        }
        self.provider_metadata_generation = self.provider_metadata_generation.saturating_add(1);
        let generation = self.provider_metadata_generation;
        let timeout = if settings.provider_transport == REMOTE_HTTPS_PROVIDER_TRANSPORT {
            settings.connect_timeout_ms.max(1500)
        } else {
            settings.connect_timeout_ms.min(500)
        };
        let client = ProviderLoopbackHttpClient::new_with_transport(
            &settings.base_url,
            settings.auth_token.as_deref(),
            timeout,
            &settings.provider_transport,
        )
        .map_err(|error| error.to_string())?;
        self.provider_metadata_inflight = Some((key.clone(), generation));
        self.provider_check_snapshot = Some(RuntimeProviderCheckSnapshot {
            source: "runtime_live_probe".into(),
            status: "check_pending".into(),
            capabilities: vec![],
            supported_action_sets: vec![],
            fallback_reason: None,
            error: None,
            checked_at_unix_ms: 0,
            cache_key: key.clone(),
        });
        Ok(Some(AgentServiceIoJob {
            token: AgentServiceIoToken {
                generation,
                config_digest: key,
                phase_digest: "metadata".into(),
            },
            client: None,
            operation: AgentServiceIoOperation::Metadata(client),
        }))
    }

    pub(in crate::viewer::runtime_live) fn apply_provider_metadata_probe(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<(), String> {
        let token = result.token;
        let current = provider_settings_from_env()?
            .map(|settings| runtime_provider_check_cache_key(&settings));
        if token.phase_digest != "metadata"
            || current.as_ref() != Some(&token.config_digest)
            || self.provider_metadata_inflight.as_ref()
                != Some(&(token.config_digest.clone(), token.generation))
        {
            // Old completions never overwrite a newer configuration/cache.
            return Ok(());
        }
        self.provider_metadata_inflight = None;
        let checked_at_unix_ms = runtime_provider_check_now_unix_ms();
        let mut snapshot = RuntimeProviderCheckSnapshot {
            source: "runtime_live_probe".into(),
            status: "check_failed".into(),
            capabilities: vec![],
            supported_action_sets: vec![],
            fallback_reason: None,
            error: None,
            checked_at_unix_ms,
            cache_key: token.config_digest,
        };
        match result.response {
            Ok(AgentServiceIoResponse::Metadata(info, health)) => {
                let compatibility = evaluate_provider_compatibility(&info, Some(&health));
                snapshot.status = compatibility.status.as_str().into();
                snapshot.capabilities = info.capabilities;
                snapshot.supported_action_sets = info.supported_action_sets;
                snapshot.fallback_reason = compatibility.fallback_reason;
            }
            Ok(_) => return Err("provider metadata response operation mismatch".into()),
            Err(_) => snapshot.error = Some("provider metadata probe failed".into()),
        }
        self.provider_check_snapshot = Some(snapshot);
        #[cfg(any(test, feature = "test_tier_required"))]
        if std::env::var("PRE2_WORLD_COHERENCE_TRACE").is_ok_and(|value| value == "1") {
            eprintln!(
                "PRE2_METADATA_PROBE phase=applied generation={} status={} failed={}",
                token.generation,
                self.provider_check_snapshot.as_ref().unwrap().status,
                self.provider_check_snapshot
                    .as_ref()
                    .unwrap()
                    .error
                    .is_some()
            );
        }
        Ok(())
    }
}

impl crate::viewer::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn prepare_agent_service_io(
        &mut self,
        _eligible: bool,
    ) -> Result<AgentServiceProgress, String> {
        let service = self.prepare_hosted_service_io(_eligible)?;
        if !matches!(service, AgentServiceProgress::Idle) {
            return Ok(service);
        }
        let fresh = self.prepare_fresh_service_io(_eligible)?;
        if !matches!(fresh, AgentServiceProgress::Idle) {
            return Ok(fresh);
        }
        match self.llm_sidecar.prepare_provider_metadata_probe()? {
            Some(job) => Ok(AgentServiceProgress::NeedsIo(Box::new(job))),
            None => Ok(AgentServiceProgress::Idle),
        }
    }
    pub(in crate::viewer::runtime_live) fn apply_agent_service_io(
        &mut self,
        result: AgentServiceIoResult,
    ) -> Result<AgentServiceProgress, String> {
        if result.token.phase_digest.starts_with("admission:") {
            return self.apply_fresh_service_io(result);
        }
        if result.token.phase_digest != "metadata" {
            return self.apply_hosted_service_io(result);
        }
        self.llm_sidecar.apply_provider_metadata_probe(result)?;
        Ok(AgentServiceProgress::Advanced)
    }
    pub(in crate::viewer::runtime_live) fn agent_service_io_worker_failed(&mut self) {
        self.llm_sidecar.provider_metadata_inflight = None;
        self.llm_sidecar.hosted_service_inflight = None;
        // Canonical maps are durable; next preparation reconstructs original Lookup.
        self.llm_sidecar.hosted_service_phase = None;
    }
}

#[derive(Clone, Debug)]
pub(in crate::viewer::runtime_live) enum FreshProviderMetadataReadiness {
    Ready { identity: String },
    Pending,
    Stale,
    Failed,
    Incompatible,
    Unavailable,
}
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn fresh_provider_metadata_readiness(
        &self,
    ) -> Result<FreshProviderMetadataReadiness, String> {
        let Some(settings) = provider_settings_from_env()? else {
            return Ok(FreshProviderMetadataReadiness::Unavailable);
        };
        let key = runtime_provider_check_cache_key(&settings);
        if self.provider_metadata_inflight.is_some() {
            return Ok(FreshProviderMetadataReadiness::Pending);
        }
        let Some(snapshot) = self.provider_check_snapshot.as_ref() else {
            return Ok(FreshProviderMetadataReadiness::Pending);
        };
        let now = runtime_provider_check_now_unix_ms();
        if snapshot.cache_key != key
            || snapshot.checked_at_unix_ms > now
            || now.saturating_sub(snapshot.checked_at_unix_ms) >= RUNTIME_PROVIDER_CHECK_CACHE_MS
        {
            return Ok(FreshProviderMetadataReadiness::Stale);
        }
        if snapshot.error.is_some() || snapshot.status == "check_failed" {
            return Ok(FreshProviderMetadataReadiness::Failed);
        }
        if snapshot.status != "ready" {
            return Ok(FreshProviderMetadataReadiness::Incompatible);
        }
        Ok(FreshProviderMetadataReadiness::Ready {
            identity: blake3::hash(key.as_bytes()).to_hex().to_string(),
        })
    }
    pub(in crate::viewer::runtime_live) fn fresh_provider_metadata_identity(
        &self,
    ) -> Result<Option<String>, String> {
        Ok(match self.fresh_provider_metadata_readiness()? {
            FreshProviderMetadataReadiness::Ready { identity } => Some(identity),
            _ => None,
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "llm_sidecar_metadata_probe_tests.rs"]
mod tests;
