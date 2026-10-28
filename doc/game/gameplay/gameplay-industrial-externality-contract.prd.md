# 工业外部性与运营缓解合同

- 上层产品映射：承接产品层可选的 `industrial_externality` profile；本合同只把 profile 约束转成生产前的玩家取舍，不把污染/环境效果写成当前普遍能力。
- 产品语义入口：[`工业外部性与操作性缓释 profile`](../../product/world-rules-core-gameplay/prd.md#工业外部性与操作性缓释-profile（target-only）)；本合同保留 target-only profile 的玩家取舍与证据边界。
- Authority 分工：产品层拥有 profile 是否存在、作用范围与完成承诺；M4/domain 拥有实际 externality fact、`operational_mitigation` 容量、ledger/impact disposition；`world-runtime` 拥有事件、状态、持久化、排序与 replay；gameplay 拥有玩家动作、机会成本、失败恢复与 progression。
- 边界：`industrial_externality` 是工厂/配方/operation 的可选运营 facet，独立于 batch quality/custody、byproduct identity/disposition、maintenance、production/delivery finality 与 systemic-crisis containment；不得把本合同的 operational mitigation 解释成区域治理权或危机处置权。

## 1. Profile 与证据边界

只有 recipe/factory profile 明确声明存在 externality obligation、受影响 scope、必要的 `operational_mitigation` authority 与评估边界时，才可进入本合同。profile 可以声明“无外部性”，也可以声明需要既有 containment/mitigation；本文件不规定累积、衰减、扩散、清理或阈值公式。profile-less legacy recipe/factory 保持既有路径，不被本合同追溯加闸。

声明 profile 的 mandatory authority 缺失、过期或冲突时，当前 run 必须显示 `industrial_externality: unknown/blocked` 并禁止不可逆排程；不得从材料名称、batch contamination、radiation-source 数据、客户端缓存或 Agent 推荐推断安全。没有声明 profile 时不虚构 externality，也不虚构 mitigation 成本。

## 2. 玩家循环

| 阶段 | 玩家动作 | 收益、失败成本与下一动力 |
| --- | --- | --- |
| 评估 | 查看当前 recipe/factory 的 externality profile、scope、mandatory mitigation、已占用/可用能力、production/terminal 影响与 `next_recheck` | 立即知道这次运行是否需要运营缓解及其机会成本；unknown/blocked 不可排程，下一步是补齐 authority、恢复缓解能力或延期 |
| 比较 | 只比较 profile 真实支持的 `run_with_existing_mitigation`、`run_reduced_or_defer`、`hold_for_treatment_or_legal_disposition` | 维持缓解可保住吞吐/目标但占用 mitigation、power、space 或时间；减产/延期保留 headroom 但延迟交付；hold/处理避免释放风险但占用 buffer 并承担等待成本；未声明 treatment/repair/legal disposition 不得展示 |
| 确认 | 玩家确认一个选项；提交前 fresh revalidate factory、recipe、profile、mitigation capacity 与既有 terminal obligations | 成功只形成一次可追溯 operation intent；profile/容量/终端漂移只能 fresh requote 或无 sink 的 atomic reject，不得静默运行、降级、清理、改道或给予区域 credit |
| 结算/恢复 | 读取 production、externality disposition 与 terminal 分层结果；遇 blocker 时执行 profile 支持的 hold、reduce、defer 或 mitigation recovery | known-contained run 至多产生一次 linked production result 与一次 externality disposition；失败保留已消费/占用/损失价值，WIP/transit/buffer/terminal-pending 按 profile 单次处置；安全运行完成后动力是交付、服务或下一配方，而非免费重复吞吐 |

`production`、`externality/mitigation` 与 `delivery/terminal` 是独立事实：外部性处置不能改变材料数量/质量，production receipt 不能冒充 mitigation、cleanup 或 terminal settlement。只有 profile 明确的 boundary 才能产生对应 progression；本合同不授予 regional credit、治理资格或危机 containment 权利。

## 3. 失败恢复与状态守恒

- mitigation authority、容量、scope 或 terminal obligation 在首个不可逆 sink 前不成立时，只能 `unknown/blocked`、fresh requote 或 atomic reject；不能 overdraft、自动补足、免费清理、静默降产或把风险转嫁成成功。
- 已产生 WIP、in-transit、buffer-held、terminal-pending 或 production effect 后，只能沿 profile 声明的 hold、treatment、legal disposition、reduce、defer、return/reroute/handoff/reject 或其他 mitigation recovery 各处置一次；terminal-pending 必须保留 quantity、recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置，不得静默改绑或提前 settlement。其他状态保留 root、revision、batch、edge、destination 与实际损失；未声明的动作不展示。
- externality facet 的变化不能改写 quality/custody、byproduct、maintenance 或 delivery receipt；改变 factory/recipe/operation 的产出因果时，沿既有 candidate/cutover 规则建立 parent-linked 新候选并从 `W=0` 开始。

## 4. Current/target evidence cutline

当前证据没有 factory/recipe externality profile、mitigation ledger 或 industrial impact receipt；runtime 的 `LocationProfile.radiation_emission_per_tick` 与辐射采集计算只是 radiation-source 窄路径，不能支持工业 externality current claim。因而本合同当前只能标为 `target-contract`，不能宣称生产已受污染/缓解规则治理。

目标 evidence 需要 fresh composite fixture，证明同一 root/revision 从 profile 读取、玩家选择、提交、生产、externality disposition 到 terminal/下一动作全程可追溯；M4/runtime/QA 必须提供 authority、容量、receipt、持久化与 replay 证据后，才能把某个 profile 标为 current-evidence-backed。

## 5. Exactly-once 与跨 surface parity

- 同一 root、recipe/factory operation、profile 与 authority revision 至多产生一次 intent、production result、externality/mitigation disposition、progression 或 reward；重复提交、重连、Agent retry、restore、事件乱序与 replay 只能重读原处置。
- preview、accepted、production、externality/mitigation、terminal pending 与 settled 不得互相冒充；外部性处置不自动减少需求、发放交付奖励或释放未结算容量。
- Viewer、pure API 与 Agent 必须从同一 snapshot/revision 表达 profile、scope、状态、primary blocker、已占用/已损失价值、可用动作、`next_action`、`next_recheck` 与 progression；无 authority 时均返回 `unknown/blocked`，不能生成安全候选。

## 6. Required / full acceptance

`test_tier_required` 至少覆盖：

- profile-less legacy unchanged；声明 `industrial_externality` 且 known-contained、mitigation capacity full、mandatory authority unknown 四态；
- preview 无世界效果；
- `run_with_existing_mitigation`、`run_reduced_or_defer`、`hold_for_treatment_or_legal_disposition` 只有 profile 支持时可选并披露吞吐、buffer、时间与下一复查代价；
- 提交 fresh revalidation 与 drift atomic reject；
- production、externality disposition、terminal settlement 分层；
- WIP/transit/buffer/terminal-pending 单次处置，terminal-pending 的 quantity/recipient/destination/admission/capacity obligation/provenance 与 settlement 边界保持不变；
- 重复 submit、reconnect、Agent retry、restore、reorder、replay 不复制效果；
- Viewer/pure API/Agent parity。

`test_tier_full` 延后至 M4/runtime 的多阶段、长时与区域影响 authority；未来可覆盖多个 operation 争用 mitigation、profile cutover、持久化恢复及 compensation，但不得把该目标测试写成当前实现通过。

## 7. Non-goals 与 residual risk

本合同不定义污染/温度/环境公式、积累/衰减/扩散半径、清理经济、税费、区域 credit、治理/宪章权利、危机 containment、自动 mitigation/cleanup/downgrade/reroute/refund，不新增 runtime/API/ABI/schema/action/UI/Agent 行为，不改 batch quality/custody、byproduct、maintenance、settlement、starter、site、capacity、source、recipe、demand、congestion、service-window、checkpoint/review 或 changeover 合同。

残余风险是当前没有工厂外部性事实与组合 runtime 证据；`containment`、`treatment` 与 `legal disposition` 容易被误读成系统性治理能力。任何 current/release claim 都必须等待 producer、M4/runtime、gameplay 与 QA 的共同复核；profile 缺失时保持 legacy unchanged，profile authority 不明时保持 `unknown/blocked`。
