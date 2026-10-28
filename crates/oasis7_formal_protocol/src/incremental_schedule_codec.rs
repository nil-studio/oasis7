use super::*;
use std::sync::OnceLock;
const MAGIC: &[u8] = b"oasis7.iws.schedule-index-page.v1\0";
pub(super) struct ScheduleCodec<const K: u8>;
pub(super) fn time_key(wake: u64) -> [u8; 32] {
    let mut key = [0; 32];
    key[..8].copy_from_slice(&wake.to_be_bytes());
    key
}
pub(super) fn instance_key(id: &str) -> Result<[u8; 32], String> {
    if id.is_empty() || id.len() > SCHEDULE_MAX_INSTANCE_BYTES {
        return Err("schedule instance bound".into());
    }
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.iws.schedule-instance.v1\0");
    h.update(&(id.len() as u16).to_be_bytes());
    h.update(id.as_bytes());
    Ok(*h.finalize().as_bytes())
}
impl<const K: u8> Codec for ScheduleCodec<K> {
    type Leaf = Leaf;
    const MAX_NODE_BYTES: usize = SCHEDULE_MAX_PAGE_BYTES;
    fn key(leaf: &Leaf) -> Result<[u8; 32], String> {
        match (K, leaf) {
            (1, Leaf::Time { wake, count, root }) if *count > 0 && root != &empty::<2>()?[0] => {
                parse_hash(root).map_err(|e| e.to_string())?;
                Ok(time_key(*wake))
            }
            (2, Leaf::Instance(id)) => instance_key(id),
            _ => Err("schedule leaf kind/count/root".into()),
        }
    }
    fn encode(node: Node<Leaf>) -> Result<Vec<u8>, String> {
        if !matches!(K, 1 | 2) {
            return Err("schedule page kind".into());
        }
        let mut w = Writer::new();
        w.u8(K);
        match node {
            Node::Branch { depth, left, right } => {
                if depth >= DEPTH {
                    return Err("schedule branch depth".into());
                }
                w.u8(1);
                w.u16(depth as u16);
                w.hash(&parse_hash(&left).map_err(|e| e.to_string())?);
                w.hash(&parse_hash(&right).map_err(|e| e.to_string())?);
            }
            Node::Leaf(leaf) => {
                Self::key(&leaf)?;
                w.u8(0);
                match leaf {
                    Leaf::Time { wake, count, root } => {
                        w.u64(wake);
                        w.u64(count);
                        w.hash(&parse_hash(&root).map_err(|e| e.to_string())?);
                    }
                    Leaf::Instance(id) => w.string(&id),
                }
            }
        }
        w.finish(MAGIC, SCHEDULE_MAX_PAGE_BYTES)
            .map_err(|e| e.to_string())
    }
    fn decode(bytes: &[u8]) -> Result<Node<Leaf>, String> {
        let decode = || -> Result<Node<Leaf>, ExecutionError> {
            let mut r = Reader::new(bytes, MAGIC, SCHEDULE_MAX_PAGE_BYTES)?;
            if !matches!(K, 1 | 2) || r.u8()? != K {
                return Err(ExecutionError::Invalid("schedule page kind"));
            }
            let node = match r.u8()? {
                1 => {
                    let depth = r.u16()? as usize;
                    if depth >= DEPTH {
                        return Err(ExecutionError::Invalid("schedule branch depth"));
                    }
                    Node::Branch {
                        depth,
                        left: hex::encode(r.array::<32>()?),
                        right: hex::encode(r.array::<32>()?),
                    }
                }
                0 => {
                    let leaf = if K == 1 {
                        Leaf::Time {
                            wake: r.u64()?,
                            count: r.u64()?,
                            root: hex::encode(r.array::<32>()?),
                        }
                    } else {
                        let length = r.u16()? as usize;
                        if length == 0 || length > SCHEDULE_MAX_INSTANCE_BYTES {
                            return Err(ExecutionError::Invalid("schedule instance bound"));
                        }
                        Leaf::Instance(
                            std::str::from_utf8(r.take(length)?)
                                .map_err(|_| ExecutionError::Invalid("schedule instance UTF-8"))?
                                .to_owned(),
                        )
                    };
                    Self::key(&leaf).map_err(ExecutionError::Tree)?;
                    Node::Leaf(leaf)
                }
                _ => return Err(ExecutionError::Invalid("schedule page tag")),
            };
            r.finish()?;
            Ok(node)
        };
        decode().map_err(|e| e.to_string())
    }
}
pub(super) fn empty<const K: u8>() -> Result<&'static [String], String> {
    // Separate statics: a static inside a generic is shared by monomorphs.
    static TIMES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    static INSTANCES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    let cell = match K {
        1 => &TIMES,
        2 => &INSTANCES,
        _ => return Err("schedule empty kind".into()),
    };
    cell.get_or_init(|| {
        let mut w = Writer::new();
        w.u8(K);
        w.u8(2);
        let marker = w
            .finish(MAGIC, SCHEDULE_MAX_PAGE_BYTES)
            .map_err(|e| e.to_string())?;
        crate::merkle::build_empty_hashes::<ScheduleCodec<K>>(&marker)
    })
    .as_ref()
    .map(Vec::as_slice)
    .map_err(Clone::clone)
}
