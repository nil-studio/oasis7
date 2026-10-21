import { cleanup,fireEvent,render,screen,waitFor } from '@solidjs/testing-library';
import { afterEach,expect,it,vi } from 'vitest';
import { createSignal } from 'solid-js';
import SearchResults from './SearchResults';
import SourceSearchMatches from './SourceSearchMatches';
import MarkdownText from './MarkdownText';
import { matchParts,matchSnippet } from './SearchText';
import type { DocumentSection } from './types';
afterEach(cleanup);
const section=(id:string,path:string,content:string)=>({id,path,content,title:id,line:1,revision:'r',committedRevision:'r',dirty:false} as DocumentSection);
it('matches literal metacharacters and Unicode without executing markup',()=>{
 expect(matchParts('恢复 A+B a+b <script>','a+b').filter(p=>p.match).map(p=>p.text)).toEqual(['A+B','a+b']);
 expect(matchSnippet('前文。恢复后继续。','恢复')).toContain('恢复');
 render(()=><MarkdownText text={'# 标题\n\n恢复 <script>alert(1)</script> 恢复'} searchQuery="恢复"/>);
 expect(document.querySelectorAll('mark')).toHaveLength(2);expect(document.querySelector('script')).toBeNull();
});
it('groups returned sections by document with snippets and original entry identity',()=>{
 const first=section('第一章','doc/a.prd.md','先恢复，再继续'),second=section('第二章','doc/a.prd.md','恢复旧工作'),other=section('第三章','doc/b.design.md','正文无此词');const open=vi.fn();
 render(()=><SearchResults items={[first,second,other]} query="恢复" onOpen={open} onEdit={()=>{}} onClear={()=>{}}/>);
 expect(document.querySelectorAll('.search-document')).toHaveLength(2);expect(screen.getByText('2 章')).toBeInTheDocument();expect(screen.getByText('查询：恢复')).toBeInTheDocument();
 expect(screen.getByText(/标题、路径或索引文本命中/)).toBeInTheDocument();fireEvent.click(screen.getByRole('button',{name:'第一章'}));expect(open.mock.calls[0][0]).toBe(first);expect(open.mock.calls[0][1].dataset.sourceRef).toBe('第一章');
});
it('does not call a capped result set a complete total',()=>{
 render(()=><SearchResults items={Array.from({length:100},(_,i)=>section(String(i),'doc/a.md','恢复'))} query="恢复" onOpen={()=>{}} onEdit={()=>{}} onClear={()=>{}}/>);
 expect(screen.getByRole('status')).toHaveTextContent('不是全部匹配总数');
});
it('navigates rendered prose matches and resets for a different source',async()=>{
 const scroll=vi.fn();Element.prototype.scrollIntoView=scroll;
 const [text,setText]=createSignal('恢复一次，恢复两次');
 render(()=><SourceSearchMatches query="恢复" identity={text()}><MarkdownText text={text()} searchQuery="恢复"/></SourceSearchMatches>);
 await waitFor(()=>expect(screen.getByRole('status')).toHaveTextContent('1 / 2'));
 const next=screen.getByRole('button',{name:'下一处'});next.focus();fireEvent.click(next);expect(screen.getByRole('status')).toHaveTextContent('2 / 2');expect(document.activeElement).toBe(next);
 fireEvent.click(next);expect(screen.getByRole('status')).toHaveTextContent('1 / 2');setText('正文没有匹配');
 await waitFor(()=>expect(screen.getByRole('status')).toHaveTextContent('正文无字面匹配'));expect(next).toBeDisabled();
});
