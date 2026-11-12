//! Sole production-queue ordering kernel. Facts are observations, never a
//! completion/receipt permit; only the fixed due committer can publish events.
use super::*;

pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait DuePriorityFactsV1: sealed::Sealed {
    type Error;
    fn reserve_tokens(&mut self, bytes: usize, slots: usize) -> Result<(), Self::Error>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn electricity_critical(&mut self) -> Result<bool, Self::Error>;
    fn hardware_critical(&mut self) -> Result<bool, Self::Error>;
    fn product_role_priority(
        &mut self,
        kind: &str,
    ) -> Result<Option<ProductionQueuePriority>, Self::Error>;
    fn bottleneck_pressure(&mut self, tags: &[String]) -> Result<bool, Self::Error>;
}
pub(in crate::world) struct LegacyDuePriorityFactsV1<'a> {
    pub world: &'a World,
}
impl sealed::Sealed for LegacyDuePriorityFactsV1<'_> {}
impl DuePriorityFactsV1 for LegacyDuePriorityFactsV1<'_> {
    type Error = std::convert::Infallible;
    fn reserve_tokens(&mut self, _: usize, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn electricity_critical(&mut self) -> Result<bool, Self::Error> {
        Ok(self.world.is_electricity_critical())
    }
    fn hardware_critical(&mut self) -> Result<bool, Self::Error> {
        Ok(self.world.is_hardware_parts_critical())
    }
    fn product_role_priority(
        &mut self,
        kind: &str,
    ) -> Result<Option<ProductionQueuePriority>, Self::Error> {
        Ok(self
            .world
            .product_profile(kind)
            .and_then(|profile| production_priority_from_role_tag(profile.role_tag.as_str())))
    }
    fn bottleneck_pressure(&mut self, tags: &[String]) -> Result<bool, Self::Error> {
        Ok(self.world.is_bottleneck_pressure_active(tags))
    }
}
fn comparisons<S: DuePriorityFactsV1>(source: &mut S, tokens: &[String]) -> Result<(), S::Error> {
    // Each original keyword test can inspect every actual token. Charge each
    // fixed pass separately, rather than hiding CPU behind one boolean fact.
    for _ in 0..SURVIVAL_KEYWORDS.len()
        + 1
        + ENERGY_KEYWORDS.len()
        + SCALE_KEYWORDS.len()
        + EXPANSION_KEYWORDS.len()
    {
        for token in tokens {
            source.charge(token.len())?;
        }
    }
    Ok(())
}
pub(in crate::world) fn factory_priority_with_facts_v1<S: DuePriorityFactsV1>(
    source: &mut S,
    job: &crate::FactoryBuildJobState,
) -> Result<ProductionQueuePriority, S::Error> {
    let mut payload = job
        .spec
        .factory_id
        .len()
        .saturating_add(job.spec.display_name.len());
    for tag in &job.spec.tags {
        payload = payload.saturating_add(tag.len());
    }
    source.reserve_tokens(payload, job.spec.tags.len().saturating_add(2))?;
    source.charge(job.spec.factory_id.len())?;
    source.charge(job.spec.display_name.len())?;
    let mut tokens = vec![
        job.spec.factory_id.to_ascii_lowercase(),
        job.spec.display_name.to_ascii_lowercase(),
    ];
    for tag in &job.spec.tags {
        source.charge(tag.len())?;
        tokens.push(tag.to_ascii_lowercase());
    }
    comparisons(source, &tokens)?;
    Ok(production_priority_from_factory_tokens_v1(
        &tokens,
        source.electricity_critical()?,
        source.hardware_critical()?,
    ))
}
pub(in crate::world) fn recipe_priority_with_facts_v1<S: DuePriorityFactsV1>(
    source: &mut S,
    job: &crate::RecipeJobState,
) -> Result<ProductionQueuePriority, S::Error> {
    let mut payload = job.recipe_id.len();
    for stack in job.produce.iter().chain(&job.byproducts) {
        payload = payload.saturating_add(stack.kind.len());
    }
    source.reserve_tokens(
        payload,
        job.produce
            .len()
            .saturating_add(job.byproducts.len())
            .saturating_add(1),
    )?;
    source.charge(job.recipe_id.len())?;
    let mut tokens = vec![job.recipe_id.to_ascii_lowercase()];
    for stack in job.produce.iter().chain(&job.byproducts) {
        source.charge(stack.kind.len())?;
        tokens.push(stack.kind.to_ascii_lowercase());
    }
    let mut role = None;
    for stack in job.produce.iter().chain(&job.byproducts) {
        source.charge(stack.kind.len())?;
        if let Some(next) = source.product_role_priority(&stack.kind)? {
            role = Some(role.map_or(next, |prior: ProductionQueuePriority| prior.min(next)));
        }
    }
    let baseline = match role {
        Some(value) => value,
        None => {
            comparisons(source, &tokens)?;
            production_priority_from_tokens(
                &tokens,
                source.electricity_critical()?,
                source.hardware_critical()?,
                ProductionQueuePriority::Scale,
            )
        }
    };
    if source.bottleneck_pressure(&job.bottleneck_tags)? {
        return Ok(bump_recipe_priority_under_bottleneck(baseline));
    }
    Ok(baseline)
}
pub(in crate::world) fn priority_from_actual_role_tag_v1(
    role: &str,
) -> Option<ProductionQueuePriority> {
    production_priority_from_role_tag(role)
}
pub(in crate::world) fn actual_bottleneck_threshold_v1(tag: &str) -> i64 {
    bottleneck_low_stock_threshold(tag)
}
