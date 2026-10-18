import type { DocumentInventoryQuery, DocumentInventoryPage, DocumentDirectoryQuery, DocumentDirectoryPage, DocumentSemanticTreeQuery, DocumentSemanticTreePage, DocumentTreeReadingKey, DocumentTreeReadingSaveRequest, DocumentTreeReadingLoadResult, DocumentTreeReadingScope, DocumentTreeLastView } from './generated/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { DecisionDraft, DecisionRecord, DocumentSection, GitHubSnapshot, Preferences, Snapshot, WorkAssociation, RuntimeSnapshot, RuntimeTarget, RequestAnswer, QuitStatus, SessionConfig } from './types';
import { listen } from '@tauri-apps/api/event';
export const connected = () => isTauri();
function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!connected()) return Promise.reject(new Error('浏览器预览未接入桌面核心。请从 App 打开真实项目。'));
  return invoke<T>(command, args);
}
// Explicit finite commands: no arbitrary method, shell, SQL or filesystem proxy.
export const bridge = {
  refreshWorkChecks: (number: string, ghPath: string | null) => call<GitHubSnapshot>('refresh_work_checks', {number,ghPath}),
  workProjection: () => call<import('./generated/core').WorkProjection>('query_work_projection'),
  documentDirectory: (query: DocumentDirectoryQuery) => call<DocumentDirectoryPage>('query_document_directory', { query }),
  documentSemanticTree: (query: DocumentSemanticTreeQuery) => call<DocumentSemanticTreePage>('query_document_semantic_tree', { query }),
  loadDocumentTreeReading: (key: DocumentTreeReadingKey) => call<DocumentTreeReadingLoadResult>('load_document_tree_reading', {key}),
  saveDocumentTreeReading: (request: DocumentTreeReadingSaveRequest) => call<void>('save_document_tree_reading', {request}),
  resetDocumentTreeReading: (key: DocumentTreeReadingKey) => call<void>('reset_document_tree_reading', {key}),
  loadDocumentTreeLastView: (scope: DocumentTreeReadingScope) => call<DocumentTreeLastView>('load_document_tree_last_view', {scope}),
  documentInventory: (query: DocumentInventoryQuery) => call<DocumentInventoryPage>('query_document_inventory', { query }),
  snapshot: () => call<Snapshot>('query_view'),
  openProject: (path: string) => call<Snapshot>('open_project', { path }),
  chooseProject: () => call<Snapshot | null>('choose_project'),
  refresh: () => call<Snapshot>('refresh_source'),
  search: (query: string) => call<DocumentSection[]>('search_rules', { query, limit: 100 }),
  saveDecision: (draft: DecisionDraft) => call<DecisionRecord>('save_decision_draft', { draft }),
  savePreferences: (preferences: Preferences) => call<void>('save_preference', { preferences }),
  recordReading: (s: DocumentSection) => call<void>('read_source', { sectionId: s.id, revision: s.revision, line: s.line }),
  rebuildCache: () => call<Snapshot>('rebuild_cache'),
  github: () => call<GitHubSnapshot>('get_github_snapshot'),
  refreshGithub: (ghPath: string | null) => call<GitHubSnapshot>('refresh_github', { ghPath }),
  openSource: (target: string) => call<void>('open_source_target', { target }),
  readVersion: (sectionId: string, revision: string) => call<DocumentSection | null>('read_version', { sectionId, revision }),
  associateWork: (workUrl: string, goalSectionIds: string[]) => call<WorkAssociation>('associate_work', { workUrl, goalSectionIds }),
};
export const runtimeBridge = {
  snapshot: () => call<RuntimeSnapshot>('get_runtime'),
  reconcile: (operationId: string) => call<RuntimeSnapshot>('reconcile_runtime_operation', { operationId }),
  enabled: (enabled: boolean) => call<RuntimeSnapshot>('set_runtime_enabled', { enabled }),
  connect: (binaryPath: string | null) => call<RuntimeSnapshot>('connect_runtime', { binaryPath }),
  create: (config: SessionConfig) => call<RuntimeSnapshot>('create_runtime_session', { config }),
  continue: (threadId: string) => call<RuntimeSnapshot>('continue_runtime_session', { threadId }),
  start: (threadId: string, input: string) => call<RuntimeSnapshot>('start_runtime_turn', { threadId, input }),
  steer: (target: RuntimeTarget, input: string) => call<RuntimeSnapshot>('steer_runtime_turn', { target, input }),
  interrupt: (target: RuntimeTarget) => call<RuntimeSnapshot>('interrupt_runtime_turn', { target }),
  answer: (reply: RequestAnswer) => call<RuntimeSnapshot>('answer_runtime_request', { reply }),
  exitStatus: () => call<QuitStatus>('get_exit_status'),
  waitExit: () => call<QuitStatus>('request_exit_wait'),
  stopExit: () => call<QuitStatus>('request_exit_stop'),
  background: () => call<QuitStatus>('cancel_exit_wait'),
};
export async function subscribeRuntime(invalidate: () => void, exitRequired: () => void): Promise<() => void> {
  if (!connected()) return () => {};
  const handlers = await Promise.allSettled([
    listen('command-center:runtime-invalidated', invalidate),
    listen('command-center:exit-required', exitRequired),
  ]);
  const stops = handlers.flatMap(result => result.status === 'fulfilled' ? [result.value] : []);
  const failed = handlers.find(result => result.status === 'rejected');
  if (failed?.status === 'rejected') { stops.forEach(stop => stop()); throw failed.reason; }
  return () => stops.forEach(unlisten => unlisten());
}
export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error && typeof error.message === 'string') return error.message;
  return String(error);
}
// Unstructured failures (old hosts, transport loss or interrupted work) never
// prove that preparing a job had no effects.
export function prepareFailureEffect(error: unknown): 'none' | 'unknown' {
  return typeof error === 'object' && error !== null && 'effect' in error && error.effect === 'none'
    && 'message' in error && typeof error.message === 'string' ? 'none' : 'unknown';
}
export function statusLabel(state: string): string {
  return ({ loading: '读取中', ready: '已读取', disconnected: '未接入', success: '已读取', empty: '读取成功 · 空', partial: '部分读取', stale: '已过期', error: '读取失败', unconnected: '未接入' } as Record<string,string>)[state] ?? state;
}
export function observedTime(value: string | null | undefined): string {
  if (!value) return '尚无成功读取';
  const date = /^\d+$/.test(value) ? new Date(Number(value) * 1000) : new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('zh-CN');
}
export function acceptsEnter(event: { key: string; isComposing: boolean; keyCode?: number }, composing: boolean): boolean {
  return event.key === 'Enter' && !event.isComposing && !composing && event.keyCode !== 229;
}

