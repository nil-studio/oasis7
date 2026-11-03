//! Original-material context and explicit pending-owner lifecycle.
use super::*;

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct RuntimeProviderActionContext {
    pub(in crate::viewer::runtime_live) request: cognition_context::ProviderContextState,
    pub(in crate::viewer::runtime_live) response: ContinuousAgentResponseContextV1,
    /// Runtime's durable admission lease remains correlated with the provider
    /// response until the Runtime receipt or terminal disposition closes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::viewer::runtime_live) cognition_lease: Option<crate::runtime::CognitionLeaseV1>,
    /// Keep provider memory intents correlated with the response until the
    /// Runtime receipt/readback authorizes their durable projection.
    #[serde(default)]
    pub(in crate::viewer::runtime_live) memory_write_intents:
        Vec<crate::simulator::MemoryWriteIntent>,
    /// Exact request/response content refs from the explicitly installed
    /// native pending owner. These refs are a cache over that owner; omitting
    /// them from the legacy lineage checkpoint never deletes the owned bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::viewer::runtime_live) provider_original_material_refs:
        Option<crate::simulator::ProviderOriginalMaterialRefsV1>,
    /// Process-local charge for the decoded response context. This lease is
    /// intentionally absent from the persisted lineage representation.
    #[serde(skip)]
    pub(in crate::viewer::runtime_live) provider_response_context_lease:
        Option<crate::simulator::provider_json_admission::ProviderResponseContextLease>,
}

impl RuntimeLlmSidecar {
    /// Install the native pending owner only after the caller has verified its
    /// explicit new-format world/genesis/parent binding. The legacy lineage
    /// store is not consulted and cannot enable this capture lane.
    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::viewer::runtime_live) fn install_provider_original_material_store(
        &mut self,
        store: std::sync::Arc<dyn ProviderOriginalMaterialStore>,
    ) -> Result<(), String> {
        if self.runner.is_some() || !self.provider_agent_ids.is_empty() {
            return Err(
                "provider original material owner must be installed before runner initialization"
                    .into(),
            );
        }
        self.provider_original_material_store = Some(store);
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn provider_original_material_capture_enabled(
        &self,
    ) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.provider_original_material_store.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    /// Require the explicitly installed pending owner to authenticate captured
    /// request/response refs before a provider result enters a Runtime commit.
    pub(in crate::viewer::runtime_live) fn verify_provider_original_materials(
        &self,
        cognition: &RuntimeProviderActionContext,
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match (
                self.provider_original_material_store.as_deref(),
                cognition.provider_original_material_refs.as_ref(),
            ) {
                (None, None) => Ok(()),
                (Some(_), None) => Err(
                    "provider original material owner is installed but this action has no captured refs"
                        .into(),
                ),
                (None, Some(_)) => Err(
                    "provider original material refs have no installed pending owner".into(),
                ),
                (Some(store), Some(refs)) => {
                    if refs.owner != *store.owner_binding() {
                        return Err(
                            "provider original material refs belong to a different owner binding"
                                .into(),
                        );
                    }
                    store.verify_commit_materials(
                        refs,
                        &cognition.request.request_context,
                        &cognition.response,
                    )
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            if cognition.provider_original_material_refs.is_some() {
                Err("native provider original material refs cannot be committed on wasm".into())
            } else {
                Ok(())
            }
        }
    }

    /// Close the pending owner only after the Runtime receipt has been read
    /// back and verified. The owner remains pending if this append fails.
    pub(in crate::viewer::runtime_live) fn mark_provider_original_materials_committed(
        &self,
        cognition: &RuntimeProviderActionContext,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match (
                self.provider_original_material_store.as_deref(),
                cognition.provider_original_material_refs.as_ref(),
            ) {
                (None, None) => Ok(()),
                (Some(_), None) => Err(
                    "provider original material owner is installed but committed action has no refs"
                        .into(),
                ),
                (None, Some(_)) => Err(
                    "provider original material refs have no installed pending owner".into(),
                ),
                (Some(store), Some(refs)) => {
                    if refs.owner != *store.owner_binding() {
                        return Err(
                            "provider original material refs belong to a different owner binding"
                                .into(),
                        );
                    }
                    store.mark_committed(refs, receipt_id, action_id)
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (receipt_id, action_id);
            if cognition.provider_original_material_refs.is_some() {
                Err("native provider original material refs cannot be committed on wasm".into())
            } else {
                Ok(())
            }
        }
    }

    /// Metadata-only fence for a read-back receipt, before projection,
    /// settlement or feedback. This never changes captured to committed.
    pub(in crate::viewer::runtime_live) fn verify_provider_original_disposition(
        &self,
        cognition: &RuntimeProviderActionContext,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match (
                self.provider_original_material_store.as_deref(),
                cognition.provider_original_material_refs.as_ref(),
            ) {
                (None, None) => Ok(()),
                (Some(store), Some(refs)) if refs.owner == *store.owner_binding() => {
                    store.verify_commit_disposition(refs, receipt_id, action_id)
                }
                _ => Err("provider disposition has missing or mismatched original owner".into()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (receipt_id, action_id);
            if cognition.provider_original_material_refs.is_some() {
                Err("native provider original material refs cannot be committed on wasm".into())
            } else {
                Ok(())
            }
        }
    }
}
