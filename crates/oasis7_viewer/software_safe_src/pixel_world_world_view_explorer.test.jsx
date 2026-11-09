import { render, screen, fireEvent, cleanup } from '@solidjs/testing-library';
import { afterEach, describe, it, expect, vi } from 'vitest';
import { createSignal } from 'solid-js';
import { PixelWorldWorldViewExplorer } from './pixel_world_world_view_explorer.jsx';
afterEach(cleanup);
const entity=(id,freshness='current')=>({entity_id:id,kind:'agent',label:id,position:{x_cm:'10',y_cm:'20',z_cm:'0'},provenance:{geometry_source:'authoritative',freshness,valid_at_source_cut:'cut'}});
describe('world exploration',()=>{
  it('offers separate overlapping candidates and does not enable unknown geometry',()=>{
    const onSelect=vi.fn();render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[entity('A'),entity('B'),entity('Unknown','unknown')],completeness:'partial'}}} onSelect={onSelect}/>);
    fireEvent.click(screen.getByRole('button',{name:'B',exact:true}));expect(onSelect).toHaveBeenCalledWith({kind:'agent',id:'B'});
    expect(screen.getAllByRole('button',{name:'Follow'})[2].disabled).toBe(true);expect(screen.getByRole('status').textContent).toContain('partial');
  });
  it('pauses follow when source expires without automatically resuming',async()=>{
    const onFollow=vi.fn();const [state,setState]=createSignal({view:{source_cut:'cut',entities:[entity('A')]}});
    render(()=> <PixelWorldWorldViewExplorer locale="en" state={state()} onFollow={onFollow}/>);
    fireEvent.click(screen.getByRole('button',{name:'Follow'}));expect(onFollow).toHaveBeenLastCalledWith({kind:'agent',id:'A'});
    setState({view:{source_cut:'cut',entities:[entity('A','expired')]}});await Promise.resolve();expect(onFollow).toHaveBeenLastCalledWith(null);
    setState({view:{source_cut:'cut',entities:[entity('A')]}});await Promise.resolve();expect(screen.getByRole('button',{name:'Follow'}).getAttribute('aria-pressed')).toBe('false');
  });
});

it('searches the server scope and exposes explicit return actions',()=>{
 const onQuery=vi.fn(),onOverview=vi.fn(),onReturnTarget=vi.fn();
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[],completeness:'unknown'}}} onQuery={onQuery} onOverview={onOverview} onReturnTarget={onReturnTarget}/>);
 fireEvent.input(screen.getByRole('textbox'),{target:{value:' mine '}});fireEvent.submit(screen.getByRole('textbox').closest('form'));
 expect(onQuery).toHaveBeenLastCalledWith({search:'mine'});
 fireEvent.click(screen.getByRole('button',{name:'Overview query results'}));expect(onOverview).toHaveBeenCalled();expect(onQuery).toHaveBeenCalledTimes(1);expect(onQuery).toHaveBeenLastCalledWith({search:'mine'});
 fireEvent.click(screen.getByRole('button',{name:'Return to target'}));expect(onReturnTarget).toHaveBeenCalled();
});
it('queries a bounded region using the fixed world basis',()=>{
 const onQuery=vi.fn();render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',world_bounds:{width_cm:'800',depth_cm:'400'},entities:[entity('A')]}}} onQuery={onQuery}/>);
 fireEvent.click(screen.getByRole('button',{name:'Explore region'}));
 expect(onQuery).toHaveBeenCalledWith({bounds:{min_x_cm:'0',max_x_cm:'110',min_y_cm:'0',max_y_cm:'70'}});
});
it('clears the follow indicator on manual camera stop',async()=>{
 const [stop,setStop]=createSignal(0);render(()=> <PixelWorldWorldViewExplorer locale="en" pauseVersion={stop()} state={{view:{source_cut:'cut',entities:[entity('A')]}}}/>);
 fireEvent.click(screen.getByRole('button',{name:'Follow'}));expect(screen.getByRole('button',{name:'Follow'}).getAttribute('aria-pressed')).toBe('true');
 setStop(1);await Promise.resolve();expect(screen.getByRole('button',{name:'Follow'}).getAttribute('aria-pressed')).toBe('false');
});

