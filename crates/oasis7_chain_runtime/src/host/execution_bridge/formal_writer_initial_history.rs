//! Qualified genesis recovery for the original pending height-one decision.
use super::*;

impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    pub(crate) fn retained_initial_read_cut(
        &self,
    ) -> Result<
        (
            World,
            VerifiedFormalReadAnchorV1,
            oasis7_node::NodeExecutionBootstrap,
        ),
        String,
    > {
        if self.anchor.head().record().kind == FormalRecordKindV1::InitialActivation {
            return Ok((
                self.world.clone(),
                self.anchor.clone(),
                self.committed_initial_bootstrap()?,
            ));
        }
        let continuation = self
            .pending_continuation()?
            .ok_or("retained genesis requires an original pending continuation")?;
        if continuation.predecessor_committed_height != 0
            || continuation.context.height != 1
            || self.anchor.authority().definition().profile_version != 2
            || self.anchor.authority().definition().epoch != 1
            || self.anchor.head().record().position != 2
        {
            return Err("retained genesis context is unavailable for this pending decision".into());
        }
        let reader = self
            .business_reader
            .as_ref()
            .ok_or("retained genesis CAS reader unavailable")?;
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position)),
        )?;
        if publication.schema_version != 1
            || &publication.candidate.candidate.record != self.anchor.head().record()
        {
            return Err("retained genesis delegation lacks exact current publication".into());
        }
        verify_record_material(self.anchor.head().record(), &publication.candidate.material)
            .map_err(|error| error.to_string())?;
        let tree = super::super::formal_business_package::tree(
            &publication.candidate.material,
            self.anchor.head().record(),
            &self.world,
            true,
        )?
        .ok_or("retained genesis current business descriptor missing")?;
        let inclusion =
            tree.historical_cut_inclusion(&self.anchor.head().record().world, 1, reader)?;
        let historical = VerifiedCommittedHistoricalCutV1::verify(
            &self.anchor,
            tree.index_root(),
            &inclusion,
            1,
        )
        .map_err(|error| error.to_string())?;
        let anchor =
            VerifiedFormalReadAnchorV1::from_committed_historical_cut(&self.anchor, &historical)
                .map_err(|error| error.to_string())?;
        if anchor.head().record().kind != FormalRecordKindV1::InitialActivation {
            return Err("retained genesis delegated cut is not InitialActivation".into());
        }
        let original = &self
            .pending
            .as_ref()
            .ok_or("retained genesis original input missing")?
            .original
            .body;
        if original.next_position != 2
            || original.parent != anchor.head().head()
            || original.pre_state_root != anchor.head().record().after_state_root
            || original.request_digest != self.anchor.head().record().request_digest
        {
            return Err("retained genesis is not the exact original height-one predecessor".into());
        }
        // Pre-upgrade endpoints durably embedded artifacts inside Qualified
        // entries without separately storing each logical artifact in raw CAS.
        // Exact local Published material is a byte cache, never an initial pin.
        let cached_initial = read_published_candidate_at(&self.root, 1)?
            .map(|(candidate, proof)| -> Result<ClosedRecord, String> {
                if candidate.candidate.record != *anchor.head().record()
                    || proof != *historical.proof()
                    || candidate.branch != proof.decision.branch
                    || candidate.candidate.writer_signature_hex
                        != proof.decision.writer_signature_hex
                    || candidate.frozen != proof.decision.frozen
                {
                    return Err("retained initial local cache differs from opaque C0".into());
                }
                verify_record_material(anchor.head().record(), &candidate.material)
                    .map_err(|error| error.to_string())?;
                Ok(candidate.material)
            })
            .transpose()?;
        let certificate = self.anchor.initial_certificate();
        let object = |role: ArtifactRole| -> Result<Vec<u8>, String> {
            let hash = anchor
                .head()
                .record()
                .artifact_roots
                .get(&role)
                .ok_or("retained initial role missing")?;
            if certificate.body.artifact_roots.get(&role) != Some(hash) {
                return Err(
                    "retained initial artifact differs from external activation certificate".into(),
                );
            }
            let bytes = match cached_initial.as_ref().and_then(|material| {
                material
                    .objects
                    .iter()
                    .find(|object| &object.content_hash == hash)
            }) {
                Some(object) => object.bytes.clone(),
                None => reader
                    .0
                    .read_node(hash)?
                    .ok_or("retained initial CAS artifact unavailable")?,
            };
            if bytes.len() > 8 * 1024 * 1024 || blake3_hex(&bytes) != *hash {
                return Err("retained initial CAS artifact hash/byte bound".into());
            }
            Ok(bytes)
        };
        let snapshot_bytes = object(ArtifactRole::Snapshot)?;
        if blake3_hex(&snapshot_bytes) != anchor.head().record().after_state_root
            || anchor.head().record().after_state_root != certificate.body.initial_state_root
        {
            return Err("retained initial Snapshot differs from qualified activation root".into());
        }
        let snapshot: Snapshot = capture::decode_snapshot(&snapshot_bytes)?;
        let journal_bytes = object(ArtifactRole::Journal)?;
        let journal: Journal = capture::decode_generic(&journal_bytes)?;
        let mut world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
            .map_err(|error| format!("{error:?}"))?
            .with_release_security_policy(self.release.clone());
        if world.state().time != 0
            || blake3_hex(&to_cbor(world.snapshot())?) != anchor.head().record().after_state_root
        {
            return Err("retained initial World is not the exact zero-height genesis".into());
        }
        let envelope = oasis7_distfs::controlled_authority::formal::business_material::FormalBusinessResultEnvelopeV1::decode(&object(ArtifactRole::Result)?)
            .map_err(|error| error.to_string())?;
        let initial_tree =
            oasis7::world_service::formal_result::FormalBusinessResultTreeV1::decode_cbor(
                &envelope.tree_descriptor_cbor,
            )?;
        initial_tree.validate_world_boundary(&world, anchor.head().record())?;
        let result: super::super::formal_initial::InitialResultV1 =
            capture::decode_generic(&envelope.execution_result_cbor)?;
        let bootstrap = result.node_bootstrap;
        if result.schema_version != 1
            || result.domain != "oasis7.formal.initial_node_result.v1"
            || result.world != anchor.head().record().world
            || result.state_root != anchor.head().record().after_state_root
            || result.journal_ref != blake3_hex(&journal_bytes)
            || result.logical_tick != 0
            || bootstrap.height != 0
            || bootstrap.execution_state_root != result.state_root
            || bootstrap.consensus_block_hash.trim().is_empty()
            || bootstrap.execution_block_hash.trim().is_empty()
        {
            return Err(
                "retained initial Node boundary differs from externally committed genesis".into(),
            );
        }
        let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(
            &anchor,
            &snapshot_bytes,
        )?;
        world.attach_verified_formal_history(parent, reader.0.clone())?;
        Ok((
            world,
            anchor,
            oasis7_node::NodeExecutionBootstrap {
                height: 0,
                consensus_block_hash: bootstrap.consensus_block_hash,
                execution_block_hash: bootstrap.execution_block_hash,
                execution_state_root: bootstrap.execution_state_root,
            },
        ))
    }
}
