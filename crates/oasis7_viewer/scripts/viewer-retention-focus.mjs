// Runs inside the owned page. Preparation waits never send or retry an input.
export async function focusRetentionTarget(selector,expectedCut) {
  const deadline=performance.now()+10000;
  let last;
  while(performance.now()<deadline) {
    const state=window.__AW_TEST__.getState(),stats=window.__OASIS7_WORLD_VIEW_RETENTION__?.(expectedCut,false),metrics=window.__QA_READ_METRICS__?.(),button=document.querySelector(selector);
    if(state.authReady!==false||state.authPlayerId!==null||state.authPublicKey!==null||state.authBoundAgentId!==null||state.authRegistrationStatus!=='guest')throw Error('observer identity changed before native input');
    const current=stats&&!stats.loading&&!stats.stale&&!stats.hasError&&stats.published.queryGenerationMatches===true&&stats.published.sourceCutMatches===true&&stats.published.completeness==='complete';
    if(current&&stats.published.entities!==1000)throw Error('complete source cardinality changed before native input');
    last={attached:Boolean(button?.isConnected),current:Boolean(current),generation:stats?.localGeneration??null,bodies:metrics?.in_view_agent_body_sprites??null,status:metrics?.status??null};
    if(current&&button?.isConnected&&metrics?.status==='observed'&&metrics.display_profile==='world_view_v1'&&metrics.source_cut===expectedCut&&metrics.projected_marker_count===1000&&metrics.agent_body_sprites===1000&&metrics.in_view_agent_body_sprites===1000&&metrics.laid_out_in_view_labels===200) {
      const rect=button.getBoundingClientRect();
      if(rect.width>0&&rect.height>0&&rect.right>0&&rect.bottom>0&&rect.left<innerWidth&&rect.top<innerHeight) {
        button.focus({preventScroll:true});
        if(document.activeElement!==button)throw Error('same native target focus failed');
        return {focused:true,generation:stats.localGeneration,renderVersion:metrics.render_version,scope:stats.scopeSequence,instance:stats.instanceSequence,time:performance.now(),origin:performance.timeOrigin};
      }
    }
    await new Promise(resolve=>setTimeout(resolve,25));
  }
  throw Error('same native target did not become ready before input: '+JSON.stringify(last));
}
export const retentionFocusScript=(selector,cut)=>`(${focusRetentionTarget.toString()})(${JSON.stringify(selector)},${JSON.stringify(cut)})`;
