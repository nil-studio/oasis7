//! Full H-to-tip replay. Protocol ancestry alone never creates this value.
//! This stage performs no durable publication and grants no serving/signing authority.
use super::*;
use oasis7_formal_protocol::bft::{FormalBftCertifiedPrefixV1, VerifiedFormalBftPrefixV1};

pub(crate) struct ReplayedFormalBftPrefixV1 {
    pub(super) proof: VerifiedFormalBftPrefixV1,
    pub(super) tip: ExecutedFormalCandidateV1,
    pub(super) dependencies: Arc<formal_dependencies::RecordingBusinessIndexReaderV1>,
}

impl FormalBftReadExecutionV1 {
    /// Reexecute every supplied height from this independently restored H.
    /// The exact target is an external constraint, never selected by the proof.
    /// Neither the existing H+1 ledger nor the retained H World is modified.
    pub(crate) fn replay_certified_prefix(
        &self,
        prefix: &FormalBftCertifiedPrefixV1,
        exact_height: u64,
        exact_block_id: &str,
    ) -> Result<ReplayedFormalBftPrefixV1, String> {
        // Verify the entire bounded protocol chain before any World execution.
        let complete = prefix.verify(&self.handoff)?;
        if complete.certificate().block().height != exact_height
            || complete.certificate().block_id() != exact_block_id
        {
            return Err("formal BFT replay differs from externally fixed exact target".into());
        }
        // New result-tree nodes belong to this isolated replay, not the old
        // H+1 ledger's exact dependency set (which is strict after reopen).
        let nodes = formal_dependencies::RecordingBusinessIndexReaderV1::new(
            self.business_source.clone(),
            self.retained_nodes.recorded_nodes()?,
        )?;
        let business_reader = formal_business_package::SharedBusinessReader(nodes.clone());
        let parent_world = RuntimeWorld::from_verified_formal_snapshot(
            self.parent_world.snapshot(),
            self.parent_world.journal().clone(),
            self.retained_parent.clone(),
            nodes.clone(),
        )
        .map_err(|error| format!("restore independent formal replay H: {error:?}"))?
        .with_release_security_policy(self.parent_world.release_security_policy().clone());
        if parent_world.snapshot() != self.parent_world.snapshot()
            || super::super::super::super::super::journal_to_cbor(parent_world.journal())?
                != super::super::super::super::super::journal_to_cbor(self.parent_world.journal())?
        {
            return Err("independent formal replay restore changed H Snapshot or Journal".into());
        }
        let mut partial = FormalBftCertifiedPrefixV1 {
            schema_version: 1,
            entries: Vec::with_capacity(prefix.entries.len()),
        };
        let mut parent_proof = None;
        let mut previous: Option<ExecutedFormalCandidateV1> = None;
        for entry in &prefix.entries {
            let block = &entry.certificate.proposal.block;
            let (parent_world, parent_result_tree, schedule) = match previous.as_ref() {
                Some(previous) => (
                    &previous.world,
                    &previous.result_tree,
                    parent_proof
                        .as_ref()
                        .map(|proof: &VerifiedFormalBftPrefixV1| proof.next_height_schedule())
                        .ok_or("formal BFT replay lacks predecessor proof")?,
                ),
                None => (&parent_world, &self.parent_result_tree, &self.schedule),
            };
            let executed = execute_formal_candidate(
                FormalCandidateContext {
                    handoff: &self.handoff,
                    schedule,
                    parent_world,
                    parent_result_tree,
                    identity: &self.parent_record.world,
                    business_reader: &business_reader,
                    prefix: parent_proof.as_ref(),
                },
                block,
            )?;
            if executed.block != *block || executed.commitment != entry.commitment {
                return Err(format!(
                    "formal BFT prefix height {} does not reproduce from qualified H",
                    block.height,
                ));
            }
            for node in &executed.result_delta.node_writes {
                business_reader.cache_content_node(&node.digest, &node.bytes)?;
            }
            partial.entries.push(entry.clone());
            parent_proof = Some(partial.verify(&self.handoff)?);
            previous = Some(executed);
        }
        Ok(ReplayedFormalBftPrefixV1 {
            proof: complete,
            dependencies: nodes,
            tip: previous.ok_or("formal BFT replay prefix is empty")?,
        })
    }
}
