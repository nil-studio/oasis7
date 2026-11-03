//! Read-only cut construction for protected retention sealing, before archive export.
use super::super::super::super::super::super::storage as common_storage;
use super::*;
impl FormalBftHeaderLedgerExecutionV3 {
    pub(in super::super) fn verified_private_retention_cut(
        &mut self,
        source: &BftSuccessorWindowSourceV1,
        height: u64,
        header_id: &str,
        deadline: Instant,
    ) -> Result<BftSuccessorWindowCutV1, String> {
        live(deadline)?;
        source.validate().map_err(err)?;
        let cut = self.verified_cut(height, header_id)?;
        live(deadline)?;
        let record = self
            .ledger
            .records_through(height)?
            .last()
            .ok_or("retention empty cut")?;
        let read_ref =
            |digest: &str, limit: usize| -> Result<BftSuccessorWindowArchiveRefV1, String> {
                let bytes = common_storage::read_cas_bounded(&self.store, digest, limit)?;
                if blake3_hex(&bytes) != digest {
                    return Err("retention cut original object digest differs".into());
                }
                Ok(BftSuccessorWindowArchiveRefV1 {
                    digest: digest.into(),
                    bytes: bytes.len() as u64,
                })
            };
        let snapshot_ref = read_ref(
            record.snapshot_ref(),
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )?;
        let journal_ref = read_ref(record.journal_ref(), common_storage::MAX_BFT_JOURNAL_BYTES)?;
        let certificate = cut.proof().certificate().certified_header();
        let c = &certificate.execution_commitment;
        let result = BftSuccessorWindowCutV1 {
            height,
            header_id: header_id.into(),
            certificate_digest: blake3_hex(&certificate.canonical_bytes()?),
            execution_commitment_digest: c.digest()?,
            snapshot_ref,
            journal_ref,
            semantic_state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            business_result_root: c.result_root.clone(),
            driver_record_digest: cut.durable_record_digest().into(),
            driver_install_digest: install_digest(
                &source.material_identity,
                cut.durable_record_digest(),
                certificate,
            )?,
        };
        if result.snapshot_ref.digest != c.execution_state_root {
            return Err("retention cut Snapshot differs from committed original".into());
        }
        result.validate().map_err(err)?;
        live(deadline)?;
        Ok(result)
    }
}
