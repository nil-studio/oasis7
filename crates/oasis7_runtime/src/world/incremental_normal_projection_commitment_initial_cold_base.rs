//! Fixed complete original columns from the actual accepted ALL source owners.
//! No World, WorldState, arbitrary root tuple or pending mutation is rebuilt.
use super::*;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, RootSetV1,
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use oasis7_world_state::{
    incremental_agent_instance_projection_delta::QualifiedAgentInstanceProjectionColumnsV1,
    incremental_industry_projection_delta::QualifiedIndustryProjectionColumnsV1,
    incremental_inventory_projection_delta::QualifiedInventoryProjectionColumnsV1,
    incremental_owner_closure::QualifiedPrimitiveProjectionColumnsV1,
    incremental_world_rule_projection_delta::QualifiedNativeProjectionColumnsV1,
};
use std::cell::RefCell;
const INDUSTRY_FIELDS: [&str; 9] = [
    "factories",
    "pending_factory_builds",
    "pending_recipe_jobs",
    "pending_material_transits",
    "recipe_completion_receipts",
    "factory_production_failure_dispositions",
    "logistics_settlement_receipts",
    "logistics_routes",
    "completed_logistics_paths",
];
fn overlay(
    error: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    match error {
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
            e.into()
        }
        e => paid(e),
    }
}
impl NormalProjectionColumnInitializerV1<'_, '_> {
    fn cold_field_v1(&mut self, name: &str) -> Result<FieldDescriptor, BodyMutationError> {
        for field in oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS
            .iter()
            .copied()
        {
            self.common
                .charge(1, (field.name.len() + name.len()) as u64)
                .map_err(paid)?;
            if field.owner == FieldOwner::WorldState && field.name == name {
                return Ok(field);
            }
        }
        Err(invalid("qualified original State field"))
    }
    fn cold_begin_v1(&mut self, name: &str, count: u64) -> Result<(), BodyMutationError> {
        let field = self.cold_field_v1(name)?;
        self.begin(
            field,
            Some(usize::try_from(count).map_err(|_| invalid("qualified column count"))?),
        )
    }
    fn cold_row_v1(
        &mut self,
        name: &str,
        semantic: &[u8],
        key: &[u8],
        json: &[u8],
    ) -> Result<(), BodyMutationError> {
        let field = self.cold_field_v1(name)?;
        self.row_from_qualified_json_v1(field, semantic, key, json)
    }
    fn cold_end_v1(&mut self, name: &str) -> Result<(), BodyMutationError> {
        let field = self.cold_field_v1(name)?;
        self.end(field)
    }
    pub(in crate::world) fn capture_qualified_industry_columns_v1(
        &mut self,
        source: &QualifiedIndustryProjectionColumnsV1,
        roots: &RootSetV1,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(roots, self.common.memory())?;
        let counts = source.record_counts();
        for (name, count) in INDUSTRY_FIELDS.into_iter().zip(counts) {
            self.cold_begin_v1(name, count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
                |path, semantic, key, json| {
                    let [name] = path else {
                        return Err(invalid("qualified industry top path"));
                    };
                    this.borrow_mut().cold_row_v1(name, semantic, key, json)
                },
            )?;
        }
        for name in INDUSTRY_FIELDS {
            self.cold_end_v1(name)?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_native_columns_v1(
        &mut self,
        source: &QualifiedNativeProjectionColumnsV1,
        roots: &RootSetV1,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(roots, self.common.memory())?;
        let counts = source.record_counts();
        for (name, count) in counts {
            self.cold_begin_v1(name, count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
                |name, semantic, key, json| {
                    this.borrow_mut().cold_row_v1(name, semantic, key, json)
                },
            )?;
        }
        for (name, _) in counts {
            self.cold_end_v1(name)?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_primitive_columns_v1(
        &mut self,
        source: &QualifiedPrimitiveProjectionColumnsV1,
        roots: &RootSetV1,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(roots, self.common.memory())?;
        let this = RefCell::new(&mut *self);
        source.visit_field_counts_with_work_v1(
            &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
            |field, count| this.borrow_mut().cold_begin_v1(field.name, count),
        )?;
        source.visit_with_work_v1(
            &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
            |name, semantic, key, json| this.borrow_mut().cold_row_v1(name, semantic, key, json),
        )?;
        source.visit_field_counts_with_work_v1(
            &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
            |field, _| this.borrow_mut().cold_end_v1(field.name),
        )?;
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_agent_instance_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source: &QualifiedAgentInstanceProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source
            .validate(tx, self.common.memory(), live)
            .map_err(overlay)?;
        for (name, count) in ["agents", "module_instances"]
            .into_iter()
            .zip(source.record_counts())
        {
            self.cold_begin_v1(name, count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_complete_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |name, semantic, key, json, electricity, intent| {
                    let mut init = this.borrow_mut();
                    init.cold_row_v1(name, semantic, key, json)?;
                    if name == "agents" {
                        let electricity = electricity
                            .ok_or_else(|| invalid("qualified Agent electricity missing"))?;
                        let intent =
                            intent.ok_or_else(|| invalid("qualified Agent intent missing"))?;
                        let copied = capture(init.common, |output| {
                            Write::write_all(output, intent).map_err(paid)
                        })?;
                        let target = init
                            .fields
                            .get_mut(&(FieldOwner::WorldState, "agents"))
                            .and_then(|field| field.rows.last_mut())
                            .ok_or_else(|| invalid("qualified Agent row absent"))?;
                        target.agent_electricity = Some(electricity);
                        target.agent_intent = Some(copied);
                    } else if electricity.is_some() || intent.is_some() {
                        return Err(invalid("qualified Instance Agent sidecars"));
                    }
                    Ok(())
                },
            )?;
        }
        for name in ["agents", "module_instances"] {
            self.cold_end_v1(name)?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_inventory_columns_v1(
        &mut self,
        source: &QualifiedInventoryProjectionColumnsV1,
        roots: &RootSetV1,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(roots, self.common.memory())?;
        let counts = source.record_counts();
        for (name, count) in [
            ("resources", counts[0]),
            ("materials", counts[1]),
            ("material_ledgers", counts[2]),
        ] {
            self.cold_begin_v1(name, count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_group_counts_with_work_v1(
                &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
                |ledger, count| {
                    let mut init = this.borrow_mut();
                    let field = init.cold_field_v1("material_ledgers")?;
                    init.group(field, ledger, count)
                },
            )?;
            source.visit_with_work_v1(
                &mut |n| this.borrow_mut().common.charge(1, n as u64).map_err(paid),
                |path, semantic, key, json| {
                    let [name] = path else {
                        return Err(invalid("qualified Inventory top path"));
                    };
                    this.borrow_mut().cold_row_v1(name, semantic, key, json)
                },
            )?;
        }
        for name in ["resources", "materials", "material_ledgers"] {
            self.cold_end_v1(name)?
        }
        Ok(())
    }
}
