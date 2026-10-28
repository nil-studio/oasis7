# oasis7: 模型视觉评审 SOP（2026-05-29）

审计轮次: 1

## 文档定位
- 本文件是截图加模型视觉评审的 canonical `*.manual.md` 操作手册。
- 它用于替代绝大部分常规人工视觉 review：布局、遮挡、层级、可读性、响应式、截图与产品目标是否一致，默认先交给具备视觉能力的模型评审。
- 它不替代 GitHub required review、required checks、发布 owner 放行、真实玩家验证或对外承诺审批。

## 适用范围
- 适用于 Viewer Web、launcher Web、站点页面、文档封面、像素世界和其他截图可判断的 UI / visual surface。
- 适用于 PR 前本地收口、PR review 辅助、发布 evidence packet 的视觉部分。
- 涉及视觉方向、交互手感、玩家屏幕流程或体验取舍时，模型视觉评审必须回流 `game_visual_interaction_designer`；模型可替代 routine 截图检查，但不替代该角色的方向裁决。
- 不适用于仅凭截图无法判断的 runtime contract、经济参数、安全权限、链上状态、LLM provider 活性或真实玩家留存结论。

## 强制触发条件
- 只要本轮改动涉及可视化相关代码、样式、资源或可见输出，就默认触发截图加模型视觉评审。
- 典型强制触发面包括：
  - UI component、DOM 结构、CSS / layout / spacing / typography / color / icon / asset 改动
  - 首屏、导航、侧栏、面板、表单、按钮、状态提示、receipt、blocker、empty/loading/error state 改动
  - 响应式断点、mobile / desktop 优先级、滚动、overflow、裁切、焦点可见性改动
  - canvas、WebGL、Three.js、pixel-world renderer、Fragment / Agent / Location / route / hotspot 等可视化映射改动
  - DTO / state 到可见 UI 的映射改动，即使没有改 CSS，只要玩家看到的文本、状态、层级或实体可能改变
  - 截图脚本、visual smoke、storybook / fixture / demo page、图片资源或品牌视觉素材改动
- 可豁免的情况只限于明确不影响任何可见 surface 的改动，例如纯后端逻辑、纯测试 helper、非渲染文档文字；豁免时 owner 必须在任务日志或 PR evidence 中写明“不影响可见输出”的理由。
- 若不确定是否会影响视觉，按触发处理，采集至少一张目标截图并输出 review card。

## 目标
- 把“看起来是否对”从临时人工主观 review，改成可重复的截图输入、固定 rubric、结构化 verdict 和升级边界。
- 让 routine visual review 默认由模型完成，人类处理未达到 `pass + high` 的评审、审美方向争议、对外 claim 和最终治理放行。
- 让模型视觉结论可追溯到截图、viewport、commit、任务目标和自动化验证结果。

## 输入要求
- 必填输入：
  - `task_uid` 或 PR / issue / project ref
  - 当前 commit SHA 或分支名
  - 变更目标，用一句话说明玩家/用户应该更容易看懂什么
  - 截图路径，至少包含目标 surface 的 desktop 视口
  - 自动化结果摘要，例如 DOM/DTO/UI tests、visual smoke、console/state probe
  - expected visual contract，例如“Agent 第一眼可见，Fragment 只是背景信息”
- 按需输入：
  - mobile 截图，建议宽度 390px 或当前产品指定断点
  - before/after 或 baseline 截图
  - DOM probe / state JSON / console log
  - 用户反馈原句或设计方向文档

## 截图采集标准
- 每轮模型视觉评审至少保留一张主截图；触达响应式或首屏时必须补 mobile 截图。
- 截图路径默认落在 `output/playwright/<topic>/<run-id>/`；若需要随文档长期归档，再复制或引用到对应 `doc/**/evidence/` 目录。
- 截图必须可复现地标明：
  - URL 或页面入口
  - viewport
  - locale
  - fixture / scenario / injected snapshot
  - commit SHA
  - 是否为 fallback / degraded mode
