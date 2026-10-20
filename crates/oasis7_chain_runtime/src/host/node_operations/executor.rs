use super::super::node_lifecycle::{LocalActionRecord, actions, management};
use super::{
    model::{Kind, Record},
    provider::{self, RemoteOutcomeV1, RemoteStepV1},
    store::Store,
};

fn persist(store: &mut Store, record: Record) -> Result<Record, String> {
    let revision = record.revision;
    let mut saved = store.cas(record, revision)?;
    if saved.terminal()
        && saved.request.managed_target.is_none()
        && saved.request.kind != Kind::MigrateDataDirectory
        && let Some(action) = saved.results.last().cloned()
    {
        // The action result is durable before any network read. A slow or
        // unavailable proof endpoint cannot erase the original outcome.
        let mut result = super::result::OperationResultV1::collect(&saved)?;
        match super::super::node_lifecycle::operation_read::read_verified_operation_view(
            &saved.request.config,
            &action.target,
        ) {
            Ok(Some(observation)) => {
                if let Err(error) = result.attach_verified_world_view(&saved, &observation) {
                    result.protocol_unavailable_reason = Some(bounded_reason(&error));
                }
            }
            Ok(None) => {
                result.protocol_unavailable_reason =
                    Some("node config has no externally pinned formal public read".into());
            }
            Err(error) => result.protocol_unavailable_reason = Some(bounded_reason(&error)),
        }
        saved.operation_result = Some(result);
        let next_revision = saved.revision;
        saved = store.cas(saved, next_revision)?;
    }
    Ok(saved)
}

