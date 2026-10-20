//! Rebuild one lost local management record from its ORIGINAL node-side
//! request binding and native outcomes. This module has no effect dispatch API.
use super::super::node_lifecycle::{LocalActionRecord, actions, local_io as io, management};
use super::{
    model::{Kind, Record, Request},
    planner,
};
pub(super) fn verify_result(
    request: &Request,
    step: &str,
    result: &LocalActionRecord,
) -> Result<(), String> {
    let expected_kind = if step == "rollback-start" {
        "start"
    } else {
        step
    };
    if result.kind != expected_kind
        || result.action_id != format!("op:{}:{step}", request.operation_id)
        || result.target.config_digest != request.expected_config_digest
    {
        return Err("recovered native result request mismatch".into());
    }
    if step == "start" || step == "rollback-start" {
        let expected_generation = if step == "rollback-start" {
            request
                .expected_generation
                .checked_add(1)
                .ok_or("generation overflow")?
        } else {
            request.expected_generation
        };
        if result.expected_generation != expected_generation
            || result.target.generation
                != expected_generation
                    .checked_add(1)
                    .ok_or("generation overflow")?
        {
            return Err("recovered start generation mismatch".into());
        }
    } else {
        let target = request
            .expected_instance
            .as_ref()
            .ok_or("recovered stop target missing")?;
        if result.expected_generation != request.expected_generation
            || result.target.incarnation != target.incarnation
            || result.target.generation != target.generation
            || result.target.action_id != target.action_id
        {
            return Err("recovered stop exact instance mismatch".into());
        }
    }
    Ok(())
}

