# oasis7：S10 五节点真实游戏数据在线长跑套件

- 对应设计文档: `doc/testing/longrun/s10-five-node-real-game-soak.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 4

## 1. Executive Summary
- Problem Statement: 现有 S9 长跑侧重 P2P/存储/共识稳定性，但对真实 gameplay 数据流、奖励结算、资产不变量的联合验证覆盖不足，发布前仍存在高风险盲区。
- Proposed Solution: 建立 S10 五节点在线长跑套件（1 sequencer + 2 storage + 2 observer），提供统一编排脚本、硬门禁指标与证据产物，在 no-LLM 默认路径下稳定复现实战数据交换与结算链路。
- Success Criteria:
  - SC-1: `scripts/s10-five-node-game-soak.sh` 支持一键执行、dry-run、可配置阈值与拓扑参数。
  - SC-2: S10 门禁覆盖 committed 进展、网络 lag、DistFS 失败率、结算应用失败率、资产不变量、mint 样本。
  - SC-3: `summary.json/summary.md/timeline.csv/nodes/*` 证据产物完整且可审计。
  - SC-4: no-LLM 五节点长跑在多轮 600s+ 场景下持续通过，关键结算签名错误计数为 0。
  - SC-5: 与 `testing-manual.md` 的 S10 触发矩阵和执行口径保持一致。
  - SC-6: 空 DistFS blob set 可用幂等、非敏感 probe seed 形成最小可观测样本；seed 失败不阻断 runtime，且绝不把样本生成等同于 metric gate 或 release 通过。

## 2. User Experience & Functionality
- User Personas:
  - 测试维护者：需要稳定可重复的五节点真实链路验证。
  - 发布负责人：需要可审计的 S10 高风险门禁证据。
  - Runtime/脚本维护者：需要定位 stall、验签失败、结算失败等复杂故障。
- User Scenarios & Frequency:
  - 发布前门禁：按候选版本执行 S10 基线（至少 600s）。
  - 故障复盘：拉取历史 run 产物对比失败签名与指标漂移。
  - 日常回归：短窗（130~260s）验证修复点是否回归稳定。
- User Stories:
  - PRD-TESTING-LONGRUN-S10REAL-001: As a 测试维护者, I want an executable five-node soak script with deterministic defaults, so that S10 can be run repeatedly across branches.
  - PRD-TESTING-LONGRUN-S10REAL-002: As a 发布负责人, I want hard-gate metrics and structured artifacts, so that release decisions are traceable and defensible.
  - PRD-TESTING-LONGRUN-S10REAL-003: As a runtime 维护者, I want explicit controls for attest strategy, epoch cadence, and state isolation, so that stall/signature regressions can be reproduced and fixed quickly.
  - PRD-TESTING-LONGRUN-S10REAL-004: As a QA 维护者, I want an idempotent DistFS probe seed with explicit evidence limits, so that empty storage can become observable without turning seed creation into a false pass.
- Critical User Flows:
  1. Flow-S10REAL-001: `参数解析 -> 生成五节点配置/peer/full-mesh -> 启动多进程`
  2. Flow-S10REAL-002: `运行期采集 status/report -> 计算门禁指标 -> 输出 summary/failures`
  3. Flow-S10REAL-003: `结算发布与 mint 链路执行 -> 记录 settlement apply/minted_records 证据`
  4. Flow-S10REAL-004: `测试手册触发矩阵对齐 -> 发布评审引用产物`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 脚本入口 | `--duration-secs`、`--scenario`、`--llm/--no-llm`、`--base-port`、`--out-dir` | 生成 run 配置并启动/停止五节点 | `planned -> running -> completed/failed` | 默认 `duration=1800`、`scenario=llm_bootstrap`；scenario 用于派生 world id，`--llm/--no-llm` 仅记录兼容字段 | 测试维护者可配置 |
| 奖励/结算配置 | `--reward-runtime-epoch-duration-secs`、`--reward-points-per-credit` | 调整 epoch 节奏与 minted 触发概率 | `configured -> observed` | 短 epoch（默认 60s）提升短窗可观测性 | 发布/测试可审阅 |
| 节点行为策略 | `--node-auto-attest-all`、`--node-no-auto-attest-all`、`--node-auto-attest-sequencer-only`、`--preserve-node-state` | 控制投票策略与状态隔离 | `isolated -> progressed` | 默认 `sequencer_only` + 隔离旧状态 | 脚本维护者控制 |
| 门禁判定 | `max_stall`、`max_lag_p95`、`max_distfs_failure_ratio`、`max_settlement_apply_failure_ratio` | 根据指标阈值给出 pass/fail | `collecting -> gated` | failure_ratio 以 attempts 为分母，需 `attempts>0` | 发布门禁消费 |
| 证据产物 | `run_config.json`、`timeline.csv`、`summary.json`、`summary.md`、`nodes/*` | 输出统一目录并保留关键日志 | `collected -> archived` | 失败时生成 `failures.md` | QA/发布读取 |
| DistFS probe bootstrap | `storage_root/blobs`、seed result、`distfs_total_checks` | 仅空集时写入最小非敏感 seed；非空跳过；失败记录错误并继续 | `checked -> seeded/skipped/error_recorded -> sampled` | 幂等、不得无界重复写入；seed 只提供样本，不改变阈值 | runtime 自动执行，QA解释结果 |

### Reward-runtime metric source and classification

下列为目标验收合同；当前脚本仍有 mint 兼容计数、逐节点缺样本、HTTP 回退和 invariant 读取/分类差距，具体见 [S9/S10 手册](../../../testing-manual.md#s9p2p存储共识在线长跑技术套件)。脚本绿灯不代表这些合同已全部实现；本次仅校正编排入口，不修改指标、seed、五节点或非 readiness 边界。

- S10 直接消费 `oasis7_chain_runtime` 的
  `/v1/chain/status.reward_runtime` 快照；`metrics_available` 与 mint、DistFS、
  settlement、invariant 字段以 runtime 输出为准，禁止用兼容字段回推。
- 聚合时，每个可用节点先取累计指标最大值，再计算跨节点比例；缺失节点必须在
  summary/failures 中保留告警，不能静默补零。
- `distfs_total_checks=0` 必须分类为 `insufficient_data`，与比例超阈值分离。
  `reward_asset_invariant_violation` 仅在
  `reward_runtime.invariant_ok=false` 时成立；`running_false` 与 `http_failure`
  即使发生于 chaos 恢复窗口，也必须作为独立失败分类，不能转换成 invariant
  violation。status 不可达时写入 HTTP failure 并终止门禁判定。
- `run_config.json`、`summary.json`、`summary.md` 与 `failures.md` 继续是兼容
  证据接口；一次历史通过或 probe seed 成功均不能替代当前完整 metric gate。
- Acceptance Criteria:
  - AC-1: 脚本支持五节点拓扑启动/停止、dry-run、帮助输出。
  - AC-2: `summary.json` 含所有 S10 门禁字段，并与 `summary.md` 一致。
  - AC-3: settlement 应用统计字段（attempts/failures/failure_ratio）稳定输出。
  - AC-4: 真实短跑与长跑样本证明关键验签失败告警为 0。
  - AC-5: 文档保持专题 PRD、设计与实际运行证据可追溯；交付记录使用 PR、实际 CI 与评审，Issue 按需，不要求额外任务身份。
  - AC-6: DistFS seed 只在空集写入且可重复启动；短窗仍缺样本时必须保留 `insufficient_data`，不能因 seed 存在伪造 pass。
- Non-Goals:
  - 不新增 `oasis7_viewer_live` 内建五节点拓扑枚举。
  - 不在本专题引入 S10 chaos 注入编排。
  - 不改共识算法、存储证明协议语义。
  - 不通过降低 DistFS 阈值、忽略 missing samples 或预置 seed 来绕过 S10 gate。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题为分布式长跑编排与运行时门禁，不涉及 AI 模型推理系统改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 基于 `oasis7_chain_runtime` 直接 headless 多进程编排 5 节点（1 sequencer + 2 storage + 2 observer），脚本统一构建 validator/peer 配置、采集每节点状态报告，并在 S10 门禁中联合判定共识推进、网络追平、存储挑战、结算应用与资产一致性。
- Integration Points:
  - `scripts/s10-five-node-game-soak.sh`
  - `crates/oasis7_chain_runtime/src/host.rs`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - 历史 no-LLM stall 处置：旧 Viewer 路径曾以仅 sequencer 执行 hook 避免 contiguous-height 冲突。当前脚本直接运行各节点 `oasis7_chain_runtime dev node --config`，以节点 role 和 attest 参数配置；`--llm/--no-llm` 仅写入 `llm_enabled_compat`，不切换 runtime 行为，不能据此声称当前仍使用旧 hook 开关。
  - 跨 run 污染：默认隔离旧 `output/node-distfs/s10-*` 状态目录。
  - 非连续 committed 高度：execution bridge 记录告警后容错续跑，避免直接卡死。
  - 结算验签失败：统一按 `node_id` 派生 signer key + CBOR 编码，解码兼容 JSON 回退。
  - minted 为空：在 mint 前补齐 settlement 涉及节点身份绑定，避免 `node identity is not bound`。
  - 样本不足：通过可控 epoch 时长提升短窗可观测性，同时保留 attempts>0 前置判断。
  - DistFS 空集：仅空 blob set 写入幂等非敏感 probe seed；不可写时记录错误并等待真实业务样本，主循环继续。seed 成功只说明采样前提存在，最终仍由 `distfs_total_checks` 与完整 metric gate 判断。
- Non-Functional Requirements:
  - NFR-S10REAL-1: 长跑脚本可重复执行，输出目录结构稳定。
  - NFR-S10REAL-2: 关键告警（`apply settlement envelope failed` / `verify settlement signature failed` / `settlement signer public key mismatch`）在通过样本中计数为 0。
  - NFR-S10REAL-3: 门禁判定在 run 结束后 5 分钟内可完成并产出摘要。
  - NFR-S10REAL-4: 短跑样本（130~260s）与长跑样本（600~1200s）均可用于回归比较。
  - NFR-S10REAL-5: 手册、脚本、项目文档路径与命名保持一致。
- Security & Privacy: 节点签名与身份绑定使用派生密钥规则；运行日志仅保留诊断所需字段，不暴露敏感密钥材料。
  Probe seed 也不得包含凭据、私钥、生产 endpoint 或玩家敏感数据。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (S10REAL-0): 建档并定义 S10 范围与门禁口径。
  - v1.1 (S10REAL-1/2): 五节点编排脚本 + 基础门禁/产物落地。
  - v2.0 (S10REAL-3~9): 依次收敛 epoch、mint、stall、验签、结算失败率等核心风险。
  - v2.1 (S10REAL-10): 连续多轮长跑验证并发布收口。
  - v2.2 (S10REAL-11): strict schema 文档迁移与命名统一。
- Technical Risks:
  - 风险-1: 单机五进程竞争导致指标抖动。
  - 风险-2: provider-backed 场景仍有 LLM 外部非确定性，但当前 S10 的 `--llm/--no-llm` 为无行为效果的兼容参数，不构成真实 LLM 覆盖或开关；需要该覆盖时必须使用适用的真实 provider 验收入口，不能以本套件通过代签。
  - 风险-3: 无 chaos 场景下恢复能力覆盖有限（由 S9 chaos 兜底）。
  - 风险-4: auto-attest 策略配置不当影响真实投票覆盖。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-LONGRUN-S10REAL-001 | S10REAL-0/1/2/3 | `test_tier_required` | 脚本帮助/语法/dry-run + 目录产物字段核验 | S10 执行入口与证据结构 |
| PRD-TESTING-LONGRUN-S10REAL-002 | S10REAL-4/5/8/9/10 | `test_tier_required` | 短跑与长跑样本审查 summary 指标与关键告警计数 | 发布门禁可信性 |
| PRD-TESTING-LONGRUN-S10REAL-003 | S10REAL-6/7/8/11 | `test_tier_required` | stall/非连续高度/验签失败回归 + 文档治理检查 | 五节点稳定性与可维护性 |
| PRD-TESTING-LONGRUN-S10REAL-004 | S10REAL-12 | `test_tier_required` | 空集 seed 幂等/失败非阻断 + summary `insufficient_data` 边界核验 | DistFS probe 样本与 gate 解释 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-S10REAL-001 | 使用单节点拓扑多进程编排五节点 | 增加新的内建拓扑枚举 | 降低实现入侵，快速落地可复用脚本。 |
| DEC-S10REAL-002 | 默认 no-LLM + sequencer_only auto-attest | 默认全节点 auto-attest + 强依赖 LLM | 优先稳定基线，控制外部非确定性。 |
| DEC-S10REAL-003 | 结算 signer 统一按 node_id 派生并使用 CBOR 传输 | 保留 root 特殊签名 + 仅 JSON 载荷 | 消除跨节点验签不一致与编码失真。 |
| DEC-S10REAL-004 | 将 settlement apply failure ratio 纳入硬门禁 | 仅观测不拦截 | 发布风险可量化并可前置阻断。 |
| DEC-S10REAL-005 | 空集时生成最小 probe seed，但仍由完整 metric gate 判定 | 降低阈值或把 seed 成功直接视为 pass | 补齐可观测前提而不削弱门禁。 |

## 原文约束点映射（内容保真）
- 原“目标：S10 五节点真实数据流长跑、发布门禁补充” -> 第 1 章 Problem/Solution/SC。
- 原“In/Out of Scope（脚本新增、五节点拓扑、无新拓扑枚举/无 chaos）” -> 第 2 章 AC/Non-Goals。
- 原“接口/参数、validator stake、auto-attest 策略、状态隔离” -> 第 2 章规格矩阵 + 第 4 章 Integration/Edge Cases。
- 原“S10 指标门禁（stall/lag/distfs/settlement/mint/invariant）” -> 第 2 章规格矩阵 + 第 4 章 NFR/错误处理。
- 原“里程碑 M0~M3 + 后续修复演进” -> 第 5 章 phased rollout（S10REAL-0~11）。
- 原“风险（资源竞争、LLM 非确定性、chaos 覆盖、auto-attest 覆盖）” -> 第 5 章 Technical Risks。
- 原“结算发布条件、mint 前置绑定、非连续高度容错、签名编码修复” -> 第 4 章 Edge Cases & Error Handling。

<a id="reward-timeline-source-contract"></a>
## Reward 逐样本时间线合同

`timeline.csv` 的 DistFS `total_checks`、`failed_checks`、`distfs_failure_ratio` 必须来自该节点该次实际 reward-runtime 样本，不能写固定零或用聚合摘要代替；S10 的 `settlement_apply_attempts_total`、`settlement_apply_failures_total`、`settlement_apply_failure_ratio` 同理。按 CSV header 名逐列解释，不依赖读者猜测列位置。

正分母时 ratio=failures/attempts；0/0 是已知无样本，保留真实 0 计数；比率列留空并按既有 insufficient_data 规则解释，不能当成成功。metrics unavailable 时对应数字列留空，并在同次 failures/summary 保留 unavailable/缺样本告警，不能伪造成有效 0/0。CSV 消费者须兼容空数值，不能默认解析为 0。现有累计聚合和 gate 的算法/阈值保持其独立合同。

离线 exporter fixture 覆盖 5/0→0、5/1→0.2、0/0、missing metrics 和 S10 的 8/1→0.125，按 header 回读后核对原始样本及时间线；不运行真实节点或结算。文档补齐不表示 exporter 已改，也不以摘要正确代签逐样本输出。
