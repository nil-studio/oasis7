import { createEffect, createSignal, onCleanup, Show } from 'solid-js';
import type { JSX } from 'solid-js';
export default function SourceSearchMatches(props:{query:string;identity:string;children:JSX.Element}){
 let root:HTMLDivElement|undefined; const [marks,setMarks]=createSignal<HTMLElement[]>([]),[index,setIndex]=createSignal(0);
 function move(next:number){const list=marks();if(!list.length)return;const at=(next+list.length)%list.length;list.forEach((mark,i)=>mark.classList.toggle('active-match',i===at));setIndex(at);list[at].scrollIntoView?.({block:'center'});}
 createEffect(()=>{props.identity;props.query;setMarks([]);setIndex(0);const frame=requestAnimationFrame(()=>{const list=Array.from(root?.querySelectorAll<HTMLElement>('.document-prose mark')??[]);setMarks(list);if(list.length)move(0);});onCleanup(()=>cancelAnimationFrame(frame));});
 return <div ref={root}><Show when={props.query}><div class="source-match-nav" role="region" aria-label="正文匹配导航"><span>查询：{props.query}</span><span role="status">{marks().length?`${index()+1} / ${marks().length} 处`:'正文无字面匹配；可能命中标题、路径或索引文本。'}</span><button disabled={!marks().length} onClick={()=>move(index()-1)}>上一处</button><button disabled={!marks().length} onClick={()=>move(index()+1)}>下一处</button></div></Show>{props.children}</div>;
}
