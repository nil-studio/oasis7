//! Bounded relative-address decoding; arbitrary original member IDs stream.
use super::*;
use crate::{incremental_semantic_key::SEMANTIC_KEY_DOMAIN, incremental_utf8_output::Utf8Output};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use std::io::{self, Write};
const SECTIONS: &[&str] = &[
    "manifest",
    "limits",
    "security",
    "active",
    "exports",
    "required_caps",
    "subscriptions",
    "subscriptions.event_kinds",
    "subscriptions.action_kinds",
    "abi_contract",
    "abi_contract.cap_slots",
    "abi_contract.policy_hooks",
    "abi_contract.gameplay",
    "abi_contract.gameplay.game_modes",
    "abi_contract.declarations.commands",
    "artifact_identity",
    "record",
];
const COLUMNS: &[&str] = &[
    "module_id",
    "name",
    "version",
    "kind",
    "role",
    "wasm_hash",
    "interface_version",
    "artifact_identity_present",
    "length",
    "value",
    "max_mem_bytes",
    "max_gas",
    "max_call_rate",
    "max_output_bytes",
    "max_effects",
    "max_emits",
    "marker",
    "stage",
    "filters_present",
    "filters",
    "abi_version",
    "input_schema",
    "output_schema",
    "gameplay_present",
    "min_players",
    "max_players",
    "namespace",
    "schema_version",
    "schema_hash",
    "max_payload_bytes",
    "source_hash",
    "build_manifest_hash",
    "signer_node_id",
    "signature_scheme",
    "artifact_signature",
    "registered_at",
    "registered_by",
    "audit_event_id",
    "content",
    "allow_builtin_manifest_fallback",
    "allow_identity_hash_signature",
    "allow_local_finality_signing",
    "allow_runtime_source_compile",
];
enum Parse {
    More,
    Invalid,
}
struct Prefix<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Prefix<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Parse> {
        let end = self.position.checked_add(count).ok_or(Parse::Invalid)?;
        let value = self.bytes.get(self.position..end).ok_or(Parse::More)?;
        self.position = end;
        Ok(value)
    }
    fn tag(&mut self, expected: u8) -> Result<(), Parse> {
        if self.take(1)? == [expected] {
            Ok(())
        } else {
            Err(Parse::Invalid)
        }
    }
    fn uint(&mut self) -> Result<u64, Parse> {
        self.tag(2)?;
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| Parse::Invalid)?,
        ))
    }
    fn text(&mut self) -> Result<&'a str, Parse> {
        self.tag(1)?;
        let length = u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| Parse::Invalid)?);
        if length > 64 {
            return Err(Parse::Invalid);
        }
        std::str::from_utf8(self.take(length as usize)?).map_err(|_| Parse::Invalid)
    }
    fn optional_ordinal(&mut self) -> Result<Option<u64>, Parse> {
        self.tag(5)?;
        let flag = self.uint()?;
        let value = self.uint()?;
        match flag {
            0 if value == 0 => Ok(None),
            1 => Ok(Some(value)),
            _ => Err(Parse::Invalid),
        }
    }
}
fn head(bytes: &[u8]) -> Result<(ArtifactConfigAddressV1<'static>, u64), Parse> {
    let mut input = Prefix { bytes, position: 0 };
    if input.take(SEMANTIC_KEY_DOMAIN.len())? != SEMANTIC_KEY_DOMAIN {
        return Err(Parse::Invalid);
    }
    input.tag(5)?;
    input.tag(5)?;
    let section = input.text()?;
    let section = SECTIONS
        .iter()
        .copied()
        .find(|value| *value == section)
        .ok_or(Parse::Invalid)?;
    let column = input.text()?;
    let column = COLUMNS
        .iter()
        .copied()
        .find(|value| *value == column)
        .ok_or(Parse::Invalid)?;
    input.tag(5)?;
    let ordinal = input.optional_ordinal()?;
    input.tag(5)?;
    let inner_ordinal = input.optional_ordinal()?;
    input.tag(5)?;
    let member = input.uint()?;
    input.tag(1)?;
    let length = u64::from_be_bytes(input.take(8)?.try_into().map_err(|_| Parse::Invalid)?);
    if member > 1 || member == 0 && length != 0 || input.position != bytes.len() {
        return Err(Parse::Invalid);
    }
    Ok((
        ArtifactConfigAddressV1 {
            section,
            ordinal,
            inner_ordinal,
            member: (member == 1).then_some(""),
            column,
        },
        length,
    ))
}
pub(super) struct Probe<'a> {
    prefix: [u8; 256],
    used: usize,
    address: Option<ArtifactConfigAddressV1<'static>>,
    remaining: u64,
    text: Utf8Output<'a, std::io::Sink>,
    bad: bool,
}
impl<'a> Probe<'a> {
    pub(super) fn new(sink: &'a mut std::io::Sink) -> Self {
        Self {
            prefix: [0; 256],
            used: 0,
            address: None,
            remaining: 0,
            text: Utf8Output::new(sink, "artifact config member key UTF8"),
            bad: false,
        }
    }
    fn fail(&mut self) -> io::Error {
        self.bad = true;
        io::Error::new(io::ErrorKind::InvalidData, "artifact config relative key")
    }
    pub(super) fn is_invalid(&self) -> bool {
        self.bad || self.text.is_invalid()
    }
    pub(super) fn finish(&self) -> Result<ArtifactConfigAddressV1<'static>, ValueTreeError> {
        if self.bad || self.remaining != 0 || self.text.is_invalid() || !self.text.is_complete() {
            return Err(read::invalid());
        }
        self.address.ok_or_else(read::invalid)
    }
}
impl Write for Probe<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bad {
            return Err(self.fail());
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.address.is_none() {
            if self.used == self.prefix.len() {
                return Err(self.fail());
            }
            self.prefix[self.used] = bytes[0];
            self.used += 1;
            match head(&self.prefix[..self.used]) {
                Ok((address, length)) => {
                    self.address = Some(address);
                    self.remaining = length;
                }
                Err(Parse::More) => {}
                Err(Parse::Invalid) => return Err(self.fail()),
            }
            return Ok(1);
        }
        if self.remaining == 0 {
            return Err(self.fail());
        }
        let count = self.remaining.min(bytes.len() as u64) as usize;
        let written = self.text.write(&bytes[..count])?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
