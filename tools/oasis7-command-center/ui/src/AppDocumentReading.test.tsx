import { cleanup,fireEvent,render,screen,waitFor } from '@solidjs/testing-library';
import { afterEach,beforeEach,expect,it,vi } from 'vitest';
import App from './App';
import type { Snapshot,DocumentSection } from './types';
const api=vi.hoisted(()=>({search:vi.fn(),savePreferences:vi.fn(),snapshot:vi.fn(),github:vi.fn(),documentInventory:vi.fn(),documentSemanticTree:vi.fn(),loadDocumentTreeLastView:vi.fn(),loadDocumentTreeReading:vi.fn(),saveDocumentTreeReading:vi.fn(),recordReading:vi.fn(),readVersion:vi.fn()}));
vi.mock('./bridge',()=>({bridge:api,connected:()=>true,errorMessage:String,observedTime:()=>'',statusLabel:String,acceptsEnter:()=>false}));
vi.mock('./WorkOverview',()=>({default:()=>null}));
vi.mock('./LocalTools',()=>({default:()=>null}));
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
it('restores inline chapter scroll passively and saves accessibility scrolling without opening an overlay',async()=>{
  const outside=document.createElement('button');document.body.append(outside);outside.focus();render(()=><App/>);
  const body=await screen.findByRole('article',{name:'文档正文'});await waitFor(()=>expect(body.scrollTop).toBe(42));expect(body).toHaveTextContent('原文第 1 行');expect(outside).toHaveFocus();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();expect(api.recordReading).not.toHaveBeenCalled();
  fireEvent.scroll(body,{target:{scrollTop:42}});await Promise.resolve();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
  fireEvent.scroll(body,{target:{scrollTop:120}});await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());expect(api.saveDocumentTreeReading.mock.calls.at(-1)![0].state.detailScrollTop).toBe(120);
  fireEvent.click(screen.getByRole('button',{name:'返回文档树与列表'}));await waitFor(()=>expect(api.saveDocumentTreeReading.mock.calls.at(-1)![0].state.selection).toBeNull());expect(screen.queryByRole('article',{name:'文档正文'})).not.toBeInTheDocument();outside.remove();
});
it('does not let legacy saved-version fallback reopen a deleted chapter owned by tree restoration',async()=>{
  setup(true);const old=data([]);old.readingPositions=[{sectionId:section.id,revision:section.revision,line:1}];api.snapshot.mockResolvedValue(old);
  render(()=><App/>);await screen.findByText(/原选中文件已删除或未读取/);expect(screen.queryByRole('region',{name:'文件读取详情'})).not.toBeInTheDocument();expect(screen.queryByLabelText('来源章节原文')).not.toBeInTheDocument();expect(api.readVersion).not.toHaveBeenCalled();expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
});

it('keeps the existing explicit saved-version restore path when no tree view owns reading',async()=>{
  const old=data([]);old.readingPositions=[{sectionId:section.id,revision:'older',line:1}];api.snapshot.mockResolvedValue(old);api.loadDocumentTreeLastView.mockResolvedValue({status:'missing',view:null,notice:null});api.readVersion.mockResolvedValue({...section,revision:'older'});
  render(()=><App/>);await screen.findByLabelText('来源章节原文');expect(api.readVersion).toHaveBeenCalledWith(section.id,'older');
});


