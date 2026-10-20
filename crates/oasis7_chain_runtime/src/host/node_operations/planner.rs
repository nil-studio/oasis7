use super::super::node_lifecycle::{actions, local_io as io};
use super::model::{Kind, ManagedTargetV1, Record, Request};
pub(super) fn plan(request: Request) -> Result<Record, String> {
    validate_identity(&request)?;
    if request.kind == Kind::RemoveCriticalReplica {
        return super::healthy_durable_set::plan(request);
    }
    if request.kind == Kind::ProtectedSignerMigration {
        let handoff_plan = super::protected_handoff::prepare_plan(&request)?;
        let request_digest = io::digest(&request)?;
        return Ok(Record {
            request,
            request_digest,
            revision: 1,
            state: "queued".into(),
            stage: "prepared".into(),
            dispatched: false,
            cancel_requested: false,
            recovered: false,
            blocker: None,
            results: vec![],
            remote_results: vec![],
            action_attempts: vec![],
            migration_receipt: None,
            migration_capacity_plan: None,
            protected_handoff_plan: Some(handoff_plan),
            formal_handoff: None,
            healthy_durable_set_plan: None,
            healthy_durable_set_progress: None,
            operation_result: None,
            package_upgrade_progress: None,
        });
    }
    match request.managed_target.as_ref() {
        Some(ManagedTargetV1::LinuxSshSystemd(target)) => {
            validate_managed_request(&request, target)?
        }
        None => validate_local_request(&request)?,
    }
    let migration_capacity_plan = if request.kind == Kind::MigrateDataDirectory {
        let migration = request
            .migration
            .as_ref()
            .ok_or("data-directory migration request missing")?;
        Some(
            super::super::node_lifecycle::migration::plan_data_directory_migration(
                &request.config,
                &migration.destination,
            )?,
        )
    } else {
        None
    };
    let capacity_blocker = migration_capacity_plan
        .as_ref()
        .and_then(|plan| plan.blocker())
        .map(str::to_owned);
    let request_digest = io::digest(&request)?;
    let package_upgrade_progress = if request.kind == Kind::PackageUpgrade {
        Some(
            super::super::node_lifecycle::package::PackageUpgradeProgressV1 {
                api_version: "oasis7.local-package-upgrade-progress/v1".into(),
                profile: request
                    .package_upgrade
                    .as_ref()
                    .ok_or("package upgrade profile is missing")?
                    .profile
                    .clone(),
                operation_id: request.operation_id.clone(),
                operation_request_digest: request_digest.clone(),
                stage_receipt: None,
                promotion_receipt: None,
                rollback_receipt: None,
            },
        )
    } else {
        None
    };
    Ok(Record {
        request_digest,
        request,
        revision: 1,
        state: if capacity_blocker.is_some() { "blocked" } else { "queued" }.into(),
        stage: match migration_capacity_plan.as_ref().map(|plan| &plan.outcome) {
            Some(
                super::super::node_lifecycle::migration::MigrationCapacityOutcomeV1::CapacityInsufficient,
            ) => "capacity_insufficient",
            Some(
                super::super::node_lifecycle::migration::MigrationCapacityOutcomeV1::SourceSizeLimitExceeded,
            ) => "source_size_limit",
            _ => "prepared",
        }
        .into(),
        dispatched: false,
        cancel_requested: false,
        recovered: false,
        blocker: capacity_blocker,
        results: vec![],
        remote_results: vec![],
        action_attempts: vec![],
        migration_receipt: None,
        migration_capacity_plan,
        protected_handoff_plan: None,
        formal_handoff: None,
            healthy_durable_set_plan: None,
            healthy_durable_set_progress: None,
        operation_result: None,
        package_upgrade_progress,
    })
}

fn validate_managed_request(
    request: &Request,
    target: &super::model::LinuxSshSystemdTargetV1,
) -> Result<(), String> {
    super::provider::validate_target(target)?;
    if request.config != target.remote_config
        || request.world_id.trim().is_empty()
        || request.chain_id.trim().is_empty()
        || !is_digest(&request.expected_config_digest)
    {
        return Err("remote operation request differs from its exact target binding".into());
    }
    match request.kind {
        Kind::Start => {
            if request.expected_instance.is_some()
                || request.expected_generation.checked_add(1).is_none()
            {
                return Err("remote start cannot carry a stop instance".into());
            }
        }
        Kind::Stop | Kind::Restart | Kind::MaintenanceStop => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("exact remote stop target required")?;
            if expected.generation != request.expected_generation
                || expected.config_digest != request.expected_config_digest
            {
                return Err("remote stop target generation/config differs from request".into());
            }
        }
        Kind::MigrateDataDirectory => {
            return Err("remote data-directory migration is unsupported; use the local preserving migration workflow".into());
        }
        Kind::PackageUpgrade => {
            return Err("package upgrade is local-only and cannot use a managed SSH target".into());
        }
        Kind::PermanentExit | Kind::ProtectedSignerMigration | Kind::RemoveCriticalReplica => {
            return validate_protected_operation(request);
        }
    }
    Ok(())
}

