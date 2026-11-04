//! Original rolling sequence wire shared by Runtime production and independent receiving.
use crate::incremental_event_fields::{
    EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1,
};

use crate::incremental_runtime_registry::RUNTIME_SEQUENCE_KEY_DOMAIN_V1 as KEY;
const VALUE: &[u8] = b"oasis7.iws.runtime-sequence.v1\0";
pub const RUNTIME_SEQUENCE_BYTES_V1: usize = VALUE.len() + 17;
const BYTES: usize = RUNTIME_SEQUENCE_BYTES_V1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAllocatorV1 {
    Event,
    Action,
    Intent,
    Proposal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeSequenceV1 {
    pub next: u64,
    pub era: u64,
}
impl RuntimeAllocatorV1 {
    pub const ALL: [Self; 4] = [Self::Event, Self::Action, Self::Intent, Self::Proposal];
    fn tag(self) -> u8 {
        match self {
            Self::Event => 0,
            Self::Action => 1,
            Self::Intent => 2,
            Self::Proposal => 3,
        }
    }
    pub fn key(self) -> ([u8; KEY.len() + 1], usize) {
        let mut bytes = [0; KEY.len() + 1];
        let length = if self == Self::Event {
            bytes[..EVENT_SEQUENCE_KEY_V1.len()].copy_from_slice(EVENT_SEQUENCE_KEY_V1);
            EVENT_SEQUENCE_KEY_V1.len()
        } else {
            bytes[..KEY.len()].copy_from_slice(KEY);
            bytes[KEY.len()] = self.tag();
            bytes.len()
        };
        (bytes, length)
    }
    pub fn canonical_bytes(self, sequence: RuntimeSequenceV1) -> ([u8; BYTES], usize) {
        let mut bytes = [0; BYTES];
        if self == Self::Event {
            bytes[..EVENT_SEQUENCE_BYTES].copy_from_slice(
                &EventSequenceV1 {
                    next: sequence.next,
                    era: sequence.era,
                }
                .canonical_bytes(),
            );
            (bytes, EVENT_SEQUENCE_BYTES)
        } else {
            bytes[..VALUE.len()].copy_from_slice(VALUE);
            bytes[VALUE.len()] = self.tag();
            bytes[VALUE.len() + 1..VALUE.len() + 9].copy_from_slice(&sequence.next.to_be_bytes());
            bytes[VALUE.len() + 9..].copy_from_slice(&sequence.era.to_be_bytes());
            (bytes, BYTES)
        }
    }
    pub fn from_canonical_bytes(self, bytes: &[u8]) -> Result<RuntimeSequenceV1, &'static str> {
        if self == Self::Event {
            let value = EventSequenceV1::from_canonical_bytes(bytes)?;
            return Ok(RuntimeSequenceV1 {
                next: value.next,
                era: value.era,
            });
        }
        if bytes.len() != BYTES || !bytes.starts_with(VALUE) || bytes[VALUE.len()] != self.tag() {
            return Err("runtime allocator domain/tag/length");
        }
        Ok(RuntimeSequenceV1 {
            next: u64::from_be_bytes(
                bytes[VALUE.len() + 1..VALUE.len() + 9]
                    .try_into()
                    .map_err(|_| "runtime allocator next width")?,
            ),
            era: u64::from_be_bytes(
                bytes[VALUE.len() + 9..]
                    .try_into()
                    .map_err(|_| "runtime allocator era width")?,
            ),
        })
    }
}
