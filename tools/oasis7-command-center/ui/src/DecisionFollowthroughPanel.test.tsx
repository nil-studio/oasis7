import { render, screen, fireEvent, waitFor, cleanup } from '@solidjs/testing-library';
import { describe,it,expect,vi,afterEach } from 'vitest';
import Panel from './DecisionFollowthroughPanel';
import type { DailyWorkState,SyncEvidence } from './generated/core';
import type { Snapshot } from './types';
afterEach(cleanup);
const decision={id:'d',content:'明确决定',reason:'理由',scope:'全部范围',relatedSection:null,state:'confirmed',confirmedAt:'1',formalUpdate:'notVerified',implementationSync:'notVerified'};
const snapshot={decisions:[decision],sections:[]} as unknown as Snapshot;
const evidence=(full:boolean,remaining:string[]=[]):SyncEvidence=>({state:'verified',source:'section',revision:'r',scope:'一个条款',conclusion:'核实',coversFullScope:full,remaining,checkedAt:'2',decisionDigest:'digest'});
function state(formal:SyncEvidence,implementation:SyncEvidence):DailyWorkState{return {decisions:[{decisionId:'d',formal,implementation,history:[],following:true}]} as unknown as DailyWorkState;}
describe('decision synchronization coverage',()=>{
 it('keeps partial verified evidence pending and never infers whole scope from one clause',()=>{render(()=><Panel snapshot={snapshot} personal={state(evidence(false),evidence(true))} busy={false} save={vi.fn()}/>);expect(screen.getByRole('button',{name:'明确决定'})).toBeTruthy();expect(screen.queryByText('适用同步义务已核实或明确不适用 · 历史')).toBeNull();});
 it('requires both tracks and no remaining obligation before leaving the pending list',async()=>{render(()=><Panel snapshot={snapshot} personal={state(evidence(true),evidence(true))} busy={false} save={vi.fn()}/>);expect(screen.queryByRole('button',{name:'明确决定'})).toBeNull();fireEvent.click(screen.getByRole('button',{name:'查看完整范围与历史'}));expect(screen.getByRole('button',{name:'明确决定'})).toBeTruthy();expect(screen.getByText('适用同步义务已核实或明确不适用 · 历史')).toBeTruthy();});
 it('preserves explicit remaining items even when full-scope checkbox is selected',()=>{render(()=><Panel snapshot={snapshot} personal={state(evidence(true,['仍需核对']),evidence(true))} busy={false} save={vi.fn()}/>);expect(screen.getByRole('button',{name:'明确决定'})).toBeTruthy();});
 it('sends specific scope and remaining coverage with no implicit full-scope approval',async()=>{const save=vi.fn().mockResolvedValue(undefined);render(()=><Panel snapshot={snapshot} personal={state(evidence(false),evidence(false))} busy={false} save={save}/>);fireEvent.click(screen.getByRole('button',{name:'明确决定'}));fireEvent.input(screen.getByLabelText('核实结论与剩余项'),{target:{value:'只比较了一个条款'}});fireEvent.input(screen.getByLabelText('尚未覆盖的同步项（每行一项）'),{target:{value:'另一个目标\n部署'}});fireEvent.click(screen.getByRole('button',{name:'保存本轨道核实'}));await waitFor(()=>expect(save).toHaveBeenCalledWith(expect.objectContaining({kind:'syncDecision',evidence:expect.objectContaining({coversFullScope:false,remaining:['另一个目标','部署']})})));});
});
