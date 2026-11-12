//! Original authority hash projection, without materializing correlation results.
//! This is paid source data. It is not complete revocation recovery or authority.
use super::*;
use crate::incremental_typed_record_codec::{TypedRecordWriteWorkV1, take_named};
use oasis7_proto::memory_budget::ReservedValue;

/// The original DTO serializer sees a real empty correlation map, exactly as
/// the legacy `clone(); world_service_results.clear()` authority projection.
/// All other fields use their original typed codecs and Serialize behavior.
#[repr(transparent)]
pub struct AuthorityRevocationProjectionV1(crate::CapabilityRevocationState);
impl serde::Serialize for AuthorityRevocationProjectionV1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.0, serializer)
    }
}
impl AuthorityRevocationProjectionV1 {
    pub fn task_terminal_index(&self) -> Option<&crate::TaskTerminalIndexV2> {
        self.0.task_terminals_v2.as_ref()
    }
}
macro_rules! authority_field {
    (world_service_results, $columns:expr, $decoder:expr) => {{
        // The enclosing genuine qualification/Current owner already proves the
        // excluded subtree's original type. Authenticate its typed root cell,
        // but do not load or clone any correlation-result payload.
        match take_named($columns, "world_service_results")? {
            ColumnCellV1::Root(root) if root.namespace == StateNamespaceV1::Authorization => {
                Ok(std::collections::BTreeMap::new())
            }
            _ => Err(invalid()),
        }
    }};
    ($name:ident, $columns:expr, $decoder:expr) => {
        Codec::decode(take_named($columns, stringify!($name))?, $decoder)
    };
}
macro_rules! authority_codec {
    ($ty:ty, {$($name:ident),+ $(,)?}) => {
        impl Codec for AuthorityRevocationProjectionV1 {
            fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
                &self, profile: &TypedRecordProfileV1, ctx: &mut MemberRecordWriteContext<'_, W, C>,
                work: &mut TypedRecordWriteWorkV1,
            ) -> Result<Encoded<'_>, MemberRecordError> {
                self.0.encode(profile, ctx, work)
            }
            fn decode<M: MerkleNodeReader, R: ValueNodeReader, E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>(
                cell: ColumnCellV1, d: &mut Decoder<'_, '_, M, R, E, C>,
            ) -> Result<Self, ValueTreeError> {
                let mut columns = d.read(cell)?;
                columns.spend_output(std::mem::size_of::<Self>())?;
                let value = crate::CapabilityRevocationState {
                    $($name: authority_field!($name, &mut columns, d)?),+
                };
                d.hold(columns.finish_parts()?)?;
                Ok(Self(value))
            }
        }
    };
}
super::dto::revocation_fields!(authority_codec);

/// One-time GEN/cold selected-source recovery. Warm normal execution must edit
/// its resident canonical projection by actual changed paths instead of
/// repeatedly recovering this complete authority projection.
pub fn recover_qualified_authority_revocation_v1<M, R, E, C>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<RevocationRecoveredV1<AuthorityRevocationProjectionV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let _metadata = read.values.memory.reserve(4096, read.values.class)?;
    qualification.validate_binding(roots)?;
    if qualification.allocation_class() != read.values.class
        || !qualification.uses_budget(read.values.memory)
        || read.root != roots.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid());
    }
    read.check()?;
    crate::incremental_typed_record_codec::recover_typed_record_body_v1(
        qualification.body(),
        &profile(),
        read,
        scan,
        work,
    )
}

/// Same projection from a genuine transaction Current owner. No caller body
/// root is accepted and no actual Current field is replaced or cleared.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_authority_revocation_v1<M, R, E, C, P, V, ER, CR>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &oasis7_formal_protocol::incremental_execution::PreparedGuard,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<
    RevocationRecoveredV1<AuthorityRevocationProjectionV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        if qualification.allocation_class() != MemoryClass::Ordinary
            || overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        let _phase = overlay
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let body = {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            super::current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        current.check(tx, &body, overlay.values.memory)?;
        let value = {
            let mut combined_read = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || (read.cancelled)() || (overlay.cancelled)(),
            };
            crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                &body,
                &profile(),
                &mut combined_read,
                scan,
                work,
            )?
        };
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(value)
    })
}
