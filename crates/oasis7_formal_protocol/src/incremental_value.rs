//! Logical value trees for IWS. Fixed chunks and fanout define identity; the
//! physical store may compress or pack their unchanged canonical bytes.
//!
//! Input/output adapters own their own charged buffers. These functions reserve
//! their workspace before allocation; retained nodes carry their charge to the
//! sink or last reader. Publishing a tree grants no world authority/durability.
use std::io::{Read, Write};
use std::sync::Arc;

use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded, ReservedValue};

#[path = "incremental_value_codec.rs"]
mod codec;
#[path = "incremental_value_stream_writer.rs"]
mod stream_writer;
use codec::{Node, decode_node, encode_index, encode_leaf, expected_size, span};
pub use stream_writer::ValueStreamWriter;

pub const VALUE_CHUNK_BYTES: usize = 64 * 1024;
pub const VALUE_FANOUT: usize = 128;
pub const VALUE_MAX_DEPTH: u8 = 7;
pub const VALUE_MAX_NODE_BYTES: usize = VALUE_CHUNK_BYTES + 64;
pub const VALUE_BUILD_WORKSPACE_BYTES: usize = 512 * 1024;
pub const VALUE_READ_WORKSPACE_BYTES: usize = 128 * 1024;

#[derive(Debug)]
pub enum ValueTreeError {
    Invalid(&'static str),
    ResourceLimited(&'static str),
    Memory(MemoryLimitExceeded),
    Io(std::io::Error),
    Cancelled,
}

impl std::fmt::Display for ValueTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid value tree: {reason}"),
            Self::ResourceLimited(reason) => write!(f, "value tree resource limited: {reason}"),
            Self::Memory(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Cancelled => f.write_str("value tree operation cancelled"),
        }
    }
}
impl std::error::Error for ValueTreeError {}
impl From<std::io::Error> for ValueTreeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<MemoryLimitExceeded> for ValueTreeError {
    fn from(error: MemoryLimitExceeded) -> Self {
        Self::Memory(error)
    }
}

pub trait ValueNodeWriter {
    /// Keep the charge if retaining bytes. Reserve any sink metadata before
    /// allocation. This is provisional storage, never a commit/head publish.
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError>;
}

pub trait ValueNodeReader {
    /// Reserve from `memory` before IO/allocation, validate the encoded length
    /// against `max_bytes`, and keep this charge with all retained reader Arcs.
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError>;
}

/// A child reference is authenticated by its parent's original node bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueNodeRefV1 {
    pub id: [u8; 32],
    pub logical_len: u64,
    pub depth: u8,
}

/// The descriptor's digest is the value reference stored in a state leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueRootV1(pub ValueNodeRefV1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuePageLinksV1 {
    Root(ValueNodeRefV1),
    Index(Vec<ValueNodeRefV1>),
    Data { logical_len: u64 },
}

/// Expose typed edges for closure verification/GC without materializing a
/// value. Callers reserve a bounded index workspace before invoking this.
pub fn verified_value_links(
    id: &[u8; 32],
    bytes: &[u8],
) -> Result<ValuePageLinksV1, ValueTreeError> {
    if bytes.len() > VALUE_MAX_NODE_BYTES || blake3::hash(bytes).as_bytes() != id {
        return Err(ValueTreeError::Invalid("value page size/content hash"));
    }
    if bytes.starts_with(b"oasis7.iws.value-root.v1\0") {
        return Ok(ValuePageLinksV1::Root(
            ValueRootV1::from_canonical_bytes(bytes)?.0,
        ));
    }
    let reference = codec::reference_from_bytes(*id, bytes)?;
    match decode_node(bytes, &reference)? {
        Node::Leaf(data) => Ok(ValuePageLinksV1::Data {
            logical_len: data.len() as u64,
        }),
        Node::Index(children) => Ok(ValuePageLinksV1::Index(children)),
    }
}

/// Operation allowances are separate from process memory and logical layout.
/// They are consumed before the corresponding IO; errors retain spent work.
#[derive(Debug, Clone)]
pub struct ValueIoBudget {
    pub nodes_remaining: u64,
    pub read_bytes_remaining: u64,
    pub write_bytes_remaining: u64,
}

pub struct ValueReadContext<'a, R> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: &'a mut ValueIoBudget,
}

impl ValueIoBudget {
    fn node(&mut self, read: u64, write: u64) -> Result<(), ValueTreeError> {
        if self.nodes_remaining == 0
            || read > self.read_bytes_remaining
            || write > self.write_bytes_remaining
        {
            return Err(ValueTreeError::ResourceLimited("node/IO allowance"));
        }
        self.nodes_remaining -= 1;
        self.read_bytes_remaining -= read;
        self.write_bytes_remaining -= write;
        Ok(())
    }
}

