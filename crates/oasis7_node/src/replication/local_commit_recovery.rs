//! Recover the same signed local commit across CAS/guard/message publication.
use super::*;

fn invalid(reason: &str) -> NodeError {
    NodeError::Replication {
        reason: format!("local commit recovery: {reason}"),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalOriginal {
    complete: bool,
    message: GossipReplicationMessage,
}
impl ReplicationRuntime {
    pub(super) fn local_commit_original_path(&self, node_id: &str) -> PathBuf {
        self.config
            .root_dir
            .join(format!("replication_local_commit_{node_id}.json"))
    }

    pub(super) fn recover_local_commit(&mut self, node_id: &str) -> Result<(), NodeError> {
        let path = self.local_commit_original_path(node_id);
        if path.exists() {
            let bytes = fs::read(&path).map_err(|e| invalid(&format!("read original: {e}")))?;
            let original: LocalOriginal = serde_json::from_slice(&bytes)
                .map_err(|e| invalid(&format!("decode original: {e}")))?;
            if !original.complete {
                self.restore_local_commit_original(node_id, &original.message)?;
            }
        }
        self.recover_legacy_local_commit(node_id)
    }

    pub(super) fn stage_local_commit_original(
        &self,
        node_id: &str,
        message: &GossipReplicationMessage,
    ) -> Result<(), NodeError> {
        self.validate_local_original(node_id, message)?;
        write_json_compact(
            self.local_commit_original_path(node_id).as_path(),
            &LocalOriginal {
                complete: false,
                message: message.clone(),
            },
        )
    }

    fn validate_local_original(
        &self,
        node_id: &str,
        m: &GossipReplicationMessage,
    ) -> Result<ReplicatedCommitPayload, NodeError> {
        let p: ReplicatedCommitPayload = serde_json::from_slice(&m.payload)
            .map_err(|e| invalid(&format!("decode payload: {e}")))?;
        if serde_json::to_vec(&p).map_err(|e| invalid(&e.to_string()))? != m.payload {
            return Err(invalid("payload is not an exact supported local encoding"));
        }
        crate::validate_replicated_execution_input_actions(&p.actions, p.height)
            .map_err(|e| invalid(&e))?;
        if let Some(c) = p.execution_checkpoint.as_ref()
            && (c.height == 0
                || c.height > p.height
                || c.manifest_ref.is_empty()
                || c.manifest_size_bytes == 0
                || (c.height == p.height
                    && (p.execution_block_hash.as_deref()
                        != Some(c.execution_block_hash.as_str())
                        || p.execution_state_root.as_deref()
                            != Some(c.execution_state_root.as_str()))))
        {
            return Err(invalid("checkpoint descriptor binding mismatch"));
        }
        let writer = self
            .signer
            .as_ref()
            .map_or(node_id, |s| s.public_key_hex.as_str());
        if m.version != REPLICATION_VERSION
            || m.node_id != node_id
            || p.node_id != node_id
            || p.world_id.is_empty()
            || m.world_id != p.world_id
            || m.record.world_id != p.world_id
            || p.proposer_id.as_deref().is_none_or(str::is_empty)
            || p.height == 0
            || m.record.writer_id != writer
            || m.record.writer_epoch == 0
            || m.record.sequence == 0
            || m.record.path != format!("{COMMIT_FILE_PREFIX}/{:020}.json", p.height)
            || m.record.content_hash != blake3_hex(&m.payload)
            || m.record.size_bytes != m.payload.len() as u64
            || m.record.updated_at_ms != p.committed_at_ms
            || p.action_root != crate::compute_consensus_action_root(&p.actions)?
            || p.execution_block_hash.is_some() != p.execution_state_root.is_some()
        {
            return Err(invalid("original identity, payload or record mismatch"));
        }
        match self.signer.as_ref() {
            Some(s) if m.public_key_hex.as_deref() == Some(s.public_key_hex.as_str()) => {
                verify_replication_message_signature(m)?;
            }
            None if !self.enforce_signature
                && m.public_key_hex.is_none()
                && m.signature_hex.is_none() => {}
            _ => return Err(invalid("original signer mismatch")),
        }
        Ok(p)
    }

    pub(super) fn restore_local_commit_original(
        &mut self,
        node_id: &str,
        m: &GossipReplicationMessage,
    ) -> Result<(), NodeError> {
        let p = self.validate_local_original(node_id, m)?;
        if let Some(existing) =
            load_commit_message_record_from_root(&self.config.root_dir, p.height)?
            && existing != *m
        {
            return Err(invalid("signed height already has a different original"));
        }
        if self.writer_state.last_replicated_height > p.height
            || (self.writer_state.last_sequence != 0
                && self.writer_state.writer_epoch > m.record.writer_epoch)
            || (self.writer_state.writer_epoch == m.record.writer_epoch
                && self.writer_state.last_sequence > m.record.sequence)
        {
            return Err(invalid("original is behind durable writer state"));
        }
        // Validate an existing index before changing CAS or writer state.
        let index = commit_retention::load_latest_commit_head_index_from_root(
            &self.config.root_dir,
            &p.world_id,
        )?;
        if let Some(index) = index.as_ref() {
            if index.height > p.height {
                return Err(invalid("latest index is ahead of pending original"));
            }
            if index.height == p.height
                && *index != latest_head_index::latest_commit_head_index_for_message(p.height, m)?
            {
                return Err(invalid("latest index conflicts with original"));
            }
        }
        let applied = self.guard.writer_id.as_deref() == Some(m.record.writer_id.as_str())
            && self.guard.writer_epoch == m.record.writer_epoch
            && self.guard.last_sequence == m.record.sequence;
        let metadata = self
            .store
            .stat_file(&m.record.path)
            .map_err(distfs_error_to_node_error)?;
        if let Some(metadata) = metadata.as_ref()
            && (metadata.size_bytes != m.record.size_bytes
                || metadata.content_hash != m.record.content_hash
                || self
                    .store
                    .read_file(&m.record.path)
                    .map_err(distfs_error_to_node_error)?
                    != m.payload)
        {
            return Err(invalid("original/CAS conflict"));
        }
        if applied {
            if metadata.is_none() {
                return Err(invalid("advanced guard has no original CAS record"));
            }
        } else {
            // The first signed original also preserves the seeded epoch if no
            // guard/writer file reached disk before the interruption.
            if self.guard.writer_id.is_none()
                && self.guard.last_sequence == 0
                && self.writer_state.last_sequence == 0
                && self.writer_state.last_replicated_height == 0
                && m.record.sequence == 1
                && !self.config.guard_state_path().exists()
                && !self.config.writer_state_path(node_id).exists()
            {
                self.writer_state.writer_epoch = m.record.writer_epoch;
            }
            let (epoch, sequence) = self.next_local_record_position(&m.record.writer_id)?;
            if (epoch, sequence) != (m.record.writer_epoch, m.record.sequence) {
                return Err(invalid("original is not the exact next writer record"));
            }
            apply_replication_record(&self.store, &mut self.guard, &m.record, &m.payload)
                .map_err(distfs_error_to_node_error)?;
        }
        // A dangling exact index is repaired only after the original CAS is verified.
        if index.as_ref().is_some_and(|index| index.height == p.height) {
            write_json_compact(self.config.commit_message_path(p.height).as_path(), m)?;
        }
        self.persist_commit_message(p.height, m)?;
        self.writer_state.writer_epoch = m.record.writer_epoch;
        self.writer_state.last_sequence = m.record.sequence;
        self.writer_state.last_replicated_height = p.height;
        self.persist_state(node_id)?;
        write_json_compact(
            self.local_commit_original_path(node_id).as_path(),
            &LocalOriginal {
                complete: true,
                message: m.clone(),
            },
        )
    }

    fn recover_legacy_local_commit(&mut self, node_id: &str) -> Result<(), NodeError> {
        let height = self.writer_state.last_replicated_height;
        if height == 0 {
            return Ok(());
        }
        // Preserve ordinary startup cost and existing signed-message authority.
        if load_commit_message_record_from_root(&self.config.root_dir, height)?.is_some() {
            return Ok(());
        }
        let signer = self
            .signer
            .as_ref()
            .ok_or_else(|| invalid("legacy gap requires original signer"))?;
        if self.guard.writer_id.as_deref() != Some(signer.public_key_hex.as_str())
            || self.guard.writer_epoch != self.writer_state.writer_epoch
            || self.guard.last_sequence != self.writer_state.last_sequence
        {
            return Err(invalid("legacy guard/writer state mismatch"));
        }
        let path = format!("{COMMIT_FILE_PREFIX}/{height:020}.json");
        let metadata = self
            .store
            .stat_file(&path)
            .map_err(distfs_error_to_node_error)?
            .ok_or_else(|| invalid("legacy gap has no original CAS payload"))?;
        let bytes = self
            .store
            .read_file(&path)
            .map_err(distfs_error_to_node_error)?;
        if metadata.content_hash != blake3_hex(&bytes) || metadata.size_bytes != bytes.len() as u64
        {
            return Err(invalid("legacy CAS hash/size mismatch"));
        }
        let p: ReplicatedCommitPayload =
            serde_json::from_slice(&bytes).map_err(|e| invalid(&format!("legacy payload: {e}")))?;
        if p.height != height || p.lineage_envelope.is_some() {
            return Err(invalid("ambiguous legacy amendment"));
        }
        let mut signed_heights: BTreeSet<u64> =
            commit_retention::hot_commit_message_heights_from_root(&self.config.root_dir)?
                .into_iter()
                .collect();
        signed_heights.extend(
            load_commit_message_cold_index_from_root(&self.config.root_dir)?
                .by_height
                .keys()
                .copied(),
        );
        for h in signed_heights {
            if h >= height
                || self
                    .store
                    .stat_file(&format!("{COMMIT_FILE_PREFIX}/{h:020}.json"))
                    .map_err(distfs_error_to_node_error)?
                    .is_none()
            {
                return Err(invalid(
                    "signed history is ahead of or absent from legacy CAS",
                ));
            }
        }
        let mut local_sequences = BTreeSet::new();
        let mut last_sequence = 0;
        for meta in self
            .store
            .list_files()
            .map_err(distfs_error_to_node_error)?
        {
            if !meta.path.starts_with(&format!("{COMMIT_FILE_PREFIX}/")) || meta.path == path {
                continue;
            }
            let h = Path::new(&meta.path)
                .file_stem()
                .and_then(|v| v.to_str())
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| invalid("invalid legacy commit path"))?;
            if h >= height {
                return Err(invalid("legacy CAS is ahead of writer state"));
            }
            let m = load_commit_message_from_root(&self.config.root_dir, &p.world_id, h)?
                .ok_or_else(|| invalid("another legacy signed message is missing"))?;
            verify_replication_message_signature(&m)?;
            let prior: ReplicatedCommitPayload =
                serde_json::from_slice(&m.payload).map_err(|e| invalid(&e.to_string()))?;
            if self
                .store
                .read_file(&meta.path)
                .map_err(distfs_error_to_node_error)?
                != m.payload
                || prior.height != h
                || prior.world_id != p.world_id
                || prior.node_id != m.node_id
                || prior.action_root != crate::compute_consensus_action_root(&prior.actions)?
                || m.public_key_hex.as_deref() != Some(m.record.writer_id.as_str())
                || meta.size_bytes != m.payload.len() as u64
                || m.record.path != meta.path
                || m.record.content_hash != meta.content_hash
                || m.record.content_hash != blake3_hex(&m.payload)
                || m.record.size_bytes != m.payload.len() as u64
                || m.record.world_id != p.world_id
                || m.world_id != p.world_id
            {
                return Err(invalid(
                    "legacy CAS has an unfinished or conflicting amendment",
                ));
            }
            if m.public_key_hex.as_deref() == Some(signer.public_key_hex.as_str()) {
                if m.node_id != node_id || m.record.writer_epoch != self.writer_state.writer_epoch {
                    return Err(invalid("legacy writer epoch is ambiguous"));
                }
                if !local_sequences.insert(m.record.sequence) {
                    return Err(invalid("duplicate local sequence"));
                }
                last_sequence = last_sequence.max(m.record.sequence);
            }
        }
        if last_sequence == 0 {
            return Err(invalid("legacy gap has no signed world/writer anchor"));
        }
        if last_sequence.checked_add(1) != Some(self.writer_state.last_sequence) {
            return Err(invalid("legacy sequence is not a unique tail gap"));
        }
        let record = build_replication_record_with_epoch(
            &p.world_id,
            &signer.public_key_hex,
            self.writer_state.writer_epoch,
            self.writer_state.last_sequence,
            &path,
            &bytes,
            p.committed_at_ms,
        )
        .map_err(distfs_error_to_node_error)?;
        let mut message = GossipReplicationMessage {
            version: REPLICATION_VERSION,
            world_id: p.world_id,
            node_id: node_id.into(),
            record,
            payload: bytes,
            public_key_hex: Some(signer.public_key_hex.clone()),
            signature_hex: None,
        };
        message.signature_hex = Some(sign_replication_message(&message, signer)?);
        self.validate_local_original(node_id, &message)?;
        self.stage_local_commit_original(node_id, &message)?;
        self.restore_local_commit_original(node_id, &message)
    }
}
