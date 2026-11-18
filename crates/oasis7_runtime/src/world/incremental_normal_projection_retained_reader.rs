//! Paid original IO for fixed retained-history formatter consumers. A reader
//! supplies bytes only; the consumer authenticates the path against the actual
//! Current retained-history root and applies every original leaf predicate.
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
pub trait PaidProjectionRetainedOriginalReaderV1 {
    type RetainedError;
    fn allocation_budget(&self) -> &MemoryBudget;
    fn allocation_class(&self) -> MemoryClass;
    /// Admit IO and the complete Vec graph on this same continuing Common before
    /// returning. No unbounded reader fallback or source qualification is implied.
    fn read_retained_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::RetainedError>;
}

#[path="incremental_normal_projection_retained_reader_node.rs"]
mod node;
pub(in crate::world) use node::get_retained_index_leaf_bounded_v1;

#[path="incremental_normal_projection_retained_reader_canonical.rs"]
mod canonical;
#[path="incremental_normal_projection_retained_reader_decode.rs"]
mod decode;

#[path="incremental_normal_projection_retained_reader_tasks.rs"]
mod tasks;

pub(in crate::world) use tasks::read_task_results_data_v1;

pub(in crate::world) use decode::decode_hot_service_result_json_data_v1;

pub(in crate::world) use decode::canonical_task_owner_digest_data_v1;

pub(in crate::world) use decode::PaidDecodedRetainedServiceDataV1;

pub(in crate::world) use decode::is_canonical_empty_retained_root_data_v1;

pub(in crate::world) use decode::equal_canonical_service_result_data_v1;


pub(in crate::world) use decode::decode_current_task_terminal_index_data_v2;

pub(in crate::world) use decode::{decode_terminal_stop_witness_data_v2,charge_terminal_record_data_v2};

pub(in crate::world) use decode::blake3_raw_data_v1;

#[path="incremental_normal_projection_retained_reader_chat.rs"]
mod chat;
pub(in crate::world) use chat::read_chat_results_data_v1;