impl ValueRootV1 {
    pub fn validate(&self) -> Result<(), ValueTreeError> {
        let reference = self.0;
        if reference.depth > VALUE_MAX_DEPTH
            || u128::from(reference.logical_len) > span(reference.depth)
            || (reference.depth > 0
                && u128::from(reference.logical_len) <= span(reference.depth - 1))
        {
            return Err(ValueTreeError::Invalid("root depth/length"));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ValueTreeError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(codec::ROOT_BYTES);
        bytes.extend_from_slice(b"oasis7.iws.value-root.v1\0");
        bytes.extend_from_slice(&41u32.to_be_bytes());
        bytes.extend_from_slice(&self.0.id);
        bytes.extend_from_slice(&self.0.logical_len.to_be_bytes());
        bytes.push(self.0.depth);
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ValueTreeError> {
        let magic = b"oasis7.iws.value-root.v1\0";
        if !bytes.starts_with(magic) || bytes.len() != magic.len() + 45 {
            return Err(ValueTreeError::Invalid("root domain/length"));
        }
        let body = &bytes[magic.len()..];
        if body[..4] != 41u32.to_be_bytes() {
            return Err(ValueTreeError::Invalid("root declared length"));
        }
        let root = Self(ValueNodeRefV1 {
            id: body[4..36]
                .try_into()
                .map_err(|_| ValueTreeError::Invalid("root ID"))?,
            logical_len: u64::from_be_bytes(
                body[36..44]
                    .try_into()
                    .map_err(|_| ValueTreeError::Invalid("root length"))?,
            ),
            depth: body[44],
        });
        root.validate()?;
        Ok(root)
    }

    pub fn digest(&self) -> Result<[u8; 32], ValueTreeError> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }
}

fn persist(
    bytes: Vec<u8>,
    charge: oasis7_proto::memory_budget::MemoryReservation,
    sink: &mut impl ValueNodeWriter,
    io: &mut ValueIoBudget,
) -> Result<[u8; 32], ValueTreeError> {
    io.node(0, bytes.len() as u64)?;
    if bytes.capacity() > charge.bytes() {
        return Err(ValueTreeError::Invalid(
            "node allocation exceeds reservation",
        ));
    }
    let id = *blake3::hash(&bytes).as_bytes();
    sink.put_node(id, charge.retain(bytes))?;
    Ok(id)
}

/// Consume exactly `logical_len` bytes from the input stream. Following bytes
/// belong to the caller's next object and are neither consumed nor hashed.
pub fn write_value(
    input: &mut impl Read,
    logical_len: u64,
    sink: &mut impl ValueNodeWriter,
    memory: &MemoryBudget,
    io: &mut ValueIoBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ValueRootV1, ValueTreeError> {
    let _workspace = memory.reserve(VALUE_BUILD_WORKSPACE_BYTES, MemoryClass::Ordinary)?;
    let mut frontier: Vec<Vec<ValueNodeRefV1>> = (0..=VALUE_MAX_DEPTH)
        .map(|_| Vec::with_capacity(VALUE_FANOUT))
        .collect();
    let mut buffer = vec![0; VALUE_CHUNK_BYTES];
    let mut remaining = logical_len;
    loop {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let size = remaining.min(VALUE_CHUNK_BYTES as u64) as usize;
        if io.read_bytes_remaining < size as u64 {
            return Err(ValueTreeError::ResourceLimited("input allowance"));
        }
        io.read_bytes_remaining -= size as u64;
        input.read_exact(&mut buffer[..size])?;
        let charge = memory.reserve(codec::LEAF_OVERHEAD + size, MemoryClass::Ordinary)?;
        let bytes = encode_leaf(&buffer[..size]);
        let id = persist(bytes, charge, sink, io)?;
        push(
            ValueNodeRefV1 {
                id,
                logical_len: size as u64,
                depth: 0,
            },
            &mut frontier,
            sink,
            memory,
            io,
        )?;
        remaining -= size as u64;
        if remaining == 0 {
            break;
        }
    }
    let root = loop {
        let level = frontier
            .iter()
            .position(|level| !level.is_empty())
            .ok_or(ValueTreeError::Invalid("empty frontier"))?;
        let count: usize = frontier.iter().map(Vec::len).sum();
        if count == 1 {
            break frontier[level][0];
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let children = std::mem::replace(&mut frontier[level], Vec::with_capacity(VALUE_FANOUT));
        let parent = make_parent(&children, sink, memory, io)?;
        push(parent, &mut frontier, sink, memory, io)?;
    };
    let root = ValueRootV1(root);
    root.validate()?;
    let charge = memory.reserve(codec::ROOT_BYTES, MemoryClass::Ordinary)?;
    persist(root.canonical_bytes()?, charge, sink, io)?;
    Ok(root)
}

fn make_parent(
    children: &[ValueNodeRefV1],
    sink: &mut impl ValueNodeWriter,
    memory: &MemoryBudget,
    io: &mut ValueIoBudget,
) -> Result<ValueNodeRefV1, ValueTreeError> {
    let logical_len = children
        .iter()
        .try_fold(0u64, |sum, child| sum.checked_add(child.logical_len))
        .ok_or(ValueTreeError::Invalid("logical length overflow"))?;
    let depth = children[0]
        .depth
        .checked_add(1)
        .filter(|depth| *depth <= VALUE_MAX_DEPTH)
        .ok_or(ValueTreeError::Invalid("index depth overflow"))?;
    let charge = memory.reserve(
        codec::INDEX_OVERHEAD + children.len() * 40,
        MemoryClass::Ordinary,
    )?;
    let bytes = encode_index(depth, logical_len, children)?;
    let id = persist(bytes, charge, sink, io)?;
    Ok(ValueNodeRefV1 {
        id,
        logical_len,
        depth,
    })
}

fn push(
    mut reference: ValueNodeRefV1,
    frontier: &mut [Vec<ValueNodeRefV1>],
    sink: &mut impl ValueNodeWriter,
    memory: &MemoryBudget,
    io: &mut ValueIoBudget,
) -> Result<(), ValueTreeError> {
    loop {
        let level = usize::from(reference.depth);
        frontier[level].push(reference);
        if frontier[level].len() < VALUE_FANOUT {
            return Ok(());
        }
        reference = make_parent(&frontier[level], sink, memory, io)?;
        frontier[level].clear();
    }
}

/// Verify only the index path(s) and chunks intersecting the authorized range.
/// Output remains provisional until success; cancellation/failure is explicit.
impl<R: ValueNodeReader> ValueReadContext<'_, R> {
    pub fn load_root(&mut self, expected_id: &[u8; 32]) -> Result<ValueRootV1, ValueTreeError> {
        self.io.node(codec::ROOT_BYTES as u64, 0)?;
        let bytes =
            self.source
                .read_node(expected_id, codec::ROOT_BYTES, self.memory, self.class)?;
        if !bytes.uses_budget(self.memory)
            || bytes.reserved_bytes() < bytes.capacity()
            || bytes.len() != codec::ROOT_BYTES
            || blake3::hash(&bytes).as_bytes() != expected_id
        {
            return Err(ValueTreeError::Invalid("uncharged or wrong value root"));
        }
        ValueRootV1::from_canonical_bytes(&bytes)
    }

    pub fn read_range(
        &mut self,
        root: &ValueRootV1,
        offset: u64,
        length: u64,
        output: &mut impl Write,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<u64, ValueTreeError> {
        let source = self.source;
        let memory = self.memory;
        let class = self.class;
        let io = &mut *self.io;
        root.validate()?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= root.0.logical_len)
            .ok_or(ValueTreeError::Invalid("range outside value"))?;
        let _workspace = memory.reserve(VALUE_READ_WORKSPACE_BYTES, class)?;
        if length == 0 {
            return Ok(0);
        }
        let mut stack = vec![(root.0, 0u64)];
        let mut written = 0;
        while let Some((reference, start)) = stack.pop() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            let size = expected_size(&reference)?;
            io.node(size as u64, 0)?;
            let bytes = source.read_node(&reference.id, size, memory, class)?;
            if !bytes.uses_budget(memory)
                || bytes.reserved_bytes() < bytes.capacity()
                || bytes.len() != size
                || blake3::hash(&bytes).as_bytes() != &reference.id
            {
                return Err(ValueTreeError::Invalid("uncharged or wrong value node"));
            }
            match decode_node(&bytes, &reference)? {
                Node::Leaf(data) => {
                    let from = offset.saturating_sub(start) as usize;
                    let to = (end - start).min(data.len() as u64) as usize;
                    if io.write_bytes_remaining < (to - from) as u64 {
                        return Err(ValueTreeError::ResourceLimited("range output allowance"));
                    }
                    io.write_bytes_remaining -= (to - from) as u64;
                    output.write_all(&data[from..to])?;
                    written += (to - from) as u64;
                }
                Node::Index(children) => {
                    let mut position = start;
                    let mut selected = Vec::with_capacity(children.len());
                    for child in children {
                        let child_end = position
                            .checked_add(child.logical_len)
                            .ok_or(ValueTreeError::Invalid("range overflow"))?;
                        if position < end && child_end > offset {
                            selected.push((child, position));
                        }
                        position = child_end;
                    }
                    stack.extend(selected.into_iter().rev());
                }
            }
        }
        if written != length {
            return Err(ValueTreeError::Invalid("incomplete range"));
        }
        Ok(written)
    }
}

#[cfg(test)]
#[path = "incremental_value_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "incremental_value_streaming_tests.rs"]
mod streaming_tests;
