use super::*;

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    pub(super) fn envelope(
        bytes: &'a [u8],
        magic: &[u8],
        max: usize,
    ) -> Result<Self, ExecutionError> {
        if bytes.len() > max || !bytes.starts_with(magic) {
            return Err(ExecutionError::Invalid("object domain/byte bound"));
        }
        let mut cursor = Self {
            bytes,
            position: magic.len(),
        };
        let length = cursor.u32()? as usize;
        if bytes.len() - cursor.position != length {
            return Err(ExecutionError::Invalid("object declared length"));
        }
        Ok(cursor)
    }
    pub(super) fn bytes(&mut self, length: usize) -> Result<&'a [u8], ExecutionError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ExecutionError::Invalid("field length overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(ExecutionError::Invalid("truncated object"))?;
        self.position = end;
        Ok(bytes)
    }
    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], ExecutionError> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| ExecutionError::Invalid("fixed field length"))
    }
    fn u8(&mut self) -> Result<u8, ExecutionError> {
        Ok(self.array::<1>()?[0])
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub(super) fn u16(&mut self) -> Result<u16, ExecutionError> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, ExecutionError> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    pub(super) fn u64(&mut self) -> Result<u64, ExecutionError> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    fn namespace(&mut self) -> Result<StateNamespaceV1, ExecutionError> {
        StateNamespaceV1::from_tag(self.u16()?).map_err(ExecutionError::Tree)
    }
    fn key(&mut self) -> Result<Vec<u8>, ExecutionError> {
        let len = usize::from(self.u16()?);
        if len > STATE_MAX_KEY_BYTES {
            return Err(ExecutionError::Invalid("key decode bound"));
        }
        Ok(self.bytes(len)?.to_vec())
    }
    fn optional(&mut self) -> Result<Option<[u8; 32]>, ExecutionError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.array()?)),
            _ => Err(ExecutionError::Invalid("optional digest tag")),
        }
    }
    fn roots(&mut self) -> Result<RootSetV1, ExecutionError> {
        let length = usize::from(self.u16()?);
        if length > 1024 {
            return Err(ExecutionError::Invalid("root-set byte bound"));
        }
        RootSetV1::from_canonical_bytes(self.bytes(length)?)
    }
    pub(super) fn finish(self) -> Result<(), ExecutionError> {
        if self.position != self.bytes.len() {
            return Err(ExecutionError::Invalid("object trailing bytes"));
        }
        Ok(())
    }
}

const MAGIC: &[u8] = b"oasis7.iws.transition.v1\0";
fn optional_len(value: &Option<[u8; 32]>) -> usize {
    if value.is_some() { 33 } else { 1 }
}
fn put_optional(bytes: &mut Vec<u8>, value: &Option<[u8; 32]>) {
    match value {
        None => bytes.push(0),
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(value);
        }
    }
}
fn put_key(bytes: &mut Vec<u8>, namespace: StateNamespaceV1, key: &[u8]) {
    bytes.extend_from_slice(&(namespace as u16).to_be_bytes());
    bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
    bytes.extend_from_slice(key);
}