pub(super) fn validate_local_request(request: &Request) -> Result<(), String> {
    let binding = actions::binding(&request.config)?;
    if request.world_id != binding.world_id
        || request.chain_id != binding.chain_id
        || request.expected_config_digest != binding.config_digest
    {
        return Err("operation world/chain/config binding mismatch".into());
    }
    if binding.current.as_ref().map_or(0, |i| i.generation) != request.expected_generation {
        return Err("operation expected generation changed".into());
    }
    match request.kind {
        Kind::Start => {
            if request.expected_instance.is_some()
                || binding
                    .current
                    .as_ref()
                    .is_some_and(|i| !matches!(i.state.as_str(), "stopped" | "failed"))
            {
                return Err(
                    "start requires terminal previous generation and no stop target".into(),
                );
            }
        }
        Kind::Stop | Kind::Restart | Kind::MaintenanceStop => {
            validate_exact_local_target(request, binding.current.as_ref())?;
        }
        Kind::MigrateDataDirectory => {
            validate_exact_local_target(request, binding.current.as_ref())?;
            let migration = request
                .migration
                .as_ref()
                .ok_or("data-directory migration request missing source/destination")?;
            if migration.source != binding.data_dir
                || !migration.destination.is_absolute()
                || migration.source == migration.destination
            {
                return Err("data-directory migration requires exact source and distinct absolute destination".into());
            }
            let instance = request
                .expected_instance
                .as_ref()
                .ok_or("data-directory migration requires exact stopped instance")?;
            if !matches!(instance.state.as_str(), "stopped" | "failed") {
                return Err("data-directory migration requires an exact stopped instance".into());
            }
        }
        Kind::PackageUpgrade => {
            let package = request
                .package_upgrade
                .as_ref()
                .ok_or("package upgrade request missing its deployment profile")?;
            package.validate()?;
            package.validate_current_preimage()?;
            package.validate_current_runtime_pin(&request.config)?;
            validate_exact_local_target(request, binding.current.as_ref())?;
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("package upgrade requires an exact running local owner")?;
            if expected.state != "running" || expected.runtime_pid.is_none() {
                return Err("package upgrade requires the exact running local owner".into());
            }
            let status = super::super::node_lifecycle::actions::status(&binding.data_dir)?;
            if !status["blocker"].is_null()
                || status["instance"]["incarnation"].as_str() != Some(&expected.incarnation)
                || status["instance"]["generation"].as_u64() != Some(expected.generation)
                || status["instance"]["config_digest"].as_str() != Some(&expected.config_digest)
                || status["instance"]["state"].as_str() != Some("running")
            {
                return Err("package upgrade requires a ready exact local process owner".into());
            }
        }
        Kind::PermanentExit | Kind::ProtectedSignerMigration | Kind::RemoveCriticalReplica => {
            return validate_protected_operation(request);
        }
    }
    Ok(())
}

fn validate_exact_local_target(
    request: &Request,
    current: Option<&super::super::node_lifecycle::LocalInstance>,
) -> Result<(), String> {
    let expected = request
        .expected_instance
        .as_ref()
        .ok_or("exact stop target required")?;
    let actual = current.ok_or("stop target absent")?;
    if expected.incarnation != actual.incarnation
        || expected.generation != actual.generation
        || expected.config_digest != actual.config_digest
        || expected.action_id != actual.action_id
    {
        return Err("operation exact instance changed".into());
    }
    Ok(())
}

