import { PixelWorldEvolutionExplorer } from "./pixel_world_world_view_evolution.jsx";
import { PixelWorldEntityInspector, PixelWorldRouteExplorer } from "./pixel_world_world_view_inspector.jsx";
import { createEffect, createSignal, For, Show } from 'solid-js';
import { worldViewPosition } from './pixel_world_world_view.js';

export function PixelWorldWorldViewExplorer(props) {
  const zh = () => String(props.locale || '').startsWith('zh');
  const text = (cn,en) => zh() ? cn : en;
  const [search,setSearch] = createSignal('');
  const [following,setFollowing] = createSignal(null);
  const [inspected,setInspected] = createSignal(null);
  const view = () => props.state?.view;
  const entities = () => view()?.entities || [];
  const kindLabel = kind => ({agent:text('智能体','Agent'),location:text('地点','Location'),factory:text('设施','Facility')})[kind] || text('对象','Object');
  const geometryLabel = source => ({authoritative:text('权威位置','Authoritative position'),location_derived:text('地点派生','Derived from location'),relationship_layout:text('关系布局','Relationship layout')})[source] || text('位置未知','Position unknown');
  const freshnessLabel = freshness => ({current:text('当前','Current'),last_known:text('最近已知','Last known'),conflict:text('冲突','Conflict'),expired:text('已过期','Expired')})[props.state?.stale && freshness === 'current' ? 'last_known' : freshness] || text('时效未知','Freshness unknown');
  const position = entity => worldViewPosition(entity,view(),props.state?.stale, true);
  const followPosition = entity => entity?.kind === 'agent' ? worldViewPosition(entity,view(),props.state?.stale) : null;
  createEffect(() => {
    const id = following();
    if (!id) return;
    const entity = entities().find(e => `${e.kind}:${e.entity_id}` === id);
    if (!followPosition(entity)) { setFollowing(null); props.onFollow?.(null); }
  });
  let pauseInitialized = false;
  createEffect(() => { props.pauseVersion; if (pauseInitialized) setFollowing(null); pauseInitialized = true; });
  function regionBounds(entity) {
    const point = position(entity), bounds = view()?.world_bounds;
    const width = Number(bounds?.width_cm), depth = Number(bounds?.depth_cm);
    if (!point || !Number.isSafeInteger(width) || !Number.isSafeInteger(depth) || width <= 0 || depth <= 0) return null;
    const halfWidth = Math.max(1, Math.floor(width / 8)), halfDepth = Math.max(1, Math.floor(depth / 8));
    return {min_x_cm:String(Math.max(0,point.x_cm-halfWidth)),max_x_cm:String(Math.min(width,point.x_cm+halfWidth)),min_y_cm:String(Math.max(0,point.y_cm-halfDepth)),max_y_cm:String(Math.min(depth,point.y_cm+halfDepth))};
  }
  function select(entity) { const ref={kind:entity.kind,id:entity.entity_id}; setInspected(ref); props.onSelect?.(ref); }
  return <section class="pixel-world-exploration" aria-label={text('授权世界探索','Authorized world exploration')}>
    <form onSubmit={event => {event.preventDefault();setFollowing(null);props.onFollow?.(null);props.onQuery?.({search:search().trim()});}}>
      <label>{text('授权空间搜索','Search authorized space')} <input value={search()} maxlength="128" onInput={event=>setSearch(event.currentTarget.value)} /></label>
      <button type="submit" disabled={props.state?.loading}>{text('搜索','Search')}</button>
      <button type="button" data-query-overview onClick={()=>{setFollowing(null);props.onOverview?.();}}>{text('总览当前查询结果','Overview query results')}</button>
      <button type="button" onClick={()=>{setSearch('');setFollowing(null);props.onFollow?.(null);props.onReturnTarget?.();}}>{text('返回主目标','Return to target')}</button>
    </form>
    <p role="status">{props.state?.loading ? text('正在读取授权范围…','Reading authorized scope…') : props.state?.stale ? text('最近已知；当前范围读取失败','Last known; scope refresh failed') : view()?.completeness === 'complete' ? text('当前查询范围完整','Current query scope complete') : view()?.completeness === 'partial' ? text('当前查询范围不完整','Current query scope partial') : text('查询范围完整性未知','Query scope completeness unknown')}
      <Show when={view()?.budget_limited}> · {text('当前范围对象过密，请缩小视口或搜索。','This area is dense. Narrow the viewport or search.')}</Show>
      <Show when={view()?.source_frozen}> · {text('同一来源切点的历史视图','Historical view at one source cut')}</Show>
      <Show when={props.state?.error}> · {text('读取不可用，请重试','Read unavailable; retry')}</Show>
    </p>
    <Show when={props.overviewStatus}><p role="status" data-query-overview-status={props.overviewStatus}>
      {props.overviewStatus==='pending'
        ? text('查询尚未完成；保留当前视角，等待完整结果后总览。','Query incomplete; keeping the current camera until complete results arrive.')
        : props.overviewStatus==='applying'
        ? text('查询结果完整，正在应用总览。','Query results complete; applying the overview.')
        : text('当前查询无法完整总览；已保留视角。请缩小范围或重新查询。','The current query cannot be overviewed completely; camera kept. Narrow the scope or query again.')}
    </p></Show>
    <Show when={props.state?.notice === 'target_unavailable'}><p>{text('主目标当前没有获准且可信的位置；可检查目标上下文。','The target has no authorized trusted position; inspect its context instead.')}</p></Show>
    <Show when={props.state?.changes?.length}><p class="pixel-world-update-notice" role="status">
      {props.state.changes.filter(value=>value.world_deleted).length} {text('个对象已确认从世界移除','objects confirmed removed from the world')} · {props.state.changes.filter(value=>!value.world_deleted).length} {text('个记录离开当前查询；不代表世界删除','records left this query; world deletion is not implied')}
    </p></Show>
    <Show when={props.state?.notice === 'event_feed_unavailable'}><p>{text('当前世界动态中没有此事件；未跳转到其他事件。','This event is absent from the current world feed.')}</p></Show>
    <ul aria-label={text('获准对象与重叠候选','Authorized objects and overlapping candidates')}>
      <For each={entities()}>{entity => <li>
        <button type="button" onClick={()=>select(entity)}>{entity.label || entity.entity_id}</button>
        <span> · {kindLabel(entity.kind)} · {freshnessLabel(entity.provenance?.freshness)}{entity.provenance?.geometry_source === 'location_derived' ? text(' · 地点派生位置',' · Location derived position') : ''}</span>
        <button type="button" disabled={!position(entity)} onClick={()=>{select(entity);setFollowing(null);props.onFollow?.(null);props.onLocate?.({kind:entity.kind,id:entity.entity_id},position(entity));}}>{text('定位','Locate')}</button>
        <button type="button" disabled={!regionBounds(entity)} onClick={()=>{select(entity);setFollowing(null);props.onFollow?.(null);props.onLocate?.({kind:entity.kind,id:entity.entity_id},position(entity));props.onQuery?.({bounds:regionBounds(entity)});}}>{text('查看周边区域','Explore region')}</button>
        <button type="button" disabled={!followPosition(entity)} aria-pressed={following()===`${entity.kind}:${entity.entity_id}`} onClick={()=>{select(entity);setFollowing(`${entity.kind}:${entity.entity_id}`);props.onFollow?.({kind:entity.kind,id:entity.entity_id});}}>{text('跟随','Follow')}</button>
        <Show when={!position(entity)}><span> {text('无当前可信位置','No current trusted position')}</span></Show>
      </li>}</For>
    </ul>
    <PixelWorldEvolutionExplorer selection={props.selection} view={view()} stale={props.state?.stale} text={text} freshnessLabel={freshnessLabel} onSelect={props.onSelect} onLocate={props.onLocate} onDrilldown={props.onDrilldown} onEventFeed={props.onEventFeed}/>
    <PixelWorldEntityInspector view={view()} selection={props.selection || inspected()} stale={props.state?.stale}
      text={text} geometryLabel={geometryLabel} freshnessLabel={freshnessLabel}/>
    <PixelWorldRouteExplorer view={view()} stale={props.state?.stale} text={text} freshnessLabel={freshnessLabel}
      onSelect={ref=>{setInspected(ref);props.onSelect?.(ref);}} onLocate={props.onLocate}/>
  </section>;
}
