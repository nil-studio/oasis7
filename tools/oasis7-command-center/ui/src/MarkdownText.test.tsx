import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import MarkdownText, { resolveMarkdownSource } from './MarkdownText';
import type { DocumentSection } from './types';
const api=vi.hoisted(()=>({openSource:vi.fn()}));
vi.mock('./bridge',()=>({bridge:api}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
const section=(path:string,anchor:string,line=1):DocumentSection=>({id:`${path}#${anchor}`,path,title:anchor,content:'body',line,revision:'rev',committedRevision:'head',dirty:false,factKind:'source',lifecycle:null});
const current=section('doc/topic/source.md','current');
const target=section('doc/target.md','目标');
describe('indexed Markdown body navigation',()=>{
  it.each(['../target.md#%E7%9B%AE%E6%A0%87','.././target.md#目标'])('normalizes %s and navigates inside the App with the original focus element',targetLink=>{
    const showSource=vi.fn();render(()=><MarkdownText text={`[目标](${targetLink})`} sourcePath={current.path} sections={[current,target]} showSource={showSource}/>);
    const button=screen.getByRole('button',{name:'目标'});fireEvent.click(button);
    expect(showSource).toHaveBeenCalledWith(target,button);expect(api.openSource).not.toHaveBeenCalled();
  });
  it('resolves same-document anchors and the unique first section of a document',()=>{
    const later=section(target.path,'later',20);
    expect(resolveMarkdownSource('#current',current.path,[current])).toBe(current);
    expect(resolveMarkdownSource('../target.md',current.path,[later,target])).toBe(target);
  });
  it.each(['../../../outside.md','%2e%2e/%2e%2e/%2e%2e/outside.md','/etc/passwd','javascript:alert','file:///etc/passwd','//example.com/file','..%5ctarget.md','../target.md%00','../target.md?execute=true'])('rejects unsafe target %s without invoking an opener',link=>{
    const showSource=vi.fn();render(()=><MarkdownText text={`[link](${link})`} sourcePath={current.path} sections={[target]} showSource={showSource}/>);
    fireEvent.click(screen.getByRole('button',{name:'link'}));expect(screen.getByRole('status')).toBeInTheDocument();expect(showSource).not.toHaveBeenCalled();expect(api.openSource).not.toHaveBeenCalled();
  });
  it.each([{sections:[]},{sections:[target,target]}])('explains unread or ambiguous indexed targets rather than silently failing',({sections})=>{
    render(()=><MarkdownText text="[目标](../target.md#目标)" sourcePath={current.path} sections={sections} showSource={vi.fn()}/>);
    fireEvent.click(screen.getByRole('button',{name:'目标'}));expect(screen.getByRole('status')).toHaveTextContent(sections.length?'歧义':'尚未索引');expect(api.openSource).not.toHaveBeenCalled();
  });
  it('retains the finite HTTPS opener and surfaces its rejection beside the document',async()=>{
    api.openSource.mockRejectedValueOnce(new Error('此外部来源不受支持'));
    render(()=><MarkdownText text="[远端](https://example.com/path)"/>);fireEvent.click(screen.getByRole('button',{name:'远端'}));
    await waitFor(()=>expect(screen.getByRole('status')).toHaveTextContent('此外部来源不受支持'));expect(api.openSource).toHaveBeenCalledWith('https://example.com/path');
  });
});