- 若页面可滚动，主图优先裁切目标体验区域；必要时再补 full-page 截图，避免模型把无关右栏或下方长内容当成首屏问题。

## 标准流程
1. 先跑确定性验证：
   - DOM/DTO/contract tests
   - Web visual smoke 或 browser screenshot script
   - console/state probe
2. 采集截图：
   - desktop 主视口
   - mobile 主视口（若涉及响应式或首屏）
   - before/after（若是视觉改版）
3. 组装模型视觉评审包：
   - 截图
   - 变更目标
   - must-pass visual contract
   - 自动化摘要
   - 已知 out of scope
4. 让模型按固定 rubric 输出 review card。
5. owner 根据 verdict 处理：
   - 按下表同时核对 `verdict`、`confidence` 和独立升级条件，不能只看 verdict。

<a id="visual-review-decision-contract"></a>
### Verdict / confidence 与后续动作

本表沿用总手册 S6 的严格口径；不把旧 SOP 的“首次 watch 可继续、第二轮才升级”作为当前放行规则。`confidence` 表示证据足以支撑视觉判断的程度，不是 pass 概率或可玩性分数。

| Verdict | Confidence | Routine review 处理 | `human_escalation_needed` |
| --- | --- | --- | --- |
| `pass` | `high` | 必需截图、目标与自动化摘要一致，且无下述独立升级条件时，可替代 routine 人工视觉 review | 仅无独立升级条件时为 `no` |
| `pass` | `medium` / `low` | 不足以替代 routine review；补齐证据或由人类 owner 复核 | `yes` |
| `watch` | 任意 | 首次即升级；记录 residual risk 与 owner action，不能自行作为视觉验收通过 | `yes` |
| `block` | 任意 | 升级 owner，并修复、复核或由有权 owner 明确裁决；未处置不得通过 | `yes` |
| `human_escalation` | 任意 | 交给对应 owner / reviewer，结论回写正式 sink | `yes` |

`high` 要求关键视口、状态与本轮目标均可判读且无未解释冲突；`medium` 表示仍有局部不确定性，`low` 表示缺关键输入、可读性不足或结论冲突。缺失/非法 verdict 或 confidence 视为未完成，不能默认 `pass`。补充证据后可生成新的评审卡；原图、原卡、原结论保留，不追改历史卡片以制造通过率。

## 模型评审 Rubric
| 维度 | 必答问题 | `block` 信号 |
| --- | --- | --- |
| 第一视觉焦点 | 用户第一眼是否看到本轮最重要的主体或任务？ | 主体被侧栏、诊断、装饰或噪声压住 |
| 层级与可读性 | 文字、按钮、状态、实体是否能快速扫读？ | 文本过密、字号失衡、标签互相竞争 |
| 遮挡与溢出 | 是否有重叠、裁切、横向溢出、按钮文本挤压？ | 主要操作或状态被遮挡/截断 |
| 数据诚实性 | 截图表达是否和真实 state/DTO 一致？ | UI 暗示了未发生的进展、因果或 agent 行动 |
| 行动反馈 | 用户动作是否有明确 receipt / next step / blocker？ | 只看到世界在动，看不到玩家导致了什么 |
| 响应式 | desktop/mobile 是否保持同一套主次逻辑？ | 移动端首屏主路径消失或需要横滚 |
| 商业化观感 | 是否像面向玩家的游戏界面，而不是内部调试台？ | diagnostics、原始数据或工程面板抢主视野 |
| 回归风险 | 与目标/基线相比是否明显退化？ | 新图比 baseline 更难读或更像 placeholder |

## 像素世界补充 Rubric
- Agent 必须比 Fragment 背景更容易被第一眼识别。
- Fragment block 默认是背景信息，不能抢过 Agent、目标或行动回执。
- Location 若只是逻辑锚点，不能以主实体方式支配画面。
- Action Receipt 必须回答“玩家做了什么，世界因此反馈了什么”。
- No-receipt 状态必须诚实，不得暗示 active agent 已造成进展。
- Renderer diagnostics 默认不得比 World Command Board 或 Action Receipt 更显眼。

