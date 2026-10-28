# Local Provider 与内置 Agent 体验等价（parity）验收方案（2026-03-12）设计

- 对应需求文档: `doc/world-simulator/llm/provider-agent-experience-parity.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义 `Local Provider` 与内置 agent 的体验等价验收框架，以标准化 parity 结果而非单次 PoC 或主观印象判断“是否可默认启用/是否可扩大覆盖范围”。

## 2. 设计结构
- 场景层：按 `P0 低频单 NPC`、`P1 多轮记忆`、`P2 多 agent 并发` 分层。
- 指标层：采集完成率、无效动作率、非法 schema 率、超时率、额外等待时间、trace 完整度、恢复率。
- 非法 schema 率独立统计 `invalid_action_schema`：sample 级 `illegal_schema_count / decision_steps`，并在聚合 summary 与 `combined.csv` 保留 `illegal_schema_rate` object。门禁为不高于 3% 且不超过 builtin 的 2 倍；计数、分母或 metric object 缺失/不一致时标记 `blocked`，不得按缺失为零。
- 评分层：组合自动 benchmark 与 QA/producer 主观评分卡。
- 准入层：基于通过线/阻断线判定 `experimental`、`gated`、`default_ready`。
- 追溯层：记录 provider 版本、adapter 版本、协议版本与场景结果。

## 3. 关键接口 / 入口
- parity 场景列表与评分卡
- builtin vs Local Provider 对照结果汇总
- `Decision Provider` trace 与 diagnostics
- launcher / viewer provider 状态展示

## 4. 约束与边界
- parity 目标针对“用户体验”，不要求内部实现完全一致。
- 未通过 parity 的 provider 不得标记为默认体验。
- 首期 parity 仅覆盖低频和中低复杂度场景，不扩展到高频战斗/经济关键路径。
- 自动指标与主观评分必须同时具备。

## 5. 设计演进计划
- 先冻结 P0/P1/P2 场景和指标阈值。
- 再补自动 benchmark 与评分卡。
- 再执行真实 `Local Provider(Local HTTP)` 对标试玩。
- 最后基于 parity 结论决定是否进入默认体验。

## 6. 当前基线、目标与证据范围

审读基线为 `4e94163a3dff9ba69f6959b406ca3fcedf8aa097`；专业 owner 为 `agent_engineer`、验收 owner 为 `qa_engineer` 与 `producer_system_designer`。本次只补齐既有合同的承接与验证方法，不改变阈值、provider 支持清单或实现状态。

<a id="parity-scope-evidence"></a>
### 6.1 声明场景与证据身份

每份结果绑定 Runtime/version、model、adapter、profile、primary access mode、observation lane、fixture/seed 与候选版本。builtin 与 Local Provider 在同批输入下比较；额外动作修正或脚本辅助必须分别标注。P0/P1/P2 是 parity 场景分层，不代替产品首期范围。当前 simulator-world benchmark 必须保留 `execution_authority=simulator_world_kernel` 与 `runtime_certification_status=not_certified`；per-agent shadow receipt 不成为同一世界的 Runtime certification。

自动数据、真实 paired artifacts 和双角色评分分别保存，不将缺失当作零或通过。恢复率按配对 PRD 的 error occurrence/合法 recovery chain 计数，零分母是 not_evaluable；非法 schema 指标按 decision_steps 计数。输入缺失、身份不一致、错误链串联或 metric object 不完整时阻断聚合，不缩小分母规避失败。

<a id="parity-rollout-boundary"></a>
### 6.2 受限结论与扩面边界

行为等价硬门禁未通过或证据不足时不得默认启用；行为通过而 latency_class=B 仅保留 experimental/受限试点，A 只是默认候选的必要条件。

真实 Runtime receipt/journal 的 failure/restart/reconnect 证据和 QA/producer 评分仍须独立齐备。输出明确缺少的组合、失败类别和下一步补测；未覆盖组合显示 limited/blocked，不以静默替代或历史 smoke 冒充等价体验。

Viewer 的具体呈现和 provider 切换在途意图由其专业设计负责，本节不声称已完成这些界面或切换实现。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | 只有声明组合与同条件证据能支持体验连续性；专业指标沿用[配对PRD](provider-agent-experience-parity.prd.md#parity-metric-object-contract)。 | [证据身份](#parity-scope-evidence) | agent_engineer 采集，QA/producer 审读；Runtime 提供权威结果。 | 不证明全玩法、跨Runtime代签或默认发布。 |
| [AC-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | 超范围或缺证据保留受限/blocked结论及真实下一步。 | [扩面边界](#parity-rollout-boundary) | QA/producer 决策，Viewer 消费结论。 | 不承担具体界面、切换去重或首局全部旅程。 |

## 11. 验证设计

以下是固定候选上待执行的方法，不是本次已运行或通过的声明。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | [设计条款](#parity-scope-evidence) | 同条件比较和独立指标计数 | required 执行 `bash scripts/provider-parity-p0-runtime-perf.test.sh`，来源[离线聚合回归](../../../scripts/provider-parity-p0-runtime-perf.test.sh)：断言profile/route、非法schema缺项及计数不一致阻断，恢复happy/timeout-only/goal-flag-only/partial/zero/malformed/wrong-origin/missing-identity结果。它构造sample JSON，不调用真实provider或验证玩家体验。full计划运行 `./scripts/provider-parity-p0.sh --scenario llm_bootstrap --scenario-id P0-001 --parity-tier P0 --samples 3 --ticks 20 --agent-provider-profile oasis7_p0_low_freq_npc --out-dir output/provider_parity/candidate`；真实provider/模型配置另行固定，Runtime证据和双角色评分另取，不由脚本自动签署。 | 保存实际命令、候选、fixture/组合、原始计数与结论；负例输出单独归档。 | 文档检查不证明运行通过；mock 不证明真实Runtime或发布资格。 |
| [AC-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | [设计条款](#parity-rollout-boundary) | 缺证据和超范围时限制结论 | 执行同一[离线聚合回归](../../../scripts/provider-parity-p0-runtime-perf.test.sh) `bash scripts/provider-parity-p0-runtime-perf.test.sh`，检查simulator_world_kernel/not_certified下即使benchmark passed、latency A或B仍为parity/release blocked。现有聚合器的Runtime认证保持阻断，不能证明B进入experimental或A进入default。缺任一真实paired/评分、超范围组合及玩家下一步呈现为待补full场景；只有未来完整证据链成立后才分别验证§6.2的准入条件。 | 保存实际命令、候选、fixture/组合、原始计数与结论；负例输出单独归档。 | 文档检查不证明运行通过；mock 不证明真实Runtime或发布资格。 |

## 12. 决策与残余风险

保留自动指标与主观评分并列的双重门槛，避免技术接通被提升为体验承诺。场景漂移、辅助条件不一致和小样本波动仍是风险；agent/QA owner 在 adapter、profile、prompt 或 rollout 范围变化时重新对照配对 PRD 与本节验证，不复用旧结论代签新组合。
