//! Exact, proof-backed handoff between formal runtime owner processes.
//!
//! The operation record binds both config bundles before dispatch. Resume is
//! query-first and each irreversible transition is persisted before it runs.
//! Local owner state is used only to control exact processes; completion also
//! requires independently restoring both signed formal endpoints.
use super::super::{
    formal_live_config::{FormalLiveConfigV1, ValidatedFormalLiveConfigV1},
    formal_live_runtime::{self, FormalRuntimeInstanceV1, FormalRuntimeOwnerSnapshotV1},
    node_lifecycle::local_io as io,
};
use super::model::{Kind, OperationAuthorizationV1, Record, Request};
use oasis7_distfs::controlled_authority::{
    formal::{VerifiedRecoveryBootstrapV1, transport::RemoteFormalEndpoint},
    replicated_protocol::EndpointRole,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const PLAN_API: &str = "oasis7.protected-signer-handoff-plan/v1";
const RECEIPT_API: &str = "oasis7.protected-signer-handoff-receipt/v1";

#[path = "protected_handoff_readback.rs"]
mod readback;

/// Non-secret, immutable admission facts. Secret bytes stay in the private
/// source files and are re-read only through the hardened formal config loader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProtectedSignerHandoffPlanV1 {
    pub api_version: String,
    pub request_digest: String,
    pub predecessor_config_file: PathBuf,
    pub predecessor_config_digest: String,
    pub predecessor_input_bundle_digest: String,
    pub predecessor_recovery_digest: Option<String>,
    pub predecessor_writer_directory: PathBuf,
    pub predecessor_instance: FormalRuntimeInstanceV1,
    pub successor_config_file: PathBuf,
    pub successor_config_digest: String,
    pub successor_input_bundle_digest: String,
    pub recovery_permission_digest: String,
    pub recovery_cut_position: u64,
    pub successor_writer_directory: PathBuf,
    pub target_signer_public_key: String,
}

pub(super) struct ValidatedHandoffInputs {
    predecessor: ValidatedFormalLiveConfigV1,
    successor: ValidatedFormalLiveConfigV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalHandoffReadbackV1 {
    pub active_authority_id: String,
    pub active_authority_epoch: u64,
    pub profile_version: u32,
    pub qualified_position: u64,
    pub qualified_record_id: String,
    pub after_state_root: String,
    pub primary_checkpoint_digest: String,
    pub replica_checkpoint_digest: String,
    pub qualified_evidence_digest: String,
    pub recovery_permission_digest: String,
}

/// Plan is produced once, during admission, and persisted before any process
/// action. The predecessor must be the exact currently-ready owner instance.
pub(super) fn prepare_plan(request: &Request) -> Result<ProtectedSignerHandoffPlanV1, String> {
    let (successor_config, target_signer, predecessor) = match request.authorization.as_ref() {
        Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file,
            target_signer_public_key,
            expected_predecessor_instance: Some(predecessor),
        }) => (
            formal_live_config_file,
            target_signer_public_key,
            predecessor,
        ),
        Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            expected_predecessor_instance: None,
            ..
        }) => return Err("FENCING_UNPROVEN: exact predecessor owner instance is required".into()),
        _ => {
            return Err(
                "FENCING_UNPROVEN: typed protected signer transfer authorization required".into(),
            );
        }
    };
    if request.kind != Kind::ProtectedSignerMigration
        || request.managed_target.is_some()
        || request.migration.is_some()
        || request.expected_instance.is_some()
        || request.expected_generation != predecessor.authority_epoch
        || request.expected_config_digest != predecessor.config_digest
        || predecessor.profile_version != 2
        || predecessor.node_id.trim().is_empty()
        || !is_digest(&predecessor.config_digest)
    {
        return Err(
            "FENCING_UNPROVEN: request does not bind an exact profile-2 predecessor".into(),
        );
    }

    let predecessor_config_file = absolute_existing_path(&request.config)?;
    let successor_config_file = absolute_existing_path(successor_config)?;
    let predecessor_real_path =
        std::fs::canonicalize(&predecessor_config_file).map_err(|error| error.to_string())?;
    let successor_real_path =
        std::fs::canonicalize(&successor_config_file).map_err(|error| error.to_string())?;
    if predecessor_real_path == successor_real_path {
        return Err(
            "FENCING_UNPROVEN: predecessor and successor formal configs must be distinct".into(),
        );
    }
    let predecessor_config_digest = io::sha256(&predecessor_config_file)?;
    if predecessor_config_digest != predecessor.config_digest
        || predecessor_config_digest != request.expected_config_digest
    {
        return Err(
            "FENCING_UNPROVEN: predecessor formal config differs from exact owner identity".into(),
        );
    }
    let predecessor_inputs = FormalLiveConfigV1::load(&predecessor_config_file)?;
    let successor_inputs = FormalLiveConfigV1::load(&successor_config_file)?;
    validate_config_pair(
        request,
        predecessor,
        target_signer,
        &predecessor_inputs,
        &successor_inputs,
    )?;

    let predecessor_writer_directory =
        canonical_existing_path(&predecessor_inputs.config.writer_directory)?;
    let successor_writer_directory =
        canonical_existing_path(&successor_inputs.config.writer_directory)?;
    if predecessor_writer_directory == successor_writer_directory
        || predecessor_writer_directory.starts_with(&successor_writer_directory)
        || successor_writer_directory.starts_with(&predecessor_writer_directory)
    {
        return Err("FENCING_UNPROVEN: predecessor and successor need separate non-nested private writer directories".into());
    }

    let predecessor_input_bundle_digest =
        input_bundle_digest(&predecessor_config_file, &predecessor_inputs)?;
    let successor_config_digest = io::sha256(&successor_config_file)?;
    let successor_input_bundle_digest =
        input_bundle_digest(&successor_config_file, &successor_inputs)?;

    let plan = ProtectedSignerHandoffPlanV1 {
        api_version: PLAN_API.into(),
        request_digest: io::digest(request)?,
        predecessor_config_file,
        predecessor_config_digest,
        predecessor_input_bundle_digest,
        predecessor_recovery_digest: recovery_permission_digest(&predecessor_inputs)?,
        predecessor_writer_directory,
        predecessor_instance: predecessor.as_ref().clone(),
        successor_config_file,
        successor_config_digest,
        successor_input_bundle_digest,
        recovery_permission_digest: recovery_permission_digest(&successor_inputs)?
            .ok_or("FENCING_UNPROVEN: successor has no verified recovery permission")?,
        recovery_cut_position: successor_inputs
            .recovery
            .as_ref()
            .ok_or("FENCING_UNPROVEN: successor has no verified recovery permission")?
            .cut()
            .record()
            .position,
        successor_writer_directory,
        target_signer_public_key: target_signer.clone(),
    };

    // Validate both externally pinned endpoints before stopping the current
    // owner. This is a read-only proof check; it makes an invalid successor
    // configuration fail before any local lifecycle transition.
    restore_successor_endpoints(&successor_inputs, &plan.recovery_permission_digest)?;
    let owner = query_predecessor(&plan)?;
    if owner.state != "ready" {
        return Err(format!(
            "FENCING_UNPROVEN: exact predecessor owner is not ready ({})",
            owner.state
        ));
    }
    Ok(plan)
}

