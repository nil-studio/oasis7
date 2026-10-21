import { createEffect, createMemo, createSignal, For, on, untrack } from 'solid-js';
import { ChevronRight, FileText } from 'lucide-solid';
import type { DocumentSection } from './generated/core';

type Node = { section: DocumentSection; level: number; children: Node[] };
export function chapterTree(sections: DocumentSection[]): Node[] {
  const roots: Node[] = [], stack: Node[] = [];
  for (const section of sections) {
    const lines = section.content.replace(/^(?:[ \t]*\r?\n)*/, '').split('\n');
    const atx = /^ {0,3}(#{1,6})(?:\s|$)/.exec(lines[0]);
    let level = atx?.[1].length ?? 0;
    if (!level && !/^ {4}|^\t/.test(lines[0]) && lines[0]?.trim() && !/^\s*(?:---|```|~~~)/.test(lines[0])) {
      for (const line of lines.slice(1)) {
        if (!line.trim()) break;
        if (/^ {0,3}=+\s*$/.test(line)) { level = 1; break; }
        if (/^ {0,3}-+\s*$/.test(line)) { level = 2; break; }
      }
    }
    const node: Node = { section, level, children: [] };
    while (stack.length && (!level || stack.at(-1)!.level >= level)) stack.pop();
    (stack.at(-1)?.children ?? roots).push(node);
    if (level) stack.push(node);
  }
  return roots;
}

export default function ChapterOutline(props: { chapters: DocumentSection[]; current?: string; choose: (section: DocumentSection) => void }) {
  const sourceKey = createMemo(() => JSON.stringify(props.chapters.map(section => [section.id, section.revision, section.line])));
  const roots = createMemo(() => { sourceKey(); return untrack(() => chapterTree(props.chapters)); });
  const [expanded, setExpanded] = createSignal(new Set<string>());
  const contains = (node: Node): boolean => node.section.id === props.current || node.children.some(contains);
  createEffect(on(roots, nodes => setExpanded(new Set(nodes.filter(n => n.children.length).map(n => n.section.id)))));
  createEffect(on(() => [roots(), props.current] as const, ([nodes]) => {
    const ancestors: string[] = [];
    function visit(node: Node): boolean {
      if (node.section.id === props.current) return true;
      if (node.children.some(visit)) { ancestors.push(node.section.id); return true; }
      return false;
    }
    nodes.forEach(visit);
    setExpanded(old => new Set([...old, ...ancestors]));
  }));
  function toggle(id: string) { setExpanded(old => { const next = new Set(old); if (next.has(id)) next.delete(id); else next.add(id); return next; }); }
  const branch = (nodes: Node[]) => <ul><For each={nodes}>{node => <li classList={{ 'chapter-ancestor': contains(node) && node.section.id !== props.current }}>
    <div class="chapter-row">
      {node.children.length ? <button class="chapter-toggle" aria-label={`${expanded().has(node.section.id) ? '折叠' : '展开'}章节：${node.section.title}`} aria-expanded={expanded().has(node.section.id)} onClick={() => toggle(node.section.id)}><ChevronRight size={16} aria-hidden="true"/></button> : <FileText class="chapter-leaf" size={16} aria-hidden="true"/>}
      <button class="chapter-title" aria-label={`${node.section.title} · 第 ${node.section.line} 行`} title={`第 ${node.section.line} 行`} aria-current={props.current === node.section.id ? 'location' : undefined} onClick={() => props.choose(node.section)}>{node.section.title}</button>
    </div>
    {node.children.length > 0 && expanded().has(node.section.id) && branch(node.children)}
  </li>}</For></ul>;
  return <div class="chapter-links chapter-outline">{branch(roots())}</div>;
}
