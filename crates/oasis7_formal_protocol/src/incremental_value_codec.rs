use super::*;

const MAGIC: &[u8] = b"oasis7.iws.value-tree.v1\0";
pub(super) const ROOT_BYTES: usize = b"oasis7.iws.value-root.v1\0".len() + 45;
pub(super) const LEAF_OVERHEAD: usize = MAGIC.len() + 5;
pub(super) const INDEX_OVERHEAD: usize = MAGIC.len() + 16;

pub(super) enum Node<'a> {
    Leaf(&'a [u8]),
    Index(Vec<ValueNodeRefV1>),
}

pub(super) fn span(depth: u8) -> u128 {
    (VALUE_CHUNK_BYTES as u128) * (VALUE_FANOUT as u128).pow(u32::from(depth))
}

pub(super) fn expected_size(reference: &ValueNodeRefV1) -> Result<usize, ValueTreeError> {
    if reference.depth > VALUE_MAX_DEPTH
        || u128::from(reference.logical_len) > span(reference.depth)
    {
        return Err(ValueTreeError::Invalid("node depth/length"));
    }
    if reference.depth == 0 {
        return Ok(LEAF_OVERHEAD + reference.logical_len as usize);
    }
    if reference.logical_len == 0 {
        return Err(ValueTreeError::Invalid("empty index"));
    }
    let count = u128::from(reference.logical_len).div_ceil(span(reference.depth - 1));
    Ok(INDEX_OVERHEAD + count as usize * 40)
}

fn header(payload_len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MAGIC.len() + 4 + payload_len);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(payload_len as u32).to_be_bytes());
    bytes
}

pub(super) fn encode_leaf(data: &[u8]) -> Vec<u8> {
    let mut bytes = header(1 + data.len());
    bytes.push(0);
    bytes.extend_from_slice(data);
    bytes
}

pub(super) fn encode_index(
    depth: u8,
    logical_len: u64,
    children: &[ValueNodeRefV1],
) -> Result<Vec<u8>, ValueTreeError> {
    validate_children(depth, logical_len, children)?;
    let mut bytes = header(12 + children.len() * 40);
    bytes.push(1);
    bytes.push(depth);
    bytes.extend_from_slice(&logical_len.to_be_bytes());
    bytes.extend_from_slice(&(children.len() as u16).to_be_bytes());
    for child in children {
        bytes.extend_from_slice(&child.logical_len.to_be_bytes());
        bytes.extend_from_slice(&child.id);
    }
    Ok(bytes)
}

fn validate_children(
    depth: u8,
    logical_len: u64,
    children: &[ValueNodeRefV1],
) -> Result<(), ValueTreeError> {
    if depth == 0 || depth > VALUE_MAX_DEPTH || children.is_empty() || children.len() > VALUE_FANOUT
    {
        return Err(ValueTreeError::Invalid("index depth/count"));
    }
    let width = span(depth - 1);
    let mut total = 0u64;
    for (index, child) in children.iter().enumerate() {
        if child.depth != depth - 1
            || child.logical_len == 0
            || u128::from(child.logical_len) > width
            || (index + 1 < children.len() && u128::from(child.logical_len) != width)
        {
            return Err(ValueTreeError::Invalid("index child boundary"));
        }
        total = total
            .checked_add(child.logical_len)
            .ok_or(ValueTreeError::Invalid("index length overflow"))?;
    }
    if total != logical_len {
        return Err(ValueTreeError::Invalid("index total length"));
    }
    Ok(())
}

pub(super) fn decode_node<'a>(
    bytes: &'a [u8],
    reference: &ValueNodeRefV1,
) -> Result<Node<'a>, ValueTreeError> {
    if bytes.len() != expected_size(reference)? || !bytes.starts_with(MAGIC) {
        return Err(ValueTreeError::Invalid("node domain/size"));
    }
    let body = &bytes[MAGIC.len()..];
    let declared = u32::from_be_bytes(
        body[..4]
            .try_into()
            .map_err(|_| ValueTreeError::Invalid("node header"))?,
    ) as usize;
    let payload = &body[4..];
    if declared != payload.len() {
        return Err(ValueTreeError::Invalid("node declared length"));
    }
    if reference.depth == 0 {
        if payload.first() != Some(&0) {
            return Err(ValueTreeError::Invalid("leaf tag"));
        }
        return Ok(Node::Leaf(&payload[1..]));
    }
    if payload.len() < 12 || payload[0] != 1 || payload[1] != reference.depth {
        return Err(ValueTreeError::Invalid("index tag/depth"));
    }
    let total = u64::from_be_bytes(
        payload[2..10]
            .try_into()
            .map_err(|_| ValueTreeError::Invalid("index length"))?,
    );
    let count = usize::from(u16::from_be_bytes([payload[10], payload[11]]));
    if count == 0
        || count > VALUE_FANOUT
        || payload.len() != 12 + count * 40
        || total != reference.logical_len
    {
        return Err(ValueTreeError::Invalid("index count/length"));
    }
    let mut children = Vec::with_capacity(count);
    for entry in payload[12..].chunks_exact(40) {
        children.push(ValueNodeRefV1 {
            logical_len: u64::from_be_bytes(
                entry[..8]
                    .try_into()
                    .map_err(|_| ValueTreeError::Invalid("child length"))?,
            ),
            id: entry[8..]
                .try_into()
                .map_err(|_| ValueTreeError::Invalid("child ID"))?,
            depth: reference.depth - 1,
        });
    }
    validate_children(reference.depth, reference.logical_len, &children)?;
    Ok(Node::Index(children))
}

pub(super) fn reference_from_bytes(
    id: [u8; 32],
    bytes: &[u8],
) -> Result<ValueNodeRefV1, ValueTreeError> {
    if bytes.len() > VALUE_MAX_NODE_BYTES
        || !bytes.starts_with(MAGIC)
        || bytes.len() < LEAF_OVERHEAD
    {
        return Err(ValueTreeError::Invalid("node metadata domain/length"));
    }
    let payload = &bytes[MAGIC.len() + 4..];
    let (depth, logical_len) = match payload.first() {
        Some(0) => (0, (payload.len() - 1) as u64),
        Some(1) if payload.len() >= 12 => (
            payload[1],
            u64::from_be_bytes(
                payload[2..10]
                    .try_into()
                    .map_err(|_| ValueTreeError::Invalid("node metadata length"))?,
            ),
        ),
        _ => return Err(ValueTreeError::Invalid("node metadata tag")),
    };
    Ok(ValueNodeRefV1 {
        id,
        logical_len,
        depth,
    })
}
