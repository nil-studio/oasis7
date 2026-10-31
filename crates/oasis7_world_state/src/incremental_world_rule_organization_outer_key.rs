//! Bounded outer-key recognition; original Case IDs are streamed, not retained.
use super::*;
use crate::{incremental_semantic_key::SEMANTIC_KEY_DOMAIN, incremental_utf8_output::Utf8Output};
use std::io::{self, Write};
pub(in super::super) struct OuterProbe<'a> {
    prefix: [u8; 256],
    used: usize,
    owner: Option<OrganizationRootKeyV1<'static>>,
    other: bool,
    remaining: Option<u64>,
    text: Utf8Output<'a, io::Sink>,
    pub bad: Option<&'static str>,
}
impl<'a> OuterProbe<'a> {
    pub fn new(sink: &'a mut io::Sink) -> Self {
        Self {
            prefix: [0; 256],
            used: 0,
            owner: None,
            other: false,
            remaining: None,
            text: Utf8Output::new(sink, "organization outer key UTF-8"),
            bad: None,
        }
    }
    fn fail(&mut self, reason: &'static str) -> io::Error {
        self.bad = Some(reason);
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }
    fn head(&mut self) -> Result<(), &'static str> {
        let bytes = &self.prefix[..self.used];
        let common = bytes.len().min(SEMANTIC_KEY_DOMAIN.len());
        if bytes[..common] != SEMANTIC_KEY_DOMAIN[..common] {
            return Err("organization outer semantic domain");
        }
        let mut p = SEMANTIC_KEY_DOMAIN.len();
        if bytes.len() <= p {
            return Ok(());
        }
        let mut depth = 0;
        while bytes.get(p) == Some(&5) {
            depth += 1;
            p += 1;
            if depth > 16 {
                return Err("organization outer key depth");
            }
        }
        let Some(tag) = bytes.get(p) else {
            return Ok(());
        };
        p += 1;
        if *tag == 0 && depth == 0 {
            self.other = true;
            return Ok(());
        }
        if *tag != 1 {
            return Err("organization outer key root");
        }
        let Some(length) = bytes.get(p..p + 8) else {
            return Ok(());
        };
        p += 8;
        let length = u64::from_be_bytes(length.try_into().unwrap());
        if length > 128 {
            return Err("organization outer static key length");
        }
        let Some(name) = bytes.get(p..p + length as usize) else {
            return Ok(());
        };
        p += length as usize;
        let name = std::str::from_utf8(name).map_err(|_| "organization outer static key UTF-8")?;
        let owner = match name {
            "organization_cases" => OrganizationRootKeyV1::Case(""),
            "organization_estate" => OrganizationRootKeyV1::Estate(""),
            _ => {
                self.other = true;
                return Ok(());
            }
        };
        if depth != 1 {
            return Err("organization flat orphan descendant");
        }
        let Some(tag) = bytes.get(p) else {
            return Ok(());
        };
        p += 1;
        if *tag != 1 {
            return Err("organization original map key type");
        }
        let Some(length) = bytes.get(p..p + 8) else {
            return Ok(());
        };
        self.owner = Some(owner);
        self.remaining = Some(u64::from_be_bytes(length.try_into().unwrap()));
        Ok(())
    }
    pub fn finish(&self) -> Result<Option<OrganizationRootKeyV1<'static>>, ValueTreeError> {
        if let Some(reason) = self.bad {
            return Err(ValueTreeError::Invalid(reason));
        }
        if self.other {
            return Ok(None);
        }
        if self.remaining != Some(0) || !self.text.is_complete() {
            return Err(ValueTreeError::Invalid("organization outer key truncated"));
        }
        self.owner
            .map(Some)
            .ok_or(ValueTreeError::Invalid("organization outer key empty"))
    }
}
impl Write for OuterProbe<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.other {
            return Ok(bytes.len());
        }
        if let Some(remaining) = self.remaining {
            if remaining == 0 {
                return Err(self.fail("organization outer key trailing bytes"));
            }
            let take = (u128::from(remaining)).min(bytes.len() as u128) as usize;
            let n = self.text.write(&bytes[..take])?;
            self.remaining = Some(remaining - n as u64);
            return Ok(n);
        }
        if self.used == self.prefix.len() {
            return Err(self.fail("organization outer prefix bound"));
        }
        self.prefix[self.used] = bytes[0];
        self.used += 1;
        if let Err(reason) = self.head() {
            return Err(self.fail(reason));
        }
        Ok(1)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
