# Oasis7 Command Center

This directory contains the macOS implementation introduced by [PR #4518](https://github.com/eng-cc/oasis7/pull/4518). The [product contract](../../doc/engineering/command-center/command-center.prd.md) and [system design](../../doc/engineering/command-center/command-center.design.md) define a cloud command center and a local companion. This README describes the code and commands in this directory; it does not establish that the cloud responsibilities have been implemented here.

The local companion owns device and worktree bindings, uncommitted document previews, private context stores, packaged CLI tools and signer operations. The Sites application owns cloud personal records and the cross-device work views. Remote pages open in the system browser and never inherit Tauri IPC. The local SQLite behavior documented below is the current implementation boundary, not a second cloud writer or an automatic upload contract; implementation evidence is tracked from [the system design](../../doc/engineering/command-center/command-center.design.md#cc-current-implementation).

The trusted local UI contains two entries: local tools and worktree document preview. It does not read GitHub PRs or maintain cloud work projections, personal questions, decisions or work relationships. The retired implementations and their IPC commands are removed, rather than hidden behind navigation.

Document search and chapter labels use the visible Markdown heading text: emphasis, link destinations, code delimiters and closing ATX markers are removed, and multiline Setext titles are joined with spaces. Original source text and line numbers remain available. Existing source-based section keys are retained so display normalization does not rebind personal reading records. This is an indexing correction within [the document-tree design](../../doc/engineering/command-center/document-tree.design.md); explicit file relationships and tree projections are described below; independent tree snapshots remain an implementation gap.

The local document preview also lists the tracked file inventory, including empty, deleted, invalid and over-budget files. Filter by path/title/declared document ID or read result, page at most 100 files, inspect per-file coverage and open matching indexed chapters. Counts describe unique files in the selected scan, not implementation completion. The inventory is published atomically with chapters/search under a separate versioned cache key; query cursors bind to its source digest and filters. Source changes invalidate old cursors; failed scans retain the old inventory with a stale notice. Missing/corrupt inventory can be rebuilt by refreshing local facts without deleting personal records. This remains a working-tree chapter-index inventory, not the planned independent tree snapshot/partial-attempt model.

## Project status and architecture

Start with the [project status](../../doc/engineering/command-center/README.md) for long-term direction, current goals, verified progress, recent changes and human attention. Use the [architecture index](../../doc/engineering/command-center/architecture/README.md) to find deployment, authority, persistence, access and recovery boundaries. The [continuous maintenance workflow](../../doc/engineering/command-center/command-center.manual.md) explains who refreshes these documents and when.

These remain repository documents, readable through the local preview when tracked and within its reported limits. Reading them does not refresh their underlying facts or configure background work.

## Run and package

Use the repository Rust toolchain, Node 22 or newer, and the macOS developer tools. From this directory:

```sh
npm --prefix ui ci
npm --prefix ui run desktop:dev
npm --prefix ui run desktop:build
```

The build produces a `.app` under `<Cargo target directory>/<target-triple>/release/bundle/macos/`; use the actual path printed by Tauri. App and bundled CLI use the same explicit target (`--target` or `-t`, defaulting to the host); universal/custom targets and runner target overrides are rejected before building. Its frontend is embedded; launching it does not require Node, Vite, a terminal backend, or the game stack. This is a local ad-hoc signed development build. Developer ID signing, notarization, public distribution, other CPU architectures and older macOS versions require separate verification. The configured deployment target is not evidence of validation on that system.

Start with **本机工具** for private libraries, backups, CLI and signer operations. Select **工作树预览** and choose a Git worktree to read tracked Markdown, HEAD, branch and dirty changes. Refresh only reads; it never fetches, changes branches, commits or launches a model. Closing the window retains the menu bar entry; quit still checks running or unknown ToolJobs.

## Sources and personal data

Current-stage sections come from `doc/core/prd.md`'s current delivery/P0 heading. Other goals and rules retain their source path, content version and committed baseline. Document identity does not establish implementation acceptance. The UI does not synthesize a completion percentage or turn long-term goals into current blockers.

The local index prioritizes the core PRD and workflow authority, then structural entries (`AGENTS.md`, document-directory `README.md` files and the project map), then product bodies/other `prd.md` files, then remaining bodies. Paths within each tier sort deterministically. Entries retain the same read limits and omission reporting; prioritization does not guarantee they all fit.

The bounded local index covers at most 3,000 tracked paths, 1 MiB per file and 16 MiB in total. It excludes untracked files and all symlinks (including document ancestors and targets inside the repository) and reports partial coverage. On the supported macOS/Unix host, document reads walk directory handles with no-follow opens, including the consistency recheck, so replacing a path with a link cannot redirect the read. Other platforms currently report `unsupported_confined_read` omissions instead of using an unsafe fallback. This protection starts at the opened selected project root. The source-open action rejects known linked paths but hands a path to the OS; it does not provide a cross-process file lock. The local source notice reports actual candidate/read/omitted counts and bytes, complete omission counts by reason, and at most 20 omitted path samples with the sample/total count. Deleted, inaccessible, symlink, non-regular, invalid UTF-8 and over-budget files are distinguished; empty files count as read. Actual reads are capped even when a file grows after its size check. A scan with no readable sections but known omissions remains partial rather than successfully empty. This is working-tree chapter-index coverage, not the planned file-level tree snapshot or a whole-repository completeness claim. Short Chinese queries use parameterized literal substring search; longer queries use SQLite FTS5 trigram. Document content is rendered as escaped text, with no raw HTML or remote media execution.

SQLite lives in the platform application-data directory (`com.oasis7.command-center`), outside the repository. Existing personal records, history, legacy runtime data and unknown formats remain in place. The companion no longer exposes personal decision, issue or relationship writes. It neither imports records into the cloud nor removes the old database. Do not upload the whole database: it can contain private material and device paths. Cloud migration and cutover are separate work under Issue #4983.

Device-only preview preferences and reading positions use a separate namespace from retired personal records. Source-cache rebuilding does not delete those records or private exclusions. Saved document versions remain readable after source changes. Concurrent worktree changes reject candidate publication and retain the previous facts; this bounded check does not lock the worktree or create an immutable committed snapshot.

Document identity blocks can declare lifecycle and successor information. Lifecycle details retain the original status text and separately show a normalized document category (proposed/draft/active/superseded/retired, other declared values, or unknown for an absent status; invalid/conflicting declarations carry a diagnostic without a guessed category). Notes and code delimiters remain visible; classification uses only the first complete token or leading code span. Older cached reading versions without this projection retain their original display until the source is reread. The client displays these as source facts, with their document revision and committed basis; an `active` declaration does not establish implementation acceptance. Explicit successor references are matched against the currently indexed repository documents and exact section identities. Missing, ambiguous, unsafe or incomplete declarations remain visible and do not enable a guessed jump. Saved historical text stays at its original version; any current successor declaration is labeled separately. The client does not infer authority from modification time, heading similarity or model memory, and does not rewrite formal documents.

## Navigation boundary

Local document opening is limited to indexed safe paths. Existing original-tool navigation accepts identity-only `codex://threads/<UUID>` and GitHub HTTPS links; appended Codex paths, query strings and fragments are rejected. These actions open the system handler without execution control. No remote page is loaded into the privileged WebView, and there is no localhost proxy, upload bridge, device pairing or remote command queue.

## Verification

```sh
env -u RUSTC_WRAPPER cargo test --locked -p oasis7-command-center-core
env -u RUSTC_WRAPPER cargo run --locked -p oasis7-command-center-core --example export_types
env -u RUSTC_WRAPPER cargo fmt --all -- --check
npm --prefix ui test
npm --prefix ui run build
env -u RUSTC_WRAPPER cargo clippy --locked --workspace --all-targets -- -D warnings
```

The independent workspace owns a single Cargo lockfile. Do not change `CARGO_TARGET_DIR`. Generated TypeScript contracts are committed and checked against Rust. [Command Center CI](../../.github/workflows/command-center.yml) runs core regressions, frontend tests/build and native host checks/build on macOS. The normal repository checks also continue to apply.

Single-run debug loading observations are retained in [the implementation PR](https://github.com/eng-cc/oasis7/pull/4533#issuecomment-6162418199); they are not startup guarantees or production benchmarks. Actual build/environment results belong in the implementation PR.

Desktop acceptance covers native worktree selection, dirty Markdown preview, Chinese search and composition, reading restoration, private exclusions, menu close/reopen and ToolJobs safe exit. Browser preview and unit tests only prove their actual scope; packaged native behavior, real signer operations and distribution signing require their own verification. Historical acceptance remains bound to its original build and environment.

## Local tools

The tools entry embeds private-library management through `oasis7_context::application`: choose an independent directory, create/connect, search, edit with the read revision, archive, read/restore Git history, and export/inspect/restore encrypted context snapshots. The simple editor retains unseen metadata. Save does not commit or push. Conflicts keep drafts. Ordinary indexes, cache fallback and history projections reject private roots in both directions.

`desktop:build` bundles a matching optional context CLI and fixed catalog. Install explicitly into the App's per-user version directory; the UI returns the actual absolute path and does not claim shell PATH is configured. Existing different files are preserved. Both CLI and signer expose `info --json`; unknown build values are null.

Existing compatible signer installs are observed via fixed protected paths and original caller UID. File preparation uses the original context/envelope and JobInput, then submit/inspect retains the original job. Independent verification requires the expected public key. Prepare and doctor are not worker authorization. General upgrades remain unsupported.

Apple-signing/admin-helper integration is paused at the user's request. No system registration, privileged installation, custody management or production changes occur through this increment. See [local tools contract](../../doc/engineering/command-center/local-tools.design.md) for the held scope and actual acceptance gaps.

Private-library editing, history, encrypted snapshots, CLI and signing follow the local-tools contract. Private records remain excluded from the document preview.

文件清单详情可查看文件头顶层列表的“配对系统设计”Markdown 链接、声明行及目标文件读取状态。普通引用不推断配对；带章节锚点的目标按同次已读内容定位；缺失、重复和未读取分别提示。旧缓存需刷新后解析；未跟踪、外部和不安全路径不沿链接读取。可打开唯一目标章节，关闭来源详情后返回原配对按钮，保留筛选和分页。旧配对缓存若缺少章节定位需刷新；设计文件头的精确“上游”字段可分组查看需求文档与设计引用，并跳转同版本章节；目标类型是路径分类，不代表需求已实现。此入口可按行查看明确“需求承接与分配表”章节的上游和本设计链接，并跳转同版本章节；两类显式导航树的详细边界见[文档树设计](../../doc/engineering/command-center/document-tree.design.md)。

文件清单还可按文件头顶层“文档 ID”的有效声明搜索（忽略大小写），并显示原声明行和同 ID 的其他已读文件。重复核对区分大小写，计数不受分页影响，路径样本最多 20 条；文件仍分别保留。缺声明、格式不支持/多声明和旧清单未解析分别提示。只覆盖同次已读文件；没有重复提示不证明全仓唯一，ID 不赋予权威或实现状态。旧缓存须刷新后支持 ID 搜索；正文身份章节和条款 ID 专用检索尚不在此切片范围。

正文承接表只识别精确章节及唯一“上游条款”/“本设计条款”列，普通表格和验证映射不推断关系。每文件 32 行、64 个链接、每行 4096 字节，原文样本最多 512 字符；超限、错误表头和无链接单元格显示不完整。两侧声明与文件头上游/配对分别展示，不证明承接完整或实现完成；旧清单需刷新后解析。

规范视图可切换“文档目录”，按清单路径逐层浏览 PRD、设计及结构入口，搜索保留命中文件的祖先目录。每层最多 100 项，可分页和用路径面包屑返回；选择文件复用覆盖、声明与章节详情，原文关闭后保留目录上下文。覆盖计数仍属完整扫描，目录匹配计数限 PRD/design/entry 且按文件去重；64 层目录预算会明确提示，深层文件仍可从文件清单定位。查询仅投影已发布清单，不新增磁盘/网络读取或语义归属。产品树和专业系统树使用单独的显式导航投影；两树支持个人阅读恢复；目录仍为逐层导航，独立树快照仍待实现。

目录默认仅隐藏明确 `superseded` / `retired` 类别，可按生命周期类别或完整原始状态声明精确筛选；隐藏历史计数与“显示历史匹配”保留已应用搜索范围，未知项不会被当作历史隐藏。文件清单继续覆盖全部纳入文件。生命周期不代表实现或发布状态；普通引用的历史占位和持久阅读偏好仍待实现。

目录隐藏历史时，会保留同一清单中被非明确历史已读文件通过已解析配对、上游或承接表显式引用的历史目标；占位显示原类别、不同来源文件总数及最多20条路径样本。普通引用/后继关系不在此范围，引用不证明主责或实现完成。

目录支持需求、设计、结构入口类型筛选，提交后与搜索/读取/生命周期共同应用；分页与显示历史沿用已应用类型。扫描及目录候选覆盖总数保持不变，跨类型显式引用仍可支撑历史占位。类型来自清单路径分类，不推断主责。

目录提供显式配对缺口筛选：区分未发现配对、尚未解析、目标/章节缺失、未读取、歧义、范围外与解析不完整。可与类型和生命周期组合，沿用已应用条件分页；普通引用和设计上游不算配对，未发现配对不证明设计或实现不存在。

规范视图提供“产品树”和“专业系统树”：产品树沿四模块入口表及根 PRD 的活跃专题导航，专业树沿工程模块矩阵、模块设计与明确导航节展开。反引号路径与明确目录入口受同次候选范围约束，不沿链接扩展读取；普通引用、配对和承接关系不变为树归属。每层最多100项，搜索保留祖先，多父位置分别展示而文件计数去重；循环/深度/解析缺口有提示。未被明确导航覆盖的文件可从未归属集合与文档目录查漏。旧清单需刷新后取得导航数据。

两树沿用类型、读取结果、生命周期与配对缺口筛选；分页、重试与显示历史使用已提交条件，项目或来源变化拒绝旧结果。方向上/下、Home/End 移动，方向右展开或进入子项，方向左折叠或回父项，Enter 打开文件；组合输入不触发导航。两树按工作树和视图保存已提交筛选、展开路由、文件/明确章节及滚动位置，重开恢复最近树；源变化按明确路径重投影，删除、歧义和章节摘要变化有提示。偏好与可重建清单分离，损坏或未来版本可显式重置；保存失败保留当前会话。窄窗口可在树和详情间返回。固定提交基线和独立树成功/部分尝试仍待实现，完整验收边界见文档树设计。

产品 PRD 详情还识别二级“文档身份”中的明确“配对产品 design / 配对产品设计”字段，显示产品设计与同版本章节跳转；不是按同名或普通正文推断。专业文件头配对语义保持，旧清单需刷新。