/// Validate a formal recovery permission through the production loader before
/// refusing the requested governance mutation. This is an authorization hook,
/// not a protocol implementation: local operations cannot submit membership
/// transitions or prove signer exclusivity.
fn validate_protected_operation(request: &Request) -> Result<(), String> {
    let auth = request.authorization.as_ref().ok_or_else(|| match request.kind {
        Kind::ProtectedSignerMigration => "FENCING_UNPROVEN: protected signer transfer requires externally verified fencing and recovery evidence".to_string(),
        Kind::RemoveCriticalReplica => "DURABILITY_WOULD_DROP: replica removal requires an authenticated durable-set transition and read-back".to_string(),
        _ => "UNSUPPORTED_ROLE_OR_PROFILE: permanent exit requires verified effective membership-removal proof".to_string(),
    })?;
    match (request.kind, auth) {
        (
            Kind::ProtectedSignerMigration,
            super::model::OperationAuthorizationV1::ProtectedSignerTransfer { .. },
        )
        | (
            Kind::PermanentExit | Kind::RemoveCriticalReplica,
            super::model::OperationAuthorizationV1::FormalRecoveryBootstrap { .. },
        ) => {}
        (Kind::ProtectedSignerMigration, _) => {
            return Err("FENCING_UNPROVEN: signer migration requires the typed signer-transfer authorization".into());
        }
        (Kind::RemoveCriticalReplica, _) => {
            return Err("DURABILITY_WOULD_DROP: replica removal requires the typed formal recovery authorization".into());
        }
        (Kind::PermanentExit, _) => {
            return Err("UNSUPPORTED_ROLE_OR_PROFILE: permanent exit requires the typed formal recovery authorization".into());
        }
        _ => return Err("unsupported protected operation".into()),
    }
    let config_path = match auth {
        super::model::OperationAuthorizationV1::FormalRecoveryBootstrap {
            formal_live_config_file,
        }
        | super::model::OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file,
            ..
        } => formal_live_config_file,
        super::model::OperationAuthorizationV1::HealthyDurableSet { .. } => {
            return Err("healthy replica replacement requires its dedicated planner".into());
        }
    };
    let validated = super::super::formal_live_config::FormalLiveConfigV1::load(config_path)?;
    if validated.anchor.authority().definition().world.world_id != request.world_id
        || validated.anchor.authority().definition().world.chain_id != request.chain_id
    {
        return Err("formal authorization anchor differs from operation world/chain".into());
    }
    let recovery = validated
        .recovery
        .as_ref()
        .ok_or_else(|| match request.kind {
            Kind::ProtectedSignerMigration => {
                "FENCING_UNPROVEN: formal recovery/fencing permission is absent".to_string()
            }
            _ => "UNSUPPORTED_ROLE_OR_PROFILE: no verified protocol transition is available"
                .to_string(),
        })?;
    if let super::model::OperationAuthorizationV1::ProtectedSignerTransfer {
        target_signer_public_key,
        ..
    } = auth
        && target_signer_public_key != &validated.active_authority().definition().writer_public_key
    {
        return Err(
            "FENCING_UNPROVEN: requested signer differs from verified recovery successor".into(),
        );
    }
    let _verified_permission: &oasis7_distfs::controlled_authority::formal::VerifiedRecoveryBootstrapV1 = recovery;
    match request.kind {
        Kind::ProtectedSignerMigration => Err("FENCING_UNPROVEN: verified recovery permission does not prove target signer exclusivity or install the protected key".into()),
        Kind::RemoveCriticalReplica => Err("DURABILITY_WOULD_DROP: external durable-set transition and endpoint read-back are not implemented".into()),
        Kind::PermanentExit => Err("UNSUPPORTED_ROLE_OR_PROFILE: signed recovery evidence is not a committed effective membership-removal transaction".into()),
        _ => Err("unsupported protected operation".into()),
    }
}

pub(super) fn validate_identity(request: &Request) -> Result<(), String> {
    if request.api_version != "oasis7.local-operation/v1"
        || request.operation_id.is_empty()
        || request.operation_id.len() > 128
        || !request
            .operation_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err("unsupported operation schema/identity".into());
    }
    if !request.config.is_absolute() {
        return Err("operation target config requires an absolute path".into());
    }
    let protected = matches!(
        request.kind,
        Kind::PermanentExit | Kind::ProtectedSignerMigration | Kind::RemoveCriticalReplica
    );
    if request.migration.is_some() != (request.kind == Kind::MigrateDataDirectory)
        || (request.authorization.is_some() && !protected)
        || (request.managed_target.is_some() && request.kind == Kind::MigrateDataDirectory)
        || request.package_upgrade.is_some() != (request.kind == Kind::PackageUpgrade)
        || (request.kind == Kind::PackageUpgrade && request.managed_target.is_some())
    {
        return Err("operation-specific migration/authorization/provider fields do not match the requested operation".into());
    }
    match request.kind {
        Kind::Start if request.expected_instance.is_some() => {
            return Err("start request cannot carry a stop-instance target".into());
        }
        Kind::Stop
        | Kind::Restart
        | Kind::MaintenanceStop
        | Kind::MigrateDataDirectory
        | Kind::PackageUpgrade => {
            let expected = request
                .expected_instance
                .as_ref()
                .ok_or("operation requires an exact instance target")?;
            if expected.generation != request.expected_generation
                || expected.config_digest != request.expected_config_digest
            {
                return Err("operation instance generation/config differs from request".into());
            }
        }
        _ => {}
    }
    if !is_digest(&request.expected_config_digest) {
        return Err("operation config digest must be canonical SHA256".into());
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
