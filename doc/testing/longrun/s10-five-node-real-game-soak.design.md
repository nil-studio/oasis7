# oasis7：S10 五节点真实游戏数据在线长跑套件设计

- 对应需求文档: `doc/testing/longrun/s10-five-node-real-game-soak.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

## 1. 设计定位
定义 S10 五节点真实游戏数据在线长跑套件，统一五节点编排、真实游戏数据流、结算/资产指标、存储/共识健康采样与发布门禁证据。
S10 本身不引入 chaos 编排；恢复、故障注入和 state-sync/commit claim boundary 由 S9 长跑与 GWSC 方案关联覆盖。

## 2. 设计结构
- 五节点编排层：定义 sequencer、storage、observer 节点拓扑、运行时长和脚本入口。
- 游戏数据流层：覆盖 gameplay 数据、reward/settlement、mint 与资产不变量。
- 稳定性观测层：采集 committed progress、lag、DistFS、settlement apply 与关键告警。
- reward-runtime 观测只消费 `/v1/chain/status.reward_runtime` 的原生快照：每个
  可用节点取累计最大值后聚合，缺失值显式告警；`distfs_total_checks=0` 与比例
  超阈值分离，且只有 `invariant_ok=false` 可分类为资产不变量违规。
- DistFS probe 预热层：reward worker 启动时仅在 blob set 为空时写入幂等、非敏感 seed；失败局部记录而不阻断主循环。
- 验收归档层：沉淀 `summary.json`、`summary.md`、`timeline.csv`、节点日志、失败签名与门禁结论。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Reward 时间线](s10-five-node-real-game-soak.prd.md#reward-timeline-source-contract) | 原始计数、零分母与缺失值逐列一致 | [导出设计](#reward-timeline-source-design) | QA / reward-runtime 维护者 | 不修改奖励与门禁，不代签真实节点 |

## 3. 关键接口 / 入口
- `scripts/s10-five-node-game-soak.sh`
- `crates/oasis7_chain_runtime/src/host.rs`
- S10 `summary.json` / `summary.md` / `timeline.csv` 证据产物
- GWSC claim boundary: `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`

## 4. 约束与边界
- 长跑场景需可重复执行、可比较。
- S10 不负责 chaos 注入编排；需要故障恢复覆盖时升级到 S9/GWSC 关联套件。
- S10 五进程或本地五节点结果不得冒充 real-env/public_testnet readiness。
- probe seed 只让 `distfs_total_checks` 具备产生样本的前提；它不改变算法阈值，不把 `insufficient_data`、其他门禁失败或缺少同窗口证据升级为 pass。
- `running_false`、`http_failure` 与 status 不可达是独立的运行/采样失败，不得
  在 chaos 窗口内改写为 invariant violation；status 不可达须终止门禁判定并保留
  failures 证据。
- 不在本专题扩展新的线上编排平台。

## 5. 设计演进计划
- 固定五节点长跑场景、节点拓扑与探针。
- 固化 settlement/mint/DistFS/lag 门禁指标。
- 固化空集 seed 的幂等、非阻断与非证明边界。
- 与 GWSC 方案对齐 state-sync/commit claim boundary。

<a id="reward-timeline-source-design"></a>
## Reward 时间线导出

产品适用性：N/A；reason=本节只规定已采集诊断计数到 CSV 的导出和空值解释，不改变奖励、结算或共识规则；scope=reward timeline exporter 的逐列序列化。真实节点、消费者、长跑与恢复验收不在此不适用范围，仍按原专业合同独立执行。

采样层将同一 node/time 的 reward 计数传给 exporter，按固定 header 写入 `total_checks`、`failed_checks` 和比率；S10 还传 settlement attempts/failures。正分母用实际计数计算比率，已知 0/0 保留零计数与空比率，缺失指标保留空数值并回链同次 unavailable 告警。CSV 解析须区分空值与 0，不以独立 aggregate/gate 正确为逐行正确的证明。

本节为 exporter 和消费者应共同满足的设计，不声称当前脚本已修复；不改变 reward 计算、结算、累计摘要聚合或既有门禁。验收按列回读 5/0、5/1、0/0、missing 和 S10 8/1 的离线 fixture，不启动节点。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Reward 时间线](s10-five-node-real-game-soak.prd.md#reward-timeline-source-contract) | [导出设计](#reward-timeline-source-design) | 同次样本、header、分母、空值一致 | 候选版本[现有验证入口](../../../scripts/s10-five-node-game-soak-summary.test.sh)，补充逐行 5/0、5/1、0/0、missing、S10 8/1 fixture；原有 summary 断言不代签新 exporter 回归 | 实施 PR 的输入、CSV、解析断言与实际退出结果 | 本次文档修订不表示回归已运行，不证明真实长跑或结算 |

## 范围、结构边界与风险

本设计范围是五节点真实游戏数据长跑的编排、采样和门禁证据结构。主要风险是把本地进程数当作真实网络拓扑、把 probe seed 当作健康证明，或用末次采样遮蔽 status/HTTP 失败；按下文保留各类失败和同窗口证据，不扩大当前运行结论。
