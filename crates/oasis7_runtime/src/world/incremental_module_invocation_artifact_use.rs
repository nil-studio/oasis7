//! Fixed artifact-retirement active-use query. Instance and registry identities
//! stay in their real namespaces; a colliding String never changes its kind.
use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_world_state::incremental_runtime_artifact_config::*;

pub(in crate::world) fn any_current_active_artifact_v1<M, P, R, E, C>(
    hash: &str,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    config: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CurrentInvocationEnumerationWorkV1,
    config_work: &mut ArtifactConfigReadWorkV1,
    config_scan: &mut StateScanBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, CurrentInvocationEnumerationErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let fence = ctx.transaction.parent_fence();
        if !directory.uses_budget(ctx.values.memory)
            || directory.parent_commit != fence.parent_commit
            || directory.roots != fence.parent_roots
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("artifact active-use directory binding").into());
        }
        let instance_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(directory.instances()),
            StateNamespaceV1::Business,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.instance_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            ModuleInstanceKeyV1::is_instance_key,
        )?;
        let slots = instance_keys
            .len()
            .checked_mul(std::mem::size_of::<ReservedValue<String>>())
            .ok_or(ValueTreeError::ResourceLimited(
                "artifact suppression slots",
            ))?;
        let _slots = ctx
            .values
            .memory
            .reserve(slots, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut suppressed = Vec::with_capacity(instance_keys.len());
        let mut previous = None;
        for key in instance_keys.iter() {
            let (id, _) = ModuleInstanceKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
            if previous == Some(id) {
                continue;
            }
            previous = Some(id);
            let mut read = ModuleReadContext {
                qualification: instances,
                values: ctx.values.source,
                memory: ctx.values.memory,
                io: &mut *ctx.values.io,
                max_value_bytes: work.max_text_bytes as u64,
                cancelled: &mut ctx.cancelled,
            };
            read.check(ctx.transaction)?;
            let instance =
                read.read_instance(id, ctx.transaction)?
                    .ok_or(ValueTreeError::Invalid(
                        "artifact incomplete current instance",
                    ))?;
            if matches!(instance.field(Field::Active), Scalar::Bool(true)) {
                let Scalar::Text(wasm) = instance.field(Field::WasmHash) else {
                    return Err(ValueTreeError::Invalid("artifact instance hash type").into());
                };
                compare_work(hash, wasm, work, &mut ctx.cancelled)?;
                if wasm == hash {
                    return Ok(true);
                }
            }
            let Scalar::Text(module) = instance.field(Field::ModuleId) else {
                return Err(ValueTreeError::Invalid("artifact instance module type").into());
            };
            let bytes = module
                .len()
                .checked_add(std::mem::size_of::<String>())
                .ok_or(ValueTreeError::ResourceLimited("artifact suppression text"))?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(module.len() as u64)
                .ok_or(ValueTreeError::ResourceLimited("artifact suppression copy"))?;
            let fee = ctx
                .values
                .memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            suppressed.push(fee.retain(module.to_owned()));
        }
        let registry_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(directory.registry()),
            StateNamespaceV1::Artifacts,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.registry_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            |key| {
                WorldFieldKeyV1::from_bytes(key).is_ok_and(|binding| {
                    binding.kind == WorldFieldRecordKindV1::Entry
                        && WorldFieldKeyV1::new(
                            ArtifactConfigFieldV1::RegistryActive.descriptor(),
                            WorldFieldRecordKindV1::Entry,
                            binding.semantic_key,
                        )
                        .is_ok_and(|k| k.as_bytes() == key)
                })
            },
        )?;
        for key in registry_keys.iter() {
            let guard = ctx.transaction.guard();
            let id = read_current_registry_active_id_v1(
                config,
                key,
                ctx,
                work.max_text_bytes,
                &guard,
                live,
            )?;
            let mut present = false;
            for module in &suppressed {
                compare_work(&id, module, work, &mut ctx.cancelled)?;
                if **module == *id {
                    present = true;
                    break;
                }
            }
            if present {
                continue;
            }
            let meta = roots
                .world_id
                .len()
                .checked_add(6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
                .and_then(|n| n.checked_add(1024))
                .and_then(|n| n.checked_mul(2))
                .ok_or(ValueTreeError::ResourceLimited(
                    "artifact registry binding work",
                ))?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(meta as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "artifact registry binding work",
                ))?;
            let active = open_artifact_config_body_overlay_v1(
                ArtifactConfigSelectedBodyRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryActive,
                    key: ArtifactConfigRecordKeyV1::Member(&id),
                    family: config,
                    parent_roots: roots,
                    current: None,
                },
                state,
                ctx,
                config_scan,
                config_work,
            )?;
            let Some(active) = active else { continue };
            let cap = work.max_text_bytes;
            let fee = ctx
                .values
                .memory
                .reserve(
                    cap.checked_add(std::mem::size_of::<String>())
                        .ok_or(ValueTreeError::ResourceLimited("artifact active version"))?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut version = Vec::with_capacity(cap);
            let guard = ctx.transaction.guard();
            let value = read_qualified_artifact_config_overlay_v1(
                ArtifactConfigCellRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryActive,
                    key: ArtifactConfigRecordKeyV1::Member(&id),
                    address: ArtifactConfigAddressV1 {
                        section: "active",
                        ordinal: None,
                        inner_ordinal: None,
                        member: None,
                        column: "value",
                    },
                    family: config,
                    qualification: &active,
                    max_payload_bytes: cap as u64,
                },
                ctx,
                &guard,
                live,
                &mut version,
            )?;
            let ArtifactConfigValueV1::Text { utf8_bytes } = *value else {
                return Err(ValueTreeError::Invalid("artifact active version type").into());
            };
            if usize::try_from(utf8_bytes).ok() != Some(version.len()) {
                return Err(ValueTreeError::Invalid("artifact active version length").into());
            }
            let version = fee.retain(
                String::from_utf8(version)
                    .map_err(|_| ValueTreeError::Invalid("artifact active version UTF8"))?,
            );
            let len = id
                .len()
                .checked_add(version.len())
                .and_then(|n| n.checked_add(1))
                .ok_or(ValueTreeError::ResourceLimited("artifact manifest key"))?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(len as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "artifact manifest key work",
                ))?;
            let _key_fee = ctx
                .values
                .memory
                .reserve(len, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut record = String::with_capacity(len);
            record.push_str(&id);
            record.push('@');
            record.push_str(&version);
            let body = open_artifact_config_body_overlay_v1(
                ArtifactConfigSelectedBodyRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryManifest,
                    key: ArtifactConfigRecordKeyV1::Member(&record),
                    family: config,
                    parent_roots: roots,
                    current: None,
                },
                state,
                ctx,
                config_scan,
                config_work,
            )?;
            let Some(body) = body else { continue };
            let guard = ctx.transaction.guard();
            let mut output = MatchText {
                expected: hash.as_bytes(),
                offset: 0,
                equal: true,
            };
            compare_work(hash, hash, work, &mut ctx.cancelled)?;
            let value = read_qualified_artifact_config_overlay_v1(
                ArtifactConfigCellRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryManifest,
                    key: ArtifactConfigRecordKeyV1::Member(&record),
                    address: ArtifactConfigAddressV1 {
                        section: "manifest",
                        ordinal: None,
                        inner_ordinal: None,
                        member: None,
                        column: "wasm_hash",
                    },
                    family: config,
                    qualification: &body,
                    max_payload_bytes: work.max_text_bytes as u64,
                },
                ctx,
                &guard,
                live,
                &mut output,
            )?;
            if !matches!(*value, ArtifactConfigValueV1::Text { .. }) {
                return Err(ValueTreeError::Invalid("artifact manifest hash type").into());
            }
            if output.equal && output.offset == output.expected.len() {
                return Ok(true);
            }
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(false)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn compare_work(
    a: &str,
    b: &str,
    work: &mut CurrentInvocationEnumerationWorkV1,
    cancel: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    work.registry_scan.nodes_remaining = work.registry_scan.nodes_remaining.checked_sub(1).ok_or(
        ValueTreeError::ResourceLimited("artifact active comparison steps"),
    )?;
    let bytes = a.len().min(b.len()).saturating_add(1) as u64;
    work.comparison_bytes_remaining = work.comparison_bytes_remaining.checked_sub(bytes).ok_or(
        ValueTreeError::ResourceLimited("artifact active comparison bytes"),
    )?;
    Ok(())
}
struct MatchText<'a> {
    expected: &'a [u8],
    offset: usize,
    equal: bool,
}
impl std::io::Write for MatchText<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let end = self
            .offset
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("artifact hash text"))?;
        self.equal &= self.expected.get(self.offset..end) == Some(b);
        self.offset = end;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
