//! Actual qualified-H Node driver. Legacy execution remains unavailable here.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_dependencies;
use super::super::super::formal_prefix_files as files;
use super::*;
use oasis7_consensus::tendermint_v3::BftHeaderValueV3;
use oasis7_distfs::protected_bft_v3::BftSignerBindingV3;
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3,
    NodeBftExecutionHeadV3, NodeBftPrivateCandidateV3, NodeExecutionHook,
};

#[path = "driver_bft_v3_node_private_store.rs"]
mod private_store;
use private_store::NodePrivateStore;

pub(crate) struct FormalBftHeaderNodeDriverV3 {
    execution: FormalBftHeaderLedgerExecutionV3,
    private: NodePrivateStore,
    initial_schedule: BftHeaderHeightScheduleV3,
    material_identity: String,
    configured_binding: Option<String>,
    cursor_height: u64,
    installed_world: RuntimeWorld,
}
impl FormalBftHeaderNodeDriverV3 {
    pub(crate) fn open_from_handoff(
        records: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        validate_storage_paths(records, store.root())?;
        let (mut private, retained_source) =
            NodePrivateStore::open(records, store.clone(), source)?;
        let mut execution = FormalBftHeaderLedgerExecutionV3::open_for_node(
            records,
            store,
            anchor,
            target,
            material,
            retained_source,
            true,
        )?;
        let initial_schedule = execution.origin.schedule.clone();
        if initial_schedule.active_set().validators().len() != 1
            || initial_schedule.pending_transition().is_some()
        {
            return Err(
                "native header Node requires one validator and no pending transition".into(),
            );
        }
        let cursor_height = execution.origin.handoff.handoff_position();
        let ledger_origin = execution.ledger.origin_digest()?;
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &ledger_origin,
                private.instance_id(),
                execution.ledger.material_pin_digest()?,
            ))
            .map_err(|_| "encode original Node material identity")?,
        );
        let nodes = execution.origin.retained_nodes.accessed_nodes()?;
        private.bind_origin(
            &ledger_origin,
            &material_identity,
            cursor_height,
            &nodes,
            &mut |bytes| execution.ledger.reserve_external_material(bytes),
        )?;
        execution
            .ledger
            .reserve_external_material(private.usage()?)?;
        let installed_world = execution.origin.parent_world.clone();
        Ok(Self {
            installed_world,
            execution,
            private,
            initial_schedule,
            material_identity,
            configured_binding: None,
            cursor_height,
        })
    }
    pub(crate) fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.execution.origin.handoff
    }
    pub(crate) fn initial_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.initial_schedule
    }
    pub(crate) fn material_identity(&self) -> &str {
        &self.material_identity
    }
    pub(crate) fn world(&self) -> &RuntimeWorld {
        &self.installed_world
    }

    fn check_binding(&self, binding: &BftSignerBindingV3) -> Result<String, String> {
        binding
            .validate()
            .map_err(|_| "Node signer binding rejected")?;
        if binding.initial_schedule() != &self.initial_schedule
            || binding.material_identity != self.material_identity
            || binding.handoff().target_digest() != self.handoff().target_digest()
            || binding.handoff().activation_digest() != self.handoff().activation_digest()
            || self.initial_schedule.active_set().validators().len() != 1
            || self.initial_schedule.pending_transition().is_some()
        {
            return Err(
                "Node signer binding differs from original actual H or supported membership".into(),
            );
        }
        let digest = binding
            .digest()
            .map_err(|_| "Node signer binding digest rejected")?;
        if self
            .configured_binding
            .as_ref()
            .is_some_and(|expected| expected != &digest)
        {
            return Err("Node restart changed its configured signer binding".into());
        }
        Ok(digest)
    }
    fn require_configured(&self) -> Result<(), String> {
        if self.configured_binding.is_none() {
            return Err("formal header Node driver is not configured".into());
        }
        Ok(())
    }
    /// Rebuild the exact parent before every preview. Failed or alternative
    /// previews cannot contaminate retained dependency observations.
    fn reset_parent(&mut self) -> Result<(), String> {
        self.execution.ledger.reload()?;
        self.execution
            .ledger
            .reserve_external_material(self.private.usage()?)?;
        let h = self.handoff().handoff_position();
        if self.cursor_height == h {
            let seed = self.private.origin_nodes()?;
            self.execution.origin = FormalBftHeaderExecutionV3::from_handoff(
                self.execution.inputs.anchor.clone(),
                self.execution.inputs.target.clone(),
                &self.execution.inputs.material,
                self.execution.inputs.source.clone(),
                seed,
                true,
            )?;
        } else {
            let record = self
                .execution
                .ledger
                .record_at(self.cursor_height)
                .ok_or("Node replay cursor lost its original canonical record")?;
            let id = record.input().certificate.header.block_id()?;
            self.execution.verified_cut(self.cursor_height, &id)?;
        }
        // Historical replay is read-only: it must reject an unrecorded node.
        // Live execution may derive new nodes, but all parent reads must still
        // resolve solely through this verified strict overlay.
        let verified_parent = self.execution.origin.retained_nodes.clone();
        let seed = verified_parent.recorded_nodes()?;
        let mut live = FormalBftHeaderExecutionV3::from_handoff(
            self.execution.inputs.anchor.clone(),
            self.execution.inputs.target.clone(),
            &self.execution.inputs.material,
            verified_parent,
            seed,
            false,
        )?;
        for record in self
            .execution
            .ledger
            .records()
            .filter(|record| record.height() <= self.cursor_height)
        {
            let entry = record.input();
            live.replay_next(&entry.certificate, &entry.actions_cbor)?;
        }
        self.execution.origin = live;
        Ok(())
    }
    fn expected(&self) -> NodeBftExecutionContextV3 {
        NodeBftExecutionContextV3::from_schedule(&self.execution.origin.schedule)
    }
    fn exact_parent(&mut self, expected: &NodeBftExecutionContextV3) -> Result<(), String> {
        self.require_configured()?;
        self.reset_parent()?;
        if &self.expected() != expected {
            return Err("Node candidate uses a stale or foreign actual parent".into());
        }
        Ok(())
    }
    fn validate_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.exact_parent(expected)?;
        let actual = self
            .execution
            .origin
            .build_candidate(&candidate.actions_cbor)
            .map_err(|_| "Node private candidate actual execution rejected")?;
        if actual.header != candidate.public_value.header
            || actual.commitment != candidate.public_value.execution_commitment
            || actual.commitment.transition.is_some()
            || actual.commitment.transition_ref.is_some()
        {
            return Err(
                "Node candidate differs from actual execution or supported membership".into(),
            );
        }
        Ok(())
    }
    fn private_candidate(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        value
            .validate_binding()
            .map_err(|_| "Node public candidate binding rejected")?;
        let height = value.execution_commitment.height;
        let candidate = if let Some(candidate) = self.private.candidate(height)? {
            candidate
        } else {
            let input = self
                .execution
                .ledger
                .record_at(height)
                .ok_or("original Node private candidate is unavailable")?
                .input();
            NodeBftPrivateCandidateV3 {
                public_value: BftHeaderValueV3 {
                    header: input.certificate.header,
                    execution_commitment: input.certificate.execution_commitment,
                },
                actions_cbor: input.actions_cbor,
            }
        };
        if &candidate.public_value != value {
            return Err("Node requested different private bytes or public candidate".into());
        }
        Ok(candidate)
    }
}

