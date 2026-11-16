//! Type-derived owned layouts of the existing closed Codec graph. DATA only.
//! No value is constructed or decoded while following field projections.
use super::*;
#[derive(Clone,Copy,Debug)]
pub struct PendingActionOwnedLayoutV1{
 pub root:usize,pub sequence_element:usize,pub map_pair:usize,pub set_element:usize,pub box_pointee:usize,
 pub default_heap_per_value:usize,pub symbols_bytes:usize,pub visits:usize,pub missing:Option<&'static str>,
}
impl PendingActionOwnedLayoutV1{
 pub fn scalar<T>()->Self{Self{root:std::mem::size_of::<T>(),sequence_element:0,map_pair:0,set_element:0,box_pointee:0,default_heap_per_value:0,symbols_bytes:0,visits:1,missing:None}}
 pub(crate) fn absent<T>()->Self{let mut v=Self::scalar::<T>();v.missing=Some(std::any::type_name::<T>());v}
 pub fn merge(&mut self,v:Self){self.sequence_element=self.sequence_element.max(v.sequence_element);self.map_pair=self.map_pair.max(v.map_pair);self.set_element=self.set_element.max(v.set_element);self.box_pointee=self.box_pointee.max(v.box_pointee);self.default_heap_per_value=self.default_heap_per_value.max(v.default_heap_per_value);self.symbols_bytes=self.symbols_bytes.saturating_add(v.symbols_bytes);self.visits=self.visits.saturating_add(v.visits);self.missing=self.missing.or(v.missing)}
 pub fn symbol(&mut self,name:&str){self.symbols_bytes=self.symbols_bytes.saturating_add(name.len()+8)}
 pub(crate) fn sequence<T:Codec,LE>(&mut self,visit:&mut dyn FnMut()->Result<(),LE>)->Result<(),LE>{self.sequence_element=self.sequence_element.max(std::mem::size_of::<T>());self.merge(T::owned_decode_layout_v1(visit)?);Ok(())}
 pub(crate) fn boxed<T:Codec,LE>(&mut self,visit:&mut dyn FnMut()->Result<(),LE>)->Result<(),LE>{self.box_pointee=self.box_pointee.max(std::mem::size_of::<T>());self.merge(T::owned_decode_layout_v1(visit)?);Ok(())}
}
pub(crate) fn projected<O,T:Codec,LE>(_:for<'a>fn(&'a O)->&'a T,visit:&mut dyn FnMut()->Result<(),LE>)->Result<PendingActionOwnedLayoutV1,LE>{T::owned_decode_layout_v1(visit)}
/// This follows the original all-92 enum and every original codec field list.
/// A hand-written reachable codec must explicitly describe its own children.
pub fn pending_action_owned_decode_layout_v1<LE>(visit:&mut dyn FnMut()->Result<(),LE>)->Result<PendingActionOwnedLayoutV1,LE>{ActionEnvelope::owned_decode_layout_v1(visit)}
#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn original_action_envelope_layout_closes_every_codec_child(){let mut visited=0usize;let layout=pending_action_owned_decode_layout_v1(&mut ||{visited+=1;Ok::<_,std::convert::Infallible>(())}).unwrap();assert_eq!(layout.missing,None,"uncovered original child: {:?}",layout.missing);assert_eq!(layout.visits,visited);assert_eq!(layout.root,std::mem::size_of::<ActionEnvelope>());assert!(layout.symbols_bytes>0);}
 #[test] fn layout_preserves_resource_callback_first_cause(){let mut visits=0;let result=pending_action_owned_decode_layout_v1(&mut ||{visits+=1;Err::<(),_>("typed cancel")});assert!(matches!(result,Err("typed cancel")));assert_eq!(visits,1);}
}

impl From<PendingActionOwnedLayoutV1> for oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLayoutV1{
 fn from(v:PendingActionOwnedLayoutV1)->Self{Self{root:v.root,sequence_element:v.sequence_element,map_pair:v.map_pair,set_element:v.set_element,box_pointee:v.box_pointee,default_heap_per_value:v.default_heap_per_value,symbols_bytes:v.symbols_bytes,visits:v.visits,missing:v.missing}}
}
