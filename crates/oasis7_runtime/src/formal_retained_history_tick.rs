//! Immutable Tick audit records and exact tick locators beneath the independently
//! qualified retained-history root. No caller digest establishes a new root.
use super::*;
use crate::TickConsensusRecord;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedTickConsensusRecordV1 {
    height: u64,
    tick: u64,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    record_cbor: Vec<u8>,
    record_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedTickConsensusHeightV1 {
    tick: u64,
    height: u64,
}
pub(crate) fn retained_tick_consensus_leaves(
    record: &TickConsensusRecord,
) -> Result<Vec<BusinessIndexLeafV1>, String> {
    let height = record.certificate.consensus_height;
    let tick = record.block.header.tick;
    if height == 0 || record.certificate.block_hash != record.block.block_hash() {
        return Err("retained Tick record identity/hash mismatch".into());
    }
    let record_cbor = oasis7_wasm_abi::encode_canonical_cbor(record).map_err(|e| e.to_string())?;
    let record_digest = blake3::hash(&record_cbor).to_hex().to_string();
    let value = RetainedHistoryLeafV1::TickConsensusRecord(RetainedTickConsensusRecordV1 {
        height,
        tick,
        record_cbor,
        record_digest,
    });
    let locator =
        RetainedHistoryLeafV1::TickConsensusHeight(RetainedTickConsensusHeightV1 { tick, height });
    Ok(vec![
        retained_leaf(
            retained_key_digest("world-tick-consensus-height", &height)
                .map_err(|e| e.to_string())?,
            &value,
        )?,
        retained_leaf(
            retained_key_digest("world-tick-consensus-tick", &tick).map_err(|e| e.to_string())?,
            &locator,
        )?,
    ])
}
impl FormalRetainedHistoryIndexViewV1 {
    pub(crate) fn read_tick_consensus_record(
        &self,
        height: u64,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<TickConsensusRecord>, RetainedHistoryReadError> {
        if height == 0 {
            return Err(RetainedHistoryReadError::Invalid(
                "retained Tick height is zero".into(),
            ));
        }
        let key = retained_key_digest("world-tick-consensus-height", &height)?;
        let entry = match self.read_leaf(&key, reader)? {
            None => return Ok(None),
            Some(RetainedHistoryLeafV1::TickConsensusRecord(entry)) => entry,
            Some(_) => {
                return Err(RetainedHistoryReadError::Invalid(
                    "retained Tick height leaf kind mismatch".into(),
                ));
            }
        };
        let fail = |message: &str| RetainedHistoryReadError::Invalid(message.into());
        if entry.height != height
            || entry.record_cbor.is_empty()
            || blake3::hash(&entry.record_cbor).to_hex().as_str() != entry.record_digest
        {
            return Err(fail("retained Tick record digest/height mismatch"));
        }
        let record: TickConsensusRecord =
            serde_cbor::from_slice(&entry.record_cbor).map_err(|e| fail(&e.to_string()))?;
        if record.certificate.consensus_height != height
            || record.block.header.tick != entry.tick
            || record.certificate.block_hash != record.block.block_hash()
            || oasis7_wasm_abi::encode_canonical_cbor(&record).map_err(|e| fail(&e.to_string()))?
                != entry.record_cbor
        {
            return Err(fail(
                "retained Tick record canonical bytes/identity mismatch",
            ));
        }
        Ok(Some(record))
    }
    pub(crate) fn read_tick_consensus_at_tick(
        &self,
        tick: u64,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<TickConsensusRecord>, RetainedHistoryReadError> {
        let key = retained_key_digest("world-tick-consensus-tick", &tick)?;
        let locator = match self.read_leaf(&key, reader)? {
            None => return Ok(None),
            Some(RetainedHistoryLeafV1::TickConsensusHeight(locator)) => locator,
            Some(_) => {
                return Err(RetainedHistoryReadError::Invalid(
                    "retained Tick locator leaf kind mismatch".into(),
                ));
            }
        };
        if locator.tick != tick || locator.height == 0 {
            return Err(RetainedHistoryReadError::Invalid(
                "retained Tick locator identity mismatch".into(),
            ));
        }
        let record = self
            .read_tick_consensus_record(locator.height, reader)?
            .ok_or_else(|| {
                RetainedHistoryReadError::Invalid(
                    "retained Tick locator references missing record".into(),
                )
            })?;
        if record.block.header.tick != tick {
            return Err(RetainedHistoryReadError::Invalid(
                "retained Tick locator references another tick".into(),
            ));
        }
        Ok(Some(record))
    }
}