fn bounded_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|character| !character.is_control())
        .take(1024)
        .collect()
}
fn observe(record: &Record, step: &str) -> Result<Option<LocalActionRecord>, String> {
    let binding = actions::binding(&record.request.config)?;
    actions::query(&binding.data_dir, &record.action_id(step))
}
fn save_result(record: &mut Record, result: LocalActionRecord) {
    if let Some(old) = record
        .results
        .iter_mut()
        .find(|r| r.action_id == result.action_id)
    {
        *old = result;
    } else {
        record.results.push(result);
    }
}
/// The Store lock spans admission, external call, and readback. A second CLI
/// cannot dispatch concurrently; process death releases ownership but retains
/// the durable step identity and its original outcome for the next executor.
pub(super) fn resume(
    store: &mut Store,
    id: &str,
    expected_revision: u64,
) -> Result<Record, String> {
    let mut record = store.get(id)?;
    if record.revision != expected_revision {
        return Err("operation CAS revision changed".into());
    }
    if record.terminal() {
        return Ok(record);
    }
    if record.request.kind == Kind::RemoveCriticalReplica {
        return super::healthy_durable_set::resume(store, id, expected_revision);
    }
    if record.request.kind == Kind::ProtectedSignerMigration {
        return super::protected_handoff::resume(store, id, expected_revision);
    }
    if record.request.managed_target.is_some() {
        return resume_remote(store, record);
    }
    if record.request.kind == Kind::PackageUpgrade {
        return resume_package_upgrade(store, record);
    }
    if record.request.kind == Kind::MigrateDataDirectory {
        return resume_migration(store, record);
    }
    let step = if matches!(
        record.request.kind,
        Kind::Stop | Kind::Restart | Kind::MaintenanceStop
    ) && !record
        .results
        .iter()
        .any(|r| r.kind == "stop" && matches!(r.target.state.as_str(), "stopped" | "failed"))
    {
        "stop"
    } else {
        "start"
    };
    record.stage = if step == "stop" {
        "draining"
    } else {
        "starting"
    }
    .into();
    record.state = "reconciling".into();
    record.blocker = None;
    // Always query BEFORE considering a resend, including the first execution.
    let mut observed = match observe(&record, step) {
        Ok(result) => result,
        Err(e) => {
            record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {e}"));
            return persist(store, record);
        }
    };
    if step == "start"
        && observed
            .as_ref()
            .is_some_and(|r| r.target.state == "unknown")
    {
        observed = match actions::reconcile_queued_start(
            &record.request.config,
            &record.action_id(step),
        ) {
            Ok(result) => result,
            Err(error) => {
                record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {error}"));
                return persist(store, record);
            }
        };
    }
    if observed
        .as_ref()
        .is_some_and(|r| r.target.state == "unknown")
    {
        save_result(&mut record, observed.unwrap());
        record.blocker=Some("ORIGINAL_OUTCOME_UNKNOWN: reconcile original local receiver; identity and payload remain fixed".into());
        return persist(store, record);
    }
    if record.recovered
        && observed.is_none()
        && (management::read(&record.request.config, &record.action_id(step))?.is_some()
            || !actions::admission_absent(&record.request.config, &record.action_id(step))?)
    {
        record.state = "reconciling".into();
        record.blocker=Some("ORIGINAL_OUTCOME_UNKNOWN: recovered admission has no native receipt; query only, no resend".into());
        return persist(store, record);
    }
    let result = if let Some(result) = observed {
        result
    } else {
        record.dispatched = true;
        record.state = "running".into();
        record = persist(store, record)?;
        let context = management::ManagementRequestBinding {
            api_version: "oasis7.local-management-request/v1".into(),
            operation_id: record.request.operation_id.clone(),
            step_id: step.into(),
            action_id: record.action_id(step),
            request_digest: record.request_digest.clone(),
            request_json: serde_json::to_string(&record.request).map_err(|e| e.to_string())?,
        };
        if let Err(error) = management::bind(&record.request.config, &context) {
            record.state = "reconciling".into();
            record.blocker = Some(format!(
                "ORIGINAL_OUTCOME_UNKNOWN: management request binding failed: {error}"
            ));
            return persist(store, record);
        }
        let attempt = if step == "stop" {
            let expected = record
                .request
                .expected_instance
                .as_ref()
                .ok_or("stop target missing from admitted plan")?;
            let binding = actions::binding(&record.request.config)?;
            actions::stop(&binding.data_dir, &record.action_id(step), expected, 1000)
        } else {
            actions::start(
                &record.request.config,
                &record.action_id(step),
                record.request.expected_generation,
                &record.request.expected_config_digest,
            )
        };
        // The returned call is not stage evidence: perform original readback.
        match observe(&record, step) {
            Ok(Some(result)) => result,
            Ok(None) => {
                record.state = "blocked".into();
                record.blocker = Some(format!(
                    "LOCAL_ADMISSION_REJECTED: {}",
                    attempt
                        .err()
                        .unwrap_or("receiver receipt absent after action".into())
                ));
                return persist(store, record);
            }
            Err(e) => {
                record.state = "reconciling".into();
                record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {e}"));
                return persist(store, record);
            }
        }
    };
    let state = result.target.state.clone();
    save_result(&mut record, result);
    if state == "unknown" {
        record.state = "reconciling".into();
        record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN".into());
    } else if step == "stop" && matches!(state.as_str(), "stopped" | "failed") {
        if record.request.kind == Kind::Restart {
            record.state = "running".into();
            record.stage = "stop_verified".into();
        } else if record.request.kind == Kind::MaintenanceStop {
            record.state = "succeeded".into();
            record.stage = "maintenance_stopped".into();
        } else {
            record.state = "succeeded".into();
            record.stage = "local_process_stopped".into();
        }
    } else if step == "start" && state == "running" {
        record.state = "succeeded".into();
        record.stage = "local_process_started".into();
    } else if record.recovered && step == "start" && state == "stopped" {
        record.state = "reconciling".into();
        record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: stopped start receipt lacks durable historical start success; query only".into());
    } else if step == "start" && matches!(state.as_str(), "failed" | "stopped") {
        record.state = "failed".into();
        record.blocker = Some("LOCAL_PROCESS_EXITED: preserved exact receiver result".into());
    } else {
        record.state = "reconciling".into();
        record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: original receiver pending".into());
    }
    persist(store, record)
}
pub(super) fn cancel(
    store: &mut Store,
    id: &str,
    expected_revision: u64,
) -> Result<Record, String> {
    let mut record = store.get(id)?;
    if record.revision != expected_revision {
        return Err("operation CAS revision changed".into());
    }
    if record.terminal() {
        return Ok(record);
    }
    if record.request.kind == Kind::RemoveCriticalReplica {
        return super::healthy_durable_set::cancel(store, id, expected_revision);
    }
    if record.request.kind == Kind::ProtectedSignerMigration {
        return super::protected_handoff::cancel(store, id, expected_revision);
    }
    record.cancel_requested = true;
    record = persist(store, record)?;
    if record.request.managed_target.is_some() || record.request.kind == Kind::MigrateDataDirectory
    {
        if !record.dispatched {
            record.state = "cancelled".into();
            record.stage = "prepared_cancelled".into();
            record.blocker = None;
        } else {
            record.state = "reconciling".into();
            record.blocker = Some("CANCELLATION_AFTER_DISPATCH: query the original action; cancellation cannot retract systemd or data migration effects".into());
        }
        return persist(store, record);
    }
    if !record.dispatched {
        record.state = "cancelled".into();
        record.stage = "prepared_cancelled".into();
        record.blocker = None;
        return persist(store, record);
    }
    if record.request.kind == Kind::PackageUpgrade {
        record.state = "reconciling".into();
        record.blocker = Some("CANCELLATION_AFTER_PACKAGE_STAGE: package promotion and exact restart continue under the original immutable request".into());
        return persist(store, record);
    }
    // Native local start/termination cannot be retracted once dispatched.
    // Query the original receiver; cancellation never targets another instance
    // or claims to undo a signal, world decision or a broadcast transaction.
    let step = if record.request.kind == Kind::Start
        || (record.request.kind == Kind::Restart
            && record.results.iter().any(|r| {
                r.kind == "stop" && matches!(r.target.state.as_str(), "stopped" | "failed")
            })) {
        "start"
    } else {
        "stop"
    };
    match observe(&record, step) {
        Ok(Some(r)) => save_result(&mut record, r),
        Ok(None) => {
            if record.results.is_empty()
                && actions::admission_absent(&record.request.config, &record.action_id(step))?
            {
                record.state = "cancelled".into();
                record.stage = "no_native_admission_verified".into();
                record.blocker = None;
                return persist(store, record);
            }
        }
        Err(e) => {
            record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {e}"));
        }
    }
    record.state = "reconciling".into();
    record.blocker=Some("CANCELLATION_AFTER_LOCAL_DISPATCH: cancellation cannot retract the original effect; resume queries and completes the admitted goal".into());
    persist(store, record)
}

