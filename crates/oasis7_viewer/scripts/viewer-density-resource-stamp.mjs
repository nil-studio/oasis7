// Fixed-load attribution only; these guards are not frame/GPU-completion fences.
export function validateDensityResourceStamp(row,{url,world,cut}) {
 if(row.url!==url||row.world!==world||row.cut!==cut||row.profile!=='world_view_v1')throw Error('Viewer resource scope/source mismatch');
 if(row.identityValid!==true)throw Error('observer acquired identity');
 if(!Number.isFinite(row.origin)||row.origin<=0||row.visibility!=='visible'||row.status!=='observed'||row.markers!==1000||row.bodies!==1000||row.inViewBodies!==1000||row.labels!==200)throw Error('Viewer resource load is not actual observed1000/200');
 const s=row.client;
 if(!s||s.scopeBound!==true||s.loading!==false||s.stale!==false||s.hasError!==false||s.published?.entities!==1000||s.published.completeness!=='complete'||s.published.sourceCutMatches!==true||s.published.queryGenerationMatches!==true)throw Error('Viewer resource client source is not current complete1000');
 for(const value of [s.scopeSequence,s.instanceSequence,s.localGeneration,row.renderVersion])if(!Number.isSafeInteger(value)||value<=0)throw Error('Viewer resource generation missing');
 if(!row.camera||!Number.isFinite(row.camera.zoom)||row.camera.zoom<=0||!Number.isFinite(row.camera.pan_x_px)||!Number.isFinite(row.camera.pan_y_px))throw Error('Viewer resource camera missing');
 return row;
}
export function matchDensityResourceStamps(before,after,expected) {
 validateDensityResourceStamp(before,expected);validateDensityResourceStamp(after,expected);
 for(const field of ['url','world','cut','origin','renderVersion'])if(before[field]!==after[field])throw Error('Viewer resource observation crossed '+field);
 for(const field of ['scopeSequence','instanceSequence','localGeneration'])if(before.client[field]!==after.client[field])throw Error('Viewer resource observation crossed client '+field);
 if(JSON.stringify(before.camera)!==JSON.stringify(after.camera))throw Error('Viewer resource camera changed during observation');
 const a=before.wasm,b=after.wasm;
 if(a?.status==='observed'||b?.status==='observed'){
  if(a?.status!=='observed'||b?.status!=='observed'||!Number.isSafeInteger(a.instanceSequence)||a.instanceSequence<=0||a.instanceSequence!==b.instanceSequence)throw Error('Viewer resource WASM mount changed');
  for(const row of [a,b])if(!Number.isSafeInteger(row.linearCapacityBytes)||row.linearCapacityBytes<=0||row.linearCapacityBytes%65536!==0)throw Error('Viewer resource WASM capacity invalid');
  return {matched:true,wasmLinearCapacityBeforeBytes:a.linearCapacityBytes,wasmLinearCapacityAfterBytes:b.linearCapacityBytes,wasmLease:a.instanceSequence};
 }
 return {matched:true,wasmLinearCapacityBeforeBytes:null,wasmLinearCapacityAfterBytes:null,wasmLease:null};
}
export const densityResourceStampScript = expectedCut => `async () => {
 if(document.querySelector('.pixel-world-canvas')?.dataset.rendererReady!=='true')throw Error('mounted Viewer is not ready');
 const api=await import('/pixel-world-bridge/pixel_world_bridge.js');
 const m=api.readPixelWorldDisplayMetrics(),wasm=api.readPixelWorldLinearMemoryCapacity?.()??null,s=window.__AW_TEST__?.getState();
 if(!s)throw Error('read-only test state unavailable');
 return {url:location.href,origin:performance.timeOrigin,visibility:document.visibilityState,world:s.worldId,camera:s.pixelWorldCamera,identityValid:s.authReady===false&&s.authPlayerId===null&&s.authPublicKey===null&&s.authBoundAgentId===null&&s.authRegistrationStatus==='guest',client:window.__OASIS7_WORLD_VIEW_RETENTION__?.(${JSON.stringify(expectedCut)},false)??null,status:m.status,profile:m.display_profile,cut:m.source_cut,renderVersion:m.render_version,markers:m.projected_marker_count,bodies:m.agent_body_sprites,inViewBodies:m.in_view_agent_body_sprites,labels:m.laid_out_in_view_labels,wasm};
}`;
