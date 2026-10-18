//! Complete field deltas preserving the exact original CBOR representation.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_SNAPSHOT_DELTA_DEPTH: usize = 32;
pub const MAX_SNAPSHOT_DELTA_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SNAPSHOT_CHAIN_BYTES: usize = 256 * 1024 * 1024;
fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
// Snapshot is a definite text-keyed struct map. Preserve nested struct field order,
// integer encoding, byte strings, floats and every otherwise unknown field verbatim.
type EncodedFields = (Vec<u8>, Vec<(String, Vec<u8>)>);
fn fields(bytes: &[u8]) -> Result<EncodedFields, String> {
    if bytes.is_empty() || bytes.len() > MAX_SNAPSHOT_DELTA_BYTES {
        return Err("snapshot size invalid".into());
    }
    let (count, header) = match bytes[0] {
        byte @ 0xa0..=0xb7 => (usize::from(byte - 0xa0), 1),
        0xb8 if bytes.len() >= 2 && bytes[1] >= 24 => (usize::from(bytes[1]), 2),
        _ => return Err("snapshot must be a bounded canonical definite field map".into()),
    };
    let mut offset = header;
    let mut result = Vec::with_capacity(count);
    let mut names = BTreeSet::new();
    for _ in 0..count {
        let mut de = serde_cbor::Deserializer::from_slice(&bytes[offset..]);
        let key = String::deserialize(&mut de).map_err(|e| e.to_string())?;
        let key_len = de.byte_offset();
        if serde_cbor::to_vec(&key).map_err(|e| e.to_string())? != bytes[offset..offset + key_len]
            || !names.insert(key.clone())
        {
            return Err("snapshot field key noncanonical or duplicated".into());
        }
        offset += key_len;
        let mut de = serde_cbor::Deserializer::from_slice(&bytes[offset..]);
        serde::de::IgnoredAny::deserialize(&mut de).map_err(|e| e.to_string())?;
        let length = de.byte_offset();
        result.push((key, bytes[offset..offset + length].to_vec()));
        offset += length;
    }
    if offset != bytes.len() {
        return Err("snapshot field map trailing bytes".into());
    }
    Ok((bytes[..header].to_vec(), result))
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotDeltaV1 {
    pub schema_version: u32,
    pub base_snapshot_digest: String,
    pub target_snapshot_digest: String,
    /// Complete ordered field inventory, including unchanged control fields.
    #[serde(deserialize_with = "deserialize_fields")]
    pub fields: Vec<String>,
    /// Original complete field value encodings. Empty/clear values are explicit.
    #[serde(
        serialize_with = "serialize_replacements",
        deserialize_with = "deserialize_replacements"
    )]
    pub replacements: BTreeMap<String, Vec<u8>>,
}
fn deserialize_fields<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    struct Fields;
    impl<'de> serde::de::Visitor<'de> for Fields {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("at most 255 snapshot field names")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let mut out = Vec::new();
            while let Some(field) = seq.next_element::<String>()? {
                if out.len() >= 255 || field.len() > 256 {
                    return Err(A::Error::custom("snapshot field inventory exceeds bound"));
                }
                out.push(field);
            }
            Ok(out)
        }
    }
    de.deserialize_seq(Fields)
}
fn serialize_replacements<S: serde::Serializer>(
    values: &BTreeMap<String, Vec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    struct Bytes<'a>(&'a [u8]);
    impl Serialize for Bytes<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_bytes(self.0)
        }
    }
    let mut map = serializer.serialize_map(Some(values.len()))?;
    for (key, value) in values {
        map.serialize_entry(key, &Bytes(value))?;
    }
    map.end()
}

