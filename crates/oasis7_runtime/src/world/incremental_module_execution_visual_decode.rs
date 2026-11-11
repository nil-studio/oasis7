//! Budget admission for the original visual payload decoder, with no alternate
//! visual wire/schema and no state/permission qualification.
use super::incremental_module_execution_effects::heap::{EffectCloneWorkV1, json_clone_bound_v1};
use oasis7_agent_api::{
    module_visual::ModuleVisualMutation,
    world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_wasm_abi::ModuleEmitEvent;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub struct RootModuleVisualDecodeWorkV1 {
    pub nodes_remaining: u64,
    pub text_bytes_remaining: u64,
    pub max_depth: usize,
    pub max_heap_bytes: usize,
}
#[derive(Debug)]
pub enum RootModuleVisualDecodeErrorV1 {
    Value(ValueTreeError),
    Business(ReservedValue<oasis7_world_state::WorldError>),
}
fn inventory() -> bool {
    static CHECKED: OnceLock<bool> = OnceLock::new();
    *CHECKED.get_or_init(|| {
        fn digest(bytes: &[u8]) -> [u8; 32] {
            let mut hash = Sha256::new();
            let mut previous = 0;
            for (index, byte) in bytes.iter().enumerate() {
                if *byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                    hash.update(&bytes[previous..index]);
                    previous = index + 1;
                }
            }
            hash.update(&bytes[previous..]);
            hash.finalize().into()
        }
        let expected_module_visual: [u8; 32] = [
            94, 205, 168, 29, 67, 68, 245, 16, 231, 234, 102, 108, 15, 105, 144, 76, 89, 113, 121,
            33, 19, 132, 25, 193, 40, 69, 125, 220, 167, 25, 172, 245,
        ];
        if digest(include_bytes!(
            "../../../oasis7_agent_api/src/module_visual.rs"
        )) != expected_module_visual
        {
            return false;
        }
        let expected_world_entity: [u8; 32] = [
            12, 100, 17, 221, 197, 180, 6, 136, 32, 69, 181, 168, 220, 117, 64, 11, 128, 213, 137,
            9, 86, 47, 45, 103, 190, 101, 1, 113, 156, 192, 177, 40,
        ];
        if digest(include_bytes!(
            "../../../oasis7_agent_api/src/world_entity.rs"
        )) != expected_world_entity
        {
            return false;
        }
        true
    })
}
// The two pinned parser sources have only String, Option<String> and the closed
// Anchor/GeoPos DTO. No business Vec/Map/Box/default allocation is hidden here.
fn layout(value: &ModuleVisualEntity) {
    let ModuleVisualEntity {
        entity_id,
        module_id,
        kind,
        label,
        anchor,
    } = value;
    let _: &String = entity_id;
    let _: &String = module_id;
    let _: &String = kind;
    let _: &Option<String> = label;
    match anchor {
        ModuleVisualAnchor::Agent { agent_id } => {
            let _: &String = agent_id;
        }
        ModuleVisualAnchor::Location { location_id } => {
            let _: &String = location_id;
        }
        ModuleVisualAnchor::Absolute { pos } => {
            let _: &oasis7_agent_api::world_action::GeoPos = pos;
        }
    }
}
const _: () = assert!(!std::mem::needs_drop::<
    oasis7_agent_api::world_action::GeoPos,
>());
const _: () = assert!(std::mem::size_of::<ModuleVisualEntity>() <= 1024);
const _: () = assert!(
    std::mem::size_of::<(
        serde::__private229::de::Content<'static>,
        serde::__private229::de::Content<'static>
    )>() <= 512
);

/// Keep this fee alive through the resulting owner mutation. Sixteen copies of
/// the JSON clone bound cover original clone, adjacent-tag Content/typed DTO,
/// container spare capacity, sanitizer copies and escaped Serde failure text.
/// The base bound already charges 4 bytes per observed string byte and a full
/// 16-slot BTree node per JSON member. No encoded-byte-size DTO assumption.
pub fn decode_root_module_visual_emit_v1(
    event: &ModuleEmitEvent,
    memory: &MemoryBudget,
    work: &mut RootModuleVisualDecodeWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<Option<ModuleVisualMutation>>, RootModuleVisualDecodeErrorV1> {
    let value_error = RootModuleVisualDecodeErrorV1::Value;
    oasis7_world_state::incremental_world_event_decode::verify_module_output_layout_inventory_v1(
        memory,
        MemoryClass::Ordinary,
    )
    .map_err(value_error)?;
    if !inventory() {
        return Err(value_error(ValueTreeError::Invalid(
            "module visual decode layout inventory changed",
        )));
    }
    let mut clone_work = EffectCloneWorkV1 {
        nodes: work.nodes_remaining,
        bytes: work.text_bytes_remaining,
        max_depth: work.max_depth,
    };
    let bound = json_clone_bound_v1(&event.payload, &mut clone_work, 0, cancelled);
    work.nodes_remaining = clone_work.nodes;
    work.text_bytes_remaining = clone_work.bytes;
    let fee = bound
        .map_err(value_error)?
        .checked_mul(16)
        .and_then(|n| {
            event
                .module_id
                .len()
                .checked_add(event.kind.len())
                .and_then(|n| n.checked_add(event.trace_id.len()))
                .and_then(|s| s.checked_mul(16))
                .and_then(|s| n.checked_add(s))
        })
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(|| {
            value_error(ValueTreeError::ResourceLimited(
                "module visual payload admission",
            ))
        })?;
    if fee > work.max_heap_bytes {
        return Err(value_error(ValueTreeError::ResourceLimited(
            "module visual payload heap",
        )));
    }
    let charge = memory
        .reserve(fee, MemoryClass::Ordinary)
        .map_err(|e| value_error(ValueTreeError::Memory(e)))?;
    if cancelled() {
        return Err(value_error(ValueTreeError::Cancelled));
    }
    let value = oasis7_agent_api::module_visual::parse_module_visual_emit(event);
    if cancelled() {
        return Err(value_error(ValueTreeError::Cancelled));
    }
    match value {
        Ok(value) => {
            if let Some(ModuleVisualMutation::Upsert(entity)) = &value {
                layout(entity);
            }
            Ok(charge.retain(value))
        }
        Err(error) => Err(RootModuleVisualDecodeErrorV1::Business(
            charge.retain(oasis7_world_state::WorldError::Serde(error)),
        )),
    }
}
