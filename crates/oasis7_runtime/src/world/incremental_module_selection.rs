//! Authenticated lookup content only. Record/manifest/artifact qualification
//! must follow before any invocation can be prepared or authorized.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_genesis_state as state,
    incremental_module_read::{ModuleReadContext, PreparedModuleInstanceFields},
};
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueStreamWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_module_instance_fields::{
        ModuleInstallTargetRefV1 as Target, ModuleInstanceFieldV1 as InstanceField,
        ModuleInstanceScalarRefV1 as InstanceScalar,
    },
    incremental_module_selection_fields::{
        ModuleSelectionFieldV1 as Field, ModuleSelectionKeyV1 as Key,
        ModuleSelectionScalarRefV1 as Scalar,
    },
};

enum Source {
    Instance(Box<ReservedValue<PreparedModuleInstanceFields>>),
    Global {
        version: ReservedValue<Vec<u8>>,
        target: Option<ReservedValue<Vec<u8>>>,
    },
}
/// Immutable, charged lookup result. This is not ActiveModuleInvocation.
pub struct PreparedModuleSelectionContent {
    invocation_id: String,
    source: Source,
}
impl PreparedModuleSelectionContent {
    pub fn invocation_id(&self) -> &str {
        &self.invocation_id
    }
    pub fn is_instance(&self) -> bool {
        matches!(self.source, Source::Instance(_))
    }
    pub fn instance_id(&self) -> &str {
        match &self.source {
            Source::Global { .. } => &self.invocation_id,
            Source::Instance(fields) => match fields.field(InstanceField::InstanceId) {
                InstanceScalar::Text(s) => s,
                _ => unreachable!("validated text"),
            },
        }
    }
    pub fn module_id(&self) -> &str {
        match &self.source {
            Source::Global { .. } => &self.invocation_id,
            Source::Instance(fields) => match fields.field(InstanceField::ModuleId) {
                InstanceScalar::Text(s) => s,
                _ => unreachable!("validated text"),
            },
        }
    }
    pub fn record_version(&self) -> &str {
        match &self.source {
            Source::Instance(fields) => match fields.field(InstanceField::Version) {
                InstanceScalar::Text(s) => s,
                _ => unreachable!("validated text"),
            },
            Source::Global { version, .. } => {
                match Scalar::from_canonical_bytes(version, Field::ActiveVersion)
                    .expect("immutable validated version")
                {
                    Scalar::ActiveVersion(s) => s,
                    _ => unreachable!("validated version"),
                }
            }
        }
    }
    pub fn target(&self) -> Target<'_> {
        match &self.source {
            Source::Instance(fields) => match fields.field(InstanceField::Target) {
                InstanceScalar::Target(t) => t,
                _ => unreachable!("validated target"),
            },
            Source::Global { target: None, .. } => Target::SelfAgent,
            Source::Global {
                target: Some(bytes),
                ..
            } => match Scalar::from_canonical_bytes(bytes, Field::Target)
                .expect("immutable validated target")
            {
                Scalar::Target(t) => t,
                _ => unreachable!("validated target"),
            },
        }
    }
}
impl<V: ValueNodeReader, C: FnMut() -> bool> ModuleReadContext<'_, V, C> {
    pub fn read_selection_field<R: MerkleNodeReader>(
        &mut self,
        module_id: &str,
        field: Field,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let key = Key::new(module_id, field).map_err(BodyMutationError::Invalid)?;
            let value = match txn.read(Ns::Artifacts, key.as_bytes())? {
                None => None,
                Some(id) => {
                    let bytes = self.load(&id)?;
                    Scalar::from_canonical_bytes(&bytes, field)
                        .map_err(BodyMutationError::Invalid)?;
                    Some(bytes)
                }
            };
            self.check(txn)?;
            Ok(value)
        })
    }
    /// Preserve legacy instance precedence and inactive-instance suppression.
    /// Parent qualification, record existence and complete invocation remain
    /// explicit caller obligations; no materialized cache is consulted.
    pub fn select_module_content<R: MerkleNodeReader>(
        &mut self,
        invocation_id: &str,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<PreparedModuleSelectionContent>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let identity =
                self.read_instance_field(invocation_id, InstanceField::InstanceId, txn)?;
            let source = if identity.is_some() {
                let active = self
                    .read_instance_field(invocation_id, InstanceField::Active, txn)?
                    .ok_or(BodyMutationError::Invalid("instance active field absent"))?;
                if InstanceScalar::from_canonical_bytes(&active, InstanceField::Active)
                    .map_err(BodyMutationError::Invalid)?
                    != InstanceScalar::Bool(true)
                {
                    self.check(txn)?;
                    return Ok(None);
                }
                // The row's retained metadata charge includes its complete
                // struct plus 128 bytes; moving it into this box stays funded.
                Source::Instance(Box::new(
                    self.read_instance(invocation_id, txn)?
                        .ok_or(BodyMutationError::Invalid("instance identity disappeared"))?,
                ))
            } else {
                if self.module_instance_count(invocation_id, txn)? > 0 {
                    self.check(txn)?;
                    return Ok(None);
                }
                let Some(version) =
                    self.read_selection_field(invocation_id, Field::ActiveVersion, txn)?
                else {
                    self.check(txn)?;
                    return Ok(None);
                };
                let target = self.read_selection_field(invocation_id, Field::Target, txn)?;
                Source::Global { version, target }
            };
            let charge = self
                .memory
                .reserve(
                    invocation_id
                        .len()
                        .checked_add(std::mem::size_of::<PreparedModuleSelectionContent>() + 128)
                        .ok_or(BodyMutationError::Invalid("module selection allocation"))?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let content = charge.retain(PreparedModuleSelectionContent {
                invocation_id: invocation_id.to_owned(),
                source,
            });
            self.check(txn)?;
            Ok(Some(content))
        })
    }
}
pub fn write_selection_scalar<W: ValueNodeWriter, C: FnMut() -> bool>(
    scalar: Scalar<'_>,
    context: &mut BodyMutationContext<'_, W, C>,
    max_bytes: u64,
) -> Result<[u8; 32], BodyMutationError> {
    let length = scalar.encoded_len().map_err(BodyMutationError::Invalid)?;
    if length > max_bytes {
        return Err(ValueTreeError::ResourceLimited("module selection scalar bytes").into());
    }
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        length,
        &mut context.cancelled,
    )?;
    if scalar.write_to(&mut writer).is_err() {
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("module selection scalar write"))
            .into());
    }
    Ok(writer.finish()?.digest()?)
}
pub fn contribute_module_selection_field<
    R: MerkleNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