import type { ContextBinding, ContextEntry, ContextSaveRequest, ToolObservation, FileSignInput, FileSignPrepared, ToolJob } from './generated/core';
export type { ContextSummary, ContextQuery } from './generated/core';
import type { ContextQuery } from './generated/core';
const bound = (binding: ContextBinding) => ({ bindingId: binding.id, bindingVersion: Number(binding.bindingVersion) });
export const contextBridge = {
  info: () => call<unknown>('context_info'),
  reconcile: async (operationId: string, passphrase?: string) => {
    const secretHandle=passphrase ? await call<string>('secret_stage', { operationId, purpose:'context_reconcile', passphrase }) : null;
    return call<ToolJob>('context_reconcile', { operationId, secretHandle });
  },
  bindings: () => call<ContextBinding[]>('context_bindings'),
  select: (create: boolean, name: string) => call<ContextBinding | null>('context_choose_library', { create, name, operationId: crypto.randomUUID() }),
  list: (binding: ContextBinding, query = '', includeArchived = false) => call<ContextQuery>('context_list', { ...bound(binding), query, includeArchived }),
  get: (binding: ContextBinding, entryId: string) => call<ContextEntry>('context_get', { ...bound(binding), entryId }),
  save: (request: ContextSaveRequest, operationId: string) => call<{revision: string; persistence: string; changed: boolean}>('context_save', { request, operationId }),
  archive: (binding: ContextBinding, entryId: string, revision: string) => call<unknown>('context_archive', { ...bound(binding), entryId, revision, operationId: crypto.randomUUID() }),
  history: (binding: ContextBinding, entryId: string) => call<unknown>('context_history', { ...bound(binding), entryId }),
  restoreEntry: (binding: ContextBinding, entryId: string, commit: string, revision: string | null) => call<unknown>('context_restore_entry', { ...bound(binding), entryId, commit, revision, operationId: crypto.randomUUID() }),
  doctor: (binding: ContextBinding) => call<unknown>('context_doctor', bound(binding)),
  snapshot: async (purpose: 'export' | 'inspect' | 'restore', binding: ContextBinding | null, passphrase: string, directoryName = '', selection: string[] = []) => {
    const operationId = crypto.randomUUID();
    const command = `context_snapshot_${purpose}`;
    const secretHandle = await call<string>('secret_stage', { operationId, purpose: command, passphrase });
    return call<unknown>(command, { ...(binding ? bound(binding) : {bindingId:null,bindingVersion:null}), operationId, secretHandle, directoryName, selection });
  },
};
export const toolsBridge = {
  signer: () => call<ToolObservation>('tool_signer_observe'),
  jobs: () => call<ToolJob[]>('tool_jobs'),
  cli: () => call<import('./generated/core').CliInstallation | null>('tool_cli_observe'),
  reconcileCli: (operationId: string) => call<unknown>('tool_cli_reconcile', { operationId }),
  reconcilePrepare: (input: FileSignInput, operationId: string) => call<FileSignPrepared>('tool_file_reconcile', { input, operationId }),
  recoverPrepare: (operationId: string) => call<import('./generated/core').FilePrepareRecovery>('tool_file_recover', { operationId }),
  doctor: () => call<unknown>('tool_signer_doctor'),
  chooseFile: () => call<string | null>('tool_choose_file'),
  prepare: (input: FileSignInput, operationId: string) => call<FileSignPrepared>('tool_file_prepare', { input, operationId }),
  submit: (jobId: string, expectedPreparedFingerprint: string, operationId: string) => call<unknown>('tool_signer_submit', { jobId, expectedPreparedFingerprint, operationId }),
  inspect: (jobId: string, expectedPreparedFingerprint?: string) => call<unknown>('tool_signer_inspect', { jobId, expectedPreparedFingerprint: expectedPreparedFingerprint??null }),
  installCli: () => call<import('./generated/core').CliInstallation>('tool_cli_install', { operationId: crypto.randomUUID() }),
  verify: (filePath: string, context: unknown, expectedPublicKey: number[], signature: number[]) => call<boolean>('tool_file_verify', { filePath, context, expectedPublicKey, signature }),
};