fn package_progress_mut(
    record: &mut Record,
) -> Result<&mut super::super::node_lifecycle::package::PackageUpgradeProgressV1, String> {
    record
        .package_upgrade_progress
        .as_mut()
        .ok_or_else(|| "package upgrade progress is missing from the admitted operation".into())
}

fn package_attempt(record: &mut Record, step: &str) -> bool {
    let action_id = record.action_id(step);
    if record
        .action_attempts
        .iter()
        .any(|attempt| attempt == &action_id)
    {
        false
    } else {
        record.action_attempts.push(action_id);
        record.dispatched = true;
        true
    }
}

fn package_reconciling(
    store: &mut Store,
    mut record: Record,
    stage: &str,
    reason: String,
) -> Result<Record, String> {
    record.state = "reconciling".into();
    record.stage = stage.into();
    record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {reason}"));
    persist(store, record)
}

fn refresh_package_transaction(
    record: &mut Record,
    helper_state: super::super::node_lifecycle::package::PackageOperationState,
) -> Result<bool, String> {
    let Some((stage, promotion, rollback)) = helper_state else {
        return Ok(false);
    };
    let progress = package_progress_mut(record)?;
    if let Some(existing) = &progress.stage_receipt
        && existing != &stage
    {
        return Err("package helper returned a different immutable stage receipt".into());
    }
    if let Some(existing) = &progress.promotion_receipt
        && Some(existing) != promotion.as_ref()
    {
        return Err("package helper returned a different immutable promotion receipt".into());
    }
    progress.stage_receipt = Some(stage);
    if let Some(promotion) = promotion {
        progress.promotion_receipt = Some(promotion);
    }
    if let Some(existing) = &progress.rollback_receipt
        && Some(existing) != rollback.as_ref()
    {
        return Err("package helper returned a different immutable rollback receipt".into());
    }
    if let Some(rollback) = rollback {
        progress.rollback_receipt = Some(rollback);
    }
    Ok(true)
}