impl TransitionRecordV1 {
    pub fn canonical_bytes(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        self.validate()?;
        let _scratch = memory.reserve(64 * 1024, class)?;
        let before = self.before.canonical_bytes()?;
        let after = self.after.canonical_bytes()?;
        let payload_len = 104
            + 2
            + before.len()
            + 2
            + after.len()
            + 6
            + self
                .reads
                .iter()
                .map(|read| 4 + read.key.len() + optional_len(&read.before))
                .sum::<usize>()
            + self
                .writes
                .iter()
                .map(|write| {
                    4 + write.mutation.key.len()
                        + optional_len(&write.mutation.before)
                        + optional_len(&write.mutation.after)
                })
                .sum::<usize>()
            + self.facts.len() * 33;
        let length = MAGIC.len() + 4 + payload_len;
        if length > EXECUTION_MAX_RECORD_BYTES {
            return Err(ExecutionError::Invalid("transition encoded byte bound"));
        }
        let charge = memory.reserve(length, class)?;
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(payload_len as u32).to_be_bytes());
        bytes.extend_from_slice(&self.parent_commit);
        bytes.extend_from_slice(&self.operation);
        bytes.extend_from_slice(&self.authority.token);
        bytes.extend_from_slice(&self.authority.epoch.to_be_bytes());
        for roots in [&before, &after] {
            bytes.extend_from_slice(&(roots.len() as u16).to_be_bytes());
            bytes.extend_from_slice(roots);
        }
        bytes.extend_from_slice(&(self.reads.len() as u16).to_be_bytes());
        for read in &self.reads {
            put_key(&mut bytes, read.namespace, &read.key);
            put_optional(&mut bytes, &read.before);
        }
        bytes.extend_from_slice(&(self.writes.len() as u16).to_be_bytes());
        for write in &self.writes {
            put_key(&mut bytes, write.namespace, &write.mutation.key);
            put_optional(&mut bytes, &write.mutation.before);
            put_optional(&mut bytes, &write.mutation.after);
        }
        bytes.extend_from_slice(&(self.facts.len() as u16).to_be_bytes());
        for fact in &self.facts {
            bytes.push(fact.kind as u8);
            bytes.extend_from_slice(&fact.id);
        }
        if bytes.len() != length || bytes.capacity() > charge.bytes() {
            return Err(ExecutionError::Invalid("transition encoding accounting"));
        }
        Ok(charge.retain(bytes))
    }

    /// Bound the complete object before decoding any owned sequence. Accepted
    /// records can live in a value tree; this is one bounded transaction only.
    pub fn from_canonical_bytes(
        bytes: &[u8],
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<ReservedValue<Self>, ExecutionError> {
        let mut cursor = Cursor::envelope(bytes, MAGIC, EXECUTION_MAX_RECORD_BYTES)?;
        let budget = bytes
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(64 * 1024))
            .ok_or(ExecutionError::ResourceLimited(
                "transition decode workspace",
            ))?;
        let mut workspace = memory.reserve(budget, class)?;
        let parent_commit = cursor.array()?;
        let operation = cursor.array()?;
        let authority = AuthorityBinding {
            token: cursor.array()?,
            epoch: cursor.u64()?,
        };
        let before = cursor.roots()?;
        let after = cursor.roots()?;
        let count = usize::from(cursor.u16()?);
        if count > EXECUTION_MAX_READS || count * 5 > cursor.remaining() {
            return Err(ExecutionError::Invalid("read decode count"));
        }
        workspace
            .merge(&mut memory.reserve(count * std::mem::size_of::<ExecutionReadV1>(), class)?)
            .map_err(|_| ExecutionError::Invalid("read decode reservation pool"))?;
        let mut reads = Vec::with_capacity(count);
        for _ in 0..count {
            reads.push(ExecutionReadV1 {
                namespace: cursor.namespace()?,
                key: cursor.key()?,
                before: cursor.optional()?,
            });
        }
        let count = usize::from(cursor.u16()?);
        if count > EXECUTION_MAX_WRITES || count * 6 > cursor.remaining() {
            return Err(ExecutionError::Invalid("write decode count"));
        }
        workspace
            .merge(&mut memory.reserve(count * std::mem::size_of::<ExecutionWriteV1>(), class)?)
            .map_err(|_| ExecutionError::Invalid("write decode reservation pool"))?;
        let mut writes = Vec::with_capacity(count);
        for _ in 0..count {
            let namespace = cursor.namespace()?;
            writes.push(ExecutionWriteV1 {
                namespace,
                mutation: StateWriteV1 {
                    key: cursor.key()?,
                    before: cursor.optional()?,
                    after: cursor.optional()?,
                },
            });
        }
        let count = usize::from(cursor.u16()?);
        if count > EXECUTION_MAX_FACTS || count * 33 > cursor.remaining() {
            return Err(ExecutionError::Invalid("fact decode count"));
        }
        workspace
            .merge(&mut memory.reserve(count * std::mem::size_of::<ExecutionFactRefV1>(), class)?)
            .map_err(|_| ExecutionError::Invalid("fact decode reservation pool"))?;
        let mut facts = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = match cursor.u8()? {
                1 => ExecutionFactKindV1::Input,
                2 => ExecutionFactKindV1::Result,
                3 => ExecutionFactKindV1::Event,
                _ => return Err(ExecutionError::Invalid("fact type tag")),
            };
            facts.push(ExecutionFactRefV1 {
                kind,
                id: cursor.array()?,
            });
        }
        cursor.finish()?;
        let record = Self {
            parent_commit,
            operation,
            authority,
            before,
            after,
            reads,
            writes,
            facts,
        };
        record.validate()?;
        let bytes = record.heap_bytes() + std::mem::size_of::<Self>() + 64;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("decode accounting"))?;
        Ok(charge.retain(record))
    }
}
