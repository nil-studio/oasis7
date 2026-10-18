import { For, Show } from 'solid-js';
import { bridge } from './bridge';

// Render text as nodes, never source HTML. Local links still go through Rust's finite opener.
export default function MarkdownText(props: { text: string; onError?: (error: unknown) => void }) {
  const inline = (text: string) => text.split(/(\[[^\]]+\]\([^)]+\))/g).map(part => {
    const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
    return link ? <button class="text-link" onClick={() => void bridge.openSource(link[2]).catch(props.onError ?? (() => {}))}>{link[1]}</button> : part.replace(/[`*_]/g, '');
  });
  const blocks = () => props.text.split(/\n\s*\n/).filter(block => block.trim() && !/^<a\s+id=/.test(block.trim()));
  return <article class="document-prose"><For each={blocks()}>{block => {
    const lines = block.split('\n');
    if (block.startsWith('```')) return <pre>{lines.slice(1, -1).join('\n')}</pre>;
    if (lines.length > 1 && lines.every(line => line.trim().startsWith('|'))) {
      const rows = lines.filter(line => !/^\|[\s:|-]+\|\s*$/.test(line.trim())).map(line => line.trim().slice(1,-1).split('|'));
      return <div class="table-scroll"><table><thead><tr><For each={rows[0]}>{cell => <th>{inline(cell)}</th>}</For></tr></thead><tbody><For each={rows.slice(1)}>{row => <tr><For each={row}>{cell => <td>{inline(cell)}</td>}</For></tr>}</For></tbody></table></div>;
    }
    return <div><For each={lines}>{line => <Show when={/^#{1,6}\s/.test(line)} fallback={<p>{inline(line.replace(/^\s*[-+]\s/, '• '))}</p>}><h4>{inline(line.replace(/^#{1,6}\s+/, ''))}</h4></Show>}</For></div>;
  }}</For></article>;
}
