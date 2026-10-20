use std::path::Path;

use super::super::publication_lifecycle::{
    LifecycleError, PublicationHeadBinding, PublicationLifecycleSnapshot,
    SEQUENCER_HEAD_PUBLICATION_GRACE_MS, has_complete_publication_quorum_at_height, load_snapshot,
    validate_current_catch_up_binding,
};
use super::super::publication_proof::{
    PublicationExecutionRecord, PublicationProofError, PublicationProofErrorKind,
    evaluate_publication_episode,
};
use oasis7_node::NodeSnapshot;

use super::{
    ChainConsensusNetworkHeadStatus, ChainNodeObservabilityAlert, ChainNodeObservabilityStatus,
    ChainReadinessPolicyStatus, observability_status_for_alerts, observability_summary_for_alerts,
    push_local_chain_ahead_alert, push_observability_alert,
    sequencer_head_publication_pending_summary,
};

pub(super) fn push_publication_or_divergence_alert(
    alerts: &mut Vec<ChainNodeObservabilityAlert>,
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    policy: &ChainReadinessPolicyStatus,
    observed_at_unix_ms: i64,
) {
    if let Some(summary) = sequencer_head_publication_pending_summary(
        snapshot,
        network_head,
        policy,
        observed_at_unix_ms,
    ) {
        push_observability_alert(
            alerts,
            "warn",
            "sequencer_head_publication_pending",
            summary,
        );
        return;
    }

    let observed_network_height = network_head
        .height
        .or_else(|| network_head.peer_heads.iter().map(|peer| peer.height).max());
    push_local_chain_ahead_alert(alerts, snapshot, observed_network_height);
}

pub(super) fn enforce_retained_publication_proof(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    policy: &ChainReadinessPolicyStatus,
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    observed_at_unix_ms: i64,
    observability: &mut ChainNodeObservabilityStatus,
) {
    if policy.tier != "public_testnet" || policy.role != "sequencer" {
        return;
    }
    if has_complete_publication_quorum_at_height(
        snapshot,
        network_head,
        snapshot.consensus.committed_height,
        true,
    ) {
        if let Err(error) = validate_current_catch_up_binding(snapshot, execution_world_dir) {
            reject_publication_proof(
                snapshot,
                network_head,
                None,
                PublicationProofRejection::from_lifecycle_error(error),
                observability,
            );
        }
        return;
    }

    let warning_index = observability
        .alerts
        .iter()
        .position(|alert| alert.code == "sequencer_head_publication_pending");
    if warning_index.is_none() && !is_publication_proof_candidate(snapshot, network_head, policy) {
        return;
    }

    let decision = execution_records_dir
        .ok_or_else(|| PublicationProofRejection::new(Reason::RecordMissing, "records_dir"))
        .and_then(|records_dir| {
            let durable_state = load_snapshot(execution_world_dir)
                .map_err(PublicationProofRejection::from_lifecycle_error)?;
            derive_publication_episode(
                snapshot,
                network_head,
                records_dir,
                durable_state.as_ref(),
                observed_at_unix_ms,
            )
        });
    match decision {
        Ok(proof) => {
            if let Some(index) = warning_index {
                observability.alerts[index].summary.push_str(
                    format!(
                        " episode_started_at_ms={} episode_age_ms={}",
                        proof.episode.timestamp_ms, proof.episode_age_ms
                    )
                    .as_str(),
                );
            }
        }
        Err(rejection) => reject_publication_proof(
            snapshot,
            network_head,
            warning_index,
            rejection,
            observability,
        ),
    }
}

fn derive_publication_episode(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    records_dir: &Path,
    durable_state: Option<&PublicationLifecycleSnapshot>,
    observed_at_unix_ms: i64,
) -> Result<PublicationEpisodeProof, PublicationProofRejection> {
    let evaluation = evaluate_publication_episode(
        snapshot,
        network_head,
        records_dir,
        durable_state,
        observed_at_unix_ms,
    )?;
    if durable_state.is_none() {
        return Err(PublicationProofRejection::new(
            Reason::StatePersistPending,
            "state_missing",
        ));
    }
    validate_grace(
        evaluation.episode,
        evaluation.episode_binding,
        observed_at_unix_ms,
    )
}

fn validate_grace(
    episode: PublicationExecutionRecord,
    episode_binding: PublicationHeadBinding,
    observed_at_unix_ms: i64,
) -> Result<PublicationEpisodeProof, PublicationProofRejection> {
    let episode_age_ms = observed_at_unix_ms
        .saturating_sub(episode.timestamp_ms)
        .max(0);
    if episode_age_ms > SEQUENCER_HEAD_PUBLICATION_GRACE_MS {
        return Err(PublicationProofRejection::new(
            Reason::GraceExpired,
            format!("episode_age_ms={episode_age_ms}"),
        ));
    }
    Ok(PublicationEpisodeProof {
        episode: episode_binding,
        episode_age_ms,
    })
}

