//! The fixed native 24-field cohort, decoded through each original field spec.
use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1};
use crate::incremental_world_native::{
    self as native_source, WorldNativeFieldSpecV1, WorldNativeFieldsQualificationV1,
    WorldNativeKeyV1, WorldNativeRecordV1,
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;

struct NativeRow {
    field: &'static str,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    value_json: Option<ReservedValue<Vec<u8>>>,
}
/// DATA only: real net writes, original semantic keys and original typed JSON.
pub struct CurrentNativeProjectionDeltaV1 {
    rows: Vec<NativeRow>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl CurrentNativeProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self.fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit<E: From<ValueTreeError>>(
        &self,
        processing: &mut BusinessDiffProcessingWorkV1,
        mut stop: impl FnMut() -> bool,
        visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        let p = Processing { bank: std::cell::RefCell::new(processing) };
        self.visit_with_work_v1(|_, _| spend(&p, 0, &mut stop).map_err(E::from), visit)
    }
    /// The same original DATA emission with continuing caller work. Neither
    /// callback can replace a field, source guard or decode predicate.
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            visit(row.field, &row.semantic, &row.key_json, row.value_json.as_deref().map(Vec::as_slice))?;
        }
        Ok(())
    }
}
macro_rules! native_fields {
    ($m:ident $(,$arg:expr)*) => { $m! { ($($arg),*)
        "product_validation_receipts" => crate::incremental_world_native_industry::ProductValidationReceiptsFieldV1,
        "product_profiles" => crate::incremental_world_native_industry::ProductProfilesFieldV1,
        "latest_product_validation" => crate::incremental_world_native_industry::LatestProductValidationFieldV1,
        "recipe_profiles" => crate::incremental_world_native_industry::RecipeProfilesFieldV1,
        "factory_profiles" => crate::incremental_world_native_industry::FactoryProfilesFieldV1,
        "factory_construction_receipts" => crate::incremental_world_native_industry::FactoryConstructionReceiptsFieldV1,
        "product_validation_attempts" => crate::incremental_world_native_industry::ProductValidationAttemptsFieldV1,
        "factory_recycle_receipts" => crate::incremental_world_native_industry::FactoryRecycleReceiptsFieldV1,
        "industry_progress" => crate::incremental_world_native_industry::IndustryProgressFieldV1,
        "gameplay_policy" => crate::incremental_world_native_gameplay::GameplayPolicyFieldV1,
        "agent_claims" => crate::incremental_world_native_gameplay::AgentClaimsFieldV1,
        "starter_oc_claims" => crate::incremental_world_native_gameplay::StarterOcClaimsFieldV1,
        "meta_progress" => crate::incremental_world_native_gameplay::MetaProgressFieldV1,
        "module_admission_freezes" => crate::incremental_world_native_gameplay::ModuleAdmissionFreezesFieldV1,
        "module_release_requests" => crate::incremental_world_native_gameplay::ModuleReleaseRequestsFieldV1,
        "module_release_manifest_mappings" => crate::incremental_world_native_gameplay::ModuleReleaseManifestMappingsFieldV1,
        "governance_finality_signer_registry" => crate::incremental_world_native_governance::GovernanceFinalitySignerRegistryFieldV1,
        "governance_validator_admissions" => crate::incremental_world_native_governance::GovernanceValidatorAdmissionsFieldV1,
        "governance_finality_key_rotations" => crate::incremental_world_native_governance::GovernanceFinalityKeyRotationsFieldV1,
        "governance_bootstrap_finality_key_rotations" => crate::incremental_world_native_governance::GovernanceBootstrapFinalityKeyRotationsFieldV1,
        "governance_main_token_controller_registry" => crate::incremental_world_native_governance::GovernanceMainTokenControllerRegistryFieldV1,
        "main_token_node_points_bridge_records" => crate::incremental_world_native_governance::MainTokenNodePointsBridgeRecordsFieldV1,
        "main_token_treasury_distribution_records" => crate::incremental_world_native_governance::MainTokenTreasuryDistributionRecordsFieldV1,
        "restricted_starter_claim_liveops_pool_top_up_records" => crate::incremental_world_native_governance::RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1,
    } };
}
macro_rules! is_native {
    (($name:expr) $($n:literal => $f:ty,)+) => { matches!($name, $($n)|+) };
}
/// Closed dispatch metadata only; this does not authenticate a field/source.
pub fn is_native_projection_field_v1(name: &str) -> bool {
    native_fields!(is_native, name)
}
fn selected(raw: &[u8]) -> Result<bool, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let key = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    Ok(key.owner == FieldOwner::WorldState && is_native_projection_field_v1(key.name))
}
enum NativeSelection<'a>{Current(&'a Pending),Qualified(&'a StateEntryV1)}
impl NativeSelection<'_>{fn key(&self)->&[u8]{match self{Self::Current(row)=>&row.key,Self::Qualified(entry)=>&entry.key}}fn root(&self)->Result<[u8;32],ValueTreeError>{match self{Self::Current(row)=>row.after.or(row.before).ok_or_else(bad),Self::Qualified(entry)=>Ok(entry.value)}}fn present(&self)->bool{match self{Self::Current(row)=>row.after.is_some(),Self::Qualified(_)=>true}}}
fn one<F:WorldNativeFieldSpecV1,M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(row:&Pending,ns:StateNamespaceV1,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,scan:&mut StateScanBudget,work:&mut TypedRecordReadWorkV1,p:&Processing<'_>)->Result<Option<NativeRow>,ValueTreeError>where F::Value:serde::Serialize{one_selection::<F,_,_,_,_>(NativeSelection::Current(row),ns,ctx,scan,work,p)}
fn one_selection<
    F: WorldNativeFieldSpecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: NativeSelection<'_>,
    ns: StateNamespaceV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
) -> Result<Option<NativeRow>, ValueTreeError>
where
    F::Value: serde::Serialize,
{
    let parsed = WorldFieldKeyV1::from_bytes(row.key()).map_err(ValueTreeError::Invalid)?;
    let fixed = WorldFieldKeyV1::new(F::descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if fixed.as_bytes() != row.key() || parsed.namespace != ns {
        return Err(bad());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        if let NativeSelection::Qualified(entry)=&row {
            ctx.values.verify_closure(&entry.value,ctx.record_bytes,&mut ctx.cancelled)?;
            ctx.records().presence(&entry.value,&fixed,0)?;
        }
        return Ok(None);
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(bad());
    }
    let id = row.root()?;
    spend(
        p,
        row.key()
            .len()
            .checked_add(size_of::<StateEntryV1>())
            .ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    let _entry_fee = ctx.values.memory.reserve(
        row.key()
            .len()
            .checked_add(size_of::<StateEntryV1>())
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let entry = StateEntryV1 {
        namespace: ns,
        key: row.key().to_vec(),
        value: id,
    };
    let header = read_member_record_header_v1(
        &id,
        &fixed,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: ctx.record_bytes,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let semantic_length = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
    spend(
        p,
        semantic_length.checked_mul(16).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    let semantic = native_source::family::key::original::<F>(
        &entry,
        ctx,
        usize::try_from(ctx.semantic_bytes)
            .map_err(|_| limit())?
            .checked_add(512)
            .ok_or_else(limit)?,
    )?;
    let key_json = match read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Unit => json(&(), ctx.values.memory, p, &mut ctx.cancelled)?,
        SemanticKeyRefV1::Text(s) => json(&s, ctx.values.memory, p, &mut ctx.cancelled)?,
        SemanticKeyRefV1::Unsigned(n) => json(&n, ctx.values.memory, p, &mut ctx.cancelled)?,
        _ => return Err(bad()),
    };
    let value_json = if row.present() {
        let body = native_source::read::read_body_at_entry_v1::<F, _, _, _, _>(&fixed, &id, ctx)?;
        let profile = F::profile()?;
        let layout = <WorldNativeRecordV1<F::Value> as Codec>::owned_decode_layout_v1(&mut || {
            spend(p, 0, &mut ctx.cancelled)
        })?;
        if layout.missing.is_some() {
            return Err(bad());
        }
        let value = recover_typed_record_body_v1::<WorldNativeRecordV1<F::Value>, _, _, _, _>(
            &body, &profile, ctx, scan, work,
        )?;
        F::validate_record_binding(
            native_source::family::key::original_ref::<F>(&semantic)?,
            &body,
            ctx,
            scan,
            work,
        )?;
        Some(json(
            value.value().value(),
            ctx.values.memory,
            p,
            &mut ctx.cancelled,
        )?)
    } else {
        if F::Key::UNIT {
            return Err(bad());
        }
        None
    };
    Ok(Some(NativeRow {
        field: F::NAME,
        semantic,
        key_json,
        value_json,
    }))
}
macro_rules! dispatch {
    (($name:expr,$row:expr,$ns:expr,$read:expr,$scan:expr,$work:expr,$p:expr) $($n:literal => $f:ty,)+) => {
        match $name { $($n => one::<$f,_,_,_,_>($row,$ns,$read,$scan,$work,$p),)+ _=>Err(bad()) }
    };
}

/// Capture only actual net changes in the original native cohort; unchanged
/// fields and non-native families remain outside this delta, never fake None.
#[allow(clippy::too_many_arguments)]
pub fn read_current_native_projection_delta_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    pending_entries: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentNativeProjectionDeltaV1, WorldFieldOverlayErrorV1> {
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let wrapped = ProcessingState {
        source: state,
        processing: &p,
        first: std::cell::RefCell::new(None),
    };
    let memory = ctx.values.memory.clone();
    let _phase = memory
        .reserve(
            size_of::<ProcessingState<'_, '_, S>>() + size_of::<Processing<'_>>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let result = (|| {
        ctx.transaction.validate_read_source_guard(guard, live)?;
        if !parent.uses_budget(&memory)
            || !ctx.transaction.uses_budget(&memory)
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(bad().into());
        }
        if roots.world_id.len() > 256 || roots.state.iter().any(|root| root.node_root.len() != 64) {
            return Err(bad().into());
        }
        let mut bytes = roots.world_id.len().checked_add(512).ok_or_else(limit)?;
        for root in &roots.state {
            spend(&p, 0, &mut ctx.cancelled)?;
            bytes = bytes
                .checked_add(root.node_root.len() + 128)
                .ok_or_else(limit)?;
        }
        spend(
            &p,
            bytes.checked_mul(32).ok_or_else(limit)?,
            &mut ctx.cancelled,
        )?;
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut count = 0usize;
        let mut key_bytes = 0usize;
        for root in &roots.state {
            spend(&p, 0, &mut ctx.cancelled)?;
            let stop = std::cell::RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                root.namespace,
                pending_entries,
                || {
                    spend(&p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before != after && selected(key)? {
                        count = count.checked_add(1).ok_or_else(limit)?;
                        key_bytes = key_bytes.checked_add(key.len()).ok_or_else(limit)?;
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
        }
        let pending_fee = memory
            .reserve(
                count
                    .checked_mul(size_of::<(StateNamespaceV1, Pending)>())
                    .and_then(|n| n.checked_add(key_bytes))
                    .ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::new();
        pending.try_reserve_exact(count).map_err(|_| limit())?;
        if pending.capacity() > count {
            return Err(limit().into());
        }
        for root in &roots.state {
            spend(&p, 0, &mut ctx.cancelled)?;
            let stop = std::cell::RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                root.namespace,
                pending_entries,
                || {
                    spend(&p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before != after && selected(key)? {
                        if pending.len() == count {
                            return Err(bad().into());
                        }
                        pending.push((
                            root.namespace,
                            Pending {
                                key: key.to_vec(),
                                before,
                                after,
                            },
                        ));
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
        }
        if pending.len() != count {
            return Err(bad().into());
        }
        let fee = memory
            .reserve(
                count
                    .checked_mul(size_of::<NativeRow>())
                    .and_then(|n| n.checked_add(size_of::<CurrentNativeProjectionDeltaV1>()))
                    .ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        spend(
            &p,
            count
                .checked_mul(size_of::<NativeRow>())
                .ok_or_else(limit)?,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count).map_err(|_| limit())?;
        if rows.capacity() > count {
            return Err(limit().into());
        }
        for (ns, row) in &pending {
            spend(&p, row.key.len(), &mut ctx.cancelled)?;
            if ctx.transaction.read(*ns, &row.key)? != row.after {
                return Err(bad().into());
            }
            let parsed = WorldFieldKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
            let mut errors =
                || (ctx.take_state_error)().or_else(|| wrapped.first.borrow_mut().take());
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(*ns),
                state: &wrapped,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory: &memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut errors,
                cancelled: &mut ctx.cancelled,
            };
            if let Some(out) =
                native_fields!(dispatch, parsed.name, row, *ns, &mut read, scan, work, &p)?
            {
                rows.push(out);
            }
        }
        drop(pending);
        drop(pending_fee);
        Ok(CurrentNativeProjectionDeltaV1 {
            rows,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory: memory.clone(),
            fee,
        })
    })();
    let first = wrapped.first.borrow_mut().take();
    match first {
        Some(error) => Err(error.into()),
        None => result,
    }
}

#[path="incremental_world_rule_projection_delta_native_cold.rs"]mod cold;
pub use cold::{QualifiedNativeProjectionColumnsV1,read_qualified_native_projection_columns_v1};
