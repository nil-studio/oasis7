//! Bounded private publication recovery beneath the externally configured cut.
//! No directory scan or service-discovered pin; long replay is paged.
use super::{
    formal_live_config::ValidatedFormalLiveConfigV1, formal_live_service::FormalLiveHistoryResolver,
};
use oasis7::{
    runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World},
    world_service::{
        formal_read::{FormalReadHistoryV1, FormalReadStepV1, MAX_FORMAL_HISTORY_STEPS},
        formal_read_service::FormalReadPublication,
    },
};
use oasis7_client_api::world_service::{CommitRef, FormalReadAnchorRefV1};
use oasis7_distfs::controlled_authority::{
    formal::{
        FormalCompletionBranchV1, FormalHeadRefV1, FormalQualifiedCommitProofV1,
        FormalReadCheckpointV1, FormalRecordKindV1, VerifiedAuthorityV1,
        VerifiedDurableSetActivationV1, VerifiedFormalReadAnchorV1, VerifiedRecoveryBootstrapV1,
        business_index::BusinessIndexNodeReader,
        endpoint::{FormalCandidateMaterialV1, VerifiedNativeHistoricalRestoreV1},
        transport::RemoteFormalEndpoint,
        verify_formal_qualified, verify_frozen_material, verify_record_material,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord, EndpointRole},
};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "formal_live_business_history.rs"]
mod business;
pub(crate) use business::VerifiedBusinessPrewarmReceipt;
#[path = "formal_live_durable_set_activation.rs"]
mod durable_set_activation;
#[path = "formal_live_history_material.rs"]
mod material;
#[path = "formal_live_history_replay.rs"]
mod replay;
#[path = "formal_live_history_transitions.rs"]
mod transitions;

use replay::{
    ReplayFailure, ReplayStepProof, ReplayTargetPolicy, ReplayedHistoryCut,
    advance_durable_set_marker, decode_role,
    replay_exact_origin_after_verified_with_activation_policy,
    replay_exact_origin_from_with_activations, requested_anchor_matches,
    require_exact_history_target,
};
#[cfg(test)]
use replay::{
    replay_exact_origin_after_verified, replay_exact_origin_after_verified_with_policy,
    replay_exact_origin_from,
};
use transitions::{
    advance_history_completion, advance_normal_history_proof, apply_ready_recoveries,
    apply_ready_recoveries_until,
};

pub(crate) fn persist_durable_set_activation_marker(
    writer_directory: &std::path::Path,
    step: &oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1,
    activation: &VerifiedDurableSetActivationV1,
) -> Result<(), String> {
    durable_set_activation::DurableSetActivationMarkerStoreV1::new(writer_directory.to_path_buf())
        .persist_activated(step.clone(), activation)
}

pub(crate) fn from_validated(
    validated: &ValidatedFormalLiveConfigV1,
    business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
) -> Result<Arc<dyn FormalLiveHistoryResolver>, String> {
    let (resolver, _) = from_validated_for_runtime(validated, business_reader)?;
    Ok(resolver)
}

/// Opaque startup context derived by replaying immutable activation markers
/// from the configured external origin. The marker files themselves confer no
/// trust and cannot be used without this external-origin replay.
pub(crate) struct FormalLiveStartupAnchorV1 {
    pub(crate) anchor: VerifiedFormalReadAnchorV1,
    pub(crate) activation: Option<VerifiedDurableSetActivationV1>,
}

pub(crate) fn from_validated_for_runtime(
    validated: &ValidatedFormalLiveConfigV1,
    business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
) -> Result<
    (
        Arc<dyn FormalLiveHistoryResolver>,
        FormalLiveStartupAnchorV1,
    ),
    String,
> {
    let mut resolver = resolver_from_validated(validated, business_reader)?;
    let startup = resolver.startup_anchor(validated)?;
    if let Some(activation) = &startup.activation {
        resolver.active = activation.successor().clone();
    }
    Ok((Arc::new(resolver), startup))
}

