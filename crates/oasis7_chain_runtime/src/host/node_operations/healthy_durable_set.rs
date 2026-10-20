//! Operating-domain coordination for healthy replica replacement.
//! Local progress is a retry cursor; only opaque dual native verification can
//! authorize activation or completion.
use super::super::{formal_live_runtime::FormalRuntimeInstanceV1, node_lifecycle::local_io as io};
use oasis7_distfs::controlled_authority::formal::{
    EndpointSignerBindingV1, SignedNormalDurableSetTransitionV1,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HealthyDurableSetAuthorizationV1 {
    pub expected_owner: FormalRuntimeInstanceV1,
    pub successor_runtime_config_file: PathBuf,
    pub successor_replica: EndpointSignerBindingV1,
    pub primary_management: SocketAddr,
    pub replica_management: SocketAddr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HealthyStageV1 {
    Prepared,
    PreparingArm,
    WaitingQualifiedBoundary,
    StoppingOldOwner,
    SealingOldPair,
    WaitingExternalCandidates,
    InstallingCandidates,
    ActivatingCandidates,
    WaitingFreshNormalBoundary,
    PublishingMarker,
    ReopeningOriginalC0,
    WaitingFirstNormalQualified,
    Completed,
    Cancelled,
}
impl HealthyStageV1 {
    pub(super) fn cancellation_allowed(self) -> bool {
        self == Self::Prepared
    }
    pub(super) fn ordinal(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::PreparingArm => 1,
            Self::WaitingQualifiedBoundary => 2,
            Self::StoppingOldOwner => 3,
            Self::SealingOldPair => 4,
            Self::WaitingExternalCandidates => 5,
            Self::InstallingCandidates => 6,
            Self::ActivatingCandidates => 7,
            Self::WaitingFreshNormalBoundary => 8,
            Self::PublishingMarker => 9,
            Self::ReopeningOriginalC0 => 10,
            Self::WaitingFirstNormalQualified => 11,
            Self::Completed => 12,
            Self::Cancelled => 13,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HealthyDurableSetPlanV1 {
    pub api_version: String,
    pub request_digest: String,
    pub core_operation_id: String,
    pub predecessor_config_file: PathBuf,
    pub predecessor_config_digest: String,
    pub successor_config_digest: String,
    pub writer_directory: PathBuf,
    pub authority_context_digest: String,
    pub external_initial_pin_digest: String,
    pub external_read_trust_digest: String,
    pub authorization: HealthyDurableSetAuthorizationV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HealthyDurableSetProgressV1 {
    pub api_version: String,
    pub plan_digest: String,
    pub stage: HealthyStageV1,
    pub transition: Option<SignedNormalDurableSetTransitionV1>,
    pub activation_step:
        Option<oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1>,
    pub successor_owner: Option<FormalRuntimeInstanceV1>,
    pub blocker: Option<String>,
    pub boundary_record_id: Option<String>,
    pub old_seals:
        [Option<oasis7_distfs::controlled_authority::formal::fencing::EpochSealReceiptV1>; 2],
    pub installs:
        [Option<oasis7_distfs::controlled_authority::formal::DurableSetInstallReceiptV1>; 2],
    pub normal_readback: Option<HealthyNormalReadbackV1>,
}
impl HealthyDurableSetProgressV1 {
    pub(super) fn new(plan: &HealthyDurableSetPlanV1) -> Result<Self, String> {
        Ok(Self {
            api_version: "oasis7.healthy-durable-set-progress/v1".into(),
            plan_digest: io::digest(plan)?,
            stage: HealthyStageV1::Prepared,
            transition: None,
            activation_step: None,
            successor_owner: None,
            blocker: None,
            boundary_record_id: None,
            old_seals: [None, None],
            installs: [None, None],
            normal_readback: None,
        })
    }
    pub(super) fn validate(&self, plan: &HealthyDurableSetPlanV1) -> Result<(), String> {
        if self.api_version != "oasis7.healthy-durable-set-progress/v1"
            || self.plan_digest != io::digest(plan)?
        {
            return Err("healthy operation progress differs from immutable admission plan".into());
        }
        if let Some(signed) = &self.transition {
            let mut expected_replica = plan.authorization.successor_replica.clone();
            expected_replica.instance.authority_epoch = signed.body.successor.epoch;
            if signed.body.operation_id != plan.core_operation_id
                || signed.body.successor.replica != expected_replica
            {
                return Err(
                    "healthy operation transition differs from exact operation/replica".into(),
                );
            }
        } else if self.stage.ordinal() >= HealthyStageV1::WaitingQualifiedBoundary.ordinal()
            && self.stage != HealthyStageV1::Cancelled
        {
            return Err("healthy operation stage lacks original signed transition".into());
        }
        if let Some(step) = &self.activation_step {
            if self.transition.as_ref() != Some(&step.transition) {
                return Err("healthy operation activation differs from original transition".into());
            }
        } else if self.stage.ordinal() >= HealthyStageV1::ActivatingCandidates.ordinal()
            && self.stage != HealthyStageV1::Cancelled
        {
            return Err("healthy operation stage lacks dual install evidence".into());
        }
        if let Some(owner) = &self.successor_owner
            && (owner.incarnation == plan.authorization.expected_owner.incarnation
                || owner.config_digest != plan.successor_config_digest
                || owner.world_id != plan.authorization.expected_owner.world_id
                || owner.chain_id != plan.authorization.expected_owner.chain_id)
        {
            return Err("healthy operation reopened owner identity mismatch".into());
        }
        Ok(())
    }
}

pub(super) fn prepare_plan(
    request_digest: &str,
    predecessor_config_file: &std::path::Path,
    authorization: HealthyDurableSetAuthorizationV1,
) -> Result<HealthyDurableSetPlanV1, String> {
    use super::super::{formal_live_config::FormalLiveConfigV1, formal_live_runtime};
    if request_digest.len() != 64
        || !request_digest.bytes().all(|b| b.is_ascii_hexdigit())
        || !predecessor_config_file.is_absolute()
        || !authorization.successor_runtime_config_file.is_absolute()
        || authorization.primary_management == authorization.replica_management
    {
        return Err(
            "healthy replica replacement requires exact request/config/management identities"
                .into(),
        );
    }
    for path in [
        predecessor_config_file,
        authorization.successor_runtime_config_file.as_path(),
    ] {
        io::reject_symlinks(path)?;
        super::super::controlled_live_config::require_private_owned(path, false)?;
    }
    let predecessor = FormalLiveConfigV1::load(predecessor_config_file)?;
    let successor = FormalLiveConfigV1::load(&authorization.successor_runtime_config_file)?;
    if predecessor.recovery.is_some() || successor.recovery.is_some() {
        return Err("healthy adapter currently requires original C0 normal lineage, not recovery-origin authority".into());
    }
    // Compare complete resolved configurations, permitting exactly one network
    // route update. The fixed external trust, writer key, C0 and P stay bound.
    let mut successor_config = successor.config.clone();
    successor_config.replica = predecessor.config.replica;
    if io::digest(&successor_config)? != io::digest(&predecessor.config)?
        || successor.config.writer_directory != predecessor.config.writer_directory
        || successor.config.primary != predecessor.config.primary
        || successor.trust.checkpoint_digest != predecessor.trust.checkpoint_digest
        || io::digest(&successor.trust)? != io::digest(&predecessor.trust)?
        || successor.writer_key.to_bytes() != predecessor.writer_key.to_bytes()
    {
        return Err(
            "healthy replacement must preserve complete C0/writer/P/fixed trust configuration"
                .into(),
        );
    }
    let predecessor_config_digest = io::sha256(predecessor_config_file)?;
    let (_, startup) = super::super::formal_live_history::from_validated_for_runtime(
        &predecessor,
        formal_live_runtime::business_reader_for(&predecessor)?,
    )?;
    let old = startup.anchor.authority();
    let owner = formal_live_runtime::current_operation_owner(predecessor_config_file)?;
    if owner.state != "ready"
        || owner.instance != authorization.expected_owner
        || owner.instance.config_digest != predecessor_config_digest
        || owner.instance.authority_id != old.authority_id().map_err(|e| e.to_string())?
    {
        return Err("healthy replacement requires the exact ready formal owner".into());
    }
    let target = &authorization.successor_replica;
    target.instance.validate().map_err(|e| e.to_string())?;
    if target.instance.world_id != old.definition().world.world_id
        || target.instance.chain_id != old.definition().world.chain_id
        || target.instance.genesis_digest != old.definition().world.genesis_digest
    {
        return Err("healthy target replica world/chain/genesis binding mismatch".into());
    }
    if target.instance.endpoint_role
        != oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica
        || target.instance.endpoint_id == old.definition().primary.instance.endpoint_id
        || target.instance.endpoint_id == old.definition().replica.instance.endpoint_id
        || target.fault_domain_id == old.definition().primary.fault_domain_id
    {
        return Err("healthy replacement requires a distinct replica and independent declared failure domain".into());
    }
    let core_operation_id = io::digest(&("oasis7.healthy-operation.v1", request_digest))?;
    Ok(HealthyDurableSetPlanV1 {
        api_version: "oasis7.healthy-durable-set-plan/v1".into(),
        request_digest: request_digest.into(),
        core_operation_id,
        predecessor_config_file: predecessor_config_file.into(),
        predecessor_config_digest,
        successor_config_digest: io::sha256(&authorization.successor_runtime_config_file)?,
        writer_directory: predecessor.config.writer_directory.clone(),
        authority_context_digest: old.context_digest().map_err(|e| e.to_string())?,
        external_initial_pin_digest: predecessor.trust.checkpoint_digest.clone(),
        external_read_trust_digest: io::digest(&predecessor.trust)?,
        authorization,
    })
}

/// Fresh normal-port evidence. This function never consumes an installed flag
/// or local progress as authority; activation is an opaque dual-signed token.
pub(super) fn fresh_normal_readback(
    validated: &super::super::formal_live_config::ValidatedFormalLiveConfigV1,
    activation: &oasis7_distfs::controlled_authority::formal::VerifiedDurableSetActivationV1,
    require_new_normal_execution: bool,
) -> Result<HealthyNormalReadbackV1, String> {
    use oasis7_distfs::controlled_authority::{
        formal::{FormalCompletionBranchV1, FormalRecordKindV1, transport::RemoteFormalEndpoint},
        replicated_protocol::EndpointRole,
    };
    let boundary = activation.boundary();
    let make = |address, role| {
        RemoteFormalEndpoint::new(
            address,
            validated.timeout,
            activation.successor().clone(),
            role,
            validated.writer_key.clone(),
            boundary.head(),
        )
        .map_err(|e| e.to_string())
    };
    let mut primary = make(validated.config.primary, EndpointRole::Primary)?;
    let mut replica = make(validated.config.replica, EndpointRole::Replica)?;
    if require_new_normal_execution {
        let p = primary
            .restore_first_normal_with_durable_set(validated.anchor.authority(), activation, None)
            .map_err(|e| format!("fresh successor primary first Normal historical read: {e}"))?;
        let r = replica
            .restore_first_normal_with_durable_set(
                validated.anchor.authority(),
                activation,
                Some(&p.reference()),
            )
            .map_err(|e| {
                format!("fresh successor replica exact first Normal historical read: {e}")
            })?;
        if p.commit().record() != r.commit().record()
            || p.proof() != r.proof()
            || p.authority().context_digest().map_err(|e| e.to_string())?
                != activation
                    .successor()
                    .context_digest()
                    .map_err(|e| e.to_string())?
            || r.authority().context_digest().map_err(|e| e.to_string())?
                != activation
                    .successor()
                    .context_digest()
                    .map_err(|e| e.to_string())?
        {
            return Err("dual fresh endpoints disagree on exact first Normal qualified cut".into());
        }
        return Ok(HealthyNormalReadbackV1 {
            position: p.commit().record().position,
            record_id: p.reference().record_id,
            after_state_root: p.commit().record().after_state_root.clone(),
            authority_id: activation
                .successor()
                .authority_id()
                .map_err(|e| e.to_string())?,
            primary_checkpoint_digest: io::digest(p.checkpoint())?,
            replica_checkpoint_digest: io::digest(r.checkpoint())?,
            qualified_evidence_digest: io::digest(p.proof())?,
        });
    }
    let p = primary
        .restore_native_current_with_durable_set(validated.anchor.authority(), activation, None)
        .map_err(|e| format!("fresh successor primary native read: {e}"))?;
    let r = replica
        .restore_native_current_with_durable_set(validated.anchor.authority(), activation, None)
        .map_err(|e| format!("fresh successor replica native read: {e}"))?;
    if p.pending().is_some()
        || r.pending().is_some()
        || p.commit().record() != r.commit().record()
        || p.proof() != r.proof()
        || p.active_authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
        || r.active_authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
    {
        return Err(
            "dual fresh normal native endpoints do not expose the same complete qualified cut"
                .into(),
        );
    }
    if require_new_normal_execution {
        if p.commit().record().position <= boundary.record().position
            || p.commit().record().kind != FormalRecordKindV1::Execution
            || p.proof().decision.branch != FormalCompletionBranchV1::Normal
        {
            return Err(
                "waiting for first new Normal P/R qualified execution beyond healthy H".into(),
            );
        }
    } else if p.commit().record() != boundary.record() {
        return Err("fresh normal endpoint activation readback differs from exact H".into());
    }
    Ok(HealthyNormalReadbackV1 {
        position: p.commit().record().position,
        record_id: p.commit().head().record_id,
        after_state_root: p.commit().record().after_state_root.clone(),
        authority_id: activation
            .successor()
            .authority_id()
            .map_err(|e| e.to_string())?,
        primary_checkpoint_digest: io::digest(p.checkpoint())?,
        replica_checkpoint_digest: io::digest(r.checkpoint())?,
        qualified_evidence_digest: io::digest(p.proof())?,
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HealthyNormalReadbackV1 {
    pub position: u64,
    pub record_id: String,
    pub after_state_root: String,
    pub authority_id: String,
    pub primary_checkpoint_digest: String,
    pub replica_checkpoint_digest: String,
    pub qualified_evidence_digest: String,
}

pub(super) fn plan(request: super::model::Request) -> Result<super::model::Record, String> {
    use super::model::{Kind, OperationAuthorizationV1, Record};
    if request.kind != Kind::RemoveCriticalReplica
        || request.managed_target.is_some()
        || request.expected_instance.is_some()
        || request.migration.is_some()
        || request.package_upgrade.is_some()
    {
        return Err(
            "healthy durable-set replacement is an exact local formal-owner operation".into(),
        );
    }
    let authorization = match &request.authorization {
        Some(OperationAuthorizationV1::HealthyDurableSet { authorization }) => {
            authorization.as_ref().clone()
        }
        _ => return Err("DURABILITY_WOULD_DROP: remove-critical-replica requires typed HealthyDurableSet replacement authorization".into()),
    };
    if request.world_id != authorization.expected_owner.world_id
        || request.chain_id != authorization.expected_owner.chain_id
        || request.expected_generation != authorization.expected_owner.authority_epoch
        || request.expected_config_digest != authorization.expected_owner.config_digest
    {
        return Err("healthy operation request differs from exact predecessor owner".into());
    }
    let request_digest = io::digest(&request)?;
    let plan = prepare_plan(&request_digest, &request.config, authorization)?;
    let progress = HealthyDurableSetProgressV1::new(&plan)?;
    Ok(Record {
        request,
        request_digest,
        revision: 1,
        state: "queued".into(),
        stage: "healthy_prepared".into(),
        dispatched: false,
        cancel_requested: false,
        recovered: false,
        blocker: None,
        results: vec![],
        remote_results: vec![],
        action_attempts: vec![],
        migration_receipt: None,
        migration_capacity_plan: None,
        protected_handoff_plan: None,
        formal_handoff: None,
        healthy_durable_set_plan: Some(plan),
        healthy_durable_set_progress: Some(progress),
        operation_result: None,
        package_upgrade_progress: None,
    })
}

pub(super) fn validate_record(record: &super::model::Record) -> Result<(), String> {
    let plan = record
        .healthy_durable_set_plan
        .as_ref()
        .ok_or("healthy operation missing immutable plan")?;
    let progress = record
        .healthy_durable_set_progress
        .as_ref()
        .ok_or("healthy operation missing durable progress")?;
    if plan.api_version != "oasis7.healthy-durable-set-plan/v1"
        || plan.request_digest != record.request_digest
        || plan.predecessor_config_file != record.request.config
        || plan.authorization.expected_owner.config_digest != record.request.expected_config_digest
        || record
            .request
            .authorization
            .as_ref()
            .map(|auth| match auth {
                super::model::OperationAuthorizationV1::HealthyDurableSet { authorization } => {
                    authorization.as_ref() == &plan.authorization
                }
                _ => false,
            })
            != Some(true)
        || !record.results.is_empty()
        || !record.remote_results.is_empty()
        || !record.action_attempts.is_empty()
        || record.protected_handoff_plan.is_some()
        || record.formal_handoff.is_some()
        || record.migration_receipt.is_some()
        || record.package_upgrade_progress.is_some()
    {
        return Err("healthy operation record carries unrelated or mismatched evidence".into());
    }
    progress.validate(plan)?;
    if !matches!(
        record.state.as_str(),
        "queued" | "running" | "reconciling" | "succeeded" | "cancelled"
    ) {
        return Err("invalid healthy operation state".into());
    }
    if let Some(result) = &record.operation_result {
        result.validate_record_binding(record)?;
    }
    if (record.state == "succeeded") != (progress.stage == HealthyStageV1::Completed)
        || (record.state == "cancelled") != (progress.stage == HealthyStageV1::Cancelled)
        || (progress.stage == HealthyStageV1::Completed
            && (progress.normal_readback.is_none() || progress.successor_owner.is_none()))
    {
        return Err("healthy operation terminal state lacks verified completion evidence".into());
    }
    Ok(())
}

fn save(
    store: &mut super::store::Store,
    mut record: super::model::Record,
) -> Result<super::model::Record, String> {
    record.operation_result = Some(super::result::OperationResultV1::collect(&record)?);
    let revision = record.revision;
    store.cas(record, revision)
}

pub(super) fn cancel(
    store: &mut super::store::Store,
    id: &str,
    revision: u64,
) -> Result<super::model::Record, String> {
    let mut record = store.get(id)?;
    if record.revision != revision {
        return Err("operation CAS revision changed".into());
    }
    if record.terminal() {
        return Ok(record);
    }
    let progress = record
        .healthy_durable_set_progress
        .as_mut()
        .ok_or("healthy progress missing")?;
    if !progress.stage.cancellation_allowed() {
        return Err("CANCELLATION_AFTER_ARM: original signed boundary may be armed; reconcile forward, never retract or reseal under a different identity".into());
    }
    progress.stage = HealthyStageV1::Cancelled;
    record.cancel_requested = true;
    record.state = "cancelled".into();
    record.stage = "healthy_cancelled".into();
    save(store, record)
}

pub(super) fn resume(
    store: &mut super::store::Store,
    id: &str,
    revision: u64,
) -> Result<super::model::Record, String> {
    let mut record = store.get(id)?;
    if record.revision != revision {
        return Err("operation CAS revision changed".into());
    }
    if record.terminal() {
        return Ok(record);
    }
    validate_record(&record)?;
    let plan = record
        .healthy_durable_set_plan
        .clone()
        .ok_or("healthy plan missing")?;
    if io::sha256(&plan.predecessor_config_file)? != plan.predecessor_config_digest
        || io::sha256(&plan.authorization.successor_runtime_config_file)?
            != plan.successor_config_digest
    {
        return Err("healthy operation immutable configuration changed".into());
    }
    let mut progress = record
        .healthy_durable_set_progress
        .clone()
        .ok_or("healthy progress missing")?;
    if progress.stage == HealthyStageV1::Prepared {
        progress.stage = HealthyStageV1::PreparingArm;
        record.dispatched = true;
        record.state = "running".into();
        record.stage = "healthy_preparing_arm".into();
        record.healthy_durable_set_progress = Some(progress.clone());
        record = save(store, record)?; // before irreversible native arm
    }
    match advance(&plan, &mut progress) {
        Ok(()) => {
            progress.blocker = None;
            record.blocker = None;
        }
        Err(error) => {
            let reason: String = error
                .chars()
                .filter(|c| !c.is_control())
                .take(2048)
                .collect();
            progress.blocker = Some(reason.clone());
            record.blocker = Some(reason);
            record.state = "reconciling".into();
        }
    }
    if progress.stage == HealthyStageV1::Completed {
        record.state = "succeeded".into();
    }
    record.stage = format!("healthy_{:?}", progress.stage).to_ascii_lowercase();
    record.healthy_durable_set_progress = Some(progress);
    save(store, record)
}

fn validated_inputs(
    plan: &HealthyDurableSetPlanV1,
    successor: bool,
) -> Result<super::super::formal_live_config::ValidatedFormalLiveConfigV1, String> {
    let path = if successor {
        &plan.authorization.successor_runtime_config_file
    } else {
        &plan.predecessor_config_file
    };
    let expected_config = if successor {
        &plan.successor_config_digest
    } else {
        &plan.predecessor_config_digest
    };
    if io::sha256(path)? != *expected_config {
        return Err("healthy immutable config changed".into());
    }
    let validated = super::super::formal_live_config::FormalLiveConfigV1::load(path)?;
    if validated.trust.checkpoint_digest != plan.external_initial_pin_digest
        || io::digest(&validated.trust)? != plan.external_read_trust_digest
    {
        return Err("healthy immutable external C0 trust/checkpoint dependency changed".into());
    }
    Ok(validated)
}

fn advance(
    plan: &HealthyDurableSetPlanV1,
    progress: &mut HealthyDurableSetProgressV1,
) -> Result<(), String> {
    use super::super::formal_live_runtime;
    use oasis7_distfs::controlled_authority::{
        formal::{
            transport::{RemoteDurableSetCandidateV1, RemoteFormalEndpoint},
            *,
        },
        replicated_protocol::EndpointRole,
    };
    use std::sync::Arc;
    let inputs = validated_inputs(plan, false)?;
    let successor_pinned_inputs = validated_inputs(plan, true)?;
    let active_inputs = if progress.stage.ordinal() >= HealthyStageV1::PublishingMarker.ordinal() {
        &successor_pinned_inputs
    } else {
        &inputs
    };
    let cache = formal_live_runtime::business_reader_for(active_inputs)?;
    let (_, startup) =
        super::super::formal_live_history::from_validated_for_runtime(active_inputs, cache)?;
    let old = if let Some(signed) = &progress.transition {
        super::super::formal_live_history::replay_operation_anchor_from_fixed_origin(
            active_inputs,
            &oasis7_client_api::world_service::FormalReadAnchorRefV1 {
                position: signed.body.predecessor.position,
                record_id: signed.body.predecessor.record_id.clone(),
                active_authority_id: signed.body.predecessor_authority_id.clone(),
            },
            std::time::Instant::now() + inputs.timeout,
        )?
        .authority()
        .clone()
    } else {
        startup.anchor.authority().clone()
    };
    if old.context_digest().map_err(|e| e.to_string())? != plan.authority_context_digest {
        return Err("original healthy authority context changed".into());
    }
    if progress.stage == HealthyStageV1::PreparingArm {
        // Owner capability itself queries original armed operation before any
        // preparation; a lost reply cannot select a new H or a new signature.
        progress.transition = Some(formal_live_runtime::control_prepare_durable_set_transition(
            &plan.predecessor_config_file,
            &plan.authorization.expected_owner,
            &plan.core_operation_id,
            plan.authorization.successor_replica.clone(),
            inputs.timeout,
        )?);
        progress.stage = HealthyStageV1::WaitingQualifiedBoundary;
        return Ok(());
    }
    let signed = progress
        .transition
        .clone()
        .ok_or("original signed transition missing")?;
    let make_old = |address, role| {
        RemoteFormalEndpoint::new(
            address,
            inputs.timeout,
            old.clone(),
            role,
            inputs.writer_key.clone(),
            inputs.anchor.head().head().clone(),
        )
        .map_err(|e| e.to_string())
    };
    if progress.stage == HealthyStageV1::WaitingQualifiedBoundary {
        let mut primary = make_old(inputs.config.primary, EndpointRole::Primary)?;
        let mut replica = make_old(inputs.config.replica, EndpointRole::Replica)?;
        let p = if let Some(previous_activation) = startup.activation.as_ref() {
            primary.restore_native_current_with_durable_set(
                inputs.anchor.authority(),
                previous_activation,
                None,
            )
        } else {
            primary.restore_native_current(&old, None, None)
        }
        .map_err(|e| e.to_string())?;
        let r = if let Some(previous_activation) = startup.activation.as_ref() {
            replica.restore_native_current_with_durable_set(
                inputs.anchor.authority(),
                previous_activation,
                None,
            )
        } else {
            replica.restore_native_current(&old, None, None)
        }
        .map_err(|e| e.to_string())?;
        if p.commit().record().position != signed.body.boundary_position
            || p.commit().record() != r.commit().record()
            || p.proof() != r.proof()
            || p.pending().is_some()
            || r.pending().is_some()
            || p.commit().record().durable_set_transition_digest.as_deref()
                != Some(signed.body.digest().map_err(|e| e.to_string())?.as_str())
        {
            return Err(
                "waiting for exact dual fresh Qualified H with armed transition and empty slot"
                    .into(),
            );
        }
        progress.boundary_record_id = Some(p.commit().head().record_id);
        progress.stage = HealthyStageV1::StoppingOldOwner;
        return Ok(());
    }
    if progress.stage == HealthyStageV1::StoppingOldOwner {
        let expected = &plan.authorization.expected_owner;
        let owner = formal_live_runtime::query_operation_owner(
            &plan.predecessor_config_file,
            &expected.operation_id,
            &expected.request_digest,
            &expected.config_digest,
            Some(&expected.incarnation),
        )?
        .ok_or("original healthy owner outcome unknown")?;
        if owner.instance != plan.authorization.expected_owner {
            return Err("original healthy owner instance changed".into());
        }
        let stopped = if matches!(owner.state.as_str(), "stopped" | "interrupted") {
            owner
        } else {
            formal_live_runtime::stop_operation_owner(
                &plan.predecessor_config_file,
                &plan.authorization.expected_owner,
                inputs.timeout,
            )?
        };
        if !matches!(stopped.state.as_str(), "stopped" | "interrupted")
            || stopped.instance != plan.authorization.expected_owner
        {
            return Err("exact original healthy owner stop outcome unknown".into());
        }
        progress.stage = HealthyStageV1::SealingOldPair;
        return Ok(());
    }
    let predecessor = super::super::formal_live_history::replay_operation_anchor_from_fixed_origin(
        active_inputs,
        &oasis7_client_api::world_service::FormalReadAnchorRefV1 {
            position: signed.body.predecessor.position,
            record_id: signed.body.predecessor.record_id.clone(),
            active_authority_id: signed.body.predecessor_authority_id.clone(),
        },
        std::time::Instant::now() + inputs.timeout,
    )?;
    let target = oasis7_client_api::world_service::FormalReadAnchorRefV1 {
        position: signed.body.boundary_position,
        record_id: progress
            .boundary_record_id
            .clone()
            .ok_or("original verified H selector missing")?,
        active_authority_id: signed.body.predecessor_authority_id.clone(),
    };
    let boundary = super::super::formal_live_history::replay_operation_anchor_from_fixed_origin(
        active_inputs,
        &target,
        std::time::Instant::now() + inputs.timeout,
    )
    .or_else(|_| {
        let mut target = target.clone();
        target.active_authority_id = signed
            .body
            .successor
            .authority_id()
            .map_err(|e| e.to_string())?;
        super::super::formal_live_history::replay_operation_anchor_from_fixed_origin(
            active_inputs,
            &target,
            std::time::Instant::now() + inputs.timeout,
        )
    })?;
    let transition =
        verify_normal_durable_set_transition(&signed, &old, predecessor.head(), boundary.head())
            .map_err(|e| e.to_string())?;
    if progress.stage == HealthyStageV1::SealingOldPair {
        for (index, address, role) in [
            (0, inputs.config.primary, EndpointRole::Primary),
            (1, inputs.config.replica, EndpointRole::Replica),
        ] {
            if progress.old_seals[index].is_some() {
                continue;
            }
            let mut endpoint = make_old(address, role)?;
            let seal = match endpoint
                .query_durable_set(signed.clone())
                .map_err(|e| e.to_string())?
            {
                Some(seal) => seal,
                None => {
                    let checkpoint = endpoint.checkpoint().map_err(|e| e.to_string())?;
                    endpoint
                        .seal_durable_set(signed.clone(), checkpoint)
                        .map_err(|e| e.to_string())?
                }
            };
            progress.old_seals[index] = Some(seal);
            return Ok(()); // persist each original seal separately
        }
        progress.stage = HealthyStageV1::WaitingExternalCandidates;
        return Ok(());
    }
    let old_seals = [
        progress.old_seals[0]
            .clone()
            .ok_or("primary seal missing")?,
        progress.old_seals[1]
            .clone()
            .ok_or("replica seal missing")?,
    ];
    let sealed = Arc::new(
        verify_sealed_durable_set_transition(
            &DurableSetSealedEvidenceV1 {
                transition: signed.clone(),
                primary_seal: old_seals[0].clone(),
                replica_seal: old_seals[1].clone(),
            },
            &old,
            predecessor.head(),
            boundary.head(),
        )
        .map_err(|e| e.to_string())?,
    );
    let make_candidate = |address, role| {
        RemoteDurableSetCandidateV1::new(
            address,
            inputs.timeout,
            sealed.clone(),
            role,
            inputs.writer_key.clone(),
        )
        .map_err(|e| e.to_string())
    };
    if matches!(
        progress.stage,
        HealthyStageV1::WaitingExternalCandidates | HealthyStageV1::InstallingCandidates
    ) {
        for (index, address, role) in [
            (
                0,
                plan.authorization.primary_management,
                EndpointRole::Primary,
            ),
            (
                1,
                plan.authorization.replica_management,
                EndpointRole::Replica,
            ),
        ] {
            if progress.installs[index].is_some() {
                continue;
            }
            let mut candidate = make_candidate(address, role)?;
            let install = match candidate.query_install() {
                Ok(receipt) => receipt,
                Err(_) => {
                    // Unknown is unsigned and proves no absence. Reconcile
                    // only the SAME signed install identity against the
                    // create-once protected provider; never create a new
                    // candidate, transition or authority from this outcome.
                    candidate.install().map_err(|e| {
                        format!(
                            "WaitingExternalCandidates: reconcile original {role:?} install: {e}"
                        )
                    })?
                }
            };
            progress.installs[index] = Some(install);
            progress.stage = HealthyStageV1::InstallingCandidates;
            return Ok(());
        }
        let installs = [
            progress.installs[0]
                .clone()
                .ok_or("primary install missing")?,
            progress.installs[1]
                .clone()
                .ok_or("replica install missing")?,
        ];
        verify_durable_set_activation(
            &transition,
            [&old_seals[0], &old_seals[1]],
            [&installs[0], &installs[1]],
        )
        .map_err(|e| e.to_string())?;
        progress.activation_step = Some(
            oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1 {
                transition: signed,
                old_seals,
                new_installs: installs,
            },
        );
        progress.stage = HealthyStageV1::ActivatingCandidates;
        return Ok(());
    }
    let step = progress
        .activation_step
        .clone()
        .ok_or("dual healthy activation evidence missing")?;
    let activation = verify_durable_set_activation(
        &transition,
        [&step.old_seals[0], &step.old_seals[1]],
        [&step.new_installs[0], &step.new_installs[1]],
    )
    .map_err(|e| e.to_string())?;
    let successor_inputs = validated_inputs(plan, true)?;
    if progress.stage == HealthyStageV1::ActivatingCandidates {
        // Ordinary dual native H readback is the query for the original
        // activation. Management installed receipts never imply readiness.
        if fresh_normal_readback(&successor_inputs, &activation, false).is_ok() {
            progress.stage = HealthyStageV1::PublishingMarker;
            return Ok(());
        }
        for (address, role, index) in [
            (
                plan.authorization.primary_management,
                EndpointRole::Primary,
                0,
            ),
            (
                plan.authorization.replica_management,
                EndpointRole::Replica,
                1,
            ),
        ] {
            let mut candidate = make_candidate(address, role)?;
            if candidate.query_install().map_err(|e| e.to_string())? != step.new_installs[index] {
                return Err(
                    "original candidate install outcome changed; activation not resent".into(),
                );
            }
            if candidate
                .activate(step.new_installs[0].clone(), step.new_installs[1].clone())
                .map_err(|e| e.to_string())?
                != step.new_installs[index]
            {
                return Err("healthy activation changed original install receipt".into());
            }
        }
        progress.stage = HealthyStageV1::WaitingFreshNormalBoundary;
        return Ok(());
    }
    if progress.stage == HealthyStageV1::WaitingFreshNormalBoundary {
        fresh_normal_readback(&successor_inputs, &activation, false)?;
        progress.stage = HealthyStageV1::PublishingMarker;
        return Ok(());
    }
    if progress.stage == HealthyStageV1::PublishingMarker {
        // Readback immediately precedes atomic marker publication even on
        // resume; local progress alone cannot cross this authority boundary.
        fresh_normal_readback(&successor_inputs, &activation, false)?;
        super::super::formal_live_history::persist_durable_set_activation_marker(
            &plan.writer_directory,
            &step,
            &activation,
        )?;
        progress.stage = HealthyStageV1::ReopeningOriginalC0;
        return Ok(());
    }
    let (_, startup) = super::super::formal_live_history::from_validated_for_runtime(
        &successor_inputs,
        formal_live_runtime::business_reader_for(&successor_inputs)?,
    )?;
    let replayed_activation = startup
        .activation
        .as_ref()
        .ok_or("fixed C0 startup does not replay healthy activation marker")?;
    replayed_activation
        .verify_evidence(
            &step.transition,
            [&step.old_seals[0], &step.old_seals[1]],
            [&step.new_installs[0], &step.new_installs[1]],
        )
        .map_err(|e| e.to_string())?;
    if progress.stage == HealthyStageV1::ReopeningOriginalC0 {
        let owner_operation = format!("healthy:{}", plan.core_operation_id);
        let existing = formal_live_runtime::query_operation_owner_with_fixed_trust(
            &plan.authorization.successor_runtime_config_file,
            &owner_operation,
            &plan.request_digest,
            &plan.successor_config_digest,
            None,
            &plan.external_read_trust_digest,
        )?;
        let owner = match existing {
            Some(owner) if owner.state == "ready" => owner,
            Some(_) | None => formal_live_runtime::launch_operation_owner_with_fixed_trust(
                &plan.authorization.successor_runtime_config_file,
                &owner_operation,
                &plan.request_digest,
                &plan.successor_config_digest,
                &plan.external_read_trust_digest,
                successor_inputs.timeout,
            )?,
        };
        if owner.state != "ready"
            || owner.instance.incarnation == plan.authorization.expected_owner.incarnation
            || owner.instance.authority_id
                != activation
                    .successor()
                    .authority_id()
                    .map_err(|e| e.to_string())?
        {
            return Err("new exact C0 owner is not ready with verified successor authority".into());
        }
        progress.successor_owner = Some(owner.instance);
        progress.stage = HealthyStageV1::WaitingFirstNormalQualified;
        return Ok(());
    }
    if progress.stage == HealthyStageV1::WaitingFirstNormalQualified {
        let expected = progress
            .successor_owner
            .as_ref()
            .ok_or("successor owner missing")?;
        let owner = formal_live_runtime::query_operation_owner_with_fixed_trust(
            &plan.authorization.successor_runtime_config_file,
            &expected.operation_id,
            &expected.request_digest,
            &expected.config_digest,
            Some(&expected.incarnation),
            &plan.external_read_trust_digest,
        )?
        .ok_or("successor owner outcome unknown")?;
        if owner.state != "ready" || owner.instance != *expected {
            return Err("successor exact instance changed before protocol completion".into());
        }
        progress.normal_readback = Some(fresh_normal_readback(
            &successor_inputs,
            replayed_activation,
            true,
        )?);
        progress.stage = HealthyStageV1::Completed;
        return Ok(());
    }
    Err("unsupported healthy operation progress stage".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn healthy_cancellation_ends_before_native_arm_dispatch() {
        assert!(HealthyStageV1::Prepared.cancellation_allowed());
        for stage in [
            HealthyStageV1::PreparingArm,
            HealthyStageV1::WaitingQualifiedBoundary,
            HealthyStageV1::StoppingOldOwner,
            HealthyStageV1::SealingOldPair,
            HealthyStageV1::WaitingExternalCandidates,
            HealthyStageV1::InstallingCandidates,
            HealthyStageV1::ActivatingCandidates,
            HealthyStageV1::WaitingFreshNormalBoundary,
            HealthyStageV1::PublishingMarker,
            HealthyStageV1::ReopeningOriginalC0,
            HealthyStageV1::WaitingFirstNormalQualified,
            HealthyStageV1::Completed,
            HealthyStageV1::Cancelled,
        ] {
            assert!(!stage.cancellation_allowed(), "{stage:?}");
        }
    }
}
