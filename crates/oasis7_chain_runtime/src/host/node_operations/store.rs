use super::super::node_lifecycle::local_io as io;
use super::{
    model::{Kind, Record},
    provider::{RemoteStepV1, request_for, validate_receipt},
    result::OperationResultV1,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    api_version: String,
    records: Vec<Record>,
}
pub(super) struct Store {
    path: PathBuf,
    data: Data,
    _ownership: File,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("operation store requires absolute path".into());
        }
        io::private_dir(root)?;
        let ownership = io::private_file(&root.join("store.lock"))?;
        ownership
            .try_lock()
            .map_err(|_| "operation store busy; another executor owns the domain")?;
        let path = root.join("operations.json");
        let data: Data = if path.exists() {
            io::read(&path)?
        } else {
            Data {
                api_version: "oasis7.local-operation-store/v1".into(),
                records: vec![],
            }
        };
        validate(&data)?;
        Ok(Self {
            path,
            data,
            _ownership: ownership,
        })
    }
    pub fn snapshot(root: &Path, id: &str) -> Result<Record, String> {
        if !root.is_absolute() {
            return Err("operation store requires absolute path".into());
        }
        io::reject_symlinks(root)?;
        super::super::controlled_live_config::require_private_owned(root, true)?;
        let data: Data = io::read(&root.join("operations.json"))?;
        validate(&data)?;
        data.records
            .into_iter()
            .find(|r| r.request.operation_id == id)
            .ok_or("operation absent".into())
    }
    pub fn get(&self, id: &str) -> Result<Record, String> {
        self.data
            .records
            .iter()
            .find(|r| r.request.operation_id == id)
            .cloned()
            .ok_or("operation absent".into())
    }
    pub fn existing(&self, id: &str) -> Option<Record> {
        self.get(id).ok()
    }
    pub fn admit(&mut self, mut record: Record) -> Result<Record, String> {
        if let Some(original) = self.existing(&record.request.operation_id) {
            if original.request_digest != record.request_digest {
                return Err("operation identity reused with different request".into());
            }
            return Ok(original);
        }
        // This local provider conservatively serializes its world. It cannot
        // coordinate another domain or replace protocol authority fencing.
        let requested_paths = reserved_config_paths(&record.request);
        if self.data.records.iter().any(|r| {
            !r.terminal()
                && ((r.request.world_id == record.request.world_id
                    && r.request.chain_id == record.request.chain_id)
                    || reserved_config_paths(&r.request)
                        .iter()
                        .any(|path| requested_paths.contains(path)))
        }) {
            return Err(
                "operating-domain world or target resource reserved by unresolved operation".into(),
            );
        }
        if self.data.records.len() >= 4096 {
            return Err("operation store capacity reached; retain receipts and choose another domain store after reconciling pending work".into());
        }
        record.operation_result = Some(OperationResultV1::collect(&record)?);
        self.data.records.push(record.clone());
        io::write(&self.path, &self.data)?;
        Ok(record)
    }
    pub fn cas(&mut self, mut record: Record, expected: u64) -> Result<Record, String> {
        let slot = self
            .data
            .records
            .iter_mut()
            .find(|r| r.request.operation_id == record.request.operation_id)
            .ok_or("operation absent")?;
        if slot.revision != expected || slot.request_digest != record.request_digest {
            return Err("operation CAS revision/request changed".into());
        }
        record.revision = expected.checked_add(1).ok_or("revision overflow")?;
        let prior = record
            .operation_result
            .as_ref()
            .filter(|result| result.validate_record_binding(&record).is_ok())
            .or_else(|| {
                slot.operation_result
                    .as_ref()
                    .filter(|result| result.validate_record_binding(&record).is_ok())
            });
        let mut operation_result = OperationResultV1::collect(&record)?;
        if let Some(prior) = prior {
            operation_result.protocol = prior.protocol.clone();
            operation_result.protocol_unavailable_reason =
                prior.protocol_unavailable_reason.clone();
        }
        record.operation_result = Some(operation_result);
        *slot = record.clone();
        io::write(&self.path, &self.data)?;
        Ok(record)
    }
}

