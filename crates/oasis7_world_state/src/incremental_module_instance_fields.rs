//! Independently addressed instance metadata; opaque reducer state is stored
//! separately. These fields do not authorize installation or invocation.
use oasis7_agent_api::world_action::ModuleInstallTarget;
use std::io::{self, Write};
const KEY: &[u8] = b"oasis7.iws.module-instance-key.v1\0";
const VALUE: &[u8] = b"oasis7.iws.module-instance-value.v1\0";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ModuleInstanceFieldV1 {
    InstanceId = 1,
    ModuleId = 2,
    Version = 3,
    WasmHash = 4,
    Owner = 5,
    Target = 6,
    Active = 7,
    InstalledAt = 8,
}
impl ModuleInstanceFieldV1 {
    pub const ALL: [Self; 8] = [
        Self::InstanceId,
        Self::ModuleId,
        Self::Version,
        Self::WasmHash,
        Self::Owner,
        Self::Target,
        Self::Active,
        Self::InstalledAt,
    ];
    pub fn from_tag(tag: u8) -> Result<Self, &'static str> {
        Self::ALL
            .into_iter()
            .find(|f| *f as u8 == tag)
            .ok_or("module instance field tag")
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::InstanceId => "module_instances.instance_id",
            Self::ModuleId => "module_instances.module_id",
            Self::Version => "module_instances.module_version",
            Self::WasmHash => "module_instances.wasm_hash",
            Self::Owner => "module_instances.owner_agent_id",
            Self::Target => "module_instances.install_target",
            Self::Active => "module_instances.active",
            Self::InstalledAt => "module_instances.installed_at",
        }
    }
}
pub struct ModuleInstanceKeyV1 {
    bytes: [u8; 1024],
    length: usize,
}
impl ModuleInstanceKeyV1 {
    pub fn new(map_key: &str, field: ModuleInstanceFieldV1) -> Result<Self, &'static str> {
        if map_key.len() > 1024 - KEY.len() - 3 {
            return Err("module instance key length");
        }
        let mut bytes = [0; 1024];
        bytes[..KEY.len()].copy_from_slice(KEY);
        bytes[KEY.len()..KEY.len() + 2].copy_from_slice(&(map_key.len() as u16).to_be_bytes());
        let length = KEY.len() + 3 + map_key.len();
        bytes[KEY.len() + 2..length - 1].copy_from_slice(map_key.as_bytes());
        bytes[length - 1] = field as u8;
        Ok(Self { bytes, length })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
    pub fn is_instance_key(bytes: &[u8]) -> bool {
        bytes.starts_with(KEY)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<(&str, ModuleInstanceFieldV1), &'static str> {
        if bytes.len() < KEY.len() + 3 || bytes.len() > 1024 || !bytes.starts_with(KEY) {
            return Err("module instance key domain/length");
        }
        let length =
            u16::from_be_bytes(bytes[KEY.len()..KEY.len() + 2].try_into().unwrap()) as usize;
        if bytes.len() != KEY.len() + 3 + length {
            return Err("module instance key framing");
        }
        Ok((
            std::str::from_utf8(&bytes[KEY.len() + 2..bytes.len() - 1])
                .map_err(|_| "module instance key UTF-8")?,
            ModuleInstanceFieldV1::from_tag(bytes[bytes.len() - 1])?,
        ))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ModuleInstallTargetRefV1<'a> {
    SelfAgent,
    LocationInfrastructure(&'a str),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ModuleInstanceScalarRefV1<'a> {
    Text(&'a str),
    Target(ModuleInstallTargetRefV1<'a>),
    Bool(bool),
    U64(u64),
}
impl ModuleInstanceScalarRefV1<'_> {
    fn validate(self, field: ModuleInstanceFieldV1) -> Result<(), &'static str> {
        use ModuleInstanceFieldV1 as F;
        if matches!(
            (field, self),
            (
                F::InstanceId | F::ModuleId | F::Version | F::WasmHash | F::Owner,
                Self::Text(_)
            ) | (F::Target, Self::Target(_))
                | (F::Active, Self::Bool(_))
                | (F::InstalledAt, Self::U64(_))
        ) {
            Ok(())
        } else {
            Err("module instance scalar type")
        }
    }
    pub fn encoded_len(self, field: ModuleInstanceFieldV1) -> Result<u64, &'static str> {
        self.validate(field)?;
        let length = match self {
            Self::Text(s) => 8u64.checked_add(s.len() as u64),
            Self::Target(ModuleInstallTargetRefV1::SelfAgent) => Some(1),
            Self::Target(ModuleInstallTargetRefV1::LocationInfrastructure(s)) => {
                9u64.checked_add(s.len() as u64)
            }
            Self::Bool(_) => Some(1),
            Self::U64(_) => Some(8),
        };
        length
            .and_then(|n| n.checked_add(VALUE.len() as u64 + 1))
            .ok_or("module instance scalar length")
    }
    pub fn write_to(self, field: ModuleInstanceFieldV1, out: &mut impl Write) -> io::Result<()> {
        self.validate(field).map_err(io::Error::other)?;
        out.write_all(VALUE)?;
        out.write_all(&[field as u8])?;
        let text = |out: &mut dyn Write, s: &str| {
            out.write_all(&(s.len() as u64).to_be_bytes())?;
            out.write_all(s.as_bytes())
        };
        match self {
            Self::Text(s) => text(out, s),
            Self::Target(ModuleInstallTargetRefV1::SelfAgent) => out.write_all(&[0]),
            Self::Target(ModuleInstallTargetRefV1::LocationInfrastructure(s)) => {
                out.write_all(&[1])?;
                text(out, s)
            }
            Self::Bool(b) => out.write_all(&[u8::from(b)]),
            Self::U64(n) => out.write_all(&n.to_be_bytes()),
        }
    }
}
impl<'a> ModuleInstanceScalarRefV1<'a> {
    pub fn from_canonical_bytes(
        bytes: &'a [u8],
        field: ModuleInstanceFieldV1,
    ) -> Result<Self, &'static str> {
        let mut bytes = bytes
            .strip_prefix(VALUE)
            .ok_or("module instance scalar domain")?;
        if take(&mut bytes, 1)?[0] != field as u8 {
            return Err("module instance scalar field");
        }
        use ModuleInstanceFieldV1 as F;
        let value = match field {
            F::InstanceId | F::ModuleId | F::Version | F::WasmHash | F::Owner => {
                Self::Text(text(&mut bytes)?)
            }
            F::Target => Self::Target(match take(&mut bytes, 1)?[0] {
                0 => ModuleInstallTargetRefV1::SelfAgent,
                1 => ModuleInstallTargetRefV1::LocationInfrastructure(text(&mut bytes)?),
                _ => return Err("module instance target tag"),
            }),
            F::Active => Self::Bool(match take(&mut bytes, 1)?[0] {
                0 => false,
                1 => true,
                _ => return Err("module instance active tag"),
            }),
            F::InstalledAt => {
                Self::U64(u64::from_be_bytes(take(&mut bytes, 8)?.try_into().unwrap()))
            }
        };
        if !bytes.is_empty() {
            return Err("module instance scalar trailing");
        }
        Ok(value)
    }
}
fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Result<&'a [u8], &'static str> {
    let (value, rest) = bytes
        .split_at_checked(len)
        .ok_or("module instance scalar truncated")?;
    *bytes = rest;
    Ok(value)
}
fn text<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, &'static str> {
    let n = usize::try_from(u64::from_be_bytes(take(bytes, 8)?.try_into().unwrap()))
        .map_err(|_| "module instance scalar length")?;
    std::str::from_utf8(take(bytes, n)?).map_err(|_| "module instance scalar UTF-8")
}

