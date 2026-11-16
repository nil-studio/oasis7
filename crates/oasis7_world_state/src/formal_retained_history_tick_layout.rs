//! Private original Tick archive nominal layout, with no field visibility lift.
use super::*;
use super::super::owned_layout::record;
record!(RetainedTickConsensusRecordV1,{height,tick,record_cbor,record_digest});
record!(RetainedTickConsensusHeightV1,{tick,height});
