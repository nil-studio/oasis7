//! Original PreparedWorldRule front gates for the fixed system commands.
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader,ValueTreeError};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,
 incremental_world_rule_requests_current::{CurrentWorldRuleRequestV1,read_current_world_rule_request_v1},
 incremental_world_rule_composite_family::{bind_current_agent_orders_v1,read_current_agent_order_v1},
 incremental_agent_subtree::{read_current_agent_presence_v1,CurrentAgentPresenceV1}};
use crate::world::{incremental_factory_directory_orders::{AcceptedFactoryOrderDirectoryV1,open_current_factory_order_directory_v1},incremental_normal_tail::NormalTailWorkV1};
use oasis7_agent_api::world_action::WorldRuleCommandV1;
pub(super) struct OrganizationSystemHeaderV1 {pub(super) request:ReservedValue<CurrentWorldRuleRequestV1>,pub(super) retry:bool}
#[derive(Debug)]struct Infra<T:std::fmt::Debug>(T);
impl<T:std::fmt::Debug>std::fmt::Display for Infra<T>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"organization system header: {:?}",self.0)}}
impl<T:std::fmt::Debug>std::error::Error for Infra<T>{}
fn infra<T:std::fmt::Debug+Send+Sync+'static>(e:T)->BodyMutationError{BodyMutationError::Original(Box::new(Infra(e)))}
struct Measure<'a,'b,'c>{tail:&'a mut NormalTailWorkV1<'b,'c>,bytes:usize,too_large:bool,enforce_limit:bool,error:Option<BodyMutationError>}
impl std::io::Write for Measure<'_,'_,'_>{
 fn write(&mut self,b:&[u8])->std::io::Result<usize>{
 if let Err(error)=self.tail.debit(1,b.len() as u64){self.error=Some(error);return Err(std::io::Error::other("organization command metering"))}
 self.bytes=self.bytes.checked_add(b.len()).ok_or_else(||std::io::Error::other("command length"))?;
 if self.enforce_limit&&self.bytes>65536{self.too_large=true;return Err(std::io::Error::other("organization command too large"))}Ok(b.len())}
 fn flush(&mut self)->std::io::Result<()>{Ok(())}
}
fn command_fits(command:&WorldRuleCommandV1,tail:&mut NormalTailWorkV1<'_,'_>,enforce_limit:bool)->Result<bool,BodyMutationError>{
 let mut sink=Measure{tail,bytes:0,too_large:false,enforce_limit,error:None};let encoded=serde_json::to_writer(&mut sink,command);
 if let Some(error)=sink.error{return Err(error)}if sink.too_large{return Ok(false)}encoded.map_err(infra)?;Ok(true)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_system_header_v1<M,P,R,E,C,PE,PC>(actor:&str,request_id:&str,command:&WorldRuleCommandV1,binding:Option<&str>,
 sources:&NormalTailSourcesV1<'_,'_>,accepted_orders:&ReservedValue<AcceptedFactoryOrderDirectoryV1>,state:&M,
 ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 work:&mut NormalOrganizationTailWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)
 ->Result<Option<OrganizationSystemHeaderV1>,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 tail.charge(actor.len().checked_add(request_id.len()).ok_or_else(||invalid("organization header IDs"))?)?;
 if oasis7_world_state::world_rules::validate_id(actor).and_then(|_|oasis7_world_state::world_rules::validate_id(request_id)).is_err(){return Ok(None)}
 let guard=ctx.transaction.guard();
 if read_current_agent_presence_v1(actor,sources.raw().agents(),sources.roots(),ctx,parent,&mut work.pending_entries_remaining,&guard,live).map_err(infra)?!=CurrentAgentPresenceV1::Existing{return Ok(None)}
 let guard=ctx.transaction.guard();
 let request=read_current_world_rule_request_v1(actor,request_id,sources.native().rules().requests(),sources.roots(),state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?;
 if let Some(receipt)=request.value(){
 // Charge both actual closed command shapes before original PartialEq.
 let _=command_fits(&receipt.command,tail,false)?;let _=command_fits(command,tail,false)?;
 if receipt.actor_id!=actor||receipt.command!=*command{return Ok(None)}
 return Ok(Some(OrganizationSystemHeaderV1{request,retry:true}));
 }
 let guard=ctx.transaction.guard();
 let orders=bind_current_agent_orders_v1(sources.native().rules().orders(),sources.roots(),state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?;
 let guard=ctx.transaction.guard();
 let directory=open_current_factory_order_directory_v1(accepted_orders,&orders,state,ctx,&mut work.scan,&mut work.read,&mut work.orders,&guard,live).map_err(infra)?;
 for(id,index)in directory.iter_order_indices(){tail.charge(index.target_agent_id().len().min(actor.len()))?;if index.target_agent_id()==actor{
 let guard=ctx.transaction.guard();
 let order=read_current_agent_order_v1(directory.source(),id,state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?.ok_or_else(||invalid("organization manufactured actor index"))?;
 tail.charge(order.value().quote.world_binding.as_ref().map_or(0,String::len).checked_add(binding.map_or(0,str::len)).ok_or_else(||invalid("organization scope layout"))?)?;
 if oasis7_world_state::world_rules::validate_manufactured_world_scope_from_order_v1(Some(order.value()),binding).is_err(){return Ok(None)}break;
 }}
 if !command_fits(command,tail,true)?{return Ok(None)}
 // All five fixed organization commands have the original system exception:
 // Close/Finalize are runtime observations; the other three are explicitly
 // exempted from blocked_agents. None acquires a manufactured identity.
 Ok(Some(OrganizationSystemHeaderV1{request,retry:false}))
}
