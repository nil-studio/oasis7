//! Fresh physical namespaces for actual previously checked static material.
//! This is not an accepted-cache constructor, Frozen receipt or execution permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::CheckedIncrementalFrozenMaterialV1;
use std::path::Path;
/// Retains the exact genuine material/content/parent borrows, both opened page
/// leases through their owners, and a newly acquired native namespace pin.
/// Ordinary successors may share a GEN static baseline without claiming that
/// its cold native/artifact proof was recomputed for the current parent body.
pub(crate) struct FrozenParentMaterialLeaseV1<'a, 'content> {
    material: &'a ReservedValue<CandidateExecutionMaterialV1>,
    content: &'a ReservedValue<CheckedIncrementalFrozenMaterialV1<'content>>,
    _pages: &'a IncrementalPageStore,
    _native_pin: std::fs::File,
}
impl FrozenParentMaterialLeaseV1<'_, '_> {
    pub(crate) fn parent_record_id(&self) -> [u8; 32] {
        self.material.record
    }
    pub(crate) fn parent_body_id(&self) -> [u8; 32] {
        self.material.body
    }
    pub(crate) fn parent_roots_id(&self) -> [u8; 32] {
        self.material.roots
    }
    pub(crate) fn reservation_id(&self) -> [u8; 32] {
        self.content.reservation_id()
    }
}
fn role_id(
    parent: &VerifiedFormalCommitV1,
    role: crate::controlled_authority::replicated_protocol::ArtifactRole,
) -> Result<[u8; 32], FormalError> {
    let value = parent
        .record()
        .artifact_roots
        .get(&role)
        .ok_or_else(|| invalid("Frozen parent static role missing"))?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Frozen parent static role digest"));
    }
    let mut result = [0; 32];
    hex::decode_to_slice(value, &mut result)
        .map_err(|_| invalid("Frozen parent static digest width"))?;
    Ok(result)
}
impl CandidateExecutionMaterialV1 {
    /// The associated function takes the actual Reserved carrier explicitly:
    /// accepting only `&Self` would lose its pool/admission observation.
    pub(crate) fn check_frozen_parent_lease_v1<'a, 'content>(
        material: &'a ReservedValue<Self>,
        check: &'a ReservedValue<CheckedIncrementalFrozenMaterialV1<'content>>,
        directory: &Path,
        pages: &'a IncrementalPageStore,
        memory: &MemoryBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ReservedValue<FrozenParentMaterialLeaseV1<'a, 'content>>, FormalError> {
        if cancelled() {
            return Err(invalid("Frozen parent lease cancelled"));
        }
        if directory.as_os_str().len() > 4096 {
            return Err(invalid("Frozen parent native path bound"));
        }
        let baseline = &material.baseline;
        let path_bytes = directory
            .as_os_str()
            .len()
            .checked_add(21)
            .and_then(|n| n.checked_add(pages.root().as_os_str().len()))
            .and_then(|n| n.checked_add(baseline.pages.root().as_os_str().len()))
            .and_then(|n| n.checked_add(10))
            .ok_or_else(|| invalid("Frozen parent path layout overflow"))?;
        // Same checked four-path-buffer plus fixed metadata/error workspace
        // model used by PageStore path admission. Pin/path joins and metadata
        // happen only after this fee, not before a returned carrier is funded.
        let _phase = memory
            .reserve(
                path_bytes
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(|| invalid("Frozen parent path workspace overflow"))?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("Frozen parent path workspace admission"))?;
        let fee = memory
            .reserve(
                std::mem::size_of::<FrozenParentMaterialLeaseV1<'a, 'content>>()
                    .checked_add(512)
                    .ok_or_else(|| invalid("Frozen parent carrier overflow"))?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("Frozen parent carrier admission"))?;
        let descriptor_bound = baseline
            .descriptor
            .capacity()
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(|| invalid("Frozen parent descriptor layout"))?;
        if !material.uses_budget(memory)
            || material.reserved_bytes() < std::mem::size_of::<Self>() + 512
            || !check.uses_budget(memory)
            || check.reserved_bytes()
                < std::mem::size_of::<CheckedIncrementalFrozenMaterialV1<'content>>()
            || !baseline.uses_budget(memory)
            || baseline.reserved_bytes() < std::mem::size_of::<StaticExecutionMaterialV1>() + 512
            || !baseline.proof.uses_budget(memory)
            || baseline.proof.reserved_bytes()
                < std::mem::size_of::<DeclaredExecutionMaterial>() + 1024
            || !baseline.proof.native.uses_budget(memory)
            || baseline.proof.native.reserved_bytes()
                < std::mem::size_of::<CheckedNativeEnvironment>()
            || !baseline.proof._artifacts.uses_budget(memory)
            || baseline.proof._artifacts.reserved_bytes()
                < std::mem::size_of::<CheckedExecutionArtifacts>()
            || !baseline.descriptor.uses_budget(memory)
            || baseline.descriptor.reserved_bytes() < descriptor_bound
            || baseline.descriptor.is_empty()
            || baseline.descriptor.len()
                > oasis7_formal_protocol::native_environment::MAX_NATIVE_DESCRIPTOR_BYTES
            || !pages.memory_budget().same_pool(memory)
            || !baseline.pages.memory_budget().same_pool(memory)
        {
            return Err(invalid("Frozen parent actual material pool/admission"));
        }
        if cancelled() {
            return Err(invalid("Frozen parent lease cancelled"));
        }
        let parent = check.parent();
        use crate::controlled_authority::replicated_protocol::ArtifactRole as Role;
        let root = role_id(parent, Role::IncrementalRootSet)?;
        let manifest = role_id(parent, Role::ExecutionManifest)?;
        let mut signed_root = [0; 32];
        hex::decode_to_slice(&parent.record().after_state_root, &mut signed_root)
            .map_err(|_| invalid("Frozen parent signed roots digest"))?;
        let mut signed_manifest = [0; 32];
        hex::decode_to_slice(
            &parent.record().runtime_manifest_digest,
            &mut signed_manifest,
        )
        .map_err(|_| invalid("Frozen parent signed manifest digest"))?;
        if material.record != check.parent_record_id()
            || material.record != parent.verified_record_id()
            || material.body != role_id(parent, Role::IncrementalCommitBody)?
            || material.roots != root
            || root != signed_root
            || material.manifest != manifest
            || manifest != signed_manifest
            || baseline.config != role_id(parent, Role::IncrementalGenesisConfig)?
            || baseline.refs.native != role_id(parent, Role::NativeExecutionEnvironment)?
            || baseline.refs.wasm != role_id(parent, Role::Wasm)?
            || baseline.refs.rules != role_id(parent, Role::Rules)?
            || baseline.policy != check.policy()
        {
            return Err(invalid("Frozen parent exact current/static material"));
        }
        // Static native/artifact proofs belong to the same original cold body.
        // Do not require that GEN body to equal a later ordinary parent's body.
        if baseline.proof.native.environment_id() != baseline.refs.native
            || baseline.proof.native.body_id() != baseline.proof._artifacts.body_id()
            || baseline.proof._artifacts.manifest_id() != baseline.refs.manifest
        {
            return Err(invalid("Frozen parent cold static proof identity"));
        }
        if pages.root() != baseline.pages.root() {
            return Err(invalid("Frozen parent PageStore path differs"));
        }
        let current_page = page_identity(pages)?;
        let opened_page = pages
            .opened_lease_identity()
            .map_err(page_store_formal_error)?;
        let baseline_page = baseline
            .pages
            .opened_lease_identity()
            .map_err(page_store_formal_error)?;
        if current_page != baseline.page_identity
            || opened_page != baseline.page_identity
            || baseline_page != baseline.page_identity
        {
            return Err(invalid("Frozen parent PageStore lease identity"));
        }
        if cancelled() {
            return Err(invalid("Frozen parent lease cancelled"));
        }
        let native_pin = crate::controlled_authority::formal::physical::pin(directory, false)?
            .ok_or_else(|| invalid("Frozen parent native pin missing"))?;
        if file_identity(&native_pin)? != baseline.native_identity
            || file_identity(&baseline.proof._native_pin)? != baseline.native_identity
        {
            return Err(invalid("Frozen parent native lease identity"));
        }
        // Recheck pages after acquiring the second namespace pin. Every source
        // owner and fresh pin remains borrowed/retained for the returned scope.
        if page_identity(pages)? != baseline.page_identity
            || pages
                .opened_lease_identity()
                .map_err(page_store_formal_error)?
                != baseline.page_identity
            || baseline
                .pages
                .opened_lease_identity()
                .map_err(page_store_formal_error)?
                != baseline.page_identity
        {
            return Err(invalid("Frozen parent final PageStore lease identity"));
        }
        if cancelled() {
            return Err(invalid("Frozen parent lease final cancellation"));
        }
        Ok(fee.retain(FrozenParentMaterialLeaseV1 {
            material,
            content: check,
            _pages: pages,
            _native_pin: native_pin,
        }))
    }
}
