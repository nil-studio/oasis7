//! Content dependency check for an IWS execution manifest and its two original
//! artifact inventories. Semantic loading, field binding and durability remain.
use super::*;
use crate::incremental_artifacts::{
    ArtifactCatalogV1, ArtifactEntryV1, ArtifactKindV1, ExecutionManifestModeV1,
    IwsExecutionManifestV1, OriginalArtifactRefV1,
};

pub struct OriginalArtifactBudget {
    pub objects_remaining: u64,
    pub bytes_remaining: u64,
    pub max_object_bytes: u64,
}
pub struct ExecutionArtifactBudget {
    pub structural: CoreBudget,
    pub originals: OriginalArtifactBudget,
}
pub struct ExecutionArtifactReadContext<'a, O, R> {
    pub objects: &'a mut O,
    pub pages: &'a R,
    pub memory: &'a MemoryBudget,
}
#[derive(Debug)]
pub struct CheckedExecutionArtifacts {
    body: [u8; 32],
    manifest: [u8; 32],
    catalog_counts: [u64; 2],
    original_bytes: u64,
}
impl CheckedExecutionArtifacts {
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn manifest_id(&self) -> [u8; 32] {
        self.manifest
    }
    pub fn catalog_counts(&self) -> [u64; 2] {
        self.catalog_counts
    }
    pub fn original_bytes(&self) -> u64 {
        self.original_bytes
    }
}
fn original<O: OriginalObjectStreamReader>(
    objects: &mut O,
    entry: OriginalArtifactRefV1,
    memory: &MemoryBudget,
    quota: &mut OriginalArtifactBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), CoreError<O::Error>> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if quota.objects_remaining == 0
        || entry.byte_len > quota.max_object_bytes
        || entry.byte_len > quota.bytes_remaining
    {
        return Err(CoreError::ResourceLimited("execution original allowance"));
    }
    quota.objects_remaining -= 1;
    quota.bytes_remaining -= entry.byte_len;
    let bytes = super::fact_material::verify_original_stream(
        objects,
        &entry.id,
        entry.byte_len,
        memory,
        cancelled,
    )?;
    if bytes != entry.byte_len {
        return Err(CoreError::Invalid("execution original length"));
    }
    Ok(())
}
/// Full cold content verification only. Never rescan unchanged inventories on
/// each ordinary commit; reuse requires the qualified parent durability lease.
pub fn verify_execution_artifacts<O: OriginalObjectStreamReader, R: MerkleNodeReader>(
    core: &CheckedCommitCore,
    native: &ReservedValue<CheckedNativeEnvironment>,
    context: ExecutionArtifactReadContext<'_, O, R>,
    budget: &mut ExecutionArtifactBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedExecutionArtifacts>, CoreError<O::Error>> {
    let mut phase = context
        .memory
        .reserve(128 * 1024, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    if !core.catalog.uses_budget(context.memory)
        || !native.uses_budget(context.memory)
        || !context
            .objects
            .allocation_budget()
            .same_pool(context.memory)
        || context.pages.allocation_class() != Some(MemoryClass::Ordinary)
        || !context
            .pages
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(context.memory))
    {
        return Err(CoreError::Invalid("execution artifact pool/class"));
    }
    let body = core.catalog.entry.commit;
    let refs = core.execution_artifacts;
    if native.body_id() != body || native.environment_id() != refs.native {
        return Err(CoreError::Invalid("execution native marker binding"));
    }
    let cancelled = RefCell::new(cancelled);
    let mut poll = || (cancelled.borrow_mut())();
    let bytes = load(
        context.objects,
        &refs.manifest,
        1024,
        context.memory,
        &mut budget.structural,
        &mut poll,
    )?;
    let manifest = IwsExecutionManifestV1::from_canonical_bytes(&bytes)?;
    if manifest.scope != core.body.scope
        || manifest.format_profile != core.config.format_profile
        || manifest.native != refs.native
        || manifest.executable != native.executable_id()
        || manifest.wasm != refs.wasm
        || manifest.rules != refs.rules
        || manifest.policy.id != core.original_execution_policy
    {
        return Err(CoreError::Invalid("execution manifest binding"));
    }
    match (&manifest.mode, core.transition.as_ref()) {
        (ExecutionManifestModeV1::Genesis, None) => {}
        (
            ExecutionManifestModeV1::Execution {
                parent_commit,
                parent_roots,
                operation,
                input,
                ..
            },
            Some(transition),
        ) if Some(*parent_commit) == core.body.parent_commit
            && *parent_roots == transition.before.digest()?
            && *operation == core.body.operation
            && *input == core.original_input => {}
        _ => return Err(CoreError::Invalid("execution manifest mode/parent/input")),
    }
    let mut result = CheckedExecutionArtifacts {
        body,
        manifest: refs.manifest,
        catalog_counts: [0; 2],
        original_bytes: 0,
    };
    for (index, id, kind) in [
        (0, refs.wasm, ArtifactKindV1::Wasm),
        (1, refs.rules, ArtifactKindV1::Rules),
    ] {
        let bytes = load(
            context.objects,
            &id,
            128,
            context.memory,
            &mut budget.structural,
            &mut poll,
        )?;
        let catalog = ArtifactCatalogV1::from_canonical_bytes(&bytes)?;
        if catalog.kind != kind {
            return Err(CoreError::Invalid("execution catalog role"));
        }
        let count = with_pages(
            context.pages,
            &mut budget.structural,
            &mut poll,
            |source, _| {
                if kind == ArtifactKindV1::Rules
                    && catalog.get(&manifest.policy.id, source)?
                        != Some(ArtifactEntryV1 {
                            kind,
                            original: manifest.policy,
                        })
                {
                    return Err(CoreError::Invalid("execution policy absent from rules"));
                }
                catalog
                    .visit(source, |entry| {
                        original(
                            context.objects,
                            entry.original,
                            context.memory,
                            &mut budget.originals,
                            &mut || (cancelled.borrow_mut())(),
                        )?;
                        result.original_bytes = result
                            .original_bytes
                            .checked_add(entry.original.byte_len)
                            .ok_or(CoreError::ResourceLimited("execution original byte total"))?;
                        Ok(())
                    })
                    .map_err(|error| match error {
                        crate::merkle::VisitError::Tree(error) => {
                            CoreError::Execution(ExecutionError::Tree(error))
                        }
                        crate::merkle::VisitError::Visitor(error) => error,
                    })
            },
        )?;
        result.catalog_counts[index] = count;
    }
    if let ExecutionManifestModeV1::Execution { continuation, .. } = manifest.mode {
        original(
            context.objects,
            continuation,
            context.memory,
            &mut budget.originals,
            &mut poll,
        )?;
        result.original_bytes = result
            .original_bytes
            .checked_add(continuation.byte_len)
            .ok_or(CoreError::ResourceLimited(
                "execution continuation byte total",
            ))?;
    }
    if poll() {
        return Err(CoreError::Cancelled);
    }
    let charge = phase
        .split(256)
        .map_err(|_| CoreError::Invalid("execution artifact result admission"))?;
    Ok(charge.retain(result))
}
