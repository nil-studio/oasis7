use super::common::*;
use super::*;
pub(super) fn record_builtin<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    B: BuiltinReleaseManifestSourceV1,
>(
    source: &B,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::BuiltinReleaseManifest;
    metadata(&mut root, kind, context, work)?;
    group(&mut root, kind, &("module_sets", ()), context, work)?;
    let expected_sets = source.len();
    u64_value(
        &mut root,
        kind,
        "module_sets.count",
        expected_sets as u64,
        context,
        work,
    )?;
    let mut visited_sets = 0usize;
    let mut previous_set: Option<&str> = None;
    source.for_each_module_set(|set_id, set| {
        if previous_set.is_some_and(|previous| previous >= set_id) {
            return Err(MemberRecordError::Invalid("builtin module set key order"));
        }
        previous_set = Some(set_id);
        visited_sets = visited_sets
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("builtin module set count"))?;
        let mut child = child_root()?;
        let child_kind = RuntimeArtifactRecordV1::BuiltinModuleSet;
        metadata(&mut child, child_kind, context, work)?;
        group(&mut child, child_kind, &("modules", ()), context, work)?;
        let expected_modules = set.len();
        u64_value(
            &mut child,
            child_kind,
            "modules.count",
            expected_modules as u64,
            context,
            work,
        )?;
        let mut visited_modules = 0usize;
        let mut previous_module: Option<&str> = None;
        set.for_each_module(|module_id, entry| {
            if previous_module.is_some_and(|previous| previous >= module_id) {
                return Err(MemberRecordError::Invalid("builtin module key order"));
            }
            previous_module = Some(module_id);
            visited_modules = visited_modules
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("builtin module count"))?;
            let body = record_builtin_entry(&entry, context, work)?;
            root_member(
                &mut child,
                child_kind,
                &("modules", module_id),
                RuntimeArtifactRecordV1::BuiltinReleaseEntry,
                body,
                context,
                work,
            )
        })?;
        if visited_modules != expected_modules {
            return Err(MemberRecordError::Invalid("builtin module count mismatch"));
        }
        root_member(
            &mut root,
            kind,
            &("module_sets", set_id),
            child_kind,
            child,
            context,
            work,
        )
    })?;
    if visited_sets != expected_sets {
        return Err(MemberRecordError::Invalid(
            "builtin module set count mismatch",
        ));
    }
    Ok(root)
}

pub(super) fn record_builtin_entry<
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &BuiltinReleaseEntryRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    let kind = RuntimeArtifactRecordV1::BuiltinReleaseEntry;
    metadata(&mut root, kind, context, work)?;
    group(&mut root, kind, &("hash_tokens", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "hash_tokens.count",
        source.hash_tokens.len() as u64,
        context,
        work,
    )?;
    for (ordinal, token) in source.hash_tokens.iter().enumerate() {
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| MemberRecordError::Invalid("builtin hash ordinal"))?;
        member(
            &mut root,
            kind,
            &("hash_tokens", ordinal),
            MemberPayloadRefV1::Text(token),
            context,
            work,
        )?;
    }
    group(&mut root, kind, &("artifact_identities", ()), context, work)?;
    u64_value(
        &mut root,
        kind,
        "artifact_identities.count",
        source.artifact_identities.len() as u64,
        context,
        work,
    )?;
    for (identity_key, identity) in source.artifact_identities {
        let identity_root = identity::record_module_identity(identity, context, work)?;
        root_member(
            &mut root,
            kind,
            &("artifact_identities", identity_key),
            RuntimeArtifactRecordV1::ModuleArtifactIdentity,
            identity_root,
            context,
            work,
        )?;
    }
    Ok(root)
}
