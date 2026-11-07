// Controlled transport fixture only: not signed WorldService or P5 evidence.
// Mount behind an owned loopback WS server; do not inject these records into snapshot.
import { randomUUID } from 'node:crypto';
const caps = ['world_view','world_view_pins_v1','world_view_delta_v1'];
const clone = value => structuredClone(value);
export function createDensityFixture() {
  let world = 'qa-density-world-A', scope = 'qa-public-A', cut = 'qa-cut-1';
  let sequence = 0, revision = 0, mode = 'world_view', labelStyle = 'normal', removed = new Set();
  const cursors = new Map(), baselines = new Map(), log = [];
  const remember=(map,key,value)=>{map.set(key,value);while(map.size>64)map.delete(map.keys().next().value);};
  const bounds = {width_cm:'1000000',depth_cm:'1000000',height_cm:'100000',min_x_cm:'0',max_x_cm:'1000000',min_y_cm:'0',max_y_cm:'1000000'};
  const sourceBinding = () => ({evidence:'controlled_transport_fixture_not_service_proof',version:{visibility_scope:scope,projection_revision:'qa-density-v1',commit:{world:{world_id:world,genesis_digest:'qa-genesis-not-a-service-assertion'},binding:{provider_world_id:world,branch_id:'qa-main',reorg_generation:'0',authority_generation:'0',permission_generation:'0',governing_manifest_ref:'qa-only'}}},viewer_epoch:'0'});
  const cells=[...Array.from({length:200},(_,i)=>({col:i%40,row:6+Math.floor(i/40)})),...Array.from({length:1000},(_,i)=>({col:i%40,row:Math.floor(i/40)})).filter(p=>p.row<6||p.row>10)];
  const entity = i => {const cell=cells[i],label=labelStyle==='empty'?'\u200b':(i<2?'观察'+i:'A');const id=`qa-agent-${String(i).padStart(4,'0')}`,pos={x_cm:String(256250+cell.col*12500),y_cm:String(275000+cell.row*18750),z_cm:'0'};return {kind:'agent',entity_id:id,label,position:pos,render_record:{id,name:label,pos,resources:{amounts:{electricity:'100'}}},provenance:{geometry_source:'authoritative',freshness:'current',valid_at_source_cut:cut,observed_at_tick:String(revision+1),source_ref:'controlled_transport_fixture'}};};
  const all = () => Array.from({length:1000},(_,i)=>entity(i)).filter(x=>!removed.has(x.entity_id));
  const base = request_generation => ({world_id:world,scope_id:scope,source_cut:cut,reorg_epoch:'0',observed_at_tick:String(revision+1),world_bounds:clone(bounds),source_binding:sourceBinding(),source_frozen:false,request_generation,relations:[],aggregates:[],events:[],unavailable_reasons:[]});
  const snapshot = () => ({version:3,chunk_generation_schema_version:1,time:revision+1,config:{space:{width_cm:1000000,depth_cm:1000000,height_cm:100000}},model:{agents:{},locations:{},agent_player_bindings:{},agent_player_public_key_bindings:{}},runtime_snapshot:null,player_gameplay:null,pending_actions:[],journal_len:0});
  function fail(q,reason) {return {type:'world_view',view:{...base(q.request_generation),entities:[],completeness:'unknown',unavailable_reasons:[reason]}};}
  function reply(message, connection = 'pure') {
    log.push(clone(message));
    if(message.type==='hello_v2')return {type:'hello_ack',server:'controlled-density-qa',version:2,min_version:1,max_version:2,world_id:world,control_profile:'live',capabilities:mode==='world_view'?caps.filter(c=>message.capabilities?.includes(c)):[]};
    if(message.type==='subscribe')return null;
    if(message.type==='request_world_feed')return {type:'world_feed',feed:{schema_version:1,world_id:world,reorg_epoch:'0',status:'unavailable',events:[],cursor:null,unavailable_reason:'source_unavailable',snapshot_reload_required:false}};
    if(message.type==='request_snapshot')return {type:'snapshot',snapshot:snapshot()};
    if(message.type==='request_scope_snapshot')return {type:'scope_snapshot',request_generation:message.request_generation,snapshot:snapshot()};
    if(message.type!=='request_world_view')throw Error('fixture rejects non-read request '+message.type);
    const q=message.query||{};
    if(mode!=='world_view')return fail(q,'scope_unavailable');
    if(!Number.isSafeInteger(q.page_size)||q.page_size<1||q.page_size>200||typeof q.request_generation!=='string')return fail(q,'invalid_query');
    // Baseline-only first. A later page must retain the same scope/cut and query.
    // Unknown resumes explicitly reset; no invented carry-forward authorization.
    const key=JSON.stringify({bounds:q.bounds??null,search:q.search??null,pinned_entities:q.pinned_entities??[]});
    if(q.resume_cursor){
      const old=baselines.get(q.resume_cursor);
      if(!old||old.connection!==connection||old.key!==key||old.world!==world||old.scope!==scope||old.cut!==cut)return {type:'world_view',view:{...base(q.request_generation),delivery:'reset'}};
      const next=String(BigInt(old.sequence)+1n),token='qa-resume-'+randomUUID();
      remember(baselines,token,{...old,sequence:next});
      return {type:'world_view',view:{...base(q.request_generation),delivery:'unchanged',carry_forward:true,base_cut:old.cut,query_id:key,previous_sequence:old.sequence,update_sequence:next,entities:[],completeness:'complete',resume_cursor:token}};
    }

    let records=all();
    if(q.search)records=records.filter(x=>x.entity_id.includes(q.search)||x.label.includes(q.search));
    if(q.bounds)records=records.filter(x=>Number(x.position.x_cm)>=Number(q.bounds.min_x_cm)&&Number(x.position.x_cm)<=Number(q.bounds.max_x_cm)&&Number(x.position.y_cm)>=Number(q.bounds.min_y_cm)&&Number(x.position.y_cm)<=Number(q.bounds.max_y_cm));
    const pins=new Set((q.pinned_entities||[]).filter(x=>x.kind==='agent').map(x=>x.id));
    const ordered=new Map([...all().filter(x=>pins.has(x.entity_id)),...records].map(x=>[x.entity_id,x]));records=[...ordered.values()];
    let offset=0;
    if(q.cursor){const c=cursors.get(q.cursor);if(!c||c.connection!==connection||c.key!==key||c.world!==world||c.scope!==scope||c.cut!==cut)return fail(q,'cursor_binding_mismatch');offset=c.offset;}
    const next=offset+q.page_size<records.length?randomUUID():null;
    if(next)remember(cursors,next,{key,world,scope,cut,connection,offset:offset+q.page_size});
    const view={...base(q.request_generation),entities:records.slice(offset,offset+q.page_size),next_cursor:next,completeness:next?'partial':'complete',delivery:'baseline',query_id:key,update_sequence:String(sequence),resume_cursor:next?undefined:'qa-resume-'+randomUUID()};
    if(!next)remember(baselines,view.resume_cursor,{cut,world,scope,key,connection,sequence:String(sequence)});
    return {type:'world_view',view};
  }
  return {reply,log,snapshot,description:()=>({world,scope,cut,mode,entityCount:1000-removed.size,labelStyle,snapshotAgents:0,controlledOnly:true}),
    setLabelStyle(style){if(!['normal','empty'].includes(style))throw Error('invalid label style');labelStyle=style;cut='qa-label-'+(++revision);},
    remove(id){removed.add(id);cut='qa-cut-'+(++revision+1);sequence++;},
    restore(){removed.clear();cut='qa-cut-'+(++revision+1);sequence++;},
    revoke(){scope='qa-denied-'+(++revision);mode='denied';cursors.clear();baselines.clear();},
    setLegacy(){mode='legacy';cursors.clear();baselines.clear();},
    newWorld(){world='qa-density-world-B';scope='qa-public-B';cut='qa-cut-B1';revision++;sequence=0;mode='world_view';removed.clear();cursors.clear();baselines.clear();},
    restoreWorldView(){mode='world_view';scope='qa-public-restored-'+(++revision);cut='qa-restored-'+revision;cursors.clear();baselines.clear();},
  };
}