/// Revalidates the exact admitted source bundle. A changed key, trust file,
/// permission, config, or writer directory blocks progress without changing
/// the World epoch or silently accepting the new source.
pub(super) fn validate_plan_inputs(
    request: &Request,
    plan: &ProtectedSignerHandoffPlanV1,
) -> Result<ValidatedHandoffInputs, String> {
    validate_persisted_plan(request, plan)?;
    if absolute_existing_path(&request.config)? != plan.predecessor_config_file
        || io::sha256(&plan.predecessor_config_file)? != plan.predecessor_config_digest
    {
        return Err(
            "FENCING_UNPROVEN: predecessor config changed after operation admission".into(),
        );
    }
    let successor_path = match request.authorization.as_ref() {
        Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file,
            ..
        }) => absolute_existing_path(formal_live_config_file)?,
        _ => return Err("FENCING_UNPROVEN: protected transfer authorization missing".into()),
    };
    if successor_path != plan.successor_config_file
        || io::sha256(&successor_path)? != plan.successor_config_digest
    {
        return Err("FENCING_UNPROVEN: successor config changed after operation admission".into());
    }
    let predecessor = FormalLiveConfigV1::load(&plan.predecessor_config_file)?;
    let successor = FormalLiveConfigV1::load(&plan.successor_config_file)?;
    if input_bundle_digest(&plan.predecessor_config_file, &predecessor)?
        != plan.predecessor_input_bundle_digest
        || recovery_permission_digest(&predecessor)? != plan.predecessor_recovery_digest
        || input_bundle_digest(&plan.successor_config_file, &successor)?
            != plan.successor_input_bundle_digest
        || recovery_permission_digest(&successor)? != Some(plan.recovery_permission_digest.clone())
        || canonical_existing_path(&predecessor.config.writer_directory)?
            != plan.predecessor_writer_directory
        || canonical_existing_path(&successor.config.writer_directory)?
            != plan.successor_writer_directory
    {
        return Err("FENCING_UNPROVEN: formal trust, signer, recovery permission, or writer directory changed after admission".into());
    }
    validate_config_pair(
        request,
        &plan.predecessor_instance,
        &plan.target_signer_public_key,
        &predecessor,
        &successor,
    )?;
    Ok(ValidatedHandoffInputs {
        predecessor,
        successor,
    })
}

