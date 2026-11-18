//! Fixed qualified typed row adapters. These borrow only complete, validated
//! ALL owners; every empty column is framed from its authenticated count.
use super::*;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, ExecutionTransaction};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use std::cell::RefCell;
impl NormalProjectionColumnInitializerV1<'_, '_> {
    fn qualified_descriptor_v1(
        &mut self,
        name: &str,
    ) -> Result<FieldDescriptor, BodyMutationError> {
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
        Err(invalid("qualified typed original field"))
    }
    fn qualified_begin_v1(&mut self, name: &str, count: u64) -> Result<(), BodyMutationError> {
        let f = self.qualified_descriptor_v1(name)?;
        self.begin(
            f,
            Some(usize::try_from(count).map_err(|_| invalid("qualified typed column count"))?),
        )
    }
    fn qualified_end_v1(&mut self, name: &str) -> Result<(), BodyMutationError> {
        let f = self.qualified_descriptor_v1(name)?;
        self.end(f)
    }
    fn qualified_row_v1(
        &mut self,
        name: &str,
        semantic: &[u8],
        key: &[u8],
        json: &[u8],
    ) -> Result<(), BodyMutationError> {
        let f = self.qualified_descriptor_v1(name)?;
        self.row_from_qualified_json_v1(f, semantic, key, json)
    }
    pub(in crate::world) fn capture_qualified_agent_authority_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_agent_authority_records::QualifiedAgentAuthorityProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        let counts = source.record_counts();
        for (kind, count) in
            oasis7_world_state::incremental_agent_authority_records::AgentAuthorityRecordKindV1::ALL
                .into_iter()
                .zip(counts)
        {
            self.qualified_begin_v1(kind.field_name(), count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        for kind in
            oasis7_world_state::incremental_agent_authority_records::AgentAuthorityRecordKindV1::ALL
        {
            self.qualified_end_v1(kind.field_name())?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_material_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_material_records::QualifiedMaterialProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        let counts = source.record_counts();
        for (kind, count) in
            oasis7_world_state::incremental_material_records::MaterialRecordKindV1::ALL
                .into_iter()
                .zip(counts)
        {
            self.qualified_begin_v1(kind.field_name(), count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        for kind in oasis7_world_state::incremental_material_records::MaterialRecordKindV1::ALL {
            self.qualified_end_v1(kind.field_name())?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_main_policy_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_main_token_policy_records::QualifiedMainTokenPolicyProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        let counts = source.record_counts();
        for (kind, count) in
            oasis7_world_state::incremental_main_token_policy_records::MainTokenPolicyKindV1::ALL
                .into_iter()
                .zip(counts)
        {
            self.qualified_begin_v1(kind.name(), count)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        for kind in
            oasis7_world_state::incremental_main_token_policy_records::MainTokenPolicyKindV1::ALL
        {
            self.qualified_end_v1(kind.name())?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_factory_authority_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_factory_authority_records::QualifiedFactoryAuthorityProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        let counts = source.record_counts();
        for (kind,count) in oasis7_world_state::incremental_factory_authority_records::FactoryAuthorityRecordKindV1::ALL.into_iter().zip(counts){self.qualified_begin_v1(kind.field_name(),count)?}
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        for kind in oasis7_world_state::incremental_factory_authority_records::FactoryAuthorityRecordKindV1::ALL{self.qualified_end_v1(kind.field_name())?}
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_fixed_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_fixed_economy_records::QualifiedFixedEconomyProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        for field in oasis7_world_state::incremental_fixed_economy_records::FixedEconomyFieldV1::ALL
        {
            self.qualified_begin_v1(field.name(), 1)?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        for field in oasis7_world_state::incremental_fixed_economy_records::FixedEconomyFieldV1::ALL
        {
            self.qualified_end_v1(field.name())?
        }
        Ok(())
    }
    pub(in crate::world) fn capture_qualified_module_state_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_module_state_projection_delta::QualifiedModuleStateProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        self.qualified_begin_v1("module_states", source.states())?;
        {
            let this = RefCell::new(&mut *self);
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |field, semantic, key, json| {
                    this.borrow_mut()
                        .qualified_row_v1(field, semantic, key, json)
                },
            )?;
        }
        self.qualified_end_v1("module_states")
    }
    fn qualified_persistent_map_v1(
        &mut self,
        field_name: &str,
        rows: &super::super::original_rows::OriginalProjectionRowsDataV1,
        max_json: usize,
    ) -> Result<(), BodyMutationError> {
        struct Encoded {
            semantic: ReservedValue<Vec<u8>>,
            key: ReservedValue<Vec<u8>>,
            value: ReservedValue<Vec<u8>>,
        }
        let n = rows
            .len()
            .checked_mul(size_of::<Encoded>())
            .ok_or_else(|| invalid("qualified persistent row staging"))?;
        let _phase = reserve(self.common, n)?;
        let mut staging = Vec::with_capacity(rows.len());
        if staging.capacity() != rows.len() {
            return Err(invalid("qualified persistent row capacity"));
        }
        rows.visit_data_v1(self.common, &mut |row, common| {
            let semantic = capture(common, |out| {
                std::io::Write::write_all(out, row.semantic()).map_err(paid)
            })?;
            let key = capture(common, |out| {
                std::io::Write::write_all(out, row.original_key_json()).map_err(paid)
            })?;
            // A typed first error may own fees; keep it through row staging.
            let first = RefCell::new(None);
            let value = if let Some(v) = row.governance_record_v1() {
                let meter = RefCell::new(&mut *common);
                super::super::shared_json::capture_shared_v1(
                    &v.serialization_v1(&meter, &first),
                    &meter,
                    &first,
                    max_json,
                )?
            } else if let Some(v) = row.diplomacy_record_v1() {
                let meter = RefCell::new(&mut *common);
                super::super::shared_json::capture_shared_v1(
                    &v.serialization_v1(&meter, &first),
                    &meter,
                    &first,
                    max_json,
                )?
            } else if let Some(v) = row.reward_pool_record_v1() {
                let meter = RefCell::new(&mut *common);
                super::super::shared_json::capture_shared_v1(
                    &v.serialization_v1(&meter, &first),
                    &meter,
                    &first,
                    max_json,
                )?
            } else if let Some(v) = row.original_value_json() {
                capture(common, |out| {
                    std::io::Write::write_all(out, v).map_err(paid)
                })?
            } else {
                return Err(invalid("qualified persistent original row kind"));
            };
            staging.push(Encoded {
                semantic,
                key,
                value,
            });
            Ok(())
        })?;
        self.qualified_begin_v1(field_name, rows.len() as u64)?;
        let field = self.qualified_descriptor_v1(field_name)?;
        for row in staging {
            self.append_original_encoded_row_v1(field, row.semantic, row.key, row.value)?
        }
        self.qualified_end_v1(field_name)
    }
    pub(in crate::world) fn capture_qualified_governance_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_governance_records::QualifiedGovernanceProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
        max_json: usize,
    ) -> Result<(), BodyMutationError> {
        use super::super::cold_record_maps::RecordMapsV1;
        source.validate(tx, self.common.memory(), live)?;
        let fields =
            oasis7_world_state::incremental_governance_records::GovernanceRecordKindV1::ALL
                .map(|kind| kind.field_name());
        let mut maps = super::super::cold_record_maps::QualifiedRecordMapsV1::<3>::empty_v1(
            fields,
            self.common,
        )?;
        super::super::governance_fold::fold_rows_v1(
            &mut maps,
            source.projection_data_v1(),
            tx,
            self.common,
            live,
        )?;
        for (name, count) in fields.into_iter().zip(source.record_counts()) {
            let rows = maps
                .map_data_v1(name, self.common)?
                .ok_or_else(|| invalid("qualified governance field missing"))?;
            if rows.len() as u64 != count {
                return Err(invalid("qualified governance entity count"));
            }
            self.qualified_persistent_map_v1(name, rows, max_json)?
        }
        source.validate(tx, self.common.memory(), live)
    }
    pub(in crate::world) fn capture_qualified_diplomacy_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source: &oasis7_world_state::incremental_diplomacy::QualifiedDiplomacyProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
        max_json: usize,
    ) -> Result<(), BodyMutationError> {
        use super::super::cold_record_maps::RecordMapsV1;
        source.validate(tx, self.common.memory(), live)?;
        let fields = oasis7_world_state::incremental_diplomacy::DiplomacyFieldV1::ALL
            .map(|kind| kind.name());
        let mut maps = super::super::cold_record_maps::QualifiedRecordMapsV1::<4>::empty_v1(
            fields,
            self.common,
        )?;
        super::super::diplomacy_fold::fold_rows_v1(
            &mut maps,
            source.projection_data_v1(),
            tx,
            self.common,
            live,
        )?;
        for (name, count) in fields.into_iter().zip(source.record_counts()) {
            let rows = maps
                .map_data_v1(name, self.common)?
                .ok_or_else(|| invalid("qualified diplomacy field missing"))?;
            if rows.len() as u64 != count {
                return Err(invalid("qualified diplomacy entity count"));
            }
            self.qualified_persistent_map_v1(name, rows, max_json)?
        }
        source.validate(tx, self.common.memory(), live)
    }
    pub(in crate::world) fn capture_qualified_reward_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_reward_budget_records::QualifiedRewardProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
        max_json: usize,
    ) -> Result<(), BodyMutationError> {
        use super::super::cold_record_maps::RecordMapsV1;
        use oasis7_world_state::incremental_reward_budget_records::RewardRecordKindV1 as Kind;
        source.validate(tx, self.common.memory(), live)?;
        let fields = Kind::ALL.map(|kind| kind.name());
        let mut maps = super::super::cold_record_maps::QualifiedRecordMapsV1::<2>::empty_v1(
            fields,
            self.common,
        )?;
        super::super::reward_fold::fold_rows_v1(
            &mut maps,
            source.projection_data_v1(),
            tx,
            self.common,
            live,
        )?;
        for (name, count) in fields.into_iter().zip(source.record_counts()) {
            let rows = maps
                .map_data_v1(name, self.common)?
                .ok_or_else(|| invalid("qualified reward field missing"))?;
            if rows.len() as u64 != count {
                return Err(invalid("qualified reward entity count"));
            }
            self.qualified_persistent_map_v1(name, rows, max_json)?
        }
        source.validate(tx, self.common.memory(), live)
    }
    pub(in crate::world) fn capture_qualified_controls_columns_v1<M: MerkleNodeReader>(
        &mut self,
        source:&oasis7_world_state::incremental_world_collection_controls::QualifiedCollectionControlsProjectionColumnsV1,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        source.validate(tx, self.common.memory(), live)?;
        let names = [
            "data_access_permissions",
            "module_release_role_bindings",
            "authenticated_collect_data_last_nonces",
            "product_validation_delivery_cursor",
        ];
        let counts = source.group_member_counts();
        for (i, name) in names.into_iter().enumerate() {
            self.qualified_begin_v1(
                name,
                if i < 3 {
                    counts[i][1]
                } else {
                    source.cursor_entries()
                },
            )?
        }
        {
            let this = RefCell::new(&mut *self);
            source.visit_groups_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |name, semantic, key, count, _set| {
                    let mut this = this.borrow_mut();
                    let field = this.qualified_descriptor_v1(name)?;
                    if !this.active.contains(&(field.owner, field.name)) {
                        return Err(invalid("qualified control group framing"));
                    }
                    let semantic = capture(this.common, |out| {
                        std::io::Write::write_all(out, semantic).map_err(paid)
                    })?;
                    let key_json = capture(this.common, |out| {
                        std::io::Write::write_all(out, key).map_err(paid)
                    })?;
                    let entries = usize::try_from(count)
                        .map_err(|_| invalid("qualified control group count"))?;
                    let this = &mut **this;
                    let target = this
                        .fields
                        .get_mut(&(field.owner, field.name))
                        .ok_or_else(|| invalid("qualified control group column"))?;
                    grow(&mut target.groups, &mut target._group_slots, this.common)?;
                    target.groups.push(ProjectionOriginalColumnGroupV1 {
                        semantic,
                        key_json,
                        entries,
                    });
                    Ok(())
                },
            )?;
            source.visit_with_work_v1(
                |s, b| this.borrow_mut().common.charge(s, b).map_err(paid),
                |name, semantic, key, json, _set| {
                    this.borrow_mut()
                        .qualified_row_v1(name, semantic, key, json)
                },
            )?;
        }
        for name in names {
            self.qualified_end_v1(name)?
        }
        source.validate(tx, self.common.memory(), live)
    }
    pub(in crate::world) fn capture_qualified_world_rule_columns_v1(
        &mut self,
        source:&oasis7_world_state::incremental_world_rule_projection_delta::QualifiedWorldRuleProjectionColumnsV1,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        max_json: usize,
    ) -> Result<(), BodyMutationError> {
        if self.world_rule_rows.is_some() {
            return Err(invalid("duplicate qualified WorldRules source"));
        }
        source.validate_context(roots, self.common.memory())?;
        let rows = super::super::world_rule_rows::OriginalWorldRuleRowsDataV1::from_qualified_v1(
            source,
            roots,
            self.common,
        )?;
        let field = self.qualified_descriptor_v1("world_rules")?;
        self.begin(field, Some(1))?;
        if let Some(rows) = rows {
            // End only the Common loan; retain the original first-error owner.
            let first;
            let bytes = {
                let meter = RefCell::new(&mut *self.common);
                first = RefCell::new(None);
                super::super::shared_json::capture_shared_v1(
                    &rows.serialization_v1(&meter, &first),
                    &meter,
                    &first,
                    max_json,
                )?
            };
            let semantic = self.key(&())?;
            let key = capture(self.common, |out| {
                serde_json::to_writer(out, &()).map_err(paid)
            })?;
            self.append_original_encoded_row_v1(field, semantic, key, bytes)?;
            let header = reserve(
                self.common,
                size_of::<super::super::world_rule_rows::OriginalWorldRuleRowsDataV1>()
                    + 2 * size_of::<usize>(),
            )?;
            self.keep(header)?;
            self.world_rule_rows = Some(std::sync::Arc::new(rows));
        } else {
            self.row(field, &(), &Option::<u8>::None)?;
        }
        self.end(field)
    }
    pub(in crate::world) fn capture_qualified_auxiliary_columns_v1(
        &mut self,
        source: &ProjectionColumnDataV1,
    ) -> Result<(), BodyMutationError> {
        if self.auxiliary.is_some() {
            return Err(invalid("duplicate qualified auxiliary columns"));
        }
        let memory = self.common.memory().clone();
        let after = source
            .share_data(&memory, &mut |s, b| self.common.charge(s, b))
            .map_err(|e| match e {
                ProjectionColumnErrorV1::Work(e) => paid(e),
                ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
                ProjectionColumnErrorV1::Invalid(e) => invalid(e),
            })?;
        self.auxiliary = Some(after);
        Ok(())
    }
}
