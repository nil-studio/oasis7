//! Closed nominal allocation graph for the original retained-leaf codec.
//! No decoding, source qualification, writer or alternate wire rule lives here.
use super::*;
use oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1 as Layout;
use std::mem::size_of;
pub(super) trait OwnedLayout {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E>;
}
pub(super) fn projected<O, T: OwnedLayout, E>(
    _: for<'a> fn(&'a O) -> &'a T,
    visit: &mut dyn FnMut() -> Result<(), E>,
) -> Result<Layout, E> {
    T::layout(visit)
}
pub(super) fn variant<O, T: OwnedLayout, E>(
    _: for<'a> fn(&'a O) -> Option<&'a T>,
    visit: &mut dyn FnMut() -> Result<(), E>,
) -> Result<Layout, E> {
    T::layout(visit)
}
macro_rules! record{($ty:ty,{$($field:ident),+$(,)?})=>{impl $crate::formal_retained_history::owned_layout::OwnedLayout for $ty{
 fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1,E>{
  let _:fn(&Self)=|value|{let Self{$($field:_),+}=value;};visit()?;let mut out=oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1::scalar::<Self>();
  $(out.symbol(stringify!($field));out.symbol(stringify!($field));out.merge($crate::formal_retained_history::owned_layout::projected::<Self,_,E>(|v|&v.$field,visit)?);)+Ok(out)
 }
}};}
pub(super) use record;
macro_rules! scalar{($($ty:ty),+)=>{$(impl OwnedLayout for $ty{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{visit()?;Ok(Layout::scalar::<Self>())}})+};}
scalar!(String, u8, u32, u64, i64, bool);
impl<T: OwnedLayout> OwnedLayout for Option<T> {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.merge(T::layout(visit)?);
        Ok(out)
    }
}
impl<T: OwnedLayout> OwnedLayout for Vec<T> {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.sequence_element = size_of::<T>();
        out.merge(T::layout(visit)?);
        Ok(out)
    }
}
impl<T: OwnedLayout> OwnedLayout for std::collections::BTreeSet<T> {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.set_element = size_of::<T>();
        out.merge(T::layout(visit)?);
        Ok(out)
    }
}
impl OwnedLayout for serde_json::Value {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        visit()?;
        let mut out = Layout::scalar::<Self>();
        out.sequence_element = size_of::<Self>();
        out.map_pair = size_of::<(String, Self)>();
        Ok(out)
    }
}
impl OwnedLayout for CapabilityInvocationContext {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        oasis7_client_api::consensus_action_wire::capability_invocation_context_owned_layout_v1(
            visit,
        )
    }
}
record!(CapabilityAuthorizationNonceRecord,{request_hash,outcome_hash,committed_receipt_id,state});
record!(CapabilityAuthorizationAuditReceipt,{receipt_id,root_receipt_id,grant_id,subject,presenter,audience,scope_hash,module_id,module_version,manifest_hash,catalog_snapshot_id,response_nonce,authorization_nonce_key_hash,decision,denial_code,budget_before,budget_after,world_head_before,world_head_after,branch_id,finality_epoch,finality_block_hash,finality_status,state_hash_before,state_hash_after,committed_effect_receipt_id,committed_effect_receipt_ids,canonical_request_hash,canonical_result_hash});
record!(AgentIntentV2,{schema_version,agent_id,intent_id,kind,summary,target_id,effect_intent_id,intent_tick,world_id,reorg_epoch,authority_scope,status,source,logical_time,event_seq,updated_at,receipt_ref,reason_code,reason_summary,replaced_by,actor_id,request_digest});
record!(RetainedCapabilityInvocationContextV1,{map_key,context});
record!(RetainedCapabilityNonceV1,{nonce_key,record,receipt,invocation_contexts});
record!(RetainedAgentIntentV1,{intent,replay_identity_digest});
record!(RetainedWorldServiceResultV1,{correlation_key_digest,canonical_result_cbor,canonical_result_digest});
record!(RetainedOwnerChatNonceHighWaterV1,{owner_public_key_digest,high_water_nonce,witness_result_key_digest});
record!(RetainedResultRefsV1, { result_key_digests });
record!(RetainedTaskChainV1,{head,count});
record!(RetainedTaskChainLinkV1,{result_key_digest,previous});
impl OwnedLayout for DistinctReorgEpochSummaryV1 {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        let _: fn(&Self) = |value| match value {
            Self::Empty => {}
            Self::Single(_) => {}
            Self::Multiple { max_epoch: _ } => {}
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        for s in ["kind", "epoch", "empty", "single", "multiple", "max_epoch"] {
            visit()?;
            out.symbol(s);
            out.symbol(s)
        }
        Ok(out)
    }
}
impl OwnedLayout for RetainedHistoryLeafV1 {
    fn layout<E>(visit: &mut dyn FnMut() -> Result<(), E>) -> Result<Layout, E> {
        let _: fn(&Self) = |value| match value {
            Self::WorldServiceResult(_) => {}
            Self::JournalEvent(_) => {}
            Self::JournalTickLocator(_) => {}
            Self::TickConsensusRecord(_) => {}
            Self::TickConsensusHeight(_) => {}
            Self::OwnerChatNonceHighWater(_) => {}
            Self::AgentChatIntentIndex(_) => {}
            Self::FeedbackPublicationIndex(_) => {}
            Self::GameTaskControlIndex(_) => {}
            Self::GameTaskControlChainIndex(_) => {}
            Self::GameTaskControlLink(_) => {}
            Self::CapabilityNonce(_) => {}
            Self::AgentIntent(_) => {}
            Self::AgentIntentReorgEpochs(_) => {}
        };
        visit()?;
        let mut out = Layout::scalar::<Self>();
        macro_rules! arm{($($v:ident),+)=>{$(out.symbol(stringify!($v));out.symbol(stringify!($v));out.merge(variant::<Self,_,E>(|value|match value{Self::$v(inner)=>Some(inner),_=>None},visit)?);)+};}
        arm!(
            WorldServiceResult,
            JournalEvent,
            JournalTickLocator,
            TickConsensusRecord,
            TickConsensusHeight,
            OwnerChatNonceHighWater,
            AgentChatIntentIndex,
            FeedbackPublicationIndex,
            GameTaskControlIndex,
            GameTaskControlChainIndex,
            GameTaskControlLink,
            CapabilityNonce,
            AgentIntent,
            AgentIntentReorgEpochs
        );
        Ok(out)
    }
}
pub fn retained_history_leaf_owned_layout_v1<E>(
    visit: &mut dyn FnMut() -> Result<(), E>,
) -> Result<Layout, E> {
    RetainedHistoryLeafV1::layout(visit)
}
