import { createSignal, For, Show } from 'solid-js';
import type { DocumentSection } from './types';
import { bridge } from './bridge';
import SearchText from './SearchText';

// Resolve only explicit indexed targets; text and links never execute source HTML.
export function resolveMarkdownSource(target: string, sourcePath: string, sections: DocumentSection[]): DocumentSection {
  if (!target || /[\\\u0000-\u0020]/.test(target) || target.startsWith('/') || target.startsWith('//') || /^[a-z][a-z0-9+.-]*:/i.test(target) || target.includes('?')) throw new Error('不支持此正文链接；仅可导航仓库内明确索引的文档。');
  const split = target.indexOf('#');
  const rawPath = split < 0 ? target : target.slice(0, split);
  const anchor = split < 0 ? undefined : decodeURIComponent(target.slice(split + 1));
  const decoded = decodeURIComponent(rawPath);
  if (/[\\\u0000-\u0020:#?]/.test(decoded) || decoded.startsWith('/') || (anchor !== undefined && /[\u0000-\u0020#]/.test(anchor))) throw new Error('正文链接路径或锚点无效。');
  const parts = sourcePath.split('/').slice(0, -1);
  if (rawPath) {
    for (const part of decoded.split('/')) {
      if (!part || part === '.') continue;
      if (part === '..') { if (!parts.length) throw new Error('正文链接不能越出仓库。'); parts.pop(); }
      else parts.push(part);
    }
  }
  const path = rawPath ? parts.join('/') : sourcePath;
  const matches = sections.filter(section => anchor !== undefined && anchor !== '' ? section.id === `${path}#${anchor}` : section.path === path).sort((a,b) => a.line-b.line);
  if (!matches.length) throw new Error('正文链接目标尚未索引、锚点缺失或不在本次读取范围内。');
  if (anchor !== undefined && anchor !== '' ? matches.length !== 1 : matches.length > 1 && matches[0].line === matches[1].line) throw new Error('正文链接目标存在歧义，请从文档树核对。');
  return matches[0];
}
export default function MarkdownText(props: { text: string; searchQuery?: string; sourcePath?: string; sections?: DocumentSection[]; showSource?: (section: DocumentSection, origin?: HTMLElement) => void; onError?: (error: unknown) => void }) {
  const [message,setMessage] = createSignal('');
  async function open(target: string, origin: HTMLElement) {
    setMessage('');
    try {
      if (/^https:\/\//i.test(target)) await bridge.openSource(target);
      else {
        if (!props.sourcePath || !props.showSource) throw new Error('当前正文缺少来源上下文，无法安全打开本地链接。');
        props.showSource(resolveMarkdownSource(target, props.sourcePath, props.sections ?? []), origin);
      }
    } catch (error) { setMessage(error instanceof Error ? error.message : String(error)); props.onError?.(error); }
  }
  const inline = (text: string) => text.split(/(\[[^\]]+\]\([^)]+\))/g).map(part => {
    const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
    return link ? <button class="text-link" onClick={event => void open(link[2], event.currentTarget)}><SearchText text={link[1]} query={props.searchQuery??''}/></button> : <SearchText text={part.replace(/[`*_]/g, '')} query={props.searchQuery??''}/>;
  });
  const blocks = () => props.text.split(/\n\s*\n/).filter(block => block.trim() && !/^<a\s+id=/.test(block.trim()));
  return <article class="document-prose"><Show when={message()}><p role="status" class="notice">{message()}</p></Show><For each={blocks()}>{block => {
    const lines = block.split('\n');
    if (block.startsWith('```')) return <pre><SearchText text={lines.slice(1, -1).join('\n')} query={props.searchQuery??''}/></pre>;
    if (lines.length > 1 && lines.every(line => line.trim().startsWith('|'))) {
      const rows = lines.filter(line => !/^\|[\s:|-]+\|\s*$/.test(line.trim())).map(line => line.trim().slice(1,-1).split('|'));
      return <div class="table-scroll"><table><thead><tr><For each={rows[0]}>{cell => <th>{inline(cell)}</th>}</For></tr></thead><tbody><For each={rows.slice(1)}>{row => <tr><For each={row}>{cell => <td>{inline(cell)}</td>}</For></tr>}</For></tbody></table></div>;
    }
    return <div><For each={lines}>{line => <Show when={/^#{1,6}\s/.test(line)} fallback={<p>{inline(line.replace(/^\s*[-+]\s/, '• '))}</p>}><h4>{inline(line.replace(/^#{1,6}\s+/, ''))}</h4></Show>}</For></div>;
  }}</For></article>;
}
