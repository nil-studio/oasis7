# testing 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/testing/prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 对应文件级索引: `doc/testing/prd.index.md`

## 1. 设计定位
`testing` 模块的 `design.md` 负责描述测试分层、验证策略、证据采集与发布门禁的总体设计。

## 2. 阅读顺序
1. `doc/testing/prd.md`
2. `doc/testing/design.md`
3. `doc/testing/prd.index.md`
5. 下钻 `ci/`、`governance/`、`launcher/`、`longrun/`、`performance/` 等专题目录

### 2.1 需求承接与分配表

本表只承接 L4 工具链本次受影响条款；既有 QA infrastructure 方法、89 项依赖矩阵和各专业域义务继续保留。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [L4 构建与驱动](prd.md#l4-build-driver) | 默认 release 通过正式 UI 驱动；显式测试构建与观测能力单独选择。 | [构建与驱动选择](design.md#l4-build-selection) | Viewer build/driver；producer bundle 入口。 | 测试产物或参数不能代签 release 体验。 |
| [L4 Hosted 会话](prd.md#l4-hosted-session) | 实际执行 session 完成受支持登录/恢复，确认 player 关联并回收自有资源。 | [浏览器与玩家会话编排](design.md#l4-session-orchestration) | 既有 Hosted 登录、runtime session 与 agent-browser 生命周期合同。 | 不新增账户、认证、签名或跨会话凭据交接体系。 |
| [L4 执行结果](prd.md#l4-execution-result) | 每个 attempt 保留实际执行模式、阶段、阻断、停止和清理结果。 | [执行记录与终态](design.md#l4-result-record) | 既有 RunInput/RunResult 与 QA receiver；runner 负责落盘。 | 有结果文件不表示已完成实玩或通过。 |
| [L4 证据与判定](prd.md#l4-evidence-judgment) | 采样事实、权威结果和人工体验判定分开，失败尝试及未评状态保留。 | [证据与体验判定边界](design.md#l4-judgment-boundary) | gameplay/runtime oracle；producer 与 QA。 | 固定动作成功不证明自主游玩、continue、满分或 L5。 |
| [L4 入口兼容](prd.md#l4-entry-compatibility) | canonical/compat 两入口均保持所选 release/test 构建边界。 | [入口与构建兼容](design.md#l4-entry-profile) | Viewer entry compatibility PRD/design 与 dist copy。 | 不删除兼容入口或把测试 API 带回 release。 |
| [L4 scaffold 输出](prd.md#l4-scaffold-output) | 路径通过 argv 传入序列化器，JSON stdout 与 manifest 一致且保留目录保护。 | [Scaffold 序列化](design.md#l4-scaffold-serialization) | prepare-playability-l4-review.sh 与其 stdout/manifest 消费者。 | 离线 scaffold 成功不完成 L4A/L4B。 |

## 3. 设计结构
- 分层层：`test_tier_required` / `test_tier_full` 的职责分工。
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

### Release Web fixture 生命周期

发行 Web QA 的 source 路径先构建 visual-test Web、signed fixture executable，以及 `run-launcher-stack.sh --chain-disable` 消费的 provider probe、launcher 和 Viewer。全部成功后才启动 fixture 的有界运行窗口，再由 stack 复用本次构建的同候选可执行文件；任何构建失败都不启动 fixture。该顺序将冷构建与运行/交互超时分开，不延长 fixture 期限、不修改正式认证，也不将缓存中未知来源的 binary 当作构建证明。

## 5. 专题导航
- CI 与覆盖进入 `ci/`
- 线上/长时验证进入 `longrun/`
- 发行与治理验证进入 `governance/`、`launcher/`

<a id="qa-infrastructure-evidence-method"></a>
### QA infrastructure evidence method

The [QA receiver contract](prd.md#qa-infrastructure-receiver) is the stable entry for combining product obligations, professional contracts, scenes, and test-tier evidence. For each scene/cell/specimen, freeze the exact clause fragment and source identity, candidate/base/tested tree, world/branch/root/window/configuration, applicable domain owner, execution state, dependency/topology and provider identity, ordered phases, artifact locator/bytes/digest, oracle, verdict, and residual proof boundary. Keep `planned`, `located`, `implemented`, and `executed` as independent state fields.

The verification path is source clause → named scene and phase → actual environment/dependency → professional oracle → raw evidence artifact and readback → tier-specific verdict. First bind the candidate and source; then enumerate required cases/cells; capture a no-mutation baseline; validate artifact identity and bytes; run structural checks separately from canonical, trust, semantic, and cross-consumer checks; compare same-candidate before/outage/fresh/recovery/retry/replay observations; and finally check that the selected tier's real environment is present. Missing mandatory identity, phase, cell, proof, archive readback, or environment is blocked/unverified. A contradictory observed invariant is failed with its exact signature. A shape-only pass remains shape-verified. Pass applies only to the declared scope when all its assertions and required environment are observed. Archive failures and successes append-only. This method does not turn a fixture, a link, a schema, or a previous candidate's pass into current execution or release evidence.

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

## 11. L4 验证与证据

### 11.1 验证映射表

以下是目标合同的验证计划；本次文档修改仅做静态一致性核对。真实构建、浏览器、登录及自主试玩必须在相应实现与前置具备后执行并保留独立结果。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [L4 构建与驱动](prd.md#l4-build-driver) | [构建与驱动选择](design.md#l4-build-selection) | 默认 release/UI 与显式 test/API 分离，缺能力准确阻断。 | [主手册 L4-C01](../../testing-manual.md#l4-toolchain-validation)：固定同候选 release/test 产物，静态核对默认选择，再按实现执行所选 driver 与能力缺失负例。 | 两构建身份、driver 选择与能力判断的本次记录。 | 文档通过不证明默认 runner 已可用或浏览器已执行。 |
| [L4 Hosted 会话](prd.md#l4-hosted-session) | [浏览器与玩家会话编排](design.md#l4-session-orchestration) | 干净 guest 登录及独立恢复路径分别验证，实际执行 player/session 一致。 | [主手册 L4-C02](../../testing-manual.md#l4-toolchain-validation)：受支持 Hosted 测试环境与同候选 driver，覆盖干净登录、恢复、失败前置和 owned cleanup。 | guest/登录/注册就绪、session/player 关联及清理证据。 | 不扩展认证体系；其他窗口或 DevLocal lane 不证明默认 Hosted 路径。 |
| [L4 执行结果](prd.md#l4-execution-result) | [执行记录与终态](design.md#l4-result-record) | 各阶段结果、取消、失败、重试与清理均有可追溯记录。 | [主手册 L4-C03](../../testing-manual.md#l4-toolchain-validation)：离线可控 driver 验证各收口路径及报告失败；真实依赖另按所选场景执行。 | attempt/RunResult、停止与清理结果、保留的失败记录。 | fake driver 不证明已登录、已实玩或世界效果。 |
| [L4 证据与判定](prd.md#l4-evidence-judgment) | [证据与体验判定边界](design.md#l4-judgment-boundary) | 采样不自动给 continue/满分；card/summary 引用同次结果并保留人工判定。 | [主手册 L4-C04](../../testing-manual.md#l4-toolchain-validation)：检查无实玩、固定脚本、失败重试和已评历史样本；真实 L4B 另由对应 oracle 与角色评审。 | 同 attempt 事实引用、unreviewed/missing 边界与独立评审依据。 | 动作 ack、模板或自动分数不证明体验或 L5。 |
| [L4 入口兼容](prd.md#l4-entry-compatibility) | [入口与构建兼容](design.md#l4-entry-profile) | canonical/compat × release/test 保留 alias、构建类型与 API opt-in 边界。 | [主手册 L4-C05](../../testing-manual.md#l4-toolchain-validation)：同候选四格矩阵，并分别核对 test opt-in 开/关及 release 加参数负例。 | 四格产物/入口结果与现有 alias/copy/freshness 验证记录。 | 不证明 CDN/线上部署或 release 试玩结论。 |
| [L4 scaffold 输出](prd.md#l4-scaffold-output) | [Scaffold 序列化](design.md#l4-scaffold-serialization) | stdout JSON 可解析且与 manifest 一致，已有目录重试保护保留。 | [主手册 L4-C06](../../testing-manual.md#l4-toolchain-validation)：离线临时目录覆盖普通、空格、单引号路径及已有目录重试。 | 退出码、stdout JSON、manifest 字段对账与目录保护结果。 | scaffold 验证不完成 L4A/L4B，也不执行模型或真实游戏。 |