fn resume_package_upgrade(store: &mut Store, mut record: Record) -> Result<Record, String> {
    let request = record
        .request
        .package_upgrade
        .clone()
        .ok_or("package upgrade immutable profile is missing")?;
    let expected = record
        .request
        .expected_instance
        .clone()
        .ok_or("package upgrade exact starting owner is missing")?;
    let operation_id = record.request.operation_id.clone();
    let operation_digest = record.request_digest.clone();

    // Read the helper's original durable transaction before considering any
    // retry. This is the recovery cut if the OperationRecord process died.
    let helper_state = match super::super::node_lifecycle::package::query(
        &record.request.config,
        &request,
        &operation_id,
        &operation_digest,
    ) {
        Ok(state) => state,
        Err(error) => {
            return package_reconciling(store, record, "querying_package_stage", error);
        }
    };
    match refresh_package_transaction(&mut record, helper_state.clone()) {
        Ok(true) => {
            package_attempt(&mut record, "stage");
        }
        Ok(false) => {
            record.stage = "staging_package".into();
            record.state = "running".into();
            record.blocker = None;
            package_attempt(&mut record, "stage");
            record = persist(store, record)?;
            let stage_result = super::super::node_lifecycle::package::stage(
                &record.request.config,
                &request,
                &operation_id,
                &operation_digest,
                &expected,
            );
            let readback = super::super::node_lifecycle::package::query(
                &record.request.config,
                &request,
                &operation_id,
                &operation_digest,
            );
            match readback {
                Ok(Some(transaction)) => {
                    refresh_package_transaction(&mut record, Some(transaction))?;
                }
                Ok(None) => {
                    let reason = stage_result.err().unwrap_or_else(|| {
                        "stage command returned without a durable receipt".into()
                    });
                    return package_reconciling(store, record, "package_stage_unresolved", reason);
                }
                Err(error) => {
                    return package_reconciling(
                        store,
                        record,
                        "package_stage_readback_failed",
                        error,
                    );
                }
            }
            record = persist(store, record)?;
        }
        Err(error) => {
            return package_reconciling(store, record, "package_stage_receipt_conflict", error);
        }
    }

    // Recheck the current runtime pin before any new stage or stop effect.
    // A durable promotion receipt means recovery must instead finish the
    // already-started exact transaction (including rollback on failed start).
    let promotion_already_durable = helper_state
        .as_ref()
        .is_some_and(|(_, promotion, _)| promotion.is_some());
    if !promotion_already_durable
        && let Err(error) = request.validate_current_runtime_pin(&record.request.config)
    {
        return package_reconciling(store, record, "package_runtime_pin_refused", error);
    }

    if package_progress_mut(&mut record)?.stage_receipt.is_none() {
        return package_reconciling(
            store,
            record,
            "package_stage_missing",
            "original package stage has no durable receipt".into(),
        );
    }

    let stop_step = "stop";
    let mut stop_action = match observe(&record, stop_step) {
        Ok(result) => result,
        Err(error) => {
            return package_reconciling(store, record, "querying_exact_stop", error);
        }
    };
    if stop_action.is_none() {
        record.stage = "stopping_exact_owner".into();
        record.state = "running".into();
        record.blocker = None;
        package_attempt(&mut record, stop_step);
        record = persist(store, record)?;
        let binding = match actions::binding(&record.request.config) {
            Ok(binding) => binding,
            Err(error) => {
                return package_reconciling(store, record, "stop_owner_binding_failed", error);
            }
        };
        let stop_result = actions::stop(
            &binding.data_dir,
            &record.action_id(stop_step),
            &expected,
            30_000,
        );
        stop_action = match observe(&record, stop_step) {
            Ok(Some(result)) => Some(result),
            Ok(None) => {
                let reason = stop_result.err().unwrap_or_else(|| {
                    "stop command returned without its exact native receipt".into()
                });
                return package_reconciling(store, record, "exact_stop_unresolved", reason);
            }
            Err(error) => {
                return package_reconciling(store, record, "exact_stop_readback_failed", error);
            }
        };
    } else {
        package_attempt(&mut record, stop_step);
    }
    let stop_action = stop_action.ok_or("exact stop action disappeared after query")?;
    if let Err(error) = super::recover::verify_result(&record.request, "stop", &stop_action) {
        return package_reconciling(store, record, "exact_stop_receipt_invalid", error);
    }
    save_result(&mut record, stop_action.clone());
    if stop_action.target.state == "unknown" {
        return package_reconciling(
            store,
            record,
            "exact_stop_pending",
            "original exact local stop remains unknown".into(),
        );
    }
    if !matches!(stop_action.target.state.as_str(), "stopped" | "failed")
        || stop_action.target.runtime_pid.is_some()
    {
        return package_reconciling(
            store,
            record,
            "exact_stop_not_terminal",
            "original exact owner is not stopped".into(),
        );
    }
    record = persist(store, record)?;

    let mut helper_state = match super::super::node_lifecycle::package::query(
        &record.request.config,
        &request,
        &operation_id,
        &operation_digest,
    ) {
        Ok(Some(state)) => state,
        Ok(None) => {
            return package_reconciling(
                store,
                record,
                "package_stage_disappeared",
                "durable package stage transaction disappeared before promotion".into(),
            );
        }
        Err(error) => return package_reconciling(store, record, "querying_package_stage", error),
    };
    refresh_package_transaction(&mut record, Some(helper_state.clone()))?;
    if helper_state.1.is_none() {
        record.stage = "promoting_staged_package".into();
        record.state = "running".into();
        record.blocker = None;
        package_attempt(&mut record, "promote");
        record = persist(store, record)?;
        let stage = package_progress_mut(&mut record)?
            .stage_receipt
            .clone()
            .ok_or("package stage receipt disappeared")?;
        let promotion_result = super::super::node_lifecycle::package::promote(
            &record.request.config,
            &request,
            &operation_id,
            &operation_digest,
            &expected,
            &stage,
            &stop_action,
        );
        helper_state = match super::super::node_lifecycle::package::query(
            &record.request.config,
            &request,
            &operation_id,
            &operation_digest,
        ) {
            Ok(Some(state @ (_, Some(_), _))) => state,
            Ok(Some(state)) => {
                let reason = promotion_result
                    .err()
                    .unwrap_or_else(|| "promotion returned without a promoted readback".into());
                refresh_package_transaction(&mut record, Some(state))?;
                return package_reconciling(store, record, "package_promotion_unresolved", reason);
            }
            Ok(None) => {
                let reason = promotion_result
                    .err()
                    .unwrap_or_else(|| "promotion returned without a durable receipt".into());
                return package_reconciling(store, record, "package_promotion_unresolved", reason);
            }
            Err(error) => {
                return package_reconciling(
                    store,
                    record,
                    "package_promotion_readback_failed",
                    error,
                );
            }
        };
    }
    if helper_state.1.is_none() {
        return package_reconciling(
            store,
            record,
            "package_promotion_missing",
            "package helper has no exact promotion receipt".into(),
        );
    }
    refresh_package_transaction(&mut record, Some(helper_state))?;
    package_attempt(&mut record, "promote");
    record = persist(store, record)?;

    let stage_receipt = package_progress_mut(&mut record)?
        .stage_receipt
        .clone()
        .ok_or("package stage receipt disappeared before start")?;
    let promotion_receipt = package_progress_mut(&mut record)?
        .promotion_receipt
        .clone()
        .ok_or("package promotion receipt disappeared before start")?;
    let runtime_artifact = super::super::node_lifecycle::package::verified_runtime_artifact(
        &request,
        &operation_id,
        &operation_digest,
        &stage_receipt,
        &promotion_receipt,
    )?;

    let start_action_id = record.action_id("start");
    let mut start_action = match observe(&record, "start") {
        Ok(result) => result,
        Err(error) => return package_reconciling(store, record, "querying_exact_start", error),
    };
    if start_action.is_none() {
        record.stage = "starting_promoted_package".into();
        record.state = "running".into();
        record.blocker = None;
        package_attempt(&mut record, "start");
        record = persist(store, record)?;
        let start_result = actions::start_package(
            &record.request.config,
            &start_action_id,
            record.request.expected_generation,
            promotion_receipt
                .candidate_node_config_digest
                .as_deref()
                .unwrap_or(&record.request.expected_config_digest),
            &runtime_artifact,
        );
        start_action = match observe(&record, "start") {
            Ok(Some(result)) => Some(result),
            Ok(None) => {
                let reason = start_result.err().unwrap_or_else(|| {
                    "start command returned without its exact native receipt".into()
                });
                return package_reconciling(store, record, "exact_start_unresolved", reason);
            }
            Err(error) => {
                return package_reconciling(store, record, "exact_start_readback_failed", error);
            }
        };
    } else {
        package_attempt(&mut record, "start");
    }
    let start_action = start_action.ok_or("exact start action disappeared after query")?;
    let progress = package_progress_mut(&mut record)?.clone();
    if let Err(error) =
        super::recover::verify_package_result(&record.request, "start", &start_action, &progress)
    {
        return package_reconciling(store, record, "exact_start_receipt_invalid", error);
    }
    save_result(&mut record, start_action.clone());
    if start_action.target.state != "running" {
        if matches!(start_action.target.state.as_str(), "failed" | "stopped") {
            return resume_package_rollback(
                store,
                record,
                &request,
                &expected,
                &stage_receipt,
                &promotion_receipt,
                &start_action,
            );
        }
        return package_reconciling(
            store,
            record,
            "exact_start_pending",
            "original exact start has not read back running".into(),
        );
    }
    if let Err(error) = super::super::node_lifecycle::package::verify_running_artifact(
        &start_action,
        &runtime_artifact,
    ) {
        return package_reconciling(store, record, "promoted_runtime_readback_mismatch", error);
    }
    record.state = "succeeded".into();
    record.stage = "package_promoted_and_started".into();
    record.blocker = None;
    persist(store, record)
}

