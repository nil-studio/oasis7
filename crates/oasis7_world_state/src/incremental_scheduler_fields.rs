//! Runtime module wake fields. Keys preserve exact stable instance identity;
//! a value is an absolute simulation time, never a relative delay or authority.
const KEY: &[u8] = b"oasis7.iws.module-wake-key.v1\0";
const VALUE: &[u8] = b"oasis7.iws.module-wake.v1\0";
pub const MODULE_WAKE_BYTES: usize = VALUE.len() + 8;

pub struct ModuleWakeKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl ModuleWakeKeyV1 {
    pub fn new(instance: &str) -> Result<Self, &'static str> {
        if instance.is_empty() || instance.len() > 1024 - KEY.len() - 2 {
            return Err("module wake key length");
        }
        let length = KEY.len() + 2 + instance.len();
        let mut bytes = [0; 1024];
        bytes[..KEY.len()].copy_from_slice(KEY);
        bytes[KEY.len()..KEY.len() + 2].copy_from_slice(&(instance.len() as u16).to_be_bytes());
        bytes[KEY.len() + 2..length].copy_from_slice(instance.as_bytes());
        Ok(Self { bytes, length })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    pub fn instance_from_bytes(bytes: &[u8]) -> Result<&str, &'static str> {
        if bytes.len() < KEY.len() + 2 || bytes.len() > 1024 || !bytes.starts_with(KEY) {
            return Err("module wake key domain/length");
        }
        let length =
            u16::from_be_bytes(bytes[KEY.len()..KEY.len() + 2].try_into().unwrap()) as usize;
        if length == 0 || bytes.len() != KEY.len() + 2 + length {
            return Err("module wake key framing");
        }
        std::str::from_utf8(&bytes[KEY.len() + 2..]).map_err(|_| "module wake key UTF-8")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleWakeV1(pub u64);
impl ModuleWakeV1 {
    pub fn canonical_bytes(self) -> [u8; MODULE_WAKE_BYTES] {
        let mut bytes = [0; MODULE_WAKE_BYTES];
        bytes[..VALUE.len()].copy_from_slice(VALUE);
        bytes[VALUE.len()..].copy_from_slice(&self.0.to_be_bytes());
        bytes
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != MODULE_WAKE_BYTES || !bytes.starts_with(VALUE) {
            return Err("module wake value domain/length");
        }
        Ok(Self(u64::from_be_bytes(
            bytes[VALUE.len()..].try_into().unwrap(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_wire_preserves_instances_times_and_rejects_ambiguous_keys() {
        let key = ModuleWakeKeyV1::new("m#2\0雪").unwrap();
        let mut expected = b"oasis7.iws.module-wake-key.v1\0".to_vec();
        expected.extend(7u16.to_be_bytes());
        expected.extend("m#2\0雪".as_bytes());
        assert_eq!(key.as_bytes(), expected);
        assert_eq!(
            ModuleWakeKeyV1::instance_from_bytes(&expected).unwrap(),
            "m#2\0雪"
        );
        assert_ne!(
            ModuleWakeKeyV1::new("m#1").unwrap().as_bytes(),
            ModuleWakeKeyV1::new("m#2").unwrap().as_bytes()
        );
        for time in [0, 1, u64::MAX] {
            let mut bytes = b"oasis7.iws.module-wake.v1\0".to_vec();
            bytes.extend(time.to_be_bytes());
            assert_eq!(ModuleWakeV1(time).canonical_bytes().as_slice(), bytes);
            assert_eq!(
                ModuleWakeV1::from_canonical_bytes(&bytes).unwrap(),
                ModuleWakeV1(time)
            );
            bytes.push(0);
            assert!(ModuleWakeV1::from_canonical_bytes(&bytes).is_err());
        }
        assert!(ModuleWakeKeyV1::new("").is_err());
        let mut invalid_utf8 = ModuleWakeKeyV1::new("a").unwrap().as_bytes().to_vec();
        *invalid_utf8.last_mut().unwrap() = 255;
        assert!(ModuleWakeKeyV1::instance_from_bytes(&invalid_utf8).is_err());
        let mut wrong = ModuleWakeV1(5).canonical_bytes();
        wrong[0] ^= 1;
        assert!(ModuleWakeV1::from_canonical_bytes(&wrong).is_err());
        assert!(
            ModuleWakeV1::from_canonical_bytes(
                &ModuleWakeV1(5).canonical_bytes()[..MODULE_WAKE_BYTES - 1]
            )
            .is_err()
        );
        assert!(ModuleWakeKeyV1::new(&"x".repeat(1024)).is_err());
        for bytes in [&expected[..expected.len() - 1], b"unknown", &[255; 50][..]] {
            assert!(ModuleWakeKeyV1::instance_from_bytes(bytes).is_err());
        }
        let max = "x".repeat(1024 - KEY.len() - 2);
        assert_eq!(ModuleWakeKeyV1::new(&max).unwrap().as_bytes().len(), 1024);
    }
}
