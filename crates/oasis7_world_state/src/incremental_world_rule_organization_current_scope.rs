//! Current organization risk: complete resident parent plus actual pending rows.
//! This is read-only metadata, not a dispatch or publication permission.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_organization_scope_index::{
        OrganizationScopeIndexBuilderV1, OrganizationScopeKindV1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::cell::RefCell;
pub struct CurrentOrganizationRiskV1 {
    active_case: bool,
    risk_frozen: bool,
    undisposed_estate: bool,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentOrganizationRiskV1 {
    pub fn active_case(&self) -> bool {
        self.active_case
    }
    pub fn risk_frozen(&self) -> bool {
        self.risk_frozen
    }
    pub fn undisposed_estate(&self) -> bool {
        self.undisposed_estate
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("Current organization risk pool").into());
        }
        Ok(())
    }
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current organization risk scope")
}
#[allow(clippy::too_many_arguments)]
pub fn observe_current_organization_risk_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    organization: &str,
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending: &mut usize,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentOrganizationRiskV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        tx.validate_guard(guard, live)?;
        read.check()?;
        if !parent.uses_budget(read.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(read.values.memory)
        {
            return Err(bad().into());
        }
        let fee = read
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| bad())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || read.root != roots.state_root(StateNamespaceV1::Authorization)
        {
            return Err(bad().into());
        }
        parent
            .scope
            .check_budget(read.values.memory, MemoryClass::Ordinary)?;
        let cancelled = RefCell::new(&mut read.cancelled);
        let mut local = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (**cancelled.borrow_mut())(),
        };
        let mut builder: Option<OrganizationScopeIndexBuilderV1> = None;
        tx.visit_pending_writes(
            StateNamespaceV1::Authorization,
            pending,
            || {
                if (**cancelled.borrow_mut())() {
                    Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let binding = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
                if binding.name != "world_rules" || binding.kind == WorldFieldRecordKindV1::Metadata
                {
                    return Ok(());
                }
                let key = WorldFieldKeyV1::new(
                    crate::incremental_world_rule_parent::descriptor()?,
                    binding.kind,
                    binding.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != raw {
                    return Err(bad().into());
                }
                let old = receiver::edge(&key, before, &mut local)?;
                let new = receiver::edge(&key, after, &mut local)?;
                if old.is_none() && new.is_none() {
                    return Ok(());
                }
                if builder.is_none() {
                    builder = Some(parent.scope.copy_for_changes(
                        local.values.memory,
                        MemoryClass::Ordinary,
                        &mut |s, b| scope::spend(scan, &mut local.cancelled, s, b),
                    )?);
                }
                let builder = builder.as_mut().ok_or_else(bad)?;
                match (old, new) {
                    (Some((owner, _, outer)), None) => {
                        if after.is_some() || matches!(owner, OrganizationRootKeyV1::Case(_)) {
                            return Err(bad().into());
                        }
                        builder.remove(OrganizationScopeKindV1::Estate, &outer, &mut |s, b| {
                            scope::spend(scan, &mut local.cancelled, s, b)
                        })?;
                    }
                    (old, Some((owner, new, outer))) => {
                        if old.as_ref().is_some_and(|(kind, _, _)| {
                            matches!(
                                (kind, owner),
                                (
                                    OrganizationRootKeyV1::Case(_),
                                    OrganizationRootKeyV1::Estate(_)
                                ) | (
                                    OrganizationRootKeyV1::Estate(_),
                                    OrganizationRootKeyV1::Case(_)
                                )
                            )
                        }) || old.is_none() && before.is_some()
                        {
                            return Err(bad().into());
                        }
                        receiver::child(
                            owner,
                            old.as_ref().map(|(_, root, _)| &**root),
                            &new,
                            &mut local,
                            scan,
                            cells,
                            max,
                        )?;
                        scope::project(
                            owner,
                            &outer,
                            binding.semantic_key.ok_or_else(bad)?,
                            &new,
                            &mut local,
                            scan,
                            cells,
                            max,
                            builder,
                        )?;
                    }
                    (None, None) => {}
                }
                Ok(())
            },
        )?;
        let index = match builder {
            Some(builder) => builder.finish()?,
            None => parent
                .scope
                .share(local.values.memory, MemoryClass::Ordinary)?,
        };
        let protection = index.observe(
            OrganizationScopeKindV1::Protection,
            organization,
            local.values.memory,
            MemoryClass::Ordinary,
            &mut |s, b| scope::spend(scan, &mut local.cancelled, s, b),
        )?;
        let estate = index.observe(
            OrganizationScopeKindV1::Estate,
            organization,
            local.values.memory,
            MemoryClass::Ordinary,
            &mut |s, b| scope::spend(scan, &mut local.cancelled, s, b),
        )?;
        drop(local);
        Ok(fee.retain(CurrentOrganizationRiskV1 {
            active_case: protection.active_case,
            risk_frozen: protection.risk_frozen,
            undisposed_estate: estate.undisposed_estate,
            guard: tx.guard(),
            memory: read.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
            .contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
