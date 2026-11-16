//! Readonly original bid-index node DATA. It contains no family/root proof.
use super::*;
#[derive(Clone,Copy,Debug)]pub struct MarketplaceBidIndexNodeDataV1{fields:[u64;7]}
impl MarketplaceBidIndexNodeDataV1{pub fn fields(&self)->[u64;7]{self.fields}}
#[derive(Clone,Copy,Debug)]pub struct MarketplaceBidIndexMetaDataV1{length:u64,root_id:u64,next_id:u64}
impl MarketplaceBidIndexMetaDataV1{pub fn length(&self)->u64{self.length}pub fn root_id(&self)->u64{self.root_id}pub fn next_id(&self)->u64{self.next_id}}
/// One-time GEN uses the same original node-shape kernel as append_balanced.
/// The caller must still bind its actual source/record/root before cache use.
pub fn visit_original_genesis_bid_index_data_v1<E:From<MemberRecordError>>(length:u64,work:&mut dyn FnMut(usize)->Result<(),E>,emit:&mut dyn FnMut(u64,MarketplaceBidIndexNodeDataV1)->Result<(),E>)->Result<MarketplaceBidIndexMetaDataV1,E>{let(root,summary)=bid_index::visit_balanced_nodes_v1(0,length,bid_index::NO_BID_ID,work,&mut|id,node|emit(id,MarketplaceBidIndexNodeDataV1{fields:node.fields()}))?;if summary.size!=length{return Err(E::from(MemberRecordError::Invalid("marketplace original GEN index length")))}Ok(MarketplaceBidIndexMetaDataV1{length,root_id:root,next_id:length})}
