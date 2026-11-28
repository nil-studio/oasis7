//! Rebuilds bounded feedback replay metadata from Runtime's durable outbox.
//!
//! Recovery imports identity digests only. It never replays feedback through
//! the memory/receipt consumer, because those effects already belong to the
//! Runtime-acknowledged delivery.

use std::collections::{BTreeMap, BTreeSet};

use oasis7_runtime::RuntimeFeedbackOutboxRecordV1;

use super::{
    AgentCognitionStore, CognitionError, FeedbackEnvelopeV1, MAX_FEEDBACK_REPLAY_ENTRIES,
    feedback_digest, validate_feedback_contract,
};

impl AgentCognitionStore {
    /// Hydrate replay/collision metadata from the complete Runtime outbox.
    /// Invalid durable records fail closed; incomplete or unacknowledged
    /// histories fence only their own `(agent, session)` from fresh admission.
    pub fn restore_runtime_feedback_outbox(
        &mut self,
        records: &[RuntimeFeedbackOutboxRecordV1],
    ) -> Result<(), CognitionError> {
        let mut metadata = Vec::new();
        for record in records {
            record.validate().map_err(|error| {
                CognitionError::new("feedback_recovery_record_invalid", error.to_string())
            })?;
            let feedback = serde_json::from_value::<FeedbackEnvelopeV1>(record.payload.clone())
                .map_err(|error| {
                    CognitionError::new("feedback_recovery_record_invalid", error.to_string())
                })?;
            metadata.push((
                feedback,
                record.envelope_digest.clone(),
                record.state.clone(),
            ));
        }
        self.restore_preverified_scoped_feedback_history(&metadata)
    }

    /// Metadata from an authenticated scoped View, whose original durable
    /// records and receipt lineage were validated by Runtime. The original
    /// digest is retained; private projections are never reconstructed.
    pub(crate) fn restore_preverified_scoped_feedback_history(
        &mut self,
        records: &[(FeedbackEnvelopeV1, String, String)],
    ) -> Result<(), CognitionError> {
        let mut groups = BTreeMap::<(String, String), Vec<(String, FeedbackEnvelopeV1)>>::new();
        let mut id_partitions = BTreeMap::<String, BTreeSet<(String, String)>>::new();
        for (feedback, original_digest, state) in records {
            validate_feedback_contract(feedback)?;
            let digest = original_digest.strip_prefix("blake3:").unwrap_or_default();
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || !matches!(state.as_str(), "pending" | "in_flight" | "acked")
            {
                return Err(CognitionError::new(
                    "feedback_recovery_record_invalid",
                    "invalid scoped replay metadata",
                ));
            }
            let key = (
                feedback.agent_subject.clone(),
                feedback.agent_session_id.clone(),
            );
            id_partitions
                .entry(feedback.feedback_id.clone())
                .or_default()
                .insert(key.clone());
            groups
                .entry(key)
                .or_default()
                .push((state.clone(), feedback.clone()));
        }

        let colliding_partitions = id_partitions
            .values()
            .filter(|partitions| partitions.len() > 1)
            .flat_map(|partitions| partitions.iter().cloned())
            .collect::<BTreeSet<_>>();

        for (key, entries) in &mut groups {
            entries.sort_by_key(|(_, feedback)| feedback.feedback_seq);
            let mut reason = colliding_partitions.contains(key).then(|| {
                "Runtime feedback identity is reused by another Agent/session partition".to_string()
            });
            let mut seqs = BTreeSet::new();
            let mut ids = BTreeSet::new();
            for (index, (state, feedback)) in entries.iter().enumerate() {
                if !seqs.insert(feedback.feedback_seq) || !ids.insert(feedback.feedback_id.as_str())
                {
                    reason = Some(
                        "Runtime feedback history contains a duplicate sequence or identity"
                            .to_string(),
                    );
                    break;
                }
                let expected_seq = index as u64 + 1;
                if feedback.feedback_seq != expected_seq {
                    reason = Some(format!(
                        "Runtime feedback history has a sequence gap: expected {expected_seq}"
                    ));
                    break;
                }
                if state != "acked" {
                    reason = Some(format!(
                        "Runtime feedback {} is not acknowledged yet",
                        feedback.feedback_id
                    ));
                    break;
                }
            }
            if reason.is_none()
                && entries.last().is_some_and(|(_, feedback)| {
                    !matches!(
                        feedback.status.as_str(),
                        "committed" | "rejected" | "failed"
                    )
                })
            {
                reason = Some(
                    "Runtime feedback history ends with a nonterminal disposition".to_string(),
                );
            }

            let maximum_seq = entries
                .last()
                .map(|(_, feedback)| feedback.feedback_seq)
                .unwrap_or_default();
            let authoritative_by_seq = entries
                .iter()
                .map(|(_, feedback)| {
                    (
                        feedback.feedback_seq,
                        (feedback.feedback_id.as_str(), feedback_digest(feedback)),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            if let Some(existing) = self.feedback_partitions.get(key) {
                if existing.next_seq > maximum_seq || !existing.held.is_empty() {
                    reason.get_or_insert_with(|| {
                        "live feedback verifier state is ahead of or held beyond Runtime history".to_string()
                    });
                }
                if existing
                    .digest_by_seq
                    .iter()
                    .any(|(seq, (feedback_id, digest))| {
                        authoritative_by_seq
                            .get(seq)
                            .is_none_or(|(source_id, source_digest)| {
                                *source_id != feedback_id.as_str() || source_digest != digest
                            })
                    })
                {
                    reason.get_or_insert_with(|| {
                        "live feedback verifier identity conflicts with Runtime history".to_string()
                    });
                }
            }

            if let Some(reason) = reason {
                self.feedback_recovery_blocked.insert(key.clone(), reason);
                continue;
            }

            let partition = self.feedback_partitions.entry(key.clone()).or_default();
            partition.next_seq = maximum_seq;
            partition.digest_by_id.clear();
            partition.digest_by_seq.clear();
            partition.replay_order.clear();
            for (_, feedback) in entries.iter().rev().take(MAX_FEEDBACK_REPLAY_ENTRIES).rev() {
                let digest = feedback_digest(feedback);
                partition.digest_by_seq.insert(
                    feedback.feedback_seq,
                    (feedback.feedback_id.clone(), digest.clone()),
                );
                partition
                    .digest_by_id
                    .insert(feedback.feedback_id.clone(), digest);
                partition
                    .replay_order
                    .push_back(feedback.feedback_id.clone());
            }
            self.feedback_recovery_blocked.remove(key);
        }

        for (key, existing) in &self.feedback_partitions {
            if existing.next_seq > 0 && !groups.contains_key(key) {
                self.feedback_recovery_blocked.insert(
                    key.clone(),
                    "Runtime outbox does not contain the live feedback verifier history"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}
