//! One selected original ModuleManifest DTO. This data is not execution authority.
use super::*;
use crate::{incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

pub struct SelectedModuleManifestLimitsV1 {
    pub heap_bytes: usize,
    pub json_heap_bytes: usize,
    pub max_collection_items: u64,
}
pub struct SelectedModuleManifestV1 {
    manifest: ModuleManifest,
    root: oasis7_formal_protocol::incremental_state::StateRootV1,
    class: MemoryClass,
    _json: Vec<MemoryReservation>,
}
impl SelectedModuleManifestV1 {
    pub fn manifest(&self) -> &ModuleManifest {
        &self.manifest
    }
    pub fn root(&self) -> &oasis7_formal_protocol::incremental_state::StateRootV1 {
        &self.root
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn spend(heap: &mut usize, bytes: usize) -> Result<(), ValueTreeError> {
    *heap = heap
        .checked_sub(bytes)
        .ok_or(ValueTreeError::ResourceLimited("selected manifest heap"))?;
    Ok(())
}
struct Decoder<'a, 'b, M, R, E, C> {
    body: &'a ReservedValue<ArtifactConfigBodyQualificationV1>,
    ctx: &'a mut WorldFieldReadContextV1<'b, M, R, E, C>,
    work: &'a mut ArtifactConfigReadWorkV1,
    heap: usize,
    limit: &'a SelectedModuleManifestLimitsV1,
    json: Vec<MemoryReservation>,
    known: BTreeSet<[u8; 32]>,
}
fn address(
    section: &'static str,
    ordinal: Option<u64>,
    inner_ordinal: Option<u64>,
    column: &'static str,
) -> ArtifactConfigAddressV1<'static> {
    ArtifactConfigAddressV1 {
        section,
        ordinal,
        inner_ordinal,
        member: None,
        column,
    }
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> Decoder<'_, '_, M, R, E, C>
{
    fn get(
        &mut self,
        a: ArtifactConfigAddressV1<'_>,
    ) -> Result<(ArtifactConfigValueV1, Vec<u8>), ValueTreeError> {
        self.ctx.check()?;
        self.work.cell()?;
        let key = read::key(a, &mut self.ctx.records())?;
        spend(&mut self.heap, 512)?;
        self.known
            .insert(*blake3::hash(key.0.as_bytes()).as_bytes());
        let id = self.ctx.lookup(&key.0)?.ok_or_else(read::invalid)?;
        let info = read_member_record_header_v1(
            &id,
            &key.0,
            &mut self.ctx.values,
            MemberRecordLimitsV1 {
                semantic_bytes: self.ctx.semantic_bytes,
                payload_bytes: self.work.max_payload_bytes,
                record_bytes: self.ctx.record_bytes,
            },
            &mut self.ctx.cancelled,
        )?;
        let n = usize::try_from(info.payload_bytes).map_err(|_| read::invalid())?;
        spend(
            &mut self.heap,
            n.checked_add(128).ok_or_else(read::invalid)?,
        )?;
        let mut bytes = vec![0; n];
        let value = read::value(
            self.body.kind(),
            a,
            &id,
            &key,
            self.work.max_payload_bytes,
            &mut self.ctx.records(),
            &mut Cursor::new(bytes.as_mut_slice()),
        )?;
        self.ctx.check()?;
        Ok((value, bytes))
    }
    fn text(&mut self, a: ArtifactConfigAddressV1<'_>) -> Result<String, ValueTreeError> {
        let (v, bytes) = self.get(a)?;
        if !matches!(v, ArtifactConfigValueV1::Text { .. }) {
            return Err(read::invalid());
        }
        String::from_utf8(bytes).map_err(|_| read::invalid())
    }
    fn uint(&mut self, a: ArtifactConfigAddressV1<'_>) -> Result<u64, ValueTreeError> {
        match self.get(a)?.0 {
            ArtifactConfigValueV1::Unsigned(v) => Ok(v),
            _ => Err(read::invalid()),
        }
    }
    fn optional_uint(
        &mut self,
        a: ArtifactConfigAddressV1<'_>,
    ) -> Result<Option<u64>, ValueTreeError> {
        match self.get(a)?.0 {
            ArtifactConfigValueV1::Unsigned(v) => Ok(Some(v)),
            ArtifactConfigValueV1::Unit => Ok(None),
            _ => Err(read::invalid()),
        }
    }
    fn boolean(&mut self, a: ArtifactConfigAddressV1<'_>) -> Result<bool, ValueTreeError> {
        match self.get(a)?.0 {
            ArtifactConfigValueV1::Boolean(v) => Ok(v),
            _ => Err(read::invalid()),
        }
    }
    fn optional_text(
        &mut self,
        a: ArtifactConfigAddressV1<'_>,
    ) -> Result<Option<String>, ValueTreeError> {
        let (v, bytes) = self.get(a)?;
        match v {
            ArtifactConfigValueV1::Text { .. } => {
                Ok(Some(String::from_utf8(bytes).map_err(|_| read::invalid())?))
            }
            ArtifactConfigValueV1::Unit => Ok(None),
            _ => Err(read::invalid()),
        }
    }
    fn length(
        &mut self,
        section: &'static str,
        parent: Option<u64>,
        item_bytes: usize,
    ) -> Result<u64, ValueTreeError> {
        let n = self.uint(address(section, parent, None, "length"))?;
        if n > self.limit.max_collection_items {
            return Err(ValueTreeError::ResourceLimited("selected manifest items"));
        }
        let count = usize::try_from(n).map_err(|_| read::invalid())?;
        spend(
            &mut self.heap,
            count.checked_mul(item_bytes).ok_or_else(read::invalid)?,
        )?;
        Ok(n)
    }
    fn texts(
        &mut self,
        section: &'static str,
        parent: Option<u64>,
    ) -> Result<Vec<String>, ValueTreeError> {
        let length = self.length(section, parent, std::mem::size_of::<String>() + 32)?;
        let mut values = Vec::with_capacity(length as usize);
        for ordinal in 0..length {
            values.push(self.text(address(
                section,
                Some(parent.unwrap_or(ordinal)),
                parent.map(|_| ordinal),
                "value",
            ))?);
        }
        Ok(values)
    }
    fn filters(&mut self, ordinal: u64) -> Result<Option<serde_json::Value>, ValueTreeError> {
        if !self.boolean(address(
            "subscriptions",
            Some(ordinal),
            None,
            "filters_present",
        ))? {
            return Ok(None);
        }
        let proof = open_artifact_config_json_v1(
            self.body,
            address("subscriptions", Some(ordinal), None, "filters"),
            self.ctx,
            &mut self.work.json,
        )?;
        let (value, charge) = json::materialize_parts(
            &proof,
            &mut self.ctx.records(),
            &mut self.work.json,
            self.limit.json_heap_bytes,
        )?;
        spend(
            &mut self.heap,
            std::mem::size_of::<MemoryReservation>() + 128,
        )?;
        self.json.push(charge);
        Ok(Some(value))
    }
    fn cap_slots(
        &mut self,
        scan: &mut StateScanBudget,
    ) -> Result<BTreeMap<String, String>, ValueTreeError> {
        let length = self.length("abi_contract.cap_slots", None, 512)?;
        let mut values = BTreeMap::new();
        // These record metadata columns are not part of ModuleManifest. Never
        // touch their value roots merely to enumerate unknown cap-slot keys.
        for column in ["registered_at", "registered_by", "audit_event_id"] {
            let key = read::key(
                address("record", None, None, column),
                &mut self.ctx.records(),
            )?;
            spend(&mut self.heap, 512)?;
            self.known
                .insert(*blake3::hash(key.0.as_bytes()).as_bytes());
        }
        let meta = WorldFieldKeyV1::new(
            genesis::body_descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .map_err(ValueTreeError::Invalid)?;
        let marker = self
            .ctx
            .records()
            .key(
                genesis::body_descriptor(),
                WorldFieldRecordKindV1::Entry,
                &(),
            )?
            .0;
        let cancel = RefCell::new(&mut self.ctx.cancelled);
        let mut local = WorldFieldReadContextV1 {
            root: self.ctx.root,
            state: self.ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: self.ctx.values.source,
                memory: self.ctx.values.memory,
                class: self.ctx.values.class,
                io: self.ctx.values.io,
            },
            key_work: self.ctx.key_work,
            semantic_bytes: self.ctx.semantic_bytes,
            record_bytes: self.ctx.record_bytes,
            take_state_error: &mut self.ctx.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: self.ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: scan,
        }
        .visit_entries(
            self.ctx.root,
            |entry| {
                self.work.cell()?;
                if entry.key == meta.as_bytes()
                    || entry.key == marker.as_bytes()
                    || self.known.contains(blake3::hash(&entry.key).as_bytes())
                {
                    return Ok(());
                }
                let binding =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let key = WorldFieldKeyV1::new(
                    genesis::body_descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    binding.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key {
                    return Err(read::invalid());
                }
                let limits = MemberRecordLimitsV1 {
                    semantic_bytes: local.semantic_bytes,
                    payload_bytes: self.work.max_payload_bytes,
                    record_bytes: local.record_bytes,
                };
                let info = read_member_record_header_v1(
                    &entry.value,
                    &key,
                    &mut local.values,
                    limits,
                    &mut local.cancelled,
                )?;
                let bound = usize::try_from(info.semantic_bytes).map_err(|_| read::invalid())?;
                spend(
                    &mut self.heap,
                    bound.checked_add(128).ok_or_else(read::invalid)?,
                )?;
                let mut member = Vec::with_capacity(bound);
                let mut probe = key_read::Probe::new(&mut member);
                read_member_record_key_v1(
                    &entry.value,
                    &key,
                    &mut local.values,
                    limits,
                    &mut probe,
                    &mut local.cancelled,
                )?;
                let a = probe.finish()?;
                if a.section != "abi_contract.cap_slots"
                    || a.column != "value"
                    || a.member.is_none()
                {
                    return Ok(());
                }
                let member = String::from_utf8(member).map_err(|_| read::invalid())?;
                let n = usize::try_from(info.payload_bytes).map_err(|_| read::invalid())?;
                spend(
                    &mut self.heap,
                    n.checked_add(128).ok_or_else(read::invalid)?,
                )?;
                let mut bytes = vec![0; n];
                let a = ArtifactConfigAddressV1 {
                    member: Some(&member),
                    ..a
                };
                let pair = (key, info.semantic_bytes);
                if !matches!(
                    read::value(
                        ArtifactConfigBodyKindV1::Module,
                        a,
                        &entry.value,
                        &pair,
                        self.work.max_payload_bytes,
                        &mut local.records(),
                        &mut Cursor::new(bytes.as_mut_slice())
                    )?,
                    ArtifactConfigValueV1::Text { .. }
                ) {
                    return Err(read::invalid());
                }
                if values
                    .insert(
                        member,
                        String::from_utf8(bytes).map_err(|_| read::invalid())?,
                    )
                    .is_some()
                {
                    return Err(read::invalid());
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let source_error = (local.take_state_error)();
        if let Some(error) = source_error {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("selected cap scan"));
            }
            _ => return Err(read::invalid()),
        }
        if values.len() as u64 != length {
            return Err(read::invalid());
        }
        Ok(values)
    }
}

pub fn read_selected_module_manifest_v1<M, R, E, C>(
    body: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ArtifactConfigReadWorkV1,
    scan: &mut StateScanBudget,
    limits: &SelectedModuleManifestLimitsV1,
) -> Result<ReservedValue<SelectedModuleManifestV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary
        || !body.uses_budget(ctx.values.memory)
        || body.allocation_class() != ctx.values.class
        || body.reserved_bytes() < 512
        || body.kind() != ArtifactConfigBodyKindV1::Module
    {
        return Err(read::invalid());
    }
    body.validate_binding(ArtifactConfigBodyKindV1::Module, ctx.root)?;
    let charge = ctx
        .values
        .memory
        .reserve(limits.heap_bytes, ctx.values.class)?;
    let _stack = ctx.values.memory.reserve(64 << 10, ctx.values.class)?;
    let mut d = Decoder {
        body,
        ctx,
        work,
        heap: limits.heap_bytes,
        limit: limits,
        json: Vec::new(),
        known: BTreeSet::new(),
    };
    spend(&mut d.heap, 2048)?;
    let module_id = d.text(address("manifest", None, None, "module_id"))?;
    let name = d.text(address("manifest", None, None, "name"))?;
    let version = d.text(address("manifest", None, None, "version"))?;
    let kind = match d.get(address("manifest", None, None, "kind"))?.0 {
        ArtifactConfigValueV1::Label("pure") => ModuleKind::Pure,
        ArtifactConfigValueV1::Label("reducer") => ModuleKind::Reducer,
        _ => return Err(read::invalid()),
    };
    let role = match d.get(address("manifest", None, None, "role"))?.0 {
        ArtifactConfigValueV1::Label("rule") => ModuleRole::Rule,
        ArtifactConfigValueV1::Label("domain") => ModuleRole::Domain,
        ArtifactConfigValueV1::Label("gameplay") => ModuleRole::Gameplay,
        ArtifactConfigValueV1::Label("body") => ModuleRole::Body,
        ArtifactConfigValueV1::Label("agent_internal") => ModuleRole::AgentInternal,
        _ => return Err(read::invalid()),
    };
    let wasm_hash = d.text(address("manifest", None, None, "wasm_hash"))?;
    let interface_version = d.text(address("manifest", None, None, "interface_version"))?;
    let exports = d.texts("exports", None)?;
    let required_caps = d.texts("required_caps", None)?;
    let module_limits = ModuleLimits {
        max_mem_bytes: d.uint(address("limits", None, None, "max_mem_bytes"))?,
        max_gas: d.uint(address("limits", None, None, "max_gas"))?,
        max_call_rate: d
            .uint(address("limits", None, None, "max_call_rate"))?
            .try_into()
            .map_err(|_| read::invalid())?,
        max_output_bytes: d.uint(address("limits", None, None, "max_output_bytes"))?,
        max_effects: d
            .uint(address("limits", None, None, "max_effects"))?
            .try_into()
            .map_err(|_| read::invalid())?,
        max_emits: d
            .uint(address("limits", None, None, "max_emits"))?
            .try_into()
            .map_err(|_| read::invalid())?,
    };
    let length = d.length(
        "subscriptions",
        None,
        std::mem::size_of::<ModuleSubscription>() + 128,
    )?;
    let mut subscriptions = Vec::with_capacity(length as usize);
    for i in 0..length {
        let stage = match d.get(address("subscriptions", Some(i), None, "stage"))?.0 {
            ArtifactConfigValueV1::Unit => None,
            ArtifactConfigValueV1::Label(v) => Some(match v {
                "pre_action" => ModuleSubscriptionStage::PreAction,
                "post_action" => ModuleSubscriptionStage::PostAction,
                "post_event" => ModuleSubscriptionStage::PostEvent,
                "tick" => ModuleSubscriptionStage::Tick,
                _ => return Err(read::invalid()),
            }),
            _ => return Err(read::invalid()),
        };
        subscriptions.push(ModuleSubscription {
            event_kinds: d.texts("subscriptions.event_kinds", Some(i))?,
            action_kinds: d.texts("subscriptions.action_kinds", Some(i))?,
            stage,
            filters: d.filters(i)?,
        });
    }
    let abi_version = d
        .optional_uint(address("abi_contract", None, None, "abi_version"))?
        .map(u32::try_from)
        .transpose()
        .map_err(|_| read::invalid())?;
    let input_schema = d.optional_text(address("abi_contract", None, None, "input_schema"))?;
    let output_schema = d.optional_text(address("abi_contract", None, None, "output_schema"))?;
    let policy_hooks = d.texts("abi_contract.policy_hooks", None)?;
    let gameplay = if d.boolean(address("abi_contract", None, None, "gameplay_present"))? {
        let kind = match d
            .get(address("abi_contract.gameplay", None, None, "kind"))?
            .0
        {
            ArtifactConfigValueV1::Label(v) => match v {
                "war" => GameplayModuleKind::War,
                "governance" => GameplayModuleKind::Governance,
                "crisis" => GameplayModuleKind::Crisis,
                "economic" => GameplayModuleKind::Economic,
                "meta" => GameplayModuleKind::Meta,
                _ => return Err(read::invalid()),
            },
            _ => return Err(read::invalid()),
        };
        Some(GameplayContract {
            kind,
            game_modes: d.texts("abi_contract.gameplay.game_modes", None)?,
            min_players: d
                .uint(address("abi_contract.gameplay", None, None, "min_players"))?
                .try_into()
                .map_err(|_| read::invalid())?,
            max_players: d
                .optional_uint(address("abi_contract.gameplay", None, None, "max_players"))?
                .map(u16::try_from)
                .transpose()
                .map_err(|_| read::invalid())?,
        })
    } else {
        None
    };
    let length = d.length(
        "abi_contract.declarations.commands",
        None,
        std::mem::size_of::<ModuleCommandDeclaration>() + 128,
    )?;
    let mut commands = Vec::with_capacity(length as usize);
    for i in 0..length {
        commands.push(ModuleCommandDeclaration {
            namespace: d.text(address(
                "abi_contract.declarations.commands",
                Some(i),
                None,
                "namespace",
            ))?,
            name: d.text(address(
                "abi_contract.declarations.commands",
                Some(i),
                None,
                "name",
            ))?,
            schema_version: d
                .uint(address(
                    "abi_contract.declarations.commands",
                    Some(i),
                    None,
                    "schema_version",
                ))?
                .try_into()
                .map_err(|_| read::invalid())?,
            schema_hash: d.text(address(
                "abi_contract.declarations.commands",
                Some(i),
                None,
                "schema_hash",
            ))?,
            max_payload_bytes: d.uint(address(
                "abi_contract.declarations.commands",
                Some(i),
                None,
                "max_payload_bytes",
            ))?,
        });
    }
    let artifact_identity =
        if d.boolean(address("manifest", None, None, "artifact_identity_present"))? {
            Some(ModuleArtifactIdentity {
                source_hash: d.text(address("artifact_identity", None, None, "source_hash"))?,
                build_manifest_hash: d.text(address(
                    "artifact_identity",
                    None,
                    None,
                    "build_manifest_hash",
                ))?,
                signer_node_id: d.text(address(
                    "artifact_identity",
                    None,
                    None,
                    "signer_node_id",
                ))?,
                signature_scheme: d.text(address(
                    "artifact_identity",
                    None,
                    None,
                    "signature_scheme",
                ))?,
                artifact_signature: d.text(address(
                    "artifact_identity",
                    None,
                    None,
                    "artifact_signature",
                ))?,
            })
        } else {
            None
        };
    let cap_slots = d.cap_slots(scan)?;
    let manifest = ModuleManifest {
        module_id,
        name,
        version,
        kind,
        role,
        wasm_hash,
        interface_version,
        exports,
        subscriptions,
        required_caps,
        abi_contract: ModuleAbiContract {
            abi_version,
            input_schema,
            output_schema,
            cap_slots,
            policy_hooks,
            gameplay,
            declarations: ModuleSchemaDeclarations { commands },
        },
        artifact_identity,
        limits: module_limits,
    };
    d.ctx.check()?;
    Ok(charge.retain(SelectedModuleManifestV1 {
        manifest,
        root: d.ctx.root.clone(),
        class: d.ctx.values.class,
        _json: d.json,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{incremental_record_tree as tree, incremental_semantic_key::SemanticKeyBudget};
    use oasis7_formal_protocol::incremental_value::ValueReadContext;
    use oasis7_proto::memory_budget::MemoryBudget;
    use std::{cell::Cell, rc::Rc, sync::Arc};
    struct Forbid<'a> {
        pages: &'a body_tests::Pages,
        root: Cell<Option<[u8; 32]>>,
    }
    impl ValueNodeReader for Forbid<'_> {
        fn read_node(
            &self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
            class: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            assert_ne!(
                self.root.get().as_ref(),
                Some(id),
                "unused registered_by read"
            );
            self.pages.read_node(id, max, memory, class)
        }
    }
    #[test]
    fn selected_manifest_reconstructs_exact_original_fields_and_json_sha_identity() {
        let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
        let _index = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
        let mut pages = body_tests::Pages {
            nodes: Rc::new(RefCell::new(BTreeMap::<
                [u8; 32],
                Arc<ReservedValue<Vec<u8>>>,
            >::new())),
        };
        let mut original = super::super::tests::module_fixture();
        original.subscriptions[0].filters =
            Some(serde_json::json!({"": [null,-0.0,u64::MAX,"字🙂"],"z":{"a":true}}));
        let expected = serde_json::to_vec(&original).unwrap();
        let expected_hash = crate::util::hash_json(&original).unwrap();
        let record = ModuleRecord {
            manifest: original,
            registered_at: u64::MAX,
            registered_by: "r".repeat(1 << 20),
            audit_event_id: Some(0),
        };
        let root = body_tests::produce(genesis::Body::Module(&record), &backing, &mut pages);
        drop(record);
        let source = Forbid {
            pages: &pages,
            root: Cell::new(None),
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let mut state_io = body_tests::io();
        let state_io = RefCell::new(&mut state_io);
        let mut stop = || false;
        let cancel = RefCell::new(&mut stop);
        let error = RefCell::new(None);
        let state = tree::Source {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &state_io,
            cancelled: &cancel,
            error: &error,
            labels: genesis::LABELS,
        };
        let mut value_io = body_tests::io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        };
        let mut scan = StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10_000,
        };
        let mut work = ArtifactConfigReadWorkV1 {
            cells_remaining: 100_000,
            max_payload_bytes: 1 << 20,
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 100_000,
                json_nodes_remaining: 1000,
                max_depth: 16,
                max_payload_bytes: 1 << 20,
            },
        };
        let body = qualify_artifact_config_body_v1(
            ArtifactConfigBodyKindV1::Module,
            &mut ctx,
            &mut scan,
            &mut work,
        )
        .unwrap();
        let owner_key = read::key(
            address("record", None, None, "registered_by"),
            &mut ctx.records(),
        )
        .unwrap();
        let owner_root = ctx.lookup(&owner_key.0).unwrap().unwrap();
        source.root.set(Some(owner_root));
        let selected = read_selected_module_manifest_v1(
            &body,
            &mut ctx,
            &mut work,
            &mut scan,
            &SelectedModuleManifestLimitsV1 {
                heap_bytes: 512 << 10,
                json_heap_bytes: 128 << 10,
                max_collection_items: 1000,
            },
        )
        .unwrap();
        assert_eq!(serde_json::to_vec(selected.manifest()).unwrap(), expected);
        assert_eq!(
            crate::util::hash_json(selected.manifest()).unwrap(),
            expected_hash
        );
        assert_eq!(selected.root(), &root);
        assert!(selected.uses_budget(&memory));
    }
}

#[path = "incremental_runtime_artifact_config_module_clone.rs"]
mod clone_admission;
