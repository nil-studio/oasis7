use super::*;

pub(crate) fn canonical_factory_profile_matches_spec(
    profile: &FactoryProfileV1,
    spec: &FactoryModuleSpec,
) -> Result<(), String> {
    let normalize = |tags: &[String]| {
        tags.iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect::<BTreeSet<_>>()
    };
    if profile.factory_id != spec.factory_id {
        return Err(format!(
            "modern factory build canonical profile identity mismatch: profile={} spec={}",
            profile.factory_id, spec.factory_id
        ));
    }
    if profile.tier != spec.tier {
        return Err(format!(
            "modern factory build canonical profile tier mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.tier, spec.tier
        ));
    }
    if profile.recipe_slots != spec.recipe_slots {
        return Err(format!(
            "modern factory build canonical profile recipe_slots mismatch: factory_id={} profile={} spec={}",
            spec.factory_id, profile.recipe_slots, spec.recipe_slots
        ));
    }
    if normalize(&profile.tags) != normalize(&spec.tags) {
        return Err(format!(
            "modern factory build canonical profile tags mismatch: factory_id={}",
            spec.factory_id
        ));
    }
    Ok(())
}

fn require_nonempty(value: &str, field: &str) -> Result<(), WorldError> {
    if value.trim().is_empty() {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("{field} cannot be empty"),
        });
    }
    Ok(())
}

pub(crate) fn next_revision(
    current: Option<u64>,
    incoming: u64,
    subject: &str,
) -> Result<(), WorldError> {
    let expected = match current {
        None => 1,
        Some(revision) => {
            revision
                .checked_add(1)
                .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                    reason: format!("{subject} authority revision exhausted at {revision}"),
                })?
        }
    };
    if incoming != expected {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("{subject} authority revision must be {expected}, got {incoming}"),
        });
    }
    Ok(())
}

pub(crate) fn normalize_allowlist(
    authority: &mut FactorySiteAuthorityV1,
) -> Result<(), WorldError> {
    require_nonempty(authority.site_id.as_str(), "site_id")?;
    require_nonempty(authority.location_id.as_str(), "location_id")?;
    require_nonempty(authority.owner_agent_id.as_str(), "owner_agent_id")?;
    if authority
        .authorized_agent_ids
        .iter()
        .any(|agent_id| agent_id.trim().is_empty())
    {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: "authorized_agent_ids cannot contain empty ids".to_string(),
        });
    }
    authority.authorized_agent_ids.sort();
    authority.authorized_agent_ids.dedup();
    Ok(())
}

pub(crate) fn require_active_location_anchor(
    anchors: &BTreeMap<String, LocationAnchorV1>,
    location_id: &str,
    now: WorldTime,
) -> Result<(), WorldError> {
    let Some(anchor) = anchors.get(location_id) else {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("location anchor unknown: {location_id}"),
        });
    };
    if anchor.location_id != location_id || !anchor.active || anchor.authority_revision == 0 {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "location anchor inactive_or_stale: location_id={} revision={} active={}",
                location_id, anchor.authority_revision, anchor.active
            ),
        });
    }
    if anchor.effective_at > now {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "location anchor not yet effective: location_id={} effective_at={} now={}",
                location_id, anchor.effective_at, now
            ),
        });
    }
    Ok(())
}

impl WorldState {
    pub(crate) fn active_location_anchor_revision(
        &self,
        location_id: &str,
        now: WorldTime,
    ) -> Result<u64, WorldError> {
        require_active_location_anchor(&self.location_anchors, location_id, now)?;
        Ok(self
            .location_anchors
            .get(location_id)
            .expect("active location anchor was validated")
            .authority_revision)
    }

    pub(super) fn apply_factory_construction_power_profile_updated(
        &mut self,
        profile: &FactoryConstructionPowerProfileV1,
    ) -> Result<(), WorldError> {
        require_nonempty(profile.factory_id.as_str(), "factory_id")?;
        require_nonempty(profile.factory_kind.as_str(), "factory_kind")?;
        if profile.electricity_amount < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "construction electricity amount must be >= 0, got {}",
                    profile.electricity_amount
                ),
            });
        }
        if profile
            .source_module_id
            .as_deref()
            .is_some_and(|module_id| module_id.trim().is_empty())
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "construction profile source_module_id cannot be empty".to_string(),
            });
        }
        let current = self
            .factory_construction_power_profiles
            .get(profile.factory_id.as_str())
            .map(|record| record.authority_revision);
        if let Some(existing) = self
            .factory_construction_power_profiles
            .get(profile.factory_id.as_str())
            && existing == profile
        {
            return Ok(());
        }
        next_revision(
            current,
            profile.authority_revision,
            "construction power profile",
        )?;
        self.factory_construction_power_profiles
            .insert(profile.factory_id.clone(), profile.clone());
        Ok(())
    }
}
