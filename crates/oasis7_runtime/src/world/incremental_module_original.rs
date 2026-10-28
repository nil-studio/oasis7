//! Streaming the existing canonical ABI event, including its array-of-u8
//! representation. Never replace that original with a byte-string summary.
use super::incremental_body::BodyMutationError;
use crate::{CausedBy, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::io::{self, Read};

pub(super) struct ModuleStateOriginal<'a> {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    state: &'a [u8],
    pub(super) length: u64,
    _charge: MemoryReservation,
}
impl<'a> ModuleStateOriginal<'a> {
    pub(super) fn new(
        event: &'a WorldEvent,
        max_bytes: u64,
        memory: &MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        let WorldEvent {
            id,
            time,
            caused_by,
            body,
        } = event;
        let WorldEventBody::ModuleStateUpdated(update) = body else {
            return Err(BodyMutationError::Invalid(
                "module state original event kind",
            ));
        };
        let oasis7_wasm_abi::ModuleStateUpdate {
            module_id,
            trace_id,
            state,
        } = update;
        let cause_len = match caused_by {
            Some(CausedBy::Effect { intent_id }) => intent_id.len(),
            Some(CausedBy::Action(_)) | None => 0,
        };
        let strings = module_id
            .len()
            .checked_add(trace_id.len())
            .and_then(|n| n.checked_add(cause_len))
            .ok_or(BodyMutationError::Invalid("module event size overflow"))?;
        if state.len() as u64 > max_bytes || strings as u64 > max_bytes {
            return Err(ValueTreeError::ResourceLimited("module original byte bound").into());
        }
        let suffix_capacity = strings.checked_add(256).ok_or(BodyMutationError::Invalid(
            "module original workspace overflow",
        ))?;
        let charge = memory
            .reserve(
                suffix_capacity
                    .checked_add(32 * 1024)
                    .ok_or(BodyMutationError::Invalid(
                        "module original workspace overflow",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut prefix = Vec::with_capacity(256);
        head(&mut prefix, 5, 4);
        text(&mut prefix, "id");
        head(&mut prefix, 0, *id);
        text(&mut prefix, "body");
        head(&mut prefix, 5, 2);
        text(&mut prefix, "kind");
        text(&mut prefix, "ModuleStateUpdated");
        text(&mut prefix, "payload");
        head(&mut prefix, 5, 3);
        text(&mut prefix, "state");
        head(&mut prefix, 4, state.len() as u64);
        let mut suffix = Vec::with_capacity(suffix_capacity);
        text(&mut suffix, "trace_id");
        text(&mut suffix, trace_id);
        text(&mut suffix, "module_id");
        text(&mut suffix, module_id);
        text(&mut suffix, "time");
        head(&mut suffix, 0, *time);
        text(&mut suffix, "caused_by");
        match caused_by {
            None => suffix.push(0xf6),
            Some(CausedBy::Action(action)) => {
                head(&mut suffix, 5, 2);
                text(&mut suffix, "data");
                head(&mut suffix, 0, *action);
                text(&mut suffix, "type");
                text(&mut suffix, "Action");
            }
            Some(CausedBy::Effect { intent_id }) => {
                head(&mut suffix, 5, 2);
                text(&mut suffix, "data");
                head(&mut suffix, 5, 1);
                text(&mut suffix, "intent_id");
                text(&mut suffix, intent_id);
                text(&mut suffix, "type");
                text(&mut suffix, "Effect");
            }
        }
        if prefix.capacity() != 256 || suffix.capacity() != suffix_capacity {
            return Err(BodyMutationError::Invalid(
                "module original metadata admission",
            ));
        }
        let state_len = (state.len() as u64)
            .checked_add(state.iter().filter(|byte| **byte >= 24).count() as u64)
            .ok_or(BodyMutationError::Invalid(
                "module original length overflow",
            ))?;
        let length = state_len
            .checked_add(prefix.len() as u64)
            .and_then(|n| n.checked_add(suffix.len() as u64))
            .ok_or(BodyMutationError::Invalid(
                "module original length overflow",
            ))?;
        if length > max_bytes {
            return Err(ValueTreeError::ResourceLimited("module original byte bound").into());
        }
        Ok(Self {
            prefix,
            suffix,
            state,
            length,
            _charge: charge,
        })
    }
    pub(super) fn reader(&self) -> ModuleStateOriginalReader<'_> {
        ModuleStateOriginalReader {
            prefix: &self.prefix,
            suffix: &self.suffix,
            state: self.state,
            pending: None,
        }
    }
    pub(super) fn digest(
        &self,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<[u8; 32], BodyMutationError> {
        let mut reader = self.reader();
        let mut hash = blake3::Hasher::new();
        let mut buffer = [0; 16 * 1024];
        loop {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let n = reader.read(&mut buffer).map_err(ValueTreeError::Io)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok(*hash.finalize().as_bytes())
    }
}
pub(super) struct ModuleStateOriginalReader<'a> {
    prefix: &'a [u8],
    suffix: &'a [u8],
    state: &'a [u8],
    pending: Option<u8>,
}
impl Read for ModuleStateOriginalReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut position = 0;
        if !self.prefix.is_empty() {
            let n = self.prefix.len().min(output.len());
            output[..n].copy_from_slice(&self.prefix[..n]);
            self.prefix = &self.prefix[n..];
            position += n;
        }
        if position < output.len()
            && let Some(byte) = self.pending.take()
        {
            output[position] = byte;
            position += 1;
        }
        while position < output.len() && !self.state.is_empty() {
            let byte = self.state[0];
            self.state = &self.state[1..];
            if byte < 24 {
                output[position] = byte;
                position += 1;
            } else {
                output[position] = 24;
                position += 1;
                if position < output.len() {
                    output[position] = byte;
                    position += 1;
                } else {
                    self.pending = Some(byte);
                }
            }
        }
        if position < output.len() && self.state.is_empty() && self.pending.is_none() {
            let n = self.suffix.len().min(output.len() - position);
            output[position..position + n].copy_from_slice(&self.suffix[..n]);
            self.suffix = &self.suffix[n..];
            position += n;
        }
        Ok(position)
    }
}
fn text(output: &mut Vec<u8>, value: &str) {
    head(output, 3, value.len() as u64);
    output.extend_from_slice(value.as_bytes());
}
fn head(output: &mut Vec<u8>, major: u8, value: u64) {
    let tag = major << 5;
    match value {
        0..=23 => output.push(tag | value as u8),
        24..=255 => {
            output.push(tag | 24);
            output.push(value as u8);
        }
        256..=65535 => {
            output.push(tag | 25);
            output.extend_from_slice(&(value as u16).to_be_bytes());
        }
        65536..=4294967295 => {
            output.push(tag | 26);
            output.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            output.push(tag | 27);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

#[cfg(test)]
#[path = "incremental_module_original_tests.rs"]
mod tests;
