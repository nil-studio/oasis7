import { cleanup,fireEvent,render,screen,waitFor } from '@solidjs/testing-library';
import { afterEach,beforeEach,expect,it,vi } from 'vitest';
import App from './App';
import type { Snapshot,DocumentSection } from './types';
const api=vi.hoisted(()=>({snapshot:vi.fn(),github:vi.fn(),documentInventory:vi.fn(),documentSemanticTree:vi.fn(),loadDocumentTreeLastView:vi.fn(),loadDocumentTreeReading:vi.fn(),saveDocumentTreeReading:vi.fn(),recordReading:vi.fn(),readVersion:vi.fn()}));
vi.mock('./bridge',()=>({bridge:api,connected:()=>true,errorMessage:String,observedTime:()=>'',statusLabel:String,acceptsEnter:()=>false}));
vi.mock('./WorkOverview',()=>({default:()=>null}));
vi.mock('./RuntimePanel',()=>({default:()=>null}));vi.mock('./LocalTools',()=>({default:()=>null}));
const section:DocumentSection={id:'explicit',path:'doc/product/module/prd.md',title:'恢复章节',revision:'rev',committedRevision:'head',dirty:false,content:Array.from({length:80},(_,i)=>`原文第 ${i+1} 行`).join('\n'),line:1,factKind:'sourceFact',lifecycle:null};
const route=[{path:section.path,sourcePath:'doc/product/README.md',destination:section.path,group:null,duplicateIndex:0,duplicateCount:1}];
const data=(sections=[section]):Snapshot=>({project:{id:'project',path:'/repo',head:'head',branch:'main',gitCommonDir:'/repo/.git',environment:'local'},sources:[{kind:'local',generation:'1',state:'ready',lastAttempt:'1',lastSuccess:'1',sourceRevision:'head',coverage:'',error:null}],sections,decisions:[],preferences:{projectPath:'/repo',query:'',view:'rules'},associations:[],readingPositions:[],git:{status:'',worktrees:''}});
function setup(deleted=false){
  api.snapshot.mockResolvedValue(data(deleted?[]:[section]));api.github.mockResolvedValue({state:'unconnected',pulls:[]});
  api.documentInventory.mockResolvedValue({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',candidates:1,read:1,omitted:0,readBytes:10,totalMatches:0,partial:false,stale:false,error:null,files:[],nextCursor:null});
  api.loadDocumentTreeLastView.mockResolvedValue({status:'ready',view:'product',notice:null});
  const file={fileKey:'file',path:section.path,title:'模块需求',kind:'prd',state:deleted?'deleted':'read',reason:deleted?'deleted':null,revision:deleted?null:'rev',bytes:10,dirty:false,lifecycle:null,identity:null,pairings:null,upstream:null,allocations:null,navigation:null};
  api.documentSemanticTree.mockResolvedValue({schemaVersion:1,projectId:'project',snapshotId:'snapshot',baselineOid:'head',generatedAt:'1',view:'product',parentOccurrence:'',parentRoute:[],routeNotice:null,breadcrumbs:[],candidates:1,read:deleted?0:1,omitted:deleted?1:0,stale:false,error:null,eligibleFiles:1,matchingFiles:1,hiddenHistoryFiles:0,lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all',totalChildren:1,entries:[{occurrenceId:'module',stableRoute:route,label:'模块需求',path:section.path,kind:'prd',matchingFiles:1,historicalReferenceFiles:0,historicalReferencePaths:[],boundary:null,cycle:false,depthLimited:false,sourcePath:'doc/product/README.md',sourceLine:4,relationship:null,file,hasChildren:false}],nextCursor:null,issues:[]});
  api.loadDocumentTreeReading.mockResolvedValue({status:'ready',state:{filters:{query:'',readFilter:'all',lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all'},expandedRoutes:[],parentRoute:route,selection:{route,path:section.path,sectionPath:section.path,sectionId:section.id,sectionRevision:section.revision},treeScrollTop:0,detailScrollTop:42},notice:null,resolvedSectionId:deleted?null:section.id});
  api.saveDocumentTreeReading.mockResolvedValue(undefined);api.recordReading.mockResolvedValue(undefined);api.readVersion.mockResolvedValue(section);
}
beforeEach(()=>{Object.values(api).forEach(mock=>mock.mockReset());setup();});afterEach(cleanup);
it('restores exact source scroll passively, captures accessibility scroll without wheel, and clears a closed chapter',async()=>{
  const outside=document.createElement('button');document.body.append(outside);outside.focus();render(()=><App/>);
  const pre=await screen.findByLabelText('来源章节原文');await waitFor(()=>expect(pre.scrollTop).toBe(42));expect(outside).toHaveFocus();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();expect(api.recordReading).not.toHaveBeenCalled();
  // A programmatic restoration event does not dirty the personal record.
  fireEvent.scroll(pre,{target:{scrollTop:42}});await Promise.resolve();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
  fireEvent.scroll(pre,{target:{scrollTop:120}});await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());expect(api.saveDocumentTreeReading.mock.calls.at(-1)![0].state.detailScrollTop).toBe(120);
  fireEvent.click(screen.getByRole('button',{name:'关闭详情'}));await waitFor(()=>expect(api.saveDocumentTreeReading.mock.calls.at(-1)![0].state.selection.sectionId).toBeNull());expect(screen.queryByLabelText('来源章节原文')).not.toBeInTheDocument();outside.remove();
});
it('does not let legacy saved-version fallback reopen a deleted chapter owned by tree restoration',async()=>{
  setup(true);const old=data([]);old.readingPositions=[{sectionId:section.id,revision:section.revision,line:1}];api.snapshot.mockResolvedValue(old);
  render(()=><App/>);await screen.findByText(/原选中文件已删除或未读取/);expect(screen.queryByRole('region',{name:'文件读取详情'})).not.toBeInTheDocument();expect(screen.queryByLabelText('来源章节原文')).not.toBeInTheDocument();expect(api.readVersion).not.toHaveBeenCalled();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});

it('keeps the existing explicit saved-version restore path when no tree view owns reading',async()=>{
  const old=data([]);old.readingPositions=[{sectionId:section.id,revision:'older',line:1}];api.snapshot.mockResolvedValue(old);api.loadDocumentTreeLastView.mockResolvedValue({status:'missing',view:null,notice:null});api.readVersion.mockResolvedValue({...section,revision:'older'});
  render(()=><App/>);await screen.findByLabelText('来源章节原文');expect(api.readVersion).toHaveBeenCalledWith(section.id,'older');
});