fn resume_package_rollback(
    store: &mut Store,
    mut record: Record,
    request: &super::super::node_lifecycle::package::PackageUpgradeV1,
    expected: &super::super::node_lifecycle::LocalInstance,
    stage: &super::super::node_lifecycle::package::PackageStageReceiptV1,
    promotion: &super::super::node_lifecycle::package::PackagePromotionReceiptV1,
    failed_start: &super::super::node_lifecycle::LocalActionRecord,
) -> Result<Record, String> {
    let operation_id = record.request.operation_id.clone();
    let operation_digest = record.request_digest.clone();
    let rollback_receipt = package_progress_mut(&mut record)?.rollback_receipt.clone();
    if rollback_receipt.is_none() {
        package_attempt(&mut record, "rollback");
        record.state = "running".into();
        record.stage = "restoring_package_preimage".into();
        record.blocker = None;
        record = persist(store, record)?;
        let rollback = super::super::node_lifecycle::package::rollback(
            &record.request.config,
            request,
            super::super::node_lifecycle::package::RollbackContext {
                operation_id: &operation_id,
                operation_request_digest: &operation_digest,
                expected,
                stage,
                promotion,
                failed_start,
            },
        );
        let receipt = match rollback {
            Ok(receipt) => receipt,
            Err(error) => match super::super::node_lifecycle::package::query(
                &record.request.config,
                request,
                &operation_id,
                &operation_digest,
            ) {
                Ok(Some((_, _, Some(receipt)))) => receipt,
                Ok(_) => {
                    return package_reconciling(
                        store,
                        record,
                        "package_rollback_unresolved",
                        error,
                    );
                }
                Err(read_error) => {
                    return package_reconciling(
                        store,
                        record,
                        "package_rollback_readback_failed",
                        format!("{error}; receipt query failed: {read_error}"),
                    );
                }
            },
        };
        package_progress_mut(&mut record)?.rollback_receipt = Some(receipt);
        record.state = "reconciling".into();
        record.stage = "package_rollback_verified_waiting_for_start".into();
        record.blocker = Some(
            "exact prior package and metadata preimage are restored; resume to start that runtime"
                .into(),
        );
        return persist(store, record);
    }

    let rollback_receipt = rollback_receipt.ok_or("package rollback receipt disappeared")?;
    package_attempt(&mut record, "rollback");
    let rollback_artifact =
        super::super::node_lifecycle::package::verified_rollback_runtime_artifact(
            request,
            &operation_id,
            &operation_digest,
            &rollback_receipt,
        )?;
    let rollback_start_step = "rollback-start";
    let rollback_start_action_id = record.action_id(rollback_start_step);
    let mut rollback_start = match observe(&record, rollback_start_step) {
        Ok(result) => result,
        Err(error) => {
            return package_reconciling(store, record, "querying_exact_rollback_start", error);
        }
    };
    if rollback_start.is_none() {
        package_attempt(&mut record, rollback_start_step);
        record.state = "running".into();
        record.stage = "starting_restored_package".into();
        record.blocker = None;
        record = persist(store, record)?;
        let expected_generation = failed_start.target.generation;
        let start_result = actions::start_package(
            &record.request.config,
            &rollback_start_action_id,
            expected_generation,
            rollback_receipt
                .restored_node_config_digest
                .as_deref()
                .unwrap_or(&record.request.expected_config_digest),
            &rollback_artifact,
        );
        rollback_start = match observe(&record, rollback_start_step) {
            Ok(Some(result)) => Some(result),
            Ok(None) => {
                let reason = start_result.err().unwrap_or_else(|| {
                    "restored runtime start returned without its exact native receipt".into()
                });
                return package_reconciling(
                    store,
                    record,
                    "exact_rollback_start_unresolved",
                    reason,
                );
            }
            Err(error) => {
                return package_reconciling(
                    store,
                    record,
                    "exact_rollback_start_readback_failed",
                    error,
                );
            }
        };
    } else {
        package_attempt(&mut record, rollback_start_step);
    }
    let rollback_start = rollback_start.ok_or("exact rollback-start action disappeared")?;
    let progress = package_progress_mut(&mut record)?.clone();
    if let Err(error) = super::recover::verify_package_result(
        &record.request,
        rollback_start_step,
        &rollback_start,
        &progress,
    ) {
        return package_reconciling(store, record, "exact_rollback_start_receipt_invalid", error);
    }
    save_result(&mut record, rollback_start.clone());
    if rollback_start.target.state == "running" {
        if let Err(error) = super::super::node_lifecycle::package::verify_running_artifact(
            &rollback_start,
            &rollback_artifact,
        ) {
            return package_reconciling(store, record, "restored_runtime_readback_mismatch", error);
        }
        record.state = "failed".into();
        record.stage = "package_promoted_start_failed_rolled_back".into();
        record.blocker = Some(
            "LOCAL_PROCESS_EXITED: promoted package failed; original package/config are restored and the prior runtime is running".into(),
        );
        return persist(store, record);
    }
    if matches!(rollback_start.target.state.as_str(), "failed" | "stopped") {
        record.state = "failed".into();
        record.stage = "package_rollback_start_failed".into();
        record.blocker = Some(
            "LOCAL_PROCESS_EXITED: original package/config were restored, but the exact prior runtime start failed".into(),
        );
        return persist(store, record);
    }
    package_reconciling(
        store,
        record,
        "exact_rollback_start_pending",
        "prior package start has not read back running".into(),
    )
}

