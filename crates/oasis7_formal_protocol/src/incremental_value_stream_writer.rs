//! Push serializer adapter; write boundaries never affect canonical identity.
use super::*;
use oasis7_proto::memory_budget::MemoryReservation;

/// Bounded, provisional value construction for serializers implementing Write.
/// No complete encoded field is retained. A failed write/flush poisons finish;
/// dropping this writer only leaves unreachable provisional pages at the sink.
pub struct ValueStreamWriter<'a, W, C> {
    sink: &'a mut W,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    cancelled: C,
    frontier: Vec<Vec<ValueNodeRefV1>>,
    buffer: Vec<u8>,
    length: u64,
    max_length: u64,
    failed: bool,
    error: Option<ValueTreeError>,
    _workspace: MemoryReservation,
}

impl<'a, W: ValueNodeWriter, C: FnMut() -> bool> ValueStreamWriter<'a, W, C> {
    pub fn new(
        sink: &'a mut W,
        memory: &'a MemoryBudget,
        io: &'a mut ValueIoBudget,
        max_length: u64,
        cancelled: C,
    ) -> Result<Self, ValueTreeError> {
        let workspace = memory.reserve(VALUE_BUILD_WORKSPACE_BYTES, MemoryClass::Ordinary)?;
        Ok(Self {
            sink,
            memory,
            io,
            cancelled,
            frontier: (0..=VALUE_MAX_DEPTH)
                .map(|_| Vec::with_capacity(VALUE_FANOUT))
                .collect(),
            buffer: Vec::with_capacity(VALUE_CHUNK_BYTES),
            length: 0,
            max_length,
            failed: false,
            error: None,
            _workspace: workspace,
        })
    }

    fn check(&mut self) -> Result<(), ValueTreeError> {
        if self.failed {
            return Err(self
                .error
                .take()
                .unwrap_or(ValueTreeError::Invalid("aborted value stream")));
        }
        self.failed = true;
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        self.failed = false;
        Ok(())
    }

    fn emit_chunk(&mut self) -> Result<(), ValueTreeError> {
        let charge = self.memory.reserve(
            codec::LEAF_OVERHEAD + self.buffer.len(),
            MemoryClass::Ordinary,
        )?;
        let id = persist(encode_leaf(&self.buffer), charge, self.sink, self.io)?;
        push(
            ValueNodeRefV1 {
                id,
                logical_len: self.buffer.len() as u64,
                depth: 0,
            },
            &mut self.frontier,
            self.sink,
            self.memory,
            self.io,
        )?;
        self.buffer.clear();
        Ok(())
    }

    fn write_inner(&mut self, mut bytes: &[u8]) -> Result<(), ValueTreeError> {
        let next = self
            .length
            .checked_add(bytes.len() as u64)
            .filter(|next| *next <= self.max_length)
            .ok_or(ValueTreeError::ResourceLimited("serialized value length"))?;
        if self.io.read_bytes_remaining < bytes.len() as u64 {
            return Err(ValueTreeError::ResourceLimited("input allowance"));
        }
        self.io.read_bytes_remaining -= bytes.len() as u64;
        self.length = next;
        while !bytes.is_empty() {
            if (self.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            let size = bytes.len().min(VALUE_CHUNK_BYTES - self.buffer.len());
            self.buffer.extend_from_slice(&bytes[..size]);
            bytes = &bytes[size..];
            if self.buffer.len() == VALUE_CHUNK_BYTES {
                self.emit_chunk()?;
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<ValueRootV1, ValueTreeError> {
        self.check()?;
        if !self.buffer.is_empty() || self.length == 0 {
            self.emit_chunk()?;
        }
        let reference = loop {
            if (self.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            let level = self
                .frontier
                .iter()
                .position(|level| !level.is_empty())
                .ok_or(ValueTreeError::Invalid("empty frontier"))?;
            if self.frontier.iter().map(Vec::len).sum::<usize>() == 1 {
                break self.frontier[level][0];
            }
            let children =
                std::mem::replace(&mut self.frontier[level], Vec::with_capacity(VALUE_FANOUT));
            let parent = make_parent(&children, self.sink, self.memory, self.io)?;
            push(parent, &mut self.frontier, self.sink, self.memory, self.io)?;
        };
        let root = ValueRootV1(reference);
        root.validate()?;
        let charge = self
            .memory
            .reserve(codec::ROOT_BYTES, MemoryClass::Ordinary)?;
        persist(root.canonical_bytes()?, charge, self.sink, self.io)?;
        Ok(root)
    }
}

impl<W: ValueNodeWriter, C: FnMut() -> bool> Write for ValueStreamWriter<'_, W, C> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Err(error) = self.check() {
            self.failed = true;
            self.error = Some(error);
            return Err(std::io::Error::other("value stream write rejected"));
        }
        // Preserve poison if an upstream caller catches a sink/cancel panic.
        self.failed = true;
        match self.write_inner(bytes) {
            Ok(()) => {
                self.failed = false;
                Ok(bytes.len())
            }
            Err(error) => {
                self.error = Some(error);
                Err(std::io::Error::other(
                    "value stream write failed; finish retains cause",
                ))
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Flush does not emit a short leaf: chunk identity is independent of
        // serializer flush frequency. Only finish publishes the descriptor.
        if let Err(error) = self.check() {
            self.failed = true;
            self.error = Some(error);
            return Err(std::io::Error::other("value stream flush failed"));
        }
        Ok(())
    }
}
