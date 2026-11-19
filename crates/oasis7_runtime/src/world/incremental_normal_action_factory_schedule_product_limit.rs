//! Sole exact reviewed-product stack-limit predicate, shared by original
//! World admission and the authenticated Current industrial source.
use super::*;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait ProductStackLimitFactsV1: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_result(&mut self, kind: &str) -> Result<(), Self::Error>;
    fn resolve_product_module(&mut self, kind: &str) -> Result<Option<String>, Self::Error>;
    fn active_module_hash(
        &mut self,
        module: &str,
    ) -> Result<std::borrow::Cow<'_, str>, Self::Error>;
}
pub(in crate::world) struct LegacyProductStackLimitFactsV1<'a> {
    pub world: &'a World,
}
impl sealed::Sealed for LegacyProductStackLimitFactsV1<'_> {}
impl ProductStackLimitFactsV1 for LegacyProductStackLimitFactsV1<'_> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_result(&mut self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn resolve_product_module(&mut self, kind: &str) -> Result<Option<String>, Self::Error> {
        Ok(self.world.resolve_product_module_for_stack(kind))
    }
    fn active_module_hash(
        &mut self,
        module: &str,
    ) -> Result<std::borrow::Cow<'_, str>, Self::Error> {
        Ok(std::borrow::Cow::Borrowed(
            &self.world.active_module_manifest(module)?.wasm_hash,
        ))
    }
}
pub(in crate::world) fn recipe_output_stack_limit_rejection_v1<S: ProductStackLimitFactsV1>(
    source: &mut S,
    plan: &RecipeExecutionPlan,
) -> Result<Option<RejectReason>, S::Error> {
    for (label, stacks) in [("produce", &plan.produce), ("byproduct", &plan.byproducts)] {
        for stack in stacks {
            source.charge(stack.kind.len())?;
            // Use the completion selector, including its builtin preference.
            let Some(module_id) = source.resolve_product_module(&stack.kind)? else {
                continue;
            };
            source.reserve_result(&stack.kind)?;
            let wasm_hash = source.active_module_hash(&module_id)?;
            let Some((_, _, _, limit)) = KNOWN_PRODUCT_LIMITS.iter().find(|(kind, id, hash, _)| {
                *kind == stack.kind && *id == module_id && *hash == wasm_hash.as_ref()
            }) else {
                continue;
            };
            if stack.amount > *limit {
                return Ok(Some(RejectReason::RuleDenied {
                    notes: vec![format!(
                        "recipe {label} stack limit exceeded: kind={} amount={} limit={limit}; reduce batches and request a new quote",
                        stack.kind, stack.amount,
                    )],
                }));
            }
        }
    }
    Ok(None)
}
