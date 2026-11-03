use super::*;
use blake3::hazmat::{HasherExt, Mode, merge_subtrees_non_root, merge_subtrees_root};

impl Row {
    pub(super) fn append<C: FnMut() -> bool>(
        &mut self,
        mut bytes: &[u8],
        work: &mut CognitionHeadHashWorkV1,
        cancelled: &mut C,
    ) -> Result<(), ValueTreeError> {
        while !bytes.is_empty() {
            check(cancelled)?;
            let prefix = if self.complete == 0 {
                usize::from(self.prefix_len)
            } else {
                0
            };
            let remaining = CHUNK - prefix - usize::from(self.tail_len);
            let take = remaining.min(bytes.len());
            let offset = usize::from(self.tail_len);
            self.tail[offset..offset + take].copy_from_slice(&bytes[..take]);
            self.tail_len += take as u16;
            bytes = &bytes[take..];
            if take == remaining {
                if self.complete != 0 {
                    work.node()?;
                    let cv = chunk(self.complete, &self.tail)?;
                    push(
                        &mut self.spans,
                        &mut self.used,
                        Span {
                            start: self.complete,
                            height: 0,
                            cv,
                        },
                        work,
                    )?;
                }
                self.complete = self.complete.checked_add(1).ok_or_else(invalid)?;
                self.tail_len = 0;
            }
            check(cancelled)?;
        }
        Ok(())
    }