pub(super) fn verify_package_result(
    request: &Request,
    step: &str,
    result: &LocalActionRecord,
    progress: &super::super::node_lifecycle::package::PackageUpgradeProgressV1,
) -> Result<(), String> {
    let expected_config_digest = match step {
        "start" => progress
            .promotion_receipt
            .as_ref()
            .ok_or("package start result has no exact promotion receipt")?
            .candidate_node_config_digest
            .as_deref()
            .unwrap_or(request.expected_config_digest.as_str()),
        "rollback-start" => progress
            .rollback_receipt
            .as_ref()
            .ok_or("package rollback-start result has no exact rollback receipt")?
            .restored_node_config_digest
            .as_deref()
            .unwrap_or(request.expected_config_digest.as_str()),
        "stop" => request.expected_config_digest.as_str(),
        _ => return Err("unsupported package action result step".into()),
    };
    let mut projected_request = request.clone();
    projected_request.expected_config_digest = expected_config_digest.to_owned();
    verify_result(&projected_request, step, result)
}
pub(super) fn reconstruct(request: Request) -> Result<Record, String> {
    planner::validate_identity(&request)?;
    let digest = io::digest(&request)?;
    if request.kind == Kind::RemoveCriticalReplica {
        return Err("ORIGINAL_OUTCOME_UNKNOWN: healthy transition recovery requires original admitted OperationStore progress; request alone cannot determine whether original arm/seal crossed its boundary".into());
    }
    if request.managed_target.is_some() {
        return reconstruct_remote(request, digest);
    }
    if request.kind == Kind::PackageUpgrade {
        return reconstruct_package_upgrade(request, digest);
    }
    if request.kind == Kind::MigrateDataDirectory {
        let migration = request.migration.as_ref().ok_or(
            "ORIGINAL_OUTCOME_UNKNOWN: migration recovery request omits source/destination",
        )?;
        // Revalidate the immutable request and its exact stopped local target,
        // but do not rerun planner::plan: successful publication necessarily
        // made the destination exist, which the fresh-operation capacity
        // preview correctly rejects.
        planner::validate_local_request(&request)?;
        let binding = actions::binding(&request.config)?;
        if migration.source != binding.data_dir {
            return Err("migration recovery source differs from bound data directory".into());
        }
        let expected = request
            .expected_instance
            .as_ref()
            .ok_or("migration recovery exact stopped instance missing")?;
        let Some(receipt) =
            super::super::node_lifecycle::migration::query_data_directory_migration(
                &request.config,
                &migration.destination,
                &request.operation_id,
                &digest,
                expected,
            )?
        else {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: no immutable migration intent proves an admitted copy; recover will not start one".into());
        };
        let migration_action_id = format!("op:{}:migration", request.operation_id);
        let mut record = Record {
            request,
            request_digest: digest,
            revision: 1,
            state: "succeeded".into(),
            stage: "data_directory_migrated_source_preserved".into(),
            dispatched: true,
            cancel_requested: false,
            recovered: true,
            blocker: None,
            results: vec![],
            remote_results: vec![],
            action_attempts: vec![migration_action_id],
            migration_receipt: Some(receipt),
            // Recovery has no durable admission record from which to recover
            // the original capacity preview. The receipt proves completion;
            // a new preview would misrepresent the already-published target.
            migration_capacity_plan: None,
            protected_handoff_plan: None,
            formal_handoff: None,
            healthy_durable_set_plan: None,
            healthy_durable_set_progress: None,
            operation_result: None,
            package_upgrade_progress: None,
        };
        record.operation_result = Some(super::result::OperationResultV1::collect(&record)?);
        return Ok(record);
    }
    let serialized = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    let binding = actions::binding(&request.config)?;
    if binding.world_id != request.world_id || binding.chain_id != request.chain_id {
        return Err("recovered operation immutable world/chain mismatch".into());
    }
    if matches!(
        request.kind,
        Kind::Stop | Kind::Restart | Kind::MaintenanceStop
    ) && request.expected_instance.is_none()
    {
        return Err("recovered operation exact instance missing".into());
    }
    if request.kind == Kind::Start && request.expected_instance.is_some() {
        return Err("recovered start unexpected stop target".into());
    }
    let steps = match request.kind {
        Kind::Start => vec!["start"],
        Kind::Stop | Kind::MaintenanceStop => vec!["stop"],
        Kind::Restart => vec!["stop", "start"],
        Kind::PackageUpgrade => unreachable!("package recovery returned above"),
        Kind::PermanentExit | Kind::ProtectedSignerMigration | Kind::RemoveCriticalReplica => {
            return Err(
                "unsupported protected operation cannot be reconstructed as a local process effect"
                    .into(),
            );
        }
        Kind::MigrateDataDirectory => unreachable!(),
    };
    let mut results = vec![];
    let mut contexts = std::collections::BTreeSet::new();
    let mut admission_without_receipt = false;
    for step in steps {
        let action_id = format!("op:{}:{step}", request.operation_id);
        let context = management::read(&request.config, &action_id)?;
        if let Some(context) = &context {
            if context.operation_id != request.operation_id
                || context.step_id != step
                || context.request_digest != digest
                || context.request_json != serialized
            {
                return Err("original immutable management request differs; recovery cannot replace the final goal".into());
            }
            contexts.insert(step);
        }
        let result = actions::query(&binding.data_dir, &action_id)?;
        if let Some(result) = result {
            if context.is_none() {
                return Err("native receipt lacks original management request binding; retroactive recovery refused".into());
            }
            verify_result(&request, step, &result)?;
            results.push(result);
        } else if context.is_some() || !actions::admission_absent(&request.config, &action_id)? {
            admission_without_receipt = true;
        }
    }
    let first = if request.kind == Kind::Start {
        "start"
    } else {
        "stop"
    };
    if !contexts.contains(first) {
        return Err(
            "original management request binding absent; unknown goal cannot be reconstructed"
                .into(),
        );
    }
    let mut record = Record {
        request,
        request_digest: digest,
        revision: 1,
        state: "reconciling".into(),
        stage: "query_only_recovery".into(),
        dispatched: true,
        cancel_requested: false,
        recovered: true,
        blocker: Some(
            "ORIGINAL_OUTCOME_UNKNOWN: original native outcome remains unresolved".into(),
        ),
        results,
        remote_results: vec![],
        action_attempts: vec![],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: None,
        formal_handoff: None,
        healthy_durable_set_plan: None,
        healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress: None,
    };
    if admission_without_receipt {
        record.blocker=Some("ORIGINAL_OUTCOME_UNKNOWN: original admission/request binding has no native receipt; recovery does not resend".into());
        return Ok(record);
    }
    if record.results.iter().any(|r| r.target.state == "unknown") {
        return Ok(record);
    }
    let terminal_stop = record
        .results
        .iter()
        .any(|r| r.kind == "stop" && matches!(r.target.state.as_str(), "stopped" | "failed"));
    let started = record.results.iter().find(|r| r.kind == "start");
    if record.request.kind == Kind::Stop && terminal_stop {
        record.state = "succeeded".into();
        record.stage = "local_process_stopped".into();
        record.blocker = None;
    } else if let Some(started) = started {
        if started.target.state == "running" {
            record.state = "succeeded".into();
            record.stage = "local_process_started".into();
            record.blocker = None;
        } else if started.target.state == "failed" {
            record.state = "failed".into();
            record.blocker =
                Some("LOCAL_PROCESS_EXITED: exact original start receipt retained".into());
        }
    } else if record.request.kind == Kind::Restart && terminal_stop && !contexts.contains("start") {
        record.state = "running".into();
        record.stage = "stop_verified".into();
        record.blocker = None;
    }
    Ok(record)
}