/// Exhaustive borrowing keeps schema additions visible to the storage owner.
pub fn visit_module_instance_fields<'a, E>(
    instance: &'a crate::ModuleInstanceState,
    mut visit: impl FnMut(ModuleInstanceFieldV1, ModuleInstanceScalarRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use ModuleInstanceFieldV1 as F;
    use ModuleInstanceScalarRefV1 as S;
    let crate::ModuleInstanceState {
        instance_id,
        module_id,
        module_version,
        wasm_hash,
        owner_agent_id,
        install_target,
        active,
        installed_at,
    } = instance;
    for (field, value) in [
        (F::InstanceId, S::Text(instance_id)),
        (F::ModuleId, S::Text(module_id)),
        (F::Version, S::Text(module_version)),
        (F::WasmHash, S::Text(wasm_hash)),
        (F::Owner, S::Text(owner_agent_id)),
    ] {
        visit(field, value)?;
    }
    let target = match install_target {
        ModuleInstallTarget::SelfAgent => ModuleInstallTargetRefV1::SelfAgent,
        ModuleInstallTarget::LocationInfrastructure { location_id } => {
            ModuleInstallTargetRefV1::LocationInfrastructure(location_id)
        }
    };
    visit(F::Target, S::Target(target))?;
    visit(F::Active, S::Bool(*active))?;
    visit(F::InstalledAt, S::U64(*installed_at))
}

/// Borrowed semantic view, not the frozen scalar wire.
pub fn visit_instances_projection<S: crate::incremental_projection::GenesisFieldVisitor>(
    instances: &std::collections::BTreeMap<String, crate::ModuleInstanceState>,
    sink: &mut S,
) -> Result<(), crate::incremental_projection::ProjectionError<S::Error>> {
    use crate::incremental_projection::*;
    let descriptor = |field: ModuleInstanceFieldV1| FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: field.name(),
        destination: FieldDestination::State(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        ),
        shape: FieldShape::Map,
    };
    for field in ModuleInstanceFieldV1::ALL {
        sink.begin(descriptor(field), Some(instances.len()))
            .map_err(ProjectionError::Sink)?;
    }
    for (key, instance) in instances {
        visit_module_instance_fields(instance, |field, value| {
            sink.entry(descriptor(field), key, &value)
        })
        .map_err(ProjectionError::Sink)?;
    }
    for field in ModuleInstanceFieldV1::ALL {
        sink.end(descriptor(field)).map_err(ProjectionError::Sink)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_module_instance_field_tests.rs"]
mod tests;