    pub(super) fn validate(&self, body_bytes: u64) -> Result<(), ValueTreeError> {
        let total = body_bytes
            .checked_add(u64::from(self.prefix_len))
            .ok_or_else(invalid)?;
        if self.complete != total / CHUNK as u64
            || usize::from(self.tail_len)
                != if self.complete == 0 {
                    body_bytes as usize
                } else {
                    (total % CHUNK as u64) as usize
                }
            || usize::from(self.used) > SPANS
        {
            return Err(invalid());
        }
        let mut next = 1;
        for span in &self.spans[..usize::from(self.used)] {
            let width = 1_u64
                .checked_shl(u32::from(span.height))
                .ok_or_else(invalid)?;
            if span.start != next || span.start % width != 0 {
                return Err(invalid());
            }
            next = next.checked_add(width).ok_or_else(invalid)?;
        }
        if next != self.complete.max(1) {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn digest<C: FnMut() -> bool>(
        &self,
        first: &[u8; CHUNK],
        body_bytes: u64,
        header: &alignment::Header,
        work: &mut CognitionHeadHashWorkV1,
        cancelled: &mut C,
    ) -> Result<[u8; 32], ValueTreeError> {
        self.validate(body_bytes)?;
        check(cancelled)?;
        let total = body_bytes
            .checked_add(header.prefix.len() as u64)
            .and_then(|v| v.checked_add(header.suffix.len() as u64))
            .ok_or_else(invalid)?;
        if total <= CHUNK as u64 {
            work.node()?;
            let mut hasher = blake3::Hasher::new();
            hasher
                .update(&header.prefix)
                .update(&first[..body_bytes as usize])
                .update(&header.suffix);
            check(cancelled)?;
            return Ok(*hasher.finalize().as_bytes());
        }
        // All cached nodes cover [1,complete), with their ORIGINAL absolute
        // counters/alignment. Chunk zero is rebuilt from the changing header.
        let mut spans = [Span::EMPTY; SPANS];
        let mut used = 0;
        let mut latest_root = None;
        let mut first_chunk = [0_u8; CHUNK];
        let prefix = header.prefix.len();
        let mut end = [0_u8; CHUNK + HEADER_MAX];
        let mut tail_len = usize::from(self.tail_len);
        // If the body alone does not fill chunk zero, the suffix is what
        // crosses 1024: rebuild that complete first chunk directly instead.
        if self.complete == 0 {
            first_chunk[..prefix].copy_from_slice(&header.prefix);
            first_chunk[prefix..prefix + body_bytes as usize]
                .copy_from_slice(&first[..body_bytes as usize]);
            let fitting = CHUNK - prefix - body_bytes as usize;
            first_chunk[CHUNK - fitting..].copy_from_slice(&header.suffix[..fitting]);
            work.node()?;
            push_root(
                &mut spans,
                &mut used,
                Span {
                    start: 0,
                    height: 0,
                    cv: chunk(0, &first_chunk)?,
                },
                &mut latest_root,
                work,
            )?;
            end[..header.suffix.len() - fitting].copy_from_slice(&header.suffix[fitting..]);
            tail_len = header.suffix.len() - fitting;
        } else {
            first_chunk[..prefix].copy_from_slice(&header.prefix);
            first_chunk[prefix..].copy_from_slice(&first[..CHUNK - prefix]);
            work.node()?;
            push_root(
                &mut spans,
                &mut used,
                Span {
                    start: 0,
                    height: 0,
                    cv: chunk(0, &first_chunk)?,
                },
                &mut latest_root,
                work,
            )?;
            for span in &self.spans[..usize::from(self.used)] {
                push_root(&mut spans, &mut used, *span, &mut latest_root, work)?;
            }
            end[..tail_len].copy_from_slice(&self.tail[..tail_len]);
            end[tail_len..tail_len + header.suffix.len()].copy_from_slice(&header.suffix);
            tail_len += header.suffix.len();
        }
        let start = self.complete.max(1);
        for (offset, bytes) in end[..tail_len].chunks(CHUNK).enumerate() {
            check(cancelled)?;
            work.node()?;
            let index = start.checked_add(offset as u64).ok_or_else(invalid)?;
            push_root(
                &mut spans,
                &mut used,
                Span {
                    start: index,
                    height: 0,
                    cv: chunk(index, bytes)?,
                },
                &mut latest_root,
                work,
            )?;
        }
        check(cancelled)?;
        if used == 1 {
            return latest_root.ok_or_else(invalid);
        }
        let mut right = spans[usize::from(used) - 1].cv;
        for index in (0..usize::from(used) - 1).rev() {
            check(cancelled)?;
            work.node()?;
            if index == 0 {
                return Ok(*merge_subtrees_root(&spans[0].cv, &right, Mode::Hash).as_bytes());
            }
            right = merge_subtrees_non_root(&spans[index].cv, &right, Mode::Hash);
        }
        Err(invalid())
    }
}
fn chunk(index: u64, bytes: &[u8]) -> Result<[u8; 32], ValueTreeError> {
    let mut hasher = blake3::Hasher::new();
    hasher.set_input_offset(index.checked_mul(CHUNK as u64).ok_or_else(invalid)?);
    hasher.update(bytes);
    Ok(hasher.finalize_non_root())
}
fn push(
    spans: &mut [Span; SPANS],
    used: &mut u8,
    span: Span,
    work: &mut CognitionHeadHashWorkV1,
) -> Result<(), ValueTreeError> {
    push_root(spans, used, span, &mut None, work)
}
fn push_root(
    spans: &mut [Span; SPANS],
    used: &mut u8,
    mut span: Span,
    root: &mut Option<[u8; 32]>,
    work: &mut CognitionHeadHashWorkV1,
) -> Result<(), ValueTreeError> {
    while *used > 0 {
        let left = spans[usize::from(*used) - 1];
        if left.height != span.height {
            break;
        }
        let width = 1_u64
            .checked_shl(u32::from(span.height))
            .ok_or_else(invalid)?;
        let combined = width.checked_mul(2).ok_or_else(invalid)?;
        if left.start.checked_add(width) != Some(span.start) || !left.start.is_multiple_of(combined)
        {
            break;
        }
        work.node()?;
        if left.start == 0 {
            *root = Some(*merge_subtrees_root(&left.cv, &span.cv, Mode::Hash).as_bytes());
        }
        span = Span {
            start: left.start,
            height: left.height + 1,
            cv: merge_subtrees_non_root(&left.cv, &span.cv, Mode::Hash),
        };
        *used -= 1;
    }
    if usize::from(*used) >= SPANS {
        return Err(invalid());
    }
    spans[usize::from(*used)] = span;
    *used += 1;
    Ok(())
}
