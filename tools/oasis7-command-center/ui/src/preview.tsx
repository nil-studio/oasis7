// Development-only Vite entry. Not included in the packaged App build.
import { render } from 'solid-js/web';
import App from './App';
import { bridge } from './bridge';
import type { Snapshot } from './generated/core';
import './style.css';
const stamp='2026-10-18T14:00:00Z';
const project={id:'layout-demo',path:'/demo/oasis7',gitCommonDir:'/demo/oasis7/.git',head:'demo-only',branch:'demo/readable-workspace',environment:'演示'};
const section={id:'demo-document',path:'doc/core/prd.md',title:'本地文档预览',content:'预览当前工作树的文档。',line:1,revision:'demo-revision',committedRevision:'demo-revision',dirty:false,factKind:'source',lifecycle:null};
const snapshot:Snapshot={project,sections:[section],sources:[{kind:'local',state:'partial',lastAttempt:stamp,lastSuccess:stamp,sourceRevision:'demo-only',generation:'demo',coverage:'演示资料；非真实仓库',error:null}],preferences:{projectPath:project.path,view:'tools',query:''},readingPositions:[],git:{status:'演示资料，无真实 Git 操作',worktrees:'演示工作树'}};
bridge.snapshot=async()=>snapshot;
bridge.openSource=async()=>{throw new Error('演示模式不打开真实外部工作');};
bridge.recordReading=async()=>{};
render(()=><App preview/>,document.getElementById('root')!);
