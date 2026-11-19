//! Unique release Apply prelude over sealed actual Legacy/Current observers.
//! A returned request is DATA; only the fixed caller can bind an install owner.
use super::instance_rules::ModuleBusinessErrorV1;
use super::release_rules::{ModuleReleaseBusinessFactsV1, ModuleReleaseRequestSnapshotV1};
use crate::{GovernanceFinalityEpochSnapshot, RejectReason};
use oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1;
use oasis7_world_state::state::{ModuleReleaseRequestState, ModuleReleaseRequestStatus as Status};
use std::{collections::BTreeSet, ops::Deref};
pub(in crate::world) enum ModuleReleaseEpochSnapshotV1 {
    Legacy {
        epoch: u64,
        snapshot: GovernanceFinalityEpochSnapshot,
    },
    Current(std::sync::Arc<crate::world::PaidCurrentModuleEpochSnapshotV1>),
}
impl Deref for ModuleReleaseEpochSnapshotV1 {
    type Target = GovernanceFinalityEpochSnapshot;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy { snapshot, .. } => snapshot,
            Self::Current(snapshot) => snapshot.snapshot(),
        }
    }
}
impl ModuleReleaseEpochSnapshotV1 {
    pub(in crate::world) fn epoch_id(&self) -> u64 {
        match self {
            Self::Legacy { epoch, .. } => *epoch,
            Self::Current(snapshot) => snapshot.epoch_id(),
        }
    }
}
pub(in crate::world) trait ModuleReleaseApplyFactsV1:
    ModuleReleaseBusinessFactsV1
{
    fn reserve_apply_query(&mut self, operator: &str, id: u64) -> Result<(), Self::Error>;
    fn epoch_snapshot(&mut self) -> Result<ModuleReleaseEpochSnapshotV1, Self::Error>;
    /// Fund the original signer sets, eligible vector, receipt tuples, error graph
    /// and all string comparisons before the actual borrowed computation below.
    fn reserve_attestation_evidence(
        &mut self,
        request: &ModuleReleaseRequestState,
        snapshot: &GovernanceFinalityEpochSnapshot,
    ) -> Result<(), Self::Error>;
}
pub(in crate::world) enum ModuleReleaseApplyDecisionV1 {
    Rejected(RejectReason),
    Ready(ModuleReleaseRequestSnapshotV1),
}
fn rejected(note: String) -> ModuleReleaseApplyDecisionV1 {
    ModuleReleaseApplyDecisionV1::Rejected(RejectReason::RuleDenied { notes: vec![note] })
}
pub(in crate::world) fn prepare_module_release_apply_v1<S: ModuleReleaseApplyFactsV1>(
    source: &mut S,
    operator: &str,
    id: u64,
) -> Result<ModuleReleaseApplyDecisionV1, S::Error> {
    source.reserve_apply_query(operator, id)?;
    if !source.agent_exists(operator)? {
        return Ok(ModuleReleaseApplyDecisionV1::Rejected(
            RejectReason::AgentNotFound {
                agent_id: operator.to_owned(),
            },
        ));
    }
    let Some(request) = source.request(id)? else {
        return Ok(rejected(format!(
            "module release apply rejected: request not found ({id})"
        )));
    };
    if matches!(
        request.status,
        Status::Requested | Status::Shadowed | Status::Rejected | Status::Applied
    ) {
        return Ok(rejected(format!(
            "module release apply rejected: invalid status {:?} for request {}",
            request.status, id
        )));
    }
    if let Err(error) = source.frozen_admission(&request.manifest) {
        match error.into_business() {
            Ok(error) => return Ok(rejected(format!("{error:?}"))),
            Err(error) => return Err(error),
        }
    }
    if !request
        .required_roles
        .iter()
        .all(|required| request.role_approvals.contains_key(required))
    {
        return Ok(rejected(format!(
            "module release apply rejected: required roles are not fully approved for request {}",
            id
        )));
    }
    // Original reads the current owner even when registration is absent, then
    // short-circuits the artifact/owner comparison. Keep that observed order.
    let owner = source.owner(&request.manifest.wasm_hash)?;
    if !source.artifact_exists(&request.manifest.wasm_hash)?
        || owner.as_ref() != Some(&request.requester_agent_id)
    {
        return Ok(rejected(format!(
            "module release apply rejected: requester {} is no longer current owner of artifact {} (owner {:?})",
            request.requester_agent_id,
            request.manifest.wasm_hash,
            owner.as_ref()
        )));
    }
    for required in &request.required_roles {
        let Some(approver) = request.role_approvals.get(required) else {
            continue;
        };
        // Membership is observed before agent presence, exactly as the old local
        // `still_bound` computation, even though the following predicate short-circuits.
        let still_bound =
            source.role_membership(approver, required)? == AuthorizationMembershipV1::Present;
        if !source.agent_exists(approver)? || !still_bound {
            return Ok(rejected(format!(
                "module release apply rejected: approver {} no longer has current role binding {}",
                approver, required
            )));
        }
    }
    let snapshot = source.epoch_snapshot()?;
    source.reserve_attestation_evidence(&request, &snapshot)?;
    let snapshot_signers: BTreeSet<&str> = snapshot
        .signer_node_ids
        .iter()
        .map(String::as_str)
        .collect();
    let aggregated_signers: BTreeSet<String> = request
        .attestations
        .values()
        .filter_map(|attestation| {
            let id = attestation.signer_node_id.trim();
            snapshot_signers.contains(id).then(|| id.to_owned())
        })
        .collect();
    let eligible: Vec<_> = request
        .attestations
        .values()
        .filter(|attestation| snapshot_signers.contains(attestation.signer_node_id.trim()))
        .collect();
    let min = snapshot.effective_min_unique_signers();
    let stake = crate::world::governance::governance_finality_signed_stake_bps(
        &snapshot.signer_node_ids,
        &snapshot.validator_stakes,
        &aggregated_signers,
    );
    if aggregated_signers.len() < min as usize || stake < snapshot.threshold_bps {
        return Ok(rejected(format!(
            "module release apply rejected: attestation threshold not met epoch_id={} min_unique_signers={} threshold_bps={} aggregated_signers={} aggregated_stake_bps={} request_id={}",
            snapshot.epoch_id(),
            min,
            snapshot.threshold_bps,
            aggregated_signers.len(),
            stake,
            id
        )));
    }
    let receipts: BTreeSet<_> = eligible
        .iter()
        .map(|a| {
            (
                a.wasm_hash.clone(),
                a.source_hash.clone(),
                a.build_manifest_hash.clone(),
                a.builder_image_digest.clone(),
                a.container_platform.clone(),
                a.canonicalizer_version.clone(),
            )
        })
        .collect();
    if receipts.len() > 1 {
        return Ok(rejected(format!(
            "module release apply rejected: attestation receipt evidence mismatch request_id={} unique_receipt_variants={}",
            id,
            receipts.len()
        )));
    }
    if let (Some(identity), Some(attestation)) = (
        request.manifest.artifact_identity.as_ref(),
        eligible.first(),
    ) && (attestation.source_hash != identity.source_hash
        || attestation.build_manifest_hash != identity.build_manifest_hash
        || attestation.wasm_hash != request.manifest.wasm_hash)
    {
        return Ok(rejected(format!(
            "module release apply rejected: attestation receipt identity mismatch request_id={} expected_wasm_hash={} actual_wasm_hash={} expected_source_hash={} actual_source_hash={} expected_build_manifest_hash={} actual_build_manifest_hash={}",
            id,
            request.manifest.wasm_hash,
            attestation.wasm_hash,
            identity.source_hash,
            attestation.source_hash,
            identity.build_manifest_hash,
            attestation.build_manifest_hash
        )));
    }
    if let Err(reason) = source.profile_changes(&request.profile_changes)? {
        return Ok(rejected(format!("module release apply rejected: {reason}")));
    }
    match request.shadow_manifest_hash.as_deref() {
        Some(reviewed) => match source.shadow_hash(
            &request.manifest,
            request.activate,
            &request.profile_changes,
        )? {
            Ok(hash) if hash == reviewed => {}
            Ok(_) => {
                return Ok(rejected(format!(
                    "module release apply rejected: reviewed manifest/profile commitment changed since shadowing request {}",
                    id
                )));
            }
            Err(reason) => return Ok(rejected(reason)),
        },
        None if !request.profile_changes.is_empty() => {
            return Ok(rejected(format!(
                "module release apply rejected: profile-bearing request {} has no reviewed commitment; resubmit and shadow again",
                id
            )));
        }
        None => {}
    }
    Ok(ModuleReleaseApplyDecisionV1::Ready(request))
}
