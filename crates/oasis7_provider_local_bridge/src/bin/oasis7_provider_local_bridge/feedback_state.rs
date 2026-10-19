use super::ContinuousAgentResponseContextV1;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use oasis7_agent_api::{ContinuousAgentRequestContextV1, FeedbackEnvelopeV1};
use serde::{Deserialize, Serialize};

use super::{
    AcceptedRequestIdentity, FEEDBACK_STATE_SCHEMA_VERSION, FeedbackPartition,
    MAX_ACCEPTED_REQUESTS, MAX_RECENT_FEEDBACK, ProviderState,
};

const MAX_FEEDBACK_ID_BYTES: usize = 256;
const MAX_DIAGNOSTIC_REASON_BYTES: usize = 256;
const PENDING_DIAGNOSTIC_REASONS: &[&str] = &[
    "recovery_pending",
    "retry_scheduled",
    "scheduler_backpressure",
];
const REJECTED_DIAGNOSTIC_REASONS: &[&str] = &[
    "stale_base",
    "expired",
    "stale_capability_snapshot",
    "authority_denied",
    "intent_conflict",
    "reorg_invalidated",
    "finality_anchor_mismatch",
    "precondition_failed",
    "action_rejected",
    "idempotency_conflict",
    "no_effect",
    "cancelled",
    "late_response_after_cancel",
    "legacy_no_cognition_proof",
    "cognition_context_mismatch",
];
const FAILED_DIAGNOSTIC_REASONS: &[&str] = &[
    "failed_provider",
    "failed_persist",
    "cognition_failed",
    "provider_unavailable",
];

