//! Shared original visual entity encoding for owned values and borrowed fields.

use super::{GeoPos, ModuleVisualAnchor, ModuleVisualEntity};
use serde::{Serialize, ser::SerializeStruct};

/// Borrowed anchor fields encoded with the original adjacent tag and data layout.
pub enum ModuleVisualAnchorProjectionDataV1<'a, T: Serialize + ?Sized> {
    Agent { agent_id: &'a T },
    Location { location_id: &'a T },
    Absolute { pos: &'a GeoPos },
}
/// Borrowed entity fields encoded in the original field order, including null labels.
pub struct ModuleVisualEntityProjectionFieldsV1<
    'a,
    T: Serialize + ?Sized,
    L: Serialize + ?Sized,
    A: Serialize + ?Sized,
> {
    pub entity_id: &'a T,
    pub module_id: &'a T,
    pub kind: &'a T,
    pub label: Option<&'a L>,
    pub anchor: &'a A,
}
impl<T: Serialize + ?Sized, L: Serialize + ?Sized, A: Serialize + ?Sized> Serialize
    for ModuleVisualEntityProjectionFieldsV1<'_, T, L, A>
{
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut s = s.serialize_struct("ModuleVisualEntity", 5)?;
        s.serialize_field("entity_id", self.entity_id)?;
        s.serialize_field("module_id", self.module_id)?;
        s.serialize_field("kind", self.kind)?;
        s.serialize_field("label", &self.label)?;
        s.serialize_field("anchor", self.anchor)?;
        s.end()
    }
}
impl<T: Serialize + ?Sized> Serialize for ModuleVisualAnchorProjectionDataV1<'_, T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        struct Tag(u32, &'static str);
        impl Serialize for Tag {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_unit_variant("ModuleVisualAnchor", self.0, self.1)
            }
        }
        struct Text<'a, T: Serialize + ?Sized> {
            variant: &'static str,
            name: &'static str,
            value: &'a T,
        }
        impl<T: Serialize + ?Sized> Serialize for Text<'_, T> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut s = s.serialize_struct(self.variant, 1)?;
                s.serialize_field(self.name, self.value)?;
                s.end()
            }
        }
        struct Pos<'a>(&'a GeoPos);
        impl Serialize for Pos<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut s = s.serialize_struct("absolute", 1)?;
                s.serialize_field("pos", self.0)?;
                s.end()
            }
        }
        let mut s = s.serialize_struct("ModuleVisualAnchor", 2)?;
        match self {
            Self::Agent { agent_id } => {
                s.serialize_field("type", &Tag(0, "agent"))?;
                s.serialize_field(
                    "data",
                    &Text {
                        variant: "agent",
                        name: "agent_id",
                        value: *agent_id,
                    },
                )?
            }
            Self::Location { location_id } => {
                s.serialize_field("type", &Tag(1, "location"))?;
                s.serialize_field(
                    "data",
                    &Text {
                        variant: "location",
                        name: "location_id",
                        value: *location_id,
                    },
                )?
            }
            Self::Absolute { pos } => {
                s.serialize_field("type", &Tag(2, "absolute"))?;
                s.serialize_field("data", &Pos(pos))?
            }
        }
        s.end()
    }
}
impl Serialize for ModuleVisualAnchor {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Agent { agent_id } => {
                ModuleVisualAnchorProjectionDataV1::Agent { agent_id }.serialize(s)
            }
            Self::Location { location_id } => {
                ModuleVisualAnchorProjectionDataV1::Location { location_id }.serialize(s)
            }
            Self::Absolute { pos } => {
                let value: ModuleVisualAnchorProjectionDataV1<'_, String> =
                    ModuleVisualAnchorProjectionDataV1::Absolute { pos };
                value.serialize(s)
            }
        }
    }
}
impl Serialize for ModuleVisualEntity {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            entity_id,
            module_id,
            kind,
            label,
            anchor,
        } = self;
        ModuleVisualEntityProjectionFieldsV1 {
            entity_id,
            module_id,
            kind,
            label: label.as_ref(),
            anchor,
        }
        .serialize(s)
    }
}

#[cfg(test)]
#[path = "world_entity_projection_tests.rs"]
mod tests;