>(
    module_id: &str,
    field: Field,
    after: Option<Scalar<'_>>,
    txn: &mut ExecutionTransaction<'_, R>,
    context: &mut BodyMutationContext<'_, W, C>,
    max_bytes: u64,
) -> Result<(), BodyMutationError> {
    txn.contribute(|txn| {
        if !txn.uses_budget(context.memory) {
            return Err(BodyMutationError::Invalid(
                "module selection transaction pool",
            ));
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let key = Key::new(module_id, field).map_err(BodyMutationError::Invalid)?;
        let value = match after {
            None => None,
            Some(scalar) => {
                if scalar.field() != field {
                    return Err(BodyMutationError::Invalid("module selection scalar type"));
                }
                Some(write_selection_scalar(scalar, context, max_bytes)?)
            }
        };
        txn.write(Ns::Artifacts, key.as_bytes(), value)?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
impl World {
    pub fn write_iws_module_selection_genesis<
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
    >(
        &self,
        mut root: StateRootV1,
        context: &mut BodyMutationContext<'_, W, C>,
        remaining_entries: &mut u64,
        max_scalar_bytes: u64,
    ) -> Result<ReservedValue<StateRootV1>, BodyMutationError> {
        let mut metadata = context
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if root.namespace != Ns::Artifacts {
            return Err(BodyMutationError::Invalid(
                "module selection genesis namespace",
            ));
        }
        let mut put = |id: &str, scalar: Scalar<'_>| {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            *remaining_entries =
                remaining_entries
                    .checked_sub(1)
                    .ok_or(ExecutionError::ResourceLimited(
                        "module selection genesis entries",
                    ))?;
            let key = Key::new(id, scalar.field()).map_err(BodyMutationError::Invalid)?;
            let value = write_selection_scalar(scalar, context, max_scalar_bytes)?;
            state::apply(
                &mut root,
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: None,
                    after: Some(value),
                }],
                context,
            )
        };
        for (id, version) in &self.module_registry.active {
            put(id, Scalar::ActiveVersion(version))?;
        }
        for (id, target) in &self.state.installed_module_targets {
            let target = match target {
                ModuleInstallTarget::SelfAgent => Target::SelfAgent,
                ModuleInstallTarget::LocationInfrastructure { location_id } => {
                    Target::LocationInfrastructure(location_id)
                }
            };
            put(id, Scalar::Target(target))?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(metadata
            .split(256)
            .map_err(BodyMutationError::Invalid)?
            .retain(root))
    }
}