fn bounded_nonempty(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

pub(super) fn validate_feedback_contract(feedback: &FeedbackEnvelopeV1) -> Result<(), String> {
    if !bounded_nonempty(feedback.feedback_id.as_str(), MAX_FEEDBACK_ID_BYTES)
        || feedback.feedback_seq == 0
        || !bounded_nonempty(feedback.agent_subject.as_str(), MAX_FEEDBACK_ID_BYTES)
        || !bounded_nonempty(feedback.agent_session_id.as_str(), MAX_FEEDBACK_ID_BYTES)
        || !bounded_nonempty(feedback.agent_turn_id.as_str(), MAX_FEEDBACK_ID_BYTES)
        || !bounded_nonempty(feedback.decision_request_id.as_str(), MAX_FEEDBACK_ID_BYTES)
        || !feedback.request_digest.is_canonical_blake3()
        || feedback.provenance != "runtime_authoritative"
        || !matches!(
            feedback.status.as_str(),
            "pending" | "committed" | "rejected" | "failed"
        )
    {
        return Err("feedback_contract_invalid".to_string());
    }
    if feedback.candidate_action_id.is_some() || feedback.runtime_receipt_id.is_some() {
        return Err("feedback_disposition_unverifiable".to_string());
    }
    if feedback
        .reject_reason
        .as_deref()
        .is_some_and(|reason| !bounded_nonempty(reason, MAX_DIAGNOSTIC_REASON_BYTES))
    {
        return Err("feedback_disposition_reason_invalid".to_string());
    }
    let reason_is_allowed = match feedback.status.as_str() {
        "pending" => feedback
            .reject_reason
            .as_deref()
            .is_none_or(|reason| PENDING_DIAGNOSTIC_REASONS.contains(&reason)),
        "rejected" => feedback
            .reject_reason
            .as_deref()
            .is_some_and(|reason| REJECTED_DIAGNOSTIC_REASONS.contains(&reason)),
        "failed" => feedback
            .reject_reason
            .as_deref()
            .is_some_and(|reason| FAILED_DIAGNOSTIC_REASONS.contains(&reason)),
        // Runtime receipt verification is deliberately not implemented in
        // this bridge, so committed/action-bearing input is never admitted.
        "committed" => false,
        _ => false,
    };
    if feedback.status == "committed" {
        return Err("feedback_disposition_unverifiable".to_string());
    }
    if !reason_is_allowed {
        return Err("feedback_disposition_reason_invalid".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedFeedbackPartition {
    agent_subject: String,
    agent_session_id: String,
    partition: FeedbackPartition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedFeedbackState {
    schema_version: u16,
    accepted_requests: BTreeMap<String, AcceptedRequestIdentity>,
    partitions: Vec<PersistedFeedbackPartition>,
}

pub(super) fn persist_cognition_state(
    path: Option<&Path>,
    accepted_requests: &BTreeMap<String, AcceptedRequestIdentity>,
    recent_feedback: &BTreeMap<(String, String), FeedbackPartition>,
) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let partitions = recent_feedback
        .iter()
        .map(
            |((agent_subject, agent_session_id), partition)| PersistedFeedbackPartition {
                agent_subject: agent_subject.clone(),
                agent_session_id: agent_session_id.clone(),
                partition: partition.clone(),
            },
        )
        .collect::<Vec<_>>();
    let state = PersistedFeedbackState {
        schema_version: FEEDBACK_STATE_SCHEMA_VERSION,
        accepted_requests: accepted_requests.clone(),
        partitions,
    };
    let bytes = serde_json::to_vec(&state)
        .map_err(|err| format!("serialize provider feedback state failed: {err}"))?;
    let temporary_path = state_temp_path(path);
    fs::write(&temporary_path, bytes)
        .map_err(|err| format!("write provider feedback state failed: {err}"))?;
    fs::rename(&temporary_path, path).map_err(|err| {
        let _ = fs::remove_file(&temporary_path);
        format!("replace provider feedback state failed: {err}")
    })
}

pub(super) fn remember_accepted_response(
    state: &ProviderState,
    context: &ContinuousAgentRequestContextV1,
    response: &ContinuousAgentResponseContextV1,
) -> Result<(), String> {
    let mut accepted_requests = state
        .accepted_requests
        .lock()
        .expect("accepted requests lock");
    if let Some(previous) = accepted_requests.get(&context.decision_request_id) {
        if previous.agent_subject != context.agent_subject
            || previous.agent_session_id != context.agent_session_id
            || previous.agent_turn_id != context.agent_turn_id
            || previous.request_digest != context.request_digest
        {
            return Err("request_identity_collision".to_string());
        }
        if previous.response_digest != response.response_digest {
            return Err("response_identity_collision".to_string());
        }
        return Ok(());
    }
    let partitions = state.recent_feedback.lock().expect("recent_feedback lock");
    let previous_requests = accepted_requests.clone();
    let accepted_order = accepted_requests
        .values()
        .map(|accepted| accepted.accepted_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    accepted_requests.insert(
        context.decision_request_id.clone(),
        AcceptedRequestIdentity {
            agent_subject: context.agent_subject.clone(),
            agent_session_id: context.agent_session_id.clone(),
            agent_turn_id: context.agent_turn_id.clone(),
            decision_request_id: context.decision_request_id.clone(),
            request_digest: context.request_digest.clone(),
            response_digest: response.response_digest.clone(),
            accepted_order,
        },
    );
    while accepted_requests.len() > MAX_ACCEPTED_REQUESTS {
        let Some(oldest_request_id) = accepted_requests
            .iter()
            .min_by(|(left_id, left), (right_id, right)| {
                left.accepted_order
                    .cmp(&right.accepted_order)
                    .then_with(|| left_id.cmp(right_id))
            })
            .map(|(request_id, _)| request_id.clone())
        else {
            break;
        };
        accepted_requests.remove(&oldest_request_id);
    }
    if let Err(error) = persist_cognition_state(
        state.feedback_state_path.as_deref(),
        &accepted_requests,
        &partitions,
    ) {
        *accepted_requests = previous_requests;
        return Err(format!("feedback_state_persist_failed: {error}"));
    }
    // Keep the lock acquisition order explicit for feedback/response races:
    // accepted request lineage always precedes feedback partitions.
    drop(partitions);
    Ok(())
}

fn state_temp_path(path: &Path) -> PathBuf {
    let mut temporary_path = path.as_os_str().to_os_string();
    temporary_path.push(format!(".tmp.{}", std::process::id()));
    PathBuf::from(temporary_path)
}

#[expect(
    clippy::type_complexity,
    reason = "Stable callback and channel topology remains explicit at this integration seam."
)]
pub(super) fn load_feedback_state(
    path: Option<&Path>,
) -> Result<
    (
        BTreeMap<String, AcceptedRequestIdentity>,
        BTreeMap<(String, String), FeedbackPartition>,
    ),
    String,
> {
    let Some(path) = path else {
        return Ok((BTreeMap::new(), BTreeMap::new()));
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        }
        Err(error) => {
            return Err(format!(
                "read provider feedback state `{}` failed: {error}",
                path.display()
            ));
        }
    };
    let state: PersistedFeedbackState =
        serde_json::from_slice(bytes.as_slice()).map_err(|err| {
            format!(
                "decode provider feedback state `{}` failed: {err}",
                path.display()
            )
        })?;
    if state.schema_version != FEEDBACK_STATE_SCHEMA_VERSION
        || state.accepted_requests.len() > MAX_ACCEPTED_REQUESTS
        || state.partitions.len() > MAX_ACCEPTED_REQUESTS
    {
        return Err(format!(
            "provider feedback state `{}` has unsupported schema or exceeds bounds",
            path.display()
        ));
    }
    for (request_id, accepted) in &state.accepted_requests {
        if request_id.trim().is_empty()
            || accepted.decision_request_id != *request_id
            || accepted.agent_subject.trim().is_empty()
            || accepted.agent_session_id.trim().is_empty()
            || accepted.agent_turn_id.trim().is_empty()
            || !accepted.request_digest.is_canonical_blake3()
            || !accepted.response_digest.is_canonical_blake3()
        {
            return Err(format!(
                "provider feedback state `{}` contains an invalid accepted request",
                path.display()
            ));
        }
    }
    let mut partitions = BTreeMap::new();
    for persisted in state.partitions {
        if persisted.agent_subject.trim().is_empty()
            || persisted.agent_session_id.trim().is_empty()
            || (persisted.partition.next_seq == 0 && !persisted.partition.digest_by_seq.is_empty())
            || persisted.partition.digest_by_seq.len() > MAX_RECENT_FEEDBACK
            || persisted.partition.digest_by_id.len() > MAX_RECENT_FEEDBACK
            || persisted.partition.held.len() > MAX_RECENT_FEEDBACK
            || persisted.partition.digest_by_seq.len() != persisted.partition.digest_by_id.len()
            || persisted
                .partition
                .digest_by_seq
                .keys()
                .next()
                .is_some_and(|seq| *seq == 0)
        {
            return Err(format!(
                "provider feedback state `{}` contains an invalid partition",
                path.display()
            ));
        }
        let max_recorded_seq = persisted
            .partition
            .digest_by_seq
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0);
        if max_recorded_seq != persisted.partition.next_seq {
            return Err(format!(
                "provider feedback state `{}` contains an inconsistent sequence cursor",
                path.display()
            ));
        }
        for (sequence, (feedback_id, digest)) in &persisted.partition.digest_by_seq {
            if feedback_id.trim().is_empty()
                || !digest.is_canonical_blake3()
                || persisted.partition.digest_by_id.get(feedback_id) != Some(digest)
            {
                return Err(format!(
                    "provider feedback state `{}` contains invalid feedback identity",
                    path.display()
                ));
            }
            if *sequence == 0 {
                return Err(format!(
                    "provider feedback state `{}` contains an invalid feedback sequence",
                    path.display()
                ));
            }
        }
        for (feedback_id, digest) in &persisted.partition.digest_by_id {
            if feedback_id.trim().is_empty() || !digest.is_canonical_blake3() {
                return Err(format!(
                    "provider feedback state `{}` contains invalid feedback digest",
                    path.display()
                ));
            }
        }
        for (sequence, feedback) in &persisted.partition.held {
            if *sequence == 0
                || feedback.feedback_seq != *sequence
                || feedback.agent_subject != persisted.agent_subject
                || feedback.agent_session_id != persisted.agent_session_id
                || validate_feedback_contract(feedback).is_err()
            {
                return Err(format!(
                    "provider feedback state `{}` contains an invalid held feedback",
                    path.display()
                ));
            }
        }
        let key = (
            persisted.agent_subject.clone(),
            persisted.agent_session_id.clone(),
        );
        if partitions.insert(key, persisted.partition).is_some() {
            return Err(format!(
                "provider feedback state `{}` contains a duplicate partition",
                path.display()
            ));
        }
    }
    Ok((state.accepted_requests, partitions))
}
