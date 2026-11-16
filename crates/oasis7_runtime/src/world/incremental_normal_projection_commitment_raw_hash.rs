//! Original three-field legacy semantic State hash over exact emitted
//! typed fragments. This DATA helper cannot mint a Current or sealed owner.
use super::*;
use serde::{
    Serialize, Serializer,
    ser::{Error as _, SerializeStruct},
};
use std::{cell::RefCell, mem::size_of};
pub(super) struct PaidOriginalStateHashDataV1 {
    hash: String,
    _fee: MemoryReservation,
    core: WorkIdentityV1,
}
impl PaidOriginalStateHashDataV1 {
    pub(super) fn as_str(&self) -> &str {
        &self.hash
    }
    pub(super) fn validate_binding(
        &self,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        if self.core != core || !self._fee.uses_budget(memory) {
            return Err(invalid("original State hash context"));
        }
        Ok(())
    }
    pub(super) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(invalid("original State hash data context"));
        }
        Ok(())
    }
}
struct StateFragments<'a, 'b, 'c> {
    columns: &'a NormalProjectionColumnSeedDataV1,
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    first: &'a RefCell<Option<BodyMutationError>>,
}
impl Serialize for StateFragments<'_, '_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct(
            "WorldState",
            self.columns
                .original_state_declared_fields()
                .ok_or_else(|| S::Error::custom("original State emitter missing"))?,
        )?;
        for field in self.columns.original_state_emissions() {
            if self.first.borrow().is_some() {
                return Err(S::Error::custom("original State meter poisoned"));
            }
            let bytes = field.original_json();
            let result = self
                .common
                .borrow_mut()
                .charge(bytes.len() as u64, bytes.len() as u64);
            if let Err(error) = result {
                *self.first.borrow_mut() = Some(paid(error));
                return Err(S::Error::custom("original State meter stopped"));
            }
            let raw: &serde_json::value::RawValue =
                serde_json::from_slice(bytes).map_err(S::Error::custom)?;
            let result = self.common.borrow_mut().consume_steps(0);
            if let Err(error) = result {
                *self.first.borrow_mut() = Some(paid(error));
                return Err(S::Error::custom("original State meter stopped"));
            }
            output.serialize_field(field.name(), raw)?;
        }
        output.end()
    }
}
#[derive(Debug)]
struct PaidRawStateCodecError {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Display for PaidRawStateCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for PaidRawStateCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
struct Preimage<'a, T: Serialize + ?Sized> {
    state: &'a T,
    manifest: &'a str,
    policy: &'a str,
}
impl<T: Serialize + ?Sized> Serialize for Preimage<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        oasis7_world_state::codec::serialize_state_root_projection_v1(
            self.state,
            self.manifest,
            self.policy,
            serializer,
        )
    }
}
/// GEN/cold initialization data only. The actual Current transition producer
/// must apply and cover all genuine TX changes before minting a normal owner;
/// callers cannot turn this initial hash into that owner or detach its fee.
pub(super) fn hash_original_emitted_state_data_v1(
    columns: &NormalProjectionColumnSeedDataV1,
    manifest_hash: &str,
    policy_hash: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidOriginalStateHashDataV1, BodyMutationError> {
    columns.validate_context(common)?;
    let core = common.work().identity();
    let n = size_of::<PaidOriginalStateHashDataV1>() + 64;
    common.charge(1, n as u64).map_err(paid)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    common.charge(1, 32768).map_err(paid)?;
    let error_phase = common
        .memory()
        .reserve(32768, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let shared = RefCell::new(common);
    let first = RefCell::new(None);
    let state = StateFragments {
        columns,
        common: &shared,
        first: &first,
    };
    let image = Preimage {
        state: &state,
        manifest: manifest_hash,
        policy: policy_hash,
    };
    let result = oasis7_world_state::util::hash_json_with_work_v1(&image, &mut |steps, bytes| {
        shared.borrow_mut().charge(steps, bytes)
    });
    if let Some(error) = first.into_inner() {
        return Err(error);
    }
    let hash = match result {
        Ok(hash) => hash,
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Work(error)) => return Err(paid(error)),
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Codec(error)) => {
            return Err(paid(PaidRawStateCodecError {
                error,
                _fee: error_phase,
            }));
        }
    };
    Ok(PaidOriginalStateHashDataV1 {
        hash,
        _fee: fee,
        core,
    })
}

