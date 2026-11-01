//! Lossless ledger *value* bytes. This is not a key, field registration, or
//! World acceptance capability. Sources and output storage remain caller-owned.
use crate::MaterialLedgerId;
use std::io::{self, Write};

pub const LEDGER_VALUE_DOMAIN: &[u8] = b"oasis7.iws.ledger-value.v1\0";
pub const LEDGER_VALUE_HEADER_BYTES: usize = LEDGER_VALUE_DOMAIN.len() + 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LedgerValueKindV1 {
    World = 0,
    Agent = 1,
    Site = 2,
    Factory = 3,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerValueRefV1<'a> {
    World,
    Agent(&'a str),
    Site(&'a str),
    Factory(&'a str),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerValueInfoV1 {
    pub kind: LedgerValueKindV1,
    pub id_bytes: u64,
}

fn parts(value: &MaterialLedgerId) -> (LedgerValueKindV1, &str) {
    match value {
        MaterialLedgerId::World => (LedgerValueKindV1::World, ""),
        MaterialLedgerId::Agent(id) => (LedgerValueKindV1::Agent, id),
        MaterialLedgerId::Site(id) => (LedgerValueKindV1::Site, id),
        MaterialLedgerId::Factory(id) => (LedgerValueKindV1::Factory, id),
    }
}
pub fn ledger_value_len_v1(value: &MaterialLedgerId) -> Result<u64, &'static str> {
    let length = u64::try_from(parts(value).1.len()).map_err(|_| "ledger ID length")?;
    (LEDGER_VALUE_HEADER_BYTES as u64)
        .checked_add(length)
        .ok_or("ledger value length")
}
/// Entire output is provisional on any error or unwind. The writer controls
/// memory/I/O admission, cancellation and durable storage; no ID-sized buffer
/// or formatted `world` / `agent:...` representation is allocated here.
pub fn write_ledger_value_v1(value: &MaterialLedgerId, output: &mut impl Write) -> io::Result<u64> {
    let length = ledger_value_len_v1(value).map_err(io::Error::other)?;
    let (kind, id) = parts(value);
    let mut header = [0; LEDGER_VALUE_HEADER_BYTES];
    header[..LEDGER_VALUE_DOMAIN.len()].copy_from_slice(LEDGER_VALUE_DOMAIN);
    header[LEDGER_VALUE_DOMAIN.len()] = kind as u8;
    header[LEDGER_VALUE_DOMAIN.len() + 1..].copy_from_slice(&(id.len() as u64).to_be_bytes());
    output.write_all(&header)?;
    output.write_all(id.as_bytes())?;
    Ok(length)
}
pub(crate) fn header_v1(header: &[u8], total: u64) -> Result<LedgerValueInfoV1, &'static str> {
    if header.len() != LEDGER_VALUE_HEADER_BYTES || !header.starts_with(LEDGER_VALUE_DOMAIN) {
        return Err("ledger value domain/header");
    }
    let kind = match header[LEDGER_VALUE_DOMAIN.len()] {
        0 => LedgerValueKindV1::World,
        1 => LedgerValueKindV1::Agent,
        2 => LedgerValueKindV1::Site,
        3 => LedgerValueKindV1::Factory,
        _ => return Err("ledger value kind"),
    };
    let id_bytes = u64::from_be_bytes(header[LEDGER_VALUE_DOMAIN.len() + 1..].try_into().unwrap());
    if (LEDGER_VALUE_HEADER_BYTES as u64).checked_add(id_bytes) != Some(total)
        || (kind == LedgerValueKindV1::World && id_bytes != 0)
    {
        return Err("ledger value length");
    }
    Ok(LedgerValueInfoV1 { kind, id_bytes })
}
/// Borrows a complete caller-admitted frame. This does not read storage or
/// qualify the ledger's business identity.
pub fn read_ledger_value_v1(bytes: &[u8]) -> Result<LedgerValueRefV1<'_>, &'static str> {
    let header = bytes
        .get(..LEDGER_VALUE_HEADER_BYTES)
        .ok_or("ledger value header")?;
    let total = u64::try_from(bytes.len()).map_err(|_| "ledger value length")?;
    let info = header_v1(header, total)?;
    let id =
        std::str::from_utf8(&bytes[LEDGER_VALUE_HEADER_BYTES..]).map_err(|_| "ledger UTF-8")?;
    Ok(match info.kind {
        LedgerValueKindV1::World => LedgerValueRefV1::World,
        LedgerValueKindV1::Agent => LedgerValueRefV1::Agent(id),
        LedgerValueKindV1::Site => LedgerValueRefV1::Site(id),
        LedgerValueKindV1::Factory => LedgerValueRefV1::Factory(id),
    })
}

#[path = "incremental_ledger_value_stream.rs"]
mod stream;
pub(crate) use crate::incremental_utf8_output::Utf8Output;
pub use stream::read_ledger_value_tree_v1;

#[cfg(test)]
#[path = "incremental_ledger_value_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "incremental_ledger_value_tree_tests.rs"]
mod tree_tests;
