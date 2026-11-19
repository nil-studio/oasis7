//! Original builtin-first/lexical product selector over genuine Current active
//! registry points. Installed invocation IDs alone never count as active.
use super::*;
use crate::world::incremental_module_invocation::collect_current_module_invocation_ids_v1;
use current::{CurrentFactoryScheduleFactsV1, EvaluationErrorV1};
use oasis7_world_state::incremental_runtime_artifact_config::*;

impl<M, P, R, V, E, C, PE, PC>
    CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn active_version(
        &mut self,
        id: &str,
    ) -> Result<Option<ReservedValue<String>>, EvaluationErrorV1> {
        self.charge(id.len())?;
        self.fees.hold(
            id.len()
                .checked_mul(24)
                .and_then(|n| {
                    n.checked_add(4 * ("module not active module record missing ".len() + 20))
                })
                .ok_or_else(limited)?,
            self.common,
        )?;
        let sources = self.sources;
        let live = self.live;
        super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let body = open_artifact_config_body_overlay_v1(
                    ArtifactConfigSelectedBodyRequestV1 {
                        field: ArtifactConfigFieldV1::RegistryActive,
                        key: ArtifactConfigRecordKeyV1::Member(id),
                        family: sources.artifact_config,
                        parent_roots: sources.roots,
                        current: None,
                    },
                    parent.state,
                    ctx,
                    &mut work.config_scan,
                    &mut work.config,
                )?;
                let Some(body) = body else {
                    return Ok(None);
                };
                let cap = work.max_module_text_bytes;
                let fee = ctx
                    .values
                    .memory
                    .reserve(
                        cap.checked_add(size_of::<String>()).ok_or_else(limited)?,
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let mut bytes = Vec::with_capacity(cap);
                if bytes.capacity() != cap {
                    return Err(limited().into());
                }
                let guard = ctx.transaction.guard();
                let value = read_qualified_artifact_config_overlay_v1(
                    ArtifactConfigCellRequestV1 {
                        field: ArtifactConfigFieldV1::RegistryActive,
                        key: ArtifactConfigRecordKeyV1::Member(id),
                        address: ArtifactConfigAddressV1 {
                            section: "active",
                            ordinal: None,
                            inner_ordinal: None,
                            member: None,
                            column: "value",
                        },
                        family: sources.artifact_config,
                        qualification: &body,
                        max_payload_bytes: cap as u64,
                    },
                    ctx,
                    &guard,
                    live,
                    &mut bytes,
                );
                let value = value?;
                let ArtifactConfigValueV1::Text { utf8_bytes } = *value else {
                    return Err(invalid().into());
                };
                if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
                    return Err(invalid().into());
                }
                Ok(Some(
                    fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
                ))
            },
        )
    }
    pub(super) fn product_module_id(
        &mut self,
        kind: &str,
    ) -> Result<Option<String>, EvaluationErrorV1> {
        self.charge(kind.len())?;
        if let Some(id) = crate::world::World::builtin_product_module_for_kind(kind) {
            if self.active_version(id)?.is_some() {
                self.fees.hold(id.len(), self.common)?;
                return Ok(Some(id.to_owned()));
            }
        }
        let suffix_len = kind.len().checked_add(1).ok_or_else(limited)?;
        self.fees.hold(suffix_len, self.common)?;
        let mut suffix = String::with_capacity(suffix_len);
        if suffix.capacity() != suffix_len {
            return Err(limited().into());
        }
        suffix.push('.');
        suffix.push_str(kind);
        let sources = self.sources;
        let live = self.live;
        let ids = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                collect_current_module_invocation_ids_v1(sources.invocation_directory,sources.module_instances,sources.artifact_config,read.state,ctx,&mut work.invocation,&guard,live).map_err(|e|match e{crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1::Execution(e)=>EvaluationErrorV1::Body(e.into()),crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1::Value(e)=>e.into(),crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1::Body(e)=>e.into(),crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1::Overlay(e)=>e.into()})
            },
        )?;
        for id in ids.iter() {
            self.charge(id.len().checked_add(suffix.len()).ok_or_else(limited)?)?;
            if id.contains(".product.")
                && id.ends_with(suffix.as_str())
                && self.active_version(id)?.is_some()
            {
                self.fees.hold(id.len(), self.common)?;
                return Ok(Some(id.clone()));
            }
        }
        Ok(None)
    }
    pub(super) fn product_module_hash(&mut self, id: &str) -> Result<String, EvaluationErrorV1> {
        self.charge(id.len())?;
        let version =
            self.active_version(id)?
                .ok_or_else(|| crate::WorldError::ModuleChangeInvalid {
                    reason: format!("module not active {id}"),
                })?;
        let len = id
            .len()
            .checked_add(version.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(limited)?;
        // Original ModuleRegistry::record_key is format!("{id}@{version}").
        // Formatting may geometrically grow, so admit both buffers up front.
        let capacity = len.checked_mul(2).ok_or_else(limited)?.max(8);
        self.fees.hold(capacity, self.common)?;
        let key = oasis7_world_state::ModuleRegistry::record_key(id, &version);
        if key.capacity() > capacity {
            return Err(limited().into());
        }
        let sources = self.sources;
        let manifest = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let body = open_artifact_config_body_overlay_v1(
                    ArtifactConfigSelectedBodyRequestV1 {
                        field: ArtifactConfigFieldV1::RegistryManifest,
                        key: ArtifactConfigRecordKeyV1::Member(&key),
                        family: sources.artifact_config,
                        parent_roots: sources.roots,
                        current: None,
                    },
                    parent.state,
                    ctx,
                    &mut work.config_scan,
                    &mut work.config,
                )?
                .ok_or_else(|| crate::WorldError::ModuleChangeInvalid {
                    reason: format!("module record missing {key}"),
                })?;
                let mut read = WorldFieldReadContextV1 {
                    root: body.root(),
                    state: parent.state,
                    values: ValueReadContext {
                        source: parent.values.source,
                        memory: parent.values.memory,
                        class: parent.values.class,
                        io: &mut *parent.values.io,
                    },
                    key_work: &mut *parent.key_work,
                    semantic_bytes: parent.semantic_bytes,
                    record_bytes: parent.record_bytes,
                    take_state_error: &mut parent.take_state_error,
                    cancelled: &mut parent.cancelled,
                };
                Ok(read_selected_module_manifest_v1(
                    &body,
                    &mut read,
                    &mut work.config,
                    &mut work.config_scan,
                    &work.manifest,
                )?)
            },
        )?;
        self.fees
            .hold(manifest.manifest().wasm_hash.len(), self.common)?;
        Ok(manifest.manifest().wasm_hash.clone())
    }
}
