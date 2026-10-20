import type { DocumentSection } from './types';
import type { DocumentTreeReadingSelection, DocumentTreeRouteStep } from './generated/core';
export type TreeSourcePosition = {section:DocumentSection;scrollTop:number;userInitiated:boolean};
export type TreeFileRestore = {section?:DocumentSection;unresolved?:boolean;scrollTop:number;passive:boolean};
export const routeKey = (route:DocumentTreeRouteStep[]) => JSON.stringify(route.map(step=>[step.path,step.sourcePath,step.destination,step.group,step.duplicateIndex,step.duplicateCount]));
export const sameRoute = (a:DocumentTreeRouteStep[],b:DocumentTreeRouteStep[]) => routeKey(a) === routeKey(b);
export function selectedSection(selection:DocumentTreeReadingSelection,sections:DocumentSection[]) {
  return sections.find(section => section.path === selection.sectionPath && section.id === selection.sectionId && section.revision === selection.sectionRevision);
}

// One process-wide ordered writer prevents an unmounted tree's pending save
// from overtaking the next view's last-view pointer. Payloads are captured by callers.
let writerGeneration=0;
let writes=Promise.resolve();
export const beginTreeReadingWriter=()=>++writerGeneration;
export function queueTreeReadingWrite(owner:number,valid:()=>boolean,write:()=>Promise<void>) {
  const result=writes.catch(()=>{}).then(async()=>{if(owner===writerGeneration&&valid())await write();});
  writes=result.catch(()=>{});return result;
}

export function readingNotice(value:string) {
  const exact:Record<string,string>={
    'source baseline changed; stable document routes will be reprojected without title matching':'来源基线已变化；按原文档路径与显式导航重新定位，不按标题匹配。',
    'saved chapter identity or revision changed; original position retained, no automatic chapter redirect':'原章节身份或内容版本已变化；保留原位置，不自动跳到其他章节。',
    'tree reading record is corrupt; reset this view to recover':'此树阅读记录损坏；可明确重置此视图以恢复。',
    'tree reading record identity or state is invalid; reset this view to recover':'此树阅读记录身份或内容无效；可明确重置此视图以恢复。',
    'saved navigation declaration is ambiguous; restored nearest confirmed ancestor without guessing':'原导航声明存在歧义；已返回最近核实祖先，不猜选目标。',
    'saved navigation target was removed or its explicit route changed; restored nearest confirmed ancestor without title matching':'原导航目标已移除或显式路由已变化；已返回最近核实祖先，不按标题匹配。',
    'saved navigation target is deleted, unresolved or ambiguous; restored nearest confirmed ancestor without redirecting to another file':'原导航目标已删除、未解析或存在歧义；已返回最近核实祖先，不跳到其他文件。',
    'saved navigation reaches a cycle; restored nearest confirmed ancestor':'原导航到达循环；已返回最近核实祖先。',
    'saved navigation reached depth budget; restored nearest confirmed ancestor':'原导航达到深度预算；已返回最近核实祖先。',
  };
  if(exact[value])return exact[value];
  if(value.startsWith('tree reading record unsupported'))return '此树阅读记录来自不支持的版本；记录保持保留，明确重置后才能保存新位置。';
  if(value.startsWith('tree reading record corrupt'))return '此树阅读记录损坏；可重置此视图，其他个人记录保持保留。';
  if(value.startsWith('tree last-view record'))return '最近树视图记录损坏或版本不受支持；保留默认视图及原记录。';
  return value;
}