fn resume_remote(store: &mut Store, mut record: Record) -> Result<Record, String> {
    let step = match record.request.kind {
        Kind::Start => RemoteStepV1::Start,
        Kind::Stop | Kind::MaintenanceStop => RemoteStepV1::Stop,
        Kind::Restart
            if record.remote_results.iter().any(|result| {
                result.step == RemoteStepV1::Stop && result.outcome == RemoteOutcomeV1::Stopped
            }) =>
        {
            RemoteStepV1::Start
        }
        Kind::Restart => RemoteStepV1::Stop,
        _ => return Err("unsupported remote operation kind".into()),
    };
    let action_id = record.action_id(match step {
        RemoteStepV1::Start => "start",
        RemoteStepV1::Stop => "stop",
    });
    record.stage = match step {
        RemoteStepV1::Start => "starting".into(),
        RemoteStepV1::Stop => "draining".into(),
    };
    record.state = "reconciling".into();
    record.blocker = None;
    let observed = match provider::query_record(&record, step) {
        Ok(result) => result,
        Err(error) => {
            record.blocker = Some(format!("ORIGINAL_OUTCOME_UNKNOWN: {error}"));
            return persist(store, record);
        }
    };
    if let Some(receipt) = observed {
        save_remote_result(&mut record, receipt);
        return complete_remote_step(store, record, step);
    }
    if record
        .action_attempts
        .iter()
        .any(|attempt| attempt == &action_id)
    {
        record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: remote action has no readable receipt; query only, never blind-redeliver".into());
        return persist(store, record);
    }
    record.action_attempts.push(action_id);
    record.dispatched = true;
    record.state = "running".into();
    record = persist(store, record)?;
    match provider::apply_record(&record, step) {
        Ok(Some(receipt)) => {
            save_remote_result(&mut record, receipt);
            complete_remote_step(store, record, step)
        }
        Ok(None) => {
            record.state = "reconciling".into();
            record.blocker = Some(
                "ORIGINAL_OUTCOME_UNKNOWN: remote helper returned no action receipt; query only"
                    .into(),
            );
            persist(store, record)
        }
        Err(error) => {
            record.state = "reconciling".into();
            record.blocker = Some(format!(
                "ORIGINAL_OUTCOME_UNKNOWN: remote effect reply unavailable: {error}"
            ));
            persist(store, record)
        }
    }
}

