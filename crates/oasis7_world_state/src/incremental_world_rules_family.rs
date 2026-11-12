//! Closed conjunction of all thirteen original WorldRules children.
//! State structure only: no authority, physical lease or complete World proof.
pub use crate::incremental_typed_record_codec::TypedRecordReadWorkV1 as WorldRulesTypedReadWorkV1;
use crate::{
    incremental_typed_record_codec::*,
    incremental_world_rule_composite_family::{
        WorldRuleCompositeQualificationV1, qualify_world_rule_branch_v1,
        readmit_world_rule_branch_ordinary_v1,
    },
    incremental_world_rule_effects::{
        WorldRuleRemainderQualificationV1, WorldRuleRemainderWorkV1,
        qualify_world_rule_remainder_v1, readmit_world_rule_remainder_ordinary_v1,
    },
    incremental_world_rule_organization::root::{
        OrganizationFamilyQualificationV1, qualify_organization_family_v1,
        readmit_organization_family_ordinary_v1,
    },
    incremental_world_rule_supply_root::{
        SupplyFamilyQualificationV1, qualify_supply_family_v1, readmit_supply_family_ordinary_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_world_rules_router.rs"]
mod router;
pub struct WorldRulesFamilyWorkV1 {
    pub outer_records_remaining: u64,
    pub legacy_cells_remaining: u64,
    pub max_semantic_bytes: u64,
    pub max_record_bytes: u64,
    pub composite: WorldRulesTypedReadWorkV1,
    pub remainder: WorldRuleRemainderWorkV1,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete WorldRules13 family")
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
macro_rules! branches {
    ($($slot:ident : $ty:ty),+ $(,)?) => {
        pub struct WorldRulesQualificationV1 {
            roots: [u8;32], class: MemoryClass,
            organization: ReservedValue<OrganizationFamilyQualificationV1>,
            supply: ReservedValue<SupplyFamilyQualificationV1>,
            remainder: ReservedValue<WorldRuleRemainderQualificationV1>,
            $($slot: ReservedValue<WorldRuleCompositeQualificationV1<$ty>>),+
        }
        impl WorldRulesQualificationV1 {
            pub fn allocation_class(&self) -> MemoryClass { self.class }
            pub fn rules_present(&self) -> bool { self.remainder.rules_present() }
            pub fn child_count(&self) -> usize { 13 }
            /// Borrow the already-qualified accepted child; no re-scan or new authority.
            pub fn continuity(&self) -> &ReservedValue<WorldRuleCompositeQualificationV1<crate::world_rules::OrganizationContinuityStateV1>> { &self.continuity }
            pub fn organization(&self) -> &ReservedValue<OrganizationFamilyQualificationV1> { &self.organization }
            pub fn orders(&self) -> &ReservedValue<WorldRuleCompositeQualificationV1<std::collections::BTreeMap<String,crate::world_rules::AgentIndustrialOrderV1>>> { &self.orders }

            pub fn validate_binding(&self, roots:&RootSetV1) -> Result<(),ValueTreeError> {
                if self.roots != roots.digest().map_err(|_|invalid())? { return Err(invalid()); }
                Ok(())
            }
        }
        const CARRIER: usize = std::mem::size_of::<WorldRulesQualificationV1>() + 512;
        /// All scans and typed/legacy allowances are supplied once and consumed
        /// continuously through the router and every fixed original child.
        pub fn qualify_world_rules_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(
            roots:&RootSetV1, ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,
            outer_scan:&mut StateScanBudget, inner_scan:&mut StateScanBudget,
            work:&mut WorldRulesFamilyWorkV1,
        ) -> Result<ReservedValue<WorldRulesQualificationV1>,ValueTreeError> {
            check(ctx)?;
            roots.validate().map_err(|_|invalid())?;
            if ctx.root != roots.state_root(StateNamespaceV1::Authorization) { return Err(invalid()); }
            let retained=ctx.values.memory.reserve(CARRIER,ctx.values.class)?;
            router::qualify(roots,ctx,outer_scan,work)?;
            let organization=qualify_organization_family_v1(roots,ctx,outer_scan,inner_scan,&mut work.legacy_cells_remaining,work.max_record_bytes)?;
            let supply=qualify_supply_family_v1(roots,ctx,outer_scan,inner_scan,&mut work.legacy_cells_remaining,work.max_record_bytes)?;
            let remainder=qualify_world_rule_remainder_v1(roots,ctx,outer_scan,&mut work.remainder)?;
            $(let $slot=qualify_world_rule_branch_v1::<$ty,_,_,_,_>(roots,ctx,inner_scan,&mut work.composite)?;)+
            let present=remainder.rules_present();
            if organization.rules_present()!=present || supply.rules_present()!=present $(|| $slot.parent_present()!=present)+ { return Err(invalid()); }
            check(ctx)?;
            let result=retained.retain(WorldRulesQualificationV1 { roots:roots.digest().map_err(|_|invalid())?,class:ctx.values.class,organization,supply,remainder,$($slot),+ });
            check(ctx)?;
            Ok(result)
        }
        /// Verify the generic transition once, then check every original child
        /// against that same immutable receipt and the caller's shared budgets.
        pub fn accept_world_rules_transition_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(
            parent:&ReservedValue<WorldRulesQualificationV1>,
            verifier:&oasis7_formal_protocol::incremental_execution::TransitionVerifier<'_,P>,
            record:&oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
            ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,
            scan:&mut StateScanBudget, work:&mut WorldRulesFamilyWorkV1,
        ) -> Result<ReservedValue<WorldRulesQualificationV1>,crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
            let result=(|| {
                check(ctx)?;
                if verifier.class!=MemoryClass::Ordinary || !verifier.memory.same_pool(ctx.values.memory) { return Err(invalid().into()); }
                { let _phase=transition_preflight(parent,record,ctx)?; }
                let witness=crate::incremental_verified_world_transition::verify_world_transition_v1(verifier,record,ctx)?;
                accept_world_rules_verified_v1(parent,&witness,ctx,scan,work)
            })();
            check(ctx)?;
            result
        }
        pub(crate) fn accept_world_rules_verified_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(
            parent:&ReservedValue<WorldRulesQualificationV1>,
            witness:&crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
            ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,
            scan:&mut StateScanBudget,work:&mut WorldRulesFamilyWorkV1,
        ) -> Result<ReservedValue<WorldRulesQualificationV1>,crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
            let result=(|| {
                check(ctx)?;
                witness.check_context(ctx.values.memory,ctx.values.class,ctx.root)?;
                let _phase=transition_preflight(parent,witness.record(),ctx)?;
                let retained=ctx.values.memory.reserve(CARRIER,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
                router::transition(witness.record(),ctx,work)?;
                let organization=crate::incremental_world_rule_organization::root::accept_organization_verified_v1(&parent.organization,witness,ctx,scan,&mut work.legacy_cells_remaining,work.max_record_bytes)?;
                let supply=crate::incremental_world_rule_supply_root::accept_supply_verified_v1(&parent.supply,witness,ctx,scan,&mut work.legacy_cells_remaining,work.max_record_bytes)?;
                let remainder=crate::incremental_world_rule_effects::accept_world_rule_remainder_verified_v1(&parent.remainder,witness,ctx,&mut work.remainder)?;
                $(let $slot=crate::incremental_world_rule_composite_family::accept_world_rule_branch_verified_v1::<$ty,_,_,_,_>(&parent.$slot,witness,ctx,scan,&mut work.composite)?;)+
                let present=remainder.rules_present();
                if organization.rules_present()!=present || supply.rules_present()!=present $(|| $slot.parent_present()!=present)+ { return Err(invalid().into()); }
                check(ctx)?;
                Ok(retained.retain(WorldRulesQualificationV1 { roots:*witness.after_digest(),class:MemoryClass::Ordinary,organization,supply,remainder,$($slot),+ }))
            })();
            check(ctx)?;
            result
        }
        fn transition_preflight<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(
            parent:&ReservedValue<WorldRulesQualificationV1>,record:&oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
            ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,
        ) -> Result<oasis7_proto::memory_budget::MemoryReservation,crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
            check(ctx)?;
            if ctx.values.class!=MemoryClass::Ordinary || parent.class!=MemoryClass::Ordinary || !parent.uses_budget(ctx.values.memory) || parent.reserved_bytes()<CARRIER || ctx.root!=record.after.state_root(StateNamespaceV1::Authorization) { return Err(invalid().into()); }
            let phase=ctx.values.memory.reserve(128<<10,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            parent.validate_binding(&record.before)?;
            check(ctx)?;
            Ok(phase)
        }
        pub fn readmit_world_rules_ordinary_v1(
            source:&ReservedValue<WorldRulesQualificationV1>, roots:&RootSetV1,
            memory:&MemoryBudget, mut cancelled:impl FnMut()->bool,
        ) -> Result<ReservedValue<WorldRulesQualificationV1>,ValueTreeError> {
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            if source.class!=MemoryClass::Recovery || !source.uses_budget(memory) || source.reserved_bytes()<CARRIER { return Err(invalid()); }
            let retained=memory.reserve(CARRIER,MemoryClass::Ordinary)?;
            let _phase=memory.reserve(128<<10,MemoryClass::Ordinary)?;
            source.validate_binding(roots)?;
            let organization=readmit_organization_family_ordinary_v1(&source.organization,memory)?;
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            let supply=readmit_supply_family_ordinary_v1(&source.supply,memory)?;
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            let remainder=readmit_world_rule_remainder_ordinary_v1(&source.remainder,roots,memory,&mut cancelled)?;
            $(let $slot=readmit_world_rule_branch_ordinary_v1(&source.$slot,roots,memory,&mut cancelled)?;)+
            let output=retained.retain(WorldRulesQualificationV1 { roots:source.roots,class:MemoryClass::Ordinary,organization,supply,remainder,$($slot),+ });
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            Ok(output)
        }
    }
}
branches! {
    requests:std::collections::BTreeMap<String,crate::world_rules::WorldRuleReceiptV1>,
    orders:std::collections::BTreeMap<String,crate::world_rules::AgentIndustrialOrderV1>,
    continuity:crate::world_rules::OrganizationContinuityStateV1,
    emergency:crate::world_rules::EmergencySupplyStateV1,
    industrial:oasis7_agent_api::world_action::IndustrialWorkStateV1,
    substitution:oasis7_agent_api::world_action::SubstitutionStateV1,
}

#[cfg(test)]
#[path = "incremental_world_rules_family_tests.rs"]
mod tests;
