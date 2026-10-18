//! Exact native recovery bytes, externally stored under signed descriptor roots.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_NATIVE_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_NATIVE_ENVIRONMENT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_NATIVE_DESCRIPTOR_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeEnvironmentRole {
    Executable,
    LoadedLibrary,
    BuildMetadata,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvironmentChunkV1 {
    pub digest: String,
    pub byte_len: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvironmentComponentV1 {
    pub role: NativeEnvironmentRole,
    pub name: String,
    pub byte_len: u64,
    pub content_digest: String,
    #[serde(deserialize_with = "chunks")]
    pub chunks: Vec<NativeEnvironmentChunkV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionEnvironmentV1 {
    pub schema_version: u32,
    pub target: String,
    pub os: String,
    pub arch: String,
    #[serde(deserialize_with = "components")]
    pub components: Vec<NativeEnvironmentComponentV1>,
}
fn chunks<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<NativeEnvironmentChunkV1>, D::Error> {
    crate::controlled_authority::replicated_protocol::package::bounded_vec::<D, _, 4096>(d)
}
fn components<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<NativeEnvironmentComponentV1>, D::Error> {
    crate::controlled_authority::replicated_protocol::package::bounded_vec::<D, _, 256>(d)
}
fn hash(id: &str) -> Result<(), String> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("native content hash".into());
    }
    Ok(())
}
impl NativeExecutionEnvironmentV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.components.is_empty() || self.components.len() > 256 {
            return Err("native descriptor schema/component count".into());
        }
        for value in [&self.target, &self.os, &self.arch] {
            if value.is_empty() || value.len() > 256 {
                return Err("native platform metadata".into());
            }
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        let mut executable = 0;
        let mut metadata = false;
        for component in &self.components {
            if component.name.is_empty()
                || component.name.len() > 4096
                || !names.insert(&component.name)
                || component.chunks.is_empty()
                || component.chunks.len() > 4096
            {
                return Err("native component name/chunk count".into());
            }
            hash(&component.content_digest)?;
            executable += usize::from(component.role == NativeEnvironmentRole::Executable);
            metadata |= component.role == NativeEnvironmentRole::BuildMetadata;
            let mut size = 0u64;
            for (index, chunk) in component.chunks.iter().enumerate() {
                hash(&chunk.digest)?;
                if chunk.byte_len == 0
                    || chunk.byte_len as usize > MAX_NATIVE_CHUNK_BYTES
                    || (index + 1 < component.chunks.len()
                        && chunk.byte_len as usize != MAX_NATIVE_CHUNK_BYTES)
                {
                    return Err("native chunk length".into());
                }
                size = size
                    .checked_add(chunk.byte_len as u64)
                    .ok_or("native size overflow")?;
            }
            if size != component.byte_len {
                return Err("native component length".into());
            }
            total = total.checked_add(size).ok_or("native total overflow")?;
        }
        if executable != 1 || !metadata || total > MAX_NATIVE_ENVIRONMENT_BYTES {
            return Err("native executable/build metadata/environment budget".into());
        }
        if serde_cbor::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_NATIVE_DESCRIPTOR_BYTES
        {
            return Err("native descriptor byte budget".into());
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_cbor::to_vec(self).map_err(|e| e.to_string())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_NATIVE_DESCRIPTOR_BYTES {
            return Err("native descriptor byte budget".into());
        }
        let value: Self = serde_cbor::from_slice(bytes).map_err(|e| e.to_string())?;
        if value.encode()? != bytes {
            return Err("noncanonical native descriptor".into());
        }
        Ok(value)
    }
    pub fn digest(&self) -> Result<String, String> {
        Ok(blake3::hash(&self.encode()?).to_hex().to_string())
    }
    /// Reads one bounded chunk at a time; checks component and whole closure bytes.
    pub fn verify(
        &self,
        mut read: impl FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        self.validate()?;
        for component in &self.components {
            let mut hasher = blake3::Hasher::new();
            for chunk in &component.chunks {
                let bytes = read(&chunk.digest)?;
                if bytes.len() != chunk.byte_len as usize
                    || blake3::hash(&bytes).to_hex().as_str() != chunk.digest
                {
                    return Err("native chunk bytes/hash".into());
                }
                hasher.update(&bytes);
            }
            if hasher.finalize().to_hex().as_str() != component.content_digest {
                return Err("native component content hash".into());
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        NativeExecutionEnvironmentV1,
        std::collections::BTreeMap<String, Vec<u8>>,
    ) {
        let mut objects = std::collections::BTreeMap::new();
        let components = [
            (
                NativeEnvironmentRole::Executable,
                "exe",
                vec![3; MAX_NATIVE_CHUNK_BYTES + 17],
            ),
            (
                NativeEnvironmentRole::BuildMetadata,
                "Cargo.lock",
                b"exact original lockfile bytes".to_vec(),
            ),
        ]
        .into_iter()
        .map(|(role, name, bytes)| {
            let chunks = bytes
                .chunks(MAX_NATIVE_CHUNK_BYTES)
                .map(|chunk| {
                    let digest = blake3::hash(chunk).to_hex().to_string();
                    objects.insert(digest.clone(), chunk.to_vec());
                    NativeEnvironmentChunkV1 {
                        digest,
                        byte_len: chunk.len() as u32,
                    }
                })
                .collect();
            NativeEnvironmentComponentV1 {
                role,
                name: name.into(),
                byte_len: bytes.len() as u64,
                content_digest: blake3::hash(&bytes).to_hex().to_string(),
                chunks,
            }
        })
        .collect();
        (
            NativeExecutionEnvironmentV1 {
                schema_version: 1,
                target: "x86_64-unknown-linux-gnu".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
                components,
            },
            objects,
        )
    }
    #[test]
    fn native_exact_streamed_bytes_and_canonical_descriptor() {
        let (descriptor, objects) = fixture();
        assert_eq!(
            NativeExecutionEnvironmentV1::decode(&descriptor.encode().unwrap()).unwrap(),
            descriptor
        );
        descriptor
            .verify(|id| objects.get(id).cloned().ok_or("missing".into()))
            .unwrap();
        assert!(descriptor.verify(|_| Ok(vec![0])).is_err());
        let mut altered = descriptor.clone();
        altered.components[0].content_digest = "00".repeat(32);
        assert!(
            altered
                .verify(|id| objects.get(id).cloned().ok_or("missing".into()))
                .is_err()
        );
    }
    #[test]
    fn native_descriptor_requires_actual_executable_and_build_bytes() {
        let (mut descriptor, _) = fixture();
        descriptor.components.pop();
        assert!(descriptor.validate().is_err());
        let (mut descriptor, _) = fixture();
        descriptor.components[0].chunks[0].byte_len -= 1;
        assert!(descriptor.validate().is_err());
        let (mut descriptor, _) = fixture();
        descriptor.components[0].chunks[0].digest = "aa".repeat(33);
        assert!(descriptor.validate().is_err());
    }
}
