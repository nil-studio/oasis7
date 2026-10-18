// Generated from Rust DTOs by cargo run -p oasis7-command-center-core --example export_types
export type WorkRef = { "kind": "pullRequest", host: string, repository: string, number: string, } | { "kind": "session", namespace: string, projectId: string, environment: string, threadId: string, };
export type SourceFact = { source: WorkRef, revision: string | null, verifiedAt: string | null, eventAt: string | null, readAt: string, coverage: string, };
export type FactCell = { value: string, evidence: SourceFact, };
export type WorkGoal = { sectionId: string, title: string | null, revision: string | null, confirmedAt: string, };
export type WorkRun = { instanceId: string, turnId: string | null, state: string, owned: boolean, };
export type WorkItem = { reference: WorkRef, purpose: FactCell, result: FactCell, remaining: string, sourceUrl: string | null, goals: Array<WorkGoal>, checks: Array<Check>, checksSource?: PullChecksSource, runs: Array<WorkRun>, };
export type WorkSource = { kind: string, state: string, coverage: string, verifiedAt: string | null, error: string | null, };
export type WorkProjection = { schemaVersion: number, projectId: string, environment: string, readAt: string, sources: Array<WorkSource>, items: Array<WorkItem>, };
export type ContextBinding = { id: string, bindingVersion: number, root: string, storeId: string, projectId: string, registeredProjectId: string, name: string, directoryIdentity: string, };
export type ContextEntry = { id: string, revision: string, rawText: string, metadataJson: string, verificationState: string, };
export type ContextSummary = { id: string, title: string, kind: string, environment: string, state: string, revision: string, verification_state: string, summary: string, };
export type ContextQuery = { entries: Array<ContextSummary>, complete: boolean, total_matches: number | null, has_more: boolean, next_cursor: string | null, warnings: Array<string>, read_at: string, };
export type ContextSaveRequest = { bindingId: string, bindingVersion: number, id: string, rawText: string,
/**
 * None means create only, never unconditional overwrite.
 */
expectedRevision: string | null, actor: string, };
export type ContextSnapshotRestoreSource = { bindingId: string | null, bindingVersion: number | null, registeredProjectId: string, };
export type ContextRestoreIdentity = { storeId: string, projectId: string, registeredProjectId: string, };
export type ToolObservation = { tool_id: string, state: string, path: string | null, reason: string | null, observed_at: string, info: unknown, };
export type FileSignInput = { job_id: string, file_path: string, signer_id: string, grant_id: string, context: unknown, };
export type FileSignPrepared = { job_id: string, prepared_fingerprint: string, file_sha256: string, payload_sha256: string, operation_key: string, work_dir: string, result: unknown, };
export type FilePrepareRecovery = { operation_id: string, input: FileSignInput, prepared: FileSignPrepared, };
export type FilePrepareEffect = "none" | "unknown";
export type FilePrepareError = { message: string, effect: FilePrepareEffect, };
export type ToolJob = { id: string, fingerprint: string, resource: string, kind: string, state: string, observed_at: string, result: string | null, };
export type CliInstallation = { tool_id: string, release_id: string, path: string, sha256: string, terminal_path_configured: boolean, };
export type PrivateInputRef = { bindingId: string, bindingVersion: number, entryId: string, revision: string, scope: string, };
export type PrivateInputPreview = { source: PrivateInputRef, text: string, attachmentBytes: number, maxInputBytes: number, };
export type Project = { id: string, path: string, gitCommonDir: string, branch: string | null, head: string, environment: string, };
export type SourceStatus = { kind: string, state: string, lastAttempt: string | null, lastSuccess: string | null, sourceRevision: string | null, generation: string, coverage: string, error: string | null, };
export type DocumentLifecycle = {
/**
 * Original declaration, including code delimiters and notes.
 */
rawStatus?: string, category?: string,
/**
 * Compatibility projection used by older clients; not a normalized category.
 */
status: string | null, successorDeclaration: string | null, successorTargets: Array<string>, successorReason: string | null, issue: string | null, };
export type DocumentSection = { id: string, path: string, title: string, content: string, line: number, revision: string, committedRevision: string, dirty: boolean, factKind: string, lifecycle: DocumentLifecycle | null, };
export type DocumentNavigation = { relationships: DocumentPairings, labels: Array<string>, };
export type DocumentTreeRouteStep = { path: string | null, sourcePath: string | null, destination: string | null, group: string | null, duplicateIndex: number, duplicateCount: number, };
export type DocumentTreeReadingScope = { schemaVersion: number, projectId: string, baselineMode: string,
/**
 * Current source provenance, not personal record identity.
 */
baselineOid: string, };
export type DocumentTreeReadingKey = { schemaVersion: number, projectId: string, baselineMode: string, baselineOid: string, view: string, };
export type DocumentTreeReadingFilters = { query: string, readFilter: string, lifecycleFilter: string, kindFilter: string, pairingFilter: string, };
export type DocumentTreeReadingSelection = { route: Array<DocumentTreeRouteStep>, path: string, sectionPath: string | null, sectionId: string | null, sectionRevision: string | null, };
export type DocumentTreeReadingState = { filters: DocumentTreeReadingFilters, expandedRoutes: Array<Array<DocumentTreeRouteStep>>, parentRoute: Array<DocumentTreeRouteStep>, selection: DocumentTreeReadingSelection | null, treeScrollTop: number, detailScrollTop: number, };
export type DocumentTreeReadingSaveRequest = { key: DocumentTreeReadingKey, state: DocumentTreeReadingState, };
export type DocumentTreeReadingLoadResult = { schemaVersion: number, key: DocumentTreeReadingKey,
/**
 * missing / ready / corrupt / unsupported
 */
status: string, state: DocumentTreeReadingState | null, notice: string | null, sourceBaselineOid: string | null, resolvedSectionId: string | null, };
export type DocumentTreeLastView = { schemaVersion: number, status: string, view: string | null, notice: string | null, };
export type DocumentSemanticTreeQuery = { inventory: DocumentInventoryQuery, view: string, parentOccurrence: string, parentRoute?: Array<DocumentTreeRouteStep>, lifecycleFilter?: string, kindFilter?: string, pairingFilter?: string, };
export type DocumentSemanticTreeEntry = { occurrenceId: string, stableRoute: Array<DocumentTreeRouteStep>, label: string, path: string | null, kind: string, matchingFiles: number,
/**
 * Current-view historical placeholder evidence; at most 20 sorted source paths.
 */
historicalReferenceFiles: number, historicalReferencePaths: Array<string>, cycle: boolean,
/**
 * Cross-domain navigation is a reference leaf, never ownership.
 */
boundary: string | null, depthLimited: boolean, sourcePath: string | null, sourceLine: number | null, relationship: DocumentPairing | null, file: IndexedDocument | null, hasChildren: boolean, };
export type DocumentSemanticTreePage = { schemaVersion: number, projectId: string, snapshotId: string, baselineOid: string, generatedAt: string, view: string, parentOccurrence: string, breadcrumbs: Array<DocumentSemanticBreadcrumb>, parentRoute: Array<DocumentTreeRouteStep>, routeNotice: string | null, candidates: number, read: number, omitted: number, stale: boolean, error: string | null, eligibleFiles: number, matchingFiles: number, hiddenHistoryFiles: number, lifecycleFilter: string, kindFilter: string, pairingFilter: string, totalChildren: number, entries: Array<DocumentSemanticTreeEntry>, nextCursor: string | null, issues: Array<string>, };
export type DocumentSemanticBreadcrumb = { occurrenceId: string, stableRoute: Array<DocumentTreeRouteStep>, label: string, };
export type DocumentPairing = { declarationLine: number, destination: string, target: string | null,
/**
 * resolved / fragment_unchecked (legacy) / missing / missing_fragment / ambiguous / unread / outside_scope
 */
resolution: string, targetSection: string | null, targetRevision: string | null,
/**
 * Candidate path classification, not proof of authority or implementation.
 */
targetKind: string | null, };
export type DocumentPairings = { targets: Array<DocumentPairing>, issue: string | null, };
export type DocumentIdentity = { raw: string | null, declaredId: string | null, declarationLine: number | null, issue: string | null,
/**
 * Other read files declaring the same exact, case-sensitive ID.
 */
duplicateFiles: number,
/**
 * Stable path sample, at most 20; duplicate_files is the full count.
 */
duplicatePaths: Array<string>, };
export type DocumentAllocation = { declarationLine: number,
/**
 * At most 512 characters of the original Markdown row, never rendered as HTML.
 */
rawSample: string, upstream: DocumentPairings, design: DocumentPairings, };
export type DocumentAllocations = { rows: Array<DocumentAllocation>, issue: string | null, };
export type IndexedDocument = {
/**
 * Explicit file-header ID; None in older caches or unread files.
 */
identity: DocumentIdentity | null, fileKey: string, path: string, title: string, kind: string, state: string, reason: string | null, revision: string | null, bytes: number | null, dirty: boolean, lifecycle: DocumentLifecycle | null,
/**
 * None in pre-pairing caches; refresh is required before claiming absence.
 */
pairings: DocumentPairings | null,
/**
 * Explicit design-header upstream declarations; None means not yet parsed.
 */
upstream: DocumentPairings | null,
/**
 * Explicit body allocation tables; None in older caches/unread or non-design files.
 */
allocations: DocumentAllocations | null,
/**
 * Explicit navigation parsed from the original source; absent in older caches.
 */
navigation: DocumentNavigation | null, };
export type DocumentInventoryQuery = { schemaVersion: number, projectId: string, snapshotId: string | null, query: string,
/**
 * all / read / omitted
 */
readFilter: string, cursor: string | null, };
export type DocumentInventoryPage = { schemaVersion: number, projectId: string, snapshotId: string, baselineOid: string, generatedAt: string, candidates: number, read: number, omitted: number, readBytes: number, totalMatches: number, partial: boolean, stale: boolean, error: string | null, files: Array<IndexedDocument>, nextCursor: string | null, };
export type DocumentDirectoryQuery = { inventory: DocumentInventoryQuery,
/**
 * Empty for root; otherwise an exact repository-relative directory path.
 */
parentPath: string,
/**
 * Omitted preserves the original unfiltered query contract.
 */
lifecycleFilter?: string,
/**
 * Omitted keeps all eligible document kinds.
 */
kindFilter?: string,
/**
 * Omitted preserves all explicit pairing states.
 */
pairingFilter?: string, };
export type DocumentDirectoryEntry = { path: string, name: string,
/**
 * directory / prd / design / entry. No semantic ownership is inferred.
 */
kind: string, matchingFiles: number, depthLimited: boolean,
/**
 * Non-historical read source files with verified explicit links, only in current view.
 */
historicalReferenceFiles: number,
/**
 * Stable sample of at most 20 distinct source paths; not proof of authority.
 */
historicalReferencePaths: Array<string>, file: IndexedDocument | null, };
export type DocumentDirectoryPage = { schemaVersion: number, projectId: string, snapshotId: string, baselineOid: string, generatedAt: string, parentPath: string,
/**
 * Full scan coverage, independent of filters and this projection's kind scope.
 */
candidates: number, read: number, omitted: number, stale: boolean, error: string | null, eligibleFiles: number,
/**
 * Matching unique files across the full projection, not just this page/parent.
 */
matchingFiles: number,
/**
 * History matches excluded by the current filter, before pagination.
 */
hiddenHistoryFiles: number, lifecycleFilter: string, kindFilter: string, pairingFilter: string, totalChildren: number, entries: Array<DocumentDirectoryEntry>, nextCursor: string | null, };
export type Preferences = { projectPath: string | null, view: string, query: string, };
export type DecisionDraft = { id: string | null, content: string, reason: string, scope: string, relatedSection: string | null, confirmed: boolean, };
export type DecisionRecord = { id: string, content: string, reason: string, scope: string, relatedSection: string | null, state: string, confirmedAt: string | null, formalUpdate: string, implementationSync: string, };
export type ReadingPosition = { sectionId: string, revision: string, line: number, };
export type GitSnapshot = { status: string, worktrees: string, };
export type WorkAssociation = { workUrl: string, goalSectionIds: Array<string>, confirmedAt: string, };
export type Snapshot = { associations: Array<WorkAssociation>, project: Project | null, sources: Array<SourceStatus>, sections: Array<DocumentSection>, decisions: Array<DecisionRecord>, preferences: Preferences, readingPositions: Array<ReadingPosition>, git: GitSnapshot, };
export type GitHubSnapshot = { state: string, repository: string | null, account: string | null, observedAt: string | null, lastAttempt: string | null, coverage: string, error: string | null, pulls: Array<PullRequest>, };
export type PullRequest = { id: string, title: string, url: string, state: string, headOid: string, reviewDecision: string | null, checks: Array<Check>, checksSource?: PullChecksSource, };
export type Check = { name: string, state: string, testedOid: string | null, sourceOid?: string, url: string | null, };
export type PullChecksSource = { schemaVersion: number, state: string, observedAt: string | null, coverage: string, error: string | null, };
export type RuntimeTarget = { instanceId: string, threadId: string, turnId: string, };
export type RuntimeSession = { purpose: string, scope: string, model: string | null, basisRevision: string, threadId: string, instanceId: string, turnId: string | null, state: string, owned: boolean, backgroundState: string, restorableHistory: boolean, privateSources: Array<PrivateInputRef>, };
export type RuntimeOperation = { id: string, instanceId: string, threadId: string | null, turnId: string | null, method: string, state: string, fingerprint: string, createdAt: string, error: string | null, };
export type RuntimeQuestion = { id: string, header: string, question: string, options: Array<string>, isSecret: boolean, };
export type RuntimeRequest = { requestId: string, instanceId: string, threadId: string, turnId: string, method: string, state: string, summary: string, questions: Array<RuntimeQuestion>, allowedDecisions: Array<string>, };
export type RequestAnswer = { requestId: string, target: RuntimeTarget, decision: string | null, answers: { [key in string]?: Array<string> }, };
export type RuntimeSnapshot = { projectId: string, environment: string, enabled: boolean, experimental: boolean, state: string, version: string | null, instanceId: string | null, capabilities: Array<string>, coverage: string, error: string | null, sessions: Array<RuntimeSession>, requests: Array<RuntimeRequest>, operations: Array<RuntimeOperation>, logs: Array<string>, logsTruncated: boolean, exitWaiting: boolean, };
export type QuitStatus = { safe: boolean, active: number, unknown: number, pendingRequests: number, waiting: boolean, reason: string, };
export type SessionConfig = { purpose: string, scope: string, model: string | null, basisRevision: string, };
