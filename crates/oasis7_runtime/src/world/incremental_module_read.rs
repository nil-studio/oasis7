//! Point reads for the module owner. Manifest/artifact/authorization selection
//! and the full tick route remain separate obligations.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1 as Scalar,
    },
    incremental_module_instance_index::*,
};

pub struct ModuleReadContext<'a, V, C> {
    pub qualification: &'a ReservedValue<
        super::incremental_module_instance_family::ModuleInstanceFamilyQualificationV1,
    >,
    pub values: &'a V,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub max_value_bytes: u64,
    pub cancelled: C,
}
/// Content only, not an invocation/authority capability. The transaction keeps
/// the authenticated reads; borrowed getters never detach charged buffers.
pub struct PreparedModuleInstanceFields {
    fields: [ReservedValue<Vec<u8>>; 8],
}
impl PreparedModuleInstanceFields {
    /// Clone only this actual eight-column source after admitting its exact
    /// owned text graph. The result is DATA, never an instance permission.
    // Only the future scheduled-invocation consumer can exercise this staged
    // read-only DATA prerequisite; unit tests exercise it until that split lands.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "scheduled invocation remains in a separate L2 slice")
    )]
    pub(in crate::world) fn snapshot_paid_v1(
        &self,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<oasis7_world_state::ModuleInstanceState>, BodyMutationError> {
        use oasis7_world_state::incremental_module_instance_fields::ModuleInstallTargetRefV1 as T;
        if self
            .fields
            .iter()
            .any(|field| !field.uses_budget(common.memory()))
        {
            return Err(BodyMutationError::Invalid("instance clone source pool"));
        }
        fn text(source: &PreparedModuleInstanceFields, field: Field) -> &str {
            match source.field(field) {
                Scalar::Text(value) => value,
                _ => unreachable!("closed instance text"),
            }
        }
        let target = match self.field(Field::Target) {
            Scalar::Target(value) => value,
            _ => unreachable!("closed instance target"),
        };
        let mut heap = std::mem::size_of::<oasis7_world_state::ModuleInstanceState>();
        for field in [
            Field::InstanceId,
            Field::ModuleId,
            Field::Version,
            Field::WasmHash,
            Field::Owner,
        ] {
            heap = heap
                .checked_add(text(self, field).len())
                .ok_or(BodyMutationError::Invalid("instance clone layout"))?;
        }
        if let T::LocationInfrastructure(value) = target {
            heap = heap
                .checked_add(value.len())
                .ok_or(BodyMutationError::Invalid("instance target clone layout"))?;
        }
        common
            .charge(
                u64::try_from(
                    heap.checked_mul(3)
                        .and_then(|n| n.checked_add(512))
                        .ok_or(BodyMutationError::Invalid("instance clone work"))?,
                )
                .map_err(|_| BodyMutationError::Invalid("instance clone width"))?,
                u64::try_from(
                    heap.checked_mul(3)
                        .ok_or(BodyMutationError::Invalid("instance clone work"))?,
                )
                .map_err(|_| BodyMutationError::Invalid("instance clone width"))?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let install_target = match target {
            T::SelfAgent => oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent,
            T::LocationInfrastructure(value) => {
                oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                    location_id: value.to_owned(),
                }
            }
        };
        let active = match self.field(Field::Active) {
            Scalar::Bool(value) => value,
            _ => unreachable!("closed instance active"),
        };
        let installed_at = match self.field(Field::InstalledAt) {
            Scalar::U64(value) => value,
            _ => unreachable!("closed instance time"),
        };
        Ok(fee.retain(oasis7_world_state::ModuleInstanceState {
            instance_id: text(self, Field::InstanceId).to_owned(),
            module_id: text(self, Field::ModuleId).to_owned(),
            module_version: text(self, Field::Version).to_owned(),
            wasm_hash: text(self, Field::WasmHash).to_owned(),
            owner_agent_id: text(self, Field::Owner).to_owned(),
            install_target,
            active,
            installed_at,
        }))
    }
    pub fn field(&self, field: Field) -> Scalar<'_> {
        Scalar::from_canonical_bytes(&self.fields[field as usize - 1], field)
            .expect("immutable validated module field")
    }
}
impl<V: ValueNodeReader, C: FnMut() -> bool> ModuleReadContext<'_, V, C> {
    pub(super) fn check<R: MerkleNodeReader>(
        &mut self,
        txn: &ExecutionTransaction<'_, R>,
    ) -> Result<(), BodyMutationError> {
        if !txn.uses_budget(self.memory) {
            return Err(BodyMutationError::Invalid("module read transaction pool"));
        }
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        super::incremental_module_instance_family::binding(self.qualification, txn, self.memory)?;
        Ok(())
    }
    pub(super) fn load(
        &mut self,
        id: &[u8; 32],
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        self.load_with_limit(id, self.max_value_bytes)
    }
    pub(super) fn load_with_limit(
        &mut self,
        id: &[u8; 32],
        max: u64,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        let mut reader = ValueReadContext {
            source: self.values,
            memory: self.memory,
            class: MemoryClass::Ordinary,
            io: self.io,
        };
        let root = reader.load_root(id)?;
        if root.0.logical_len > self.max_value_bytes.min(max) {
            return Err(ValueTreeError::ResourceLimited("module value bytes").into());
        }
        let length = usize::try_from(root.0.logical_len)
            .map_err(|_| ValueTreeError::ResourceLimited("module value allocation"))?;
        let charge = self
            .memory
            .reserve(
                length.checked_add(128).ok_or(BodyMutationError::Invalid(
                    "module value allocation overflow",
                ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut bytes = vec![0; length];
        if length == 0 {
            reader.verify_closure(id, 0, &mut self.cancelled)?;
        } else {
            reader.read_range(
                &root,
                0,
                root.0.logical_len,
                &mut bytes.as_mut_slice(),
                &mut self.cancelled,
            )?;
        }
        Ok(charge.retain(bytes))
    }
    pub(super) fn instance_index_total<R: MerkleNodeReader>(
        &mut self,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<u64, BodyMutationError> {
        self.check(txn)?;
        let id = txn
            .read(Ns::Business, MODULE_INSTANCE_INDEX_KEY_V1)?
            .ok_or(BodyMutationError::Invalid(
                "module instance count index absent",
            ))?;
        let value = self.load(&id)?;
        module_instance_index_from_bytes(&value).map_err(BodyMutationError::Invalid)
    }
    /// All stored instances count, including inactive ones. A missing index is
    /// not zero. The parent owner must establish count/field consistency.
    pub fn module_instance_count<R: MerkleNodeReader>(
        &mut self,
        module_id: &str,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<u64, BodyMutationError> {
        txn.contribute(|txn| {
            let total = self.instance_index_total(txn)?;
            let key = module_instance_count_key(module_id);
            let count = match txn.read(Ns::Business, &key)? {
                None => 0,
                Some(id) => {
                    let value = self.load(&id)?;
                    module_instance_count_from_bytes(&value).map_err(BodyMutationError::Invalid)?
                }
            };
            if count > total {
                return Err(BodyMutationError::Invalid(
                    "module instance count exceeds total",
                ));
            }
            self.check(txn)?;
            Ok(count)
        })
    }
    /// Read only the requested scalar, e.g. Active before loading invocation
    /// metadata. Parent qualification owns complete-row/absence invariants.
    pub fn read_instance_field<R: MerkleNodeReader>(
        &mut self,
        map_key: &str,
        field: Field,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let key =
                ModuleInstanceKeyV1::new(map_key, field).map_err(BodyMutationError::Invalid)?;
            let Some(id) = txn.read(Ns::Business, key.as_bytes())? else {
                self.check(txn)?;
                return Ok(None);
            };
            let value = self.load(&id)?;
            Scalar::from_canonical_bytes(&value, field).map_err(BodyMutationError::Invalid)?;
            self.check(txn)?;
            Ok(Some(value))
        })
    }
    /// Exact stored instance fields, without resolving registry fallback. The
    /// qualified parent must establish complete eight-field instance rows.
    pub fn read_instance<R: MerkleNodeReader>(
        &mut self,
        map_key: &str,
        txn: &mut ExecutionTransaction<'_, R>,
    ) -> Result<Option<ReservedValue<PreparedModuleInstanceFields>>, BodyMutationError> {
        txn.contribute(|txn| {
            self.check(txn)?;
            let mut workspace = self
                .memory
                .reserve(16 * 1024, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let identity = ModuleInstanceKeyV1::new(map_key, Field::InstanceId)
                .map_err(BodyMutationError::Invalid)?;
            let Some(first) = txn.read(Ns::Business, identity.as_bytes())? else {
                self.check(txn)?;
                return Ok(None);
            };
            let mut fields = Vec::with_capacity(8);
            for field in Field::ALL {
                self.check(txn)?;
                let id = if field == Field::InstanceId {
                    first
                } else {
                    let key = ModuleInstanceKeyV1::new(map_key, field)
                        .map_err(BodyMutationError::Invalid)?;
                    txn.read(Ns::Business, key.as_bytes())?
                        .ok_or(BodyMutationError::Invalid("incomplete module instance row"))?
                };
                let value = self.load(&id)?;
                Scalar::from_canonical_bytes(&value, field).map_err(BodyMutationError::Invalid)?;
                fields.push(value);
            }
            let fields = fields
                .try_into()
                .map_err(|_| BodyMutationError::Invalid("module instance field count"))?;
            let charge = workspace
                .split(std::mem::size_of::<PreparedModuleInstanceFields>() + 128)
                .map_err(BodyMutationError::Invalid)?;
            self.check(txn)?;
            Ok(Some(charge.retain(PreparedModuleInstanceFields { fields })))
        })
    }
}

#[cfg(test)]
mod paid_snapshot_tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
        FormalBoundedContextV1, FormalBoundedWorkV1,
    };
    use oasis7_world_state::incremental_module_instance_fields::ModuleInstallTargetRefV1 as Target;

    fn fields(memory: &MemoryBudget, location: bool) -> PreparedModuleInstanceFields {
        let rows = [
            Scalar::Text("instance-a"),
            Scalar::Text("module-a"),
            Scalar::Text("1.0"),
            Scalar::Text("verified-wasm"),
            Scalar::Text("owner-a"),
            Scalar::Target(if location {
                Target::LocationInfrastructure("site-a")
            } else {
                Target::SelfAgent
            }),
            Scalar::Bool(true),
            Scalar::U64(42),
        ];
        PreparedModuleInstanceFields {
            fields: std::array::from_fn(|i| {
                let field = Field::ALL[i];
                let mut encoded = Vec::new();
                rows[i].write_to(field, &mut encoded).unwrap();
                memory
                    .reserve(encoded.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(encoded)
            }),
        }
    }

    #[test]
    fn snapshot_retains_charged_typed_data_and_original_field_values() {
        for location in [false, true] {
            let memory = MemoryBudget::new(1 << 20, 0).unwrap();
            let source = fields(&memory, location);
            let before = memory.used();
            let work = FormalBoundedWorkV1::new(1_000_000, 1_000_000).unwrap();
            let mut stop = || false;
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let snapshot = source.snapshot_paid_v1(&mut common).unwrap();
            assert!(snapshot.uses_budget(&memory));
            assert!(memory.used() > before);
            assert_eq!(snapshot.instance_id, "instance-a");
            assert_eq!(snapshot.module_id, "module-a");
            assert_eq!(snapshot.module_version, "1.0");
            assert_eq!(snapshot.wasm_hash, "verified-wasm");
            assert_eq!(snapshot.owner_agent_id, "owner-a");
            assert!(snapshot.active);
            assert_eq!(snapshot.installed_at, 42);
            assert_eq!(
                snapshot.install_target,
                if location {
                    oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                        location_id: "site-a".into()
                    }
                } else {
                    oasis7_agent_api::world_action::ModuleInstallTarget::SelfAgent
                }
            );
            drop(snapshot);
            assert_eq!(memory.used(), before);
        }
    }

    #[test]
    fn cross_pool_and_cancellation_fail_before_cloning() {
        let source_pool = MemoryBudget::new(1 << 20, 0).unwrap();
        let other_pool = MemoryBudget::new(1 << 20, 0).unwrap();
        let source = fields(&source_pool, false);
        let work = FormalBoundedWorkV1::new(1_000_000, 1_000_000).unwrap();
        let mut stop = || false;
        let mut other = FormalBoundedContextV1::new(&other_pool, &work, &mut stop);
        assert!(matches!(
            source.snapshot_paid_v1(&mut other),
            Err(BodyMutationError::Invalid("instance clone source pool"))
        ));
        assert_eq!(other_pool.used(), 0);

        let before = source_pool.used();
        let cancelled_work = FormalBoundedWorkV1::new(1_000_000, 1_000_000).unwrap();
        let mut cancelled = || true;
        let mut context =
            FormalBoundedContextV1::new(&source_pool, &cancelled_work, &mut cancelled);
        assert!(source.snapshot_paid_v1(&mut context).is_err());
        assert_eq!(source_pool.used(), before);
    }
}
