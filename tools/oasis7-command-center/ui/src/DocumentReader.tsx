import { createEffect, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { BookOpen } from 'lucide-solid';
import MarkdownText from './MarkdownText';
import DocumentRelations from './DocumentRelations';
import type { DocumentSection, IndexedDocument } from './generated/core';
import type { TreeFileRestore, TreeSourcePosition } from './documentTreeReading';

export default function DocumentReader(props:{file:IndexedDocument;chapters:DocumentSection[];sections:DocumentSection[];restore?:TreeFileRestore;onPosition:(position:TreeSourcePosition)=>void;showSource:(section:DocumentSection,origin?:HTMLElement)=>void}) {
  const [chapter,setChapter]=createSignal<DocumentSection>();
  const media=typeof window.matchMedia==='function'?window.matchMedia('(min-width: 901px)'):undefined;
  const [wide,setWide]=createSignal(media?.matches??true);
  const resize=()=>setWide(media!.matches);media?.addEventListener('change',resize);onCleanup(()=>media?.removeEventListener('change',resize));
  let body:HTMLElement|undefined, programmaticTop:number|null=null;
  function choose(section:DocumentSection,user=true,top=0) {
    setChapter(section);programmaticTop=top;
    props.onPosition({section,scrollTop:top,userInitiated:user});
    queueMicrotask(()=>{if(chapter()!==section||!body?.isConnected)return;body.scrollTop=top;if(user)body.focus({preventScroll:true});});
  }
  createEffect(on(()=>JSON.stringify([props.file.fileKey,props.file.revision]),()=>{
    const saved=props.restore?.section;
    const exact=saved&&props.chapters.find(s=>s.path===saved.path&&s.id===saved.id&&s.revision===saved.revision);
    const first=props.chapters.find(s=>s.content.replace(/^#{1,6} .*$/gm,'').trim())??props.chapters[0];
    setChapter(undefined);
    if(props.restore?.unresolved||(saved&&!exact))return;
    if(exact||first)choose((exact||first)!,false,exact?props.restore?.scrollTop??0:0);
  }));
  const links=()=> <div class="chapter-links"><For each={props.chapters}>{section=><button aria-current={chapter()?.id===section.id?'location':undefined} onClick={()=>choose(section)}>{section.title} · 第 {section.line} 行</button>}</For></div>;
  return <section class="document-reader" aria-label="文档阅读器">
    <h4 class="reader-title"><BookOpen size={20} aria-hidden="true"/>{props.file.title}</h4>
    <DocumentRelations sourceTitle={props.file.title} relations={props.file.pairings} mode={props.file.path.startsWith('doc/product/')?'product_pairing':'pairing'} sections={props.sections} showSource={props.showSource}/>
    <Show when={props.chapters.length} fallback={<p>已读取，文件没有可展示的章节。</p>}>
      <div class="reader-layout"><Show when={wide()} fallback={<details class="reader-directory"><summary>章节目录（{props.chapters.length}）</summary>{links()}</details>}><nav class="reader-directory" aria-label="章节目录">{links()}</nav></Show>
      <article class="reader-body" ref={body} tabindex="0" aria-label="文档正文" onWheel={()=>{programmaticTop=null;}} onTouchMove={()=>{programmaticTop=null;}} onKeyDown={()=>{programmaticTop=null;}} onScroll={()=>{const current=chapter(),top=body?.scrollTop??0;const passive=programmaticTop!==null&&Math.abs(top-programmaticTop)<1;programmaticTop=null;if(current&&!passive)props.onPosition({section:current,scrollTop:top,userInitiated:true});}}>
        <Show when={chapter()} fallback={<p role="status">原阅读位置无法在当前文档恢复，请从目录明确选择当前章节。</p>}>{section=><><MarkdownText text={section().content} sourcePath={section().path} sections={props.sections} showSource={props.showSource}/></>}</Show>
      </article></div>
    </Show>
  </section>;
}
