import { ChevronRight, Flag, Target } from 'lucide-solid';
import { createMemo, For, Show, type JSX } from 'solid-js';
import MarkdownText from './MarkdownText';
import type { DocumentSection } from './types';

// Project only explicit source text. Unsupported/malformed tables stay in the original.
export function goalSummary(content:string) {
  let fenced=false;
  const lines=content.split('\n').filter(line=>{
    if (/^\s*(```|~~~)/.test(line)){fenced=!fenced;return false;}
    return !fenced;
  });
  const paragraph=lines.join('\n').split(/\n\s*\n/).find(block=>block.trim()&&!/^[#<|\s]*[#<|]|^\s*(?:[-+*]|\d+[.)])\s/.test(block)&&!block.includes('\n|'))?.trim()??'';
  const statement=paragraph.includes('当前目标：')?paragraph.slice(paragraph.indexOf('当前目标：')+5):paragraph;
  const milestones:{name:string;owner:string;dependency:string;exit:string}[]=[];
  for(const block of lines.join('\n').split(/\n\s*\n/)) {
    const table=block.split('\n').map(line=>line.trim());
    if(table.length<3||!table.every(line=>line.startsWith('|')&&line.endsWith('|')))continue;
    const cells=(line:string)=>line.slice(1,-1).split(/(?<!\\)\|/).map(cell=>cell.trim().replace(/\\\|/g,'|'));
    const headers=cells(table[0]);
    if(headers.join('|')!=='里程碑 / 工作包|主责|依赖与产出|退出条件'||!/^\|[\s:|-]+\|$/.test(table[1]))continue;
    const rows=table.slice(2).map(cells);
    if(rows.some(row=>row.length!==4||row.some(cell=>!cell)))continue;
    milestones.push(...rows.map(([name,owner,dependency,exit])=>({name,owner,dependency,exit})));
  }
  return {statement,milestones};
}

export default function GoalSummary(props:{children?:JSX.Element;goal:DocumentSection;sections?:DocumentSection[];showSource?:(section:DocumentSection,origin?:HTMLElement)=>void}) {
  const summary=createMemo(()=>goalSummary(props.goal.content));
  return <section class="goal-summary" aria-label="具体交付目标">
    <div class="goal-statement"><Target size={22} aria-hidden="true"/><div><span class="eyebrow">当前目标</span><Show when={summary().statement} fallback={<p>具体目标正文尚未提取，请打开主责原文。</p>}><MarkdownText text={summary().statement} sourcePath={props.goal.path} sections={props.sections} showSource={props.showSource}/></Show><button class="text-link" onClick={event=>props.showSource?.(props.goal,event.currentTarget)}>查看目标来源</button></div></div>
    {props.children}
    <Show when={summary().milestones.length}><p class="small muted">主责计划 · 验收尚未核实；卡片顺序不表示依赖</p><div class="milestone-cards"><For each={summary().milestones}>{milestone=><details class="milestone-card"><summary><Flag size={18} aria-hidden="true"/><span>{milestone.name}</span><ChevronRight class="milestone-chevron" size={16} aria-hidden="true"/></summary><dl><dt>主责</dt><dd>{milestone.owner}</dd><dt>依赖与产出</dt><dd>{milestone.dependency}</dd><dt>退出条件</dt><dd>{milestone.exit}</dd></dl></details>}</For></div></Show>
  </section>;
}
