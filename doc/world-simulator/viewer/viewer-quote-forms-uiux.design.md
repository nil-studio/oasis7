# Viewer 十类报价表单与状态 UI/UX 设计

- Professional owner: `game_visual_interaction_designer`；Integration owner: `tpm`。
- 状态：专业呈现与交互补充；源证据与目标稿分开。尚未完成的图稿关系保持明确缺口，不是功能已发布声明。
- 固定源：PR #5277 `113868200d1f91bef1008babb689bd257f241428`；产品/Viewer源与审读main `5cc5c2b8a84c22f80cfa9ee16d893eaf6deedb08` 对应subtree相同。
- 配套：[派生目录](viewer-full-function-coverage-catalog.md)、[既有页面总览](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)。目录不是产品authority。

## 1. 问题、目标与非目标

本设计把已有源的字段、动作、条件及未解决的目标合同分开，使玩家能读清观察对象、当前权限、下一步与原结果。具体引用仅承接表中适用义务；组件存在、图稿数量、accepted和设计覆盖都不等于世界效果或运行验收。

不新增玩法/认证/治理能力，不改变后端权限、报价或结算语义；不代替产品PRD、实际服务capability或真实试玩。

## 2. 上游约束与相关角色

产品语义由各行唯一上游声明拥有；Viewer负责呈现、交互与失败/恢复信息，不接管identity、runtime、world或治理authority。现有品牌和终端层级继续适用。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | compoundMassG 输入；只读 quote/分类/缺口/关联与下一决策，非精炼提交 | [R-QUOTE-01](viewer-quote-forms-uiux.design.md#r-quote-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | compoundMassG 输入；只读 quote/分类/缺口/关联与下一决策，非精炼提交 | [R-QUOTE-01](viewer-quote-forms-uiux.design.md#r-quote-01) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 售电数量/预计收益/剩余电量/下步负担及建议，不等于售电效果 | [R-QUOTE-02](viewer-quote-forms-uiux.design.md#r-quote-02) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 当前电量、恢复选项与成本的已公布报价；不足/停机不伪装正常 | [R-QUOTE-03](viewer-quote-forms-uiux.design.md#r-quote-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | 当前电量、恢复选项与成本的已公布报价；不足/停机不伪装正常 | [R-QUOTE-03](viewer-quote-forms-uiux.design.md#r-quote-03) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | factory/recipe/batches；输入缺口/电量/前置步骤/风险只读 | [R-QUOTE-04](viewer-quote-forms-uiux.design.md#r-quote-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | factory/recipe/batches；输入缺口/电量/前置步骤/风险只读 | [R-QUOTE-04](viewer-quote-forms-uiux.design.md#r-quote-04) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | from/to ledger/material/amount/distance/priority/ordered route IDs/auto reroute；quote 不提交转运 | [R-QUOTE-05](viewer-quote-forms-uiux.design.md#r-quote-05) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | chunk x/y/z；replenishment_enabled/count/time uncertainty；不承诺未来必有资源 | [R-QUOTE-06](viewer-quote-forms-uiux.design.md#r-quote-06) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | aggressor/defender/intensity；freshness 与成本预览，宣战按钮固定禁用 | [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | aggressor/defender/intensity；freshness 与成本预览，宣战按钮固定禁用 | [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | proposal/option/weight；预览不是投票，提交未开放 | [R-QUOTE-08](viewer-quote-forms-uiux.design.md#r-quote-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | proposal/option/weight；预览不是投票，提交未开放 | [R-QUOTE-08](viewer-quote-forms-uiux.design.md#r-quote-08) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)；Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | material/amount；当前市场报价及建议只读，不表示成交或紧急保供授权 | [R-QUOTE-09](viewer-quote-forms-uiux.design.md#r-quote-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | material/amount；当前市场报价及建议只读，不表示成交或紧急保供授权 | [R-QUOTE-09](viewer-quote-forms-uiux.design.md#r-quote-09) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 产品/验证目标、输入条件与结果预估分离，不将请求成功当解锁 | [R-QUOTE-10](viewer-quote-forms-uiux.design.md#r-quote-10) | 对应产品owner；当前runtime/identity权限与发布字段 | 仅承接此表面读写边界，不覆盖该REQ其他入口或全部运行组合 |

## 3. 当前状态、目标状态与差距

`implemented_in_frozen_source` 只表示固定源码有实际消费者；条件显示不是未实现。`target_only` 只表示已有产品义务的设计目标；未确认专用Viewer入口不证明全仓后端不存在。QA与禁用占位不进入普通玩家当前功能分母。图中的示例数值不是真实世界结果。

## 4. 边界与结构

World / Targets / Command 是三个主Dock；Search在Targets内，Diagnostics/More为次级。Bevy是唯一世界renderer；图稿占位不是JS世界回退。当前主体、观察目标、模型连接、账号、session、admission、control分别表述。

## 5. 关键交互流程

先读对象与可用条件，再编辑草稿/请求预估，只有实际capability允许时才显式提交。pending/unknown保留原操作身份；切换观察对象不改写原回执。关闭弹层返回invoker，焦点/IME不得误触提交。具体feature条款优先于此共同顺序。

## 6. 字段、动作与具体表面

<a id="r-quote-01"></a>
### R-QUOTE-01 精炼报价与 preflight

- 固定来源：[refine_quote_preflight_card.jsx:1–220](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/refine_quote_preflight_card.jsx#L1-L220)。
- 实现/条件：`conditional_current`；compoundMassG 输入；只读 quote/分类/缺口/关联与下一决策，非精炼提交。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q05-refine_quote_preflight.svg](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 精炼量（克） | 必填 · 正安全整数 · g；视觉必填提示不宣称源码已有HTML required | 199 | `q05-refine_quote_preflight.svg#field-0` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-02"></a>
### R-QUOTE-02 售电预估与生存风险

- 固定来源：[power_sale_quote_card.jsx:1–31](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/power_sale_quote_card.jsx#L1-L31)。
- 实现/条件：`conditional_current`；售电数量/预计收益/剩余电量/下步负担及建议，不等于售电效果。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q03-power_sale_quote.svg](assets/ui-ux/full-function/quotes/q03-power_sale_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 买方 Agent | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 30 | `q03-power_sale_quote.svg#field-0` |
| 出售电量 | 必填 · 正安全整数 · PU；视觉必填提示不宣称源码已有HTML required | 30 | `q03-power_sale_quote.svg#field-1` |
| 每单位报价 | 必填 · 非负安全整数；视觉必填提示不宣称源码已有HTML required | 30 | `q03-power_sale_quote.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q03-power_sale_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-03"></a>
### R-QUOTE-03 电量生存与恢复预估

- 固定来源：[power_survival_quote_card.jsx:1–55](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/power_survival_quote_card.jsx#L1-L55)。
- 实现/条件：`conditional_current`；当前电量、恢复选项与成本的已公布报价；不足/停机不伪装正常。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q02-power_survival_quote.svg](assets/ui-ux/full-function/quotes/q02-power_survival_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 卖方 Agent | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 47 | `q02-power_survival_quote.svg#field-0` |
| 补电量 | 必填 · 正安全整数 · PU；视觉必填提示不宣称源码已有HTML required | 47 | `q02-power_survival_quote.svg#field-1` |
| 每单位报价 | 必填 · 非负安全整数；视觉必填提示不宣称源码已有HTML required | 47 | `q02-power_survival_quote.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q02-power_survival_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-04"></a>
### R-QUOTE-04 工厂配方排产预估

- 固定来源：[schedule_recipe_quote_card.jsx:1–27](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.jsx#L1-L27)。
- 实现/条件：`conditional_current`；factory/recipe/batches；输入缺口/电量/前置步骤/风险只读。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q09-schedule_recipe_quote.svg](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 工厂 | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 26 | `q09-schedule_recipe_quote.svg#field-0` |
| 配方 | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 26 | `q09-schedule_recipe_quote.svg#field-1` |
| 批次 | 必填 · 正安全整数；视觉必填提示不宣称源码已有HTML required | 26 | `q09-schedule_recipe_quote.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-05"></a>
### R-QUOTE-05 物料转运预估与路线取舍

- 固定来源：[transfer_material_quote_card.jsx:1–58](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/transfer_material_quote_card.jsx#L1-L58)。
- 实现/条件：`conditional_current`；from/to ledger/material/amount/distance/priority/ordered route IDs/auto reroute；quote 不提交转运。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q10-transfer_material_quote.svg](assets/ui-ux/full-function/quotes/q10-transfer_material_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 来源账本 | 必填 · 非空账本；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-0` |
| 目的地账本 | 必填 · 非空账本；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-1` |
| 物料 | 必填 · key iron_ingot；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-2` |
| 数量 | 必填 · 正安全整数；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-3` |
| 距离（公里） | 必填 · 非负安全整数 · km；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-4` |
| 优先级（可选） | select · standard / urgent；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-5` |
| 路线 ID（可选，按顺序） | textarea · 逗号或换行分隔；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-6` |
| 自动改道 | checkbox · 默认 false；视觉必填提示不宣称源码已有HTML required | 57 | `q10-transfer_material_quote.svg#field-7` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q10-transfer_material_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-06"></a>
### R-QUOTE-06 材料区块补充预览

- 固定来源：[fragment_refill_preview_card.jsx:1–71](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/fragment_refill_preview_card.jsx#L1-L71)。
- 实现/条件：`conditional_current`；chunk x/y/z；replenishment_enabled/count/time uncertainty；不承诺未来必有资源。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q04-fragment_refill_preview.svg](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 区块 X | 必填 · 安全整数；视觉必填提示不宣称源码已有HTML required | 60 | `q04-fragment_refill_preview.svg#field-0` |
| 区块 Y | 必填 · 安全整数；视觉必填提示不宣称源码已有HTML required | 61 | `q04-fragment_refill_preview.svg#field-1` |
| 区块 Z | 必填 · 安全整数；视觉必填提示不宣称源码已有HTML required | 62 | `q04-fragment_refill_preview.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-07"></a>
### R-QUOTE-07 战争声明预览

- 固定来源：[war_declaration_quote_card.jsx:1–37](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/war_declaration_quote_card.jsx#L1-L37)。
- 实现/条件：`conditional_current`；aggressor/defender/intensity；freshness 与成本预览，宣战按钮固定禁用。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q07-war_declaration_quote.svg](assets/ui-ux/full-function/quotes/q07-war_declaration_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 进攻联盟 | 必填 · 与防守联盟不同；视觉必填提示不宣称源码已有HTML required | 36 | `q07-war_declaration_quote.svg#field-0` |
| 防守联盟 | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 36 | `q07-war_declaration_quote.svg#field-1` |
| 强度 | 必填 · 整数 1–10；视觉必填提示不宣称源码已有HTML required | 36 | `q07-war_declaration_quote.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q07-war_declaration_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-08"></a>
### R-QUOTE-08 治理投票结果预览

- 固定来源：[governance_vote_quote_card.jsx:1–22](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/governance_vote_quote_card.jsx#L1-L22)。
- 实现/条件：`conditional_current`；proposal/option/weight；预览不是投票，提交未开放。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q08-governance_vote_quote.svg](assets/ui-ux/full-function/quotes/q08-governance_vote_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 提案 ID | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 21 | `q08-governance_vote_quote.svg#field-0` |
| 选项 | 必填 · 非空字符串；视觉必填提示不宣称源码已有HTML required | 21 | `q08-governance_vote_quote.svg#field-1` |
| 票权 | 必填 · 正安全整数；视觉必填提示不宣称源码已有HTML required | 21 | `q08-governance_vote_quote.svg#field-2` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q08-governance_vote_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-09"></a>
### R-QUOTE-09 市场材料决策预估

- 固定来源：[market_quote_decision_card.jsx:1–25](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/market_quote_decision_card.jsx#L1-L25)。
- 实现/条件：`conditional_current`；material/amount；当前市场报价及建议只读，不表示成交或紧急保供授权。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q06-market_quote_decision.svg](assets/ui-ux/full-function/quotes/q06-market_quote_decision.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 材料 | 必填 · 非空材料；视觉必填提示不宣称源码已有HTML required | 24 | `q06-market_quote_decision.svg#field-0` |
| 数量 | 必填 · 正安全整数；视觉必填提示不宣称源码已有HTML required | 24 | `q06-market_quote_decision.svg#field-1` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q06-market_quote_decision-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。

<a id="r-quote-10"></a>
### R-QUOTE-10 产品验证报价

- 固定来源：[product_validation_quote_card.jsx:1–109](https://github.com/nil-studio/oasis7/blob/113868200d1f91bef1008babb689bd257f241428/crates/oasis7_viewer/software_safe_src/product_validation_quote_card.jsx#L1-L109)。
- 实现/条件：`conditional_current`；产品/验证目标、输入条件与结果预估分离，不将请求成功当解锁。
- 字段/动作/状态：派生证据 `coverage-catalog.json` 中同ID保留精确源码控件、member表达式、条件与imperative证据；这些是静态消费者，不是后端DTO全枚举。
- 具体稿：[q01-product_validation_quote.svg](assets/ui-ux/full-function/quotes/q01-product_validation_quote.svg)；`normal_frame`，仅代表性结果。

| 当前输入 | 业务提示/当前差异 | 源行 | frame区域 |
|---|---|---|---|
| 产品 ID | 必填 · 非空 ID；视觉必填提示不宣称源码已有HTML required | 100 | `q01-product_validation_quote.svg#field-0` |
| 数量 | 必填 · 正安全整数；视觉必填提示不宣称源码已有HTML required | 101 | `q01-product_validation_quote.svg#field-1` |

- 状态片段：[empty](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-empty), [pending](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-pending), [error](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-error), [blocked](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-blocked), [stale](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-stale), [unknown](assets/ui-ux/full-function/quotes/q01-product_validation_quote-states.svg#state-unknown)。
- 提交/回执边界：Quote cards create no receipt and expose no original-receipt recovery action; reuse parent Action Receipt design, never invent another.
- 未绘制域组合与未知/迟到/权限细节仍为text_spec_only；源没有通用TTL/quote_version时不得补造倒计时。


## 7. 状态、事务与持久化

loading、empty、unavailable、permission denied、pending、unknown与明确失败不得混成一个状态。模型/查询成功不等于世界生效。原结果按原主体和请求身份回读，不能因刷新/切换/重连重放在途意图。本设计不增加持久化协议；动态DTO/服务capability未提供的字段或枚举保留未知。

## 8. 安全、权限与隐私

以当前授权为准；权限丢失先停止可执行入口并清理旧主体私有上下文，不把旧图形/反馈留给新账号。凭据只以空字段/掩码示意，图稿不包含真实key、OTP、Cookie或完整私有响应。通用raw JSON不作为获得更多数据的许可。

## 9. 质量、响应式与可访问性

遵循既有brand与visual规范。窄屏/低高度、触控、键盘、焦点回返、读屏语义、IME/reduced-motion须按实际候选验证；静态SVG与文字不证明这些运行行为。错误近字段，busy和禁用原因可读，重要结果不只靠颜色。

## 10. 兼容、迁移与回滚

沿用原14张概览与兼容入口，不移除旧authority锚点。新稿先按具体source/条件接入，不将target-only按钮启用为当前。文档/资产回退只恢复对应呈现规范，不撤销任何外部世界效果；本设计不改变运行合同。

## 11. 验证设计与可追溯性

固定source与静态SVG只支持结构和视觉审读；真实交互、权限执行、跨入口连续性与世界结果仍需按下表验证。测试/手册引用是验证方法，不代表相应场景已经通过。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-QUOTE-01](viewer-quote-forms-uiux.design.md#r-quote-01) | compoundMassG 输入；只读 quote/分类/缺口/关联与下一决策，非精炼提交 | 对照 [refine_quote_preflight_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/refine_quote_preflight_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-QUOTE-01](viewer-quote-forms-uiux.design.md#r-quote-01) | compoundMassG 输入；只读 quote/分类/缺口/关联与下一决策，非精炼提交 | 对照 [refine_quote_preflight_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/refine_quote_preflight_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-01；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-QUOTE-02](viewer-quote-forms-uiux.design.md#r-quote-02) | 售电数量/预计收益/剩余电量/下步负担及建议，不等于售电效果 | 对照 [power_sale_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/power_sale_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-02；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-QUOTE-03](viewer-quote-forms-uiux.design.md#r-quote-03) | 当前电量、恢复选项与成本的已公布报价；不足/停机不伪装正常 | 对照 [power_survival_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/power_survival_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-MW-002](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-002)；Trace refs: O7-REQ-5d55db85-0a00-4354-af2a-37c9f8d5b182 | [R-QUOTE-03](viewer-quote-forms-uiux.design.md#r-quote-03) | 当前电量、恢复选项与成本的已公布报价；不足/停机不伪装正常 | 对照 [power_survival_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/power_survival_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-03；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-QUOTE-04](viewer-quote-forms-uiux.design.md#r-quote-04) | factory/recipe/batches；输入缺口/电量/前置步骤/风险只读 | 对照 [schedule_recipe_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | [R-QUOTE-04](viewer-quote-forms-uiux.design.md#r-quote-04) | factory/recipe/batches；输入缺口/电量/前置步骤/风险只读 | 对照 [schedule_recipe_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/schedule_recipe_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-04；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-QUOTE-05](viewer-quote-forms-uiux.design.md#r-quote-05) | from/to ledger/material/amount/distance/priority/ordered route IDs/auto reroute；quote 不提交转运 | 对照 [transfer_material_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/transfer_material_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-05；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [R-QUOTE-06](viewer-quote-forms-uiux.design.md#r-quote-06) | chunk x/y/z；replenishment_enabled/count/time uncertainty；不承诺未来必有资源 | 对照 [fragment_refill_preview_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/fragment_refill_preview_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-06；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-CC-001](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)；Trace refs: O7-REQ-2e68a48f-4a44-4135-9268-e17c6f32d0b3 | [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07) | aggressor/defender/intensity；freshness 与成本预览，宣战按钮固定禁用 | 对照 [war_declaration_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/war_declaration_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07) | aggressor/defender/intensity；freshness 与成本预览，宣战按钮固定禁用 | 对照 [war_declaration_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/war_declaration_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-07；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-GCB-001](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001)；Trace refs: O7-REQ-f282f0ef-655c-4e41-a1b6-fa37482d663b | [R-QUOTE-08](viewer-quote-forms-uiux.design.md#r-quote-08) | proposal/option/weight；预览不是投票，提交未开放 | 对照 [governance_vote_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/governance_vote_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [R-QUOTE-08](viewer-quote-forms-uiux.design.md#r-quote-08) | proposal/option/weight；预览不是投票，提交未开放 | 对照 [governance_vote_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/governance_vote_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-08；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)；Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [R-QUOTE-09](viewer-quote-forms-uiux.design.md#r-quote-09) | material/amount；当前市场报价及建议只读，不表示成交或紧急保供授权 | 对照 [market_quote_decision_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/market_quote_decision_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-QUOTE-09](viewer-quote-forms-uiux.design.md#r-quote-09) | material/amount；当前市场报价及建议只读，不表示成交或紧急保供授权 | 对照 [market_quote_decision_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/market_quote_decision_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-09；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [R-QUOTE-10](viewer-quote-forms-uiux.design.md#r-quote-10) | 产品/验证目标、输入条件与结果预估分离，不将请求成功当解锁 | 对照 [product_validation_quote_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/product_validation_quote_card.test.jsx) 既有组件/状态场景逐项静态审读；后续仅在获准同候选执行。当前未运行，不认定其覆盖全部义务 | [字段/动作/条件派生证据](assets/ui-ux/full-function/coverage-catalog.json) 的 R-QUOTE-10；实际运行证据待后续 | 不证明部署、动态全部变体、跨入口与世界结果 |

## 12. 决策、长期风险与未决问题

- 图稿仅代表实际映射区域/状态；没有对应frame的项保持text_spec_only，未公布提交合同保持contract_blocked。图稿完备性不得用页面数判定。
- 本主题的具体frame及字段/动作/状态关系见随稿覆盖目录；未画状态明确保留text_spec_only，不能从概览推导。
- 动态DTO、未知action集合、部署能力、原生桌面及真实世界结果链仍待各专业owner在获准环境验证。
- 需求设计关系只覆盖表中适用范围，不自动将产品状态升为已覆盖；未核实的专业协议或目标CRUD保持明确缺口。

