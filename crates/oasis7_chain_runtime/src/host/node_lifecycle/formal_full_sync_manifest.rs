use super::*;

impl FormalFullSyncSession {
    pub(super) fn fetch_manifest(
        &self,
        cut: &FormalFullSyncCutV1,
    ) -> Result<FormalFullSyncManifestV1, String> {
        let request = FormalFullSyncManifestRequestV1 {
            schema_version: 1,
            grant: self
                .config
                .grant
                .clone()
                .ok_or("full-sync issuer grant missing")?,
            cut: cut.clone(),
        };
        request.validate()?;
        let manifest: FormalFullSyncManifestV1 =
            self.post_signed(FORMAL_FULL_SYNC_MANIFEST_PATH, request, 8 * 1024 * 1024)?;
        manifest.validate()?;
        if manifest.cut != *cut {
            return Err("full-sync manifest differs from independently verified cut".into());
        }
        Ok(manifest)
    }

    pub(super) fn persist_manifest_original(
        &self,
        manifest: &FormalFullSyncManifestV1,
        cut: &FormalFullSyncCutV1,
        budget: &mut CasBudget,
    ) -> Result<oasis7_game_client::formal_sync_exchange::FullSyncManifestReference, String> {
        // Persist the canonical verified manifest under the same operation
        // material budget and candidate roots. This is not the signed envelope.
        let manifest_bytes = serde_json::to_vec(manifest).map_err(|e| e.to_string())?;
        if manifest_bytes.is_empty() || manifest_bytes.len() > MAX_FULL_SYNC_RECORD_BYTES {
            return Err("canonical full-sync manifest exceeds material bound".into());
        }
        let manifest_reference =
            oasis7_game_client::formal_sync_exchange::FullSyncManifestReference {
                content_hash: blake3::hash(&manifest_bytes).to_hex().to_string(),
                size_bytes: manifest_bytes.len() as u64,
            };
        budget.add(
            &manifest_reference.content_hash,
            manifest_reference.size_bytes,
        )?;
        self.persist_cas_object(&manifest_reference.content_hash, &manifest_bytes)?;
        let recovered_manifest = local_closure::read_manifest(
            &self.cas,
            &manifest_reference,
            cut,
            std::time::Instant::now()
                .checked_add(Duration::from_millis(self.config.request_timeout_ms))
                .ok_or("local manifest deadline overflow")?,
        )?;
        if serde_json::to_vec(&recovered_manifest).map_err(|e| e.to_string())? != manifest_bytes {
            return Err("durable manifest differs from verified candidate".into());
        }
        Ok(manifest_reference)
    }
}