fn resolver_from_validated(
    validated: &ValidatedFormalLiveConfigV1,
    business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
) -> Result<NativeHistoryResolver, String> {
    let trusted_anchor = if let Some(permission) = &validated.recovery {
        permission
            .external_anchor()
            .with_pending_recovery(permission.recovery())
            .map_err(|e| e.to_string())?
    } else {
        validated.anchor.clone()
    };
    Ok(NativeHistoryResolver {
        addresses: [validated.config.primary, validated.config.replica],
        timeout: validated.timeout,
        authentication: validated.writer_key.clone(),
        external_origin: validated.anchor.clone(),
        trusted: trusted_anchor,
        permission: validated.recovery.clone(),
        active: validated.active_authority().clone(),
        release: validated.release_policy.clone(),
        business_reader,
        writer_directory: validated.config.writer_directory.clone(),
        origin_history_directory: validated.config.recovery_origin_history_directory.clone(),
        permissions: checked_permissions(validated)?,
        activation_markers: durable_set_activation::DurableSetActivationMarkerStoreV1::new(
            validated.config.writer_directory.clone(),
        ),
        prewarm_scope: Arc::new(()),
    })
}

/// Resolve the immutable owner authority from the same fixed-origin proof
/// replay used by runtime startup, before acquiring the writer lease.
pub(crate) fn verified_startup_authority(
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<VerifiedAuthorityV1, String> {
    let reader = super::formal_live_runtime::business_reader_for(validated)?;
    let resolver = resolver_from_validated(validated, reader)?;
    let startup = resolver.startup_anchor(validated)?;
    Ok(startup.activation.as_ref().map_or_else(
        || validated.active_authority().clone(),
        |activation| activation.successor().clone(),
    ))
}

/// Local publication proofs are replay inputs, never a replacement trust root.
/// This resolves an operation selector from fixed external C0 without requiring
/// the old endpoint to remain online after its sealed store has been handed off.
pub(crate) fn replay_operation_anchor_from_fixed_origin(
    validated: &ValidatedFormalLiveConfigV1,
    requested: &FormalReadAnchorRefV1,
    deadline: Instant,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let reader = super::formal_live_runtime::business_reader_for(validated)?;
    let resolver = resolver_from_validated(validated, reader)?;
    Ok(resolver.replay_exact_origin(requested, deadline)?.anchor)
}

fn checked_permissions(
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<Vec<VerifiedRecoveryBootstrapV1>, String> {
    let mut raw = validated.recovery.as_ref().map(|p| p.permission_evidence());
    let mut values = Vec::new();
    while let Some(permission) = raw {
        if values.len() == MAX_FORMAL_HISTORY_STEPS {
            return Err("recovery permission chain bound".into());
        }
        values.push(permission.verify(
            &validated.trust.initial_trust,
            &validated.trust.checkpoint.initial_certificate,
        )?);
        raw = permission.prior_permission.as_deref();
    }
    values.reverse();
    Ok(values)
}
struct NativeHistoryResolver {
    addresses: [SocketAddr; 2],
    timeout: Duration,
    authentication: ed25519_dalek::SigningKey,
    external_origin: VerifiedFormalReadAnchorV1,
    trusted: VerifiedFormalReadAnchorV1,
    permission: Option<VerifiedRecoveryBootstrapV1>,
    active: VerifiedAuthorityV1,
    permissions: Vec<VerifiedRecoveryBootstrapV1>,
    release: ReleaseSecurityPolicy,
    business_reader: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    writer_directory: PathBuf,
    origin_history_directory: Option<PathBuf>,
    activation_markers: durable_set_activation::DurableSetActivationMarkerStoreV1,
    // An unforgeable in-memory scope for one constructor's immutable origin,
    // permissions, activation markers, release policy and endpoint bindings.
    prewarm_scope: Arc<()>,
}
impl NativeHistoryResolver {
    fn startup_anchor(
        &self,
        validated: &ValidatedFormalLiveConfigV1,
    ) -> Result<FormalLiveStartupAnchorV1, String> {
        let positions = self.activation_markers.positions()?;
        let Some(&last_position) = positions.last() else {
            return Ok(FormalLiveStartupAnchorV1 {
                anchor: validated.anchor.clone(),
                activation: None,
            });
        };
        if validated.recovery.is_some() {
            return Err(
                "formal startup with a healthy durable-set activation after configured recovery is unsupported".into(),
            );
        }
        if positions
            .iter()
            .any(|position| *position <= self.external_origin.head().record().position)
        {
            return Err(
                "HistoryUnavailable: healthy activation marker is at/before the external origin"
                    .into(),
            );
        }
        let last_marker = self
            .activation_markers
            .read(last_position)?
            .ok_or("healthy durable-set startup marker disappeared")?;
        let boundary_record_id = last_marker.step.new_installs[0]
            .body
            .boundary
            .record_id
            .clone();
        let active_authority_id = last_marker
            .step
            .transition
            .body
            .successor
            .authority_id()
            .map_err(|error| error.to_string())?;
        let requested = FormalReadAnchorRefV1 {
            position: last_position,
            record_id: boundary_record_id,
            active_authority_id,
        };
        let deadline = Instant::now()
            .checked_add(self.timeout)
            .ok_or("HistoryUnavailable: healthy activation replay deadline overflow")?;
        let replayed = self.replay_exact_origin(&requested, deadline)?;
        if replayed.activation.is_none()
            || replayed.anchor.head().record().position != last_position
            || replayed
                .anchor
                .authority()
                .durable_set_activation_digest()
                .is_none()
        {
            return Err(
                "HistoryUnavailable: latest healthy activation was not derived from external-origin replay".into(),
            );
        }
        Ok(FormalLiveStartupAnchorV1 {
            anchor: replayed.anchor,
            activation: replayed.activation,
        })
    }

    fn replay_exact_origin(
        &self,
        requested: &FormalReadAnchorRefV1,
        deadline: Instant,
    ) -> Result<ReplayedHistoryCut, String> {
        replay_exact_origin_from_with_activations(
            &self.external_origin,
            requested,
            &self.permissions,
            deadline,
            |position| {
                self.activation_markers
                    .read(position)?
                    .map(|marker| {
                        if marker.boundary_position != position {
                            return Err("formal durable-set marker position mismatch".into());
                        }
                        Ok(marker.step)
                    })
                    .transpose()
            },
            |cursor, position, pending_permission, operation_deadline| {
                if let Some(proof) =
                    super::execution_bridge::formal_writer::read_published_proof_at(
                        &self.writer_directory,
                        position,
                    )?
                {
                    return Ok(proof);
                }
                self.restore_history_proof(cursor, position, pending_permission, operation_deadline)
            },
        )
    }

    fn replay_exact_origin_for_prewarm(
        &self,
        requested: &FormalReadAnchorRefV1,
        target_policy: ReplayTargetPolicy,
        deadline: Instant,
        mut proof_at: impl FnMut(
            &VerifiedFormalReadAnchorV1,
            u64,
            Option<usize>,
            Instant,
        ) -> Result<ReplayStepProof, ReplayFailure>,
        mut after_verified: impl FnMut(
            &VerifiedFormalReadAnchorV1,
            &VerifiedFormalReadAnchorV1,
            &FormalQualifiedCommitProofV1,
            Option<&FormalCandidateMaterialV1>,
        ) -> Result<(), ReplayFailure>,
    ) -> Result<ReplayedHistoryCut, ReplayFailure> {
        replay_exact_origin_after_verified_with_activation_policy(
            &self.external_origin,
            requested,
            &self.permissions,
            deadline,
            target_policy,
            |cursor, position, pending_permission, operation_deadline| {
                proof_at(cursor, position, pending_permission, operation_deadline)
            },
            |position| {
                self.activation_markers
                    .read(position)
                    .map_err(ReplayFailure::Invalid)?
                    .map(|marker| {
                        if marker.boundary_position != position {
                            return Err(ReplayFailure::Invalid(
                                "formal durable-set marker position mismatch".into(),
                            ));
                        }
                        Ok(marker.step)
                    })
                    .transpose()
            },
            |predecessor, anchor, proof, candidate| {
                after_verified(predecessor, anchor, proof, candidate)
            },
        )
    }

    fn restore_history_proof(
        &self,
        cursor: &VerifiedFormalReadAnchorV1,
        position: u64,
        pending_permission: Option<usize>,
        deadline: Instant,
    ) -> Result<FormalQualifiedCommitProofV1, String> {
        if cursor.authority().durable_set_activation_digest().is_some() {
            return Err(
                "HistoryUnavailable: post-activation proof is missing from exact local publication history".into(),
            );
        }
        let permission = pending_permission.map(|index| &self.permissions[index]);
        let authority = permission.map_or_else(|| cursor.authority(), |p| p.original_authority());
        let original = authority;
        let recovery = permission.map(|p| p.recovery().clone());
        let restored =
            self.restore_native_position(position, None, authority, original, recovery, deadline)?;
        if restored.commit().record().position != position
            || restored.minimum() != &self.external_origin.head().head()
        {
            return Err(
                "HistoryUnavailable: native proof fallback differs from replay cursor".into(),
            );
        }
        Ok(restored.proof().clone())
    }

    fn activation_at_start(
        &self,
        cursor: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        deadline: Instant,
    ) -> Result<
        Option<(
            VerifiedFormalReadAnchorV1,
            oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1,
        )>,
        String,
    > {
        if cursor.checkpoint() == target.checkpoint() {
            return Ok(None);
        }
        let position = cursor.head().record().position;
        let Some(marker) = self.activation_markers.read(position)? else {
            return Ok(None);
        };
        let current_authority_id = cursor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?;
        let predecessor_authority_id = marker
            .step
            .transition
            .body
            .predecessor_authority_id
            .as_str();
        if current_authority_id != predecessor_authority_id {
            return Ok(None);
        }
        let marker_boundary = &marker.step.new_installs[0].body.boundary;
        if marker_boundary.position != position
            || marker_boundary.record_id != cursor.head().head().record_id
            || target.checkpoint() == cursor.checkpoint()
        {
            return Ok(None);
        }
        let successor_authority_id = marker
            .step
            .transition
            .body
            .successor
            .authority_id()
            .map_err(|error| error.to_string())?;
        let requested = FormalReadAnchorRefV1 {
            position,
            record_id: marker_boundary.record_id.clone(),
            active_authority_id: successor_authority_id,
        };
        let replayed = self.replay_exact_origin(&requested, deadline)?;
        let activation = replayed
            .activation
            .as_ref()
            .ok_or("HistoryUnavailable: exact healthy activation replay is missing")?;
        let predecessor = replayed
            .activation_predecessor
            .as_ref()
            .ok_or("HistoryUnavailable: exact activation predecessor is missing")?;
        let boundary = predecessor
            .after_verified_commit(
                activation.predecessor().clone(),
                activation.boundary().clone(),
            )
            .map_err(|error| error.to_string())?;
        if boundary.checkpoint() != cursor.checkpoint()
            || !requested_anchor_matches(&replayed.anchor, &requested)?
            || replayed.activation_step.as_ref() != Some(&marker.step)
        {
            return Err(
                "HistoryUnavailable: start-position activation differs from exact verified history"
                    .into(),
            );
        }
        Ok(Some((replayed.anchor, marker.step)))
    }
}

impl FormalLiveHistoryResolver for NativeHistoryResolver {
    fn prewarm_qualified_business_history(
        &self,
        target: &VerifiedFormalReadAnchorV1,
        reader: &dyn BusinessIndexNodeReader,
    ) -> Result<(), String> {
        business::prewarm_qualified_history(self, target, reader)
    }

    fn prewarm_qualified_business_history_hint(
        &self,
        hint: &FormalReadAnchorRefV1,
        reader: &dyn BusinessIndexNodeReader,
    ) -> Result<Option<VerifiedBusinessPrewarmReceipt>, String> {
        business::prewarm_qualified_history_hint(self, hint, reader)
    }

    fn prewarm_qualified_business_history_after_hint(
        &self,
        target: &VerifiedFormalReadAnchorV1,
        reader: &dyn BusinessIndexNodeReader,
        receipt: Option<VerifiedBusinessPrewarmReceipt>,
    ) -> Result<bool, String> {
        business::prewarm_qualified_history_after_hint(self, target, reader, receipt)
    }

    #[cfg(test)]
    fn business_result(
        &self,
        cut: &CommitRef,
        correlation: &oasis7_client_api::world_service::RequestCorrelation,
        scope: &str,
        reader: &str,
    ) -> Result<
        Option<(
            oasis7_client_api::world_service::FormalBusinessResultProofV1,
            Option<String>,
        )>,
        String,
    > {
        business::result_from_commit(self, cut, correlation, scope, reader)
    }
    fn business_result_for_publication(
        &self,
        cut: FormalReadPublication,
        correlation: &oasis7_client_api::world_service::RequestCorrelation,
        scope: &str,
        reader: &str,
    ) -> Result<
        Option<(
            oasis7_client_api::world_service::FormalBusinessResultProofV1,
            Option<String>,
        )>,
        String,
    > {
        business::result(self, cut, correlation, scope, reader)
    }
    fn origin_publication(
        &self,
        anchor: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, String> {
        let has_healthy_markers = !self.activation_markers.positions()?.is_empty();
        if !has_healthy_markers
            && anchor.active_authority_id
                == self.active.authority_id().map_err(|e| e.to_string())?
        {
            return self.publication(&self.restore(anchor.position, Some(&anchor.record_id))?);
        }

        let deadline = Instant::now()
            .checked_add(self.timeout)
            .ok_or("HistoryUnavailable: origin replay deadline overflow")?;
        let replayed = self.replay_exact_origin(anchor, deadline)?;
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: exact origin restore deadline exceeded".into());
        }
        if Instant::now() >= deadline {
            return Err(
                "HistoryUnavailable: exact origin material validation deadline exceeded".into(),
            );
        }
        if has_healthy_markers {
            self.publication_from_replayed_cut(replayed)
        } else {
            let restored = self.restore_exact_history_cut(&replayed, deadline)?;
            let world = self.world_for(&replayed.anchor, restored.material())?;
            FormalReadPublication::new(replayed.anchor, world)
        }
    }
    fn fixed_publication(&self, commit: &CommitRef) -> Result<FormalReadPublication, String> {
        if !self.activation_markers.positions()?.is_empty() {
            let requested = self.anchor_ref_for_position(
                commit.position,
                Some(commit.execution_block_hash.as_str()),
            )?;
            return self.publication_from_exact_replay(&requested);
        }
        let record_id = commit.execution_block_hash.as_str();
        self.publication(&self.restore(commit.position, Some(record_id))?)
    }
    fn forward_publication(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        current: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadPublication, String> {
        if max_steps == 0
            || max_steps > MAX_FORMAL_HISTORY_STEPS
            || origin.head().record().position > current.head().record().position
        {
            return Err("formal forward page bounds".into());
        }
        let position = origin
            .head()
            .record()
            .position
            .saturating_add(max_steps as u64)
            .min(current.head().record().position);
        if !self.activation_markers.positions()?.is_empty() {
            let requested = if position == current.head().record().position {
                FormalReadAnchorRefV1 {
                    position,
                    record_id: current.head().head().record_id.clone(),
                    active_authority_id: current
                        .authority()
                        .authority_id()
                        .map_err(|error| error.to_string())?,
                }
            } else {
                self.anchor_ref_for_position(position, None)?
            };
            return self.publication_from_exact_replay(&requested);
        }
        self.publication(&self.restore(position, None)?)
    }

    fn history_page(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadHistoryV1, String> {
        let deadline = Instant::now()
            .checked_add(self.timeout)
            .ok_or("HistoryUnavailable: formal history deadline overflow")?;
        let start = origin.head().record().position;
        let end = target.head().record().position;
        if max_steps == 0
            || max_steps > MAX_FORMAL_HISTORY_STEPS
            || end < start
            || end - start > max_steps as u64
        {
            return Err("HistoryUnavailable: formal history requires another bounded page".into());
        }
        let mut cursor = origin.clone();
        let mut steps = Vec::with_capacity((end - start) as usize);
        let mut next_permission = 0;
        let mut previous_permission = None;
        let mut previous_emitted_in_page = false;
        let mut pending_permission = None;
        apply_ready_recoveries(
            &mut cursor,
            &self.permissions,
            &mut next_permission,
            &mut previous_permission,
            &mut previous_emitted_in_page,
            &mut pending_permission,
            &mut steps,
            max_steps,
        )?;
        if let Some((activated, step)) = self.activation_at_start(&cursor, target, deadline)? {
            if steps.len() >= max_steps {
                return Err(
                    "HistoryUnavailable: durable-set activation requires another bounded page"
                        .into(),
                );
            }
            cursor = activated;
            steps.push(FormalReadStepV1::DurableSetActivation(Box::new(step)));
        }
        if start < end {
            for position in start + 1..=end {
                if Instant::now() >= deadline {
                    return Err("HistoryUnavailable: formal history page deadline exceeded".into());
                }
                apply_ready_recoveries(
                    &mut cursor,
                    &self.permissions,
                    &mut next_permission,
                    &mut previous_permission,
                    &mut previous_emitted_in_page,
                    &mut pending_permission,
                    &mut steps,
                    max_steps,
                )?;
                let proof = match super::execution_bridge::formal_writer::read_published_proof_at(
                    &self.writer_directory,
                    position,
                )? {
                    Some(proof) => proof,
                    None if cursor.authority().durable_set_activation_digest().is_some() => {
                        return Err(
                            "HistoryUnavailable: post-activation proof is missing from exact local publication history".into(),
                        );
                    }
                    None => self
                        .restore_before(position, None, deadline)?
                        .proof()
                        .clone(),
                };
                if Instant::now() >= deadline {
                    return Err("HistoryUnavailable: formal history page deadline exceeded".into());
                }
                let branch_is_normal = proof.decision.branch == FormalCompletionBranchV1::Normal;
                let predecessor = cursor.clone();
                let completion_permission = pending_permission;
                if let Some(permission_index) = completion_permission {
                    cursor = advance_history_completion(
                        &cursor,
                        &self.permissions[permission_index],
                        &proof,
                    )?;
                } else if branch_is_normal {
                    cursor = advance_normal_history_proof(&cursor, &proof)?;
                } else {
                    return Err("history recovery permission unavailable at completion".into());
                }
                pending_permission = None;
                previous_permission = None;
                previous_emitted_in_page = false;
                if steps.len() >= max_steps {
                    return Err(
                        "HistoryUnavailable: recovery transitions require another bounded page"
                            .into(),
                    );
                }
                steps.push(FormalReadStepV1::Commit(Box::new(proof)));
                if let Some(marker) = self.activation_markers.read(position)? {
                    // The boundary is first qualified under the old authority;
                    // an activation is a separate page step and must never
                    // replace an unresolved recovery completion.
                    if completion_permission.is_some() || !branch_is_normal {
                        return Err(
                            "HistoryUnavailable: healthy durable-set activation overlaps recovery completion".into(),
                        );
                    }
                    if cursor.checkpoint() == target.checkpoint() {
                        continue;
                    }
                    if steps.len() >= max_steps {
                        return Err(
                            "HistoryUnavailable: durable-set activation requires another bounded page"
                                .into(),
                        );
                    }
                    let (activated, _) =
                        advance_durable_set_marker(&predecessor, &cursor, &marker.step)?;
                    cursor = activated;
                    steps.push(FormalReadStepV1::DurableSetActivation(Box::new(
                        marker.step,
                    )));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: formal history page deadline exceeded".into());
        }
        // A recovery can be sealed exactly at the requested target cut. Emit
        // its evidence after the final commit so readers can derive the target
        // active authority without requesting a nonexistent next position.
        apply_ready_recoveries(
            &mut cursor,
            &self.permissions,
            &mut next_permission,
            &mut previous_permission,
            &mut previous_emitted_in_page,
            &mut pending_permission,
            &mut steps,
            max_steps,
        )?;
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: formal history page deadline exceeded".into());
        }
        require_exact_history_target(&cursor, target)?;
        let history = FormalReadHistoryV1 {
            schema_version: 1,
            steps,
        };
        history.encode_hex()?;
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: formal history page deadline exceeded".into());
        }
        Ok(history)
    }

    fn full_sync_material(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<ClosedRecord, String> {
        if anchor.authority().durable_set_activation_digest().is_some()
            || !self.activation_markers.positions()?.is_empty()
        {
            let deadline = Instant::now()
                .checked_add(self.timeout)
                .ok_or("HistoryUnavailable: full-sync replay deadline overflow")?;
            let requested = FormalReadAnchorRefV1 {
                position: anchor.head().record().position,
                record_id: anchor.head().head().record_id.clone(),
                active_authority_id: anchor
                    .authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?,
            };
            let replayed = self.replay_exact_origin(&requested, deadline)?;
            if replayed.anchor.checkpoint() != anchor.checkpoint() {
                return Err(
                    "HistoryUnavailable: full-sync target differs from external replay".into(),
                );
            }
            return self.material_for_replayed_cut(&replayed);
        }
        let position = anchor.head().record().position;
        let head = anchor.head().head();
        let record_id = head.record_id.as_str();
        let restored = self.restore(position, Some(record_id))?;
        if restored.commit().record() != anchor.head().record()
            || restored.commit().finalizing_authority_id()
                != anchor.head().finalizing_authority_id()
        {
            return Err("HistoryUnavailable: full-sync native cut differs from publication".into());
        }
        let verified_anchor = self.anchor_for(&restored)?;
        if verified_anchor
            .authority()
            .authority_id()
            .map_err(|e| e.to_string())?
            != anchor
                .authority()
                .authority_id()
                .map_err(|e| e.to_string())?
            || verified_anchor.head().record() != anchor.head().record()
            || verified_anchor.head().finalizing_authority_id()
                != anchor.head().finalizing_authority_id()
        {
            return Err("HistoryUnavailable: full-sync authority context differs".into());
        }
        self.world_for(&verified_anchor, restored.material())?;
        Ok(restored.material().clone())
    }

    fn full_sync_business_object(
        &self,
        _anchor: &VerifiedFormalReadAnchorV1,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("formal full-sync CAS digest shape".into());
        }
        let object = self.business_reader.read_node(digest)?;
        if object
            .as_ref()
            .is_some_and(|bytes| blake3::hash(bytes).to_hex().as_str() != digest)
        {
            return Err("formal full-sync retained CAS hash mismatch".into());
        }
        Ok(object)
    }
}

#[cfg(test)]
#[path = "formal_live_history_tests.rs"]
mod tests;
