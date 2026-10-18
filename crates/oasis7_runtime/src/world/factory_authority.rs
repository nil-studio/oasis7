use super::super::{
    AgentLocationAuthorityV1, DomainEvent, FactoryConstructionPowerProfileV1,
    FactorySiteAuthorityV1, LocationAnchorV1, WorldError, WorldEventBody,
};
use super::World;

impl World {
    /// Register an exact runtime location identity through the trusted
    /// bootstrap/authority surface. This is deliberately not a player auth
    /// protocol; gameplay actions can only consume the resulting registry.
    pub fn set_location_anchor(&mut self, anchor: LocationAnchorV1) -> Result<(), WorldError> {
        self.append_event(
            WorldEventBody::Domain(DomainEvent::LocationAnchorUpdated { anchor }),
            None,
        )?;
        Ok(())
    }

    pub fn register_location_anchor(&mut self, anchor: LocationAnchorV1) -> Result<(), WorldError> {
        self.set_location_anchor(anchor)
    }

    /// Apply a trusted bootstrap/test authority input through the journal.
    /// The resulting event is the replay source; gameplay callers cannot
    /// mutate the authority registries without going through this path.
    pub fn set_agent_location_authority(
        &mut self,
        authority: AgentLocationAuthorityV1,
    ) -> Result<(), WorldError> {
        self.append_event(
            WorldEventBody::Domain(DomainEvent::AgentLocationAuthorityUpdated { authority }),
            None,
        )?;
        Ok(())
    }

    pub fn register_agent_location_authority(
        &mut self,
        authority: AgentLocationAuthorityV1,
    ) -> Result<(), WorldError> {
        self.set_agent_location_authority(authority)
    }

    pub fn set_agent_location_assignment(
        &mut self,
        authority: AgentLocationAuthorityV1,
    ) -> Result<(), WorldError> {
        self.set_agent_location_authority(authority)
    }

    /// Apply a site registration/access/readiness update through a replayable
    /// runtime event. The state reducer owns revision and normalization checks.
    pub fn set_factory_site_authority(
        &mut self,
        authority: FactorySiteAuthorityV1,
    ) -> Result<(), WorldError> {
        let mut authority = authority;
        authority.authorized_agent_ids.sort();
        authority.authorized_agent_ids.dedup();
        self.append_event(
            WorldEventBody::Domain(DomainEvent::FactorySiteAuthorityUpdated { authority }),
            None,
        )?;
        Ok(())
    }

    pub fn register_factory_site_authority(
        &mut self,
        authority: FactorySiteAuthorityV1,
    ) -> Result<(), WorldError> {
        self.set_factory_site_authority(authority)
    }

    pub fn set_factory_site(
        &mut self,
        authority: FactorySiteAuthorityV1,
    ) -> Result<(), WorldError> {
        self.set_factory_site_authority(authority)
    }

    /// Apply an M4-governed construction profile through a replayable event.
    pub fn set_factory_construction_power_profile(
        &mut self,
        profile: FactoryConstructionPowerProfileV1,
    ) -> Result<(), WorldError> {
        self.append_event(
            WorldEventBody::Domain(DomainEvent::FactoryConstructionPowerProfileUpdated { profile }),
            None,
        )?;
        Ok(())
    }

    pub fn register_factory_construction_power_profile(
        &mut self,
        profile: FactoryConstructionPowerProfileV1,
    ) -> Result<(), WorldError> {
        self.set_factory_construction_power_profile(profile)
    }
}
