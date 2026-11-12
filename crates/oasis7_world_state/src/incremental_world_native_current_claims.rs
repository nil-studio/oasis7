//! Current claim selection using the original native-record grammar.
//! This module is a child of native qualification to keep family slots private.
use super::*;
use crate::incremental_world_native_gameplay::{AgentClaimsFieldV1, StarterOcClaimsFieldV1};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
#[allow(clippy::too_many_arguments)]
fn selected<
    F: WorldNativeFieldSpecV1<Key = String>,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<WorldNativeRecoveredRecordV1<F::Value>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < 512
            || roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        let field = F::descriptor()?;
        let profile = F::profile()?;
        let namespace = profile.namespace()?;
        if ctx.root != roots.state_root(namespace) {
            return Err(invalid().into());
        }
        let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = tx
            .read(namespace, metadata.as_bytes())?
            .ok_or_else(invalid)?;
        read::check(ctx)?;
        ctx.records().presence(&value, &metadata, 0)?;
        let (key, _) = ctx.key(field, WorldFieldRecordKindV1::Entry, id)?;
        let value = tx.read(namespace, key.as_bytes())?;
        read::check(ctx)?;
        let Some(value) = value else { return Ok(None) };
        let body = read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &value, ctx)?;
        let recovered = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
        F::validate_record_binding(WorldNativeKeyRefV1::Text(id), &body, ctx, scan, work)?;
        read::check(ctx)?;
        Ok(Some(recovered))
    })
}
macro_rules! current_claim {
    ($fn:ident,$slot:ident,$field:ty,$value:ty) => {
        #[allow(clippy::too_many_arguments)]
        pub fn $fn<
            M: MerkleNodeReader,
            P: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        >(
            id: &str,
            parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
            roots: &RootSetV1,
            tx: &mut ExecutionTransaction<'_, M>,
            ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
            scan: &mut StateScanBudget,
            work: &mut WorldNativeSelectedReadWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<Option<WorldNativeRecoveredRecordV1<$value>>, WorldFieldOverlayErrorV1> {
            tx.contribute(|tx| {
                if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
                    return Err(invalid().into());
                }
                selected::<$field, _, _, _, _, _>(
                    id,
                    &parent.$slot,
                    roots,
                    tx,
                    ctx,
                    scan,
                    work,
                    guard,
                    live,
                )
            })
        }
    };
}
current_claim!(
    recover_current_starter_oc_claim_v1,
    starter_oc_claims,
    StarterOcClaimsFieldV1,
    crate::StarterOcClaimState
);
current_claim!(
    recover_current_agent_claim_v1,
    agent_claims,
    AgentClaimsFieldV1,
    crate::AgentClaimState
);

#[path = "incremental_factory_schedule_current.rs"]
mod factory_schedule;
pub use factory_schedule::{
    recover_current_schedule_factory_profile_v1, recover_current_schedule_product_profile_v1,
    recover_current_schedule_recipe_profile_v1,
};
