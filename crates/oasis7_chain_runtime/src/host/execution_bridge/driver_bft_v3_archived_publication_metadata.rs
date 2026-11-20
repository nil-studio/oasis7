//! Validation of the original typed publication promise against actual replay.
use super::*;

const DOMAIN: &str = "oasis7.formal.bft.node_publication_reservation.v1";
pub(super) const MAX_PUBLICATION_METADATA_BYTES: usize = 16 * 1024;
const MAX_PUBLICATION_BYTES: u64 = 512 * 1024 * 1024;

// Exact original typed CBOR field order from the protected candidate producer.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicationReservationV1 {
    schema_version: u32,
    domain: String,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    header_id: String,
    actions_digest: String,
    snapshot_digest: String,
    snapshot_bytes: u64,
    journal_digest: String,
    journal_bytes: u64,
    derived_nodes_digest: String,
    publication_bytes: u64,
}

impl PublicationReservationV1 {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_PUBLICATION_METADATA_BYTES {
            return Err("original publication metadata capacity".into());
        }
        let reservation: Self =
            serde_cbor::from_slice(bytes).map_err(|_| "decode original publication metadata")?;
        if reservation.schema_version != 1
            || reservation.domain != DOMAIN
            || reservation.publication_bytes == 0
            || reservation.publication_bytes > MAX_PUBLICATION_BYTES
            || reservation
                .snapshot_bytes
                .checked_add(reservation.journal_bytes)
                .is_none_or(|size| size > reservation.publication_bytes)
            || serde_cbor::to_vec(&reservation).map_err(error)? != bytes
        {
            return Err(
                "original publication metadata schema, capacity or encoding rejected".into(),
            );
        }
        Ok(reservation)
    }

    pub(super) fn verify_execution(
        &self,
        ledger_origin: &str,
        material_identity: &str,
        instance_id: &str,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
    ) -> Result<(), String> {
        if self.ledger_origin_digest != ledger_origin
            || self.material_identity != material_identity
            || self.instance_id != instance_id
            || self.height != executed.candidate.commitment.height
            || self.header_id != executed.candidate.header.block_id()?
            || self.actions_digest != blake3_hex(actions)
            || self.snapshot_digest != blake3_hex(&executed.snapshot_bytes)
            || self.snapshot_bytes != executed.snapshot_bytes.len() as u64
            || self.journal_digest != blake3_hex(&executed.journal_bytes)
            || self.journal_bytes != executed.journal_bytes.len() as u64
            || self.derived_nodes_digest != derived_nodes_digest(executed)?
        {
            return Err("original publication metadata differs from actual execution".into());
        }
        // The pinned original promise and reservation price need the original
        // operation's authenticated inventory to be independently re-derived.
        Ok(())
    }

    pub(super) fn verify_publication_bytes(&self, observed: usize) -> Result<(), String> {
        if self.publication_bytes != observed as u64 {
            return Err("original publication bound differs from actual preparation".into());
        }
        Ok(())
    }

    pub(super) fn from_actual(
        ledger_origin: &str,
        material_identity: &str,
        instance_id: &str,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
        publication_bytes: usize,
    ) -> Result<Self, String> {
        Ok(Self {
            schema_version: 1,
            domain: DOMAIN.into(),
            ledger_origin_digest: ledger_origin.into(),
            material_identity: material_identity.into(),
            instance_id: instance_id.into(),
            height: executed.candidate.commitment.height,
            header_id: executed.candidate.header.block_id()?,
            actions_digest: blake3_hex(actions),
            snapshot_digest: blake3_hex(&executed.snapshot_bytes),
            snapshot_bytes: executed.snapshot_bytes.len() as u64,
            journal_digest: blake3_hex(&executed.journal_bytes),
            journal_bytes: executed.journal_bytes.len() as u64,
            derived_nodes_digest: derived_nodes_digest(executed)?,
            publication_bytes: publication_bytes as u64,
        })
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_cbor::to_vec(self).map_err(error)?;
        if bytes.len() > MAX_PUBLICATION_METADATA_BYTES {
            return Err("original publication metadata capacity".into());
        }
        Ok(bytes)
    }

    #[cfg(test)]
    pub(super) fn fixture(
        ledger_origin: &str,
        material_identity: &str,
        instance_id: &str,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
        publication_bytes: usize,
        mutation: Option<&str>,
    ) -> Vec<u8> {
        let snapshot_bytes = executed.snapshot_bytes.len() as u64;
        let journal_bytes = executed.journal_bytes.len() as u64;
        let mut reservation = Self {
            schema_version: 1,
            domain: DOMAIN.into(),
            ledger_origin_digest: ledger_origin.into(),
            material_identity: material_identity.into(),
            instance_id: instance_id.into(),
            height: executed.candidate.commitment.height,
            header_id: executed.candidate.header.block_id().unwrap(),
            actions_digest: blake3_hex(actions),
            snapshot_digest: blake3_hex(&executed.snapshot_bytes),
            snapshot_bytes,
            journal_digest: blake3_hex(&executed.journal_bytes),
            journal_bytes,
            derived_nodes_digest: blake3_hex(
                &serde_cbor::to_vec(&executed.result_delta.node_writes).unwrap(),
            ),
            publication_bytes: publication_bytes as u64,
        };
        match mutation {
            Some("snapshot") => reservation.snapshot_digest = "ab".repeat(32),
            Some("journal") => reservation.journal_bytes += 1,
            Some("derived") => reservation.derived_nodes_digest = "cd".repeat(32),
            Some("publication") => reservation.publication_bytes += 1,
            _ => {}
        }
        serde_cbor::to_vec(&reservation).unwrap()
    }
}

fn derived_nodes_digest(executed: &ExecutedFormalHeaderV3) -> Result<String, String> {
    let mut hash = blake3::Hasher::new();
    serde_cbor::to_writer(&mut hash, &executed.result_delta.node_writes)
        .map_err(|_| "encode original publication executed delta")?;
    Ok(hash.finalize().to_hex().to_string())
}
