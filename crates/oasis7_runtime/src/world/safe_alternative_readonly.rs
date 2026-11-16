//! Original safe-alternative scope join over sealed readonly sources. The
//! consumer chooses no predicate, identity, root, or authorization verdict.
use super::*;
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_agent_api::world_action::GeoPos;
pub(in crate::world) mod sealed {
    pub trait Sources {}
}
pub(in crate::world) trait Sources: sealed::Sources {
    type Error;
    fn work(&self, s: u64, b: usize, h: usize) -> Result<(), Self::Error>;
    fn before_value<T: serde::Serialize + ?Sized>(&self, v: &T) -> Result<(), Self::Error>;
    fn identity(&self, agent: &str) -> Result<Option<crate::CapabilityAgentIdentity>, Self::Error>;
    fn binding(&self) -> Result<Option<RuntimeBindingV1>, Self::Error>;
    fn owner(&self, agent: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, Self::Error>;
    fn instances_len(&self) -> Result<usize, Self::Error>;
    fn active_modules_len(&self) -> Result<usize, Self::Error>;
    fn owner_key(&self, agent: &str) -> Result<Option<String>, Self::Error>;
    fn position(&self, agent: &str) -> Result<Option<GeoPos>, Self::Error>;
    fn with_forbidden<T>(
        &self,
        visit: impl FnOnce(&[String]) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
}
fn refusal<S: Sources>(source: &S, note: &'static str) -> Result<Result<(), String>, S::Error> {
    source.work(1, note.len(), std::mem::size_of::<String>() + note.len())?;
    Ok(Err(note.into()))
}
pub(in crate::world) fn validate_scope_from_sources_v1<S: Sources>(
    source: &S,
    r: &Value,
    candidate: &Value,
    now: u64,
) -> Result<Result<(), String>, S::Error> {
    source.before_value(&r["binding"])?;
    let command: SafeAlternativeCommandV1 = match serde_json::from_value(r["binding"].clone()) {
        Ok(value) => value,
        Err(_) => return refusal(source, "outcome_unknown"),
    };
    let Some(identity) = source.identity(&command.agent_id)? else {
        return refusal(source, "authority_changed");
    };
    let Some(binding) = source.binding()? else {
        return refusal(source, "authority_changed");
    };
    let Some(owner) = source.owner(&command.agent_id)? else {
        return refusal(source, "authority_changed");
    };
    if source.instances_len()? > 0 || source.active_modules_len()? > 0 {
        return refusal(source, "evaluation_unavailable");
    }
    let key = source.owner_key(&command.agent_id)?;
    let position = source.position(&command.agent_id)?;
    // Admit the original fixed position JSON, comparison operands and dynamic
    // coordinate formatting before entering the unchanged original scope test.
    source.before_value(&(r,candidate,&identity,&binding,(&owner.primary_agent_id,&owner.player_id,&owner.public_key,&owner.source_id,owner.generation),&key,&position))?;
    source.with_forbidden(|forbidden| {
        let n = forbidden.iter().try_fold(0usize, |n, text| {
            source.work(1, text.len(), 0)?;
            Ok::<_, S::Error>(n.saturating_add(text.len()))
        })?;
        source.work(forbidden.len() as u64, n, 4096)?;
        Ok(
            crate::world::incremental_safe_alternative_scope::validate_scope_state_v1(
                r,
                candidate,
                &command,
                &crate::world::incremental_safe_alternative_scope::SafeAlternativeScopeStateV1 {
                    identity: &identity,
                    runtime_world: &binding.world_id,
                    runtime_branch: &binding.branch_id,
                    runtime_reorg: binding.reorg_epoch,
                    owner_binding: &owner.player_id,
                    owner_public_key: key.as_deref(),
                    position: position.as_ref(),
                    forbidden,
                },
                now,
            ),
        )
    })
}
struct Legacy<'a>(&'a World);
impl sealed::Sources for Legacy<'_> {}
impl Sources for Legacy<'_> {
    type Error = std::convert::Infallible;
    fn work(&self, _: u64, _: usize, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn before_value<T: serde::Serialize + ?Sized>(&self, _: &T) -> Result<(), Self::Error> {
        Ok(())
    }
    fn identity(&self, a: &str) -> Result<Option<crate::CapabilityAgentIdentity>, Self::Error> {
        Ok(self.0.current_agent_identity(a))
    }
    fn binding(&self) -> Result<Option<RuntimeBindingV1>, Self::Error> {
        Ok(self.0.current_cognition_runtime_binding().ok())
    }
    fn owner(&self, a: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, Self::Error> {
        Ok(self.0.current_agent_owner(a))
    }
    fn instances_len(&self) -> Result<usize, Self::Error> {
        Ok(self.0.state.module_instances.len())
    }
    fn active_modules_len(&self) -> Result<usize, Self::Error> {
        Ok(self.0.module_registry.active.len())
    }
    fn owner_key(&self, a: &str) -> Result<Option<String>, Self::Error> {
        Ok(crate::service_admission::agent_authority::owner_public_key(self.0, a).ok())
    }
    fn position(&self, a: &str) -> Result<Option<GeoPos>, Self::Error> {
        Ok(self.0.state.agents.get(a).map(|v| v.state.pos.clone()))
    }
    fn with_forbidden<T>(
        &self,
        visit: impl FnOnce(&[String]) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(&self.0.state.gameplay_policy.forbidden_location_ids)
    }
}
pub(super) fn legacy(world: &World, r: &Value, candidate: &Value, now: u64) -> Result<(), String> {
    match validate_scope_from_sources_v1(&Legacy(world), r, candidate, now) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}
