//! Private two-phase dispatch capsule. No serde, authority or network access.
use super::super::agent_service_phase::HostedServicePhase;
use super::*;
use crate::viewer::ViewerRuntimeLiveServer;

#[derive(Clone, PartialEq, Eq)]
pub(in crate::viewer::runtime_live) struct HostedIdentityReservation {
    pub(in crate::viewer::runtime_live) generation: u64,
    pub(in crate::viewer::runtime_live) config_digest: String,
}
pub(in crate::viewer::runtime_live) struct PreparedHostedIdentity {
    reservation: HostedIdentityReservation,
    phase: HostedServicePhase,
    client: RemoteWorldServiceClient,
    operation: AgentServiceIoOperation,
}
pub(in crate::viewer::runtime_live) struct ReadyHostedIdentity {
    reservation: HostedIdentityReservation,
    expected_phase: HostedServicePhase,
    job: AgentServiceIoJob,
}
impl PreparedHostedIdentity {
    pub(in crate::viewer::runtime_live) fn new(
        generation: u64,
        config_digest: String,
        phase: HostedServicePhase,
        client: RemoteWorldServiceClient,
        operation: AgentServiceIoOperation,
    ) -> Self {
        Self {
            reservation: HostedIdentityReservation {
                generation,
                config_digest,
            },
            phase,
            client,
            operation,
        }
    }
    pub(in crate::viewer::runtime_live) fn reservation(&self) -> HostedIdentityReservation {
        self.reservation.clone()
    }
    pub(in crate::viewer::runtime_live) fn reserve_submit(phase: Option<&mut HostedServicePhase>) {
        match phase {
            Some(HostedServicePhase::Act { submit, .. })
            | Some(HostedServicePhase::ActSettle { submit, .. })
            | Some(HostedServicePhase::HandoffWake { submit, .. })
            | Some(HostedServicePhase::FeedbackAck { submit, .. }) => *submit = false,
            _ => {}
        }
    }
    pub(in crate::viewer::runtime_live) fn compute(self) -> Result<ReadyHostedIdentity, String> {
        #[cfg(any(test, feature = "test_tier_required"))]
        let _phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(74);
        #[cfg(any(test, feature = "test_tier_required"))]
        crate::viewer::runtime_live::stream_budget_diagnostics::identity_barrier();
        let phase_digest = self.phase.identity()?;
        let token = AgentServiceIoToken {
            generation: self.reservation.generation,
            config_digest: self.reservation.config_digest.clone(),
            phase_digest,
        };
        Ok(ReadyHostedIdentity {
            reservation: self.reservation,
            expected_phase: self.phase,
            job: AgentServiceIoJob {
                token,
                client: Some(self.client),
                operation: self.operation,
            },
        })
    }
}
impl ViewerRuntimeLiveServer {
    /// Install a verified new-format owner for provider request/response
    /// materials before the provider runner is created. This does not derive
    /// a binding from the legacy lineage checkpoint and does not activate a
    /// legacy World as an IWS owner.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install_provider_original_material_store(
        &mut self,
        store: std::sync::Arc<dyn crate::simulator::ProviderOriginalMaterialStore>,
    ) -> Result<(), String> {
        if !matches!(
            &self.config.decision_mode,
            super::ViewerLiveDecisionMode::Llm
        ) {
            return Err("provider original material owner requires LLM decision mode".into());
        }
        if crate::viewer::runtime_live::control_plane::runtime_provider_settings_from_env()?
            .is_none()
        {
            return Err(
                "provider original material owner requires an explicit provider backend".into(),
            );
        }
        let owner = store.owner_binding();
        owner.validate().map_err(|error| error.to_string())?;
        if owner.world_id != self.config.world_id {
            return Err(
                "provider original material owner world does not match this Viewer runtime".into(),
            );
        }
        self.llm_sidecar
            .install_provider_original_material_store(store)
    }

    /// A stale capsule cannot clear a newer reservation or release any I/O.
    pub(in crate::viewer::runtime_live) fn finish_hosted_identity(
        &mut self,
        ready: ReadyHostedIdentity,
    ) -> Result<AgentServiceIoJob, String> {
        if self.llm_sidecar.hosted_identity_reservation.as_ref() != Some(&ready.reservation) {
            return Err("hosted identity reservation changed before dispatch".into());
        }
        if self.llm_sidecar.hosted_service_generation != ready.reservation.generation
            || self.llm_sidecar.hosted_service_inflight.is_some()
            || self.hosted_service_config_digest()? != ready.reservation.config_digest
            || self.llm_sidecar.hosted_service_phase.as_ref() != Some(&ready.expected_phase)
        {
            return Err("hosted identity state changed before dispatch".into());
        }
        self.llm_sidecar.hosted_identity_reservation = None;
        self.llm_sidecar.hosted_service_inflight = Some(ready.job.token.clone());
        Ok(ready.job)
    }
    pub(in crate::viewer::runtime_live) fn abandon_hosted_identity(
        &mut self,
        reservation: &HostedIdentityReservation,
    ) {
        if self.llm_sidecar.hosted_identity_reservation.as_ref() == Some(reservation) {
            self.llm_sidecar.hosted_identity_reservation = None;
        }
        // No lineage deletion, generation rollback or Submit-bit restoration.
    }
}
