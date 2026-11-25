# Read-only requirement trace queries

`requirement-trace.py` is a Python 3.9+ standard-library CLI and importable module.
It implements the bounded query tool in Issue #4681 against the trace syntax in
[the adopted specification](../doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md).
The specification was adopted in PR #4680 and this CLI was delivered in PR #4682. This tool is engineering
infrastructure: there is no player-facing behavior, CI gate, service, new task
entity, or new credential requirement. Source documents and task bodies remain
authoritative; the index can always be rebuilt. Product feature design and
implementation statuses follow the [product content contract](../doc/engineering/doc-governance/product-documentation-standard.design.md#feature-design-implementation-status); this CLI returns recorded relationships and context, not inferred feature status or acceptance verdicts.

## Commands

Place global options **before** the subcommand:

```sh
python3 scripts/requirement-trace.py --help
python3 scripts/requirement-trace.py --root . query O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6
python3 scripts/requirement-trace.py --root . --format json reverse --document doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md
python3 scripts/requirement-trace.py --root . --format json rebuild
python3 scripts/requirement-trace.py --root . --github-snapshot /tmp/oasis7-tasks.json reverse --task https://github.com/nil-studio/oasis7/issues/4681
```

No network is used by default. `--github OWNER/REPO` explicitly opts in to the
existing `gh` identity and read-only REST requests. It neither installs `gh` nor
signs in, requests permissions, reads credentials, or changes remote records.
Issue and PR endpoints are paged independently with `state=all`, 100 per page,
until a short page. PR entries returned by the Issue endpoint are not counted
twice. Closed Issues and closed/merged PRs are included. Only **bodies** are formal
task sources in v1; titles, comments, review discussions and Project fields are
not. URLs outside the selected repository are never followed automatically.

```sh
python3 scripts/requirement-trace.py --root . --github nil-studio/oasis7 --known-url https://github.com/nil-studio/oasis7/pull/4680 query O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6
python3 scripts/requirement-trace.py --root . --github nil-studio/oasis7 --cache /tmp/oasis7-trace-cache.json --refresh --format json rebuild
```

`--known-url` and online `reverse --task` trigger a fresh read, including direct
reads of named tasks after listing. Same-repository task links found in local
formal contexts are also read directly on a fresh fetch. Query-string URLs and
non-GitHub URLs are rejected as direct-read targets. Direct reads can succeed
when listing fails; their scope and result are recorded separately.

Exit status 0 means the query ran, **not** that the requirement is implemented or
its trace is complete. Input/source-format errors return 2. GitHub failure and
conflict diagnostics remain data in a successful query, so this is not a gate.

## Parsing and links

- Only complete, lowercase, hyphenated UUID v4 IDs following `Trace ID:` or
  `Trace refs:` establish formal relationships. Bare mentions never do.
- Fields are recognized at a paragraph/list-item start or in a Markdown table
  cell. Tables may instead use `Trace ID` / `Trace refs` column headings.
  Inline-code values and inline-code labels are supported. Multiple refs can be
  comma-, semicolon- or whitespace-separated. A declaration lists one ID.
- Whole inline-code fields, fenced code, all lines indented by four spaces or
  a tab, blockquotes, HTML comments,
  example/template/sample sections and descendants are excluded. Directories
  named `templates`, `examples`, or `fixtures`, and GitHub's named Issue/PR
  template files are excluded from formal interpretation. A filename merely
  containing `templates` is not excluded. A descriptive document-level title
  ending in “模板” does not exclude its formal explanatory prose; a standalone
  `# Template` / `# 示例` heading does. Four-space indentation can also denote
  ordinary nested Markdown lists, not just code; formal fields in those nested
  lists are currently unsupported and skipped. This is a bounded parser, not a
  general Markdown block parser. Unlabelled examples cannot always be
  recognized: unsupported field positions and lifecycle wording are flagged
  for human review, never promoted from substring search.
- Paragraph records preserve their section text; table records preserve their
  own row. This retains applicability, “not yet formed”, remaining obligations,
  and many-to-many scope rather than inventing a completion state. Lifecycle
  prose is returned with a manual-review diagnostic; successor IDs are not
  inferred as declarations of the old requirement.
- Exact link destinations, path fragments and explicit HTML anchors are kept.
  Relative local paths and ATX-heading/explicit anchors are checked against the
  current corpus. Heading slugs handle ordinary Markdown, Unicode and duplicate
  headings; this is a bounded parser, not GitHub's complete Markdown renderer.
  Reference-style links with explicit labels are supported. Complex Markdown,
  Setext heading anchors and unusual escapes require manual verification.
  An unrecognized anchor is reported within this parser's scope, not silently
  repaired. External/blob-version links remain unverified rather than falsely
  equated with the current checkout. Malformed URLs, uninspectable local paths
  and symlink targets produce per-link diagnostics without discarding the other
  relationships. Local path components are inspected without following symlinks,
  including symlink directories before a `..` component.

## Coverage, versions and diagnostics

The local source scans UTF-8 `.md` files, skipping dependency/build/VCS directories
listed in the output. Symlinks are not followed. Local read failures make coverage
partial. Each record carries its path, line, anchor, content SHA-256, detected
Git HEAD and `working_tree` label. A dirty or exported corpus is not represented
as committed content. `--revision` is an asserted provenance hint kept separately
from the detected HEAD, not a checkout command or version verification.

GitHub snapshots retain repository, task URL, state, update time, PR head/merge
metadata when returned, and fetch/start/end time. Different sources are not an
atomic view. REST listing avoids search-index caps/delay but can change during
pagination and only sees records permitted to the existing identity. Completion
means **only** that endpoint pagination ended in its stated scope. It never proves
global absence, visibility of private records, or current completeness of cached
data. Known-link failures are distinct from listing scope.

Duplicate formal declarations are reported separately from normal repeated refs.
No observed declaration produces `unresolved_reference`, with the locations and
scope for human verification; it never claims global `missing`. Query results
also group declarations, `.design.md` references, Issue/PR tasks, and other
document references; this filename/source grouping does not infer product
applicability. Zero query
results mean `not_found_in_observed_sources`. All coverage, stale/unknown cache
freshness, permission/rate-limit/timeout failures, repeated-page failures and
retry inputs remain visible. Raw `gh` stderr is not echoed, to avoid credential
leakage. Repeat with `--refresh`, inspect the specific source, or correct local
input; the tool never automatically fixes source text.

## Offline snapshots and cache

An offline snapshot is a JSON object shaped as follows (the empty records list
is deliberate; this is a format illustration, not a trace declaration):

```json
{
  "schema_version": 1,
  "repository": "nil-studio/oasis7",
  "fetched_at": "2026-10-14T00:00:00+00:00",
  "coverage": {
    "issues": {"complete": true, "pages": 1, "state_filter": "all"},
    "pull_requests": {"complete": false, "pages": 0, "errors": ["offline"]}
  },
  "records": []
}
```

Each record uses `kind` (`issue` or `pull_request`), `number`, `url`, `body`,
`state`, `updated_at`, and optional `title`, `merged_at`, `head_sha`. Only one
current record per task belongs in a snapshot; store historical snapshots in
separate files and query them separately. Omitted coverage is unknown, not
complete. Metadata in supplied snapshots is caller-provided evidence, not
independently authenticated GitHub provenance.

`--cache` must be an explicitly selected `.json` file outside the source root;
symlink targets and repository paths are refused. A pre-existing file must be a
recognizable snapshot before replacement. It is an optional derived GitHub
snapshot, not a source of truth or manually maintained ledger. Writes are atomic.
A cache hit does not contact GitHub unless `--refresh`, `--known-url`, or online
Task reverse lookup is requested. Freshness defaults to 24 hours (`--max-age`).
A stale hit is still readable, labelled stale, and has partial coverage; it does not silently claim to
be current. Failed/incomplete refresh keeps the previous file and records, and
adds the failed refresh coverage to the result. Initial partial fetches can be
cached with their partial scope. Rebuild always re-reads the local corpus and
prints a derived index to stdout; redirecting that output is the caller's choice.

## Separate full-corpus content checks

This Trace ID query remains read-only and non-gating. To inspect historical
professional design trace tables or core document structure, use the separate
content checkers from a real repository working tree:

```sh
python3 scripts/system-design-traceability-check.py --repo-root . --full-corpus
python3 scripts/doc-core-content-check.py --repo-root . --full-corpus
OASIS7_PRODUCT_DOC_BASE=<base-ref> OASIS7_PRODUCT_DOC_HEAD=<head-ref> ./scripts/doc-governance-check.sh --full-corpus
```

The professional inventory selects applicable `doc/**/*.design.md` without a
Git-diff or existing-trace-table prefilter. Product, fiction/novel, `.governance`
directories and the existing normative-overlay exclusions are outside that
professional selection. All remaining professional lifecycle values are
included. Core inventory covers both product documents and professional designs.
Module roots `doc/<module>/design.md` are also inventoried separately from topic
contracts. Every root path is reported. Pure module overviews are checked by core
content against their architecture/component/flow/navigation duties, without
imposing topic 2.1/11.1 mapping tables. A root with either standard mapping heading
adopts the complete traceability contract, even if its table is empty or malformed;
comment/code examples do not opt in. Counts distinguish topic contracts, module
overviews, and module contracts. Module roots are also checked incrementally; a
root that adopted the contract in the comparison base cannot evade it by deleting
or renaming mapping headings. Current mapping tables also retain the contract.

Product features and professional designs relate many-to-many through applicable
requirements and Trace IDs, never by matching file counts. The content checkers
validate recognizable exact upstream/local links, explicit Trace refs against
linked declarations, and positive product design-state links against the relevant
allocation relation. Existing paragraph/list/table-column/table-cell Trace fields,
inline-code labels/values and multiple refs are supported. AC and professional
intermediate clauses may reference the original declaration through exact
fragment links; they need not redeclare or mint another ID. Bounded traversal
rejects unresolved cyclic/over-depth chains and does not borrow identities from
unrelated neighboring sections. Without explicit Trace refs, a professional clause
keeps its own identity and direct exact product-clause links; ordinary professional
navigation is not recursively treated as product allocation. Multi-hop allocation
uses explicit refs to the same existing ID and exact links, without minting or
redeclaring IDs for historical clauses. A directly linked product AC with explicit
refs can still resolve its original requirement declaration.
A product-to-professional-to-design chain is
valid; only engineering-only relations need an explicit product N/A reason and
scope. Undesigned or partially covered features remain legitimate;
checks must not encourage invented designs. Design coverage and implementation
status are separate, so implementation gaps may remain when design is covered.
The scoped core checker also compares unchanged product PRDs against the
merge-base snapshot: newly invalid positive design links or newly missing
feature-allocation relations fail even if only the target design changed.
Pre-existing identical relation errors and unrelated legacy status-field gaps
are not promoted to blockers by that differential check. It is bounded to those
recognizable relation failures, not arbitrary semantic changes.
Actual semantic coverage and a complete feature inventory still require human
review. These content checks do not change the query CLI's non-gating behavior.

These two standalone full-corpus modes default to historical warnings and reject
`--base`, `--head` or `--worktree` combinations. Input, read, unsafe-path and
execution errors still fail. The aggregate shell entry first preserves scoped
strict checks, then runs separate historical warning checks; full-corpus output
never masks an incremental failure. See the
[maintenance instructions](../doc/engineering/doc-governance/documentation-governance.manual.md#40-全量历史内容与专业设计扫描)
for scope and usage. These checks do not infer design completeness, implementation
status or acceptance, and do not turn this query CLI into a CI gate.

## Tests and implementation boundaries

Run the independent standard-library test suite supplied with this change:

```sh
python3 scripts/requirement-trace.test.py
python3 -m py_compile scripts/requirement-trace.py scripts/requirement-trace.test.py
```
Tests use temporary Markdown repositories and fake `gh` responses, including
multiple pages, closed/merged records, inaccessible sources, exact ID boundaries,
cache failures and read-only assertions. These are fixture tests; they do not
claim real network or repository-wide integration coverage. No Rust/game code
changes or gameplay tests are involved. Python script and tests are the ownership
boundary; no shared CI selector, permission configuration or gate is modified.
