//! Actual terminal scalar/Policy/Journal projection sources. Unimplemented
//! Runtime field deltas are rejected explicitly; they never inherit stale data.
use super::*;
use crate::world::{
    incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    incremental_normal_final_sources::NormalCognitionProjectionInputsV1,
    incremental_normal_projection_commitment::{
        ActualCurrentJournalProjectionV1, capture_projection_json_data_v1,
        runtime_projection_links_v1,
    },
};
use oasis7_world_projection::current_commitment::{
    ProjectionColumnDataV1 as Column, ProjectionDataPathRefV1 as Path,
};
use oasis7_world_state::{
    incremental_field_member::WorldFieldKeyV1, incremental_projection::FieldOwner,
};
use std::cell::RefCell;

/// Temporary coverage check while the remaining real Runtime family adapters
/// are integrated. This checks actual net writes, never a caller support flag.
pub(super) fn validate_runtime_projection_coverage_v1<M: MerkleNodeReader>(
    parent: &BoundNormalTickParentV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    remaining: &mut usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let common = RefCell::new(common);
    for root in &parent.refs().roots.state {
        tx.visit_pending_writes::<BodyMutationError>(
            root.namespace,
            remaining,
            || common.borrow_mut().consume_steps(1).map_err(error),
            |key, before, after| {
                common
                    .borrow_mut()
                    .charge(1, key.len() as u64)
                    .map_err(error)?;
                if before == after || !WorldFieldKeyV1::is_field_key_v1(key) {
                    return Ok(());
                }
                let field = WorldFieldKeyV1::from_bytes(key).map_err(BodyMutationError::Invalid)?;
                if field.owner != FieldOwner::Runtime {
                    return Ok(());
                }
                let selected = runtime_projection_links_v1().any(|link| {
                    field.name == link.runtime_field
                        || field
                            .name
                            .strip_prefix(link.runtime_field)
                            .is_some_and(|rest| rest.starts_with('.'))
                });
                if !selected {
                    return Ok(());
                }
                if field.namespace != root.namespace {
                    return Err(BodyMutationError::Invalid(
                        "Runtime projection field namespace",
                    ));
                }
                let covered = [
                    "capability_revocation_state",
                    "capability_authorization_root",
                    "policies",
                    "journal",
                    "next_event_id",
                    "next_event_id_era",
                    "manifest",
                    "chain_resource_manifest",
                    "module_registry",
                    "capability_grants_v2",
                    "capability_invocation_contexts",
                    "governance_execution_policy",
                    "governance_finality_epoch_snapshots",
                    "governance_identity_penalties",
                    "governance_emergency_brake_until_tick",
                    "tick_consensus_authority_source",
                    "proposals",
                ]
                .iter()
                .any(|name| {
                    field.name == *name
                        || field
                            .name
                            .strip_prefix(name)
                            .is_some_and(|rest| rest.starts_with('.'))
                });
                if !covered {
                    return Err(BodyMutationError::Invalid(
                        "actual Runtime projection field delta not yet integrated",
                    ));
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

pub(super) fn fold_current_runtime_metadata_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    parent: &BoundNormalTickParentV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    metadata: &NormalTerminalReadMetadataV1,
    journal: &ActualCurrentJournalProjectionV1,
    cognition: &NormalCognitionProjectionInputsV1<'_>,
    authorization: &FrozenNormalAuthorizationProjectionV1,
    policy: &PaidCurrentBootstrapPolicyV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    metadata.validate_provenance(tx, common, live)?;
    journal.validate(parent, tx, common, live)?;
    cognition.validate(tx, common, live)?;
    policy.validate_current(tx, common, live)?;
    fold_metadata_after_actual_sources_v1(
        before,
        parent,
        tx,
        journal,
        authorization,
        policy,
        Some((cognition.enabled(), cognition.revision())),
        common,
        live,
    )
}
pub(super) fn fold_qualified_runtime_metadata_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    parent: &BoundNormalTickParentV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    metadata: &NormalTerminalReadMetadataV1,
    journal: &ActualCurrentJournalProjectionV1,
    authorization: &FrozenNormalAuthorizationProjectionV1,
    policy: &PaidCurrentBootstrapPolicyV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    parent.validate_context(common).map_err(error)?;
    tx.validate_bootstrap_read_only_v1(live)?;
    metadata.validate_provenance(tx, common, live)?;
    journal.validate(parent, tx, common, live)?;
    policy.validate_current(tx, common, live)?;
    let selected = parent.refs().cognition.selected().manifest();
    common.charge(2, 16).map_err(error)?;
    if selected.enabled != parent.refs().cognition.current().enabled() {
        return Err(bad());
    }
    fold_metadata_after_actual_sources_v1(before,parent,tx,journal,authorization,policy,if parent.refs().storage.current().cognition().presence()==oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Disabled{None}else{Some((selected.enabled,selected.revision))},common,live)
}
fn fold_metadata_after_actual_sources_v1<M: MerkleNodeReader>(
    before: &Column,
    parent: &BoundNormalTickParentV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    journal: &ActualCurrentJournalProjectionV1,
    authorization: &FrozenNormalAuthorizationProjectionV1,
    policy: &PaidCurrentBootstrapPolicyV1,
    header: Option<(bool, u64)>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    let hash = authorization
        .projection_root_hash_v1(parent, tx, common, live)
        .map_err(|e| error(ReadCause(e)))?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(fold::graph)?;
    macro_rules! set {
        ($path:expr,$value:expr) => {{
            let raw = capture_projection_json_data_v1($value, common)?;
            let value = fold::value(&raw, common)?;
            columns = columns
                .with_path_data_v1($path, Some(&value), &memory, &mut |s, b| {
                    common.charge(s, b)
                })
                .map_err(fold::graph)?;
        }};
    }
    set!(&[Path::Field("capability_authorization_root")], hash);
    set!(&[Path::Field("policies")], policy.value());
    set!(&[Path::Field("next_event_id")], &journal.next_event_id_v1());
    set!(
        &[Path::Field("event_id_era")],
        &journal.next_event_id_era_v1()
    );
    if let Some((enabled, revision)) = header {
        set!(
            &[Path::Field("cognition"), Path::Field("enabled")],
            &enabled
        );
        set!(
            &[Path::Field("cognition"), Path::Field("revision")],
            &revision
        );
    }
    columns = columns
        .with_path_data_v1(
            &[Path::Field("world_journal")],
            Some(journal.event_columns_v1()),
            &memory,
            &mut |s, b| common.charge(s, b),
        )
        .map_err(fold::graph)?;
    Ok(columns)
}
