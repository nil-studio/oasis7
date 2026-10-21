import {cleanup,fireEvent,render,screen} from '@solidjs/testing-library';
import {afterEach,expect,it,vi} from 'vitest';
import GoalSummary,{goalSummary} from './GoalSummary';
import type {DocumentSection} from './types';
afterEach(cleanup);
const table='| 里程碑 / 工作包 | 主责 | 依赖与产出 | 退出条件 |\n| --- | --- | --- | --- |\n| 持久单权威 | runtime | 顶层合同后恢复 | 已确认效果保留 |';
it('uses explicit source goal and milestone fields without claiming completion or inventing dependencies',()=>{
 const source={id:'goal',path:'doc/core/prd.md',title:'当前阶段交付目标与全局 P0',content:'# 当前阶段\n\n<a id="goal"></a>\n\nPRD-CORE-004 当前目标：保留成果并回访继续。\n\n'+table} as DocumentSection;
 const showSource=vi.fn();render(()=><GoalSummary goal={source} showSource={showSource}/>);
 expect(screen.getByText('保留成果并回访继续。')).toBeVisible();expect(screen.getByText(/主责计划 · 验收尚未核实/)).toBeVisible();
 expect(screen.getByText('已确认效果保留')).not.toBeVisible();fireEvent.click(screen.getByText('持久单权威'));expect(screen.getByText('已确认效果保留')).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'查看目标来源'}));expect(showSource).toHaveBeenCalledWith(source,expect.any(HTMLElement));
});
it('ignores fenced plans and malformed tables while preserving a readable source fallback',()=>{
 expect(goalSummary('```md\n假目标\n\n'+table+'\n```\n\n实际正文')).toEqual({statement:'实际正文',milestones:[]});
 expect(goalSummary('# 标题\n\n'+table.replace('已确认效果保留','')).milestones).toEqual([]);
 expect(goalSummary('# 标题\n\n'+table.replace('主责','完成度')).milestones).toEqual([]);
 expect(goalSummary('# 标题\n\n<a id="x"></a>').statement).toBe('');
});
