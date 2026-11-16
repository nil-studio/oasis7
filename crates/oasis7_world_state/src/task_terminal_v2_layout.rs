//! Nominal allocation graph for the original permanent task record codecs.
use super::*;
use oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1 as Layout;
use std::mem::size_of;
trait Owned{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>;}
fn projected<O,T:Owned,E>(_:for<'a>fn(&'a O)->&'a T,visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{T::layout(visit)}
macro_rules! record{($ty:ty,{$($field:ident),+})=>{impl Owned for $ty{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{let _:fn(&Self)=|v|{let Self{$($field:_),+}=v;};visit()?;let mut out=Layout::scalar::<Self>();$(out.symbol(stringify!($field));out.symbol(stringify!($field));out.merge(projected::<Self,_,E>(|v|&v.$field,visit)?);)+Ok(out)}}};}
macro_rules! primitive{($($ty:ty),+)=>{$(impl Owned for $ty{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{visit()?;Ok(Layout::scalar::<Self>())}})+};}
primitive!(String,u8,u32,u64);
impl<T:Owned> Owned for Option<T>{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{visit()?;let mut out=Layout::scalar::<Self>();out.merge(T::layout(visit)?);Ok(out)}}
impl<T:Owned> Owned for Vec<T>{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{visit()?;let mut out=Layout::scalar::<Self>();out.sequence_element=size_of::<T>();out.merge(T::layout(visit)?);Ok(out)}}
impl<K:Owned,V:Owned> Owned for BTreeMap<K,V>{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{visit()?;let mut out=Layout::scalar::<Self>();out.map_pair=size_of::<(K,V)>();out.merge(K::layout(visit)?);out.merge(V::layout(visit)?);Ok(out)}}
impl Owned for crate::service_admission::SignedReadRequest<GameTaskRevokeAndCloseV2>{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{oasis7_client_api::consensus_action_wire::task_cleanup_request_owned_layout_v1(visit)}}
impl Owned for GameTaskClosedV2{fn layout<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{oasis7_client_api::consensus_action_wire::task_closed_owned_layout_v1(visit)}}
record!(TaskTerminalRecordV2,{schema_revision,original,committed_height,committed_at_unix_ms,stop_predecessor_canonical_cbor,closed});
record!(TaskTerminalIndexV2,{schema_revision,records});
pub fn task_terminal_record_owned_layout_v2<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{TaskTerminalRecordV2::layout(visit)}
pub fn task_terminal_index_owned_layout_v2<E>(visit:&mut dyn FnMut()->Result<(),E>)->Result<Layout,E>{TaskTerminalIndexV2::layout(visit)}
