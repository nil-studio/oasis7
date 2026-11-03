//! Derived counts include every stored instance, even inactive ones. They
//! reproduce legacy global-activation suppression, not invocation authority.
const COUNT_KEY: &[u8] = b"oasis7.iws.module-instance-count-key.v1\0";
pub const MODULE_INSTANCE_COUNT_KEY_BYTES: usize = COUNT_KEY.len() + 32;
const COUNT: &[u8] = b"oasis7.iws.module-instance-count.v1\0";
const INDEX: &[u8] = b"oasis7.iws.module-instance-count-index.v1\0";
pub const MODULE_INSTANCE_INDEX_KEY_V1: &[u8] = b"oasis7.iws.module-instance-count-index-key.v1\0";
pub const MODULE_INSTANCE_COUNT_BYTES: usize = COUNT.len() + 8;
pub const MODULE_INSTANCE_INDEX_BYTES: usize = INDEX.len() + 8;
pub fn module_instance_count_key(module_id: &str) -> [u8; COUNT_KEY.len() + 32] {
    let mut hash = ModuleInstanceCountKeyHashV1::new(module_id.len() as u64);
    hash.update(module_id.as_bytes())
        .expect("original string length");
    hash.finish().expect("complete original string")
}
/// Same identity as the original string producer, without materializing text.
pub struct ModuleInstanceCountKeyHashV1 {
    hash: blake3::Hasher,
    remaining: u64,
}
impl ModuleInstanceCountKeyHashV1 {
    pub fn new(length: u64) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.module-instance-count-id.v1\0");
        hash.update(&length.to_be_bytes());
        Self {
            hash,
            remaining: length,
        }
    }
    pub fn update(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        self.remaining = self
            .remaining
            .checked_sub(bytes.len() as u64)
            .ok_or("module ID excess bytes")?;
        self.hash.update(bytes);
        Ok(())
    }
    pub fn finish(self) -> Result<[u8; MODULE_INSTANCE_COUNT_KEY_BYTES], &'static str> {
        if self.remaining != 0 {
            return Err("module ID incomplete");
        }
        let mut bytes = [0; MODULE_INSTANCE_COUNT_KEY_BYTES];
        bytes[..COUNT_KEY.len()].copy_from_slice(COUNT_KEY);
        bytes[COUNT_KEY.len()..].copy_from_slice(self.hash.finalize().as_bytes());
        Ok(bytes)
    }
}
pub fn is_module_instance_count_key(bytes: &[u8]) -> bool {
    bytes.starts_with(COUNT_KEY)
}
pub fn validate_module_instance_count_key(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.starts_with(COUNT_KEY) && bytes.len() == COUNT_KEY.len() + 32 {
        Ok(())
    } else {
        Err("module instance count key domain/length")
    }
}
pub fn module_instance_count_bytes(
    count: u64,
) -> Result<[u8; MODULE_INSTANCE_COUNT_BYTES], &'static str> {
    if count == 0 {
        return Err("zero module instance count must be absent");
    }
    let mut bytes = [0; MODULE_INSTANCE_COUNT_BYTES];
    bytes[..COUNT.len()].copy_from_slice(COUNT);
    bytes[COUNT.len()..].copy_from_slice(&count.to_be_bytes());
    Ok(bytes)
}
pub fn module_instance_count_from_bytes(bytes: &[u8]) -> Result<u64, &'static str> {
    let n = number(bytes, COUNT)?;
    if n == 0 {
        return Err("zero module instance count must be absent");
    }
    Ok(n)
}
pub fn module_instance_index_bytes(total: u64) -> [u8; MODULE_INSTANCE_INDEX_BYTES] {
    let mut bytes = [0; MODULE_INSTANCE_INDEX_BYTES];
    bytes[..INDEX.len()].copy_from_slice(INDEX);
    bytes[INDEX.len()..].copy_from_slice(&total.to_be_bytes());
    bytes
}
pub fn module_instance_index_from_bytes(bytes: &[u8]) -> Result<u64, &'static str> {
    number(bytes, INDEX)
}
fn number(bytes: &[u8], domain: &[u8]) -> Result<u64, &'static str> {
    if bytes.len() != domain.len() + 8 || !bytes.starts_with(domain) {
        return Err("module instance count domain/length");
    }
    Ok(u64::from_be_bytes(
        bytes[domain.len()..].try_into().unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_codec_preserves_identity_and_rejects_noncanonical_numbers() {
        for n in [1, 256, u64::MAX] {
            let bytes = module_instance_count_bytes(n).unwrap();
            assert_eq!(module_instance_count_from_bytes(&bytes).unwrap(), n);
            assert!(module_instance_index_from_bytes(&bytes).is_err());
            assert!(module_instance_count_from_bytes(&bytes[..bytes.len() - 1]).is_err());
            let mut trailing = bytes.to_vec();
            trailing.push(0);
            assert!(module_instance_count_from_bytes(&trailing).is_err());
        }
        assert!(module_instance_count_bytes(0).is_err());
        assert!(module_instance_count_from_bytes(&module_instance_index_bytes(0)).is_err());
        for n in [0, 1, u64::MAX] {
            assert_eq!(
                module_instance_index_from_bytes(&module_instance_index_bytes(n)).unwrap(),
                n
            );
        }
        let ids = ["", "a", " a", "a ", "A", "a\0", "模块"];
        for (i, a) in ids.iter().enumerate() {
            assert_eq!(module_instance_count_key(a), module_instance_count_key(a));
            for b in &ids[i + 1..] {
                assert_ne!(module_instance_count_key(a), module_instance_count_key(b));
            }
        }
        assert_ne!(
            module_instance_count_key(&"x".repeat(2048)),
            module_instance_count_key(&"x".repeat(2049))
        );
    }
}