fn deserialize_replacements<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<BTreeMap<String, Vec<u8>>, D::Error> {
    struct Bytes(Vec<u8>);
    impl<'de> Deserialize<'de> for Bytes {
        fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            struct ByteVisitor;
            impl<'de> serde::de::Visitor<'de> for ByteVisitor {
                type Value = Bytes;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("bounded original CBOR field bytes")
                }
                fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                    if bytes.len() > MAX_SNAPSHOT_DELTA_BYTES {
                        return Err(E::custom("snapshot field value exceeds bound"));
                    }
                    Ok(Bytes(bytes.to_vec()))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Self::Value, A::Error> {
                    use serde::de::Error;
                    let mut out = Vec::new();
                    if seq
                        .size_hint()
                        .is_some_and(|n| n > MAX_SNAPSHOT_DELTA_BYTES)
                    {
                        return Err(A::Error::custom("snapshot field value exceeds bound"));
                    }
                    while let Some(byte) = seq.next_element::<u8>()? {
                        if out.len() >= MAX_SNAPSHOT_DELTA_BYTES {
                            return Err(A::Error::custom("snapshot field value exceeds bound"));
                        }
                        out.push(byte);
                    }
                    Ok(Bytes(out))
                }
            }
            de.deserialize_bytes(ByteVisitor)
        }
    }
    struct Replacements;
    impl<'de> serde::de::Visitor<'de> for Replacements {
        type Value = BTreeMap<String, Vec<u8>>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded unique snapshot replacements")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if map.size_hint().is_some_and(|n| n > 255) {
                return Err(A::Error::custom("snapshot replacement count exceeds bound"));
            }
            let mut out = BTreeMap::new();
            let mut size = 0usize;
            while let Some((key, Bytes(value))) = map.next_entry::<String, Bytes>()? {
                size = size
                    .checked_add(value.len())
                    .ok_or_else(|| A::Error::custom("snapshot replacement size overflow"))?;
                if out.len() >= 255
                    || key.len() > 256
                    || size > MAX_SNAPSHOT_DELTA_BYTES
                    || out.insert(key, value).is_some()
                {
                    return Err(A::Error::custom(
                        "snapshot replacement count, bytes or duplicate invalid",
                    ));
                }
            }
            Ok(out)
        }
    }
    de.deserialize_map(Replacements)
}

impl SnapshotDeltaV1 {
    pub fn between<T: Serialize>(base: &T, target: &T) -> Result<Self, String> {
        Self::between_bytes(
            &serde_cbor::to_vec(base).map_err(|e| e.to_string())?,
            &serde_cbor::to_vec(target).map_err(|e| e.to_string())?,
        )
    }
    pub fn apply<T: Serialize + serde::de::DeserializeOwned>(&self, base: &T) -> Result<T, String> {
        let bytes = self.apply_bytes(&serde_cbor::to_vec(base).map_err(|e| e.to_string())?)?;
        let target: T = serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
        if serde_cbor::to_vec(&target).map_err(|e| e.to_string())? != bytes {
            return Err("snapshot delta typed canonical output mismatch".into());
        }
        Ok(target)
    }

