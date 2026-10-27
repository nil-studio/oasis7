//! Bounded public-message assembly. No private candidate DTO is decoded here.
use super::*;
use serde::{Deserialize, Serialize};
pub(super) const CHUNK: usize = 16 * 1024;
pub(super) const MEMORY: usize = 32 * 1024 * 1024;
pub(super) const TTL: Duration = Duration::from_secs(120);
const DOMAIN: &str = "oasis7.bft.public-peer.chunk.v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    domain: String,
    context: String,
    digest: String,
    total: usize,
    offset: usize,
    bytes_hex: String,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Ack {
    domain: String,
    context: String,
    digest: String,
    next: usize,
}
fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(v).map_err(|_| "public chunk encode".into())
}
impl Frame {
    pub fn new(
        context: &str,
        bytes: &[u8],
        digest: &str,
        offset: usize,
    ) -> Result<Vec<u8>, String> {
        let end = bytes.len().min(offset.saturating_add(CHUNK));
        encode(&Self {
            domain: DOMAIN.into(),
            context: context.into(),
            digest: digest.into(),
            total: bytes.len(),
            offset,
            bytes_hex: hex::encode(&bytes[offset..end]),
        })
    }
    pub fn decode(bytes: &[u8], expected: &str) -> Result<Self, String> {
        if bytes.len() > 64 * 1024 {
            return Err("public chunk frame capacity".into());
        }
        // The frame has only strings/scalars. Check declared allocations before
        // derived decoding by first decoding a bounded small CBOR value scanner.
        scan_frame(bytes)?;
        let frame: Self = serde_cbor::from_slice(bytes).map_err(|_| "public chunk decode")?;
        if encode(&frame)? != bytes
            || frame.domain != DOMAIN
            || frame.context != expected
            || frame.digest.len() != 64
            || !frame
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || frame.total == 0
            || frame.total > BFT_PUBLIC_PEER_MAX_BYTES_V3
            || frame.offset >= frame.total
            || frame.bytes_hex.is_empty()
            || frame.bytes_hex.len() > CHUNK * 2
            || frame.bytes_hex.len() % 2 != 0
        {
            return Err("public chunk identity/size rejected".into());
        }
        Ok(frame)
    }
}
// Frame maximum is tiny, but serde must not trust an attacker declared map or
// string length before discovering EOF. This scanner admits only scalar fields.
fn scan_frame(bytes: &[u8]) -> Result<(), String> {
    fn item(bytes: &[u8], at: &mut usize, depth: usize) -> Result<(), String> {
        if depth > 2 {
            return Err("public frame nesting".into());
        }
        let b = *bytes.get(*at).ok_or("public frame truncated")?;
        *at += 1;
        let major = b >> 5;
        let ai = b & 31;
        let n = match ai {
            0..=23 => u64::from(ai),
            24..=27 => {
                let count = 1usize << (ai - 24);
                let end = at.checked_add(count).ok_or("frame overflow")?;
                let data = bytes.get(*at..end).ok_or("frame truncated")?;
                *at = end;
                data.iter().fold(0u64, |n, b| n * 256 + u64::from(*b))
            }
            _ => return Err("indefinite public frame".into()),
        };
        let n = usize::try_from(n).map_err(|_| "frame integer overflow")?;
        match major {
            0 => Ok(()),
            3 => {
                *at = at
                    .checked_add(n)
                    .filter(|end| *end <= bytes.len())
                    .ok_or("frame string length")?;
                Ok(())
            }
            5 if depth == 0 && n <= 8 => {
                for _ in 0..n * 2 {
                    item(bytes, at, depth + 1)?;
                }
                Ok(())
            }
            _ => Err("public frame type".into()),
        }
    }
    let mut at = 0;
    item(bytes, &mut at, 0)?;
    if at != bytes.len() {
        return Err("public frame trailing".into());
    }
    Ok(())
}
pub(super) fn check_ack(
    bytes: &[u8],
    context: &str,
    digest: &str,
    next: usize,
) -> Result<(), String> {
    if bytes.len() > 1024 {
        return Err("public ack capacity".into());
    }
    scan_frame(bytes)?;
    let a: Ack = serde_cbor::from_slice(bytes).map_err(|_| "public ack decode")?;
    if encode(&a)? != bytes
        || a != (Ack {
            domain: DOMAIN.into(),
            context: context.into(),
            digest: digest.into(),
            next,
        })
    {
        return Err("public ack mismatch".into());
    }
    Ok(())
}
struct Partial {
    total: usize,
    bytes: Vec<u8>,
    deadline: Instant,
}
pub(super) struct Inbox {
    partial: BTreeMap<(PeerId, String), Partial>,
    complete: VecDeque<(usize, NodeBftReceivedPublicV3)>,
    completed: BTreeMap<(PeerId, String), (usize, Instant)>,
    partial_bytes: usize,
    complete_bytes: usize,
}
impl Inbox {
    pub fn new() -> Self {
        Self {
            partial: BTreeMap::new(),
            complete: VecDeque::new(),
            completed: BTreeMap::new(),
            partial_bytes: 0,
            complete_bytes: 0,
        }
    }
    pub fn receive(
        &mut self,
        peer: PeerId,
        f: Frame,
        context: &BftPeerContextV3,
    ) -> Result<Vec<u8>, String> {
        let now = Instant::now();
        self.partial.retain(|_, p| {
            if p.deadline <= now {
                self.partial_bytes -= p.total;
                false
            } else {
                true
            }
        });
        self.completed.retain(|_, (_, deadline)| *deadline > now);
        let bytes = hex::decode(&f.bytes_hex).map_err(|_| "public chunk hex")?;
        if hex::encode(&bytes) != f.bytes_hex
            || f.offset
                .checked_add(bytes.len())
                .is_none_or(|end| end > f.total)
        {
            return Err("public chunk range".into());
        }
        let key = (peer, f.digest.clone());
        if let Some((total, _)) = self.completed.get(&key) {
            if *total != f.total {
                return Err("public completed conflict".into());
            }
            return encode(&Ack {
                domain: DOMAIN.into(),
                context: f.context,
                digest: f.digest,
                next: f.offset + bytes.len(),
            });
        }
        if !self.partial.contains_key(&key) {
            if f.offset != 0
                || self.partial.len() >= 8
                || self.partial.keys().filter(|(p, _)| *p == peer).count() >= 2
                || f.total > MEMORY.saturating_sub(self.partial_bytes)
            {
                return Err("public assembly admission".into());
            }
            self.partial_bytes += f.total;
            self.partial.insert(
                key.clone(),
                Partial {
                    total: f.total,
                    bytes: Vec::with_capacity(f.total),
                    deadline: now + TTL,
                },
            );
        }
        let p = self
            .partial
            .get_mut(&key)
            .ok_or("public assembly missing")?;
        if p.total != f.total {
            return Err("public assembly total changed".into());
        }
        if f.offset < p.bytes.len() {
            if p.bytes.get(f.offset..f.offset + bytes.len()) != Some(bytes.as_slice()) {
                return Err("public duplicate chunk conflict".into());
            }
        } else if f.offset == p.bytes.len() {
            p.bytes.extend_from_slice(&bytes);
        } else {
            return Err("public chunk out of order".into());
        }
        let next = f.offset + bytes.len();
        if p.bytes.len() == p.total {
            if message_digest(&f.context, &p.bytes) != f.digest {
                return Err("public assembled digest mismatch".into());
            }
            let message = decode_bft_public_peer_message_v3(&p.bytes, context)?;
            if self.complete.len() >= 128 || p.total > MEMORY.saturating_sub(self.complete_bytes) {
                return Err("public inbox capacity".into());
            }
            let p = self.partial.remove(&key).ok_or("public assembly missing")?;
            self.partial_bytes -= p.total;
            self.complete_bytes += p.total;
            self.complete.push_back((
                p.total,
                NodeBftReceivedPublicV3 {
                    peer_id: peer.to_string(),
                    message,
                },
            ));
            if self.completed.len() >= 256 {
                if let Some(oldest) = self
                    .completed
                    .iter()
                    .min_by_key(|(_, (_, d))| *d)
                    .map(|(k, _)| k.clone())
                {
                    self.completed.remove(&oldest);
                }
            }
            self.completed.insert(key, (p.total, p.deadline));
        }
        encode(&Ack {
            domain: DOMAIN.into(),
            context: f.context,
            digest: f.digest,
            next,
        })
    }
    #[cfg(test)]
    pub fn expire_partials_for_test(&mut self) {
        for p in self.partial.values_mut() {
            p.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
    #[cfg(test)]
    pub fn deadlines_for_test(&self) -> Vec<Instant> {
        self.partial.values().map(|p| p.deadline).collect()
    }
    pub fn drain(&mut self, max: usize) -> Vec<NodeBftReceivedPublicV3> {
        let mut out = Vec::new();
        for _ in 0..max.min(128) {
            let Some((n, message)) = self.complete.pop_front() else {
                break;
            };
            self.complete_bytes -= n;
            out.push(message);
        }
        out
    }
}
