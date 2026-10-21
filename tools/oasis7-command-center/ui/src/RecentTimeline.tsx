import { For } from 'solid-js';
import { StatusIcon } from './VisualStatus';
import type { WorkItem } from './generated/core';

export default function RecentTimeline(props:{works:WorkItem[];open:(work:WorkItem,origin:HTMLElement)=>void;status:(value:string)=>string}) {
  const groups=()=>{
    const result:{date:string;items:WorkItem[]}[]=[];
    for(const work of props.works){
      const date=new Date(work.result.evidence.eventAt!).toLocaleDateString('zh-CN',{year:'numeric',month:'long',day:'numeric'});
      const last=result.at(-1);if(last?.date===date)last.items.push(work);else result.push({date,items:[work]});
    }
    return result;
  };
  return <div class="compact-timeline"><For each={groups()}>{group=><section class="timeline-day"><h4>{group.date}</h4><ol><For each={group.items}>{work=><li><StatusIcon value={work.result.value}/><div><button class="row-title" onClick={event=>props.open(work,event.currentTarget)}>{work.purpose.value}</button><div class="timeline-facts"><span class="tag">{props.status(work.result.value)}</span><details><summary><time dateTime={work.result.evidence.eventAt!}>{new Date(work.result.evidence.eventAt!).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit',hour12:false})}</time></summary><time dateTime={work.result.evidence.eventAt!}>{work.result.evidence.eventAt}</time></details></div></div></li>}</For></ol></section>}</For></div>;
}
