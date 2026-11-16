# testing 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/testing/prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 对应文件级索引: `doc/testing/prd.index.md`

## 1. 设计定位
`testing` 模块的 `design.md` 描述测试分层、验证策略、证据采集与发布门禁的总体设计。

## 2. 阅读顺序
1. `doc/testing/prd.md`
2. `doc/testing/design.md`
3. `doc/testing/prd.index.md`
5. 下钻 `ci/`、`governance/`、`launcher/`、`longrun/`、`performance/` 等专题目录

### 2.1 需求承接与分配表

本表承接 L4 工具链、受控浏览器回归和候选整包接收的有界条款；既有 QA infrastructure 方法、89 项依赖矩阵和各专业域义务继续保留。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | 将两 primary mode、适用入口、执行/恢复、经济、材料、QA 与公开说明接成同候选、同窗口的评估输入 | [候选整包接收](#release-candidate-evidence-reception) | 各领域原 authority；QA 汇总，producer 决定，LiveOps 同步 | 不新增 schema、发行阈值或审批；设计完成不产生当前 release verdict |
| [AC-ENTRY-CANDIDATE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-candidate-001) | 完整对应正例与混候选、局部/历史 green、缺证和对应不明反例分别判定 | [候选整包接收](#release-candidate-evidence-reception) | 原领域 oracle、总手册与发布证据模板 | 不以材料齐全或一个专题通过代签领域有效性、整体验收或阶段升级 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[入口根 SC-5](../product/player-entry-distribution/prd.md#sc-entry-005) | 仅接收候选组合中的两 mode 独立证据与 observer/blocked 边界；共享事实不等于共享入口验收 | [候选整包接收](#release-candidate-evidence-reception) | Viewer、正式 API 与原 MODE001/002 消费者 | 不替代根 SC-5 的全部模式、能力、失败恢复设计和实际验证 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[入口根 SC-6](../product/player-entry-distribution/prd.md#sc-entry-006) | 仅接收发行到运行身份链作为候选输入；每个受支持平台/入口的实际字节与 mode/backend 分别关联 | [候选整包接收](#release-candidate-evidence-reception) | Site、发行资产、Launcher 与原升级/恢复 authority | 不替代平台安装、升级首写、恢复或整个 SC-6；候选接收不升级根状态卡 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | 实际采样/执行与人工体验结论分开，低层证据不升级为真人意愿 | [L4 判断边界](#l4-judgment-boundary) | 原 RunResult/QA/producer 与领域 oracle | 不修复旧 runner 行为，也不以本地检查宣布 L4/L5 已执行 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | L4、评分、玩法评审、真人及发布材料保留各自证明范围和独立阻断 | [证据消费组合](#playability-evidence-consumer-reception) | 各原证据 owner / 产品与发行 authority | 不新增评分、自动 verdict、统一报告或发布权限 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | 每次结果绑定原来源、实际候选/入口/环境与观察窗口，不拼接别的尝试 | [执行记录与终态](#l4-result-record) | 原 SourceSceneRecord/RunResult 与 QA receiver | 字节和字段完整不证明世界结果或可玩性通过 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | 产品建议按原比较单位接收，错配或未知不抵消独立阻断 | [证据消费组合](#playability-evidence-consumer-reception) | producer/QA 及总纲、评分、真人与公开 claim 原 owner | 不将同评分组、一个入口或局部正例外推全部声明范围 |
| Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；[AC-14C](prd.md#browser-functional-regression) | 真实组件与可控依赖、确定性时序、隔离和分层证据。 | [浏览器功能回归层](#browser-functional-layer) | QA 维护 runner；Viewer 拥有状态/交互；CI owner 拥有选测接线。 | 产品适用性：N/A；reason=前端验证支撑；scope=fixture-functional。域正确性仍由原 owner 判定，不代签真实世界或 L4B/L5。 |
| [L4 构建与驱动](prd.md#l4-build-driver) | 默认 release 通过正式 UI 驱动；显式测试构建与观测能力单独选择。 | [构建与驱动选择](design.md#l4-build-selection) | Viewer build/driver；producer bundle 入口。 | 测试产物或参数不能代签 release 体验。 |
| [L4 Hosted 会话](prd.md#l4-hosted-session) | 实际执行 session 完成受支持登录/恢复，确认 player 关联并回收自有资源。 | [浏览器与玩家会话编排](design.md#l4-session-orchestration) | 既有 Hosted 登录、runtime session 与 agent-browser 生命周期合同。 | 不新增账户、认证、签名或跨会话凭据交接体系。 |
| [L4 执行结果](prd.md#l4-execution-result) | 每个 attempt 保留实际执行模式、阶段、阻断、停止和清理结果。 | [执行记录与终态](design.md#l4-result-record) | 既有 RunInput/RunResult 与 QA receiver；runner 负责落盘。 | 有结果文件不表示已完成实玩或通过。 |
| [L4 证据与判定](prd.md#l4-evidence-judgment) | 采样事实、权威结果和人工体验判定分开，失败尝试及未评状态保留。 | [证据与体验判定边界](design.md#l4-judgment-boundary) | gameplay/runtime oracle；producer 与 QA。 | 固定动作成功不证明自主游玩、continue、满分或 L5。 |
| [L4 入口兼容](prd.md#l4-entry-compatibility) | canonical/compat 两入口均保持所选 release/test 构建边界。 | [入口与构建兼容](design.md#l4-entry-profile) | Viewer entry compatibility PRD/design 与 dist copy。 | 不删除兼容入口或把测试 API 带回 release。 |
| [L4 scaffold 输出](prd.md#l4-scaffold-output) | 路径通过 argv 传入序列化器，JSON stdout 与 manifest 一致且保留目录保护。 | [Scaffold 序列化](design.md#l4-scaffold-serialization) | prepare-playability-l4-review.sh 与其 stdout/manifest 消费者。 | 离线 scaffold 成功不完成 L4A/L4B。 |

## 3. 设计结构
- 测试分层：`test_tier_required` / `test_tier_full` 的职责分工。
- 证据层：测试结果、失败签名、门禁与复审记录。
- 发布层：go/no-go、回归范围与阻断结论。
- 性能证据层：`performance-coverage-gap-matrix-2026-06-09.md` 统一当前 runtime/LLM observability、Viewer Web browser metrics、tier 与 report-only/blocking 边界；历史 native probe schema 不反向定义当前 Web harness。
- 好玩性证据层：`L1` automation、`L2` probe、`L3` telemetry/experiment、`L4A` synthetic review、`L4B` embodied-agent playtest 与 `L5` external signals 按证明强度递进；低层不替代高层，世界活动不等于玩家杠杆。
- 内部评审层：standard-role packet/card 收口，persona panel 只提供结构化假设并回流角色结论，不新增正式 `player` 角色或外部验证结论。

## 4. 集成点
- `testing-manual.md`
- `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md`
- `doc/playability_test_result/prd.md`
- `doc/core/prd.md`
- `doc/scripts/prd.md`
- `scripts/prepare-playability-l4-review.sh`：在一个 worktree 固定 L4 packet、role/persona cards、L4B card、可选校准 notes、summary 与命令。
- `scripts/run-playability-l4b-agent.sh`：现有 L4 driver 与采样入口；其构建、登录编排及结果判定缺口见 [L4 工具链设计](#l4-toolchain-design)，不能仅凭脚本名称认定已完成具身 agent 试玩。

<a id="browser-functional-layer"></a>
### 真实前端与可控依赖的浏览器功能层

Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；承接 [AC-14C / BROWSER-AC-1–5](prd.md#browser-functional-regression)。这是 S6 中显式标记依赖为合成的功能回归 profile，不另建测试分层或替代现有真实栈。

现有 Vitest/jsdom 验证组件和协议处理，tooling smoke 验证浏览器驱动接线，`player-visual-feedback-browser-smoke.mjs` 已用真实 shell 与固定 bridge/feed 做浏览器采证。新增目标是把真实 `viewer_app.jsx`、`legacy_core.js` 及其 PromptControl/feedback 模块与可控 HTTP/WebSocket 事件调度连接，让相同交互在相同交付顺序下可重复验证。业务逻辑仍来自生产模块；初始合成身份、传输与非试点渲染依赖的替换范围逐项写入报告。

数据流为场景声明→隔离浏览器与同候选测试构建→真实 UI 请求→受控传输交付→生产状态处理与 DOM→独立断言/原始产物。场景声明保留固定预期值和域合同链接，不能从被测状态函数生成预期。候选 Prompt Overrides 试点及事件、生命周期、只读探针和产物合同集中在[浏览器设计](manual/web-ui-playwright-closure-manual.design.md#browser-functional-fixture-contract)，避免在总览复制场景矩阵。

该 profile 现已登记到现有 planner/Viewer driver：`crates/oasis7_viewer/**` 选择既有 `viewer_js_required` 组，由 `scripts/ci-tests.sh` 在 release/visual-test 构建后执行统一 runner，并归档 7 天浏览器证据；没有新增平行路径表，也不删改 #5376 已有快速 contract。JS-required 仍先给低层反馈，fixture-functional 只提供所测浏览器的受控功能结果；影响可见表面的截图与模型视觉评审、真实集成和 JS-full 按原 S6 要求继续执行。CI 的 Node/agent-browser 版本与本地开发环境不同，判定以对应 CI 实际执行产物为准。

实现沿 #5387 分片：先连通隔离 test entry、owned browser 生命周期、首个 pending/成功/拒绝用例及失败产物，再补迟到/重连/重挂与选择合同。#5167 保留命令反馈缺陷，#4966 保留网页绑定/恢复，#5116 保留能力与展示边界；正式选定试点前由 QA 与对应 owner 对齐试点和具体 oracle，不能为测试通过改变这些合同。未来扩到网页入口或指挥中心须各自选择真实组件与适用 oracle，本次不声称已有覆盖。

### Release Web fixture 生命周期

发行 Web QA 的 source 路径先构建 visual-test Web、signed fixture executable，以及 `run-launcher-stack.sh --chain-disable` 消费的 provider probe、launcher 和 Viewer。全部成功后才启动 fixture 的有界运行窗口，再由 stack 复用本次构建的同候选可执行文件；任何构建失败都不启动 fixture。该顺序将冷构建与运行/交互超时分开，不延长 fixture 期限、不修改正式认证，也不将缓存中未知来源的 binary 当作构建证明。

## 5. 专题导航
- CI 与覆盖进入 `ci/`
- 线上/长时验证进入 `longrun/`
- 发行与治理验证进入 `governance/`、`launcher/`

<a id="qa-infrastructure-evidence-method"></a>
### 基础设施证据的组合验证方法

The [QA receiver contract](prd.md#qa-infrastructure-receiver) is the stable entry for combining product obligations, professional contracts, scenes, and test-tier evidence. For each scene/cell/specimen, freeze the exact clause fragment and source identity, candidate/base/tested tree, world/branch/root/window/configuration, applicable domain owner, execution state, dependency/topology and provider identity, ordered phases, artifact locator/bytes/digest, oracle, verdict, and residual proof boundary. Keep `planned`, `located`, `implemented`, and `executed` as independent state fields.

The verification path is source clause → named scene and phase → actual environment/dependency → professional oracle → raw evidence artifact and readback → tier-specific verdict.

First bind the candidate and source; then enumerate required cases/cells; capture a no-mutation baseline; validate artifact identity and bytes; run structural checks separately from canonical, trust, semantic, and cross-consumer checks; compare same-candidate before/outage/fresh/recovery/retry/replay observations; and finally check that the selected tier's real environment is present.

Missing mandatory identity, phase, cell, proof, archive readback, or environment is blocked/unverified. A contradictory observed invariant is failed with its exact signature. A shape-only pass remains shape-verified. Pass applies only to the declared scope when all its assertions and required environment are observed. Archive failures and successes append-only. This method does not turn a fixture, a link, a schema, or a previous candidate's pass into current execution or release evidence.

The root QA scene families are `V-FINALITY`, `V-RECOVERY`, `V-EXECUTION`, `V-INDUSTRIAL`, `V-OUTAGE`, `V-WORLD-SCOPE`, and `V-CONSUMER`; their source and oracle mapping is in the [testing PRD](prd.md#qa-infrastructure-scene-families). They retain all distinct product SC/narrative, DCS/DWE REQ/AC, P2P/runtime DC/DE, and GWSC 17/10/8 axes. Gameplay owns industrial quantities and W/window meaning; Runtime/WASM own execution and migration semantics; P2P owns finality and topology proof; Ops establishes the observed environment; Agent/Viewer own consumer observations; QA joins evidence without replacing those authorities.

<a id="qa-infrastructure-89-consumption-matrix"></a>
### Frozen 89-object dependency-consumption and evidence-retention matrix

This matrix is per frozen object, not a whole-file rewrite list. It preserves the exact 89 input paths from the admitted dependency inventory. Five of those objects receive bounded QA edits in this slice: the GWSC PRD, Playwright manual, Agent-browser manual, testing PRD, and `testing-manual.md`. `doc/testing/design.md` and the Playwright design companion are method documents outside the 89-object denominator. Every dated or candidate-bound artifact remains at its original path; it is eligible as current evidence only after its own source/candidate/window identity and bytes are read back. A locator, template, historical result, or neighboring object's status cannot close another object's gap.

| Frozen object | QA receiver consumption | Retention, owner, and unresolved proof |
| --- | --- | --- |
| `doc/playability_test_result/topics/industrial-onboarding-required-tier-cards-2026-03-15.md` | `V-INDUSTRIAL`; retain the onboarding steps and evidence questions as the gameplay sample input. | Preserve its date, candidate, and verdict; gameplay owns experience interpretation. A current same-candidate execution is still required for current claims. |
| `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md` | `V-FINALITY` and `V-RECOVERY`; use benchmark dimensions only to select topology/scale context. | Keep benchmark design as a reference, not a run result; Ops/P2P own actual environment and finality evidence. |
| `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md` | `V-FINALITY`; use the benchmark acceptance context when a chain-scale claim is in scope. | Retain its distinct PRD obligations; QA still needs current candidate measurements and Ops provenance. |
| `doc/testing/evidence/README.md` | Evidence index used to locate candidate-specific raw artifacts across the receiver scenes. | Preserve as navigation only; each artifact must independently satisfy identity, digest, window, and owner checks. |
| `doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md` | `V-FINALITY`/`V-RECOVERY`; historical shared-devnet provenance can explain an older sample's origin. | Preserve date and limitation; Ops must supply same-candidate topology/provenance before current use. |
| `doc/testing/evidence/network-tier-signer-truth-binding-2026-06-05.md` | `V-FINALITY`; signer identity and tier binding context. | Retain historical signer binding evidence; P2P/Ops must read back the current governed signer set and threshold. |
| `doc/testing/evidence/p2p-public-testnet-faucet-service-2026-05-19.md` | `V-RECOVERY` only when the selected scenario depends on faucet service or funding availability. | Preserve as scoped service evidence; Ops must establish current service/dependency. It does not prove consensus or world effects. |
| `doc/testing/evidence/public-testnet-api-viewer-projection-2026-07-05.json` | `V-CONSUMER`; historical envelope joining API and Viewer projection. | Retain original JSON and candidate identity; Viewer/runtime must provide a current same-window readback for parity claims. |
| `doc/testing/evidence/public-testnet-api-viewer-projection-2026-07-05/api-projection.json` | `V-CONSUMER`; API-side fields for the dated projection sample. | Keep bytes/digest with the parent envelope; current canonical API oracle remains required. |
| `doc/testing/evidence/public-testnet-api-viewer-projection-2026-07-05/chain-status-samples.json` | `V-CONSUMER`/`V-FINALITY`; dated chain-status observations. | Preserve as historical samples; Ops/runtime must bind new samples to current candidate and window. |
| `doc/testing/evidence/public-testnet-api-viewer-projection-2026-07-05/viewer-projection.json` | `V-CONSUMER`; Viewer-side fields for the dated projection sample. | Keep with parent envelope; Viewer owner must supply current surface readback, not inferred parity. |
| `doc/testing/evidence/public-testnet-claims-boundary-review-2026-05-21.md` | Tier/claim boundary reference for `V-FINALITY` and `V-CONSUMER`. | Retain its reviewed date and boundaries; release owner must reassess current candidate before external claims. |
| `doc/testing/evidence/public-testnet-claims-boundary-review-2026-07-06.md` | Latest dated claims-boundary context for `V-CONSUMER`/release-scoped scenes. | Preserve as an independent dated review; it does not supply current same-window operational evidence. |
| `doc/testing/evidence/public-testnet-current-required-lanes-2026-07-03.md` | `V-FINALITY`/`V-RECOVERY`; identify required operational lane names and their historical state. | Keep as a dated lane record; Ops must provide current same-window lane results. |
| `doc/testing/evidence/public-testnet-current-required-lanes-2026-07-03.tsv` | Machine-readable counterpart for the dated lane record. | Retain exact rows and digest; schema/TSV presence is not a passing lane or current readback. |
| `doc/testing/evidence/public-testnet-ecs-freshness-audit-2026-05-22.md` | `V-FINALITY`/`V-RECOVERY`; freshness audit history. | Preserve original freshness window; Ops must observe current service/readiness and peer head. |
| `doc/testing/evidence/public-testnet-faucet-guard-ready-2026-07-05.md` | `V-RECOVERY` only for scenarios requiring faucet guard behavior. | Retain scoped dated result; does not establish current faucet health or unrelated finality. |
| `doc/testing/evidence/public-testnet-five-node-inventory-2026-06-23.md` | `V-FINALITY`/`V-RECOVERY`; topology inventory source. | Preserve inventory identity/date; Ops must verify actual current nodes, roles, and connectivity. |
| `doc/testing/evidence/public-testnet-governance-public-signers-2026-06-05.json` | `V-FINALITY`; historical governed public signer set. | Keep immutable signer bytes; P2P/Ops must bind actual current signer membership and proof. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt` | `V-FINALITY`/`V-RECOVERY`; peer bootstrap input for the dated governed candidate. | Preserve exact list; Ops must capture current peer discovery and heads. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json` | `V-RECOVERY`; dated governed bootstrap bundle envelope. | Retain envelope and linked bytes; state-sync verification must independently validate closure and candidate identity. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-genesis-2026-06-06.json` | `V-WORLD-SCOPE`/`V-RECOVERY`; genesis/world identity of the dated bootstrap. | Preserve exact identity; runtime/Ops must compare against the current target world, not reuse by filename. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-manifest-2026-06-06.json` | `V-RECOVERY`; manifest-to-artifact binding for the dated bootstrap. | Retain exact manifest bytes; missing/mismatched current artifacts remain blocked. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-topology-2026-06-06.md` | `V-FINALITY`; dated node/role/topology description. | Preserve the historical topology; Ops must capture current roles and actual dependency classification. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json` | `V-FINALITY`; dated validator membership input. | Retain registry digest; P2P/Ops must prove the live governed set and threshold. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-validator-triad-bootstrap-peers-2026-09-15.txt` | `V-FINALITY`/`V-RECOVERY`; newer dated triad peer seed. | Preserve as its own candidate input; current Ops readback must prove actual peer use and heads. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world/journal.json` | `V-EXECUTION`/`V-RECOVERY`; dated world history sample. | Retain raw journal and digest; runtime must bind replay/readback to the current world and source. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world/journal.segments.json` | `V-EXECUTION`/`V-RECOVERY`; journal segment index for that world. | Preserve segment boundaries and identity; current replay/closure requires actual segment bytes. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world/module_registry.json` | `V-EXECUTION`; module registry state of the dated world. | Retain exact registry; runtime/WASM must read back candidate-compatible module/version state. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world/snapshot.json` | `V-RECOVERY`; dated snapshot state. | Preserve original snapshot; runtime must verify root/world and current restore outcome. |
| `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world/snapshot.manifest.json` | `V-RECOVERY`; snapshot-to-artifact manifest. | Keep manifest and referenced digests; absent/mismatched closure is a blocker, not a template fill. |
| `doc/testing/evidence/public-testnet-governed-reset-policy-announcement-2026-07-03.md` | `V-RECOVERY`; records announced reset policy boundaries. | Preserve announcement; Ops must prove actual governed reset execution and unchanged confirmed history. |
| `doc/testing/evidence/public-testnet-live-candidate-bootstrap-peers-2026-05-22.txt` | `V-FINALITY`/`V-RECOVERY`; historical live-candidate peer seeds. | Retain exact dated peers; no current connectivity or topology claim without Ops observation. |
| `doc/testing/evidence/public-testnet-live-candidate-bundle-2026-05-22.json` | `V-RECOVERY`; old live-candidate bundle envelope. | Preserve envelope and source candidate; closure/readback must be repeated for a current candidate. |
| `doc/testing/evidence/public-testnet-live-candidate-endpoint-deploy-2026-05-19.md` | `V-FINALITY`; dated endpoint deployment context. | Keep deployment history; Ops must establish current endpoint route and actual services. |
| `doc/testing/evidence/public-testnet-live-candidate-lanes-2026-05-22.tsv` | `V-FINALITY`/`V-RECOVERY`; old candidate lane rows. | Retain raw TSV; all current lane outcomes and evidence refs remain to be captured. |
| `doc/testing/evidence/public-testnet-live-candidate-manifest-2026-05-22.json` | `V-RECOVERY`; manifest identity of the old candidate. | Preserve digest; cannot bind a different candidate or prove current artifact availability. |
| `doc/testing/evidence/public-testnet-liveops-public-signers-2026-06-05.json` | `V-FINALITY`; LiveOps signer material reference. | Retain immutable file; P2P/Ops must prove signer use and governance state in the selected window. |
| `doc/testing/evidence/public-testnet-provider-resource-provenance-2026-07-05.md` | `V-INDUSTRIAL`/`V-OUTAGE`; provider-resource source and dependency context. | Preserve its provenance window; Ops must identify which dependency actually gates authority versus read paths. |
| `doc/testing/evidence/public-testnet-public-surface-freshness-2026-07-03.md` | `V-CONSUMER`; historical public-surface freshness observation. | Retain sampled time and endpoint; current Viewer/API freshness requires a same-window re-read. |
| `doc/testing/evidence/public-testnet-resource-delta-replay-2026-07-05.md` | `V-INDUSTRIAL`; resource delta/replay evidence context. | Preserve raw quantities and candidate; gameplay/runtime must revalidate conservation and once-only effect. |
| `doc/testing/evidence/public-testnet-runtime-world-resource-closure-2026-07-05.md` | `V-INDUSTRIAL`/`V-RECOVERY`; resource-to-world closure evidence. | Keep its exact closure scope; it does not prove current roots, windows, or all industrial transitions. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05.json` | `V-CONSUMER`/`V-WORLD-SCOPE`; hosted entry identity envelope. | Retain the dated same-world association; current world identity and consumer receipt continuity still need readback. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/chain-status-sample.json` | `V-FINALITY`/`V-CONSUMER`; chain-side sample for that entry. | Preserve bytes/digest; Ops/runtime must supply a current same-window status sample. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/hosted-entry.json` | `V-WORLD-SCOPE`; hosted entry world association. | Retain exact association; missing/mismatched target identity must fail closed. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/launcher-config.json` | `V-CONSUMER`; launcher configuration of the dated entry. | Preserve configuration identity; current candidate config must be separately captured and joined. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/pure-api-config.json` | `V-CONSUMER`; pure API configuration of the dated entry. | Keep exact settings; active-provider pure API parity needs current candidate and provider provenance. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/pure-api-snapshot.json` | `V-CONSUMER`; dated pure API world snapshot. | Retain snapshot/root identity; runtime must compare current authoritative state, not only shape. |
| `doc/testing/evidence/public-testnet-same-world-hosted-entry-2026-07-05/viewer-config.json` | `V-CONSUMER`; Viewer configuration of the dated entry. | Preserve exact config; Viewer must provide current read-only projection and surface evidence. |
| `doc/testing/evidence/pure-api-shared-player-gameplay-parity-2026-04-28.md` | `V-CONSUMER`; historical parity method/result reference. | Retain its date and limit; no current parity without same-candidate active-provider API plus surface observations. |
| `doc/testing/evidence/shared-network-shared-devnet-follow-up-promotion-record-2026-03-24.md` | `V-FINALITY`/`V-RECOVERY`; promotion decision history. | Preserve decision provenance; current promotion/readiness remains an Ops/release evidence question. |
| `doc/testing/evidence/software-safe-primary-web-entry-evidence-2026-04-07.md` | `V-CONSUMER`; historical Viewer entry observation. | Retain as bounded historical evidence; it cannot prove current headed desktop+narrow behavior. |
| `doc/testing/evidence/viewer-wasm-only-runtime-proof-2026-05-13.md` | `V-EXECUTION`/`V-CONSUMER`; WASM-only execution evidence. | Preserve its exact runtime scope; it is not provider-backed, full-browser, or consumer-parity proof. |
| `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md` | QA semantic authority for 17 cases, 10 runtime groups, 8 cells and their receiver oracles. | Read-only in this slice; preserve all design anchors and limitations. QA links only, does not redefine runtime/P2P/gameplay semantics. |
| `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md` | `V-RECOVERY`/`V-EXECUTION`/`V-OUTAGE`; §§2/6 now receive QA source/tier record and preserve 17/10/8 identities. | QA-owned bounded sections; test implementation, topology, real provider, and release evidence remain unexecuted here. |
| `doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md` | `V-FINALITY`/`V-RECOVERY`; soak and fault-selection source. | Retain its duration/topology authority; Ops/P2P own actual runs, and a short or proxy run cannot claim endurance. |
| `doc/testing/longrun/s10-five-node-real-game-soak.prd.md` | `V-RECOVERY`/`V-CONSUMER`; five-node integration/release source. | Preserve same-window/API projection requirements; current S10 execution remains a separate evidence obligation. |
| `doc/testing/manual/public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md` | `V-FINALITY`/`V-RECOVERY`; operational bootstrap and environment procedure. | Keep Ops procedure unchanged; actual host/bootstrap/peer proofs require a separately authorized run. |
| `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md` | `V-CONSUMER`; evidence boundary links visible action/read-only oracle and artifact joins. | QA-owned limited evidence section; actual browser/provider execution remains absent from this authoring slice. |
| `doc/testing/manual/web-ui-playwright-closure-manual.manual.md` | `V-INDUSTRIAL`/`V-EXECUTION`/`V-RECOVERY`/`V-CONSUMER`; PWT-003/005/006/007 capture contracts. | QA-owned planned rows; preserve PWT-001 and dated PWT-004 status; no planned case is an executed pass. |
| `doc/testing/prd.md` | QA receiver root, seven scene families, tier boundary, semantic equal-version expected negative, PRD traceability. | QA-owned §§2/6; scope states expected design and proof limits, not implementation or release readiness. |
| `doc/testing/templates/mainnet-bootstrap.example.txt` | `V-FINALITY`/`V-RECOVERY`; bootstrap input shape reference only. | Preserve example; copy to candidate-owned evidence before use and prove actual peers/roles independently. |
| `doc/testing/templates/mainnet-genesis.example.json` | `V-WORLD-SCOPE`/`V-RECOVERY`; genesis example shape. | Template is not a current genesis; Ops/runtime must bind real genesis bytes and root. |
| `doc/testing/templates/network-tier-mainnet.example.json` | `V-FINALITY`; mainnet tier envelope example. | Keep as template; no network tier is passed by validating this example. |
| `doc/testing/templates/network-tier-public-testnet-rehearsal.example.json` | `V-FINALITY`/`V-RECOVERY`; rehearsal tier example. | Preserve example status; use actual rehearsal candidate and same-window evidence for any claim. |
| `doc/testing/templates/network-tier-public-testnet.example.json` | `V-FINALITY`/`V-RECOVERY`; public-testnet tier example. | Template only; release/Ops lane evidence and claims boundary remain required. |
| `doc/testing/templates/public-testnet-genesis.example.json` | `V-WORLD-SCOPE`/`V-RECOVERY`; public-testnet genesis example. | Never treat sample identity as a live world; bind actual genesis and manifest. |
| `doc/testing/templates/public-testnet-rehearsal-bootstrap.example.txt` | `V-FINALITY`/`V-RECOVERY`; rehearsal peer bootstrap example. | Preserve the sample; actual peer discovery/topology belongs to Ops evidence. |
| `doc/testing/templates/public-testnet-rehearsal-genesis.example.json` | `V-WORLD-SCOPE`; rehearsal genesis example. | Template bytes are not world identity proof; actual candidate must provide its genesis readback. |
| `doc/testing/templates/public-testnet-skeleton-evidence.example.md` | All applicable scene families only as a field-shape aid. | Skeleton is never a run receipt; every field must be backed by candidate-bound raw evidence. |
| `doc/testing/templates/public-testnet-validator-pair-rebuild-evidence-v1.json` | `V-FINALITY`/`V-RECOVERY`; validator-pair rebuild envelope shape. | Keep schema/example distinct from proof; Ops must retain actual rebuild logs and readback. |
| `doc/testing/templates/shared-network-exit-decision-template.md` | `V-RECOVERY`; exit-decision record shape. | Template is not an exit decision; release/Ops owners retain an actual signed/current decision. |
| `doc/testing/templates/shared-network-incident-review-template.md` | `V-RECOVERY`/`V-OUTAGE`; post-incident review shape. | Preserve template; actual incident evidence and owner assessment must fill a separate artifact. |
| `doc/testing/templates/shared-network-incident-template.md` | `V-OUTAGE`; incident capture fields. | Not an observed fault; actual before/outage/fresh/recovery samples remain required. |
| `doc/testing/templates/shared-network-mixed-topology-gate-template.md` | `V-FINALITY`; mixed-topology gate record shape. | Retain template; Ops/P2P must produce current topology and gate outputs. |
| `doc/testing/templates/shared-network-promotion-record-template.md` | `V-FINALITY`/`V-RECOVERY`; promotion record envelope. | Not a promotion outcome; actual governance and same-window lanes remain missing until supplied. |
| `doc/testing/templates/shared-network-rollback-target-template.md` | `V-RECOVERY`; rollback target declaration shape. | Do not infer rollback target or success from example fields; actual root/history preservation must be observed. |
| `doc/testing/templates/shared-network-shared-access-check-template.md` | `V-FINALITY`; shared-access check envelope. | Template does not prove isolation; current role/access observations remain an Ops/P2P gap. |
| `doc/testing/templates/state-sync-closure-evidence-packet-template.md` | `V-RECOVERY`; expected closure-packet envelope fields. | Template only; actual attachments, raw proof bytes, digests and readback are required for semantic closure. |
| `doc/world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md` | `V-CONSUMER`; launcher cross-surface action selection when the scenario traverses launcher. | Launcher owner retains its surface contract; QA compares same candidate only where applicable, no launcher parity execution here. |
| `doc/world-simulator/launcher/game-client-launcher-runtime-session-continuity.prd.md` | `V-CONSUMER`/`V-RECOVERY`; session continuity when the selected consumer path uses launcher runtime. | Preserve launcher lifecycle clauses; actual session recovery and same-world readback remain separate proof. |
| `doc/world-simulator/llm/continuous-agent-harness.prd.md` | `V-CONSUMER`; Agent is a consumer of same-candidate state, receipts and next action. | Agent owns any PRD edits; QA link does not prove provider-backed Agent parity or receipt-gated memory/goal behavior. |
| `doc/world-simulator/llm/decision-provider-contract.prd.md` | `V-EXECUTION`/`V-CONSUMER`; provider lane and response semantics. | Provider owner retains contract; QA records provider identity and actual preflight, not a prose or mock substitution. |
| `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` | `V-INDUSTRIAL`; source for inputs, capacity, reservation, handoff, sinks and industrial output. | Gameplay/runtime retain rule and state authority; QA requires actual quantity/root/once-only evidence. |
| `doc/world-simulator/prd.md` | Product consumer/root context for player-visible world scope and gameplay status. | Preserve module authority; QA maps only accepted clauses to consumer scenes and does not author product semantics. |
| `doc/world-simulator/viewer/README.md` | `V-CONSUMER`; Viewer navigation and surface owner locator. | Index only; current Viewer observation and authoritative projection need actual artifacts. |
| `doc/world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md` | `V-CONSUMER`/`V-EXECUTION`; applied/control feedback and readmodel scope. | Viewer owns its contract; QA checks same-candidate status and application boundary without claiming headed validation. |
| `doc/world-simulator/viewer/viewer-manual.manual.md` | `V-CONSUMER`; Viewer user-facing status, blocker, and readback surface. | Viewer owns any manual changes; QA requires fresh visible state and matching authority refs for parity. |
| `testing-manual.md` | S6/S9A tier selector and receiver navigation; current provider/full proof boundary and 17/10/8 links. | QA-owned S6/S9A additions; real headed/provider/runtime execution stays unperformed and cannot be inferred from this method. |

## 设计目标
- 提供 `testing` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/testing/prd.md`
- 可变执行状态：PR、实际 CI 与评审记录（Issue 按需）
- 索引入口：`doc/testing/prd.index.md`

## 设计演进计划
- 历史来源 M1（2026-03-09）：ROUND-006 记录了补齐模块级 `design.md` 标准入口的目标；该轮次不构成当前任务准入。
- 历史计划 M2：按专题继续补齐高复杂度主题的 `*.design.md`；本条未提供完成日期或当前任务归属，不视为现行排期。当前工作按 [workflow](../engineering/workflow/source-of-truth.md) 与实际 PR、CI、评审记录追踪，Issue/Project 按需。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。
- 若自动化、synthetic、agent 实操和真实人类信号混写，stage/release claim 会越过证据边界；QA 以 `world_activity_only`、L4B session evidence 与 L5 缺失状态阻断升级。

<a id="l4-toolchain-design"></a>
## 6. L4 构建、会话与证据编排

### 当前基线与交付边界

固定基线 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056` 的静态核查中，L4B runner 仍默认调用 producer 普通 bundle、依赖 `__AW_TEST__`、创建独立 automation session 并只等待 `connected/authReady`；这与 release 不安装测试 API、Hosted 干净会话需要登录的现有合同不兼容。[#4438](https://github.com/eng-cc/oasis7/issues/4438) 是本组问题的历史来源；其[关闭决定](https://github.com/eng-cc/oasis7/issues/4438#issuecomment-6127544475) 将后续落实交给 [PR #4641](https://github.com/eng-cc/oasis7/pull/4641) 承接的现行规范，未认定实现完成。本节补齐目标设计，未修改 runner 或执行真实试玩；下列验证矩阵不是通过记录，默认命令仍需实现修复后才能按目标合同验收。

<a id="l4-build-selection"></a>
### 6.1 构建与驱动选择

启动前固定同一候选的 source/build、Viewer 构建类型、driver、访问模式和证据范围。Viewer `release/visual-test` 与 Rust bundle 的 `packaging/dev` 是不同维度；不得把现有 `--profile dev` 当成开启 Viewer 测试 API 的开关，也不在文档中假设 runner 已存在新的选项。

| Viewer 构建与用途 | 驱动与观测 | 失败/证明边界 |
| --- | --- | --- |
| 默认 release，正式 L4B 玩家入口 | 使用正式 UI 的可见控件和状态；世界效果由同候选的现有权威 oracle 核验。 | `__AW_TEST__` 缺失是预期；UI driver 不可用时报告所缺驱动能力，不能回退到测试 API。 |
| 显式 `build:viewer:visual-test`，受限隔离自动化 | 从 `.viewer-test-dist/` 选择测试产物；仅在明确 opt-in 后使用约定测试 API，并记录该构建与实际 `execution_mode`。 | 所选测试 lane 缺 API 时报告构建/观测能力 blocker，不循环等待登录；测试动作和 fixture 结果不能代签 release UI 或自主试玩。 |

`test_api=1` 是显式测试产物内的运行时 opt-in，不能改变编译产物。凡以 L4B 玩家行为为目标，操作必须由该候选支持的正式玩家路径完成；测试 API 采样不能替代玩家动作或世界结果 oracle。

<a id="l4-session-orchestration"></a>
### 6.2 浏览器与玩家会话编排

复用 [Hosted 玩家流程](../p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#2-user-experience-functionality)、[Hosted 身份职责](../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#4-目标身份模型) 和 [agent-browser 生命周期](manual/web-ui-agent-browser-closure-manual.manual.md#agent-browser-会话生命周期约定)。L4B 仅消费这些合同，不改变其认证、注册、刷新和撤销语义。

1. runner 创建本轮唯一的 owned browser session，producer 采用同一个 session 引用；登录、观测、正式 UI 操作和最终截图均绑定该 session。若另有 observer，其角色与独立 session 单列，不能用 observer 的登录状态证明 driver 已登录。
2. 默认干净 `hosted_public_join` 路径先记录 guest 状态，再在实际 driver session 通过现有合法登录/注册路径进入玩家态。缺账号配置、受支持登录能力或登录失败时记录具体前置 blocker；不静默改部署模式或自动赋予身份。
3. 登录后核对实际 player/device session 与 runtime 的受支持注册/恢复就绪结果，再进入玩法。浏览器 session 名称、连接成功、`authReady` 或另一个窗口的状态均不能单独代替这一步；记录关联引用和就绪证据，不记录凭据。
4. 已认证恢复是独立选择的受支持路径：按既有 restore 检查确认当前 driver 恢复的是预期玩家及当前世界，再核验注册/重连结果。恢复失败如实收口；不得作为干净 guest→登录场景的通过证据，也不把 `--session` 名称前缀解释为认证恢复。
5. 正常结束、前置阻断、超时或取消均保存本次结果并执行 owned cleanup；仅回收本轮资源，保留清理失败及其诊断。会话准备与实际游玩继续遵循 [自主试玩启动边界](governance/agent-runtime-self-playtest.design.md#selftest-skill-entry)，prepared world 不代签 cold entry。

<a id="l4-result-record"></a>
### 6.3 执行记录与终态

复用 [RunInput/RunResult](governance/agent-runtime-self-playtest.design.md#6-接口与数据合同) 和 [SourceSceneRecord](governance/agent-runtime-self-playtest.design.md#selftest-source-scene)，沿用 `preparing → ready → playing → verifying → reported → cleanup` 的阶段与任意阶段 blocked/aborted 收口。项 verdict 保持 `pass/fail/blocked/inconclusive/not_applicable`，与 `execution_state` 正交，不建立第二套 run-status 枚举。

每个 attempt 关联候选/build、实际 `execution_mode`、driver/session 与 player 绑定、就绪证据、ArtifactRef、oracle、`blocking_reason`/`failure_signature`、`stop_reason`、`cleanup_result` 和复测引用。`current_attempt` 只引用该次 RunResult；首次执行即建立记录，失败/超时/取消也须收口。重试追加新 `attempt_id`，保留原尝试，不能通过重试覆盖失败分母。

构建与 driver 不匹配、显式测试 lane 缺 API、登录前置缺失、runtime 未就绪分别保留原因；负例按自身 oracle 判断，不能将产品符合预期的拒绝误记成玩法失败。结果写入失败须向调用者明确报告并保留已有材料，不能返回完整证据已生成的成功结论。

<a id="l4-judgment-boundary"></a>
### 6.4 证据与体验判定边界

采样器只写实际观测、动作/回执引用和相应 oracle 检查；真实自主试玩与旧固定动作模式按 [现有 runner 演进边界（§10）](governance/agent-runtime-self-playtest.design.md) 记录不同 `execution_mode`。世界自然推进、一个动作 ack 或固定建厂成功不自动生成 continue 判定、满分或发布建议。

新 attempt 的体验评审默认 `review_status=unreviewed`；缺实际 L4B 游玩时保留 `l4b_agentic_verdict=missing`。producer/QA 在回读证据后填写其负责的体验结论与依据，记录评审者、关联 attempt 和未证明项；脚本不得替其评分。card 和最终 summary 从同一 RunResult 引用采样事实，人工评审单独保存，不能用预填成功值或本次未评结果覆盖历史已评结论。

<a id="l4-entry-profile"></a>
### 6.5 入口与构建兼容

两类构建均保留 `viewer` canonical 与 `software_safe` compat alias；测试 API 的一致性只在显式测试构建与同一 opt-in 条件下成立。构建、复制、stale-dist rebuild 必须保留已选类型，不能因 alias/freshness 修复换用另一构建。具体矩阵和独有 alias anchors 由 [Viewer entry compatibility](../world-simulator/viewer/viewer-web-entry-compatibility.design.md#compat-build-profile-boundary) 承接。

<a id="l4-scaffold-serialization"></a>
### 6.6 Scaffold 序列化

产品适用性：N/A；reason=此条只规定离线材料生成器的路径序列化、stdout/manifest 一致性和目录保护，不实现玩家行为或产品结论；scope=prd.md#l4-scaffold-output 对应的本节序列化合同及 L4-C06 离线验证，不延伸到其他 L4 条款或取消试玩证据要求。

`prepare-playability-l4-review.sh --json` 的路径值通过 argv 传入 Python，由 `json.dumps` 序列化；不把 Bash `${VAR@Q}` 拼成 Python 源。普通、含空格、含单引号的合法路径使用同一方式，成功时 stdout 仅输出 JSON 对象且退出 0；`run_id/output_dir/packet_path/summary_path` 与磁盘 manifest 一致，`manifest_path` 指向该 manifest。保留已有目录保护；末尾输出失败后不得无提示覆盖已生成目录，调用方需要保留诊断并按既有保护处理。该合同可离线验证，不启动 L4A 栈、模型或浏览器，也不产生试玩完成结论。

<a id="playability-evidence-consumer-reception"></a>
### 6.7 可玩性证据的组合消费

Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1、O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；分别承接[证据等级](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)与[比较单位](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)。QA 接收现有原件和各 owner 的判断，不新增报告格式或自动裁决。一次当前结论先明确体验目标、正式玩家入口、候选/明确变更范围和证据窗口；环境影响可进入性、执行或解释时纳入范围。逐项沿[原来源到场景记录](governance/agent-runtime-self-playtest.design.md#selftest-source-scene)核来源、实际树、profile、真实执行/层级与领域 oracle；身份或字节一致仍不代签因果和体验。

| 原消费者 | 接收并保留的事实 | 不可替代的判断 |
| --- | --- | --- |
| [L4 原结果](#l4-result-record)与[人工判断](#l4-judgment-boundary) | 当前 attempt、事实与原件、失败/停止、reviewed_attempt 和既有独立阻断 | ACK、环境推进、固定路线或自动分数不产生玩家杠杆/continue；未执行与失败分开，重跑不覆盖原评审 |
| [评分证据](../playability_test_result/design.md#feedback-score-evidence-boundary)与[比较边界](../playability_test_result/design.md#feedback-score-comparison-boundary) | 当前尝试、来源分组、完整/缺失/排除卡、原值及逐卡阻断；比较继续按该设计的原比较条款 | 同组均值只描述该组，不能补缺卡、跨来源或抵消另一比较单位的阻断 |
| [总纲证据分层](../game/gameplay/gameplay-top-level-design.design.md#evidence-layer-boundary)与[比较单位](../game/gameplay/gameplay-top-level-design.design.md#comparison-unit-boundary) | 当前目标/入口/profile 的真实行动、原结果和后继选择；结构、live/provider、L4 和 L5 分别给依据 | 局部工业或成熟样本不代签全部受支持路径；原 gameplay/领域 owner 仍核实际因果和范围 |
| [真人观察](manual/first-session-human-playtest.manual.md#human-study-assessment) | 实际人数/情境、独立或提示完成、原话、领域事实和未观察；候选变化保留复测关联 | 内部校准、外部真人、Agent 分开；被询问意愿、预约接续与自发返回不互换，小样本不推出留存 |
| [L4 汇总](templates/playability-l4-summary-template.md#verdict-summary)与[发布材料](../playability_test_result/templates/playability-release-evidence-bundle-template.md#评分摘要) | 原 RunResult、已评来源与适用比较单位、独立 blocker、各层未证明项及当前 owner 判断 | 材料生成、分数或局部 pass 不自行提升公开阶段；发行/公开 claim 仍由原 authority 接收 |

不属于同一比较单位的样本只作相邻观察；范围不明只支持原 watch，正在支持更强 claim 时保持 hold；已有独立 blocker 不被另一正向样本、未知总分或重跑消除。核对采用原产品要求及各消费者既有方法，不按文档文件数或只挑成功样本声明全量。某个声明内的必需路径/消费者尚无可核对记录时保留原 gap/owner/下一步；只完成该受测子范围，不删掉上位要求。

旧 runner 的自动结论/覆盖问题仍是已披露的实施差距。按原汇总合同保留已评原件、另存新尝试并独立审其事实；新的追踪关系、模板或静态审阅不代表修复、真实 L4/L5 执行或发行就绪。

<a id="release-candidate-evidence-reception"></a>
## 7. 候选整包的证据接收

Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；承接 [REQ-ENTRY-CANDIDATE-001 / AC-ENTRY-CANDIDATE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)。本节把已有专业结果接到同一候选评估，复用[发布证据包](templates/release-evidence-bundle-template.md)、[source-to-scene](prd.md#qa-infrastructure-source-scene-method)和各领域 oracle；不新增报告格式、测试选择器、批准层级或公开阶段。输入基线为 `b09c1ae38366fafa635fcd8a3752995c713e6518`；2026-11-06 补充目标接收与方法，未执行候选运行或发行评审。

### 7.1 声明范围与逐输入对应

producer 沿原产品条款确定待评阶段、claim 和受支持的平台/入口；QA 在原模板的“适用合同 / scope / tier”和“覆盖范围”中逐项展开其必需义务、专业来源、实际 owner 与验证方法。范围从原承诺得出，不能因某项缺证而事后改成不适用。`viewer`、`pure_api` 分列，alias、provider、execution lane、deployment 和 session 只作为实际环境，不生成新 mode。只选择一个 mode 的专题或 observer 样本可以保留其窄结论，不能因此通过两 mode 的整包要求；另一 mode 和原上位义务仍留在未覆盖范围。

每份输入沿原 Meta/覆盖行保留 source/base/tested tree、build/实际产物 digest 与构建来源、合同及配置/schema 版本、attempt 和证据范围，再记录适用的 world/genesis/branch、authority/manifest、状态根/高度/原操作、起止窗口及其单位、真实入口/session/driver/provider。合同版本、历史原件和被测候选身份是不同角色：旧版本可以是升级前态或合法历史证明，必须标明角色并由本候选的恢复/消费结果接上，不能将旧候选的 pass 改名为当前通过。源树相同也不省略实际构建与配置核对。

各独立场景可以使用自己的 world、分支与窗口；本接收不要求整个测试包共用一个世界或同时启动。每个被声明为跨域闭环的组合必须在自身场景内证明原件和窗口对应，不能从互不相干的场景拼出一条未验证的链。

同窗口按各原合同的时间、tick、高度或 C0/C1 定义核对，不能用同一天、相同 tick 字符串或一个候选昵称替代。QA 必须把跨域相关的原操作、结果/状态切点、规则与证据窗口对应起来；长期审计的完整范围不能剪成一次 UI 观察窗，异步回读可以引用同一不可变原件，但要证明该回读确属所声明的原窗口。无法对应的材料只能保留为历史或旁证，不能由 QA 另造容差、时长或等价规则放行。

| 候选输入及原 authority | 必须接收的对应关系与环境 | 独立判据及缺口去向 |
| --- | --- | --- |
| [Viewer 入口/世界上下文](../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#uiux-entry-session-world)与[正式 API 适用组合](../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability) | 各声明 mode 和入口分别保留实际构建、当前玩家/Agent/世界、协议能力、driver/session、provider 与正式/受控环境；按各原方法读取或执行受支持的相同玩家语义 | 每一适用入口有自己的阶段/目标/阻塞/原结果与合法下一步证据。共享 canonical 事实可复用，但一个入口的操作或截图不能代签另一入口；fixture、GET-only、无 LLM observer 不升级为正式可玩或 parity |
| [Runtime 执行](../world-runtime/design.md#runtime-deterministic-design)与 [P2P receipt/finality](../p2p/design.md#des-p2p-receipt-finality) | 同候选的实际 runtime/工件/manifest、world/parent、原输入/receipt/root 与该记录适用的合法 authority profile/proof；环境、节点和层级沿原 required/full | 先验证原领域语义和 proof，再关联消费者；本地签名、schema、transport、单节点或模拟 provider green 只证明其原范围 |
| [持久待决](../world-runtime/design.md#runtime-pending-design)、[固定原决定](../world-runtime/design.md#runtime-controlled-candidate-recovery-receiver)与[服务恢复](../world-runtime/design.md#runtime-recovery-design) | 同 world/原请求的故障前、未知、恢复、回读及后续结果；snapshot/journal、版本、权利与原 history 按各合同相接，不换世界或身份补证 | 已知无效果、固定 continuation、未知提交分别处理；正常续跑、零/一次效果及真实消费者分别有原判据。仍未知或不可读就保留 blocker；QA 整包检查不重新提交世界动作或授予读取权 |
| [长期经济审计](../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)与[全资源/长窗口方法](manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-09) | 原报告的候选/配置、world/genesis/branch、C0/C1 与 roots/history、完整 writer/ledger/resource/unit、规则/治理依据和当前合法读者，与本次阶段所需窗口相符 | 报告的来源完整性、核算、异常和各正式消费者仍分别验收；四 tick 样例、旧 OC 汇总、空告警或部分资源不代签长期审计。缺页/权限/来源保留 blocked，已证经济异常保留原 fail 与专业 owner |
| [发行到入口链](../product/player-entry-distribution/prd.md#sc-entry-006)、[官网下载快照](../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)与 [Launcher 重验](../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation) | 当前渠道候选标识→该平台/入口实际下载或 Web/API 交付版本→实际安装/加载 bytes 与构建身份→玩家看到的 backend/mode；适用 Launcher、升级与恢复路径沿原合同 | tag、manifest 或可达页面不单独证明实际运行身份；每个已声明支持的组合分别核验。unsupported、失败和恢复有原来源，不用某平台成功补另一平台 |
| [发行材料](../readme/governance/player-support-and-service-lifecycle.design.md#support-release-materials) | 选定渠道/地区/受众与该候选的安装材料、实际内容和素材、来源/权利/限制、说明与支持/数据/退出材料逐项关联；保留适用性依据与真实责任人 | 缺必需素材、来源权利或渠道资料不能写完整交付；未选择的新渠道不被默认选中。实际专业适用评估和正式素材仍由原 owner 提供，不由 QA 文档替代 |
| [QA 证据分层](#playability-evidence-consumer-reception)与[来源到场景方法](prd.md#qa-infrastructure-source-scene-method) | 原 required/full、工程、L4/L5 等结果各带 source、attempt、scope、环境、原 artifact/oracle、实际执行状态和独立阻断；使用模板现有覆盖行汇总 | 原领域通过、素材到位、玩家理解/意愿和整包对应分别成立。必需项未运行、skip/dry-run 或只有历史样本不构成该层通过；不平均抵消失败 |
| [公开口径原合同](../product/player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model)与[沟通模板](../readme/governance/readme-release-communication-template.md) | 准备发布的事实、目标、假设、证据边界逐句引用本候选相应 mode/入口/窗口的已核输入；QA 结论、产品决定、LiveOps 记录和根 README 各自保留真实状态 | 已审核草稿、内部 go 或技术 pass 均不是已发布；所述平台/能力或材料扩大时，回到受影响输入重核 |

相同的 runtime/proof、经济报告或公共资产可被多个覆盖行引用，前提是其原身份、适用范围和各实际消费者的对应均成立；这不要求为了整包重复制造世界效果，也不免除两 mode 各自的消费验证。没有实际关联的同名文件、hash 清单或复制的 summary 不构成接收。

### 7.2 汇总、变更与交接

QA 先分别回读原证据、核完整性和该领域 oracle，再检查上表的跨域对应；材料存在、结构通过和业务有效分开。按原模板汇总：只有声明范围内所有必需判据、环境和对应关系满足才记 `pass`；前置、必需证据或对应关系缺失记 `blocked`；已执行且违反原判据的 `fail` 不能被其他缺项、局部正例或重试覆盖。负例中的预期拒绝按该负例原 oracle 判断，不误写成产品行为失败。领域自身的 partial/unknown/observer 或具体异常原样保留，再说明它为何不能支撑本次更强结论；不把所有领域状态改成一套新枚举。

每个缺口在原 PR/评审记录保留受影响 mode/入口/义务、原证据、专业 owner 和下一步。补证先回读原 request/attempt/source；修复或重测保留新 attempt 和旧失败，不覆盖旧记录。 修复后的独立结果确实解除原阻断时，沿原 QA/产品职责更新当前结论，历史失败仍可回读。build、规则/配置/schema、authority、内容/权利、真实环境或声明范围发生影响判据的变化时，由原 owner 标出受影响输入与后续消费者，沿既有触发方法重核；未受影响的材料只有原身份、范围和依据仍成立才可复用，不通过改候选标签迁移旧 pass。

完整正例必须能够形成可用的候选门禁输入：各 mode 的实际能力证据、合法执行与恢复、满足该阶段原资源/窗口要求的完整经济审计、实际发行资产/材料、QA 结论及公开事实对应全部满足原判据后，QA 在模板中给出该范围 `pass` 并列明来源。producer 依据这组输入做原阶段/claim 决定，LiveOps 据真实决定和 QA 限制起草并同步既有公开 surface，根 [README 当前状态](../../README.md#项目状态)继续拥有唯一公开 claim envelope。`pass` 仅允许进入该决定步骤，不替代决定；若未作发布评估，沿模板记 `not_evaluated`，若尚未发布就保留 reviewed/publish_ready 等原状态，不能称已发布。任何阶段的 blocker 都不能由下一角色用文案消除。

具体方法为总手册 [RC01–RC10](../../testing-manual.md#release-candidate-reception-validation)。静态设计走查只核可执行合同和预期判定，真实候选按原方法取得实际环境/运行/生产/玩家证据；本节不修改 release 脚本、bundle 字段或当前阶段。

## 11. L4 与候选组合验证及证据

### 11.1 验证映射表

以下是目标合同的验证计划；本次文档修改仅做静态一致性核对。真实构建、浏览器、登录及自主试玩必须在相应实现与前置具备后执行并保留独立结果。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | [候选整包接收](#release-candidate-evidence-reception) | 原领域有效性与同候选/窗口/入口对应分别成立，QA 结论交原产品与公开 authority | [RC01–RC10](../../testing-manual.md#release-candidate-reception-validation)：同候选完整正例与各输入反例；原 required/full、真实环境和领域方法不降级 | 既有发布包逐输入来源/关联、原始 artifacts/oracle、独立 verdict、缺口 owner 与实际交接记录 | 方法未运行；静态样例、工程全绿或模板齐全不产生发布、L5 或阶段通过 |
| [AC-ENTRY-CANDIDATE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-candidate-001) | [候选整包接收](#release-candidate-evidence-reception) | 完整对应的正例可成为候选输入；混候选、局部/历史 green、缺证和对应不明保持限制 | [RC01–RC10](../../testing-manual.md#release-candidate-reception-validation)：逐输入改变一个身份、窗口、消费者或必需材料，独立核 pass/fail/blocked 与历史保留 | 原完整样本及每个变体的差异、未覆盖、失败签名和允许结论范围 | 拒绝错误输入不代签正常运行；QA pass 不自动升级 README |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[入口根 SC-5](../product/player-entry-distribution/prd.md#sc-entry-005) | [候选整包接收](#release-candidate-evidence-reception) | 仅核整包中的 mode 各自来源、正式/受控环境和独立结论 | [RC01/03/09](../../testing-manual.md#release-candidate-reception-validation)：真实 viewer/pure_api 逐入口；删除一端、借 alias/mock/observer、复制另一端结果的反例 | 两 mode 各自原结果、相同权威事实的合法复用与不能互签的缺项 | 不关闭根全部模式/恢复义务；各入口实测仍由原 owner 提供 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[入口根 SC-6](../product/player-entry-distribution/prd.md#sc-entry-006) | [候选整包接收](#release-candidate-evidence-reception) | 仅核发行资产→实际入口/运行身份→候选包的对应，各支持组合独立 | [RC01/02/07/08](../../testing-manual.md#release-candidate-reception-validation)：同平台/入口真实资产及运行关联，混版、缺权、不同 build/backend/mode 和陈旧公开材料 | 交付/加载原件与来源、真实运行身份、缺证及公开交接范围 | 不代签各平台安装/升级/首写/恢复或整个 SC-6 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | [L4 判断边界](#l4-judgment-boundary) | 自动采样、无实玩、已评历史与当前尝试分开 | [L4-C03/C04](../../testing-manual.md#l4-toolchain-validation)：固定同候选，核 ACK/progress、缺观察项、失败重试及已评 artifact；真实层另取原证据 | 事实/判断的写入归属、unreviewed/missing 与保留阻断 | fixture 不证明真人愿意继续或旧 runner 已修复 |
| Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | [证据消费组合](#playability-evidence-consumer-reception) | 技术可用、理解与继续/回访信号不相互代签 | [真人分项判据](manual/first-session-human-playtest.manual.md#human-study-assessment)：固定实际首局入口/候选/窗口，按原 10.1/10.2 保留独立/提示、预约/自发、反证与未观察；方法自身样例不作真人结果 | 真实观察/原话、原领域 oracle、证据等级和建议范围 | 首局人测不覆盖未选路径；内部正向不能覆盖外部反证 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | [执行记录与终态](#l4-result-record) | 来源、实际树、阶段/入口和证据完整性决定记录适用范围 | [来源到场景核对](manual/agent-runtime-self-playtest.manual.md#source-scene-check)：固定条款/候选，核旧 OID、错树、层级错标、必需阶段缺失及跨 world/task/profile 拼接；按原 required/full 分层 | SourceClauseRef/CandidateIdentity、原字节与逐项 gap | 记录检查通过不形成运行或产品建议通过 |
| Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | [证据消费组合](#playability-evidence-consumer-reception) | 候选/入口/目标/窗口及影响解释的环境变化必须保留可比范围 | [真人复测与范围核对](manual/first-session-human-playtest.manual.md#human-study-retest)：沿原准备/评估记录审规则、候选、会话与环境变更，保留不可合并、旧失败和提示污染；依 PE003 原 AC 复核建议范围 | 原 finding/候选/观察窗、不能合并的理由及 retained blocker | 具体人测样本不替其他消费者执行；范围不明不形成更强 claim |
| Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；[AC-14C](prd.md#browser-functional-regression) | [浏览器功能回归层](#browser-functional-layer) | BROWSER-AC-1–5：真实 consumer、受控事件、独立预期、隔离与证据边界。 | [实施核验](../../testing-manual.md#browser-functional-regression-validation)执行 BFR-P01–P07：同候选明确 fixture-functional，真实浏览器与声明 viewport。 | 实施 PR 的命令、构建/模块身份、场景 trace、截图、console/network、准确断言和选择结果。 | fixture-functional 片已实现并接入选测；不证明后端/provider、生产权限、世界结果、其他浏览器或试玩。 |
| [L4 构建与驱动](prd.md#l4-build-driver) | [构建与驱动选择](design.md#l4-build-selection) | 默认 release/UI 与显式 test/API 分离，缺能力准确阻断。 | [主手册 L4-C01](../../testing-manual.md#l4-toolchain-validation)：固定同候选 release/test 产物，静态核对默认选择，再按实现执行所选 driver 与能力缺失负例。 | 两构建身份、driver 选择与能力判断的本次记录。 | 文档通过不证明默认 runner 已可用或浏览器已执行。 |
| [L4 Hosted 会话](prd.md#l4-hosted-session) | [浏览器与玩家会话编排](design.md#l4-session-orchestration) | 干净 guest 登录及独立恢复路径分别验证，实际执行 player/session 一致。 | [主手册 L4-C02](../../testing-manual.md#l4-toolchain-validation)：受支持 Hosted 测试环境与同候选 driver，覆盖干净登录、恢复、失败前置和 owned cleanup。 | guest/登录/注册就绪、session/player 关联及清理证据。 | 不扩展认证体系；其他窗口或 DevLocal lane 不证明默认 Hosted 路径。 |
| [L4 执行结果](prd.md#l4-execution-result) | [执行记录与终态](design.md#l4-result-record) | 各阶段结果、取消、失败、重试与清理均有可追溯记录。 | [主手册 L4-C03](../../testing-manual.md#l4-toolchain-validation)：离线可控 driver 验证各收口路径及报告失败；真实依赖另按所选场景执行。 | attempt/RunResult、停止与清理结果、保留的失败记录。 | fake driver 不证明已登录、已实玩或世界效果。 |
| [L4 证据与判定](prd.md#l4-evidence-judgment) | [证据与体验判定边界](design.md#l4-judgment-boundary) | 采样不自动给 continue/满分；card/summary 引用同次结果并保留人工判定。 | [主手册 L4-C04](../../testing-manual.md#l4-toolchain-validation)：检查无实玩、固定脚本、失败重试和已评历史样本；真实 L4B 另由对应 oracle 与角色评审。 | 同 attempt 事实引用、unreviewed/missing 边界与独立评审依据。 | 动作 ack、模板或自动分数不证明体验或 L5。 |
| [L4 入口兼容](prd.md#l4-entry-compatibility) | [入口与构建兼容](design.md#l4-entry-profile) | canonical/compat × release/test 保留 alias、构建类型与 API opt-in 边界。 | [主手册 L4-C05](../../testing-manual.md#l4-toolchain-validation)：同候选四格矩阵，并分别核对 test opt-in 开/关及 release 加参数负例。 | 四格产物/入口结果与现有 alias/copy/freshness 验证记录。 | 不证明 CDN/线上部署或 release 试玩结论。 |
| [L4 scaffold 输出](prd.md#l4-scaffold-output) | [Scaffold 序列化](design.md#l4-scaffold-serialization) | stdout JSON 可解析且与 manifest 一致，已有目录重试保护保留。 | [主手册 L4-C06](../../testing-manual.md#l4-toolchain-validation)：离线临时目录覆盖普通、空格、单引号路径及已有目录重试。 | 退出码、stdout JSON、manifest 字段对账与目录保护结果。 | scaffold 验证不完成 L4A/L4B，也不执行模型或真实游戏。 |
