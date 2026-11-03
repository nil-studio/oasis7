//! RootSet-bound snapshot access without materializing an Intent or its params.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_record_value_read::RecordValueV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
#[derive(Clone, Copy, Debug)]
pub enum RuntimeEffectRecordKeyV1<'a> {
    Pending(u64),
    Inflight(&'a str),
}
impl RuntimeEffectRecordKeyV1<'_> {
    fn kind(self) -> RuntimeEffectKindV1 {
        match self {
            Self::Pending(_) => RuntimeEffectKindV1::Pending,
            Self::Inflight(_) => RuntimeEffectKindV1::Inflight,
        }
    }
    fn key<R: ValueNodeReader, C: FnMut() -> bool>(
        self,
        records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
        match self {
            Self::Pending(ordinal) => records.key(
                self.kind().descriptor(),
                WorldFieldRecordKindV1::Entry,
                &ordinal,
            ),
            Self::Inflight(key) => {
                records.key(self.kind().descriptor(), WorldFieldRecordKindV1::Entry, key)
            }
        }
    }
}
#[derive(Debug)]
pub struct RuntimeEffectRecordQualificationV1 {
    roots: [u8; 32],
    kind: RuntimeEffectKindV1,
    owner: [u8; 32],
    body: StateRootV1,
}
impl RuntimeEffectRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.body
    }
    pub fn validate_binding(
        &self,
        roots: &[u8; 32],
        kind: RuntimeEffectKindV1,
        owner: &[u8; 32],
    ) -> Result<(), ValueTreeError> {
        if &self.roots == roots && self.kind == kind && &self.owner == owner {
            Ok(())
        } else {
            Err(family::invalid())
        }
    }
}
/// Original scalar names only. Params has a separate exact child-root reader.
#[derive(Clone, Copy, Debug)]
pub enum RuntimeEffectFieldV1 {
    BirthEra,
    IntentId,
    Kind,
    CapRef,
    OriginKind,
    OriginName,
    OriginModuleId,
}
impl RuntimeEffectFieldV1 {
    fn profile(self) -> (&'static str, MemberPayloadCodecV1, bool) {
        match self {
            Self::BirthEra => ("birth_era", MemberPayloadCodecV1::U64, true),
            Self::IntentId => ("intent_id", MemberPayloadCodecV1::Text, false),
            Self::Kind => ("kind", MemberPayloadCodecV1::Text, false),
            Self::CapRef => ("cap_ref", MemberPayloadCodecV1::Text, false),
            Self::OriginKind => ("origin.kind", MemberPayloadCodecV1::Text, false),
            Self::OriginName => ("origin.name", MemberPayloadCodecV1::Text, true),
            Self::OriginModuleId => ("origin.module_id", MemberPayloadCodecV1::Text, true),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeEffectFieldValueV1 {
    Unit,
    Unsigned(u64),
    Text { utf8_bytes: u64 },
}
/// Inherits complete Body/JSON closure only from this exact family RootSet.
/// The record-local token never grants dispatch or business authority.
pub fn open_runtime_effect_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RuntimeEffectRecordKeyV1<'_>,
    roots: &RootSetV1,
    parent: &ReservedValue<RuntimeEffectFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RuntimeEffectRecordQualificationV1>>, ValueTreeError> {
    ctx.check()?;
    let digest = roots.digest().map_err(|_| family::invalid())?;
    if !parent.uses_budget(ctx.values.memory)
        || ctx.root != roots.state_root(StateNamespaceV1::Effects)
    {
        return Err(family::invalid());
    }
    parent.validate_binding(&digest)?;
    let retained = ctx.values.memory.reserve(1024, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let outer_root = ctx.root;
    let body = {
        let mut records = ctx.records();
        let outer = key.key(&mut records)?;
        let Some(id) = tree::lookup(outer_root, &outer.0, &mut records, LABELS)? else {
            ctx.check()?;
            return Ok(None);
        };
        let root = family::reference(
            key.kind(),
            &id,
            &outer,
            false,
            &mut records,
            &mut std::io::sink(),
        )?;
        let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(family::invalid)?;
        (root, owner)
    };
    let (root, owner) = body;
    let body = if key.kind() == RuntimeEffectKindV1::Pending {
        family::pending_item(
            &root,
            ctx,
            &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 5,
                json_nodes_remaining: 0,
                max_depth: 0,
                max_payload_bytes: 165,
            },
        )?
        .0
    } else {
        root
    };
    if body.namespace != StateNamespaceV1::Effects || body.leaf_count != 10 {
        return Err(family::invalid());
    }
    ctx.check()?;
    Ok(Some(retained.retain(RuntimeEffectRecordQualificationV1 {
        roots: digest,
        kind: key.kind(),
        owner,
        body,
    })))
}
fn bind<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    proof: &ReservedValue<RuntimeEffectRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if !proof.uses_budget(ctx.values.memory) || ctx.root != proof.root() {
        return Err(family::invalid());
    }
    Ok(())
}
/// Text output is provisional until success and the final cancellation check.
/// None and Some("") have distinct Unit/Text results; no field scans params.
pub fn read_qualified_runtime_effect_field_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    proof: &ReservedValue<RuntimeEffectRecordQualificationV1>,
    field: RuntimeEffectFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_text_bytes: u64,
    output: &mut impl std::io::Write,
) -> Result<RuntimeEffectFieldValueV1, ValueTreeError> {
    bind(proof, ctx)?;
    let _scratch = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let (name, codec, nullable) = field.profile();
    let value = {
        let mut records = ctx.records();
        let key = records.key(
            Space::Body.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &((), name),
        )?;
        let id = tree::lookup(proof.root(), &key.0, &mut records, LABELS)?
            .ok_or_else(family::invalid)?;
        family::read(
            &id,
            &key,
            codec,
            nullable,
            if codec == MemberPayloadCodecV1::U64 {
                8
            } else {
                max_text_bytes
            },
            &mut records,
            output,
        )?
    };
    ctx.check()?;
    match value {
        RecordValueV1::Unit => Ok(RuntimeEffectFieldValueV1::Unit),
        RecordValueV1::Unsigned(v) => Ok(RuntimeEffectFieldValueV1::Unsigned(v)),
        RecordValueV1::Text { utf8_bytes } => Ok(RuntimeEffectFieldValueV1::Text { utf8_bytes }),
        _ => Err(family::invalid()),
    }
}
/// Returns the original immutable JSON root without reading its payload graph.
/// The charged result belongs to this exact privately qualified body snapshot.
pub fn read_qualified_runtime_effect_params_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    proof: &ReservedValue<RuntimeEffectRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    bind(proof, ctx)?;
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let root = {
        let mut records = ctx.records();
        let key = records.key(
            Space::Body.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &((), "params"),
        )?;
        let id = tree::lookup(proof.root(), &key.0, &mut records, LABELS)?
            .ok_or_else(family::invalid)?;
        family::reference(
            RuntimeEffectKindV1::Inflight,
            &id,
            &key,
            true,
            &mut records,
            &mut std::io::sink(),
        )?
    };
    ctx.check()?;
    Ok(retained.retain(root))
}
