import { cleanup, fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import ChapterOutline, { chapterTree } from './ChapterOutline';
import type { DocumentSection } from './generated/core';
const section=(id:string,content:string,line:number):DocumentSection=>({id,title:id,content,line,path:'doc/example.md',revision:'r',committedRevision:'head',dirty:false,factKind:'sourceFact',lifecycle:null});
afterEach(cleanup);
it('uses source heading levels including jumps and Setext without guessing numbered titles',()=>{
 const nodes=chapterTree([section('intro','前言',1),section('root','# 根',3),section('branch','### 分支',5),section('leaf','叶标题\n续行\n---\n正文',8),section('unknown','    # 代码不是标题',12),section('numbered','1.2 不是 Markdown 标题',14)]);
 expect(nodes.map(n=>n.section.id)).toEqual(['intro','root','unknown','numbered']);
 expect(nodes[1].children.map(n=>n.section.id)).toEqual(['branch','leaf']);
});
it('reveals a restored deep chapter, permits collapse and chooses the exact original section',async()=>{
 const chapters=[section('root','# 根',1),section('branch','## 分支',4),section('leaf','### 叶子',8),section('other','## 其他',12)];
 const [current,setCurrent]=createSignal('root');const choose=vi.fn();
 render(()=><ChapterOutline chapters={chapters} current={current()} choose={choose}/>);
 expect(screen.queryByRole('button',{name:'leaf · 第 8 行'})).not.toBeInTheDocument();
 setCurrent('leaf');await waitFor(()=>expect(screen.getByRole('button',{name:'leaf · 第 8 行'})).toHaveAttribute('aria-current','location'));
 fireEvent.click(screen.getByRole('button',{name:'折叠章节：branch'}));expect(screen.queryByRole('button',{name:'leaf · 第 8 行'})).not.toBeInTheDocument();
 expect(screen.getByRole('button',{name:'branch · 第 4 行'}).closest('li')).toHaveClass('chapter-ancestor');
 fireEvent.click(screen.getByRole('button',{name:'展开章节：branch'}));fireEvent.click(screen.getByRole('button',{name:'leaf · 第 8 行'}));expect(choose).toHaveBeenCalledWith(chapters[2]);
 expect(screen.getByRole('button',{name:'leaf · 第 8 行'})).toHaveTextContent('leaf');expect(screen.queryByText('第 8 行')).not.toBeInTheDocument();
});

it('preserves manual expansion across unchanged source refreshes and resets for a new revision',()=>{
 const initial=[section('root','# Root',1),section('branch','## Branch',3),section('leaf','### Leaf',5)];const [chapters,setChapters]=createSignal(initial);
 render(()=><ChapterOutline chapters={chapters()} current="root" choose={()=>{}}/>);
 fireEvent.click(screen.getByRole('button',{name:'展开章节：branch'}));
 setChapters(initial.map(value=>({...value})));expect(screen.getByRole('button',{name:'leaf · 第 5 行'})).toBeInTheDocument();
 setChapters(initial.map(value=>({...value,revision:'new'})));expect(screen.queryByRole('button',{name:'leaf · 第 5 行'})).not.toBeInTheDocument();
});