fn is_publication_proof_candidate(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    policy: &ChainReadinessPolicyStatus,
) -> bool {
    let local_height = snapshot.consensus.committed_height;
    let Some(parent_height) = local_height.checked_sub(1) else {
        return false;
    };
    policy.tier == "public_testnet"
        && policy.role == "sequencer"
        && has_complete_publication_quorum_at_height(snapshot, network_head, parent_height, false)
}

fn reject_publication_proof(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    warning_index: Option<usize>,
    rejection: PublicationProofRejection,
    observability: &mut ChainNodeObservabilityStatus,
) {
    if let Some(index) = warning_index {
        observability.alerts.remove(index);
    }
    if !observability
        .alerts
        .iter()
        .any(|alert| alert.code == "local_chain_ahead_of_network_head")
    {
        push_local_chain_ahead_alert(&mut observability.alerts, snapshot, network_head.height);
    }
    push_observability_alert(
        &mut observability.alerts,
        "critical",
        "sequencer_head_publication_proof_rejected",
        rejection.summary(),
    );
    observability.status = observability_status_for_alerts(observability.alerts.as_slice());
    observability.summary = observability_summary_for_alerts(observability.alerts.as_slice());
    observability.ready = false;
}

struct PublicationEpisodeProof {
    episode: PublicationHeadBinding,
    episode_age_ms: i64,
}

struct PublicationProofRejection {
    reason: Reason,
    detail: String,
}

impl PublicationProofRejection {
    fn new(reason: Reason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }

    fn summary(&self) -> String {
        format!(
            "sequencer publication proof rejected: reason={} detail={}",
            self.reason.as_str(),
            self.detail
        )
    }

    fn from_lifecycle_error(error: LifecycleError) -> Self {
        let reason = lifecycle_rejection_reason(error.reason);
        Self::new(reason, error.detail)
    }
}

fn lifecycle_rejection_reason(reason: &str) -> Reason {
    match reason {
        "state_malformed" => Reason::StateMalformed,
        "state_persist_failed" => Reason::StatePersistFailed,
        "state_persist_pending" => Reason::StatePersistPending,
        _ => Reason::StateBindingInvalid,
    }
}

#[cfg(test)]
pub(crate) fn publication_lifecycle_rejection_reason(reason: &str) -> &'static str {
    lifecycle_rejection_reason(reason).as_str()
}

impl From<PublicationProofError> for PublicationProofRejection {
    fn from(error: PublicationProofError) -> Self {
        let reason = match error.kind {
            PublicationProofErrorKind::RecordMissing => Reason::RecordMissing,
            PublicationProofErrorKind::RecordMalformed => Reason::RecordMalformed,
            PublicationProofErrorKind::ContinuityInvalid => Reason::ContinuityInvalid,
            PublicationProofErrorKind::AncestryInvalid => Reason::AncestryInvalid,
            PublicationProofErrorKind::ChronologyInvalid => Reason::ChronologyInvalid,
            PublicationProofErrorKind::TimestampMismatch => Reason::TimestampMismatch,
            PublicationProofErrorKind::BindingInvalid => Reason::BindingInvalid,
            PublicationProofErrorKind::StateBindingInvalid => Reason::StateBindingInvalid,
            PublicationProofErrorKind::ScanLimitExceeded => Reason::ScanLimitExceeded,
        };
        Self::new(reason, error.detail)
    }
}

#[derive(Clone, Copy)]
enum Reason {
    RecordMissing,
    RecordMalformed,
    TimestampMismatch,
    ChronologyInvalid,
    AncestryInvalid,
    ContinuityInvalid,
    BindingInvalid,
    ScanLimitExceeded,
    GraceExpired,
    StateMalformed,
    StatePersistFailed,
    StatePersistPending,
    StateBindingInvalid,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Self::RecordMissing => "record_missing",
            Self::RecordMalformed => "record_malformed",
            Self::TimestampMismatch => "timestamp_mismatch",
            Self::ChronologyInvalid => "chronology_invalid",
            Self::AncestryInvalid => "ancestry_invalid",
            Self::ContinuityInvalid => "continuity_invalid",
            Self::BindingInvalid => "binding_invalid",
            Self::ScanLimitExceeded => "scan_limit_exceeded",
            Self::GraceExpired => "grace_expired",
            Self::StateMalformed => "state_malformed",
            Self::StatePersistFailed => "state_persist_failed",
            Self::StatePersistPending => "state_persist_pending",
            Self::StateBindingInvalid => "state_binding_invalid",
        }
    }
}
