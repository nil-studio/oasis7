use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedJournalEventV1 {
    pub event_id_era: u64,
    pub event_id: u64,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    pub event_cbor: Vec<u8>,
    pub event_digest: String,
}

impl RetainedJournalEventV1 {
    pub(crate) fn encode(event_id_era: u64, event: &WorldEvent) -> Result<Self, String> {
        if event.id == 0 {
            return Err("journal event id is zero".into());
        }
        let event_cbor =
            oasis7_wasm_abi::encode_canonical_cbor(event).map_err(|error| error.to_string())?;
        let event_digest = blake3::hash(&event_cbor).to_hex().to_string();
        Ok(Self {
            event_id_era,
            event_id: event.id,
            event_cbor,
            event_digest,
        })
    }

    pub(crate) fn decode(&self) -> Result<WorldEvent, String> {
        if self.event_id == 0
            || self.event_cbor.is_empty()
            || self.event_digest != blake3::hash(&self.event_cbor).to_hex().to_string()
        {
            return Err("retained journal event digest or identity is invalid".into());
        }
        let event: WorldEvent =
            serde_cbor::from_slice(&self.event_cbor).map_err(|error| error.to_string())?;
        let canonical =
            oasis7_wasm_abi::encode_canonical_cbor(&event).map_err(|error| error.to_string())?;
        if canonical != self.event_cbor || event.id != self.event_id {
            return Err("retained journal event bytes or id are not canonical".into());
        }
        Ok(event)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedJournalTickLocatorV1 {
    tick: u64,
    event_id: u64,
    event_id_era: u64,
}
/// Immutable companion to an archived (era,id) JournalEvent. Time and id
/// choose exactly one authenticated era without scanning old epochs.
pub(crate) fn retained_tick_journal_leaf(
    era: u64,
    event: &WorldEvent,
) -> Result<(String, RetainedHistoryLeafV1), String> {
    if event.id == 0 || era == u64::MAX {
        return Err("retained tick event locator identity invalid".into());
    }
    let key = retained_key_digest("world-journal-tick-event", &(event.time, event.id))
        .map_err(|e| e.to_string())?;
    Ok((
        key,
        RetainedHistoryLeafV1::JournalTickLocator(RetainedJournalTickLocatorV1 {
            tick: event.time,
            event_id: event.id,
            event_id_era: era,
        }),
    ))
}
impl FormalRetainedHistoryIndexViewV1 {
    pub(crate) fn read_tick_journal_event(
        &self,
        tick: u64,
        event_id: u64,
        max_event_id_era: u64,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<WorldEvent>, RetainedHistoryReadError> {
        if event_id == 0 {
            return Err(RetainedHistoryReadError::Invalid(
                "retained tick event id is zero".into(),
            ));
        }
        let key = retained_key_digest("world-journal-tick-event", &(tick, event_id))?;
        let locator = match self.read_leaf(&key, reader)? {
            None => return Ok(None),
            Some(RetainedHistoryLeafV1::JournalTickLocator(locator)) => locator,
            Some(_) => {
                return Err(RetainedHistoryReadError::Invalid(
                    "retained tick locator leaf kind mismatch".into(),
                ));
            }
        };
        if locator.tick != tick
            || locator.event_id != event_id
            || locator.event_id_era == u64::MAX
            || locator.event_id_era > max_event_id_era
        {
            return Err(RetainedHistoryReadError::Invalid(
                "retained tick locator time/era/id mismatch".into(),
            ));
        }
        let event = self
            .read_journal_event(locator.event_id_era, event_id, reader)?
            .ok_or_else(|| {
                RetainedHistoryReadError::Invalid(
                    "retained tick locator references missing journal event".into(),
                )
            })?;
        if event.time != tick || event.id != event_id {
            return Err(RetainedHistoryReadError::Invalid(
                "retained tick event time/id mismatch".into(),
            ));
        }
        Ok(Some(event))
    }
}
