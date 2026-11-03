use super::*;
const MAGIC: &[u8] = b"oasis7.iws.page-recipe.v1\0";
pub(super) const MAX_FRAMES: usize = 17;
pub(super) const MAX_RECIPE_BYTES: usize = 2048;

pub(super) fn encode(id: &[u8; 32], length: usize, frames: &[FrameDescriptor]) -> Result<Vec<u8>> {
    validate(length, frames)?;
    let mut bytes = Vec::with_capacity(MAX_RECIPE_BYTES);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes.push(frames.len() as u8);
    for frame in frames {
        bytes.extend_from_slice(&frame.stored);
        bytes.extend_from_slice(&frame.original);
        bytes.extend_from_slice(&frame.length.to_be_bytes());
        bytes.push(u8::from(frame.compressed));
    }
    let hash = blake3::hash(&bytes);
    bytes.extend_from_slice(hash.as_bytes());
    Ok(bytes)
}
pub(super) fn decode(id: &[u8; 32], bytes: &[u8]) -> Result<(usize, Vec<FrameDescriptor>)> {
    let bad = || PageStoreError::Corrupt("page recipe domain/length/checksum");
    let header = MAGIC.len() + 32 + 4 + 1;
    if bytes.len() < header + 32 || bytes.len() > MAX_RECIPE_BYTES || !bytes.starts_with(MAGIC) {
        return Err(bad());
    }
    let data = bytes.len() - 32;
    if blake3::hash(&bytes[..data]).as_bytes() != &bytes[data..]
        || &bytes[MAGIC.len()..MAGIC.len() + 32] != id
    {
        return Err(bad());
    }
    let length = u32::from_be_bytes(
        bytes[MAGIC.len() + 32..MAGIC.len() + 36]
            .try_into()
            .map_err(|_| bad())?,
    ) as usize;
    let count = bytes[header - 1] as usize;
    if count > MAX_FRAMES || bytes.len() != header + count * 69 + 32 {
        return Err(bad());
    }
    let mut frames = Vec::with_capacity(count);
    for frame in bytes[header..data].chunks_exact(69) {
        if frame[68] > 1 {
            return Err(bad());
        }
        frames.push(FrameDescriptor {
            stored: frame[..32].try_into().map_err(|_| bad())?,
            original: frame[32..64].try_into().map_err(|_| bad())?,
            length: u32::from_be_bytes(frame[64..68].try_into().map_err(|_| bad())?),
            compressed: frame[68] == 1,
        });
    }
    validate(length, &frames)?;
    Ok((length, frames))
}
fn validate(length: usize, frames: &[FrameDescriptor]) -> Result<()> {
    if length == 0
        || length > VALUE_MAX_NODE_BYTES
        || frames.is_empty()
        || frames.len() > MAX_FRAMES
        || frames
            .iter()
            .any(|f| f.length == 0 || f.length as usize > frame_codec::FRAME_BYTES)
        || frames.iter().map(|f| f.length as usize).sum::<usize>() != length
    {
        return Err(PageStoreError::Corrupt("page recipe frame coverage"));
    }
    Ok(())
}