fn save_remote_result(record: &mut Record, result: super::provider::RemoteActionReceiptV1) {
    if let Some(old) = record
        .remote_results
        .iter_mut()
        .find(|old| old.action_id == result.action_id)
    {
        *old = result;
    } else {
        record.remote_results.push(result);
    }
}

fn complete_remote_step(
    store: &mut Store,
    mut record: Record,
    step: RemoteStepV1,
) -> Result<Record, String> {
    let action_id = record.action_id(match step {
        RemoteStepV1::Start => "start",
        RemoteStepV1::Stop => "stop",
    });
    let observed = record
        .remote_results
        .iter()
        .find(|receipt| receipt.action_id == action_id)
        .cloned()
        .ok_or("remote receipt disappeared")?;
    match observed.outcome {
        RemoteOutcomeV1::Running if step == RemoteStepV1::Start => {
            let instance = observed
                .instance
                .as_ref()
                .ok_or("running receipt omitted instance")?;
            if instance.generation
                != record
                    .request
                    .expected_generation
                    .checked_add(1)
                    .ok_or("generation overflow")?
                || instance.action_id != action_id
                || instance.config_digest != record.request.expected_config_digest
                || instance.state != "running"
            {
                record.state = "reconciling".into();
                record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: remote running receipt does not match the requested next generation".into());
            } else {
                record.state = "succeeded".into();
                record.stage = "remote_process_started".into();
                record.blocker = None;
            }
        }
        RemoteOutcomeV1::Stopped if step == RemoteStepV1::Stop => {
            let expected = record
                .request
                .expected_instance
                .as_ref()
                .ok_or("remote stop target missing")?;
            let instance = observed
                .instance
                .as_ref()
                .ok_or("stopped receipt omitted instance")?;
            if instance.incarnation != expected.incarnation
                || instance.action_id != expected.action_id
                || instance.generation != expected.generation
                || instance.config_digest != expected.config_digest
                || !matches!(instance.state.as_str(), "stopped" | "failed")
            {
                record.state = "reconciling".into();
                record.blocker = Some(
                    "ORIGINAL_OUTCOME_UNKNOWN: remote stop receipt changed exact instance".into(),
                );
            } else if record.request.kind == Kind::Restart {
                record.state = "running".into();
                record.stage = "stop_verified".into();
                record.blocker = None;
            } else {
                record.state = "succeeded".into();
                record.stage = if record.request.kind == Kind::MaintenanceStop {
                    "maintenance_stopped"
                } else {
                    "remote_process_stopped"
                }
                .into();
                record.blocker = None;
            }
        }
        RemoteOutcomeV1::Rejected => {
            record.state = "failed".into();
            record.blocker = observed
                .blocker
                .or_else(|| Some("REMOTE_ACTION_REJECTED".into()));
        }
        _ => {
            record.state = "reconciling".into();
            record.blocker = observed.blocker.or_else(|| {
                Some("ORIGINAL_OUTCOME_UNKNOWN: remote action remains pending; query only".into())
            });
        }
    }
    persist(store, record)
}