pub(super) fn validate_persisted_plan(
    request: &Request,
    plan: &ProtectedSignerHandoffPlanV1,
) -> Result<(), String> {
    let (successor_config, signer, predecessor) = match request.authorization.as_ref() {
        Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file,
            target_signer_public_key,
            expected_predecessor_instance: Some(predecessor),
        }) => (
            formal_live_config_file,
            target_signer_public_key,
            predecessor,
        ),
        _ => {
            return Err(
                "FENCING_UNPROVEN: stored request lacks exact protected transfer authorization"
                    .into(),
            );
        }
    };
    if plan.api_version != PLAN_API
        || plan.request_digest != io::digest(request)?
        || request.kind != Kind::ProtectedSignerMigration
        || request.managed_target.is_some()
        || request.migration.is_some()
        || request.expected_instance.is_some()
        || request.world_id != plan.predecessor_instance.world_id
        || request.chain_id != plan.predecessor_instance.chain_id
        || request.expected_generation != plan.predecessor_instance.authority_epoch
        || request.config != plan.predecessor_config_file
        || successor_config != &plan.successor_config_file
        || signer != &plan.target_signer_public_key
        || predecessor.as_ref() != &plan.predecessor_instance
        || plan.predecessor_instance.api_version != "oasis7.formal-runtime-owner/v1"
        || plan.predecessor_instance.operation_id.is_empty()
        || !is_owner_incarnation(&plan.predecessor_instance.incarnation)
        || !is_digest(&plan.predecessor_instance.request_digest)
        || plan.predecessor_instance.config_digest != request.expected_config_digest
        || plan.predecessor_instance.node_id.trim().is_empty()
        || !is_digest(&plan.predecessor_instance.authority_id)
        || plan.predecessor_instance.profile_version != 2
        || plan.predecessor_config_digest != request.expected_config_digest
        || !is_digest(&plan.predecessor_config_digest)
        || !is_digest(&plan.predecessor_input_bundle_digest)
        || !is_digest(&plan.successor_config_digest)
        || !is_digest(&plan.successor_input_bundle_digest)
        || !is_digest(&plan.recovery_permission_digest)
        || plan
            .predecessor_recovery_digest
            .as_ref()
            .is_some_and(|digest| !is_digest(digest))
        || !is_public_key(&plan.target_signer_public_key)
        || !plan.predecessor_config_file.is_absolute()
        || !plan.successor_config_file.is_absolute()
        || !plan.predecessor_writer_directory.is_absolute()
        || !plan.successor_writer_directory.is_absolute()
        || plan.predecessor_config_file == plan.successor_config_file
        || plan.predecessor_writer_directory == plan.successor_writer_directory
        || plan
            .predecessor_writer_directory
            .starts_with(&plan.successor_writer_directory)
        || plan
            .successor_writer_directory
            .starts_with(&plan.predecessor_writer_directory)
    {
        return Err(
            "FENCING_UNPROVEN: stored handoff plan does not bind the exact request and identities"
                .into(),
        );
    }
    Ok(())
}

fn validate_config_pair(
    request: &Request,
    predecessor: &FormalRuntimeInstanceV1,
    target_signer: &str,
    old: &ValidatedFormalLiveConfigV1,
    new: &ValidatedFormalLiveConfigV1,
) -> Result<(), String> {
    let old_authority = old.active_authority();
    let new_authority = new.active_authority();
    let old_authority_id = old_authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    let new_authority_id = new_authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    let recovery = new
        .recovery
        .as_ref()
        .ok_or("FENCING_UNPROVEN: successor has no externally verified recovery permission")?;
    let recovery_predecessor = recovery.active_predecessor();
    if old.config.node_id != predecessor.node_id
        || old_authority_id != predecessor.authority_id
        || old_authority.definition().epoch != predecessor.authority_epoch
        || old_authority.definition().profile_version != 2
        || old_authority.definition().world.world_id != request.world_id
        || old_authority.definition().world.chain_id != request.chain_id
        || new.config.node_id != old.config.node_id
        || new_authority.definition().profile_version != 2
        || new_authority.definition().epoch <= old_authority.definition().epoch
        || new_authority.definition().world != old_authority.definition().world
        || new_authority_id == old_authority_id
        || new_authority.definition().writer_public_key != target_signer
        || recovery_predecessor
            .authority_id()
            .map_err(|error| error.to_string())?
            != old_authority_id
        || recovery_predecessor.definition() != old_authority.definition()
        || recovery.recovery().successor().definition() != new_authority.definition()
        || recovery.cut().record().world != old_authority.definition().world
        || recovery.cut().record().position != new.anchor.head().record().position
        || recovery.cut().head() != new.anchor.head().head()
    {
        return Err("FENCING_UNPROVEN: predecessor, successor authority, recovery cut, or requested signer differs".into());
    }
    Ok(())
}

