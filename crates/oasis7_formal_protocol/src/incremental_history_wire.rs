use super::*;

pub(super) struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    pub(super) fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub(super) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(super) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(super) fn hash(&mut self, value: &[u8; 32]) {
        self.bytes.extend_from_slice(value);
    }
    pub(super) fn string(&mut self, value: &str) {
        self.u16(value.len() as u16);
        self.bytes.extend_from_slice(value.as_bytes());
    }
    pub(super) fn scope(&mut self, scope: &WorldScopeV1) {
        self.string(&scope.world_id);
        self.hash(&scope.genesis_config);
    }
    pub(super) fn optional(&mut self, value: &Option<[u8; 32]>) {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.hash(value);
            }
        }
    }
    pub(super) fn checkpoint(&mut self, value: &Option<CheckpointPointerV1>) {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.u64(value.height);
                self.hash(&value.descriptor);
            }
        }
    }
    pub(super) fn nested(&mut self, bytes: &[u8]) {
        self.u16(bytes.len() as u16);
        self.bytes.extend_from_slice(bytes);
    }
    pub(super) fn finish(self, magic: &[u8], max: usize) -> Result<Vec<u8>, ExecutionError> {
        let length = magic.len() + 4 + self.bytes.len();
        if length > max {
            return Err(ExecutionError::Invalid("history encoded byte bound"));
        }
        let mut result = Vec::with_capacity(length);
        result.extend_from_slice(magic);
        result.extend_from_slice(&(self.bytes.len() as u32).to_be_bytes());
        result.extend(self.bytes);
        Ok(result)
    }
    pub(super) fn query(&mut self, query: CatalogQueryV1) {
        match query {
            CatalogQueryV1::Height(height) => {
                self.u8(1);
                self.u64(height);
            }
            CatalogQueryV1::Commit(commit) => {
                self.u8(2);
                self.hash(&commit);
            }
        }
    }
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    next: usize,
}
impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8], magic: &[u8], max: usize) -> Result<Self, ExecutionError> {
        if bytes.len() > max || !bytes.starts_with(magic) {
            return Err(ExecutionError::Invalid("history domain/byte bound"));
        }
        let header = bytes
            .get(magic.len()..magic.len() + 4)
            .ok_or(ExecutionError::Invalid("history header truncated"))?;
        let length = u32::from_be_bytes(
            header
                .try_into()
                .map_err(|_| ExecutionError::Invalid("history length"))?,
        ) as usize;
        if bytes.len() - magic.len() - 4 != length {
            return Err(ExecutionError::Invalid("history declared length"));
        }
        Ok(Self {
            bytes,
            next: magic.len() + 4,
        })
    }
    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], ExecutionError> {
        let end = self
            .next
            .checked_add(length)
            .ok_or(ExecutionError::Invalid("history length overflow"))?;
        let bytes = self
            .bytes
            .get(self.next..end)
            .ok_or(ExecutionError::Invalid("history field truncated"))?;
        self.next = end;
        Ok(bytes)
    }
    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], ExecutionError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ExecutionError::Invalid("history array width"))
    }
    pub(super) fn u8(&mut self) -> Result<u8, ExecutionError> {
        Ok(self.array::<1>()?[0])
    }
    pub(super) fn u16(&mut self) -> Result<u16, ExecutionError> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    pub(super) fn u64(&mut self) -> Result<u64, ExecutionError> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    pub(super) fn string(&mut self) -> Result<String, ExecutionError> {
        let length = usize::from(self.u16()?);
        if length == 0 || length > 256 {
            return Err(ExecutionError::Invalid("history string bound"));
        }
        Ok(std::str::from_utf8(self.take(length)?)
            .map_err(|_| ExecutionError::Invalid("history UTF-8"))?
            .to_owned())
    }
    pub(super) fn scope(&mut self) -> Result<WorldScopeV1, ExecutionError> {
        Ok(WorldScopeV1 {
            world_id: self.string()?,
            genesis_config: self.array()?,
        })
    }
    pub(super) fn optional(&mut self) -> Result<Option<[u8; 32]>, ExecutionError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.array()?)),
            _ => Err(ExecutionError::Invalid("history optional tag")),
        }
    }
    pub(super) fn checkpoint(&mut self) -> Result<Option<CheckpointPointerV1>, ExecutionError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(CheckpointPointerV1 {
                height: self.u64()?,
                descriptor: self.array()?,
            })),
            _ => Err(ExecutionError::Invalid("checkpoint pointer tag")),
        }
    }
    pub(super) fn nested(&mut self) -> Result<&'a [u8], ExecutionError> {
        let length = usize::from(self.u16()?);
        if length > HISTORY_MAX_HEADER_BYTES {
            return Err(ExecutionError::Invalid("nested history bound"));
        }
        self.take(length)
    }
    pub(super) fn finish(self) -> Result<(), ExecutionError> {
        if self.next != self.bytes.len() {
            return Err(ExecutionError::Invalid("history trailing bytes"));
        }
        Ok(())
    }
    pub(super) fn query(&mut self) -> Result<CatalogQueryV1, ExecutionError> {
        match self.u8()? {
            1 => Ok(CatalogQueryV1::Height(self.u64()?)),
            2 => Ok(CatalogQueryV1::Commit(self.array()?)),
            _ => Err(ExecutionError::Invalid("catalog query tag")),
        }
    }
}

pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}
pub(super) fn parse_hash(value: &str) -> Result<[u8; 32], ExecutionError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ExecutionError::Invalid("catalog digest encoding"));
    }
    hex::decode(value)
        .map_err(|_| ExecutionError::Invalid("catalog digest"))?
        .try_into()
        .map_err(|_| ExecutionError::Invalid("catalog digest width"))
}
