//! Durable import/recovery of a complete formal BFT prefix. No votes are
//! produced here, and an exact historical cut is not a freshness assertion.
use super::*;
use oasis7_formal_protocol::bft::{FormalBftCertifiedPrefixV1, VerifiedFormalBftPrefixV1};

#[derive(Clone)]
struct OriginInputs {
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV1,
    material: ClosedRecord,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}

pub(crate) struct FormalBftPrefixExecutionV1 {
    pub(super) origin: formal_origin::QualifiedFormalBftOriginV1,
    pub(super) ledger: formal_prefix_storage::Ledger,
    store: LocalCasStore,
    inputs: OriginInputs,
}

/// Constructed only after locked durable reread and complete H-to-tip replay.
/// It never conveys permission to sign or claim a current/latest head.
pub(crate) struct FormalBftPrefixCutV1 {
    certified_prefix: FormalBftCertifiedPrefixV1,
    proof: VerifiedFormalBftPrefixV1,
    world: RuntimeWorld,
    result_tree: FormalBusinessResultTreeV1,
    reader: formal_business_package::SharedBusinessReader,
    record_digest: String,
}
struct ReplayedStoredPrefix {
    proof: VerifiedFormalBftPrefixV1,
    tip: ExecutedFormalCandidateV1,
    record_digest: String,
}

impl FormalBftPrefixCutV1 {
    pub(crate) fn certified_prefix(&self) -> &FormalBftCertifiedPrefixV1 {
        &self.certified_prefix
    }
    pub(crate) fn proof(&self) -> &VerifiedFormalBftPrefixV1 {
        &self.proof
    }
    pub(crate) fn world(&self) -> &RuntimeWorld {
        &self.world
    }
    pub(crate) fn result_tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.result_tree
    }
    pub(crate) fn index_reader(&self) -> &dyn BusinessIndexNodeReader {
        &self.reader
    }
    pub(crate) fn durable_record_digest(&self) -> &str {
        &self.record_digest
    }
}

impl FormalBftPrefixExecutionV1 {
    pub(crate) fn open_from_handoff(
        records_dir: &Path,
        store: LocalCasStore,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV1,
        material: ClosedRecord,
        source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        initialize: bool,
    ) -> Result<Self, String> {
        // Authenticate the handoff and closure before creating a local format.
        let handoff = verify_formal_bft_handoff_v1(&anchor, target.clone())?;
        validate_handoff_material(&anchor, &handoff, &material)?;
        let material_bytes = serde_cbor::to_vec(&material)
            .map_err(|e| e.to_string())?
            .len();
        let snapshot_bytes = material.snapshot_bytes().map_err(|e| e.to_string())?.len();
        let journal_bytes = material
            .roots
            .get(&ArtifactRole::Journal)
            .and_then(|reference| {
                material
                    .objects
                    .iter()
                    .find(|object| &object.content_hash == reference)
            })
            .ok_or("qualified H Journal is absent")?
            .bytes
            .len();
        let h_bytes = material_bytes
            .checked_add(snapshot_bytes)
            .and_then(|n| n.checked_add(journal_bytes))
            .ok_or("formal prefix H material capacity overflow")?;
        let mut ledger = formal_prefix_storage::Ledger::open(records_dir, initialize, h_bytes)?;
        let seed = ledger.preload(&store, None)?;
        let strict = !initialize && !ledger.empty();
        let origin = formal_origin::restore(
            anchor.clone(),
            target.clone(),
            material.clone(),
            source.clone(),
            seed,
            strict,
        )?;
        ledger.bind(&origin.pin.digest()?, origin.schedule.height())?;
        Ok(Self {
            origin,
            ledger,
            store,
            inputs: OriginInputs {
                anchor,
                target,
                material,
                source,
            },
        })
    }

    /// Import only this independently pinned complete prefix. Existing records
    /// and an interrupted intent must retain their exact original certificates.
    pub(crate) fn install_prefix(
        mut self,
        prefix: &FormalBftCertifiedPrefixV1,
        exact_height: u64,
        exact_block_id: &str,
    ) -> Result<FormalBftPrefixCutV1, String> {
        let proof = prefix.verify(&self.origin.handoff)?;
        check_target(&proof, exact_height, exact_block_id)?;
        self.ledger.check_input(prefix)?;
        self.replay(prefix, exact_height, exact_block_id, true)?;
        // Discard the candidate World and captured reader. Re-read all durable
        // records and CAS receipts, then reproduce H and every height using
        // only the persisted dependency set before returning a cut.
        self.verified_cut(exact_height, exact_block_id)
    }

