// Resource bounds only. Authorization and projection provenance remain upstream.
export const LEGACY_DISPLAY_PROFILE = Object.freeze({name:"legacy_snapshot_v1",markers:500,labels:100,bytes:256*1024,nodes:16384});
export const WORLD_VIEW_DISPLAY_PROFILE = Object.freeze({name:"world_view_v1",markers:1000,labels:200,bytes:2*1024*1024,nodes:131072});
export function pixelWorldDisplayProfile(input) {
  const view=input?.worldView;
  if (view == null) return LEGACY_DISPLAY_PROFILE;
  if (typeof view !== "object" || Array.isArray(view)) throw Error("invalid world view display profile");
  let items=0;
  for(const field of ["entities","aggregates","events","relations"]) {
    const values=view[field] ?? [];
    if(!Array.isArray(values)) throw Error("pixel world world-view capacity exceeded");
    items+=values.length;
  }
  if(items>1000) throw Error("pixel world world-view capacity exceeded");
  return WORLD_VIEW_DISPLAY_PROFILE;
}
