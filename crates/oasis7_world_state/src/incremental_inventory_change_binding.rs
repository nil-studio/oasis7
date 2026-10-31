use crate::{
    incremental_inventory_genesis::InventoryWriteSink,
    incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
};
use oasis7_formal_protocol::{incremental_state::StateWriteV1, incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::ReservedValue;

pub(crate) fn write_digest(write: &StateWriteV1) -> Result<[u8; 32], ValueTreeError> {
    if write.key.len() > WORLD_FIELD_KEY_MAX_BYTES {
        return Err(ValueTreeError::Invalid("inventory write key bound"));
    }
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.iws.inventory-prepared-write.v1\0");
    hash.update(&(write.key.len() as u64).to_be_bytes());
    hash.update(&write.key);
    for root in [write.before, write.after] {
        hash.update(&[u8::from(root.is_some())]);
        if let Some(root) = root {
            hash.update(&root);
        }
    }
    Ok(*hash.finalize().as_bytes())
}

pub(crate) struct WriteBinding<'a, S> {
    pub sink: &'a mut S,
    pub digests: [Option<[u8; 32]>; 2],
    pub count: usize,
}
impl<S: InventoryWriteSink> InventoryWriteSink for WriteBinding<'_, S> {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        let slot = self
            .digests
            .get_mut(self.count)
            .ok_or(ValueTreeError::Invalid("inventory write count"))?;
        let digest = write_digest(&write)?;
        self.sink.put_write(write)?;
        *slot = Some(digest);
        self.count += 1;
        Ok(())
    }
}