fn resume_migration(store: &mut Store, mut record: Record) -> Result<Record, String> {
    let destination = record
        .request
        .migration
        .as_ref()
        .ok_or("migration source/destination missing")?
        .destination
        .clone();
    let expected = record
        .request
        .expected_instance
        .clone()
        .ok_or("exact migration instance missing")?;
    let action_id = record.action_id("migration");
    if record
        .action_attempts
        .iter()
        .any(|attempt| attempt == &action_id)
    {
        match super::super::node_lifecycle::migration::query_data_directory_migration(
            &record.request.config,
            &destination,
            &record.request.operation_id,
            &record.request_digest,
            &expected,
        ) {
            Ok(Some(receipt)) => {
                record.migration_receipt = Some(receipt);
                record.state = "succeeded".into();
                record.stage = "data_directory_migrated_source_preserved".into();
                record.blocker = None;
            }
            Ok(None) => {
                record.state = "reconciling".into();
                record.blocker = Some("ORIGINAL_OUTCOME_UNKNOWN: migration dispatch was persisted without a source intent; query only, no copy retry".into());
            }
            Err(error) => {
                record.state = "reconciling".into();
                record.blocker = Some(error);
            }
        }
        return persist(store, record);
    }
    let capacity = super::super::node_lifecycle::migration::plan_data_directory_migration(
        &record.request.config,
        &destination,
    )?;
    record.migration_capacity_plan = Some(capacity.clone());
    if let Some(blocker) = capacity.blocker() {
        record.state = "blocked".into();
        record.stage = if blocker == "CAPACITY_INSUFFICIENT" {
            "capacity_insufficient"
        } else {
            "source_size_limit"
        }
        .into();
        record.blocker = Some(blocker.into());
        return persist(store, record);
    }
    record.state = "running".into();
    record.stage = "migrating_data_directory".into();
    record.blocker = None;
    record.dispatched = true;
    record.action_attempts.push(action_id);
    record = persist(store, record)?;
    match super::super::node_lifecycle::migration::migrate_data_directory(
        &record.request.config,
        &destination,
        &record.request.operation_id,
        &record.request_digest,
        &expected,
    ) {
        Ok(receipt) => {
            record.migration_receipt = Some(receipt);
            record.state = "succeeded".into();
            record.stage = "data_directory_migrated_source_preserved".into();
            record.blocker = None;
        }
        Err(error) => {
            record.state = "reconciling".into();
            record.blocker = Some(error);
        }
    }
    persist(store, record)
}
