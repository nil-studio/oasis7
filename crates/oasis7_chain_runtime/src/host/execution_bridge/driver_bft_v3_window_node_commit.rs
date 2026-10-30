//! Sequential canonical publication with exact historical replay at a Node cursor.
use super::*;
impl ActiveNodeDriver {
    pub(super) fn commit_at_cursor(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        let deadline = Self::deadline()?;
        self.reset_cursor_at(deadline)?;
        if token.schedule() != self.window.schedule() {
            return Err("active Node commit parent differs".into());
        }

        let certificate = token.certified_header().clone();
        let height = certificate.execution_commitment.height;
        let value = BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let private = self
            .window
            .private
            .candidate_for_value(&value)?
            .ok_or("active Node exact original input unavailable")?;
        if self.window.ledger.record_at(height).is_some() {
            let seed = self
                .window
                .ledger
                .preload(&self.window.store, Some(height))?;
            let observed = self.window.execution.retained_nodes.accessed_nodes()?;
            self.window.execution = self.window.execution.fork_replay_current(seed)?;
            for (digest, bytes) in observed {
                if self
                    .window
                    .execution
                    .retained_nodes
                    .read_node(&digest)?
                    .as_ref()
                    != Some(&bytes)
                {
                    return Err("active Node prefix dependency changed".into());
                }
            }
        } else {
            self.window.execution = self.window.execution.fork_live_closed()?;
        }
        let entry = PrivateHeaderEntryV3 {
            certificate: certificate.clone(),
            actions_cbor: private.actions_cbor,
        };
        let executed = self
            .window
            .execution
            .replay_next(&entry.certificate, &entry.actions_cbor)?;
        let nodes = self.window.execution.retained_nodes.accessed_nodes()?;
        let refs = self.window.ledger.retained_business_refs(height, &nodes)?;
        let previous = if height == self.window.base.execution.schedule.height() {
            self.window.ledger.origin_digest()?
        } else {
            self.window
                .ledger
                .record_at(height - 1)
                .ok_or("active Node predecessor absent")?
                .digest()?
        };
        let record = WindowRecord::from_actual(
            self.window.ledger.origin()?,
            &previous,
            &entry,
            &executed,
            refs,
        )?;
        let mut entries = self
            .window
            .ledger
            .records()
            .filter(|record| record.height() < height)
            .map(|record| record.input().certificate)
            .collect::<Vec<_>>();
        entries.push(certificate.clone());
        let prefix = FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries,
        };
        self.window
            .ledger
            .charge_proof(prefix.canonical_bytes()?.len())?;
        self.window.ledger.reserve_private_material(
            self.window.private.usage()?,
            self.window.private.publication_promises()?,
            &self.window.store,
        )?;
        live(deadline)?;
        self.window.ledger.accept_prepared(
            record,
            &executed.snapshot_bytes,
            &executed.journal_bytes,
            &nodes,
            &self.window.store,
            true,
        )?;
        self.window.reset_through(Some(height), deadline)?;
        if self
            .window
            .execution
            .tip
            .as_ref()
            .is_none_or(|tip| tip.certified_header() != &certificate)
        {
            return Err("active Node durable exact QC reread differs".into());
        }
        self.cursor = height;
        let c = &certificate.execution_commitment;
        if self
            .window
            .world()
            .execution_state_identity()
            .map_err(err)?
            .state_root_hash
            != c.state_root
        {
            return Err("active Node durable World differs from result".into());
        }
        Ok(NodeBftExecutionCommitResultV3 {
            height,
            header_id: token.block_id().into(),
            execution_commitment_hash: certificate.header.execution_commitment_hash.clone(),
            execution_state_root: c.execution_state_root.clone(),
            state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            driver_install_digest: self.window.current_install_digest()?,
        })
    }
}
