//! Shared operation result envelope. Local/service status is never promoted to
//! formal authority, durability, or runtime-installation evidence.
use super::model::Record;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationResultV1 {
    pub api_version: String,
    pub operation_id: String,
    pub request_digest: String,
    pub world_id: String,
    pub chain_id: String,
    pub target_id: String,
    pub kind: String,
    pub outcome: OperationOutcomeV1,
    pub action_ids: Vec<String>,
    pub instance: Option<OperationInstanceV1>,
    pub migration_receipt:
        Option<super::super::node_lifecycle::migration::DataDirectoryMigrationReceiptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_capacity_plan:
        Option<super::super::node_lifecycle::migration::DataDirectoryMigrationCapacityPlanV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_handoff: Option<super::model::ProtectedSignerHandoffReceiptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_durable_set: Option<super::healthy_durable_set::HealthyDurableSetProgressV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_upgrade_progress:
        Option<super::super::node_lifecycle::package::PackageUpgradeProgressV1>,
    pub protocol: Option<VerifiedProtocolObservationV1>,
    /// Why this collector has no externally pinned WorldService observation.
    /// This is explanatory only and never substitutes for protocol evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_unavailable_reason: Option<String>,
    /// This version has no consensus installer contract. It is always absent,
    /// even when governance is effective or a process reports healthy.
    pub runtime_applied_commit: Option<oasis7_client_api::world_service::CommitRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperationOutcomeV1 {
    Pending,
    ReconciliationRequired,
    DeploymentSucceeded,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperationInstanceV1 {
    pub node_id: String,
    pub action_id: String,
    pub incarnation: String,
    pub generation: u64,
    pub config_digest: String,
    pub observed_state: String,
}

/// Constructible only from a VerifiedWorldView carrying a formal qualified
/// commit and its externally verified active authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerifiedProtocolObservationV1 {
    pub schema_version: u32,
    pub operation_id: String,
    pub request_digest: String,
    pub target_id: String,
    pub node_id: String,
    pub operation_instance: OperationInstanceV1,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub authority_id: String,
    pub authority_epoch: u64,
    pub profile_version: u32,
    pub position: u64,
    pub record_id: String,
    pub after_state_root: String,
    pub execution_policy_digest: String,
    pub artifact_roots: BTreeMap<String, String>,
    pub verified_commitment_digest: String,
}

impl OperationResultV1 {
    pub(super) fn collect(record: &Record) -> Result<Self, String> {
        let request = &record.request;
        let target_id = request
            .managed_target
            .as_ref()
            .map(|target| match target {
                super::model::ManagedTargetV1::LinuxSshSystemd(target) => target.node_id.clone(),
            })
            .or_else(|| {
                record
                    .migration_receipt
                    .as_ref()
                    .map(|receipt| receipt.node_id.clone())
            })
            .or_else(|| {
                record
                    .formal_handoff
                    .as_ref()
                    .map(|receipt| receipt.successor_instance.node_id.clone())
            })
            .or_else(|| {
                record
                    .healthy_durable_set_plan
                    .as_ref()
                    .map(|plan| plan.authorization.expected_owner.node_id.clone())
            })
            .or_else(|| {
                record
                    .protected_handoff_plan
                    .as_ref()
                    .map(|plan| plan.predecessor_instance.node_id.clone())
            })
            .or_else(|| {
                record.results.last().map(|result| {
                    format!(
                        "local:{}",
                        result.target.config_digest.get(..16).unwrap_or("unknown")
                    )
                })
            })
            .unwrap_or_else(|| {
                format!(
                    "local:{}",
                    request
                        .expected_config_digest
                        .get(..16)
                        .unwrap_or("unknown")
                )
            });
        let mut action_ids = record
            .results
            .iter()
            .map(|result| result.action_id.clone())
            .chain(
                record
                    .remote_results
                    .iter()
                    .map(|result| result.action_id.clone()),
            )
            .chain(
                record
                    .migration_receipt
                    .iter()
                    .map(|r| format!("op:{}:migration", r.operation_id)),
            )
            .collect::<Vec<_>>();
        if request.kind == super::model::Kind::PackageUpgrade {
            action_ids.extend(record.action_attempts.iter().cloned());
        }
        action_ids.sort();
        action_ids.dedup();
        let instance = record
            .results
            .last()
            .map(|result| OperationInstanceV1 {
                node_id: target_id.clone(),
                action_id: result.action_id.clone(),
                incarnation: result.target.incarnation.clone(),
                generation: result.target.generation,
                config_digest: result.target.config_digest.clone(),
                observed_state: result.target.state.clone(),
            })
            .or_else(|| {
                record.remote_results.last().and_then(|result| {
                    result
                        .instance
                        .as_ref()
                        .map(|instance| OperationInstanceV1 {
                            node_id: result.node_id.clone(),
                            action_id: result.action_id.clone(),
                            incarnation: instance.incarnation.clone(),
                            generation: instance.generation,
                            config_digest: instance.config_digest.clone(),
                            observed_state: instance.state.clone(),
                        })
                })
            });
        let instance = instance.or_else(|| {
            record
                .healthy_durable_set_progress
                .as_ref()
                .and_then(|progress| progress.successor_owner.as_ref())
                .map(|owner| OperationInstanceV1 {
                    node_id: owner.node_id.clone(),
                    action_id: owner.operation_id.clone(),
                    incarnation: owner.incarnation.clone(),
                    generation: owner.authority_epoch,
                    config_digest: owner.config_digest.clone(),
                    observed_state: "ready".into(),
                })
        });
        let outcome = match record.state.as_str() {
            "succeeded" => OperationOutcomeV1::DeploymentSucceeded,
            "cancelled" => OperationOutcomeV1::Cancelled,
            "failed" => OperationOutcomeV1::Failed,
            "queued" | "running" => OperationOutcomeV1::Pending,
            _ => OperationOutcomeV1::ReconciliationRequired,
        };
        let kind = match request.kind {
            super::model::Kind::Start => "start",
            super::model::Kind::Stop => "stop",
            super::model::Kind::Restart => "restart",
            super::model::Kind::MigrateDataDirectory => "migrate_data_directory",
            super::model::Kind::MaintenanceStop => "maintenance_stop",
            super::model::Kind::PermanentExit => "permanent_exit",
            super::model::Kind::ProtectedSignerMigration => "protected_signer_migration",
            super::model::Kind::RemoveCriticalReplica => "remove_critical_replica",
            super::model::Kind::PackageUpgrade => "package_upgrade",
        };
        Ok(Self {
            api_version: "oasis7.operation-result/v1".into(),
            operation_id: request.operation_id.clone(),
            request_digest: record.request_digest.clone(),
            world_id: request.world_id.clone(),
            chain_id: request.chain_id.clone(),
            target_id,
            kind: kind.into(),
            outcome,
            action_ids,
            instance,
            migration_receipt: record.migration_receipt.clone(),
            migration_capacity_plan: record.migration_capacity_plan.clone(),
            formal_handoff: record.formal_handoff.clone(),
            healthy_durable_set: record.healthy_durable_set_progress.clone(),
            package_upgrade_progress: record.package_upgrade_progress.clone(),
            protocol: None,
            protocol_unavailable_reason: None,
            runtime_applied_commit: None,
        })
    }

    pub(super) fn attach_verified_world_view(
        &mut self,
        operation: &Record,
        observation: &super::super::node_lifecycle::operation_read::VerifiedOperationReadV1,
    ) -> Result<(), String> {
        let result = operation
            .results
            .last()
            .ok_or("formal protocol observation requires an exact local action result")?;
        if operation.request.managed_target.is_some()
            || operation.request.kind == super::model::Kind::MigrateDataDirectory
            || self.operation_id != operation.request.operation_id
            || self.request_digest != operation.request_digest
            || self.world_id != operation.request.world_id
            || self.chain_id != operation.request.chain_id
            || self.instance.as_ref().is_none_or(|instance| {
                instance.action_id != result.action_id
                    || instance.incarnation != result.target.incarnation
                    || instance.generation != result.target.generation
                    || instance.config_digest != result.target.config_digest
                    || instance.observed_state != result.target.state
            })
            || observation.node_id.trim().is_empty()
            || observation.world_id != self.world_id
            || observation.chain_id != self.chain_id
            || observation.instance.action_id != result.target.action_id
            || observation.instance.generation != result.target.generation
            || observation.instance.incarnation != result.target.incarnation
            || observation.instance.config_digest != result.target.config_digest
            || observation.instance.state != result.target.state
            || observation.instance.exit_code != result.target.exit_code
        {
            return Err(
                "verified protocol observation differs from exact operation/instance binding"
                    .into(),
            );
        }
        let view = &observation.view;
        let commit = view
            .formal_commit()
            .ok_or("formal operation result requires independently verified qualified commit")?;
        let anchor = view
            .formal_read_anchor()
            .ok_or("formal operation result requires externally verified authority anchor")?;
        let record = commit.record();
        let authority = anchor.authority();
        let definition = authority.definition();
        let authority_id = authority
            .authority_id()
            .map_err(|error| error.to_string())?;
        if record.world.world_id != self.world_id
            || record.world.chain_id != self.chain_id
            || record.world != definition.world
            || commit.finalizing_authority_id() != authority_id
            || record.position != view.version().commit.position
            || record.world.genesis_digest != view.version().commit.world.genesis_digest
            || record.position != anchor.head().record().position
        {
            return Err(
                "formal protocol observation differs from operation world/qualified head".into(),
            );
        }
        let artifact_roots = record
            .artifact_roots
            .iter()
            .map(|(role, root)| (format!("{role:?}").to_ascii_lowercase(), root.clone()))
            .collect::<BTreeMap<_, _>>();
        if !artifact_roots.contains_key("snapshot")
            || !artifact_roots.contains_key("journal")
            || !artifact_roots.contains_key("effectoutbox")
        {
            return Err("qualified protocol observation omits required durable roots".into());
        }
        let verified_commitment_digest = oasis7::world_service::authority::request_digest(
            "oasis7.operation-result.formal-commit.v1",
            &(
                record,
                commit.finalizing_authority_id(),
                anchor.head().head(),
                &artifact_roots,
                &self.operation_id,
                &self.request_digest,
                &self.target_id,
                &observation.node_id,
                self.instance.as_ref(),
            ),
        )?;
        self.protocol = Some(VerifiedProtocolObservationV1 {
            schema_version: 1,
            operation_id: self.operation_id.clone(),
            request_digest: self.request_digest.clone(),
            target_id: self.target_id.clone(),
            node_id: observation.node_id.clone(),
            operation_instance: self.instance.clone().ok_or("operation instance missing")?,
            world_id: record.world.world_id.clone(),
            chain_id: record.world.chain_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
            authority_id: commit.finalizing_authority_id().to_owned(),
            authority_epoch: definition.epoch,
            profile_version: definition.profile_version,
            position: record.position,
            record_id: commit.head().record_id,
            after_state_root: record.after_state_root.clone(),
            execution_policy_digest: record.execution_policy_digest.clone(),
            artifact_roots,
            verified_commitment_digest,
        });
        self.protocol_unavailable_reason = None;
        // Governance membership and a verified commit still do not prove the
        // running service installed that set.
        self.runtime_applied_commit = None;
        Ok(())
    }

    pub(super) fn validate_record_binding(&self, record: &Record) -> Result<(), String> {
        let expected = Self::collect(record)?;
        if self.api_version != expected.api_version
            || self.operation_id != expected.operation_id
            || self.request_digest != expected.request_digest
            || self.world_id != expected.world_id
            || self.chain_id != expected.chain_id
            || self.target_id != expected.target_id
            || self.kind != expected.kind
            || self.outcome != expected.outcome
            || self.action_ids != expected.action_ids
            || self.instance != expected.instance
            || self.migration_receipt != expected.migration_receipt
            || self.migration_capacity_plan != expected.migration_capacity_plan
            || self.formal_handoff != expected.formal_handoff
            || self.healthy_durable_set != expected.healthy_durable_set
            || self.package_upgrade_progress != expected.package_upgrade_progress
            || self.runtime_applied_commit.is_some()
        {
            return Err(
                "operation result does not match its request, action receipts or state".into(),
            );
        }
        if let Some(protocol) = &self.protocol
            && (protocol.operation_id != self.operation_id
                || protocol.request_digest != self.request_digest
                || protocol.target_id != self.target_id
                || Some(&protocol.operation_instance) != self.instance.as_ref()
                || protocol.node_id.trim().is_empty()
                || protocol.world_id != self.world_id
                || protocol.chain_id != self.chain_id
                || protocol.schema_version != 1
                || !is_blake3_digest(&protocol.verified_commitment_digest)
                || protocol
                    .artifact_roots
                    .values()
                    .any(|root| !is_digest(root)))
        {
            return Err(
                "operation result has invalid verified protocol observation binding".into(),
            );
        }
        if self
            .protocol_unavailable_reason
            .as_ref()
            .is_some_and(|reason| reason.is_empty() || reason.len() > 1024)
        {
            return Err("operation result has an invalid protocol collection reason".into());
        }
        Ok(())
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_blake3_digest(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
