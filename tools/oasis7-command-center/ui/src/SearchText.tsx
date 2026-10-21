import { For } from 'solid-js';
export function matchParts(text:string, query:string):{text:string;match:boolean}[] {
  const needle=query.trim(); if(!needle)return [{text,match:false}];
  const escaped=needle.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
  const pattern=new RegExp(escaped,'giu'); const parts:{text:string;match:boolean}[]=[];let from=0;
  for(const hit of text.matchAll(pattern)){const at=hit.index!;if(at>from)parts.push({text:text.slice(from,at),match:false});parts.push({text:hit[0],match:true});from=at+hit[0].length;}
  if(from<text.length)parts.push({text:text.slice(from),match:false});return parts;
}
export default function SearchText(props:{text:string;query:string}){return <For each={matchParts(props.text,props.query)}>{part=>part.match?<mark>{part.text}</mark>:part.text}</For>;}
export function matchSnippet(text:string,query:string){const match=matchParts(text,query).find(p=>p.match);if(!match)return '';const at=text.indexOf(match.text);const start=Math.max(0,at-45),end=Math.min(text.length,at+match.text.length+85);return `${start?'…':''}${text.slice(start,end).replace(/\s+/g,' ')}${end<text.length?'…':''}`;}
