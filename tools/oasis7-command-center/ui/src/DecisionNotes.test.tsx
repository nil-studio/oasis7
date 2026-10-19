import {cleanup,fireEvent,render,screen,waitFor,within} from '@solidjs/testing-library';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import type {Snapshot,DecisionRecord} from './types';
const api=vi.hoisted(()=>({snapshot:vi.fn(),github:vi.fn(),saveDecision:vi.fn(),replaceDecision:vi.fn(),decisionHistory:vi.fn(),recordReading:vi.fn(),savePreferences:vi.fn()}));
vi.mock('./bridge',()=>({bridge:api,connected:()=>true,errorMessage:String,observedTime:(v:string)=>v,statusLabel:String,acceptsEnter:()=>false}));
vi.mock('./WorkOverview',()=>({default:()=>null}));
vi.mock('./LocalTools',()=>({default:()=>null}));
vi.mock('./DocumentInventory',()=>({default:()=>null}));
import App from './App';
const document={id:'formal',path:'doc/product/module/prd.md',title:'正式条款',content:'文档正文',revision:'rev',committedRevision:'head',dirty:false,line:1,factKind:'sourceFact',lifecycle:null} as const;
const record:DecisionRecord={id:'decision',content:'保留简单决定',reason:'减少手工维护',scope:'指挥中心',relatedSection:'formal',state:'confirmed',confirmedAt:'original-confirmation',formalUpdate:'verified',implementationSync:'pending'};
const data=(decisions:DecisionRecord[]=[]):Snapshot=>({project:{id:'project',path:'/repo',head:'head',branch:'main',gitCommonDir:'/repo/.git',environment:'local'},sources:[],sections:[document],decisions,preferences:{projectPath:'/repo',view:'direction',query:''},associations:[],readingPositions:[],git:{status:'',worktrees:''}});
beforeEach(()=>{Object.values(api).forEach(m=>m.mockReset());api.snapshot.mockResolvedValue(data());api.github.mockResolvedValue({state:'ready',repository:'nil-studio/oasis7',account:'reader',pulls:[{id:4919,title:'不应重复的 PR',url:'https://github.com/nil-studio/oasis7/pull/4919',state:'MERGED',headOid:'oid'}],coverage:'已读一页',error:null});api.savePreferences.mockResolvedValue(undefined);api.recordReading.mockResolvedValue(undefined);});
afterEach(cleanup);
it('saves the simple note with a formal document association and opens that document',async()=>{
 api.saveDecision.mockResolvedValue(record);api.snapshot.mockResolvedValueOnce(data()).mockResolvedValue(data([record]));
 render(()=><App/>);await screen.findByText('oasis7 · 本地项目');fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));
 fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:record.content}});fireEvent.input(screen.getByLabelText('理由'),{target:{value:record.reason}});fireEvent.input(screen.getByLabelText('适用范围'),{target:{value:record.scope}});fireEvent.change(screen.getByLabelText('正式文档关联'),{target:{value:'formal'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'记录已决定'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'记录已决定'}));
 await waitFor(()=>expect(api.saveDecision).toHaveBeenCalledWith({id:null,content:record.content,reason:record.reason,scope:record.scope,relatedSection:'formal',confirmed:true}));
 const link=await screen.findByRole('button',{name:'查看正式文档：正式条款'});fireEvent.click(link);expect(await screen.findByLabelText('来源详情')).toHaveTextContent('文档正文');
 expect(screen.queryByText(/实现同步：|正式规范：|决定的同步与历史/)).not.toBeInTheDocument();
});
it('retains confirmed note history and offers a replacement without synchronization fields',async()=>{
 api.snapshot.mockResolvedValue(data([record]));api.decisionHistory.mockResolvedValue([record]);api.replaceDecision.mockResolvedValue({...record,id:'new',supersedes:'decision'});
 render(()=><App/>);await screen.findByText('oasis7 · 本地项目');fireEvent.click(screen.getByRole('button',{name:/规范与决策/}));fireEvent.click(screen.getByRole('button',{name:record.content}));
 expect(screen.getByRole('button',{name:'保存草稿'})).toBeDisabled();fireEvent.input(screen.getByLabelText('决定内容'),{target:{value:'新的决定'}});await waitFor(()=>expect(screen.getByRole('button',{name:'建立新决定并替代原决定'})).toBeEnabled());fireEvent.click(screen.getByRole('button',{name:'建立新决定并替代原决定'}));await waitFor(()=>expect(api.replaceDecision).toHaveBeenCalledWith(expect.objectContaining({content:'新的决定',confirmed:true}),'decision'));
 await screen.findByText('新决定已保存，原决定与确认时间保留在历史。');fireEvent.click(screen.getByRole('button',{name:'查看修订历史'}));await waitFor(()=>expect(api.decisionHistory).toHaveBeenCalledWith('decision'));expect(screen.getByText(/原确认 original-confirmation/)).toBeInTheDocument();
});
it('keeps GitHub diagnostics and paging without a second PR list',async()=>{
 render(()=><App/>);await screen.findByText('oasis7 · 本地项目');fireEvent.click(screen.getByRole('button',{name:/开发执行/}));
 const summary=screen.getByText('Git 与 GitHub 来源检查 / 连接');fireEvent.click(summary);const diagnostics=summary.closest('details')!;
 expect(within(diagnostics).getByText('已读一页')).toBeInTheDocument();expect(within(diagnostics).getByRole('button',{name:'只读连接 / 刷新 GitHub'})).toBeInTheDocument();expect(within(diagnostics).getByRole('button',{name:/继续读取第/})).toBeInTheDocument();expect(within(diagnostics).queryByRole('button',{name:/不应重复的 PR/})).not.toBeInTheDocument();
});
