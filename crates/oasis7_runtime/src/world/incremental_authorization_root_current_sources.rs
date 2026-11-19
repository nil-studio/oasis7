//! Fixed selected-source joins. These functions return only authenticated
//! data equality; only the complete seven-source/count closure can own a cache.
use super::read::{AuthorizationRootSourceReadWorkV1, SourceError};
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueNodeReader,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_authorization_grants_family::{
        AuthorizationGrantsQualificationV1, recover_current_authorization_grant_v1,
    },
};
#[allow(clippy::too_many_arguments)]
pub(super) fn grant<M, R, E, C, V, ER, CR>(
    family: &ReservedValue<AuthorizationGrantsQualificationV1>,
    roots: &RootSetV1,
    key: &str,
    cached: &ValueRef,
    limits: AuthorizationRootCacheLimitsV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, M, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let guard = overlay.transaction.guard();
    let actual = read::paid(
        roots,
        overlay,
        parent,
        work,
        common,
        |current, parent, work| {
            recover_current_authorization_grant_v1(
                family,
                roots,
                key,
                current.transaction,
                parent,
                &mut work.json,
                work.max_selected_heap,
                &guard,
                live,
            )
            .map_err(SourceError::from)
        },
    )?
    .ok_or_else(|| SourceError::from(invalid("authorization hint grant missing in Current")))?;
    join::compare_original(cached, &*actual, limits, common, MemoryClass::Ordinary)?;
    common.consume_steps(0)?;
    Ok(())
}

macro_rules! authorization_record_join {
    ($name:ident,$kind:ident,$recover:ident $(,$json:ident)?) => {
        #[allow(clippy::too_many_arguments)]
        pub(super) fn $name<M,R,E,C,V,ER,CR>(
            family:&ReservedValue<oasis7_world_state::incremental_runtime_authorization_family::AuthorizationRecordFamiliesQualificationV1>,
            roots:&RootSetV1,key:&str,cached:&ValueRef,limits:AuthorizationRootCacheLimitsV1,
            overlay:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,M,V,ER,CR>,
            work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)
            ->Result<(),SourceError>
        where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
            V:ValueNodeReader,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool {
            let guard=overlay.transaction.guard();
            let actual=read::paid(roots,overlay,parent,work,common,|current,parent,work| {
                let body=oasis7_world_state::incremental_runtime_authorization_family::open_current_authorization_record_body_v1(
                    family,roots,oasis7_world_state::incremental_runtime_authorization_decode::AuthorizationBodyKindV1::$kind,
                    key,current.transaction,parent,&guard,live)?.ok_or_else(||SourceError::from(invalid("authorization hint selected record missing")))?;
                let actual=oasis7_world_state::incremental_runtime_authorization_decode::$recover(
                    &body,parent,&mut work.scan,$(&mut work.$json,)?work.max_selected_heap)?;
                // Closed decode is source DATA and may not advance Current;
                // nonetheless validate the actual post-read guard, never the
                // pre-read revision, before exposing the recovered owner.
                current.transaction.validate_guard(&current.transaction.guard(),live).map_err(SourceError::Execution)?;
                Ok(actual)
            })?;
            join::compare_original(cached,actual.value(),limits,common,MemoryClass::Ordinary)?;
            common.consume_steps(0)?;Ok(())
        }
    }
}
authorization_record_join!(audit, Audit, recover_authorization_audit_body_v1, json);
authorization_record_join!(
    invocation,
    Invocation,
    recover_authorization_invocation_body_v1
);

#[allow(clippy::too_many_arguments)]
pub(super) fn capability<M, R, E, C, P, V, ER, CR>(
    family:&ReservedValue<oasis7_world_state::incremental_runtime_capability_records::RuntimeCapabilityFamilyQualificationV1>,
    roots: &RootSetV1,
    key: oasis7_world_state::incremental_runtime_capability_records::RuntimeCapabilityRecordKeyV1<
        '_,
    >,
    cached: &ValueRef,
    limits: AuthorizationRootCacheLimitsV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let hint = decode::decode(cached, key.kind, limits, common, MemoryClass::Ordinary)?;
    // Reject unknown fields, aliases, omitted defaults and noncanonical scalar
    // representations before the hint is used as the expected full cell DTO.
    join::compare_hint_capability(cached, &hint, limits, common, MemoryClass::Ordinary)?;
    let guard = overlay.transaction.guard();
    let actual = read::paid(roots, overlay, parent, work, common, |current, _, work| {
        oasis7_world_state::incremental_runtime_capability_records::match_current_runtime_capability_record_v1(
            family,roots,key,hint.borrowed(),current,&mut work.capabilities,work.max_text_bytes,&guard,live)
            .map_err(SourceError::from)
    })?;
    if actual.is_none() {
        return Err(invalid("authorization hint capability record missing").into());
    }
    common.consume_steps(0)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn authority<M, R, E, C, P, V, ER, CR>(
    family: &ReservedValue<
        oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
    >,
    current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
    roots: &RootSetV1,
    cached: &ValueRef,
    limits: AuthorizationRootCacheLimitsV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let guard = overlay.transaction.guard();
    let actual = read::paid(
        roots,
        overlay,
        parent,
        work,
        common,
        |overlay, parent, work| {
            oasis7_world_state::incremental_runtime_revocation::recover_current_authority_revocation_v1(
            family,current,overlay,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
        },
    )?;
    if let Some(index) = actual.value().task_terminal_index() {
        genesis::validate_terminals(index, limits, common, MemoryClass::Ordinary)?;
    }
    join::compare_original(
        cached,
        actual.value(),
        limits,
        common,
        MemoryClass::Ordinary,
    )?;
    common.consume_steps(0)?;
    Ok(())
}

/// One-time accepted-parent qualification must precede any Authorization or
/// Effects mutation. In particular the parent Q counts must never be applied
/// to a partially updated Current family. Reads may already have advanced the
/// transaction guard and are deliberately not confused with pending writes.
pub(super) fn require_parent_authorization_unchanged<M: MerkleNodeReader>(
    tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    remaining: &mut usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), SourceError> {
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
    let common = std::cell::RefCell::new(common);
    for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Effects] {
        tx.visit_pending_writes::<SourceError>(
            ns,
            remaining,
            || {
                common.borrow_mut().consume_steps(1)?;
                Ok(())
            },
            |_, _, _| {
                Err(invalid("authorization parent cache must initialize before mutation").into())
            },
        )?;
    }
    common.borrow_mut().consume_steps(0)?;
    Ok(())
}
