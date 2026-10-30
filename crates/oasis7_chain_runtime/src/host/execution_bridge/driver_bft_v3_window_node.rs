//! Actual Node cursor over an authenticated active base and bounded suffix.
use super::*;
use oasis7_consensus::tendermint_v3::{BftHeaderValueV3, VerifiedBftHeaderCertificateV3};
use oasis7_distfs::protected_bft_v3::BftSignerBindingV3;
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3,
    NodeBftExecutionHeadV3, NodeBftPrivateCandidatePackageV3, NodeBftPrivateCandidateV3,
    NodeExecutionHook,
};
use std::collections::BTreeMap;
pub(in super::super) struct ActiveNodeDriver {
    window: Box<FormalBftHeaderActiveExecutionV1>,
    binding_digest: String,
    cursor: u64,
    configured: bool,
    membership: bool,
}
impl ActiveNodeDriver {
    pub(in super::super) fn new(
        window: Box<FormalBftHeaderActiveExecutionV1>,
        binding: &BftSignerBindingV3,
    ) -> Result<Self, String> {
        let digest = binding.digest().map_err(err)?;
        if window.base.metadata.source.binding_digest != digest
            || window.material_identity() != binding.material_identity
        {
            return Err("active Node base binding differs".into());
        }
        let cursor = window.base.metadata.cut.height;
        Ok(Self {
            window,
            binding_digest: digest,
            cursor,
            configured: false,
            membership: false,
        })
    }
    fn deadline() -> Result<Instant, String> {
        Instant::now()
            .checked_add(std::time::Duration::from_secs(120))
            .ok_or("Node private deadline overflow".into())
    }
    fn check_binding(&self, binding: &BftSignerBindingV3) -> Result<(), String> {
        if binding.digest().map_err(err)? != self.binding_digest {
            return Err("active Node binding differs".into());
        }
        Ok(())
    }
    fn reset_cursor(&mut self) -> Result<(), String> {
        self.reset_cursor_at(Self::deadline()?)
    }
    fn reset_cursor_at(&mut self, deadline: Instant) -> Result<(), String> {
        if !self.configured {
            return Err("active Node is not configured".into());
        }
        self.window.reset_through(Some(self.cursor), deadline)
    }
    fn exact_parent(&mut self, context: &NodeBftExecutionContextV3) -> Result<(), String> {
        self.exact_parent_at(context, Self::deadline()?)
    }
    fn exact_parent_at(
        &mut self,
        context: &NodeBftExecutionContextV3,
        deadline: Instant,
    ) -> Result<(), String> {
        self.reset_cursor_at(deadline)?;
        if context != &NodeBftExecutionContextV3::from_schedule(self.window.schedule()) {
            return Err("active Node protocol and actual parent differ".into());
        }
        Ok(())
    }
    fn preview(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.preview_at(context, candidate, Self::deadline()?)
    }
    fn preview_at(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
        deadline: Instant,
    ) -> Result<(), String> {
        self.exact_parent_at(context, deadline)?;
        self.window.execution = self.window.execution.fork_live_closed()?;
        let actual = self
            .window
            .execution
            .build_candidate(&candidate.actions_cbor)?;
        if actual.header != candidate.public_value.header
            || actual.commitment != candidate.public_value.execution_commitment
            || (!self.membership
                && (actual.commitment.transition.is_some()
                    || actual.commitment.transition_ref.is_some()))
        {
            return Err("active Node original candidate did not reproduce".into());
        }
        Ok(())
    }
}
impl NodeExecutionHook for ActiveNodeDriver {
    fn preflight_bft_v3_capacity(
        &mut self,
        context: &NodeBftBuildContextV3,
    ) -> Result<oasis7_node::NodeBftCapacityAdmissionV3, String> {
        use formal_dependencies::ContentNodeCapacityV1;
        use oasis7_node::NodeBftCapacityAdmissionV3 as Admission;
        self.exact_parent(&context.execution)?;
        self.window.execution = self.window.execution.fork_live_closed()?;
        let actions =
            serde_cbor::to_vec(&context.actions).map_err(|_| "encode capacity actions")?;
        let current = self.window.execution.plan_candidate(&actions)?.1;
        if matches!(current, ContentNodeCapacityV1::Fits { .. })
            && self.window.ledger.records().count() < 64
        {
            return Ok(Admission::Fits);
        }
        if self.cursor == self.window.base.metadata.cut.height {
            return Ok(Admission::Blocked);
        }
        // Discard speculative observations and use the actual strict suffix
        // replay as the new closed parent, preserving verified ancestry.
        self.reset_cursor()?;
        Ok(
            if matches!(
                self.window.execution.capacity_in_new_layer(&actions)?,
                ContentNodeCapacityV1::Fits { .. }
            ) {
                Admission::NeedsRoll
            } else {
                Admission::Blocked
            },
        )
    }

    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("legacy commit unavailable in active wire-3 driver".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        self.check_binding(binding)?;
        self.configured = true;
        self.cursor = self.window.base.metadata.cut.height;
        self.reset_cursor()?;
        Ok(None)
    }
    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        self.membership = true;
        Ok(())
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.reset_cursor()?;
        if schedule != self.window.schedule() {
            return Err("active Node schedule differs".into());
        }
        Ok(NodeBftExecutionContextV3::from_schedule(schedule))
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.exact_parent(context)?;
        self.window.private.candidate(context.height)
    }
    fn build_bft_v3_candidate(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.exact_parent(&context.execution)?;
        if self
            .window
            .private
            .candidate(context.execution.height)?
            .is_some()
        {
            return Err("active Node must resume original private selection".into());
        }
        self.window.execution = self.window.execution.fork_live_closed()?;
        let actions_cbor = serde_cbor::to_vec(&context.actions).map_err(err)?;
        let actual = self.window.execution.build_candidate(&actions_cbor)?;
        if !self.membership
            && (actual.commitment.transition.is_some()
                || actual.commitment.transition_ref.is_some())
        {
            return Err("active Node membership opt-in absent".into());
        }
        Ok(NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: actual.header,
                execution_commitment: actual.commitment,
            },
            actions_cbor,
        })
    }
    fn preview_bft_v3_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.preview(context, candidate)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        let deadline = Self::deadline()?;
        self.preview_at(context, candidate, deadline)?;
        let original = self.window.private.candidate(context.height)?;
        if original
            .as_ref()
            .is_some_and(|original| original != candidate)
        {
            return Err("active Node original selection differs".into());
        }
        let observed = self.window.execution.retained_nodes.accessed_nodes()?;
        let nodes = if original.is_some() {
            let package = self.window.private.export(&candidate.public_value)?;
            let nodes: BTreeMap<_, _> = package.dependency_nodes.into_iter().collect();
            if observed
                .iter()
                .any(|(digest, bytes)| nodes.get(digest) != Some(bytes))
            {
                return Err("active Node original dependency bytes differ".into());
            }
            nodes
        } else {
            observed
        };
        live(deadline)?;
        self.window
            .private
            .retain(candidate, &nodes, &mut self.window.ledger)
    }
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        package: &NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        let deadline = Self::deadline()?;
        if !package
            .dependency_nodes
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
        {
            return Err("active received dependency order differs".into());
        }
        let supplied: BTreeMap<_, _> = package.dependency_nodes.iter().cloned().collect();
        super::super::super::super::formal_dependencies::validate_nodes(&supplied)?;
        // Never seed an actual parent with untrusted incoming material.
        self.preview_at(context, &package.candidate, deadline)?;
        let observed = self.window.execution.retained_nodes.accessed_nodes()?;
        if observed != supplied {
            return Err("active received dependencies did not independently reproduce".into());
        }
        live(deadline)?;
        self.window
            .private
            .retain_received(&package.candidate, &observed, &mut self.window.ledger)
    }
    fn export_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        self.reset_cursor()?;
        self.window.private.export(value)
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.reset_cursor()?;
        self.window
            .private
            .candidate_for_value(value)?
            .ok_or("active original candidate absent".into())
    }
    fn lookup_bft_v3_public_value(
        &mut self,
        height: u64,
        id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        self.reset_cursor()?;
        if let Some(record) = self.window.ledger.record_at(height) {
            let certificate = record.input().certificate;
            let value = BftHeaderValueV3 {
                header: certificate.header,
                execution_commitment: certificate.execution_commitment,
            };
            return Ok((value.block_id()? == id).then_some(value));
        }
        self.window.private.public_value(height, id)
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        binding: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.check_binding(binding)?;
        self.reset_cursor()?;
        if max == 0 || max > 64 || after < self.window.base.metadata.cut.height {
            return Err("active history request range differs".into());
        }
        Ok(self
            .window
            .ledger
            .records()
            .filter(|record| record.height() > after)
            .take(max)
            .map(|record| record.input().certificate)
            .collect())
    }
    fn validate_bft_v3_static_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        validate_payload(payload)
    }
    fn validate_bft_v3_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        self.reset_cursor()?;
        validate_payload(payload)
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
        actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        let context = self.current_bft_v3_execution_context(schedule)?;
        self.window.execution = self.window.execution.fork_live_closed()?;
        let actual = self
            .window
            .execution
            .build_candidate(&serde_cbor::to_vec(&actions).map_err(err)?)?;
        if actual.commitment.height != context.height
            || (!self.membership
                && (actual.commitment.transition.is_some()
                    || actual.commitment.transition_ref.is_some()))
        {
            return Err("active Node batch policy differs".into());
        }
        Ok(())
    }
    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        self.commit_at_cursor(token)
    }
}
fn validate_payload(payload: &[u8]) -> Result<(), String> {
    let envelope =
        oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(payload)
            .map_err(err)?;
    match envelope.body {
        oasis7::consensus_action_payload::ConsensusActionPayloadBody::RuntimeAction { .. } => {
            Ok(())
        }
        _ => Err("active Node requires a closed RuntimeAction payload".into()),
    }
}
#[path = "driver_bft_v3_window_node_commit.rs"]
mod commit;
