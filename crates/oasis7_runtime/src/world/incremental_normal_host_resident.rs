//! Actual GEN/cold construction and ordinary borrowing of normal Host sources.
//! Cold projection DATA is authenticated by the original seven Current joins;
//! ordinary execution never calls these bootstrap scans.
use super::{
    incremental_authorization_root_current::{
        AcceptedAuthorizationProjectionV1, AuthorizationRootSourceReadWorkV1,
        CurrentAuthorizationProjectionV1, ProvisionalAuthorizationProjectionV1,
        qualify_accepted_parent,
    },
    incremental_body::BodyMutationError,
    incremental_module_execution_manifest_hash::{
        AcceptedWorldManifestHashV1, RootWorldManifestHashWorkV1, recover_world_manifest_hash_v1,
    },
    incremental_normal_host_context::NormalHostContextSourcesV1,
    incremental_normal_industry_due::delivery::{
        ProductValidationDeliveryIndexV1, ProductValidationDeliveryWorkV1,
        build_product_validation_delivery_index_v1,
    },
    incremental_normal_parent_caches::{
        NormalParentCacheBootstrapWorkV1, NormalParentCachesV1,
        bootstrap_genesis_or_cold_normal_parent_caches_v1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_world_event_counter::WorldJournalRetentionV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryMaterialScopeWorkV1, ParentInventoryMaterialScopeV1,
        build_parent_inventory_material_scope_v1,
    },
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
};
#[path = "incremental_normal_host_resident_tail.rs"]
mod bootstrap_tail;
#[path = "incremental_normal_host_resident_startup_io.rs"]
mod startup_io;
pub use bootstrap_tail::NormalHostBootstrapTailWorkV1;
/// No public field, default, arbitrary ID constructor or post-install setter.
/// Every owned component was produced against the same actual accepted parent.
pub struct NormalHostResidentSourcesV1 {
    projection_cache: ReservedValue<
        super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    >,
    business: ReservedValue<
        super::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1,
    >,
    process: std::sync::Arc<
        ReservedValue<super::incremental_normal_process_source::NormalProcessSourceV1>,
    >,
    manifest: ReservedValue<AcceptedWorldManifestHashV1>,
    delivery: ReservedValue<ProductValidationDeliveryIndexV1>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    storage: ResidentStorage,
    authorization: AcceptedAuthorizationProjectionV1,
    record: [u8; 32],
    roots: [u8; 32],
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
pub(in crate::world) struct PendingNormalHostResidentSourcesV1 {
    outer: oasis7_proto::memory_budget::MemoryReservation,
    business: ReservedValue<
        super::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1,
    >,
    process: std::sync::Arc<
        ReservedValue<super::incremental_normal_process_source::NormalProcessSourceV1>,
    >,
    manifest: ReservedValue<AcceptedWorldManifestHashV1>,
    delivery: ReservedValue<ProductValidationDeliveryIndexV1>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    storage: ResidentStorage,
    authorization: AcceptedAuthorizationProjectionV1,
    record: [u8; 32],
    roots: [u8; 32],
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
impl PendingNormalHostResidentSourcesV1 {
    pub(in crate::world) fn root_read_sources_v1(
        &self,
    ) -> super::incremental_normal_host_context::NormalHostRootReadSourcesV1<'_> {
        super::incremental_normal_host_context::NormalHostRootReadSourcesV1 {
            manifest: &self.manifest,
            delivery: &self.delivery,
            retention: &self.retention,
            materials: self.storage.materials(),
        }
    }
    pub(in crate::world) fn business_v1(
        &self,
    ) -> &ReservedValue<
        super::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1,
    > {
        &self.business
    }
    pub(in crate::world) fn parent_caches_v1(&self) -> &ReservedValue<NormalParentCachesV1> {
        self.storage.caches()
    }
    pub(in crate::world) fn process_source_v1(
        &self,
    ) -> &super::incremental_normal_process_source::NormalProcessSourceV1 {
        &self.process
    }
    /// Bootstrap metadata source only. The readonly transaction and the pending
    /// carrier cannot satisfy the normal execution constructor's cache input.
    pub(in crate::world) fn begin_bootstrap_authorization_v1<'a, M: MerkleNodeReader>(
        &'a self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CurrentAuthorizationProjectionV1<'a>, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        tx.validate_bootstrap_read_only_v1(live)?;
        common.consume_steps(1).map_err(formal)?;
        if self.record != parent.refs().head.verified_record_id()
            || self.roots != parent.fence().parent_roots
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || tx.parent_fence() != parent.fence()
            || !tx.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.business.validate_parent(parent, common)?;
        self.storage.caches().validate_parent_v1(parent, common)?;
        self.process
            .is_devlocal_for_current_parent_v1(parent, common)?;
        self.authorization
            .begin_current(parent.refs().head, tx, common, live)
            .map_err(original)
    }
    pub(in crate::world) fn finish_projection_cache_v1(
        self,
        parent: &BoundNormalTickParentV1<'_>,
        projection_cache: ReservedValue<
            super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
        >,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<NormalHostResidentSourcesV1>, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        projection_cache.validate_parent_v1(parent, common)?;
        common.charge(3, 64).map_err(formal)?;
        if self.record != parent.refs().head.verified_record_id()
            || self.roots != parent.fence().parent_roots
            || self.core != common.work().identity()
            || !self.outer.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        let n = std::mem::size_of::<NormalHostResidentSourcesV1>();
        common.charge(1, n as u64).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(NormalHostResidentSourcesV1 {
            projection_cache,
            business: self.business,
            process: self.process,
            manifest: self.manifest,
            delivery: self.delivery,
            retention: self.retention,
            storage: self.storage,
            authorization: self.authorization,
            record: self.record,
            roots: self.roots,
            core: self.core,
            fee: self.fee,
        }))
    }
}
enum ResidentStorage {
    Built {
        materials: ReservedValue<ParentInventoryMaterialScopeV1>,
        caches: ReservedValue<NormalParentCachesV1>,
        tail: ReservedValue<bootstrap_tail::NormalHostBootstrapTailSourcesV1>,
    },
    Warm {
        directories: crate::world::incremental_commit::AcceptedRuntimeDirectoryCacheV1,
        cognition: Option<cognition_promotion::AcceptedNormalHostCognitionV1>,
        capture: std::sync::Arc<
            ReservedValue<
                crate::world::incremental_normal_final_sources::CapturedNormalFinalSourcesV1,
            >,
        >,
    },
}
impl ResidentStorage {
    fn caches(&self) -> &ReservedValue<NormalParentCachesV1> {
        match self {
            Self::Built { caches, .. } => caches,
            Self::Warm { directories, .. } => directories.normal_parent_caches_v1(),
        }
    }
    fn materials(&self) -> &ReservedValue<ParentInventoryMaterialScopeV1> {
        match self {
            Self::Built { materials, .. } => materials,
            Self::Warm { directories, .. } => directories.normal_material_scope_v1(),
        }
    }
}
pub struct NormalHostStartupWorkV1<'a> {
    pub manifest: &'a mut RootWorldManifestHashWorkV1,
    pub material_scan: &'a mut StateScanBudget,
    pub inventory: &'a mut InventoryFamilyWorkV1,
    pub material: &'a mut InventoryMaterialScopeWorkV1,
    pub caches: NormalParentCacheBootstrapWorkV1<'a>,
    pub delivery: &'a mut ProductValidationDeliveryWorkV1,
    pub authorization: &'a mut AuthorizationRootSourceReadWorkV1,
    pub tail: NormalHostBootstrapTailWorkV1<'a>,
}
#[derive(Debug)]
struct Cause<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for Cause<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for Cause<T> {}
fn original<T: std::fmt::Debug + Send + Sync + 'static>(e: T) -> BodyMutationError {
    BodyMutationError::Original(Box::new(Cause(e)))
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("normal resident actual parent sources")
}
fn formal(
    e: oasis7_formal_protocol::controlled_authority::formal::FormalError,
) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
#[allow(clippy::too_many_arguments)]
pub fn bootstrap_normal_host_resident_sources_v1<M, R, E, C, O>(
    parent: &BoundNormalTickParentV1<'_>,
    projection: ProvisionalAuthorizationProjectionV1,
    projection_cache: ReservedValue<
        super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    >,
    process: super::incremental_normal_process_source::NormalProcessStartupSourceV1<'_>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    revocation: &RuntimeRevocationCurrentV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut NormalHostStartupWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<NormalHostResidentSourcesV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        projection_cache.validate_parent_v1(parent, common)?;
        let base = prepare_normal_host_resident_base_v1(
            parent, projection, process, retention, revocation, ctx, read, originals, work, common,
            live,
        )?;
        base.finish_projection_cache_v1(parent, projection_cache, common)
    }));
    match attempt {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_normal_host_resident_base_v1<M, R, E, C, O>(
    parent: &BoundNormalTickParentV1<'_>,
    projection: ProvisionalAuthorizationProjectionV1,
    process: super::incremental_normal_process_source::NormalProcessStartupSourceV1<'_>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    revocation: &RuntimeRevocationCurrentV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut NormalHostStartupWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PendingNormalHostResidentSourcesV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parent.validate_context(common).map_err(formal)?;
        let process =
            super::incremental_normal_process_source::NormalProcessSourceV1::bind_startup_v1(
                process, parent, common,
            )?;
        if ctx.transaction.parent_fence() != parent.fence()
            || !retention.uses_budget(common.memory())
        {
            return Err(bad());
        }
        // The existing bootstrap enforces pristine TX and uses only this same Q.
        let caches = bootstrap_genesis_or_cold_normal_parent_caches_v1(
            parent,
            read.state,
            ctx,
            &mut work.caches,
            common,
            live,
        )?;
        let tail = bootstrap_tail::build(
            parent,
            ctx,
            read,
            work.caches.factory,
            work.caches.industry,
            &mut work.tail,
            common,
            live,
        )?;
        let (manifest, materials) = startup_io::selected(parent, read, work, common)?;
        let delivery = build_product_validation_delivery_index_v1(
            parent.refs().head,
            parent.refs().runtime,
            parent.refs().roots,
            read,
            originals,
            work.delivery,
            common,
        )
        .map_err(original)?;
        let mut authorization_read = WorldFieldReadContextV1 {
            root: parent
                .refs()
                .roots
                .state_root(StateNamespaceV1::Authorization),
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        let authorization = qualify_accepted_parent(
            projection,
            parent.refs().head,
            parent.refs().runtime,
            parent.refs().storage.current().runtime(),
            revocation,
            parent.refs().roots,
            ctx,
            &mut authorization_read,
            work.authorization,
            common,
            live,
        )
        .map_err(original)?;
        let business=super::incremental_normal_business_commitment::metadata_parent::bootstrap_normal_business_metadata_parent_v1(parent,ctx,read,originals,work.authorization,work.delivery,common,live).map_err(original)?;
        common
            .charge(
                1,
                std::mem::size_of::<NormalHostResidentSourcesV1>()
                    .max(std::mem::size_of::<PendingNormalHostResidentSourcesV1>())
                    as u64,
            )
            .map_err(formal)?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<NormalHostResidentSourcesV1>()
                    .max(std::mem::size_of::<PendingNormalHostResidentSourcesV1>()),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let outer = common
            .memory()
            .reserve(
                std::mem::size_of::<NormalHostResidentSourcesV1>()
                    .max(std::mem::size_of::<PendingNormalHostResidentSourcesV1>()),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(PendingNormalHostResidentSourcesV1 {
            outer,
            business,
            process,
            manifest,
            delivery,
            retention,
            storage: ResidentStorage::Built {
                materials,
                caches,
                tail,
            },
            authorization,
            record: parent.refs().head.verified_record_id(),
            roots: parent.fence().parent_roots,
            core: common.work().identity(),
            fee,
        })
    }));
    match result {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(payload) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
    }
}
impl NormalHostResidentSourcesV1 {
    pub fn process_source_v1(
        &self,
    ) -> &ReservedValue<super::incremental_normal_process_source::NormalProcessSourceV1> {
        &self.process
    }
    pub fn validate_same_host_process_v1(
        &self,
        previous: &Self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(formal)?;
        if !std::sync::Arc::ptr_eq(&self.process, &previous.process) {
            return Err(bad());
        }
        self.process
            .same_configuration_v1(&previous.process, common)
    }
    pub fn validate_live_binding_v1(
        &self,
        record: [u8; 32],
        fence: &ExecutionFence,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 96).map_err(formal)?;
        if self.record != record
            || self.roots != fence.parent_roots
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    /// Inspect the actual installed storage only after checking its accepted
    /// parent and original Common. Cold-built residents still need Cognition
    /// admission; ordinary promoted residents retain their warm sources.
    pub fn requires_cognition_startup_bounded_v1(
        &self,
        record: [u8; 32],
        fence: &ExecutionFence,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        self.validate_live_binding_v1(record, fence, common)?;
        Ok(matches!(&self.storage, ResidentStorage::Built { .. }))
    }
    pub(in crate::world) fn begin_authorization<'a, M: MerkleNodeReader>(
        &'a self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CurrentAuthorizationProjectionV1<'a>, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        common.consume_steps(1).map_err(formal)?;
        if self.record != parent.refs().head.verified_record_id()
            || self.roots != parent.fence().parent_roots
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.business.validate_parent(parent, common)?;
        self.projection_cache.validate_parent_v1(parent, common)?;
        self.storage.caches().validate_parent_v1(parent, common)?;
        self.process
            .is_devlocal_for_current_parent_v1(parent, common)?;
        self.authorization
            .begin_current(parent.refs().head, tx, common, live)
            .map_err(original)
    }
    pub(in crate::world) fn sources<'a>(
        &'a self,
        compiler: &'a crate::module_source_compiler::RuntimeSourceCompiler,
    ) -> NormalHostContextSourcesV1<'a> {
        NormalHostContextSourcesV1 {
            business: &self.business,
            projection_cache: &self.projection_cache,
            process: &self.process,
            manifest: &self.manifest,
            delivery: &self.delivery,
            retention: &self.retention,
            materials: self.storage.materials(),
            caches: self.storage.caches(),
            compiler,
        }
    }
}

#[path = "incremental_normal_host_resident_cognition.rs"]
mod cognition_promotion;
#[path = "incremental_normal_host_resident_promotion.rs"]
mod promotion;
pub use promotion::{
    PreparedNormalHostResidentV1, prepare_normal_host_resident_promotion_from_io_v1,
};

#[path = "incremental_normal_host_resident_genesis.rs"]
mod genesis;

#[path = "incremental_normal_host_genesis_bootstrap.rs"]
mod genesis_host;
pub use genesis_host::{
    NormalHostGenesisAuthorityV1, NormalHostGenesisBootstrapErrorV1,
    bootstrap_authenticated_genesis_normal_host_resident_v1,
    bootstrap_authenticated_genesis_replica_resident_v1,
};

#[path = "incremental_normal_host_cognition_bootstrap.rs"]
mod cognition_bootstrap;
pub use cognition_bootstrap::{
    NormalHostCognitionBootstrapErrorV1, PaidNormalHostCognitionParentSourcesV1,
    bootstrap_normal_host_cognition_parent_v1,
};