it('returns to the original external entry after following a body link to an unlisted chapter',async()=>{
  const first={...section,content:'[打开后继正文](../../../other.md#next)'};
  const next={...section,id:'other.md#next',path:'other.md',title:'仅正文可到达的章节',content:'后继正文内容'};
  api.snapshot.mockResolvedValue(data([first,next]));
  api.loadDocumentTreeLastView.mockResolvedValue({status:'missing',view:null,notice:null});
  api.loadDocumentTreeReading.mockResolvedValue({status:'missing',state:null,notice:null,resolvedSectionId:null});
  render(()=><App/>);
  const entry=await screen.findByRole('button',{name:'恢复章节'});
  entry.focus();fireEvent.click(entry);
  const link=await screen.findByRole('button',{name:'打开后继正文'});
  await waitFor(()=>expect(screen.getByLabelText('来源章节原文')).toHaveTextContent('打开后继正文'));
  link.focus();fireEvent.click(link);
  await waitFor(()=>expect(screen.getByLabelText('来源章节原文')).toHaveTextContent('后继正文内容'));
  expect(link).not.toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'仅正文可到达的章节'})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'关闭详情'}));
  await waitFor(()=>expect(entry).toHaveFocus());
  expect(screen.queryByLabelText('来源章节原文')).not.toBeInTheDocument();
  expect(api.recordReading.mock.calls.map(call=>call[0].id)).toEqual([first.id,next.id]);
});

it('retains an invalid saved chapter until the user explicitly chooses current content',async()=>{
 const restored=await api.loadDocumentTreeReading();api.loadDocumentTreeReading.mockResolvedValue({...restored,resolvedSectionId:null});
 render(()=><App/>);await screen.findByText('原阅读位置无法在当前文档恢复，请从目录明确选择当前章节。');expect(screen.getByRole('article',{name:'文档正文'})).not.toHaveTextContent('原文第 1 行');expect(api.saveDocumentTreeReading).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'恢复章节 · 第 1 行'}));expect(screen.getByRole('article',{name:'文档正文'})).toHaveTextContent('原文第 1 行');await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());
});

it('keeps the inline document position when following and closing an external source link',async()=>{
 const original={...section,content:'[关联设计](design.md)\n\n原始条款正文'};const target={...section,id:'design',path:'doc/product/module/design.md',title:'关联设计正文',content:'另一个文档的设计正文'};
 api.snapshot.mockResolvedValue(data([original,target]));render(()=><App/>);const body=await screen.findByRole('article',{name:'文档正文'});const link=screen.getByRole('button',{name:'关联设计'});
 fireEvent.click(link);await screen.findByRole('region',{name:'来源详情'});expect(screen.getByLabelText('来源章节原文')).toHaveTextContent('另一个文档的设计正文');
 fireEvent.scroll(body,{target:{scrollTop:120}});await waitFor(()=>expect(api.saveDocumentTreeReading).toHaveBeenCalled());expect(api.saveDocumentTreeReading.mock.calls.at(-1)![0].state.selection.sectionPath).toBe(section.path);
 fireEvent.click(screen.getByRole('button',{name:'关闭详情'}));await waitFor(()=>expect(link).toHaveFocus());expect(body).toHaveTextContent('原始条款正文');
});

it('labels results with the submitted query and ignores responses after clearing',async()=>{
 const snapshot=data([section]);snapshot.preferences.view='direction';api.snapshot.mockResolvedValue(snapshot);api.savePreferences.mockResolvedValue(undefined);api.search.mockResolvedValue([section]);
 render(()=><App/>);await screen.findByText('oasis7 · 本地项目');
 const input=screen.getByLabelText('搜索规范、标题、路径或条款 ID');fireEvent.input(input,{target:{value:'恢复'}});fireEvent.click(screen.getByRole('button',{name:'搜索',exact:true}));await screen.findByText('查询：恢复');
 fireEvent.input(input,{target:{value:'新查询'}});expect(screen.getByText('查询：恢复')).toBeInTheDocument();expect(screen.queryByText('查询：新查询')).not.toBeInTheDocument();
 let resolve!:(value:DocumentSection[])=>void;api.search.mockImplementation(()=>new Promise(r=>{resolve=r;}));fireEvent.click(screen.getByRole('button',{name:'搜索',exact:true}));fireEvent.click(screen.getByRole('button',{name:'返回当前视图'}));resolve([section]);await Promise.resolve();await Promise.resolve();expect(screen.queryByLabelText('文档检索结果')).not.toBeInTheDocument();
});