## 输出格式
- 使用模板：`doc/testing/templates/model-visual-review-card-template.md`
- 每张评审卡必须包含：
  - verdict: `pass` / `watch` / `block` / `human_escalation`
  - confidence: `high` / `medium` / `low`
  - screenshots reviewed
  - must-pass checks
  - findings
  - what this review does not prove
  - human escalation needed: `yes/no`
  - owner action

## 替代人工 Review 规则
- 可由模型视觉评审替代的人类 review：
  - routine screenshot inspection
  - 布局/遮挡/溢出检查
  - 首屏主次关系判断
  - 响应式截图扫查
  - UI 是否误导玩家因果/状态
  - 与已冻结视觉方向的一致性检查
- 不能完全替代的人类 review：
  - 全新品牌/美术方向定调
  - 玩家屏幕流程、交互手感、商业化观感或体验取舍的方向争议
  - 对外宣传、社区承诺、事故复盘口径
  - 法务、安全、经济、权限、隐私或财务风险
  - L5 真实玩家 / 真实外部信号
  - GitHub branch protection 要求的 review / approval
  - 未达到 `pass + high`，或截图证据缺关键视口

## 升级条件
- 任意 `watch/block/human_escalation`，或 `confidence=medium/low`；详见 [判定表](#visual-review-decision-contract)。
- 任一 must-pass check 为 `block`。
- 模型指出截图缺失、状态不明或无法判断真实交互。
- 模型与自动化 state / DOM probe 结论冲突。
- 评审结论会影响公开 claim、release note、limited preview 或用户承诺。
- 评审结论涉及视觉方向、交互手感或玩家屏幕流程方向裁决。
- 用户或 owner 明确要求人工审美判断。
- 连续评审只用于复核整改；不得等待第二轮 `watch` 才升级。

## 正式 Sink
- 当前 task：回写 PR、实际 CI 与评审记录（Issue 按需）。
- PR：在 PR 描述或评论里附 screenshot path、review card 摘要和 residual risk。
- 长期规则：回写 `doc/testing/**`、`doc/world-simulator/**` 或对应模块 PRD/project。
- 一次评审的原始截图、review card 和结论保留原窗口、候选与 verdict，可存于 `doc/ui_review_result/**` 并由实际 PR 引用；后续整改追加新记录，不覆盖原失败。只有可复用的规则、判据或接口变化才回写正式模块文档，不把每次运行摘要迁入长期规范。

## 最小完成定义
- 至少一组截图已采集并可打开。
- 模型视觉 review card 已按模板填写。
- verdict 与 human escalation 状态明确。
- 若主张替代 routine review，必须为 `pass + high`，自动化验证也必须已通过或明确列出未跑原因；记录原因不豁免其他适用 required checks。
- 若 verdict 为 `watch/block/human_escalation`，下一步 owner action 已写清。

## 推荐命令入口
- Viewer Web 闭环：`doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`
- Pixel-world visual smoke：
```bash
npm --prefix crates/oasis7_viewer run test:pixel-world:visual
```
- Viewer UI deterministic tests：
```bash
npm --prefix crates/oasis7_viewer run test:ui
```

## 决策记录
- DEC-MVR-001: 模型视觉评审是 routine visual review 的默认替代路径，但不是发布治理或 GitHub required review 的替代品。
- DEC-MVR-002: 模型视觉评审必须以截图证据为输入，不能只读 DOM 或口头描述后给视觉 verdict。
- DEC-MVR-003: 只有 `pass + high` 且截图、自动化摘要与任务目标一致时才能替代 routine 人工视觉 review；其他组合及独立升级条件按判定表处理。release、对外 claim、方向裁决、required review 与 L5 的治理边界继续独立成立。
