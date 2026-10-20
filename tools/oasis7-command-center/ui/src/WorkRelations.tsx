import { For, Show } from 'solid-js';
import { ArrowLeft, FileText, Link, Target } from 'lucide-solid';
import type { DocumentSection, WorkItem, WorkRelation } from './generated/core';
import { StatusIcon } from './VisualStatus';

export default function WorkRelations(props:{work:WorkItem;relations:WorkRelation[];sections:DocumentSection[];showSource?:(section:DocumentSection,origin?:HTMLElement)=>void;openSource:()=>void;onBasis:()=>void}) {
  const goals=()=>[
    ...props.work.goals.filter(goal=>!props.relations.some(r=>r.sectionId===goal.sectionId)).map(goal=>({id:goal.sectionId,title:goal.title,revision:goal.revision,origin:'来源投影 · 原个人确认'})),
    ...props.relations.filter(r=>!r.removedAt).map(r=>({id:r.sectionId,title:null,revision:r.sourceRevision,origin:'个人确认'})),
  ];
  return <section class="work-relations" aria-label="工作关系图"><div class="relationship-heading"><h3><Link size={18} aria-hidden="true"/>工作关系</h3><Show when={goals().length}><button onClick={props.onBasis}>管理依据关系</button></Show></div>
    <Show when={goals().length} fallback={<div class="relation-empty-inline"><Target size={18} aria-hidden="true"/><span>尚无明确目标关系</span><button onClick={props.onBasis}>选择依据并确认关联</button></div>}>
    <div class="relation-map">
      <div class="relation-node current-work"><StatusIcon value={props.work.result.value}/><span class="eyebrow">当前工作</span><strong>{props.work.reference.kind==='pullRequest'?`PR #${props.work.reference.number}`:'原工具工作'}</strong><Show when={props.work.reference.kind==='pullRequest'}><button disabled={!props.work.sourceUrl} onClick={props.openSource}>查看原 PR #{props.work.reference.kind==='pullRequest'?props.work.reference.number:''}</button></Show></div>
      <div class="relation-branches"><For each={goals()}>{goal=>{const section=()=>props.sections.find(s=>s.id===goal.id);return <div class="relation-branch"><span class="relation-line"><ArrowLeft size={18} aria-hidden="true"/><span>关联</span></span><div class="relation-node"><span class="eyebrow"><FileText size={16} aria-hidden="true"/>{goal.origin}</span><Show when={section()} fallback={<strong>{goal.title??goal.id}</strong>}>{source=><button disabled={!props.showSource} onClick={e=>props.showSource?.(source(),e.currentTarget)}>{source().title}</button>}</Show><p class="small muted">{!section()?'原文未读取 · 保留原关系':!goal.revision?'确认版本未知 · 打开当前原文':section()!.revision!==goal.revision?'依据版本已变化 · 打开当前原文':'确认版本匹配'}</p></div></div>;}}</For></div>
    </div><p class="small muted">连线仅表示明确关联；PR 合入不代表目标验收或部署完成。</p></Show>
  </section>;
}
