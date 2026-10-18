//! Bounded leaves and fixed-fanout hash paths; the manifest stores one root.
use super::*;
use std::sync::{Arc, Mutex, OnceLock};
pub(super) const CATALOG: &str = ".catalog-v3";
const LEAF_IDS: usize = 4096;
const NODE_BYTES: usize = 512 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeRef {
    digest: String,
    count: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalogs {
    root: Option<NodeRef>,
}
impl Catalogs {
    pub(super) fn new() -> Self {
        Self::default()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    version: u32,
    prefix: String,
    kind: Kind,
}
#[derive(Serialize, Deserialize)]
enum Kind {
    Leaf(#[serde(deserialize_with = "decode_ids")] Vec<String>),
    Branch(Box<[Option<NodeRef>; 16]>),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Retired {
    prefix: String,
    digest: String,
}
fn decode_ids<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<String>, D::Error> {
    struct Ids;
    impl<'de> serde::de::Visitor<'de> for Ids {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded generation leaf IDs")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence.size_hint().is_some_and(|length| length > LEAF_IDS) {
                return Err(serde::de::Error::custom("generation leaf count"));
            }
            let mut ids = Vec::new();
            while let Some(id) = sequence.next_element()? {
                if ids.len() == LEAF_IDS {
                    return Err(serde::de::Error::custom("generation leaf count"));
                }
                ids.push(id);
            }
            Ok(ids)
        }
    }
    decoder.deserialize_seq(Ids)
}
fn prefix_valid(prefix: &str) -> bool {
    prefix.len() <= 64
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn reference(value: &NodeRef) -> Result<(), FormalError> {
    hash_hex(&value.digest)?;
    if value.count == 0 {
        return Err(invalid("generation empty node reference"));
    }
    Ok(())
}
fn shape(node: &Node) -> Result<u64, FormalError> {
    if node.version != 3 || !prefix_valid(&node.prefix) {
        return Err(invalid("generation node version/prefix"));
    }
    match &node.kind {
        Kind::Leaf(ids) => {
            if ids.is_empty()
                || ids.len() > LEAF_IDS
                || ids.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(invalid("generation leaf order/count"));
            }
            for id in ids {
                hash_hex(id)?;
                if !id.starts_with(&node.prefix) {
                    return Err(invalid("generation leaf path"));
                }
            }
            Ok(ids.len() as u64)
        }
        Kind::Branch(children) => {
            if node.prefix.len() == 64 {
                return Err(invalid("generation branch exceeds hash path"));
            }
            let count = children.iter().flatten().try_fold(0u64, |count, child| {
                reference(child)?;
                count
                    .checked_add(child.count)
                    .ok_or_else(|| invalid("generation count overflow"))
            })?;
            if count <= LEAF_IDS as u64 {
                return Err(invalid("generation nonminimal branch"));
            }
            Ok(count)
        }
    }
}
fn node_count(node: &Node) -> Result<u64, FormalError> {
    match &node.kind {
        Kind::Leaf(ids) => Ok(ids.len() as u64),
        Kind::Branch(children) => children.iter().flatten().try_fold(0u64, |sum, child| {
            sum.checked_add(child.count)
                .ok_or_else(|| invalid("generation count overflow"))
        }),
    }
}
fn decode(bytes: &[u8]) -> Result<Node, FormalError> {
    if bytes.len() > NODE_BYTES {
        return Err(invalid("generation node byte bound"));
    }
    let node: Node = serde_cbor::from_slice(bytes).map_err(io)?;
    if serde_cbor::to_vec(&node).map_err(io)? != bytes {
        return Err(invalid("generation node canonical bytes"));
    }
    shape(&node)?;
    Ok(node)
}
// Combined with the recipe cache's 12 MiB / 16 entries, all parsed physical
// metadata remains within the existing 16 MiB / 32-entry process budget.
#[derive(Default)]
struct Cache {
    nodes: BTreeMap<[u8; 32], (usize, Arc<Node>)>,
    order: std::collections::VecDeque<[u8; 32]>,
    bytes: usize,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn parsed(bytes: &[u8], digest: [u8; 32]) -> Result<Arc<Node>, FormalError> {
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Cache::default()))
        .lock()
        .map_err(|_| invalid("generation parsed cache poisoned"))?;
    if let Some((_, node)) = cache.nodes.get(&digest) {
        return Ok(node.clone());
    }
    let node = Arc::new(decode(bytes)?);
    let allocations = match &node.kind {
        Kind::Leaf(ids) => {
            ids.capacity() * std::mem::size_of::<String>()
                + ids.iter().map(String::capacity).sum::<usize>()
        }
        Kind::Branch(children) => {
            std::mem::size_of::<[Option<NodeRef>; 16]>()
                + children
                    .iter()
                    .flatten()
                    .map(|child| child.digest.capacity())
                    .sum::<usize>()
        }
    };
    let weight =
        bytes.len() + std::mem::size_of::<Node>() + node.prefix.capacity() + allocations + 256;
    const BYTES: usize = 4 * 1024 * 1024;
    if weight <= BYTES {
        while cache.nodes.len() >= 16 || cache.bytes + weight > BYTES {
            let Some(old) = cache.order.pop_front() else {
                return Ok(node);
            };
            if let Some((weight, _)) = cache.nodes.remove(&old) {
                cache.bytes -= weight;
            }
        }
        cache.bytes += weight;
        cache.order.push_back(digest);
        cache.nodes.insert(digest, (weight, node.clone()));
    }
    Ok(node)
}
fn load(dir: &Path, value: &NodeRef, prefix: &str) -> Result<Arc<Node>, FormalError> {
    reference(value)?;
    let bytes = read(&dir.join(CATALOG).join(&value.digest), NODE_BYTES)?;
    let digest = blake3::hash(&bytes);
    if digest.to_hex().as_str() != value.digest {
        return Err(invalid("generation node checksum"));
    }
    let node = parsed(&bytes, *digest.as_bytes())?;
    if node.prefix != prefix || node_count(&node)? != value.count {
        return Err(invalid("generation node path/count binding"));
    }
    Ok(node)
}
fn publish(dir: &Path, node: Node) -> Result<NodeRef, FormalError> {
    let count = shape(&node)?;
    let bytes = serde_cbor::to_vec(&node).map_err(io)?;
    if bytes.len() > NODE_BYTES {
        return Err(invalid("generation node byte bound"));
    }
    let digest = blake3::hash(&bytes).to_hex().to_string();
    publication::publish(&dir.join(CATALOG), &dir.join(CATALOG).join(&digest), &bytes)?;
    Ok(NodeRef { digest, count })
}
fn nibble(id: &str, depth: usize) -> Result<usize, FormalError> {
    id.as_bytes()
        .get(depth)
        .and_then(|byte| match byte {
            b'0'..=b'9' => Some((byte - b'0') as usize),
            b'a'..=b'f' => Some((byte - b'a' + 10) as usize),
            _ => None,
        })
        .ok_or_else(|| invalid("generation hash path"))
}
fn child_prefix(prefix: &str, index: usize) -> String {
    format!("{prefix}{index:x}")
}
fn groups(ids: Vec<String>, depth: usize) -> Result<[Vec<String>; 16], FormalError> {
    let mut groups: [Vec<String>; 16] = std::array::from_fn(|_| Vec::new());
    for id in ids {
        groups[nibble(&id, depth)?].push(id);
    }
    Ok(groups)
}
fn build(dir: &Path, prefix: &str, ids: Vec<String>) -> Result<NodeRef, FormalError> {
    if ids.len() <= LEAF_IDS {
        return publish(
            dir,
            Node {
                version: 3,
                prefix: prefix.into(),
                kind: Kind::Leaf(ids),
            },
        );
    }
    if prefix.len() == 64 {
        return Err(invalid("generation unique IDs exceed hash space"));
    }
    let mut children: [Option<NodeRef>; 16] = std::array::from_fn(|_| None);
    for (index, ids) in groups(ids, prefix.len())?.into_iter().enumerate() {
        if !ids.is_empty() {
            children[index] = Some(build(dir, &child_prefix(prefix, index), ids)?);
        }
    }
    publish(
        dir,
        Node {
            version: 3,
            prefix: prefix.into(),
            kind: Kind::Branch(Box::new(children)),
        },
    )
}
fn update(
    dir: &Path,
    old: Option<&NodeRef>,
    prefix: &str,
    ids: Vec<String>,
    retired: &mut Vec<Retired>,
) -> Result<NodeRef, FormalError> {
    let Some(old) = old else {
        return build(dir, prefix, ids);
    };
    let node = load(dir, old, prefix)?;
    let next = match &node.kind {
        Kind::Leaf(previous) => {
            let mut union: BTreeSet<String> = previous.iter().cloned().collect();
            union.extend(ids);
            if union.len() == previous.len() {
                return Ok(old.clone());
            }
            build(dir, prefix, union.into_iter().collect())?
        }
        Kind::Branch(previous) => {
            let mut children = previous.clone();
            for (index, ids) in groups(ids, prefix.len())?.into_iter().enumerate() {
                if !ids.is_empty() {
                    children[index] = Some(update(
                        dir,
                        previous[index].as_ref(),
                        &child_prefix(prefix, index),
                        ids,
                        retired,
                    )?);
                }
            }
            if children == *previous {
                return Ok(old.clone());
            }
            publish(
                dir,
                Node {
                    version: 3,
                    prefix: prefix.into(),
                    kind: Kind::Branch(children),
                },
            )?
        }
    };
    retired.push(Retired {
        prefix: prefix.into(),
        digest: old.digest.clone(),
    });
    Ok(next)
}
pub(super) fn validate(catalog: &Catalogs) -> Result<(), FormalError> {
    if let Some(root) = &catalog.root {
        reference(root)?;
    }
    Ok(())
}
pub(super) fn add(
    dir: &Path,
    catalog: &mut Catalogs,
    ids: &[String],
) -> Result<Vec<Retired>, FormalError> {
    if ids.len() > 1024 {
        return Err(invalid("generation catalog input page"));
    }
    validate(catalog)?;
    directory(&dir.join(CATALOG))?;
    let mut unique = BTreeSet::new();
    for id in ids {
        hash_hex(id)?;
        unique.insert(id.clone());
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let mut retired = Vec::new();
    catalog.root = Some(update(
        dir,
        catalog.root.as_ref(),
        "",
        unique.into_iter().collect(),
        &mut retired,
    )?);
    Ok(retired)
}
pub(super) fn count(catalog: &Catalogs) -> Result<usize, FormalError> {
    validate(catalog)?;
    usize::try_from(catalog.root.as_ref().map_or(0, |root| root.count))
        .map_err(|_| invalid("generation platform count overflow"))
}
fn page_node(
    dir: &Path,
    value: &NodeRef,
    prefix: &str,
    after: Option<&str>,
    limit: usize,
    out: &mut Vec<String>,
) -> Result<(), FormalError> {
    if out.len() == limit || after.is_some_and(|id| prefix < &id[..prefix.len()]) {
        return Ok(());
    }
    let node = load(dir, value, prefix)?;
    match &node.kind {
        Kind::Leaf(ids) => {
            for id in ids {
                if after.is_none_or(|previous| id.as_str() > previous) {
                    out.push(id.clone());
                    if out.len() == limit {
                        break;
                    }
                }
            }
        }
        Kind::Branch(children) => {
            for (index, child) in children.iter().enumerate() {
                if let Some(child) = child {
                    page_node(dir, child, &child_prefix(prefix, index), after, limit, out)?;
                }
                if out.len() == limit {
                    break;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn page(
    dir: &Path,
    catalog: &Catalogs,
    after: Option<&str>,
    limit: usize,
) -> Result<Vec<String>, FormalError> {
    validate(catalog)?;
    if limit == 0 || limit > 1024 {
        return Err(invalid("generation catalog page budget"));
    }
    if let Some(id) = after {
        hash_hex(id)?;
    }
    let mut out = Vec::new();
    if let Some(root) = &catalog.root {
        page_node(dir, root, "", after, limit, &mut out)?;
    }
    Ok(out)
}
pub(super) fn visit(
    dir: &Path,
    catalog: &Catalogs,
    mut visitor: impl FnMut(String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    let mut after = None;
    loop {
        let ids = page(dir, catalog, after.as_deref(), 1024)?;
        if ids.is_empty() {
            return Ok(());
        }
        after = ids.last().cloned();
        for id in ids {
            visitor(id)?;
        }
    }
}
fn at(dir: &Path, catalog: &Catalogs, prefix: &str) -> Result<Option<NodeRef>, FormalError> {
    let Some(mut value) = catalog.root.clone() else {
        return Ok(None);
    };
    let mut path = String::new();
    loop {
        let node = load(dir, &value, &path)?;
        if path == prefix {
            return Ok(Some(value));
        }
        match &node.kind {
            Kind::Leaf(_) => return Ok(None),
            Kind::Branch(children) => {
                let index = nibble(prefix, path.len())?;
                let Some(child) = &children[index] else {
                    return Ok(None);
                };
                value = child.clone();
                path = child_prefix(&path, index);
            }
        }
    }
}
pub(super) fn contains(dir: &Path, catalog: &Catalogs, id: &str) -> Result<bool, FormalError> {
    hash_hex(id)?;
    validate(catalog)?;
    let Some(mut value) = catalog.root.clone() else {
        return Ok(false);
    };
    let mut prefix = String::new();
    loop {
        let node = load(dir, &value, &prefix)?;
        match &node.kind {
            Kind::Leaf(ids) => {
                return Ok(ids
                    .binary_search_by(|candidate| candidate.as_str().cmp(id))
                    .is_ok());
            }
            Kind::Branch(children) => {
                let index = nibble(id, prefix.len())?;
                let Some(child) = &children[index] else {
                    return Ok(false);
                };
                value = child.clone();
                prefix = child_prefix(&prefix, index);
            }
        }
    }
}
fn difference(
    dir: &Path,
    all: &NodeRef,
    done: Option<&NodeRef>,
    prefix: &str,
    carry: Option<&BTreeSet<String>>,
    limit: usize,
    out: &mut Vec<String>,
) -> Result<(), FormalError> {
    if out.len() == limit || done == Some(all) {
        return Ok(());
    }
    let node = load(dir, all, prefix)?;
    let previous = done.map(|done| load(dir, done, prefix)).transpose()?;
    let leaf = previous.as_ref().and_then(|node| match &node.kind {
        Kind::Leaf(ids) => Some(ids.iter().cloned().collect::<BTreeSet<_>>()),
        _ => None,
    });
    let carry = carry.or(leaf.as_ref());
    match &node.kind {
        Kind::Leaf(ids) => {
            for id in ids {
                let included = if let Some(known) = carry {
                    known.contains(id)
                } else if done.is_some() {
                    return Err(invalid(
                        "generation completed branch cannot fit captured leaf",
                    ));
                } else {
                    false
                };
                if !included {
                    out.push(id.clone());
                    if out.len() == limit {
                        break;
                    }
                }
            }
        }
        Kind::Branch(children) => {
            for (index, child) in children.iter().enumerate() {
                if let Some(child) = child {
                    let previous = previous.as_ref().and_then(|node| match &node.kind {
                        Kind::Branch(children) => children[index].as_ref(),
                        _ => None,
                    });
                    difference(
                        dir,
                        child,
                        previous,
                        &child_prefix(prefix, index),
                        carry,
                        limit,
                        out,
                    )?;
                }
                if out.len() == limit {
                    break;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn pending(
    dir: &Path,
    all: &Catalogs,
    done: &Catalogs,
    limit: usize,
) -> Result<Vec<String>, FormalError> {
    validate(all)?;
    validate(done)?;
    if limit == 0 || limit > 1024 {
        return Err(invalid("generation pending page budget"));
    }
    let mut out = Vec::new();
    if let Some(root) = &all.root {
        difference(dir, root, done.root.as_ref(), "", None, limit, &mut out)?;
    }
    Ok(out)
}
pub(super) fn merge_retired(target: &mut Vec<Retired>, more: Vec<Retired>) {
    target.extend(more);
    target.sort();
    target.dedup();
}
pub(super) fn validate_retired(values: &[Retired]) -> Result<(), FormalError> {
    if values.len() > 131072 {
        return Err(invalid("generation retired catalog page budget"));
    }
    for value in values {
        if !prefix_valid(&value.prefix) {
            return Err(invalid("generation retired prefix"));
        }
        hash_hex(&value.digest)?;
    }
    Ok(())
}
pub(super) fn prune(
    dir: &Path,
    roots: [&Catalogs; 2],
    pending: &mut Vec<Retired>,
) -> Result<(), FormalError> {
    validate_retired(pending)?;
    let count = pending.len().min(1024);
    for value in pending.iter().take(count) {
        let mut live = false;
        for root in roots {
            if at(dir, root, &value.prefix)?.is_some_and(|current| current.digest == value.digest) {
                live = true;
                break;
            }
        }
        if live {
            continue;
        }
        let path = dir.join(CATALOG).join(&value.digest);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
            Ok(_) => {}
        }
        let node = stored_file(&path, &value.digest)?;
        if node.prefix != value.prefix {
            return Err(invalid("generation retired node prefix binding"));
        }
        fs::remove_file(path).map_err(io)?;
    }
    if count > 0 {
        sync(&dir.join(CATALOG))?;
        pending.drain(..count);
    }
    Ok(())
}
fn stored_file(path: &Path, digest: &str) -> Result<Node, FormalError> {
    let bytes = read(path, NODE_BYTES)?;
    if blake3::hash(&bytes).to_hex().as_str() != digest {
        return Err(invalid("generation catalog stored checksum"));
    }
    decode(&bytes)
}
pub(super) fn validate_file(path: &Path, digest: &str) -> Result<(), FormalError> {
    stored_file(path, digest).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_same_prefix_and_prune_after_interrupted_root_publication() {
        let fixture = super::super::super::tests::Fixture::new();
        let dir = &fixture.0;
        let ids = (0..4097u64)
            .map(|n| format!("abc{n:061x}"))
            .collect::<Vec<_>>();
        let mut all = Catalogs::new();
        let mut retired = Vec::new();
        for page in ids[..4096].chunks(1024) {
            merge_retired(&mut retired, add(dir, &mut all, page).unwrap());
        }
        let completed = all.clone();
        let old = completed.root.as_ref().unwrap().digest.clone();
        merge_retired(&mut retired, add(dir, &mut all, &ids[4096..]).unwrap());
        assert_eq!(count(&all).unwrap(), 4097);
        assert_eq!(pending(dir, &all, &completed, 1024).unwrap(), ids[4096..]);
        for id in [&ids[0], &ids[4096]] {
            assert!(contains(dir, &all, id).unwrap());
        }
        let mut collected = Vec::new();
        visit(dir, &all, |id| {
            collected.push(id);
            Ok(())
        })
        .unwrap();
        assert_eq!(collected, ids);
        // A durable completed root still refers to the old leaf.
        prune(dir, [&all, &completed], &mut retired).unwrap();
        assert!(dir.join(CATALOG).join(&old).exists());
        // Replacing completed queues its obsolete root again. Simulate a crash
        // after unlink and before persisting the drained maintenance queue.
        merge_retired(
            &mut retired,
            add(dir, &mut completed.clone(), &ids[4096..]).unwrap(),
        );
        let before_unlink = retired.clone();
        prune(dir, [&all, &all], &mut retired).unwrap();
        assert!(!dir.join(CATALOG).join(&old).exists());
        let mut replay = before_unlink;
        prune(dir, [&all, &all], &mut replay).unwrap();
        assert!(replay.is_empty());
        let mut alternate = Catalogs::new();
        for page in ids.rchunks(1024) {
            add(dir, &mut alternate, page).unwrap();
        }
        assert_eq!(alternate, all);
        let root = all.root.as_ref().unwrap();
        fs::write(dir.join(CATALOG).join(&root.digest), b"corrupt").unwrap();
        assert!(contains(dir, &all, &ids[0]).is_err());
    }
}
