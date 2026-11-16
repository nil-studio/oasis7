//! Fixed Manifest paths and dirty selected Registry entries.
use super::*;
use crate::incremental_runtime_artifact_config::{self as cfg, *};
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub(super) fn capture<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    index: usize,
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = match index {
        0 => ArtifactConfigFieldV1::Manifest,
        2 => ArtifactConfigFieldV1::RegistryManifest,
        3 => ArtifactConfigFieldV1::RegistryRegisteredAt,
        4 => ArtifactConfigFieldV1::RegistryRegisteredBy,
        5 => ArtifactConfigFieldV1::RegistryAuditEventId,
        6 => ArtifactConfigFieldV1::RegistryActive,
        _ => return Err(bad().into()),
    };
    let parsed = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    if selected.namespace()
        != oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts
    {
        return Err(bad().into());
    }
    let key = WorldFieldKeyV1::new(field.descriptor(), parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != selected.key() {
        return Err(bad().into());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(bad().into());
    }
    if index == 0 {
        return manifest(selected, &key, read, work, p, build).map_err(Into::into);
    }
    let identity = member::text_root(
        &key,
        &selected.after().or(selected.before()).ok_or_else(bad)?,
        &field.schema(),
        read,
        p,
    )?;
    let id = identity.key.as_str();
    let names = if index == 6 {
        ["active", id]
    } else {
        ["records", id]
    };
    if build.contains("module_registry", &names, p, &mut ctx.cancelled)? {
        return Ok(());
    }
    let (path, _path_fee) = owned_path(&names, p, &build.memory, &mut ctx.cancelled)?;
    // All four original aliases are re-opened together by the original selected
    // reader. Their absence and shared child-root checks are unchanged.
    let root_bytes = roots
        .state
        .iter()
        .try_fold(roots.world_id.len() + 4096, |n, v| {
            n.checked_add(v.node_root.len() + 128).ok_or_else(limit)
        })?;
    spend(
        p,
        root_bytes.checked_mul(16).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    let body = open_artifact_config_body_overlay_v1(
        ArtifactConfigSelectedBodyRequestV1 {
            field,
            key: ArtifactConfigRecordKeyV1::Member(id),
            family: parent.config(),
            parent_roots: roots,
            current: None,
        },
        read.state,
        ctx,
        work.inner_scan,
        work.config,
    )?;
    let Some(body) = body else {
        if selected.after().is_some() {
            return Err(bad().into());
        }
        build.value::<u8, _>("module_registry", &path, None, p, &mut ctx.cancelled)?;
        return Ok(());
    };
    if selected.after().is_none() {
        return Err(bad().into());
    }
    if index == 6 {
        let memory = build.memory.clone();
        let mut output = member::Text {
            bytes: Vec::new(),
            fee: memory
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
            memory: &memory,
            p,
            first: None,
        };
        let root = body.root();
        let mut current = WorldFieldReadContextV1 {
            root,
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
            cancelled: &mut read.cancelled,
        };
        let result = inspect_artifact_config_cell_v1(
            ArtifactConfigBodyKindV1::Active,
            ArtifactConfigAddressV1 {
                section: "active",
                ordinal: None,
                inner_ordinal: None,
                member: None,
                column: "value",
            },
            work.config.max_payload_bytes,
            &mut current,
            &mut output,
        );
        if let Some(error) = output.first {
            return Err(error.into());
        }
        let value = result?;
        let ArtifactConfigValueV1::Text { utf8_bytes } = *value else {
            return Err(bad().into());
        };
        if utf8_bytes != output.bytes.len() as u64 {
            return Err(bad().into());
        }
        let text = std::str::from_utf8(&output.bytes).map_err(|_| bad())?;
        build.value(
            "module_registry",
            &path,
            Some(text),
            p,
            &mut current.cancelled,
        )?;
    } else {
        let limits = SelectedModuleManifestLimitsV1 {
            heap_bytes: work.selected_heap,
            json_heap_bytes: work.selected_heap,
            max_collection_items: work.config.cells_remaining,
        };
        let mut current = WorldFieldReadContextV1 {
            root: body.root(),
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
            cancelled: &mut read.cancelled,
        };
        let record = read_selected_module_record_v1(
            &body,
            &mut current,
            work.config,
            work.inner_scan,
            &limits,
        )?;
        let clone_bytes = record
            .reserved_bytes()
            .checked_add(record.additional_graph_fee_bytes_v1().ok_or_else(limit)?)
            .ok_or_else(limit)?;
        spend(p, clone_bytes, &mut current.cancelled)?;
        let _clone = build
            .memory
            .reserve(clone_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let value = oasis7_wasm_abi::ModuleRecord {
            manifest: record.manifest().clone(),
            registered_at: record.registered_at(),
            registered_by: record.registered_by().to_owned(),
            audit_event_id: record.audit_event_id(),
        };
        build.value(
            "module_registry",
            &path,
            Some(&value),
            p,
            &mut current.cancelled,
        )?;
    }
    Ok(())
}
fn manifest<
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    key: &WorldFieldKeyV1,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    let before = {
        let mut records = read.records();
        selected
            .before()
            .map(|id| {
                cfg::projection::outer_root_v1(
                    ArtifactConfigFieldV1::Manifest,
                    key,
                    &id,
                    &mut records,
                )
            })
            .transpose()?
    };
    let after = {
        let mut records = read.records();
        selected
            .after()
            .map(|id| {
                cfg::projection::outer_root_v1(
                    ArtifactConfigFieldV1::Manifest,
                    key,
                    &id,
                    &mut records,
                )
            })
            .transpose()?
    };
    let Some(after) = after else {
        return Err(bad());
    };
    let before = before
        .as_ref()
        .map(|root| cfg::projection::manifest_parts_v1(root, read, work.config))
        .transpose()?;
    let (version, content) = cfg::projection::manifest_parts_v1(&after, read, work.config)?;
    if before.as_ref().is_none_or(|v| v.0 != version) {
        let (path, _fee) = owned_path(&["version"], p, &build.memory, &mut read.cancelled)?;
        build.value("manifest", &path, Some(&version), p, &mut read.cancelled)?;
    }
    let old = before
        .as_ref()
        .map(|v| match &*v.1 {
            ArtifactConfigValueV1::JsonRoot(root) => Ok(root),
            _ => Err(bad()),
        })
        .transpose()?;
    let ArtifactConfigValueV1::JsonRoot(new) = &*content else {
        return Err(bad());
    };
    let profile = cfg::projection::json_profile_v1();
    let mut records = read.records();
    if !selected.qualified() {
        if let Some(old) = old {
            crate::incremental_json_records::advance_from_qualified_root(
                profile,
                old,
                new,
                &mut records,
                work.json,
            )?
        } else {
            crate::incremental_json_records::qualify_json_subtree_records_v1(
                profile,
                new,
                &mut records,
                &mut work.json.json,
            )?;
        }
    }
    let mut patches = JsonAuxBuildV1::new(&build.memory)?;
    crate::incremental_json_records::project_json_aux_paths_v1(
        profile,
        old,
        Some(new),
        &mut records,
        &mut work.json.json,
        &mut work.json.paths_remaining,
        p,
        &mut patches,
    )?;
    let (path, _fee) = owned_path(&["content"], p, &build.memory, &mut *records.cancelled)?;
    build.push(
        "manifest",
        &path,
        Payload::Deep(patches),
        p,
        &mut *records.cancelled,
    )
}