fn reconstruct_package_upgrade(request: Request, digest: String) -> Result<Record, String> {
    let package = request
        .package_upgrade
        .clone()
        .ok_or("ORIGINAL_OUTCOME_UNKNOWN: package recovery profile is absent")?;
    package.validate_staged()?;
    let initial_binding = actions::binding(&request.config)?;
    if initial_binding.world_id != request.world_id || initial_binding.chain_id != request.chain_id
    {
        return Err(
            "ORIGINAL_OUTCOME_UNKNOWN: package recovery world/chain identity changed".into(),
        );
    }
    let (stage, promotion, rollback) = super::super::node_lifecycle::package::query(
        &request.config,
        &package,
        &request.operation_id,
        &digest,
    )?
    .ok_or("ORIGINAL_OUTCOME_UNKNOWN: no durable package stage transaction binds this OperationRecord; recovery will not create one")?;
    let progress = super::super::node_lifecycle::package::PackageUpgradeProgressV1 {
        api_version: "oasis7.local-package-upgrade-progress/v1".into(),
        profile: package.profile.clone(),
        operation_id: request.operation_id.clone(),
        operation_request_digest: digest.clone(),
        stage_receipt: Some(stage),
        promotion_receipt: promotion,
        rollback_receipt: rollback,
    };
    super::super::node_lifecycle::package::validate_progress(
        &progress,
        &package,
        &request.operation_id,
        &digest,
    )?;
    let binding = actions::binding(&request.config)?;
    let expected_live_config_digest = progress
        .rollback_receipt
        .as_ref()
        .and_then(|receipt| receipt.restored_node_config_digest.as_deref())
        .or_else(|| {
            progress
                .promotion_receipt
                .as_ref()
                .and_then(|receipt| receipt.candidate_node_config_digest.as_deref())
        })
        .unwrap_or(request.expected_config_digest.as_str());
    if binding.world_id != request.world_id
        || binding.chain_id != request.chain_id
        || binding.config_digest != expected_live_config_digest
    {
        return Err(
            "ORIGINAL_OUTCOME_UNKNOWN: package recovery node/config identity is outside the exact staged, promoted, or rolled-back transaction states".into(),
        );
    }
    let mut record = Record {
        request,
        request_digest: digest,
        revision: 1,
        state: "reconciling".into(),
        stage: "package_transaction_recovered".into(),
        dispatched: true,
        cancel_requested: false,
        recovered: true,
        blocker: Some(
            "ORIGINAL_OUTCOME_UNKNOWN: recovered package transaction is query-only until resumed"
                .into(),
        ),
        results: vec![],
        remote_results: vec![],
        action_attempts: vec![format!("op:{}:stage", progress.operation_id)],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: None,
        formal_handoff: None,
        healthy_durable_set_plan: None,
        healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress: Some(progress.clone()),
    };
    if let Some(stop) = actions::query(&binding.data_dir, &record.action_id("stop"))? {
        verify_result(&record.request, "stop", &stop)?;
        record.action_attempts.push(record.action_id("stop"));
        record.results.push(stop.clone());
        if let Some(promotion) = &progress.promotion_receipt
            && (stop.target.state != "stopped"
                || stop.target.runtime_pid.is_some()
                || io::digest(&stop)? != promotion.stop_receipt_sha256)
        {
            return Err(
                "ORIGINAL_OUTCOME_UNKNOWN: package promotion and original stop receipt disagree"
                    .into(),
            );
        }
    } else if progress.promotion_receipt.is_some() {
        return Err("ORIGINAL_OUTCOME_UNKNOWN: promoted transaction has no original exact stop action receipt".into());
    }
    if progress.promotion_receipt.is_some() {
        record.action_attempts.push(record.action_id("promote"));
    }
    if let Some(start) = actions::query(&binding.data_dir, &record.action_id("start"))? {
        verify_package_result(&record.request, "start", &start, &progress)?;
        if progress.promotion_receipt.is_none() {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: package start receipt predates a durable promotion receipt".into());
        }
        record.action_attempts.push(record.action_id("start"));
        record.results.push(start);
    }
    if progress.rollback_receipt.is_some() {
        record.action_attempts.push(record.action_id("rollback"));
    }
    if let Some(start) = actions::query(&binding.data_dir, &record.action_id("rollback-start"))? {
        verify_package_result(&record.request, "rollback-start", &start, &progress)?;
        if progress.rollback_receipt.is_none() {
            return Err(
                "ORIGINAL_OUTCOME_UNKNOWN: rollback-start receipt has no durable package rollback"
                    .into(),
            );
        }
        record
            .action_attempts
            .push(record.action_id("rollback-start"));
        record.results.push(start);
    }
    if let Some(start) = record
        .results
        .iter()
        .find(|result| result.action_id == record.action_id("start"))
    {
        if matches!(start.target.state.as_str(), "failed" | "stopped") {
            if progress.rollback_receipt.is_none() {
                record.state = "reconciling".into();
                record.stage = "package_promoted_start_failed_rollback_pending".into();
                record.blocker = Some(
                    "LOCAL_PROCESS_EXITED: exact promoted start failed; rollback is pending".into(),
                );
            }
        } else if start.target.state == "running" {
            let stage = progress
                .stage_receipt
                .as_ref()
                .ok_or("package stage receipt missing during start recovery")?;
            let promotion = progress
                .promotion_receipt
                .as_ref()
                .ok_or("package promotion receipt missing during start recovery")?;
            let runtime_artifact =
                super::super::node_lifecycle::package::verified_runtime_artifact(
                    &package,
                    &record.request.operation_id,
                    &record.request_digest,
                    stage,
                    promotion,
                )?;
            if let Err(error) = super::super::node_lifecycle::package::verify_running_artifact(
                start,
                &runtime_artifact,
            ) {
                record.state = "reconciling".into();
                record.blocker = Some(format!(
                    "ORIGINAL_OUTCOME_UNKNOWN: promoted runtime executable readback failed: {error}"
                ));
            } else {
                record.state = "succeeded".into();
                record.stage = "package_promoted_and_started".into();
                record.blocker = None;
            }
        }
    }
    if let Some(rollback) = progress.rollback_receipt.as_ref() {
        if let Some(start) = record
            .results
            .iter()
            .find(|result| result.action_id == record.action_id("rollback-start"))
        {
            if start.target.state == "running" {
                let artifact =
                    super::super::node_lifecycle::package::verified_rollback_runtime_artifact(
                        &package,
                        &record.request.operation_id,
                        &record.request_digest,
                        rollback,
                    )?;
                super::super::node_lifecycle::package::verify_running_artifact(start, &artifact)?;
                record.state = "failed".into();
                record.stage = "package_promoted_start_failed_rolled_back".into();
                record.blocker = Some("LOCAL_PROCESS_EXITED: promoted package failed; prior package restored and running".into());
            } else if matches!(start.target.state.as_str(), "failed" | "stopped") {
                record.state = "failed".into();
                record.stage = "package_rollback_start_failed".into();
                record.blocker = Some(
                    "LOCAL_PROCESS_EXITED: prior package was restored but its exact restart failed"
                        .into(),
                );
            }
        } else {
            record.state = "reconciling".into();
            record.stage = "package_rollback_verified_waiting_for_start".into();
            record.blocker = Some(
                "package rollback restored the exact preimage; prior runtime start is pending"
                    .into(),
            );
        }
    } else if progress.promotion_receipt.is_some()
        && !record
            .results
            .iter()
            .any(|result| result.action_id == record.action_id("start"))
    {
        record.state = "running".into();
        record.stage = "package_promoted_waiting_for_exact_start".into();
        record.blocker = None;
    } else if record.results.iter().any(|result| {
        result.kind == "stop" && matches!(result.target.state.as_str(), "stopped" | "failed")
    }) {
        record.state = "running".into();
        record.stage = "exact_stop_recovered".into();
        record.blocker = None;
    }
    record.operation_result = Some(super::result::OperationResultV1::collect(&record)?);
    Ok(record)
}