fn validate(data: &Data) -> Result<(), String> {
    if data.api_version != "oasis7.local-operation-store/v1" {
        return Err("unsupported operation store schema".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for r in &data.records {
        if !ids.insert(&r.request.operation_id)
            || io::digest(&r.request)? != r.request_digest
            || r.revision == 0
            || validate_record(r).is_err()
        {
            return Err("corrupt operation store identity/digest/revision".into());
        }
    }
    Ok(())
}

fn validate_record(record: &Record) -> Result<(), String> {
    super::planner::validate_identity(&record.request)?;
    if record.request.kind == Kind::RemoveCriticalReplica {
        return super::healthy_durable_set::validate_record(record);
    }
    super::protected_handoff::validate_persisted_record(record)?;
    if let Some(plan) = &record.migration_capacity_plan {
        let migration = record
            .request
            .migration
            .as_ref()
            .filter(|_| record.request.kind == Kind::MigrateDataDirectory)
            .ok_or("non-migration operation contains a migration capacity preview")?;
        plan.validate(&migration.source, &migration.destination)?;
        if let Some(blocker) = plan.blocker()
            && (record.state != "blocked" || record.blocker.as_deref() != Some(blocker))
        {
            return Err("migration capacity blocker differs from the stored preview".into());
        }
    }
    if !matches!(
        record.state.as_str(),
        "queued" | "running" | "blocked" | "reconciling" | "succeeded" | "cancelled" | "failed"
    ) {
        return Err("unsupported operation state".into());
    }
    if record.request.kind == Kind::ProtectedSignerMigration {
        if !record.action_attempts.is_empty() {
            return Err("protected handoff cannot contain local or remote action attempts".into());
        }
        if let Some(result) = &record.operation_result {
            result.validate_record_binding(record)?;
        }
        return Ok(());
    }
    let expected_steps: &[&str] = match record.request.kind {
        Kind::Start => &["start"],
        Kind::Stop | Kind::MaintenanceStop => &["stop"],
        Kind::Restart => &["stop", "start"],
        Kind::MigrateDataDirectory => &["migration"],
        Kind::PackageUpgrade => &[
            "stage",
            "stop",
            "promote",
            "start",
            "rollback",
            "rollback-start",
        ],
        Kind::PermanentExit | Kind::ProtectedSignerMigration | Kind::RemoveCriticalReplica => {
            return Err("unsupported protected operation was persisted".into());
        }
    };
    let mut allowed_action_ids = std::collections::BTreeSet::new();
    for step in expected_steps {
        allowed_action_ids.insert(record.action_id(step));
    }
    let mut attempts = std::collections::BTreeSet::new();
    if record
        .action_attempts
        .iter()
        .any(|attempt| !allowed_action_ids.contains(attempt) || !attempts.insert(attempt))
    {
        return Err("operation action-attempt list is not bound to the request".into());
    }

    if record.request.managed_target.is_some() {
        if !record.results.is_empty()
            || record.migration_receipt.is_some()
            || record.package_upgrade_progress.is_some()
        {
            return Err("managed remote operation contains local or migration evidence".into());
        }
        let mut action_ids = std::collections::BTreeSet::new();
        for result in &record.remote_results {
            let step_name = match result.step {
                RemoteStepV1::Start => "start",
                RemoteStepV1::Stop => "stop",
            };
            if !allowed_action_ids.contains(&record.action_id(step_name))
                || result.action_id != record.action_id(step_name)
                || !action_ids.insert(&result.action_id)
            {
                return Err("remote result action identity differs from operation".into());
            }
            validate_receipt(result, &request_for(record, result.step)?)?;
        }
    } else {
        if !record.remote_results.is_empty() {
            return Err("local operation contains remote receipts".into());
        }
        let mut action_ids = std::collections::BTreeSet::new();
        for result in &record.results {
            if !action_ids.insert(&result.action_id) {
                return Err("duplicate local operation action receipt".into());
            }
            let result_step = if record.request.kind == Kind::PackageUpgrade
                && result.action_id == record.action_id("rollback-start")
            {
                "rollback-start"
            } else {
                result.kind.as_str()
            };
            if record.request.kind == Kind::PackageUpgrade {
                let progress = record
                    .package_upgrade_progress
                    .as_ref()
                    .ok_or("package action result has no durable package progress")?;
                super::recover::verify_package_result(
                    &record.request,
                    result_step,
                    result,
                    progress,
                )?;
            } else {
                super::recover::verify_result(&record.request, result_step, result)?;
            }
            if !allowed_action_ids.contains(&result.action_id) {
                return Err("local result action identity differs from operation".into());
            }
            if record.request.kind != Kind::PackageUpgrade && result.runtime_artifact.is_some() {
                return Err("non-package local start contains a package runtime binding".into());
            }
        }
        if record.request.kind == Kind::MigrateDataDirectory {
            let migration = record
                .request
                .migration
                .as_ref()
                .ok_or("migration request missing")?;
            if let Some(receipt) = &record.migration_receipt
                && (receipt.api_version != "oasis7.local-data-migration-receipt/v1"
                    || receipt.operation_id != record.request.operation_id
                    || receipt.request_digest != record.request_digest
                    || receipt.source_data_dir != migration.source
                    || receipt.destination_data_dir != migration.destination
                    || receipt.world_id != record.request.world_id
                    || receipt.chain_id != record.request.chain_id
                    || receipt.node_id.is_empty()
                    || receipt.root_public_key.is_empty()
                    || receipt.peer_id.is_empty())
            {
                return Err("migration receipt differs from exact operation request".into());
            }
        } else if record.migration_receipt.is_some() {
            return Err("non-migration operation contains migration receipt".into());
        }
    }
    if record.request.kind == Kind::PackageUpgrade {
        let package_request = record
            .request
            .package_upgrade
            .as_ref()
            .ok_or("package upgrade request is missing its immutable deployment profile")?;
        let progress = record
            .package_upgrade_progress
            .as_ref()
            .ok_or("package upgrade progress is missing")?;
        super::super::node_lifecycle::package::validate_progress(
            progress,
            package_request,
            &record.request.operation_id,
            &record.request_digest,
        )?;
        let expected_attempts = expected_steps
            .iter()
            .take(record.action_attempts.len())
            .map(|step| record.action_id(step))
            .collect::<Vec<_>>();
        if record.action_attempts != expected_attempts {
            return Err(
                "package lifecycle effects are not a fixed stage/stop/promote/start prefix".into(),
            );
        }
        let stop = record.results.iter().find(|result| result.kind == "stop");
        let start = record
            .results
            .iter()
            .find(|result| result.action_id == record.action_id("start"));
        let rollback_start = record
            .results
            .iter()
            .find(|result| result.action_id == record.action_id("rollback-start"));
        if progress.stage_receipt.is_some()
            != record
                .action_attempts
                .iter()
                .any(|id| id == &record.action_id("stage"))
            && record
                .action_attempts
                .iter()
                .any(|id| id == &record.action_id("stop"))
        {
            return Err("package stop cannot precede its durable stage receipt".into());
        }
        if let Some(promotion) = &progress.promotion_receipt {
            let stop = stop.ok_or("package promotion receipt has no local stop receipt")?;
            if !matches!(stop.target.state.as_str(), "stopped" | "failed")
                || stop.target.runtime_pid.is_some()
                || promotion.stop_receipt_sha256
                    != super::super::node_lifecycle::local_io::digest(stop)?
            {
                return Err(
                    "package promotion receipt does not bind the exact stopped owner".into(),
                );
            }
        }
        if let Some(start) = start {
            let stage = progress
                .stage_receipt
                .as_ref()
                .ok_or("package start receipt has no stage receipt")?;
            let promotion = progress
                .promotion_receipt
                .as_ref()
                .ok_or("package start receipt is not after exact promotion")?;
            let artifact = start
                .runtime_artifact
                .as_ref()
                .ok_or("package start receipt omits its verified runtime binding")?;
            super::super::node_lifecycle::package::validate_runtime_artifact_binding(
                package_request,
                &record.request.operation_id,
                &record.request_digest,
                stage,
                promotion,
                artifact,
            )?;
            if start.target.generation
                != record
                    .request
                    .expected_generation
                    .checked_add(1)
                    .ok_or("package generation overflow")?
                || (start.target.state != "running"
                    && !(matches!(record.state.as_str(), "failed" | "reconciling" | "running")
                        && matches!(start.target.state.as_str(), "failed" | "stopped")))
            {
                return Err("package start receipt is not after exact promotion".into());
            }
        }
        if progress.rollback_receipt.is_some()
            && (!record
                .action_attempts
                .iter()
                .any(|id| id == &record.action_id("rollback"))
                || start.is_none_or(|result| {
                    !matches!(result.target.state.as_str(), "failed" | "stopped")
                }))
        {
            return Err(
                "package rollback receipt lacks the failed promoted start and rollback dispatch"
                    .into(),
            );
        }
        if record
            .action_attempts
            .iter()
            .any(|id| id == &record.action_id("rollback-start"))
            && progress.rollback_receipt.is_none()
        {
            return Err("package rollback start has no exact restored package receipt".into());
        }
        if let Some(start) = rollback_start {
            let rollback = progress
                .rollback_receipt
                .as_ref()
                .ok_or("rollback-start receipt has no package rollback receipt")?;
            let artifact = start
                .runtime_artifact
                .as_ref()
                .ok_or("rollback-start receipt omits its verified prior runtime binding")?;
            super::super::node_lifecycle::package::validate_rollback_runtime_artifact_binding(
                package_request,
                &record.request.operation_id,
                &record.request_digest,
                rollback,
                artifact,
            )?;
            let expected_generation = record
                .request
                .expected_generation
                .checked_add(2)
                .ok_or("package rollback generation overflow")?;
            if start.target.generation != expected_generation
                || (start.target.state != "running"
                    && !(matches!(record.state.as_str(), "failed" | "reconciling" | "running")
                        && matches!(start.target.state.as_str(), "failed" | "stopped")))
            {
                return Err("rollback-start receipt is not the exact successor of the failed promoted start".into());
            }
        }
        if record.state == "succeeded"
            && (progress.promotion_receipt.is_none()
                || start.is_none_or(|result| result.target.state != "running"))
        {
            return Err(
                "package operation cannot succeed before promoted package start readback".into(),
            );
        }
    } else if record.package_upgrade_progress.is_some() {
        return Err("non-package operation contains package progress".into());
    }
    if let Some(result) = &record.operation_result {
        result.validate_record_binding(record)?;
    }
    Ok(())
}

fn reserved_config_paths(request: &super::model::Request) -> Vec<&Path> {
    let mut paths = vec![request.config.as_path()];
    if let Some(super::model::OperationAuthorizationV1::ProtectedSignerTransfer {
        formal_live_config_file,
        ..
    }) = request.authorization.as_ref()
    {
        paths.push(formal_live_config_file.as_path());
    }
    if let Some(super::model::OperationAuthorizationV1::HealthyDurableSet { authorization }) =
        request.authorization.as_ref()
    {
        paths.push(authorization.successor_runtime_config_file.as_path());
    }
    paths
}
