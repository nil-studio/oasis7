//! Root-only complete body schema, independently of a source DTO or snapshot.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonReadWorkV1, qualify_json_subtree_v1},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    },
    incremental_semantic_key::write_semantic_key_v1,
    incremental_utf8_output::Utf8Output,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{
    cell::RefCell,
    io::{Cursor, Write},
};

pub struct ArtifactConfigReadWorkV1 {
    pub cells_remaining: u64,
    pub max_payload_bytes: u64,
    pub json: JsonReadWorkV1,
}
impl ArtifactConfigReadWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "artifact config closure cells",
                ))?;
        Ok(())
    }
}
#[derive(Debug)]
pub struct ArtifactConfigBodyQualificationV1 {
    kind: ArtifactConfigBodyKindV1,
    root: StateRootV1,
    class: oasis7_proto::memory_budget::MemoryClass,
}
impl ArtifactConfigBodyQualificationV1 {
    pub fn allocation_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn kind(&self) -> ArtifactConfigBodyKindV1 {
        self.kind
    }
    pub fn validate_binding(
        &self,
        kind: ArtifactConfigBodyKindV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.kind != kind || self.root != *root {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn mint(
    kind: ArtifactConfigBodyKindV1,
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<ReservedValue<ArtifactConfigBodyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(ArtifactConfigBodyQualificationV1 {
            kind,
            root: root.clone(),
            class,
        }))
}
/// Copy only an already checked body binding into newly paid Ordinary memory.
/// The caller still needs its family/head binding before using it for execution.
pub fn readmit_artifact_config_body_ordinary_v1(
    source: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    kind: ArtifactConfigBodyKindV1,
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<ArtifactConfigBodyQualificationV1>, ValueTreeError> {
    use oasis7_proto::memory_budget::MemoryClass;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(read::invalid());
    }
    let _scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(kind, root)?;
    let output = mint(source.kind, &source.root, memory, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}

struct Checker<'a, 'ctx, M, R, E, C> {
    kind: ArtifactConfigBodyKindV1,
    ctx: &'a mut WorldFieldReadContextV1<'ctx, M, R, E, C>,
    work: &'a mut ArtifactConfigReadWorkV1,
    expected: u64,
}
impl<M, R, E, C> Checker<'_, '_, M, R, E, C>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    fn get(
        &mut self,
        section: &'static str,
        ordinal: Option<u64>,
        inner_ordinal: Option<u64>,
        column: &'static str,
    ) -> Result<ArtifactConfigValueV1, ValueTreeError> {
        self.work.cell()?;
        self.expected = self.expected.checked_add(1).ok_or_else(read::invalid)?;
        let address = ArtifactConfigAddressV1 {
            section,
            ordinal,
            inner_ordinal,
            member: None,
            column,
        };
        let key = read::key(address, &mut self.ctx.records())?;
        let id = self.ctx.lookup(&key.0)?.ok_or_else(read::invalid)?;
        let value = read::value(
            self.kind,
            address,
            &id,
            &key,
            self.work.max_payload_bytes,
            &mut self.ctx.records(),
            &mut std::io::sink(),
        )?;
        if let ArtifactConfigValueV1::JsonRoot(root) = &value {
            let mut nested = WorldFieldReadContextV1 {
                root,
                state: self.ctx.state,
                values: ValueReadContext {
                    source: self.ctx.values.source,
                    memory: self.ctx.values.memory,
                    class: self.ctx.values.class,
                    io: &mut *self.ctx.values.io,
                },
                key_work: &mut *self.ctx.key_work,
                semantic_bytes: self.ctx.semantic_bytes,
                record_bytes: self.ctx.record_bytes,
                take_state_error: &mut self.ctx.take_state_error,
                cancelled: &mut self.ctx.cancelled,
            };
            qualify_json_subtree_v1(
                genesis::json_profile(),
                root,
                &mut nested,
                &mut self.work.json,
            )?;
        }
        Ok(value)
    }
    fn fixed(
        &mut self,
        section: &'static str,
        column: &'static str,
    ) -> Result<ArtifactConfigValueV1, ValueTreeError> {
        self.get(section, None, None, column)
    }
    fn unsigned(
        &mut self,
        section: &'static str,
        ordinal: Option<u64>,
        column: &'static str,
    ) -> Result<u64, ValueTreeError> {
        match self.get(section, ordinal, None, column)? {
            ArtifactConfigValueV1::Unsigned(value) => Ok(value),
            _ => Err(read::invalid()),
        }
    }
    fn boolean(
        &mut self,
        section: &'static str,
        ordinal: Option<u64>,
        column: &'static str,
    ) -> Result<bool, ValueTreeError> {
        match self.get(section, ordinal, None, column)? {
            ArtifactConfigValueV1::Boolean(value) => Ok(value),
            _ => Err(read::invalid()),
        }
    }
    fn texts(&mut self, section: &'static str) -> Result<(), ValueTreeError> {
        let length = self.unsigned(section, None, "length")?;
        for ordinal in 0..length {
            self.get(section, Some(ordinal), None, "value")?;
        }
        Ok(())
    }
    fn limits(&mut self) -> Result<(), ValueTreeError> {
        for column in [
            "max_mem_bytes",
            "max_gas",
            "max_call_rate",
            "max_output_bytes",
            "max_effects",
            "max_emits",
        ] {
            self.fixed("limits", column)?;
        }
        Ok(())
    }
    fn module(&mut self) -> Result<u64, ValueTreeError> {
        for column in [
            "module_id",
            "name",
            "version",
            "kind",
            "role",
            "wasm_hash",
            "interface_version",
        ] {
            self.fixed("manifest", column)?;
        }
        self.texts("exports")?;
        self.texts("required_caps")?;
        self.limits()?;
        let length = self.unsigned("subscriptions", None, "length")?;
        for ordinal in 0..length {
            self.get("subscriptions", Some(ordinal), None, "marker")?;
            self.get("subscriptions", Some(ordinal), None, "stage")?;
            for section in ["subscriptions.event_kinds", "subscriptions.action_kinds"] {
                let length = self.unsigned(section, Some(ordinal), "length")?;
                for inner in 0..length {
                    self.get(section, Some(ordinal), Some(inner), "value")?;
                }
            }
            if self.boolean("subscriptions", Some(ordinal), "filters_present")? {
                self.get("subscriptions", Some(ordinal), None, "filters")?;
            }
        }
        for column in ["abi_version", "input_schema", "output_schema"] {
            self.fixed("abi_contract", column)?;
        }
        let cap_count = self.unsigned("abi_contract.cap_slots", None, "length")?;
        self.texts("abi_contract.policy_hooks")?;
        if self.boolean("abi_contract", None, "gameplay_present")? {
            for column in ["kind", "min_players", "max_players"] {
                self.fixed("abi_contract.gameplay", column)?;
            }
            self.texts("abi_contract.gameplay.game_modes")?;
        }
        let length = self.unsigned("abi_contract.declarations.commands", None, "length")?;
        for ordinal in 0..length {
            for column in [
                "namespace",
                "name",
                "schema_version",
                "schema_hash",
                "max_payload_bytes",
            ] {
                self.get(
                    "abi_contract.declarations.commands",
                    Some(ordinal),
                    None,
                    column,
                )?;
            }
        }
        if self.boolean("manifest", None, "artifact_identity_present")? {
            for column in [
                "source_hash",
                "build_manifest_hash",
                "signer_node_id",
                "signature_scheme",
                "artifact_signature",
            ] {
                self.fixed("artifact_identity", column)?;
            }
        }
        for column in ["registered_at", "registered_by", "audit_event_id"] {
            self.fixed("record", column)?;
        }
        Ok(cap_count)
    }
}

/// Matches only the canonical cap_slots key prefix; the original member text
/// is streamed through UTF8 validation and never retained. Other semantic keys
/// remain authenticated by their Member reader, and the exact cardinality
/// proof below excludes all leaves outside the independently required set.
struct CapProbe<'a, 'b> {
    prefix: &'a [u8],
    used: usize,
    length: [u8; 8],
    remaining: u64,
    matched: bool,
    text: Utf8Output<'b, std::io::Sink>,
}
impl Write for CapProbe<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if !self.matched {
            return Ok(bytes.len());
        }
        if self.used < self.prefix.len() {
            let take = bytes.len().min(self.prefix.len() - self.used);
            for (offset, byte) in bytes[..take].iter().enumerate() {
                let position = self.used + offset;
                if position < self.prefix.len() - 8 {
                    self.matched &= *byte == self.prefix[position];
                } else {
                    self.length[position - (self.prefix.len() - 8)] = *byte;
                }
            }
            self.used += take;
            if self.used == self.prefix.len() {
                self.remaining = u64::from_be_bytes(self.length);
            }
            return Ok(take);
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            self.matched = false;
            return Ok(bytes.len());
        }
        let take = self.remaining.min(bytes.len() as u64) as usize;
        let written = self.text.write(&bytes[..take])?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.text.flush()
    }
}
impl CapProbe<'_, '_> {
    fn complete(&self) -> bool {
        self.matched
            && self.used == self.prefix.len()
            && self.remaining == 0
            && self.text.is_complete()
            && !self.text.is_invalid()
    }
}

