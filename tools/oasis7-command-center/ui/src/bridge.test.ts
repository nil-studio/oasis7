import { beforeEach, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: mock.invoke }));
import { bridge } from './bridge';
beforeEach(() => mock.invoke.mockReset());
it('reads saved document revisions through finite commands', async () => {
 await bridge.readVersion('doc/core/prd.md#3','saved-sha');
 expect(mock.invoke).toHaveBeenLastCalledWith('read_version', {sectionId:'doc/core/prd.md#3',revision:'saved-sha'});
 for (const command of ['github','refreshGithub','readGithubWindow','dailyWork','saveDailyWork','workProjection','saveDecision','replaceDecision','associateWork']) expect(bridge).not.toHaveProperty(command);
});
it('opens a saved original Codex thread through navigation only',async()=>{
 const target='codex://threads/019ea241-685c-79b3-a0b0-97ac2aa9f7e2';await bridge.openSource(target);
 expect(mock.invoke).toHaveBeenLastCalledWith('open_source_target',{target});
});
it('bounds Chinese search and refreshes without an execution command', async () => {
  mock.invoke.mockResolvedValue([]);
  await bridge.search('合入');
  expect(mock.invoke).toHaveBeenLastCalledWith('search_rules',{query:'合入',limit:100});
  await bridge.refresh();
  expect(mock.invoke).toHaveBeenLastCalledWith('refresh_source',{});
  expect(mock.invoke.mock.calls.some(([name]) => /start|input|interrupt/.test(name))).toBe(false);
});

it('binds file inventory paging to an explicit schema, project, snapshot and filter', async () => {
  mock.invoke.mockResolvedValue(undefined);
  const query = {schemaVersion:1,projectId:'project',snapshotId:'snapshot',query:'设计',readFilter:'omitted',cursor:'cursor'};
  await bridge.documentInventory(query);
  expect(mock.invoke).toHaveBeenLastCalledWith('query_document_inventory', {query});
});

it('passes finite directory queries without filesystem or shell proxy arguments', async () => {
  const query = {parentPath:'doc',inventory:{schemaVersion:1,projectId:'project',snapshotId:'snapshot',query:'设计',readFilter:'all',cursor:null}};
  await bridge.documentDirectory(query);
  expect(mock.invoke).toHaveBeenLastCalledWith('query_document_directory',{query});
});

it('passes semantic tree occurrence and same-snapshot filters through one finite command', async () => {
  const query = {view:'product',parentOccurrence:'product/module/topic',lifecycleFilter:'current',kindFilter:'prd',pairingFilter:'missing',inventory:{schemaVersion:1,projectId:'project',snapshotId:'snapshot',query:'设计',readFilter:'all',cursor:'cursor'}};
  await bridge.documentSemanticTree(query);
  expect(mock.invoke).toHaveBeenLastCalledWith('query_document_semantic_tree',{query});
});

it('uses independent finite personal reading commands with explicit project and baseline scope',async()=>{
  const scope={schemaVersion:1,projectId:'project',baselineMode:'tracked-working-tree',baselineOid:'head'};const key={...scope,view:'product'};
  const state={filters:{query:'',readFilter:'all',lifecycleFilter:'current',kindFilter:'all',pairingFilter:'all'},expandedRoutes:[],parentRoute:[],selection:null,treeScrollTop:0,detailScrollTop:0};
  await bridge.loadDocumentTreeLastView(scope);expect(mock.invoke).toHaveBeenLastCalledWith('load_document_tree_last_view',{scope});
  await bridge.loadDocumentTreeReading(key);expect(mock.invoke).toHaveBeenLastCalledWith('load_document_tree_reading',{key});
  await bridge.saveDocumentTreeReading({key,state});expect(mock.invoke).toHaveBeenLastCalledWith('save_document_tree_reading',{request:{key,state}});
  await bridge.resetDocumentTreeReading(key);expect(mock.invoke).toHaveBeenLastCalledWith('reset_document_tree_reading',{key});
});
