//! Concrete upper side roots reuse the single client-owned service inventory.
use super::*;
fn from_client(
    value: oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1,
) -> Layout {
    let oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    } = value;
    Layout {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    }
}
fn to_client(
    value: Layout,
) -> oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1 {
    let Layout {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    } = value;
    oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    }
}
macro_rules! service_root {($($ty:ty),+) => {$(impl OwnedLayout for $ty{
    fn layout<LE>(runtime:Layout,visit:&mut dyn FnMut()->Result<(),LE>)->Result<Layout,LE>{
        visit()?;let mut out=Layout::scalar::<Self>();out.merge(from_client(oasis7_client_api::consensus_action_wire::legacy_payload_owned_layout_v1(to_client(runtime),visit)?));Ok(out)
    }
})+};}
service_root!(
    oasis7_client_api::world_service::RequestCorrelation,
    oasis7_client_api::MainTokenActionAuthProof,
    crate::world_service::WorldServicePayloadV1,
    crate::world_service::task_cleanup_v2::TaskCleanupSubmitV2
);
// Shared AgentAPI GeoPos is already in the actual WS Action graph.
runtime_layout!(oasis7_agent_api::world_action::GeoPos);
