//! Dedicated opaque module-state key family. Module state is not a generic
//! field-member value and remains keyed by the exact reducer state key.
const DOMAIN: &[u8] = b"oasis7.iws.module-state-key.v1\0";
pub const MODULE_STATE_KEY_MAX_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleStateKeyV1 {
    bytes: [u8; MODULE_STATE_KEY_MAX_BYTES],
    length: usize,
}

impl ModuleStateKeyV1 {
    pub fn is_state_key(bytes: &[u8]) -> bool {
        bytes.starts_with(DOMAIN)
    }

    pub fn new(state_key: &str) -> Result<Self, &'static str> {
        let key_len = u16::try_from(state_key.len()).map_err(|_| "module state key length")?;
        let length = DOMAIN
            .len()
            .checked_add(2)
            .and_then(|n| n.checked_add(state_key.len()))
            .ok_or("module state key length")?;
        if length > MODULE_STATE_KEY_MAX_BYTES {
            return Err("module state key length");
        }
        let mut bytes = [0; MODULE_STATE_KEY_MAX_BYTES];
        bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
        bytes[DOMAIN.len()..DOMAIN.len() + 2].copy_from_slice(&key_len.to_be_bytes());
        bytes[DOMAIN.len() + 2..length].copy_from_slice(state_key.as_bytes());
        Ok(Self { bytes, length })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    pub fn state_key_from_bytes(bytes: &[u8]) -> Result<&str, &'static str> {
        if bytes.len() < DOMAIN.len() + 2
            || bytes.len() > MODULE_STATE_KEY_MAX_BYTES
            || !bytes.starts_with(DOMAIN)
        {
            return Err("module state key domain/length");
        }
        let length = usize::from(u16::from_be_bytes(
            bytes[DOMAIN.len()..DOMAIN.len() + 2]
                .try_into()
                .map_err(|_| "module state key framing")?,
        ));
        if bytes.len() != DOMAIN.len() + 2 + length {
            return Err("module state key framing");
        }
        let state_key = std::str::from_utf8(&bytes[DOMAIN.len() + 2..])
            .map_err(|_| "module state key UTF-8")?;
        if Self::new(state_key)?.as_bytes() != bytes {
            return Err("module state key canonical");
        }
        Ok(state_key)
    }
}

#[cfg(test)]
#[path = "incremental_module_fields_tests.rs"]
mod tests;