fn is_public_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_owner_incarnation(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn input_bundle_digest(
    config_file: &Path,
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<String, String> {
    let mut inputs = vec![
        ("formal_config", io::sha256(config_file)?),
        (
            "node_keypair",
            io::sha256(
                &validated
                    .config
                    .node_keypair_directory
                    .join(super::super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE),
            )?,
        ),
        ("writer_key", io::sha256(&validated.config.writer_key_file)?),
        (
            "external_formal_read_trust",
            io::sha256(&validated.config.formal_read_trust_file)?,
        ),
    ];
    if let Some(path) = validated.config.approved_initial_file.as_deref() {
        inputs.push(("approved_initial", io::sha256(path)?));
    }
    if let Some(path) = validated.config.recovery_permission_file.as_deref() {
        inputs.push(("recovery_permission", io::sha256(path)?));
    }
    if let Some(path) = validated.config.recovery_issuer_minimum_file.as_deref() {
        inputs.push(("recovery_issuer_minimum", io::sha256(path)?));
    }
    inputs.sort_by_key(|(name, _)| *name);
    io::digest(&("oasis7.formal-handoff-inputs/v1", inputs))
}

fn recovery_permission_digest(
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<Option<String>, String> {
    let Some(recovery) = validated.recovery.as_ref() else {
        return Ok(None);
    };
    Ok(Some(digest_verified_recovery(recovery, validated)?))
}

fn digest_verified_recovery(
    recovery: &VerifiedRecoveryBootstrapV1,
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<String, String> {
    io::digest(&(
        "oasis7.formal-handoff-recovery/v1",
        &validated.trust.checkpoint_digest,
        validated.anchor.checkpoint(),
        recovery.permission_evidence(),
        recovery.external_issuer_minimum(),
        recovery.cut().record(),
        recovery.cut().finalizing_authority_id(),
        recovery.active_predecessor().definition(),
        recovery.recovery().successor().definition(),
    ))
}

fn query_predecessor(
    plan: &ProtectedSignerHandoffPlanV1,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    let instance = &plan.predecessor_instance;
    let snapshot = formal_live_runtime::query_operation_owner(
        &plan.predecessor_config_file,
        &instance.operation_id,
        &instance.request_digest,
        &instance.config_digest,
        Some(&instance.incarnation),
    )?
    .ok_or("FENCING_UNPROVEN: exact predecessor owner receipt is absent")?;
    if snapshot.instance != *instance {
        return Err(
            "FENCING_UNPROVEN: predecessor owner response differs from exact admitted instance"
                .into(),
        );
    }
    Ok(snapshot)
}

fn query_successor(
    plan: &ProtectedSignerHandoffPlanV1,
    request: &Request,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    formal_live_runtime::query_operation_owner(
        &plan.successor_config_file,
        &request.operation_id,
        &plan.request_digest,
        &plan.successor_config_digest,
        None,
    )
}

/// Query first; stop only the exact admitted incarnation. A lost stop reply is
/// resolved by its durable owner state and never by process id or socket deletion.
fn stop_predecessor(
    plan: &ProtectedSignerHandoffPlanV1,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    let before = query_predecessor(plan)?;
    if before.state == "stopped" {
        return Ok(before);
    }
    if before.state != "ready" {
        return Err(format!(
            "ORIGINAL_OUTCOME_UNKNOWN: predecessor owner state is {}",
            before.state
        ));
    }
    let _ = formal_live_runtime::stop_operation_owner(
        &plan.predecessor_config_file,
        &plan.predecessor_instance,
        timeout.min(Duration::from_secs(60)),
    );
    let after = query_predecessor(plan)?;
    if after.state != "stopped" {
        return Err(format!(
            "ORIGINAL_OUTCOME_UNKNOWN: predecessor stop readback is {}",
            after.state
        ));
    }
    Ok(after)
}

fn launch_successor(
    plan: &ProtectedSignerHandoffPlanV1,
    request: &Request,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    let _ = formal_live_runtime::launch_operation_owner(
        &plan.successor_config_file,
        &request.operation_id,
        &plan.request_digest,
        &plan.successor_config_digest,
        timeout.min(Duration::from_secs(60)),
    );
    let snapshot = query_successor(plan, request)?
        .ok_or("ORIGINAL_OUTCOME_UNKNOWN: successor owner record is absent after launch attempt")?;
    if snapshot.state != "ready" {
        return Err(format!(
            "ORIGINAL_OUTCOME_UNKNOWN: successor owner readback is {}",
            snapshot.state
        ));
    }
    Ok(snapshot)
}

/// Independently restores both signed successor endpoints after process start.
/// The receipt is constructed only from typed native restore tokens, never from
/// process-owner status.
fn verify_successor_readback(
    request: &Request,
    validated: &ValidatedFormalLiveConfigV1,
    plan: &ProtectedSignerHandoffPlanV1,
    owner: &FormalRuntimeOwnerSnapshotV1,
) -> Result<FormalHandoffReadbackV1, String> {
    let authority = validated.active_authority().clone();
    let authority_id = authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    if owner.state != "ready"
        || owner.instance.operation_id != request.operation_id
        || owner.instance.request_digest != plan.request_digest
        || owner.instance.config_digest != plan.successor_config_digest
        || owner.instance.node_id != validated.config.node_id
        || owner.instance.world_id != authority.definition().world.world_id
        || owner.instance.chain_id != authority.definition().world.chain_id
        || owner.instance.authority_id != authority_id
        || owner.instance.authority_epoch != authority.definition().epoch
        || owner.instance.profile_version != 2
    {
        return Err("FENCING_UNPROVEN: successor runtime owner does not match the validated recovery authority".into());
    }
    readback::verify_successor_published_cut(
        validated,
        &plan.recovery_permission_digest,
        &plan.predecessor_writer_directory,
    )
}

fn restore_successor_endpoints(
    validated: &ValidatedFormalLiveConfigV1,
    expected_recovery_permission_digest: &str,
) -> Result<FormalHandoffReadbackV1, String> {
    let recovery = validated
        .recovery
        .as_ref()
        .ok_or("FENCING_UNPROVEN: successor recovery permission disappeared")?;
    let authority = validated.active_authority().clone();
    let authority_id = authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    let minimum = validated.anchor.head().head().clone();
    let endpoint = |address, role| {
        RemoteFormalEndpoint::new(
            address,
            validated.timeout,
            authority.clone(),
            role,
            validated.writer_key.clone(),
            minimum.clone(),
        )
        .map_err(|error| format!("successor formal {role:?} endpoint: {error}"))
    };
    let primary = endpoint(validated.config.primary, EndpointRole::Primary)?
        .restore_native_current(
            recovery.original_authority(),
            Some(recovery.recovery()),
            Some(recovery.cut()),
        )
        .map_err(|error| format!("successor primary native restore: {error}"))?;
    let replica = endpoint(validated.config.replica, EndpointRole::Replica)?
        .restore_native_current(
            recovery.original_authority(),
            Some(recovery.recovery()),
            Some(recovery.cut()),
        )
        .map_err(|error| format!("successor replica native restore: {error}"))?;
    let primary_commit = primary.commit();
    let replica_commit = replica.commit();
    let primary_head = primary_commit.head();
    let replica_head = replica_commit.head();
    if primary
        .active_authority()
        .authority_id()
        .map_err(|error| error.to_string())?
        != authority_id
        || replica
            .active_authority()
            .authority_id()
            .map_err(|error| error.to_string())?
            != authority_id
        || primary.minimum() != &minimum
        || replica.minimum() != &minimum
        || primary_head != replica_head
        || primary_commit.record() != replica_commit.record()
        || primary_commit.finalizing_authority_id() != replica_commit.finalizing_authority_id()
        || primary.candidate() != replica.candidate()
        || primary.proof() != replica.proof()
        || primary.pending() != replica.pending()
        || !primary.checkpoint().head.qualified
        || !replica.checkpoint().head.qualified
        || primary.checkpoint().head.position != primary_head.position
        || replica.checkpoint().head.position != replica_head.position
        || primary.checkpoint().head.decision_hash != primary_head.record_id
        || replica.checkpoint().head.decision_hash != replica_head.record_id
        || primary_head.position < recovery.cut().record().position
    {
        return Err("FENCING_UNPROVEN: independently restored successor endpoints differ or lack a qualified head".into());
    }
    let record = primary_commit.record();
    if record.world != authority.definition().world
        || record.position != primary_head.position
        || record.record_id().map_err(|error| error.to_string())? != primary_head.record_id
        || record.after_state_root.is_empty()
    {
        return Err("FENCING_UNPROVEN: successor qualified record identity is incomplete".into());
    }
    let primary_checkpoint_digest = io::digest(primary.checkpoint())?;
    let replica_checkpoint_digest = io::digest(replica.checkpoint())?;
    let qualified_evidence_digest = io::digest(&(
        record,
        primary.candidate(),
        primary.proof(),
        primary_commit.finalizing_authority_id(),
        &primary_head,
        &primary_checkpoint_digest,
        &replica_checkpoint_digest,
    ))?;
    let recovery_permission_digest = recovery_permission_digest(validated)?
        .ok_or("FENCING_UNPROVEN: successor recovery permission is absent")?;
    if recovery_permission_digest != expected_recovery_permission_digest {
        return Err(
            "FENCING_UNPROVEN: successor recovery permission differs from admission".into(),
        );
    }
    Ok(FormalHandoffReadbackV1 {
        active_authority_id: authority_id,
        active_authority_epoch: authority.definition().epoch,
        profile_version: authority.definition().profile_version,
        qualified_position: primary_head.position,
        qualified_record_id: primary_head.record_id,
        after_state_root: record.after_state_root.clone(),
        primary_checkpoint_digest,
        replica_checkpoint_digest,
        qualified_evidence_digest,
        recovery_permission_digest,
    })
}

/// Executes one query-first reconciliation pass. The caller persists the
/// admission plan before calling this function; each process effect is then
/// preceded by a durable stage update. Resume after dispatch never relaunches
/// an owner when its immutable receipt is absent or ambiguous.
pub(super) fn resume(
    store: &mut super::store::Store,
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
    if record.request.kind != Kind::ProtectedSignerMigration {
        return Err("protected handoff executor received a different operation kind".into());
    }
    let Some(plan) = record.protected_handoff_plan.clone() else {
        return Err(
            "ORIGINAL_OUTCOME_UNKNOWN: protected handoff has no immutable admission plan".into(),
        );
    };
    if let Err(error) = validate_plan_inputs(&record.request, &plan) {
        record.state = if record.dispatched {
            "reconciling"
        } else {
            "blocked"
        }
        .into();
        record.blocker = Some(bounded_reason(&error));
        return persist(store, record);
    }
    if record.stage == "prepared" {
        record.state = "running".into();
        record.stage = "predecessor_stopping".into();
        record.dispatched = true;
        record.blocker = None;
        record = persist(store, record)?;
    }
    let inputs = validate_plan_inputs(&record.request, &plan)?;
    let timeout = inputs.predecessor.timeout.min(inputs.successor.timeout);
    if record.stage == "predecessor_stopping" {
        match stop_predecessor(&plan, timeout) {
            Ok(_) => {
                record.state = "running".into();
                record.stage = "successor_preflight".into();
                record.blocker = None;
                record = persist(store, record)?;
            }
            Err(error) => return persist_unknown(store, record, error),
        }
    }
    if record.stage == "successor_preflight" {
        match query_predecessor(&plan) {
            Ok(snapshot) if snapshot.state == "stopped" => {}
            Ok(snapshot) => {
                return persist_unknown(
                    store,
                    record,
                    format!(
                        "FENCING_UNPROVEN: predecessor stop readback is {}",
                        snapshot.state
                    ),
                );
            }
            Err(error) => return persist_unknown(store, record, error),
        }
        if let Err(error) =
            restore_successor_endpoints(&inputs.successor, &plan.recovery_permission_digest)
        {
            return persist_unknown(store, record, error);
        }
        match query_successor(&plan, &record.request) {
            Ok(Some(snapshot)) if snapshot.state == "ready" => {
                if !successor_owner_matches(&snapshot, &record.request, &plan, &inputs.successor)? {
                    return persist_unknown(
                        store,
                        record,
                        "FENCING_UNPROVEN: existing successor owner differs from the exact request"
                            .into(),
                    );
                }
                record.state = "running".into();
                record.stage = "successor_running".into();
                record.blocker = None;
                record = persist(store, record)?;
            }
            Ok(Some(snapshot)) => {
                return persist_unknown(
                    store,
                    record,
                    format!(
                        "ORIGINAL_OUTCOME_UNKNOWN: exact successor owner state is {}",
                        snapshot.state
                    ),
                );
            }
            Ok(None) => {
                record.state = "running".into();
                record.stage = "successor_starting".into();
                record.dispatched = true;
                record.blocker = None;
                record = persist(store, record)?;
                match launch_successor(&plan, &record.request, timeout) {
                    Ok(snapshot)
                        if successor_owner_matches(
                            &snapshot,
                            &record.request,
                            &plan,
                            &inputs.successor,
                        )? =>
                    {
                        record.stage = "successor_running".into();
                        record.blocker = None;
                        record = persist(store, record)?;
                    }
                    Ok(_) => {
                        return persist_unknown(
                            store,
                            record,
                            "FENCING_UNPROVEN: successor owner identity differs after launch"
                                .into(),
                        );
                    }
                    Err(error) => return persist_unknown(store, record, error),
                }
            }
            Err(error) => return persist_unknown(store, record, error),
        }
    } else if record.stage == "successor_starting" {
        match query_successor(&plan, &record.request) {
            Ok(Some(snapshot)) if snapshot.state == "ready" => {
                if !successor_owner_matches(&snapshot, &record.request, &plan, &inputs.successor)? {
                    return persist_unknown(store, record, "FENCING_UNPROVEN: successor owner identity differs from admission".into());
                }
                record.state = "running".into();
                record.stage = "successor_running".into();
                record.blocker = None;
                record = persist(store, record)?;
            }
            Ok(Some(snapshot)) => {
                return persist_unknown(
                    store,
                    record,
                    format!("ORIGINAL_OUTCOME_UNKNOWN: successor owner state is {}", snapshot.state),
                )
            }
            Ok(None) => {
                return persist_unknown(store, record, "ORIGINAL_OUTCOME_UNKNOWN: successor launch was durably dispatched but no immutable owner receipt exists; query only".into())
            }
            Err(error) => return persist_unknown(store, record, error),
        }
    }
    if record.stage != "successor_running" {
        return persist_unknown(
            store,
            record,
            "ORIGINAL_OUTCOME_UNKNOWN: unsupported protected handoff stage; query only".into(),
        );
    }
    let owner = match query_successor(&plan, &record.request) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return persist_unknown(
                store,
                record,
                "ORIGINAL_OUTCOME_UNKNOWN: successor owner receipt disappeared; query only".into(),
            );
        }
        Err(error) => return persist_unknown(store, record, error),
    };
    if !successor_owner_matches(&owner, &record.request, &plan, &inputs.successor)? {
        return persist_unknown(
            store,
            record,
            "FENCING_UNPROVEN: successor owner identity changed before formal readback".into(),
        );
    }
    let readback =
        match verify_successor_readback(&record.request, &inputs.successor, &plan, &owner) {
            Ok(readback) => readback,
            Err(error) => return persist_unknown(store, record, error),
        };
    let stopped = match query_predecessor(&plan) {
        Ok(snapshot) if snapshot.state == "stopped" => snapshot,
        Ok(snapshot) => {
            return persist_unknown(
                store,
                record,
                format!(
                    "FENCING_UNPROVEN: predecessor is not confirmed stopped ({})",
                    snapshot.state
                ),
            );
        }
        Err(error) => return persist_unknown(store, record, error),
    };
    let _ = stopped;
    record.formal_handoff = Some(build_receipt(&record, &plan, &owner, &readback)?);
    record.state = "succeeded".into();
    record.stage = "successor_qualified_readback_verified".into();
    record.blocker = None;
    persist(store, record)
}

pub(super) fn cancel(
    store: &mut super::store::Store,
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
    record.cancel_requested = true;
    if record.dispatched {
        record.state = "reconciling".into();
        record.blocker = Some("CANCELLATION_AFTER_DISPATCH: protected handoff cannot be rolled back; resume queries the exact predecessor and successor owners".into());
    } else {
        record.state = "cancelled".into();
        record.stage = "prepared_cancelled".into();
        record.blocker = None;
    }
    persist(store, record)
}

/// Structural store validation. It does not treat a persisted receipt as
/// fresh protocol evidence; only `resume` can create it after native restores.
pub(super) fn validate_persisted_record(record: &Record) -> Result<(), String> {
    if record.request.kind != Kind::ProtectedSignerMigration {
        if record.protected_handoff_plan.is_some() || record.formal_handoff.is_some() {
            return Err("non-handoff operation contains protected handoff material".into());
        }
        return Ok(());
    }
    let plan = record
        .protected_handoff_plan
        .as_ref()
        .ok_or("protected handoff record omits its immutable admission plan")?;
    validate_persisted_plan(&record.request, plan)?;
    if !record.results.is_empty()
        || !record.remote_results.is_empty()
        || record.migration_receipt.is_some()
        || record.migration_capacity_plan.is_some()
        || !record.action_attempts.is_empty()
        || record.recovered
    {
        return Err(
            "protected handoff contains unrelated local, remote, or migration evidence".into(),
        );
    }
    let state_stage_valid = matches!(
        (
            record.state.as_str(),
            record.stage.as_str(),
            record.dispatched,
            record.formal_handoff.is_some(),
        ),
        ("queued", "prepared", false, false)
            | ("blocked", "prepared", false, false)
            | ("cancelled", "prepared_cancelled", false, false)
            | (
                "running" | "reconciling",
                "predecessor_stopping",
                true,
                false
            )
            | (
                "running" | "reconciling",
                "successor_preflight",
                true,
                false
            )
            | ("running" | "reconciling", "successor_starting", true, false)
            | ("running" | "reconciling", "successor_running", true, false)
            | (
                "succeeded",
                "successor_qualified_readback_verified",
                true,
                true
            )
    );
    if !state_stage_valid {
        return Err("protected handoff state/stage does not match its durable transition".into());
    }
    if (record.state == "queued" && record.cancel_requested)
        || (record.state == "cancelled" && !record.cancel_requested)
    {
        return Err("protected handoff cancellation flag differs from its state".into());
    }
    match (
        &record.formal_handoff,
        record.state.as_str(),
        record.stage.as_str(),
    ) {
        (None, "succeeded", _) | (Some(_), _, _) => {
            let receipt = record
                .formal_handoff
                .as_ref()
                .ok_or("successful handoff has no verified receipt")?;
            let (successor_config, signer, _) = match record.request.authorization.as_ref() {
                Some(OperationAuthorizationV1::ProtectedSignerTransfer {
                    formal_live_config_file,
                    target_signer_public_key,
                    expected_predecessor_instance: Some(_),
                }) => (formal_live_config_file, target_signer_public_key, ()),
                _ => {
                    return Err(
                        "protected handoff receipt has no exact request authorization".into(),
                    );
                }
            };
            if record.state != "succeeded"
                || record.stage != "successor_qualified_readback_verified"
                || !record.dispatched
                || receipt.api_version != RECEIPT_API
                || receipt.operation_id != record.request.operation_id
                || receipt.request_digest != record.request_digest
                || receipt.predecessor_instance != plan.predecessor_instance
                || !receipt.predecessor_stopped
                || receipt.successor_instance.operation_id != record.request.operation_id
                || receipt.successor_instance.request_digest != record.request_digest
                || receipt.successor_instance.api_version != "oasis7.formal-runtime-owner/v1"
                || !is_owner_incarnation(&receipt.successor_instance.incarnation)
                || receipt.successor_instance.config_digest != plan.successor_config_digest
                || receipt.successor_instance.node_id != plan.predecessor_instance.node_id
                || receipt.successor_instance.world_id != record.request.world_id
                || receipt.successor_instance.chain_id != record.request.chain_id
                || receipt.successor_instance.authority_epoch
                    <= plan.predecessor_instance.authority_epoch
                || receipt.successor_instance.profile_version != 2
                || !is_digest(&receipt.successor_instance.authority_id)
                || receipt.active_authority_id != receipt.successor_instance.authority_id
                || !is_digest(&receipt.active_authority_id)
                || receipt.active_authority_epoch != receipt.successor_instance.authority_epoch
                || receipt.profile_version != 2
                || receipt.qualified_position < plan.recovery_cut_position
                || !is_digest(&receipt.qualified_record_id)
                || !is_digest(&receipt.after_state_root)
                || !is_digest(&receipt.primary_checkpoint_digest)
                || !is_digest(&receipt.replica_checkpoint_digest)
                || !is_digest(&receipt.qualified_evidence_digest)
                || receipt.recovery_permission_digest != plan.recovery_permission_digest
                || receipt.recovery_permission_digest.is_empty()
                || successor_config != &plan.successor_config_file
                || signer != &plan.target_signer_public_key
            {
                return Err("protected handoff receipt differs from the exact admitted request or verified readback".into());
            }
        }
        (None, _, _) => {}
    }
    Ok(())
}

fn successor_owner_matches(
    snapshot: &FormalRuntimeOwnerSnapshotV1,
    request: &Request,
    plan: &ProtectedSignerHandoffPlanV1,
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<bool, String> {
    let authority = validated.active_authority();
    Ok(snapshot.state == "ready"
        && snapshot.api_version == "oasis7.formal-runtime-owner/v1"
        && snapshot.instance.api_version == "oasis7.formal-runtime-owner/v1"
        && snapshot.instance.operation_id == request.operation_id
        && snapshot.instance.request_digest == plan.request_digest
        && snapshot.instance.config_digest == plan.successor_config_digest
        && snapshot.instance.node_id == validated.config.node_id
        && snapshot.instance.world_id == request.world_id
        && snapshot.instance.chain_id == request.chain_id
        && snapshot.instance.authority_id
            == authority
                .authority_id()
                .map_err(|error| error.to_string())?
        && snapshot.instance.authority_epoch == authority.definition().epoch
        && is_owner_incarnation(&snapshot.instance.incarnation)
        && snapshot.instance.profile_version == 2)
}

fn build_receipt(
    record: &Record,
    plan: &ProtectedSignerHandoffPlanV1,
    successor: &FormalRuntimeOwnerSnapshotV1,
    readback: &FormalHandoffReadbackV1,
) -> Result<super::model::ProtectedSignerHandoffReceiptV1, String> {
    let predecessor = query_predecessor(plan)?;
    if predecessor.state != "stopped"
        || successor.state != "ready"
        || successor.instance.operation_id != record.request.operation_id
        || successor.instance.request_digest != record.request_digest
        || successor.instance.config_digest != plan.successor_config_digest
        || readback.profile_version != 2
        || readback.active_authority_id != successor.instance.authority_id
        || readback.active_authority_epoch != successor.instance.authority_epoch
        || readback.recovery_permission_digest != plan.recovery_permission_digest
        || !is_digest(&readback.primary_checkpoint_digest)
        || !is_digest(&readback.replica_checkpoint_digest)
        || !is_digest(&readback.qualified_evidence_digest)
    {
        return Err("FENCING_UNPROVEN: final protected handoff readback does not match exact owner receipts".into());
    }
    Ok(super::model::ProtectedSignerHandoffReceiptV1 {
        api_version: RECEIPT_API.into(),
        operation_id: record.request.operation_id.clone(),
        request_digest: record.request_digest.clone(),
        predecessor_instance: plan.predecessor_instance.clone(),
        successor_instance: successor.instance.clone(),
        predecessor_stopped: true,
        active_authority_id: readback.active_authority_id.clone(),
        active_authority_epoch: readback.active_authority_epoch,
        profile_version: readback.profile_version,
        qualified_position: readback.qualified_position,
        qualified_record_id: readback.qualified_record_id.clone(),
        after_state_root: readback.after_state_root.clone(),
        primary_checkpoint_digest: readback.primary_checkpoint_digest.clone(),
        replica_checkpoint_digest: readback.replica_checkpoint_digest.clone(),
        qualified_evidence_digest: readback.qualified_evidence_digest.clone(),
        recovery_permission_digest: readback.recovery_permission_digest.clone(),
    })
}

fn persist_unknown(
    store: &mut super::store::Store,
    mut record: Record,
    reason: String,
) -> Result<Record, String> {
    record.state = if record.dispatched {
        "reconciling"
    } else {
        "blocked"
    }
    .into();
    record.blocker = Some(bounded_reason(&reason));
    persist(store, record)
}

fn persist(store: &mut super::store::Store, record: Record) -> Result<Record, String> {
    let revision = record.revision;
    store.cas(record, revision)
}

fn bounded_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|character| !character.is_control())
        .take(1024)
        .collect()
}

fn absolute_existing_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("formal handoff requires absolute config paths".into());
    }
    let absolute = path.to_path_buf();
    io::reject_symlinks(&absolute)?;
    if !absolute.is_file() {
        return Err(format!(
            "formal handoff config is not a regular file: {}",
            absolute.display()
        ));
    }
    super::super::controlled_live_config::require_private_owned(&absolute, false)?;
    Ok(absolute)
}

fn canonical_existing_path(path: &Path) -> Result<PathBuf, String> {
    io::reject_symlinks(path)?;
    std::fs::canonicalize(path).map_err(|error| error.to_string())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "protected_handoff_tests.rs"]
mod tests;
