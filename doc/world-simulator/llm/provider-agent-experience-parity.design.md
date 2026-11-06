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

<a id="parity-declaration-consumption"></a>
### 6.3 支持声明的来源、版本与入口接收

本节承接 REQ-AGENT-PARITY-001 的声明消费，复用 §6.1/6.2 的证据与判据。它是待实施的组合合同；目前存在的配置、Hello、benchmark summary 或可运行 driver 均不自动成为正式支持声明。

**唯一来源及发布。** Agent owner 按[driver 合同](external-agent-runtime-adapter.design.md#adapter-drivers)固定实际 Runtime/driver/schema、模型/profile、本机权限和已验证平台；QA 与 producer 按原 parity 规则分别给出证据审读及适用结论；发行 owner 将获准的准确范围随[原可信 Skill/worker 发布链及支持范围 manifest](external-agent-runtime-adapter.design.md#adapter-skill)发布。首次发布、修订、撤回和替代均由该原发布链产生新版本，保留被替代版本与证据引用。入口只读并验证该来源，不能从本地探测生成批准、用同一不可信下载站的 checksum 自证，或替 QA/producer 把实验结论升级。本文不新增 Provider 认证制度、世界权限账或独立发布服务。

支持范围 manifest 中的每条声明提供以下语义记录；SDK、Launcher、Viewer 与纯 API 的适配器可使用各自现有结构，但必须保持完整关联。已有产品支持范围和原阶段是输入，不能由该记录增加可玩承诺。

| 记录组 | 原生产者及必须可核对的内容 | 接收规则 |
| --- | --- | --- |
| 声明身份 | 可信发布来源、source commit、资产内容 hash、manifest/schema 版本、声明记录 ID/revision、原批准结论及其依据引用；替代/撤回关系 | 固定本次读取到的完整版本；显示来源与版本。文件名、显示名或本地配置不是身份，缺失或无法验证时不能显示已支持 |
| 精确组合 | Runtime/version、driver/adapter 与协议 schema、model/profile revision、Skill/API 版本、实际平台、primary mode、使用表面与 observation/execution lane；不适用项显式记不适用 | 按声明的明确精确值或已发布兼容范围匹配，不能自己放宽版本范围。`viewer/pure_api` 与 Web/native、执行 lane 分开；普通 HTTP 客户端不适用 Runtime/Provider 时不虚造依赖 |
| 适用能力与条件 | 原声明场景/fixture、世界或规则版本适用范围、必要能力、观察/权限范围、本机 sandbox/approval 条件、明确排除项与原支持阶段 | 只解释该组合和条件；当前身份仍须取得原世界权限。不能删去缺失的必要能力后声称整组支持 |
| 证据与结论 | §6.1 的固定候选、输入与辅助标记、paired artifacts、Runtime failure/restart/reconnect、QA/producer scorecard 引用；原行为门槛、latency class、缺项及限制 | 自动指标、专业 parity 结论、实际入口能力和发行阶段分别保留。`A` 只是必要条件，`B` 不默认启用；旧 smoke 或另一组合的证据不能填空 |
| 当前入口观察 | 原入口报告的 backend/版本、已协商能力、当前合法身份/权限、依赖可用性及读取时点；来源分别为 driver、入口协议和世界 authority | 与声明逐项对照；probe/Hello 只证明所观察层。读取失败保留“声明/当前状态未确认”及已有可用的查询/修复动作，不伪造拒绝或自动 observer |

**消费顺序。** 打开配置、选择声明组合或启动前，入口先解析受信发布记录，再读取实际组合及当前能力；先核匹配与来源，后呈现结论。声明和 probe 来自不同 authority 时分别标时，不合成一个原子世界状态。改变 Runtime/model/profile/lane、入口版本或必要能力后重新匹配；已签原操作保留原身份及旧声明 provenance，不随新选择改写。进入新组合仍沿原显式启动/授权流程，不能借声明更新自动开启模型、付费调用或世界行动。

| 对照结果 | 玩家或机器得到的结论 | 真实下一步与保留能力 |
| --- | --- | --- |
| 来源有效、组合匹配、证据完整，实际必要能力可用 | 显示原声明阶段和准确范围；只有原批准明确允许的默认/受限选择才可被如此呈现 | 沿当前入口已有显式开始/继续；当前权限和预算继续独立校验 |
| 声明允许 experimental/受限试点 | 显示限制、适用场景和原因；不称整体等价或默认就绪 | 使用原允许的受限入口，或查看受信说明；不自动换模型/Runtime |
| 缺 paired/评分/恢复证据，或组合未纳入 | parity/支持结论未确认或受限，并列明具体缺项/不匹配字段 | 查看声明版本与限制、刷新受信声明或选择已发布且本人获准的组合；原合法只读/历史查询和独立已支持能力按各自 authority 保留 |
| 声明匹配，但当前必要权限、能力或 Provider 依赖失败 | 只阻塞真实依赖失败的组合/动作，显示原拒绝原因 | 沿既有重新认证、修正配置、重试检查或停止入口；无 Provider 依赖的普通 HTTP 不因此被阻断 |
| 来源不可读、完整性不符、声明被撤回或被替代且不再适用 | 不把旧缓存当当前支持；旧记录明确为历史来源，当前结论未确认/不适用 | 重读同一受信发布链及实际状态；无安全恢复时明确停止该新启动。不会删除旧任务、取消已提交工作或重发未知原件 |

**具体消费者。** Launcher 在[配置/运行对照及状态文案](../launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-uiux-state-copy)接收本次目标组合的声明与实际依赖，不把运行成功合成支持通过；Viewer 的模型/执行方设置与诊断沿[实际执行方与预算](agent-game-client-boundary.design.md#des-agc-005)展示同一来源/范围/限制，主界面只给适用摘要和真实 CTA；`pure_api` 沿[逐入口适用合同](external-agent-game-api.design.md#api-primary-entry-applicability)在结构化结果/诊断提供同一事实。只有实际存在相应选择/启动能力的表面才接收该动作，不为 Launcher 新增 pure_api 按钮，也不要求普通 HTTP 启动模型。各端内部布局和传输适配不同，不允许一个端的成功代签另一端。

**有界性与恢复。** 一次目标投影只处理所选组合的一条完整声明，编码后上限 64 KiB；引用指向既有受信发布物，不把原始 trace、私有 Prompt 或秘密打包进 UI。原 manifest、客户端、HTTP/command 的更小限制优先；超限不得截掉能力或证据后报完整，应停止该声明消费并由原发行流程提供可独立核对的有界记录。只读刷新复用原入口节奏，不启动模型或产生世界费用。网络恢复先核当前声明/实际组合及原操作；已发送 unknown 仍沿[原操作恢复](agent-game-client-boundary.design.md#original-operation-lifecycle-port)查询，Provider 切换沿 PSW 原合同，不靠换声明、换 nonce 或换 Provider 洗掉旧结果。记录的具体生成/发布适配器、各端读取接线及真实 paired/玩家验证仍待实施；上述生产者、匹配和错误规则已经固定，不能以当前未接线省略目标投影。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | 只有声明组合与同条件证据能支持体验连续性；专业指标沿用[配对PRD](provider-agent-experience-parity.prd.md#parity-metric-object-contract)。 | [证据身份](#parity-scope-evidence) | agent_engineer 采集，QA/producer 审读；Runtime 提供权威结果。 | 不证明全玩法、跨Runtime代签或默认发布。 |
| [AC-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | 超范围或缺证据保留受限/blocked结论及真实下一步。 | [扩面边界](#parity-rollout-boundary) | QA/producer 决策，Viewer 消费结论。 | 不承担具体界面、切换去重或首局全部旅程。 |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | 原发布声明的来源/版本、精确组合、证据与当前能力由实际入口完整接收，失败保留合法下一步 | [声明消费](#parity-declaration-consumption) | Agent/QA/producer/发行原主责；Launcher、Viewer、API 消费者 | 不新增认证、支持范围、世界写权或默认发布结论 |

## 11. 验证设计

以下是固定候选上待执行的方法，不是本次已运行或通过的声明。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | [设计条款](#parity-scope-evidence) | 同条件比较和独立指标计数 | required 执行 `bash scripts/provider-parity-p0-runtime-perf.test.sh`，来源[离线聚合回归](../../../scripts/provider-parity-p0-runtime-perf.test.sh)：断言profile/route、非法schema缺项及计数不一致阻断，恢复happy/timeout-only/goal-flag-only/partial/zero/malformed/wrong-origin/missing-identity结果。它构造sample JSON，不调用真实provider或验证玩家体验。full计划运行 `./scripts/provider-parity-p0.sh --scenario llm_bootstrap --scenario-id P0-001 --parity-tier P0 --samples 3 --ticks 20 --agent-provider-profile oasis7_p0_low_freq_npc --out-dir output/provider_parity/candidate`；真实provider/模型配置另行固定，Runtime证据和双角色评分另取，不由脚本自动签署。 | 保存实际命令、候选、fixture/组合、原始计数与结论；负例输出单独归档。 | 文档检查不证明运行通过；mock 不证明真实Runtime或发布资格。 |
| [AC-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#ac-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | [设计条款](#parity-rollout-boundary) | 缺证据和超范围时限制结论 | 执行同一[离线聚合回归](../../../scripts/provider-parity-p0-runtime-perf.test.sh) `bash scripts/provider-parity-p0-runtime-perf.test.sh`，检查simulator_world_kernel/not_certified下即使benchmark passed、latency A或B仍为parity/release blocked。现有聚合器的Runtime认证保持阻断，不能证明B进入experimental或A进入default。缺任一真实paired/评分、超范围组合及玩家下一步呈现为待补full场景；只有未来完整证据链成立后才分别验证§6.2的准入条件。 | 保存实际命令、候选、fixture/组合、原始计数与结论；负例输出单独归档。 | 文档检查不证明运行通过；mock 不证明真实Runtime或发布资格。 |
| [REQ-AGENT-PARITY-001](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-001)；Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6 | [声明消费](#parity-declaration-consumption) | 声明与实际入口的来源/版本/能力不混同；未知与漂移安全恢复 | [声明消费验证](../../testing/manual/external-agent-runtime-contract-validation.manual.md#parity-declaration-consumption-validation)：PD01–PD08，固定候选分别验证实际声明表面的 producer/reader 与独立负例 | 声明原件/版本、入口观察、显示/结构化输出、旧请求及零新调用/效果记录 | fixture 不证明真实 Provider、完整 parity、当前支持或发行 |

## 12. 决策与残余风险

保留自动指标与主观评分并列的双重门槛，避免技术接通被提升为体验承诺。场景漂移、辅助条件不一致和小样本波动仍是风险；agent/QA owner 在 adapter、profile、prompt 或 rollout 范围变化时重新对照配对 PRD 与本节验证，不复用旧结论代签新组合。