    pub fn between_bytes(base: &[u8], target: &[u8]) -> Result<Self, String> {
        let (_, before) = fields(base)?;
        let (_, after) = fields(target)?;
        if before
            .iter()
            .map(|(k, _)| k)
            .ne(after.iter().map(|(k, _)| k))
        {
            return Err("snapshot delta cannot cross an undeclared field schema".into());
        }
        let delta = Self {
            schema_version: 1,
            base_snapshot_digest: digest(base),
            target_snapshot_digest: digest(target),
            fields: before.iter().map(|(k, _)| k.clone()).collect(),
            replacements: before
                .iter()
                .zip(after)
                .filter_map(|((_, old), (key, new))| (old != &new).then_some((key, new)))
                .collect(),
        };
        if serde_cbor::to_vec(&delta).map_err(|e| e.to_string())?.len() > MAX_SNAPSHOT_DELTA_BYTES {
            return Err("snapshot delta exceeds byte budget".into());
        }
        if delta.apply_bytes(base)? != target {
            return Err("snapshot delta producer self-check failed".into());
        }
        Ok(delta)
    }
    pub fn apply_bytes(&self, base: &[u8]) -> Result<Vec<u8>, String> {
        if self.schema_version != 1
            || self.base_snapshot_digest != digest(base)
            || serde_cbor::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_SNAPSHOT_DELTA_BYTES
        {
            return Err("snapshot delta version, base or byte bound invalid".into());
        }
        let (mut output, before) = fields(base)?;
        if self.fields.iter().ne(before.iter().map(|(k, _)| k))
            || self
                .replacements
                .keys()
                .any(|key| !self.fields.contains(key))
        {
            return Err("snapshot delta field coverage incomplete or unknown".into());
        }
        for (key, old) in before {
            output.extend(serde_cbor::to_vec(&key).map_err(|e| e.to_string())?);
            let value = self.replacements.get(&key).unwrap_or(&old);
            if self.replacements.contains_key(&key) && *value == old {
                return Err("snapshot delta redundant replacement".into());
            }
            output.extend_from_slice(value);
            if output.len() > MAX_SNAPSHOT_DELTA_BYTES {
                return Err("snapshot delta output exceeds byte budget".into());
            }
        }
        fields(&output)?;
        if digest(&output) != self.target_snapshot_digest {
            return Err("snapshot delta target hash mismatch".into());
        }
        Ok(output)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotArtifactV1 {
    pub schema_version: u32,
    pub domain: String,
    pub base_ref: String,
    pub depth: usize,
    pub delta: SnapshotDeltaV1,
}
impl SnapshotArtifactV1 {
    pub const DOMAIN: &'static str = "oasis7.runtime.snapshot-delta.v1";
    pub const MAGIC: &'static [u8] = b"oasis7.runtime.snapshot-delta.v1\0";
    pub fn new(base_ref: String, depth: usize, delta: SnapshotDeltaV1) -> Result<Self, String> {
        let result = Self {
            schema_version: 1,
            domain: Self::DOMAIN.into(),
            base_ref,
            depth,
            delta,
        };
        result.encode()?;
        Ok(result)
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.schema_version != 1
            || self.domain != Self::DOMAIN
            || !valid_digest(&self.base_ref)
            || self.depth == 0
            || self.depth > MAX_SNAPSHOT_DELTA_DEPTH
        {
            return Err("snapshot artifact metadata invalid".into());
        }
        let mut encoded = Self::MAGIC.to_vec();
        encoded.extend(serde_cbor::to_vec(self).map_err(|e| e.to_string())?);
        if encoded.len() > MAX_SNAPSHOT_DELTA_BYTES {
            return Err("snapshot artifact exceeds byte bound".into());
        }
        Ok(encoded)
    }
    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, String> {
        if bytes.len() > MAX_SNAPSHOT_DELTA_BYTES {
            return Err("snapshot artifact exceeds byte bound".into());
        }
        let Some(payload) = bytes.strip_prefix(Self::MAGIC) else {
            return Ok(None);
        };
        let result: Self = serde_cbor::from_slice(payload).map_err(|e| e.to_string())?;
        if result.encode()? != bytes {
            return Err("snapshot artifact noncanonical encoding".into());
        }
        Ok(Some(result))
    }
}
pub fn resolve_snapshot_artifact_bytes(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    resolve_snapshot_artifact_bytes_with_depth(reference, reader).map(|(bytes, _)| bytes)
}
pub fn resolve_snapshot_artifact_bytes_with_depth(
    reference: &str,
    reader: &impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(Vec<u8>, usize), String> {
    let mut current = reference.to_owned();
    let mut seen = BTreeSet::new();
    let mut deltas = Vec::new();
    let mut total_bytes = 0usize;
    let mut snapshot;
    loop {
        if !valid_digest(&current) || !seen.insert(current.clone()) {
            return Err("snapshot artifact reference invalid or cyclic".into());
        }
        let encoded = reader(&current)?;
        total_bytes = total_bytes
            .checked_add(encoded.len())
            .ok_or("snapshot chain size overflow")?;
        if encoded.len() > MAX_SNAPSHOT_DELTA_BYTES
            || total_bytes > MAX_SNAPSHOT_CHAIN_BYTES
            || digest(&encoded) != current
        {
            return Err("snapshot artifact hash or recovery budget mismatch".into());
        }
        let Some(artifact) = SnapshotArtifactV1::decode(&encoded)? else {
            fields(&encoded)?;
            snapshot = encoded;
            break;
        };
        if deltas.len() >= MAX_SNAPSHOT_DELTA_DEPTH {
            return Err("snapshot delta depth budget exceeded".into());
        }
        current = artifact.base_ref.clone();
        deltas.push(artifact);
    }
    let depth = deltas.len();
    for (index, artifact) in deltas.into_iter().rev().enumerate() {
        if artifact.depth != index + 1 {
            return Err("snapshot delta authenticated depth mismatch".into());
        }
        snapshot = artifact.delta.apply_bytes(&snapshot)?;
    }
    Ok((snapshot, depth))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
    struct Control {
        allocator: (u64, u64),
        pending: Vec<(u64, String)>,
        schedule: BTreeMap<String, u64>,
        original_result: Vec<u8>,
    }
    fn checkpoint() -> Control {
        Control {
            allocator: (u64::MAX, 7),
            pending: vec![],
            schedule: BTreeMap::new(),
            original_result: vec![1, 2, 255],
        }
    }
    fn insert(objects: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) -> String {
        let reference = digest(&bytes);
        objects.insert(reference.clone(), bytes);
        reference
    }
    #[test]
    fn complete_field_delta_preserves_original_nested_bytes_and_authenticates_chain() {
        let base = checkpoint();
        let raw = serde_cbor::to_vec(&base).unwrap();
        let mut objects = BTreeMap::new();
        let mut reference = insert(&mut objects, raw.clone());
        let mut current = base;
        for depth in 1..=MAX_SNAPSHOT_DELTA_DEPTH {
            let mut next = current.clone();
            next.allocator = (depth as u64, 8);
            next.pending = vec![(depth as u64, format!("preallocated-{depth}"))];
            next.schedule
                .insert("pure.instance.a".into(), depth as u64 + 9);
            let delta = SnapshotDeltaV1::between(&current, &next).unwrap();
            assert!(!delta.replacements.contains_key("original_result"));
            let artifact = SnapshotArtifactV1::new(reference, depth, delta).unwrap();
            reference = insert(&mut objects, artifact.encode().unwrap());
            current = next;
        }
        let read = |key: &str| objects.get(key).cloned().ok_or("missing object".into());
        let (bytes, depth) = resolve_snapshot_artifact_bytes_with_depth(&reference, &read).unwrap();
        assert_eq!(depth, 32);
        assert_eq!(bytes, serde_cbor::to_vec(&current).unwrap());
        let bad = SnapshotArtifactV1::new(
            reference.clone(),
            1,
            SnapshotDeltaV1::between(&current, &current).unwrap(),
        )
        .unwrap();
        let wrong_depth = insert(&mut objects, bad.encode().unwrap());
        assert!(
            resolve_snapshot_artifact_bytes(&wrong_depth, &|key| objects
                .get(key)
                .cloned()
                .ok_or("missing".into()))
            .is_err()
        );
        assert!(resolve_snapshot_artifact_bytes(&reference, &|_| Ok(raw.clone())).is_err());
        assert!(
            SnapshotArtifactV1::new(
                reference,
                33,
                SnapshotDeltaV1::between(&current, &current).unwrap()
            )
            .is_err()
        );
    }
    #[test]
    fn complete_field_delta_rejects_unknown_missing_duplicate_and_wrong_target() {
        let base = serde_cbor::to_vec(&checkpoint()).unwrap();
        let mut next = checkpoint();
        next.pending.push((7, "original-input".into()));
        let target = serde_cbor::to_vec(&next).unwrap();
        let delta = SnapshotDeltaV1::between_bytes(&base, &target).unwrap();
        let mut wrong = delta.clone();
        wrong.fields.pop();
        assert!(wrong.apply_bytes(&base).is_err());
        let mut wrong = delta.clone();
        wrong.replacements.clear();
        assert!(wrong.apply_bytes(&base).is_err());
        let mut wrong = delta;
        wrong.replacements.insert("unknown".into(), vec![0xf6]);
        assert!(wrong.apply_bytes(&base).is_err());
        assert!(
            SnapshotDeltaV1::between_bytes(&[0xa2, 0x61, b'a', 1, 0x61, b'a', 2], &target).is_err()
        );
    }
    #[test]
    fn replacement_decoder_rejects_hostile_declared_allocation_before_reserving() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(
                serialize_with = "serialize_replacements",
                deserialize_with = "deserialize_replacements"
            )]
            replacements: BTreeMap<String, Vec<u8>>,
        }
        let mut encoded = vec![0xa1, 0x6c];
        encoded.extend_from_slice(b"replacements");
        encoded.extend_from_slice(&[0xa1, 0x67]);
        encoded.extend_from_slice(b"pending");
        encoded.push(0x9b);
        encoded.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(serde_cbor::from_slice::<Wrapper>(&encoded).is_err());
        let empty = Wrapper {
            replacements: BTreeMap::new(),
        };
        assert!(empty.replacements.is_empty());
    }
}