/// Full schema qualification of one exact body root. RootSet/registry-column
/// relations and artifact availability remain separate owner obligations.
pub fn qualify_artifact_config_body_v1<M, R, E, C>(
    kind: ArtifactConfigBodyKindV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<ReservedValue<ArtifactConfigBodyQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Artifacts {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    // Authenticate only metadata/body kind here, without reading any selected
    // Text column twice merely to choose the closed layout.
    work.cell()?;
    read::check_layout(kind, ctx)?;
    let (expected, cap_count) = {
        let mut checker = Checker {
            kind,
            ctx,
            work,
            expected: 2,
        };
        let cap_count = match kind {
            ArtifactConfigBodyKindV1::Manifest => {
                checker.fixed("manifest", "version")?;
                checker.fixed("manifest", "content")?;
                0
            }
            ArtifactConfigBodyKindV1::Module => checker.module()?,
            ArtifactConfigBodyKindV1::Active => {
                checker.fixed("active", "value")?;
                0
            }
            ArtifactConfigBodyKindV1::Limits => {
                checker.limits()?;
                0
            }
            ArtifactConfigBodyKindV1::Security => {
                for column in [
                    "allow_builtin_manifest_fallback",
                    "allow_identity_hash_signature",
                    "allow_local_finality_signing",
                    "allow_runtime_source_compile",
                ] {
                    checker.fixed("security", column)?;
                }
                0
            }
        };
        (
            checker
                .expected
                .checked_add(cap_count)
                .ok_or_else(read::invalid)?,
            cap_count,
        )
    };
    if ctx.root.leaf_count != expected {
        return Err(read::invalid());
    }
    let mut prefix = [0; 256];
    let mut target = Cursor::new(prefix.as_mut_slice());
    write_semantic_key_v1(
        &ArtifactConfigAddressV1 {
            section: "abi_contract.cap_slots",
            ordinal: None,
            inner_ordinal: None,
            member: Some(""),
            column: "value",
        }
        .semantic(),
        &mut target,
    )
    .map_err(|_| read::invalid())?;
    let prefix_len = target.position() as usize;
    let prefix = &prefix[..prefix_len];
    let meta = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let mut observed_caps = 0u64;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || (cancel.borrow_mut())(),
    };
    let result = StateScanContext {
        source: ctx.state,
        memory: local.values.memory,
        class: local.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| {
            work.cell()?;
            if entry.namespace != StateNamespaceV1::Artifacts {
                return Err(read::invalid());
            }
            if entry.key == meta.as_bytes() {
                return local.records().presence(&entry.value, &meta, 0);
            }
            let binding =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            let key = WorldFieldKeyV1::new(
                genesis::body_descriptor(),
                WorldFieldRecordKindV1::Entry,
                binding.semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            if entry.key != key.as_bytes() {
                return Err(read::invalid());
            }
            let mut records = local.records();
            let limits = MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: work.max_payload_bytes,
                record_bytes: records.record_bytes,
            };
            let header = read_member_record_header_v1(
                &entry.value,
                &key,
                records.values,
                limits,
                &mut *records.cancelled,
            )?;
            let mut sink = std::io::sink();
            let mut probe = CapProbe {
                prefix,
                used: 0,
                length: [0; 8],
                remaining: 0,
                matched: true,
                text: Utf8Output::new(&mut sink, "artifact config cap key UTF8"),
            };
            read_member_record_profile_v1(
                &entry.value,
                &key,
                records.values,
                MemberReadProfileV1 {
                    codec: header.codec,
                    semantic_bytes: header.semantic_bytes,
                    limits,
                },
                &mut probe,
                &mut std::io::sink(),
                &mut *records.cancelled,
            )?;
            if probe.complete() {
                if kind != ArtifactConfigBodyKindV1::Module
                    || header.codec != MemberPayloadCodecV1::Text
                {
                    return Err(read::invalid());
                }
                read_record_value_v1(
                    &entry.value,
                    &(key, header.semantic_bytes),
                    &mut records,
                    work.max_payload_bytes,
                    &mut std::io::sink(),
                    RecordReadProfileV1 {
                        codec: MemberPayloadCodecV1::Text,
                        nullable: false,
                        integer_max: u64::MAX,
                        labels: RecordReadLabelsV1 {
                            utf8: "artifact config cap value UTF8",
                            ledger_utf8: "artifact config unused",
                            ledger_frame: "artifact config unused",
                            boolean: "artifact config unused",
                            fixed: "artifact config unused",
                            profile: "artifact config cap profile",
                            integer: "artifact config unused",
                        },
                    },
                )?;
                observed_caps = observed_caps.checked_add(1).ok_or_else(read::invalid)?;
            }
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    let error = (local.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = error {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Memory(error)) => return Err(error.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "artifact config State scan",
            ));
        }
        Err(_) => return Err(read::invalid()),
    }
    if observed_caps != cap_count {
        return Err(read::invalid());
    }
    Ok(retained.retain(ArtifactConfigBodyQualificationV1 {
        kind,
        root: ctx.root.clone(),
        class: ctx.values.class,
    }))
}
