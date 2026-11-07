import {describe,it,expect} from "vitest";
import {projectPixelWorldRenderInput} from "./pixel_world_render_input.js";
import {pixelWorldDisplayProfile} from "./pixel_world_display_profile.js";

const view = count => ({schema_version:1,world_id:"W",scope_id:"S",source_cut:"C",entities:Array.from({length:count},(_,i)=>({kind:"agent",entity_id:`A${i}`,render_record:{name:`Actor${i}`,pos:{x_cm:i,y_cm:0,z_cm:0}},provenance:{geometry_source:"authoritative",freshness:"current",valid_at_source_cut:"C"}})),aggregates:[],events:[],relations:[]});
describe("finite display profiles",()=>{
 it("retains 1000 authorized records, provenance and the matching profile",()=>{
  const input={worldView:view(1000),selectedKind:"agent",selectedId:"A999"};
  const dto=projectPixelWorldRenderInput(input);
  expect(dto.worldView.entities).toHaveLength(1000);
  expect(dto.display_profile).toBe("world_view_v1");
  expect(dto.worldView.entities[999].provenance.valid_at_source_cut).toBe("C");
  input.worldView.entities=input.worldView.entities.filter(e=>e.entity_id!=="A999");
  expect(projectPixelWorldRenderInput(input).worldView.entities.some(e=>e.entity_id==="A999")).toBe(false);
 });
 it("keeps legacy capacity and does not accept a caller-invented expansion",()=>{
  const dto=projectPixelWorldRenderInput({display_profile:"world_view_v1",lists:{agents:Array.from({length:1000},(_,i)=>({id:`A${i}`}))}});
  expect(dto.display_profile).toBe("legacy_snapshot_v1");
  expect(dto.lists.agents).toHaveLength(500);
  expect(()=>projectPixelWorldRenderInput({},1000)).toThrow(/capacity/);
  expect(()=>projectPixelWorldRenderInput({worldView:view(0)},1001)).toThrow(/capacity/);
 });
 it("bounds the entire query projection including relations and rejects malformed views",()=>{
  expect(()=>projectPixelWorldRenderInput({worldView:view(1001)})).toThrow(/capacity/);
  const input={worldView:view(1000)};
  input.worldView.relations=[{relation_id:"R"}];
  expect(()=>projectPixelWorldRenderInput(input)).toThrow(/capacity/);
  expect(()=>pixelWorldDisplayProfile({worldView:[]})).toThrow(/profile/);
  expect(()=>pixelWorldDisplayProfile({worldView:{events:{}}})).toThrow(/capacity/);
 });
 it("keeps rich UTF-8 records above the legacy byte limit but below the world-view bound",()=>{
  const input={worldView:view(1000)};
  for(const entity of input.worldView.entities)entity.render_record.name="界".repeat(128);
  const dto=projectPixelWorldRenderInput(input);
  const bytes=new TextEncoder().encode(JSON.stringify(dto)).length;
  expect(dto.worldView.entities).toHaveLength(1000);
  expect(bytes).toBeGreaterThan(256*1024);
  expect(bytes).toBeLessThanOrEqual(2*1024*1024);
 });
 it("retains bounded relations without reapplying the legacy 500 array ceiling",()=>{
  const input={worldView:view(2)};
  input.worldView.relations=Array.from({length:998},(_,i)=>({relation_id:`R${i}`,kind:"test_relation",from:{kind:"agent",id:"A0"},to:{kind:"agent",id:"A1"}}));
  const dto=projectPixelWorldRenderInput(input);
  expect(dto.worldView.entities).toHaveLength(2);
  expect(dto.worldView.relations).toHaveLength(998);
 });
 it("bounds JSON property names as well as string values",()=>{
  const input={worldView:view(1)};
  input.worldView.entities[0].render_record.resources={['x'.repeat(8193)]:1};
  expect(()=>projectPixelWorldRenderInput(input)).toThrow(/capacity/);
 });
});