/// Hash the actual resident transition with the genuine Current manifest and
/// ordered PolicySet sources. Source checks precede the shared old JSON/SHA
/// kernels; neither string input nor RootSet can stand in for these sources.
fn hash_current_state_columns_inner_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
>(
    columns: &super::state_columns::OriginalStateColumnsDataV1,
    manifest_hash: &str,
    policy:&crate::world::incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<PaidOriginalStateHashDataV1, BodyMutationError> {
    policy.validate_current(tx, common, live)?;
    let core = common.work().identity();
    common
        .charge(1, (size_of::<String>() + 64) as u64)
        .map_err(paid)?;
    let _policy_fee = common
        .memory()
        .reserve(size_of::<String>() + 64, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    common.charge(1, 32768).map_err(paid)?;
    let error_phase = common
        .memory()
        .reserve(32768, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let policy_hash = match oasis7_world_state::util::hash_json_with_work_v1(
        policy.value(),
        &mut |steps, bytes| common.charge(steps, bytes),
    ) {
        Ok(hash) => hash,
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Work(error)) => return Err(paid(error)),
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Codec(error)) => {
            return Err(paid(PaidRawStateCodecError {
                error,
                _fee: error_phase,
            }));
        }
    };
    policy.validate_current(tx, common, live)?;
    let n = size_of::<PaidOriginalStateHashDataV1>() + 64;
    common.charge(1, n as u64).map_err(paid)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let shared = RefCell::new(common);
    let first = RefCell::new(None);
    let state = columns.serialization_v1(&shared, &first);
    let image = Preimage {
        state: &state,
        manifest: manifest_hash,
        policy: &policy_hash,
    };
    let result = oasis7_world_state::util::hash_json_with_work_v1(&image, &mut |steps, bytes| {
        shared.borrow_mut().charge(steps, bytes)
    });
    if let Some(error) = first.into_inner() {
        return Err(error);
    }
    let hash = match result {
        Ok(hash) => hash,
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Work(error)) => return Err(paid(error)),
        Err(oasis7_world_state::util::JsonHashWorkErrorV1::Codec(error)) => {
            return Err(paid(PaidRawStateCodecError {
                error,
                _fee: error_phase,
            }));
        }
    };
    Ok(PaidOriginalStateHashDataV1 {
        hash,
        _fee: fee,
        core,
    })
}

/// Original terminal consumer keeps its strict provenance gate and delegates
/// the very same three-field JSON/SHA kernel.
pub(super) fn hash_current_state_columns_data_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
>(
    columns: &super::state_columns::OriginalStateColumnsDataV1,
    terminal: &crate::world::incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    policy:&crate::world::incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<PaidOriginalStateHashDataV1, BodyMutationError> {
    columns.validate_context(common)?;
    terminal.validate_provenance(tx, common, live)?;
    hash_current_state_columns_inner_v1(columns, terminal.manifest_hash(), policy, tx, common, live)
}
pub(super) fn hash_current_state_columns_with_manifest_data_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
>(
    columns: &super::state_columns::OriginalStateColumnsDataV1,
    manifest: &ReservedValue<
        crate::world::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
    >,
    policy:&crate::world::incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    parent: &crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,
    tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<PaidOriginalStateHashDataV1, BodyMutationError> {
    columns.validate_context(common)?;
    // The private Current producer admits only Ordinary sources and retains
    // its Ordinary carrier; ReservedValue itself exposes only pool identity.
    if !manifest.uses_budget(common.memory()) {
        return Err(invalid("actual Current manifest outer source pool"));
    }
    manifest.validate_current_read_source_v1(parent, tx, common, live)?;
    let value =
        hash_current_state_columns_inner_v1(columns, manifest.hash(), policy, tx, common, live)?;
    manifest.validate_current_read_source_v1(parent, tx, common, live)?;
    Ok(value)
}
