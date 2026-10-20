//! Typed deterministic replay from inputs saved at both engineering endpoints.
use super::*;
use oasis7_distfs::controlled_authority::replicated_protocol::{
    FrozenCertificate, FrozenInputBody, FrozenOutcome, verify_evidence,
};

impl GuardedExecutionDriver {
    fn original_predecessor_record(&self, p: &PendingTransaction) -> Result<ClosedRecord, String> {
        let previous = history::load_at_height(
            &self.authority,
            &self.records,
            p.continuation.predecessor_committed_height,
        )?;
        let path = if previous.head.position == 1 {
            self.authority.config.activation_evidence.clone()
        } else {
            history::evidence_path(&self.records, previous.head.position)
        };
        let bytes = crate::controlled_history_cli::read_bounded(&path, 64 * 1024 * 1024)?;
        let evidence: oasis7_distfs::controlled_authority::replicated_protocol::DurabilityEvidence =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let parent = oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor {
            position: evidence
                .proposal
                .body
                .position
                .checked_sub(1)
                .ok_or("zero predecessor position")?,
            decision_hash: evidence.proposal.body.parent_hash.clone(),
            qualified: false,
        };
        if verify_evidence(&evidence, &self.authority.policy.trust, &parent)
            .map_err(|e| e.to_string())?
            != previous.head
        {
            return Err("frozen predecessor proof changed".into());
        }
        Ok(evidence.proposal.body.record)
    }
    fn input_body(&self, p: &PendingTransaction) -> Result<FrozenInputBody, String> {
        let previous = history::load_at_height(
            &self.authority,
            &self.records,
            p.continuation.predecessor_committed_height,
        )?;
        let context = &p.continuation.context;
        if p.continuation.schema_version != 1
            || context.world_id != self.authority.policy.trust.world_id
            || context.node_id != self.authority.config.node_id
            || context.node_id != context.proposer_id
            || p.continuation.predecessor_execution_height
                != p.continuation.predecessor_committed_height
            || p.continuation.predecessor_committed_height.checked_add(1) != Some(context.height)
            || p.continuation.predecessor_execution_block_hash.as_deref()
                != Some(previous.record.execution_block_hash.as_str())
            || p.continuation.predecessor_execution_state_root.as_deref()
                != Some(previous.record.execution_state_root.as_str())
            || p.continuation.reserved_action_bytes
                != context
                    .committed_actions
                    .iter()
                    .try_fold(0usize, |n, action| n.checked_add(action.payload_cbor.len()))
                    .ok_or("frozen action bytes overflow")?
            || oasis7_node::compute_consensus_action_root(&context.committed_actions)
                .map_err(|e| format!("{e:?}"))?
                != context.action_root
        {
            return Err("original frozen continuation binding mismatch".into());
        }
        for action in &context.committed_actions {
            action.validate().map_err(|e| format!("{e:?}"))?;
        }
        let (_, simulator, bootstrap, _) =
            super::super::driver_replicated_input::decode_committed_actions(context)?;
        if !simulator.is_empty() || bootstrap.is_some() {
            return Err("frozen execution rejects simulator/bootstrap mutations".into());
        }
        Ok(FrozenInputBody {
            schema_version: 1,
            scope: oasis7_distfs::controlled_authority::replicated_protocol::EVIDENCE_SCOPE.into(),
            trust: self.authority.policy.trust.clone(),
            next_position: previous
                .head
                .position
                .checked_add(1)
                .ok_or("frozen position exhausted")?,
            parent: previous.head,
            request: history::request(&p.continuation)?,
            predecessor_record: self.original_predecessor_record(p)?,
            execution_binding: self.authority.frozen_execution_digest.clone(),
            continuation_bytes: to_cbor(&p.continuation)?,
            action_bytes: to_cbor(&p.continuation.context.committed_actions)?,
        })
    }
    pub(super) fn retained_frozen(
        &self,
        p: &PendingTransaction,
    ) -> Result<FrozenCertificate, String> {
        let bytes = self
            .private_store
            .get_verified(
                p.frozen_certificate_ref
                    .as_deref()
                    .ok_or("original frozen certificate is incomplete")?,
            )
            .map_err(|e| format!("{e:?}"))?;
        let certificate: FrozenCertificate = capture::decode_generic(&bytes)?;
        certificate
            .verify(&self.authority.policy.trust)
            .map_err(|e| e.to_string())?;
        if certificate.input.body != self.input_body(p)? {
            return Err("original frozen input or execution binding changed".into());
        }
        Ok(certificate)
    }
    /// A reply loss before the local certificate write is reconciled by the
    /// same signed input. Execution never begins until both certificate saves.
    pub(super) fn freeze_original(&mut self) -> Result<bool, String> {
        let p = self
            .pending
            .as_ref()
            .ok_or("original continuation missing")?
            .clone();
        let body = self.input_body(&p)?;
        let FrozenOutcome::Complete(certificate) = self
            .coordinator
            .freeze_input(body.clone())
            .map_err(|e| e.to_string())?
        else {
            return Ok(false);
        };
        if certificate.input.body != body {
            return Err("frozen original input differs".into());
        }
        let reference =
            capture::persist_capture_bytes(&self.private_store, &to_cbor(&certificate)?)?;
        if p.frozen_certificate_ref
            .as_ref()
            .is_some_and(|r| r != &reference)
        {
            return Err("original frozen certificate changed".into());
        }
        let mut complete = p;
        complete.frozen_certificate_ref = Some(reference);
        self.persist_pending(&complete)?;
        self.pending = Some(complete);
        Ok(true)
    }
}
