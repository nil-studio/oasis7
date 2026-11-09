//! Structural keys for schema-owned generic fields. Dedicated field codecs
//! remain authoritative for their own key families. Parsing conveys no field
//! registration, semantic-key verification, authorization or commit capability.
use crate::incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Namespace;

const DOMAIN: &[u8] = b"oasis7.iws.world-field-key.v1\0";
pub const WORLD_FIELD_NAME_MAX_BYTES: usize = 128;
pub const WORLD_FIELD_KEY_MAX_BYTES: usize = DOMAIN.len() + 7 + WORLD_FIELD_NAME_MAX_BYTES + 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WorldFieldRecordKindV1 {
    Metadata = 1,
    Group = 2,
    Entry = 3,
    Bytes = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldFieldKeyV1 {
    bytes: [u8; WORLD_FIELD_KEY_MAX_BYTES],
    length: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldFieldKeyRefV1<'a> {
    pub owner: FieldOwner,
    pub namespace: Namespace,
    pub shape: FieldShape,
    pub kind: WorldFieldRecordKindV1,
    pub name: &'a str,
    /// None only for a Metadata record; all-zero member digests stay Some.
    pub semantic_key: Option<[u8; 32]>,
}

fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= WORLD_FIELD_NAME_MAX_BYTES
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}
fn owner_tag(owner: FieldOwner) -> u8 {
    match owner {
        FieldOwner::WorldState => 1,
        FieldOwner::Runtime => 2,
        FieldOwner::Cognition => 3,
    }
}
fn shape_tag(shape: FieldShape) -> Result<u8, &'static str> {
    Ok(match shape {
        FieldShape::Value => 1,
        FieldShape::Map => 2,
        FieldShape::NestedMap => 3,
        FieldShape::Set => 4,
        FieldShape::MapSet => 5,
        FieldShape::Ordered => 6,
        FieldShape::ByteMap => 7,
        FieldShape::History | FieldShape::Process | FieldShape::Children => {
            return Err("non-state world field shape");
        }
    })
}
fn combination(shape: FieldShape, kind: WorldFieldRecordKindV1) -> Result<(), &'static str> {
    shape_tag(shape)?;
    match kind {
        WorldFieldRecordKindV1::Group
            if !matches!(shape, FieldShape::NestedMap | FieldShape::MapSet) =>
        {
            Err("world field group shape")
        }
        WorldFieldRecordKindV1::Bytes if shape != FieldShape::ByteMap => {
            Err("world field bytes shape")
        }
        _ => Ok(()),
    }
}

impl WorldFieldKeyV1 {
    pub fn new(
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        semantic_key: Option<[u8; 32]>,
    ) -> Result<Self, &'static str> {
        let FieldDestination::State(namespace) = field.destination else {
            return Err("non-state world field destination");
        };
        if !name_valid(field.name) {
            return Err("world field name");
        }
        combination(field.shape, kind)?;
        if (kind == WorldFieldRecordKindV1::Metadata) != semantic_key.is_none() {
            return Err("world field semantic key presence");
        }
        let mut bytes = [0; WORLD_FIELD_KEY_MAX_BYTES];
        bytes[..DOMAIN.len()].copy_from_slice(DOMAIN);
        let start = DOMAIN.len();
        bytes[start] = owner_tag(field.owner);
        bytes[start + 1..start + 3].copy_from_slice(&(namespace as u16).to_be_bytes());
        bytes[start + 3] = shape_tag(field.shape)?;
        bytes[start + 4] = kind as u8;
        bytes[start + 5..start + 7].copy_from_slice(&(field.name.len() as u16).to_be_bytes());
        let end = start + 7 + field.name.len();
        bytes[start + 7..end].copy_from_slice(field.name.as_bytes());
        bytes[end..end + 32].copy_from_slice(&semantic_key.unwrap_or([0; 32]));
        Ok(Self {
            bytes,
            length: end + 32,
        })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    /// Decodes structural ownership only. Consumers must bind the registered
    /// descriptor, expected namespace and the value's original semantic key.
    pub fn from_bytes(bytes: &[u8]) -> Result<WorldFieldKeyRefV1<'_>, &'static str> {
        let start = DOMAIN.len();
        if bytes.len() < start + 8 + 32
            || bytes.len() > WORLD_FIELD_KEY_MAX_BYTES
            || !bytes.starts_with(DOMAIN)
        {
            return Err("world field key domain/length");
        }
        let owner = match bytes[start] {
            1 => FieldOwner::WorldState,
            2 => FieldOwner::Runtime,
            3 => FieldOwner::Cognition,
            _ => return Err("world field owner tag"),
        };
        let namespace = match u16::from_be_bytes(bytes[start + 1..start + 3].try_into().unwrap()) {
            1 => Namespace::Business,
            2 => Namespace::Allocator,
            3 => Namespace::Scheduler,
            4 => Namespace::Authorization,
            5 => Namespace::Effects,
            6 => Namespace::Artifacts,
            _ => return Err("world field namespace tag"),
        };
        let shape = match bytes[start + 3] {
            1 => FieldShape::Value,
            2 => FieldShape::Map,
            3 => FieldShape::NestedMap,
            4 => FieldShape::Set,
            5 => FieldShape::MapSet,
            6 => FieldShape::Ordered,
            7 => FieldShape::ByteMap,
            _ => return Err("world field shape tag"),
        };
        let kind = match bytes[start + 4] {
            1 => WorldFieldRecordKindV1::Metadata,
            2 => WorldFieldRecordKindV1::Group,
            3 => WorldFieldRecordKindV1::Entry,
            4 => WorldFieldRecordKindV1::Bytes,
            _ => return Err("world field record tag"),
        };
        combination(shape, kind)?;
        let name_len = usize::from(u16::from_be_bytes(
            bytes[start + 5..start + 7].try_into().unwrap(),
        ));
        if bytes.len() != start + 7 + name_len + 32 {
            return Err("world field key framing");
        }
        let end = start + 7 + name_len;
        let name =
            std::str::from_utf8(&bytes[start + 7..end]).map_err(|_| "world field name UTF-8")?;
        if !name_valid(name) {
            return Err("world field name");
        }
        let digest: [u8; 32] = bytes[end..].try_into().unwrap();
        let semantic_key = if kind == WorldFieldRecordKindV1::Metadata {
            if digest != [0; 32] {
                return Err("world field metadata key");
            }
            None
        } else {
            Some(digest)
        };
        Ok(WorldFieldKeyRefV1 {
            owner,
            namespace,
            shape,
            kind,
            name,
            semantic_key,
        })
    }
}

#[cfg(test)]
#[path = "incremental_world_field_key_tests.rs"]
mod tests;
