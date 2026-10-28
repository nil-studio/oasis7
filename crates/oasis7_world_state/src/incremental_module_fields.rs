//! Opaque reducer state, addressed by the exact state_key used by the ABI.
//! An empty payload is a present value. Module/version resolution is separate.
const DOMAIN: &[u8] = b"oasis7.iws.module-state-key.v1\0";
pub struct ModuleStateKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl ModuleStateKeyV1 {
    pub fn new(state_key: &str) -> Result<Self, &'static str> {
        if state_key.len() > 1024 - DOMAIN.len() - 2 {
            return Err("module state key length");
        }
        let mut bytes = [0; 1024];
        bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
        bytes[DOMAIN.len()..DOMAIN.len() + 2]
            .copy_from_slice(&(state_key.len() as u16).to_be_bytes());
        let length = DOMAIN.len() + 2 + state_key.len();
        bytes[DOMAIN.len() + 2..length].copy_from_slice(state_key.as_bytes());
        Ok(Self { bytes, length })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    pub fn state_key_from_bytes(bytes: &[u8]) -> Result<&str, &'static str> {
        if bytes.len() < DOMAIN.len() + 2 || bytes.len() > 1024 || !bytes.starts_with(DOMAIN) {
            return Err("module state key domain/length");
        }
        let length =
            u16::from_be_bytes(bytes[DOMAIN.len()..DOMAIN.len() + 2].try_into().unwrap()) as usize;
        if bytes.len() != DOMAIN.len() + 2 + length {
            return Err("module state key framing");
        }
        std::str::from_utf8(&bytes[DOMAIN.len() + 2..]).map_err(|_| "module state key UTF-8")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_framing_preserves_stable_keys_and_rejects_wrong_or_truncated_bytes() {
        for key in ["", "m#1", "m#2", "m\0雪"] {
            let mut expected = DOMAIN.to_vec();
            expected.extend((key.len() as u16).to_be_bytes());
            expected.extend(key.as_bytes());
            let encoded = ModuleStateKeyV1::new(key).unwrap();
            assert_eq!(encoded.as_bytes(), expected);
            assert_eq!(
                ModuleStateKeyV1::state_key_from_bytes(&expected).unwrap(),
                key
            );
            expected.push(0);
            assert!(ModuleStateKeyV1::state_key_from_bytes(&expected).is_err());
        }
        let max = "x".repeat(1024 - DOMAIN.len() - 2);
        assert_eq!(ModuleStateKeyV1::new(&max).unwrap().as_bytes().len(), 1024);
        assert!(ModuleStateKeyV1::new(&(max + "x")).is_err());
        let mut bad = ModuleStateKeyV1::new("x").unwrap().as_bytes().to_vec();
        *bad.last_mut().unwrap() = 255;
        assert!(ModuleStateKeyV1::state_key_from_bytes(&bad).is_err());
        assert!(ModuleStateKeyV1::state_key_from_bytes(b"unknown").is_err());
    }
}
