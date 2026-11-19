//! Private original Journal archive nominal layout; stored originals remain bytes.
use super::super::owned_layout::record;
use super::*;
record!(RetainedJournalEventV1,{event_id_era,event_id,event_cbor,event_digest});
record!(RetainedJournalTickLocatorV1,{tick,event_id,event_id_era});