it('stops following before returning to the main target',()=>{
 const order=[];render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[entity('B')]}}} onFollow={value=>order.push(value?'follow':'stop')} onReturnTarget={()=>order.push('return')}/>);
 fireEvent.click(screen.getByRole('button',{name:'Follow'}));
 fireEvent.click(screen.getByRole('button',{name:'Return to target'}));
 expect(order).toEqual(['follow','stop','return']);
 expect(screen.getByRole('button',{name:'Follow'}).getAttribute('aria-pressed')).toBe('false');
});
it('inspects an authorized facility with derived position without offering Agent follow or controls',()=>{
 const facility={...entity('F'),kind:'factory',label:'Furnace',provenance:{...entity('F').provenance,geometry_source:'location_derived'}};
 const onSelect=vi.fn(),onLocate=vi.fn();render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[facility]}}} onSelect={onSelect} onLocate={onLocate}/>);
 fireEvent.click(screen.getByRole('button',{name:'Furnace'}));expect(onSelect).toHaveBeenCalledWith({kind:'factory',id:'F'});
 expect(screen.getByRole('region',{name:'Object inspection'})).toHaveTextContent('Read-only observation');
 expect(screen.getByRole('button',{name:'Follow'}).disabled).toBe(true);fireEvent.click(screen.getByRole('button',{name:'Locate'}));
 expect(onLocate).toHaveBeenCalledWith({kind:'factory',id:'F'},{x_cm:10,y_cm:20,z_cm:0});expect(screen.queryByRole('button',{name:/Submit|Build|Chat/})).not.toBeInTheDocument();
});
it('offers read-only route endpoint inspection only when both endpoints are in the same authorized view',()=>{
 const from=entity('A'),to={...entity('F'),kind:'factory',label:'Furnace'};
 const route={relation_id:'r',kind:'logistics_route',label:'Ore delivery',source_class:'runtime_projection',status:'unavailable',freshness:'unknown',valid_at_source_cut:'cut',from:{kind:'agent',id:'A'},to:{kind:'factory',id:'F'}};
 const onSelect=vi.fn();const [state,setState]=createSignal({view:{source_cut:'cut',entities:[from,to],relations:[route]}});
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={state()} onSelect={onSelect}/>);
 fireEvent.click(screen.getByRole('button',{name:'Ore delivery'}));expect(screen.getByRole('region',{name:'Route inspection'})).toHaveTextContent('read-only');
 fireEvent.click(screen.getByRole('button',{name:'Inspect Furnace'}));expect(onSelect).toHaveBeenCalledWith({kind:'factory',id:'F'});
 setState({view:{source_cut:'cut',entities:[from],relations:[route]}});expect(screen.queryByRole('region',{name:'Authorized routes'})).not.toBeInTheDocument();
});

it('explores aggregate regions and locates a trusted historical event without offering follow',()=>{
 const aggregate={aggregate_id:'area',label:'Industry zone',member_count:'20',provenance:{freshness:'current'},drilldown:{bounds:{min_x_cm:'0',max_x_cm:'20',min_y_cm:'0',max_y_cm:'20'},detail_level:'entity'}};
 const event={event_id:'W:0:1',identity:{world_id:'W',reorg_epoch:'0',event_seq:'1'},label:'Factory opened',freshness:'last_known',source_class:'runtime_journal_payload',source_cut:'cut',position:{x_cm:'1',y_cm:'2',z_cm:'0'}};
 const onDrilldown=vi.fn(),onLocate=vi.fn(),onEventFeed=vi.fn();
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{world_id:'W',reorg_epoch:'0',source_cut:'cut',entities:[],aggregates:[aggregate],events:[event]}}} onDrilldown={onDrilldown} onLocate={onLocate} onEventFeed={onEventFeed}/>);
 fireEvent.click(screen.getByRole('button',{name:'Explore area'}));expect(onDrilldown).toHaveBeenCalledWith(aggregate);
 fireEvent.click(screen.getByRole('button',{name:'Locate event'}));expect(onLocate).toHaveBeenCalledWith({kind:'world_event',id:'W:0:1'});
 fireEvent.click(screen.getByRole('button',{name:'View world feed'}));expect(onEventFeed).toHaveBeenCalledWith(event);
 expect(screen.queryByRole('button',{name:'Follow'})).toBeNull();
});
it('keeps unknown event anchors read-only and explains an absent exact feed event',()=>{
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={{notice:'event_feed_unavailable',view:{source_cut:'cut',entities:[],events:[{event_id:'unknown',label:'Unknown event',freshness:'unknown'}]}}}/>);
 expect(screen.getByRole('button',{name:'Locate event'}).disabled).toBe(true);expect(screen.getByText('This event is absent from the current world feed.')).toBeTruthy();
});

it('exposes an industrial aggregate route as a summary with read-only aggregate inspection',()=>{
 const onSelect=vi.fn();const aggregates=[{aggregate_id:'north',label:'North industry',member_count:'3',provenance:{freshness:'current'}},{aggregate_id:'south',label:'South industry',member_count:'2',provenance:{freshness:'current'}}];
 const relation={relation_id:'summary-route',kind:'industry_route',source_class:'display_aggregation',route_count:'12',label:'Industrial corridor',status:'active',freshness:'current',valid_at_source_cut:'cut',from:{kind:'aggregate',id:'north'},to:{kind:'aggregate',id:'south'}};
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[],aggregates,relations:[relation]}}} onSelect={onSelect}/>);
 expect(screen.getByText(/12 routes summarized/)).toBeInTheDocument();fireEvent.click(screen.getByRole('button',{name:'Industrial corridor'}));
 fireEvent.click(screen.getByRole('button',{name:'Inspect North industry'}));expect(onSelect).toHaveBeenCalledWith({kind:'industry_aggregate',id:'north'});
 expect(screen.getByRole('button',{name:'Locate North industry'}).disabled).toBe(true);expect(screen.getAllByRole('button',{name:'Explore area'})).toHaveLength(2);
});
it('does not expose a runtime route with forged aggregate endpoints',()=>{
 const relation={relation_id:'forged',kind:'logistics_route',source_class:'runtime_projection',label:'Forged runtime route',valid_at_source_cut:'cut',from:{kind:'aggregate',id:'north'},to:{kind:'aggregate',id:'north'}};
 render(()=> <PixelWorldWorldViewExplorer locale="en" state={{view:{source_cut:'cut',entities:[],aggregates:[{aggregate_id:'north',label:'North',member_count:'3'}],relations:[relation]}}}/>);
 expect(screen.queryByRole('button',{name:'Forged runtime route'})).toBeNull();
});
