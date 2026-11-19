//! Real pre-Rule economy-module resolution. This private source binds the
//! original selected queue envelope and actual Current observations; request
//! DATA cannot select a worker or grant an Accepted/action receipt.
use super::*;
use oasis7_wasm_abi::{FactoryBuildRequest, ProductValidationRequest, RecipeExecutionRequest};
use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::world) enum FactoryModuleCallKindV1 {
    Build,
    Recipe,
    Product,
}
impl FactoryModuleCallKindV1 {
    pub(in crate::world) fn label(self) -> &'static str {
        match self {
            Self::Build => "factory",
            Self::Recipe => "recipe",
            Self::Product => "product",
        }
    }
}
pub(in crate::world) enum FactoryModuleRequestV1 {
    Build(FactoryBuildRequest),
    Recipe(RecipeExecutionRequest),
    Product(ProductValidationRequest),
}
impl Serialize for FactoryModuleRequestV1 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Build(v) => v.serialize(s),
            Self::Recipe(v) => v.serialize(s),
            Self::Product(v) => {
                crate::world::economy::ProductValidationModuleCallRequest::from_request(v)
                    .serialize(s)
            }
        }
    }
}
pub(in crate::world) struct PreparedFactoryModuleCallV1<'s> {
    selected: &'s SelectedPendingActionV1,
    kind: FactoryModuleCallKindV1,
    module_id: &'s str,
    request: ReservedValue<FactoryModuleRequestV1>,
    trace: ReservedValue<String>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _source_fees: current::SourceFeesV1,
    _layout: MemoryReservation,
}
impl PreparedFactoryModuleCallV1<'_> {
    pub(in crate::world) fn kind(&self) -> FactoryModuleCallKindV1 {
        self.kind
    }
    pub(in crate::world) fn module_id(&self) -> &str {
        self.module_id
    }
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.selected.envelope().id
    }
    pub(in crate::world) fn trace_id(&self) -> &str {
        &self.trace
    }
    pub(in crate::world) fn request(&self) -> &FactoryModuleRequestV1 {
        &self.request
    }
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.selected
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        self.validate_evidence_identity(tx, common, now, live)
    }
    pub(in crate::world) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.selected.validate(tx)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        charge(common, self.module_id.len() + self.trace.len())?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self.memory.same_pool(common.memory())
            || !self.request.uses_budget(common.memory())
            || !self.trace.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        let (kind, module) = match &self.selected.envelope().action {
            crate::Action::BuildFactoryWithModule { module_id, .. } => {
                (FactoryModuleCallKindV1::Build, module_id.as_str())
            }
            crate::Action::ScheduleRecipeWithModule { module_id, .. } => {
                (FactoryModuleCallKindV1::Recipe, module_id.as_str())
            }
            crate::Action::ValidateProductWithModule { module_id, .. } => {
                (FactoryModuleCallKindV1::Product, module_id.as_str())
            }
            _ => return Err(invalid()),
        };
        if kind != self.kind
            || module != self.module_id
            || !matches!(
                (&*self.request, self.kind),
                (
                    FactoryModuleRequestV1::Build(_),
                    FactoryModuleCallKindV1::Build
                ) | (
                    FactoryModuleRequestV1::Recipe(_),
                    FactoryModuleCallKindV1::Recipe
                ) | (
                    FactoryModuleRequestV1::Product(_),
                    FactoryModuleCallKindV1::Product
                )
            )
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// A pre-VM business rejection has its actual selected/fact provenance and
/// paid notes. It is consumed by the fixed no-Rule rejection driver.
pub(in crate::world) struct RejectedFactoryModuleActionV1<'s> {
    selected: &'s SelectedPendingActionV1,
    reason: crate::RejectReason,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    _fees: current::SourceFeesV1,
    _layout: MemoryReservation,
}
pub(in crate::world) enum PreparedFactoryModuleActionV1<'s> {
    Call(PreparedFactoryModuleCallV1<'s>),
    Rejected(RejectedFactoryModuleActionV1<'s>),
}
impl RejectedFactoryModuleActionV1<'_> {
    pub(in crate::world) fn reason(&self) -> &crate::RejectReason {
        &self.reason
    }
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.selected
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.selected.validate(tx)?;
        tx.validate_guard(&self.guard, live)?;
        charge(common, 0)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self._layout.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[path = "incremental_normal_action_factory_module_product.rs"]
mod product;
#[path = "incremental_normal_action_factory_module_request.rs"]
mod request;
#[path = "incremental_normal_action_factory_module_resolve.rs"]
mod resolve;

pub(super) use product::{apply as apply_product_v1, result as product_result_v1};
pub(in crate::world) use request::prepare_factory_module_action_v1;
pub(in crate::world) use resolve::{
    DeniedFactoryModuleOutputV1, InvalidFactoryModuleOutputV1, ResolvedFactoryModuleActionV1,
    ResolvedFactoryModuleOutcomeV1, format_failure_rejection_v1,
    resolve_actual_factory_module_output_v1,
};