    pub(crate) fn verified_cut(
        &mut self,
        exact_height: u64,
        exact_block_id: &str,
    ) -> Result<FormalBftPrefixCutV1, String> {
        self.ledger.reload()?;
        let seed = self.ledger.preload(&self.store, Some(exact_height))?;
        self.origin = formal_origin::restore(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            self.inputs.material.clone(),
            self.inputs.source.clone(),
            seed,
            true,
        )?;
        if self.origin.pin.digest()? != self.ledger.material_pin_digest()? {
            return Err("formal prefix reread H material identity changed".into());
        }
        let prefix = self.ledger.prefix_through(exact_height)?;
        let replayed = self.replay(&prefix, exact_height, exact_block_id, false)?;
        Ok(FormalBftPrefixCutV1 {
            certified_prefix: prefix,
            proof: replayed.proof,
            world: replayed.tip.world,
            result_tree: replayed.tip.result_tree,
            reader: self.origin.business_reader.clone(),
            record_digest: replayed.record_digest,
        })
    }

    fn replay(
        &mut self,
        prefix: &FormalBftCertifiedPrefixV1,
        exact_height: u64,
        exact_block_id: &str,
        append: bool,
    ) -> Result<ReplayedStoredPrefix, String> {
        let proof = prefix.verify(&self.origin.handoff)?;
        check_target(&proof, exact_height, exact_block_id)?;
        self.ledger.charge_proof(proof.encoded_bytes())?;
        let mut partial = FormalBftCertifiedPrefixV1 {
            schema_version: 1,
            entries: Vec::new(),
        };
        let mut previous: Option<ExecutedFormalCandidateV1> = None;
        let mut parent_proof: Option<VerifiedFormalBftPrefixV1> = None;
        let mut record_digest = self.ledger.origin_digest()?;
        for entry in &prefix.entries {
            let (parent, tree, schedule) = match &previous {
                Some(previous) => (
                    &previous.world,
                    &previous.result_tree,
                    parent_proof
                        .as_ref()
                        .ok_or("formal prefix predecessor proof missing")?
                        .next_height_schedule(),
                ),
                None => (
                    &self.origin.parent_world,
                    &self.origin.parent_result_tree,
                    &self.origin.schedule,
                ),
            };
            let executed = execute_formal_candidate(
                FormalCandidateContext {
                    handoff: &self.origin.handoff,
                    schedule,
                    parent_world: parent,
                    parent_result_tree: tree,
                    identity: &self.origin.parent_record.world,
                    business_reader: &self.origin.business_reader,
                    prefix: parent_proof.as_ref(),
                },
                &entry.certificate.proposal.block,
            )?;
            if executed.block != entry.certificate.proposal.block
                || executed.commitment != entry.commitment
            {
                return Err(format!(
                    "formal prefix height {} does not reproduce from qualified H",
                    executed.block.height
                ));
            }
            for node in &executed.result_delta.node_writes {
                self.origin
                    .business_reader
                    .cache_content_node(&node.digest, &node.bytes)?;
            }
            let nodes = self.origin.retained_nodes.accessed_nodes()?;
            record_digest = self.ledger.accept(
                entry,
                &executed,
                &nodes,
                &record_digest,
                &self.store,
                append,
            )?;
            partial.entries.push(entry.clone());
            parent_proof = Some(partial.verify(&self.origin.handoff)?);
            previous = Some(executed);
        }
        let tip = previous.ok_or("formal prefix is empty")?;
        Ok(ReplayedStoredPrefix {
            proof,
            tip,
            record_digest,
        })
    }
}

fn check_target(
    proof: &VerifiedFormalBftPrefixV1,
    height: u64,
    block_id: &str,
) -> Result<(), String> {
    if proof.certificate().block().height != height || proof.certificate().block_id() != block_id {
        return Err("formal prefix differs from independently configured exact target".into());
    }
    Ok(())
}
