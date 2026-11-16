//! Fixed complete module-lifecycle configuration replacements. These neutral
//! producers establish original codec closure and exact Current outer CAS;
//! they confer no governance, module invocation or Action permission.
use super::*;
use crate::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

#[derive(Clone, Copy)]
pub enum ModuleLifecycleArtifactConfigEditV1<'a> {
    Manifest(&'a crate::Manifest),
    RegistryRecord {
        original_key: &'a str,
        after: &'a ModuleRecord,
    },
    Active {
        module_id: &'a str,
        after: Option<&'a str>,
    },
}
#[allow(clippy::too_many_arguments)]
pub fn replace_current_module_lifecycle_artifact_config_v1<M, R, E, C, W, CW>(
    edit: ModuleLifecycleArtifactConfigEditV1<'_>,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    before: Option<&ReservedValue<ArtifactConfigBodyQualificationV1>>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut ArtifactConfigWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    let (field, key, after) = match edit {
        ModuleLifecycleArtifactConfigEditV1::Manifest(value) => (
            ArtifactConfigFieldV1::Manifest,
            ArtifactConfigRecordKeyV1::Global,
            Some(ArtifactConfigBodyRefV1::Manifest(value)),
        ),
        ModuleLifecycleArtifactConfigEditV1::RegistryRecord {
            original_key,
            after,
        } => (
            ArtifactConfigFieldV1::RegistryManifest,
            ArtifactConfigRecordKeyV1::Member(original_key),
            Some(ArtifactConfigBodyRefV1::Module(after)),
        ),
        ModuleLifecycleArtifactConfigEditV1::Active { module_id, after } => (
            ArtifactConfigFieldV1::RegistryActive,
            ArtifactConfigRecordKeyV1::Member(module_id),
            after.map(ArtifactConfigBodyRefV1::Active),
        ),
    };
    change_qualified_artifact_config_record_v1(
        ArtifactConfigRecordChangeV1 {
            field,
            key,
            family,
            expected: before,
            after,
        },
        ctx,
        write,
        work,
        guard,
        live,
    )
    .map(|result| result.qualification)
}