impl NodeExecutionHook for FormalBftHeaderNodeDriverV3 {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("qualified-H header driver refuses legacy execution".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        let digest = self.check_binding(binding)?;
        self.configured_binding = Some(digest);
        self.cursor_height = self.handoff().handoff_position();
        self.reset_parent()?;
        // Structural disk history cannot itself authorize a Node head.
        Ok(None)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.require_configured()?;
        self.reset_parent()?;
        if schedule != &self.execution.origin.schedule {
            return Err("Node protocol and actual execution cursors differ".into());
        }
        Ok(self.expected())
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.exact_parent(expected)?;
        self.private.candidate(expected.height)
    }
    fn build_bft_v3_candidate(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.exact_parent(&context.execution)?;
        if self.private.candidate(context.execution.height)?.is_some() {
            return Err("Node height has an original private candidate to resume".into());
        }
        let actions_cbor =
            serde_cbor::to_vec(&context.actions).map_err(|_| "encode private Node actions")?;
        let candidate = self
            .execution
            .origin
            .build_candidate(&actions_cbor)
            .map_err(|_| "Node action execution rejected")?;
        if candidate.commitment.transition.is_some()
            || candidate.commitment.transition_ref.is_some()
        {
            return Err("Node membership transitions are not installed".into());
        }
        Ok(NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: candidate.header,
                execution_commitment: candidate.commitment,
            },
            actions_cbor,
        })
    }
    fn preview_bft_v3_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.validate_candidate(expected, candidate)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        expected: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.validate_candidate(expected, candidate)?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        self.private.retain(candidate, &nodes, &mut |bytes| {
            self.execution.ledger.reserve_external_material(bytes)
        })
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.require_configured()?;
        self.private_candidate(value)
    }
    fn validate_bft_v3_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        self.require_configured()?;
        // Actual batch execution still checks the original signed payload and
        // immutable parent policy before retention or signing.
        let envelope =
            oasis7::consensus_action_payload::decode_consensus_action_payload_envelope(payload)
                .map_err(|_| "Node action payload encoding rejected")?;
        match envelope.body {
            oasis7::consensus_action_payload::ConsensusActionPayloadBody::RuntimeAction {
                ..
            } => Ok(()),
            _ => Err("formal header Node requires a closed RuntimeAction payload".into()),
        }
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
        actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        let context = self.current_bft_v3_execution_context(schedule)?;
        let bytes = serde_cbor::to_vec(&actions).map_err(|_| "encode Node batch admission")?;
        let actual = self
            .execution
            .origin
            .build_candidate(&bytes)
            .map_err(|cause| {
                #[cfg(test)]
                eprintln!("fixture parent-policy failure: {cause}");
                let _ = cause;
                "Node immutable parent-policy admission rejected"
            })?;
        if actual.commitment.height != context.height
            || actual.commitment.transition.is_some()
            || actual.commitment.transition_ref.is_some()
        {
            return Err("Node batch changes unsupported membership".into());
        }
        Ok(())
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        binding: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.check_binding(binding)?;
        if max == 0 || max > 64 {
            return Err("Node history page capacity".into());
        }
        self.execution.ledger.reload()?;
        Ok(self
            .execution
            .ledger
            .records()
            .filter(|record| record.height() > after)
            .take(max)
            .map(|record| record.input().certificate)
            .collect())
    }
    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        self.require_configured()?;
        self.reset_parent()?;
        if token.schedule() != &self.execution.origin.schedule {
            return Err("Node commit differs from actual parent schedule".into());
        }
        let certificate = token.certified_header().clone();
        let value = BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let private = self.private_candidate(&value)?;
        let executed = self
            .execution
            .origin
            .replay_next(&certificate, &private.actions_cbor)
            .map_err(|_| "Node canonical private execution rejected")?;
        let nodes = self.execution.origin.retained_nodes.accessed_nodes()?;
        let height = certificate.execution_commitment.height;
        let previous = if height == self.initial_schedule.height() {
            self.execution.ledger.origin_digest()?
        } else {
            self.execution
                .ledger
                .record_at(height - 1)
                .ok_or("Node canonical predecessor unavailable")?
                .digest()?
        };
        let entry = PrivateHeaderEntryV3 {
            certificate: certificate.clone(),
            actions_cbor: private.actions_cbor,
        };
        let record = storage::Record::from_actual(
            self.execution.ledger.origin()?,
            &previous,
            &entry,
            &executed,
            nodes.keys().cloned().collect(),
        )?;
        self.execution
            .ledger
            .reserve_external_material(self.private.usage()?)?;
        let mut entries = self
            .execution
            .ledger
            .records()
            .filter(|record| record.height() < height)
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        entries.push(certificate.clone());
        let public_prefix = FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries,
        };
        self.execution
            .ledger
            .charge_proof(public_prefix.canonical_bytes()?.len())?;
        self.execution.ledger.accept_prepared(
            record,
            &executed.snapshot_bytes,
            &executed.journal_bytes,
            &nodes,
            &self.execution.store,
            true,
        )?;
        // Obtain the installed World only from a fresh strict durable replay.
        let cut = self.execution.verified_cut(height, token.block_id())?;
        let c = cut.proof().commitment();
        let install = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_install.v3",
                &self.material_identity,
                cut.durable_record_digest(),
                token.block_id(),
                &certificate.header.execution_commitment_hash,
                &c.execution_state_root,
                &c.state_root,
                &c.post_manifest_hash,
            ))
            .map_err(|_| "encode exact Node canonical install")?,
        );
        self.installed_world = cut.world().clone();
        self.cursor_height = height;
        let installed_execution_root = self
            .world()
            .execution_state_identity()
            .map_err(|_| "installed Node World identity unavailable")?
            .state_root_hash;
        if installed_execution_root != c.execution_state_root {
            return Err("installed Node World differs from durable execution receipt".into());
        }
        Ok(NodeBftExecutionCommitResultV3 {
            height,
            header_id: token.block_id().into(),
            execution_commitment_hash: certificate.header.execution_commitment_hash,
            execution_state_root: installed_execution_root,
            state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            driver_install_digest: install,
        })
    }
}