fn reconstruct_remote(request: Request, digest: String) -> Result<Record, String> {
    let steps = match request.kind {
        Kind::Start => vec![super::provider::RemoteStepV1::Start],
        Kind::Stop | Kind::MaintenanceStop => vec![super::provider::RemoteStepV1::Stop],
        Kind::Restart => vec![
            super::provider::RemoteStepV1::Stop,
            super::provider::RemoteStepV1::Start,
        ],
        _ => return Err("unsupported remote operation cannot be reconstructed".into()),
    };
    let mut record = Record {
        request,
        request_digest: digest,
        revision: 1,
        state: "reconciling".into(),
        stage: "query_only_remote_recovery".into(),
        dispatched: true,
        cancel_requested: false,
        recovered: true,
        blocker: Some("ORIGINAL_OUTCOME_UNKNOWN: remote native outcomes are being queried; no action will be reissued".into()),
        results: vec![],
        remote_results: vec![],
        action_attempts: vec![],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: None,
        formal_handoff: None,
            healthy_durable_set_plan: None,
            healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress: None,
    };
    for step in steps {
        if let Some(receipt) = super::provider::query_record(&record, step)? {
            record.action_attempts.push(receipt.action_id.clone());
            record.remote_results.push(receipt);
        }
    }
    if record.remote_results.is_empty() {
        return Err("ORIGINAL_OUTCOME_UNKNOWN: no remote action receipt proves this immutable request was admitted; recover will not create an operation".into());
    }
    Ok(record)
}
