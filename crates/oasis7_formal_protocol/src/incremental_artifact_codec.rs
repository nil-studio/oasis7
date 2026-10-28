use super::*;
use std::sync::OnceLock;
const MAGIC: &[u8] = b"oasis7.iws.artifact-page.v1\0";
pub(super) struct ArtifactCodec;
impl Codec for ArtifactCodec {
    type Leaf = ArtifactEntryV1;
    const MAX_NODE_BYTES: usize = ARTIFACT_MAX_PAGE_BYTES;
    fn key(entry: &ArtifactEntryV1) -> Result<[u8; 32], String> {
        Ok(entry.original.id)
    }
    fn encode(node: Node<ArtifactEntryV1>) -> Result<Vec<u8>, String> {
        let mut w = Writer::new();
        match node {
            Node::Branch { depth, left, right } => {
                if depth >= DEPTH {
                    return Err("artifact branch depth".into());
                }
                w.u8(1);
                w.u16(depth as u16);
                w.hash(&parse_hash(&left).map_err(|e| e.to_string())?);
                w.hash(&parse_hash(&right).map_err(|e| e.to_string())?);
            }
            Node::Leaf(entry) => {
                w.u8(0);
                w.u8(entry.kind as u8);
                w.hash(&entry.original.id);
                w.u64(entry.original.byte_len);
            }
        }
        w.finish(MAGIC, ARTIFACT_MAX_PAGE_BYTES)
            .map_err(|e| e.to_string())
    }
    fn decode(bytes: &[u8]) -> Result<Node<ArtifactEntryV1>, String> {
        let mut r =
            Reader::new(bytes, MAGIC, ARTIFACT_MAX_PAGE_BYTES).map_err(|e| e.to_string())?;
        let node = match r.u8().map_err(|e| e.to_string())? {
            1 => {
                let depth = r.u16().map_err(|e| e.to_string())? as usize;
                if depth >= DEPTH {
                    return Err("artifact branch depth".into());
                }
                Node::Branch {
                    depth,
                    left: hex::encode(r.array::<32>().map_err(|e| e.to_string())?),
                    right: hex::encode(r.array::<32>().map_err(|e| e.to_string())?),
                }
            }
            0 => Node::Leaf(ArtifactEntryV1 {
                kind: ArtifactKindV1::from_tag(r.u8().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
                original: OriginalArtifactRefV1 {
                    id: r.array().map_err(|e| e.to_string())?,
                    byte_len: r.u64().map_err(|e| e.to_string())?,
                },
            }),
            _ => return Err("artifact page tag".into()),
        };
        r.finish().map_err(|e| e.to_string())?;
        Ok(node)
    }
}
pub(super) fn empty() -> Result<&'static [String], String> {
    static HASHES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    HASHES
        .get_or_init(|| {
            let mut w = Writer::new();
            w.u8(2);
            let marker = w
                .finish(MAGIC, ARTIFACT_MAX_PAGE_BYTES)
                .map_err(|e| e.to_string())?;
            crate::merkle::build_empty_hashes::<ArtifactCodec>(&marker)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}
