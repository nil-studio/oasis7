# testing PRD 文件级索引

审计轮次: 10

更新时间：2026-10-12

## 入口
- 模块 PRD：`doc/testing/prd.md`
- 模块设计总览：`doc/testing/design.md`
- 可变任务状态与历史：PR、实际 CI 与评审记录（Issue 按需）
- 当前 QA 测试合同与覆盖边界：`doc/testing/prd.md`

## 首读分流

- 想核对 Release Trunk 安装、可信缓存复用、冷暖耗时或 apt 安装预算：先读 [CI 分级执行 PRD](ci/ci-tiered-execution.prd.md#ci-install-trunk)与[配对设计](ci/ci-tiered-execution.design.md#ci-install-trunk-design)，操作见[依赖安装核验](../../testing-manual.md#ci-dependency-installation-validation)。
- 想先回答 testing 模块覆盖哪些测试层级、证据与门禁边界：先读 `doc/testing/prd.md`
- 想判断好玩性证据、`L4A/L4B/L5` 边界、角色 subagent review 或 simulated player persona 的 canonical topic：先读 `doc/testing/governance/README.md`
- 想在一个 worktree 里准备 `L4A + L4B` 执行包：先经 `doc/testing/governance/README.md` 确认所需证据层，再读 [L4 工具链合同与验收](../../testing-manual.md#l4-toolchain-validation)。`./scripts/prepare-playability-l4-review.sh` 只生成 scaffold；实际 L4B 需满足 release/UI、Hosted 会话与当前尝试结果合同，旧 runner 尚存实现差距。
- 想执行 Web UI、Playwright、public-testnet attach 或模型视觉评审手册：先读 `doc/testing/manual/README.md`，再按问题进入对应 manual。
- 想快速判断“现有性能测试覆盖到哪、哪些功能面最值得补性能测试、哪些更适合进 scoped gate”：先读 `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md`
- 想先回答 world state sync、commit closure、state-sync bundle、API/viewer projection，或 `module_required/module_full/integration_required/release_full` 如何定档：先读 `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`，再下钻 S9/S10 执行套件。
- 想确认当前任务、QA 阻断或收口状态：读取 PR、实际 CI 与评审记录（Issue 按需）；文档只提供稳定测试合同与证据边界。
- 想直接决定要跑哪套测试或按步骤执行：先读 `testing-manual.md`；涉及专项操作再进入 `doc/testing/manual/README.md`。
- 想先进入 `evidence` 热点子域，并按 release gate / hosted access / legacy p2p network rehearsal / governance drill / claim-audit 问题分流：先读 `doc/testing/evidence/README.md`
- 想继续按子域或文件名下钻：使用下方热点子域导航，再跳到对应清单区域

## 密度说明
- 本索引不维护静态文件数量；数量快照容易漂移，只能作为历史审计材料，不作为默认阅读顺序或覆盖完整性的依据。
- 需要重新统计时，以 `./scripts/doc-inventory-report.sh` 的 module density / hotspot 状态为准，并把正式报告结果写入具体审计任务；`find doc/testing -type f` / `rg --files doc/testing` 只可作为本地探索辅助，不能替代正式 inventory 状态。

## 热点子域导航
| 子域 | 适合回答的问题 |
| --- | --- |
| `benchmarks/` | 测试成熟度对标、单实现栈等价要求、历史缺口矩阵与 claims denylist；不维护当前网络状态 |
| `evidence/` | 发布证据、趋势基线与审计留痕；当前已补 `evidence/README.md` 作为热点子域入口 |
| `ci/` | CI、wasm determinism、tiering、required check 保护、依赖安装与工具缓存量测 |
| `longrun/` | 长稳、chaos、soak、world state sync / commit closure 与在线稳定性 |
| `launcher/` | 启动器链路测试、playtest 与配置自动接线 |
| `governance/` | 质量趋势、release-gate 指标、审计检查与 playability 证据治理；先读 `doc/testing/governance/README.md` 再按问题下钻 |
| `templates/` | 证据包、报告、模型视觉评审卡与检查清单模板；默认按需进入 |
| `performance/` | runtime / viewer 性能观测与方法学 |
| `manual/` | 系统测试手册分册、Web UI / Playwright / public-testnet attach 与模型视觉评审分流；先读 `doc/testing/manual/README.md` |
| `chaos-plans/` | 专项 chaos plan 入口 |

## 活跃补充文档

- [工业创建四阶段验收](manual/industrial-creation-simulation-and-admission.manual.md)：同输入模拟、试点预算/退出、当前准入和原请求恢复。
- [有限许可与版税验收](manual/industrial-license-and-royalty-settlement.manual.md)：真实批次、托管、通知与最小保全、累计整数分配及开放。
- [情境声誉验收](manual/contextual-reputation-acceptance.manual.md)：合同来源、主体/情境/时效、补救、更正和旧分数消费者隔离。
- [组织与岗位履历验收](manual/organization-custody-role-reputation-acceptance.manual.md)：登记/接替/真实托管返还到组织与原任期咨询的来源、权限、时效、派生更正及恢复；方法与实际运行证据分开。
- [R&D 归因与份额转让验收](manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md)：预工作时序、贡献与使用权、冻结分配、有限程序和原槽转让；不把方法或静态复算写成真实业务通过。
- [外部 Runtime 私有上下文恢复](manual/external-agent-runtime-contract-validation.manual.md#adapter-private-context-recovery)：原 run/operation 核对、完整任务基线、告知/重规划与双 Agent 隔离；真实 Runtime 分别取证。
- [外部 Runtime 自主试玩与开发自测设计](governance/agent-runtime-self-playtest.design.md)：承接现有 L4B、真实 subagent、玩家 Skill、自主游玩、权威验真及修复回归，不新增通用门禁。
- [开发 Agent 自测操作手册](manual/agent-runtime-self-playtest.manual.md)：唯一操作入口，集中维护预检、启动、派工、七组场景、反馈与清理；试玩者读取正式游戏 Skill。
- [首局真人试玩手册](manual/first-session-human-playtest.manual.md)：复用现行证据层级与首局要求，提供受控目标玩家的筛选、两次会话、主持话术、提示规则、原始记录和复测方法；真实执行与结论在原 PR/证据入口分别留证。
- [独立试玩任务模板](templates/agent-runtime-playtester-task.template.md)与[自测报告模板](templates/agent-runtime-playtest-report.template.md)：由手册引用的可复用格式，不是独立流程入口或已执行证据。
- `doc/testing/governance/README.md`：`governance/` 热点子域 landing page，按质量门禁、playability 证据、L4 分层、role review 与 persona 问题分流读者。
- `doc/testing/evidence/README.md`：`evidence/` 热点子域 landing page，按 release gate、hosted access、legacy p2p network rehearsal、governance drill 与 claim/audit 分流读者。
- `testing-manual.md`：仓库级系统测试手册，不并入下方模块 PRD 三件套长表。
- `doc/testing/manual/README.md`：`manual/` 子树 landing page，按 Web UI 页面闭环、真实 Playwright 流程、formal public-testnet attach 与模型视觉评审分流；具体 manual 保留 operator 真值，不并入下方模块 PRD 三件套长表。
- `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md`：按 surface 汇总现有性能覆盖、当前缺口、建议补测和建议 tier 的速查表。
- `doc/testing/performance/compile-metrics-v2-warm-check.design.md`：Compile Metrics V2 warm/no-op check 的技术合同、冷/暖隔离、schema/identity、阈值与 rollout；它补充性能矩阵，不承载可变任务状态。
- Provider dual-mode 历史 blocker 已收敛到 `doc/world-simulator/llm/provider-agent-dual-mode.prd.md`、`provider-agent-dual-mode-contract.md` 与复签 evidence；当前 task blockers 只从 PR、实际 CI 与评审记录（Issue 按需） 进入。

Manual convergence note: Web UI manual 当前按 driver / evidence mode 分流，不再作为单一 PRD/design/project 三件套维护。`web-ui-agent-browser-closure-manual.*` 保留 Viewer 页面通用闭环的 PRD / project / manual 真值；`web-ui-playwright-closure-manual.manual.md` 保留真实本地栈 + 玩家 UI 操作流程的 `PWT-###` 系列入口；`web-ui-playwright-closure-manual.design.md` 仅作为历史/共享设计 companion 追溯，不再把两条 manual surface 合并成一个 normal triplet。

## 默认阅读面边界
- 本页首屏只负责分流，不再要求读者从第一行开始顺扫完整长表。
- README 不再平铺“近期专题”；专题清单继续保留在下方，用于精确文件名检索和互链可达性。
- `evidence/README.md` 负责最高密度热点子域的首读分流；完整长表继续由本页与目标 evidence 文件承担。
- 手册、blocker、evidence 与 template 等 supporting / 审计材料继续保留可检索性，但不并入模块 PRD 三件套长表。

## 覆盖规则
- 纳入规则：列出 `doc/testing/**` 下专题 PRD、实际承接的设计及相关 PR、CI 与评审记录（Issue 按需）。本次 coverage 与 testcase-tiering 两份 PRD 共用 CI 分级执行设计的独立章节，不复制设计正文；下方 launcher 两个完成态专题保留既有退休例外，其历史拆解从 PR、实际验证记录与 Git 历史追溯。
- 活跃补充：`testing-manual.md`、`*.manual.md` 与仍被当前模块 PRD / 项目态直接引用的 blocker/supporting spec，可在“活跃补充文档”区定向列出，但不并入下方三件套长表。
- 排除规则：不纳入 `doc/devlog/**`、`doc/testing/evidence/**`、`doc/testing/templates/**` 与其他非 PRD 配对文档。
- 按需进入：evidence、template、blocker、closure 说明与历史归档继续保留可检索性；除非它们重新成为当前 operator 或 owner 的直接入口，否则不进入默认首屏。

## 专题清单（含历史完成项，按文件名精确检索）
本表用于互链可达性和 traceability，不代表每一行都是默认首读或当前活跃执行面；一般已完成规划三件套应先读其 redirect / historical status，再进入当前 canonical 手册或 topic 文档。本表明确标记 CI 两专题的共享设计，以及 launcher 两专题的既有退休例外；不据此批量改变其他专题的组织方式。

2026-07-15 governance note: 早期 launcher 专题三件套与已完成的 design/project 拆解已退役删除；当前 launcher / playtest / Web UI 闭环入口收敛到 `testing-manual.md`、`doc/testing/launcher/README.md`、两份 launcher 现行 PRD、Web UI manual、PR、实际 CI 与评审记录（Issue 按需） 与 Git 历史。

Performance observability historical/provenance note: 2026-02-25 的 runtime foundation、LLM/API decoupling、Viewer native bottleneck 与 Viewer methodology 四组三件套已完成专业权威合并并物理删除；当前 authority 为 `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md`、`doc/testing/performance/viewer-current-web-performance-harness-2026-06-02.prd.md` 与 `testing-manual.md`。旧 Viewer native `RenderPerfSummary` / `PerfHotspot` / runtime-stage / CSV contract 随 old `perf_probe` / `viewer-owr4-stress` 退役；当前浏览器 probe 不承诺 hotspot 分类或 runtime-stage diagnosis。历史里程碑、阈值来源与完成证据仅从 Git/PR 与实际验证记录 追溯。

Builtin WASM CI 历史收口：早期 m1 independent multi-runner、required-check protection、base-gate hash、wasm32 target install 与 build-determinism guard 五组三件套已退役删除。当前测试入口为 `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.prd.md`、`doc/testing/prd.md`、`testing-manual.md` 与 `.github/workflows/wasm-determinism-gate.yml`；发布级 build/hash/identity/release evidence authority 为 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`。历史实施与完成证据只从 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯。

| 专题 PRD | 专题设计文档 |
| --- | --- |
| `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md` | `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md` |
| `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.prd.md` | `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.design.md` |
| `doc/testing/ci/ci-test-coverage.prd.md` | [CI 共享设计：独立承接](ci/ci-tiered-execution.design.md#ci-coverage-design) |
| `doc/testing/ci/ci-testcase-tiering.prd.md` | [CI 共享设计：独立承接](ci/ci-tiered-execution.design.md#ci-case-tier-design) |
| `doc/testing/ci/ci-tiered-execution.prd.md` | `doc/testing/ci/ci-tiered-execution.design.md` |
| `doc/testing/governance/llm-skip-tick-ratio-metric.prd.md` | `doc/testing/governance/llm-skip-tick-ratio-metric.design.md` |
| `doc/testing/prd.md`（Durable Playability Evidence Governance） | `testing-manual.md` 的 L4A/L4B/L5 operator steps | 已吸收的 2026-05 playability governance batch：Git history 与 PR 与实际验证记录 |
| `doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md` | `doc/testing/governance/token-genesis-allocation-audit-checklist.design.md` |
| `doc/testing/governance/testing-quality-trend-tracking-2026-03-11.prd.md` | `doc/testing/governance/testing-quality-trend-tracking-2026-03-11.design.md` |
| `doc/testing/launcher/launcher-bundle-first-playtest-entry-2026-03-12.prd.md` | 当前设计决策与验收记录已并入 PRD | 已完成项目拆解见 Git 历史与 PR 与实际验证记录 |
| `doc/testing/launcher/launcher-manual-test-checklist-2026-03-10.prd.md` | 当前执行设计与分层清单已并入 PRD | 已完成项目拆解见 Git 历史与 PR 与实际验证记录 |
| `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md` | `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md` |
| `doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md` | `doc/testing/longrun/p2p-longrun-soak-and-chaos.design.md` |
| `doc/testing/longrun/s10-five-node-real-game-soak.prd.md` | `doc/testing/longrun/s10-five-node-real-game-soak.design.md` |

## 说明
- 本索引用于保证模块专题文档在根入口文档树中可达。
- 文档配对规则：一般专题保持需求、设计与交付记录分工；本页 CI 两专题通过精确章节链接共用设计，launcher 完成态保留既有退休例外。以上限定处置不作为全仓批量合并或删除授权。
