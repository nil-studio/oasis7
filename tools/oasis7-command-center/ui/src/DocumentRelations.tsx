import { ArrowRight, FileText } from 'lucide-solid';
import { For, Show } from 'solid-js';
import type { DocumentPairings, DocumentPairing, DocumentSection } from './generated/core';

const pairingIssues: Record<string, string> = {
  'multiline pairing declaration; projection incomplete': '声明跨多行，目前仅解析首行链接',
  'pairing declaration exceeds 4096 bytes; projection incomplete': '声明超过 4096 字节，未解析该字段',
  'pairing target limit 32 exceeded; projection incomplete': '超过 32 个目标，仅展示前 32 条',
  'allocation cell has no explicit Markdown link': '单元格没有可识别的 Markdown 链接，未推断条款关系',
  'allocation row exceeds 4096 bytes': '表格行超过 4096 字节，未解析链接',
  'allocation link limit 64 exceeded': '超过每文件 64 个承接链接，仅保留预算内链接',
  'pairing declaration has no explicit Markdown link': '声明中没有可识别的 Markdown 链接',
};
const resolutions: Record<string, string> = { resolved: '目标文件已读取', fragment_unchecked: '文件已读取，章节锚点尚未核对', missing: '目标已删除', missing_fragment: '目标文件中未找到章节锚点', ambiguous: '目标锚点重复，无法唯一定位', unread: '目标尚未读取', outside_scope: '目标不在本次清单或路径不安全' };

export default function DocumentRelations(props: {
  sourceTitle?: string;
  relations: DocumentPairings | null | undefined;
  mode: 'pairing' | 'product_pairing' | 'upstream' | 'allocation_upstream' | 'allocation_design';
  sections: DocumentSection[];
  showSource: (section: DocumentSection, origin?: HTMLElement) => void;
}) {
  const subject = () => ({pairing:'配对',product_pairing:'配对',upstream:'上游',allocation_upstream:'承接上游',allocation_design:'承接设计'}[props.mode]);
  const title = () => ({pairing:'显式配对系统设计',product_pairing:'显式配对产品设计',upstream:'显式上游声明',allocation_upstream:'承接上游条款',allocation_design:'本设计承接条款'}[props.mode]);
  function targetSection(edge: DocumentPairing) {
    if (edge.resolution !== 'resolved' || !edge.target || !edge.targetSection || !edge.targetRevision) return undefined;
    const path = edge.target.split('#')[0];
    return props.sections.find(s => s.path === path && s.id === edge.targetSection && s.revision === edge.targetRevision);
  }
  const targetKind = (edge: DocumentPairing) => ({prd:'需求文档',design:'设计引用',entry:'结构入口',other:'其他引用'}[edge.targetKind ?? ''] ?? '未分类目标');
  return <section aria-label={title()}>
    <h5>{title()}</h5>
    <Show when={props.mode === 'upstream'}><p class="muted small">仅展示首个顶层非一级标题之前的顶层“上游”字段。目标类型来自清单路径分类，声明和链接存在不代表需求已经实现；正文承接表另行展示。</p></Show>
    <Show when={props.relations} fallback={<p>旧清单尚未解析{subject()}，请刷新本地事实。</p>}>{relations => <>
      <Show when={relations().issue}><p role="status">{subject()}解析不完整：{pairingIssues[relations().issue!] ?? relations().issue}</p></Show>
      <Show when={!relations().targets.length && !relations().issue}><p>{props.mode === 'pairing' ? '本次范围未发现显式配对系统设计链接。' : props.mode === 'product_pairing' ? '本次范围未发现显式配对产品设计链接。' : props.mode === 'upstream' ? '本次文件头范围未发现显式上游链接。' : '本单元格明确声明无链接。'}</p></Show>
      <For each={relations().targets}>{edge => <div class="small" classList={{'pairing-map':props.mode==='pairing'||props.mode==='product_pairing'}}>
        <Show when={props.mode==='pairing'||props.mode==='product_pairing'}><div class="pairing-origin"><FileText size={20} aria-hidden="true"/><strong>{props.sourceTitle??'当前文档'}</strong></div><div class="pairing-arrow"><ArrowRight size={20} aria-hidden="true"/><span>显式配对</span></div></Show><div class="pairing-target">
        <p>第 {edge.declarationLine} 行 · {edge.target ?? edge.destination} · {resolutions[edge.resolution] ?? edge.resolution}</p>
        <Show when={props.mode !== 'pairing' && props.mode !== 'product_pairing'}><p class="muted">目标类型：{targetKind(edge)}</p></Show>
        <Show when={targetSection(edge)}>{target => <button onClick={event => props.showSource(target(), event.currentTarget)}>打开{subject()}章节：{target().title}</button>}</Show>
        <Show when={edge.resolution === 'resolved' && !targetSection(edge)}><p class="muted">{edge.targetRevision ? '目标没有可定位的索引章节。' : '旧清单尚无章节定位，请刷新本地事实。'}</p></Show>
      </div></div>}</For>
    </>}</Show>
  </section>;
}
