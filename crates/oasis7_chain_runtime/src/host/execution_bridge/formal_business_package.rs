//! Runtime interpretation of the shared profile-2 business envelope.
use super::controlled_capture as capture;
use oasis7::world_service::formal_result::FormalBusinessResultTreeV1;
use oasis7_distfs::controlled_authority::{
    formal::{
        CanonicalRecordV1, business_index::BusinessIndexNodeReader,
        business_material::FormalBusinessResultEnvelopeV1,
    },
    replicated_protocol::{ArtifactRole, ClosedRecord},
};
use oasis7_runtime::World;
pub(super) struct EmptyBusinessReader;
impl BusinessIndexNodeReader for EmptyBusinessReader {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
#[derive(Clone)]
pub(crate) struct SharedBusinessReader(
    pub(crate) std::sync::Arc<dyn BusinessIndexNodeReader + Send + Sync>,
);
impl BusinessIndexNodeReader for SharedBusinessReader {
    fn prefetch_path(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), String> {
        self.0.prefetch_path(root, key)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read_cached_node(digest)
    }
    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.0.cache_content_node(digest, bytes)
    }
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read_node(digest)
    }
}

pub(super) fn envelope(
    material: &ClosedRecord,
    required: bool,
) -> Result<Option<FormalBusinessResultEnvelopeV1>, String> {
    let reference = material
        .roots
        .get(&ArtifactRole::Result)
        .ok_or("formal Result artifact missing")?;
    let bytes = capture::object(material, reference)?;
    match FormalBusinessResultEnvelopeV1::decode(bytes) {
        Ok(envelope) => Ok(Some(envelope)),
        Err(error) if required => Err(error.to_string()),
        Err(_) => Ok(None),
    }
}

pub(super) fn tree(
    material: &ClosedRecord,
    record: &CanonicalRecordV1,
    world: &World,
    required: bool,
) -> Result<Option<FormalBusinessResultTreeV1>, String> {
    let Some(envelope) = envelope(material, required)? else {
        return Ok(None);
    };
    let tree = FormalBusinessResultTreeV1::decode_cbor(&envelope.tree_descriptor_cbor)?;
    tree.validate_world_boundary(world, record)?;
    if envelope.delta.new.canonical_result_root()? != tree.root()? {
        return Err("formal business descriptor differs from envelope delta".into());
    }
    Ok(Some(tree))
}

pub(super) fn execution_result<T: serde::de::DeserializeOwned>(
    material: &ClosedRecord,
    required: bool,
) -> Result<T, String> {
    if let Some(envelope) = envelope(material, required)? {
        capture::decode_generic(&envelope.execution_result_cbor)
    } else {
        capture::decode_role(material, ArtifactRole::Result)
    }
}
