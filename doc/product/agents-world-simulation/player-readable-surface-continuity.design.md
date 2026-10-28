# 玩家可读表面连续性产品设计

## 文档身份

- 结构裁剪：理由：稳定锚点和连接状态共同表达阅读与恢复体验；范围：响应式、文本和行动反馈表面；风险：表现恢复不能代签权限或世界结果；映射：[目标与设计命题](#1-稳定阅读锚点)、[代表性体验流程](#4-连接状态)

- 配对产品 PRD：[`doc/product/agents-world-simulation/player-readable-surface-continuity.prd.md`](player-readable-surface-continuity.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-12
- 专业域权威：[`Viewer 手册`](../../world-simulator/viewer/viewer-manual.manual.md)、[`world-simulator PRD`](../../world-simulator/prd.md)、[`Web 语义测试 API`](../../world-simulator/viewer/viewer-web-semantic-test-api.prd.md)

本文定义跨 surface 的稳定设计语义，不把历史 Viewer 布局或控件提升为当前实现要求。

下述流程是目标设计。配对 PRD 的 `active` 状态与本设计的存在，不证明个人反馈隔离、提交前检查或视觉验收已经完成。

## 1. 稳定阅读锚点

任何受支持布局都优先保留四个锚点：当前目标、主要 blocker、最近可信行动反馈、下一决策或恢复入口。次级世界信息、设置和诊断可以重排或渐进隐藏，但不能抢占或切断这四个锚点。

## 2. 响应式与遮挡

- desktop、窄屏和低高度可以采用不同结构；主要行动表面不被不可恢复的 panel 或 overlay 遮挡。
- 收起或隐藏是可理解、可逆且可发现的状态，不依赖记住历史控件位置。
- 全屏或密度偏好只调整呈现，不改变世界真值、权限、选择或动作结果。
- 颜色、动效、hover 和屏幕位置不能成为关键状态的唯一信号。

## 3. 双语与 fallback

- 语言选择及当前语言状态可辨认，切换不会改变动作或世界状态。
- 目标、blocker、接受/拒绝、结果、错误和恢复文本优先保证语义覆盖。
- 缺失翻译时使用可读 fallback 并保留诊断线索；不得显示空白或把技术键当成已完成本地化。

## 4. 连接状态

- `已连接 / 中断 / 恢复中 / 恢复失败` 使用文本或等价语义区分。
- 恢复入口与当前决策上下文相邻；重连成功只证明连接恢复，不证明请求、动作或世界进展成功。
- 当前 surface authority 未支持的 retry、fullscreen、panel 或 locale 动作不得展示成可用能力。

## 5. 当前模式与行动反馈

- 在主动作附近表达当前可用行动；不支持、连接中断、规则/权限阻塞和已接受请求必须保持不同含义，不能统称为“失败”或“请重连”。
- 不支持的行动提供当前模式适用的替代、等待或安全返回入口；能力边界不因布局、自动开始、重载或背景推进而消失。
- 界面时间、事件计数和动画只帮助阅读；只有权威反馈能够确认行动产生世界后果。没有可见进展时，继续呈现当前可信状态和下一决策，而不是用自动刷新制造成功印象。

<a id="player-feedback-ownership"></a>
### 5.1 本人反馈与共享世界信息

对应[个人反馈要求](player-readable-surface-continuity.prd.md#req-agent-surface-003)及[交叉身份验收](player-readable-surface-continuity.prd.md#ac-agent-surface-003)。

玩家阅读“我刚才的行动发生了什么”时，反馈属于当前经确认的身份和本人有权读取的行动。另一玩家的错误、匿名失败或客户端自报的身份不能抢占这条解释。匿名连接仍可读到自己请求被拒绝及安全下一步；这不使其获得其他玩家的反馈或权限。

当前身份变化时，先重新确定反馈归属，再呈现该身份可读结果；先前身份的私有反馈不继续留在新的个人结果面。同一身份的其他会话能否读取历史结果，按既有授权和反馈可见范围判断。公共世界事件可以继续按现行可见性显示，须保留事件主体，不能被归为本人的成功或失败。

<a id="actionable-rejection"></a>
### 5.2 可理解的拒绝与恢复选择

对应[拒绝原因要求](player-readable-surface-continuity.prd.md#req-agent-surface-004)及[不同失败验收](player-readable-surface-continuity.prd.md#ac-agent-surface-004)。

| 本人可确认的结果 | 玩家需要知道 | 真实下一步 |
| --- | --- | --- |
| 输入不符合行动要求 | 哪类输入需要修正，且该行动没有因此成功 | 修改输入后明确提交，或返回 |
| 身份或权限不满足 | 当前无法使用该能力；仅展示允许披露的原因 | 重新确认身份、取得合法权限或停止 |
| 世界条件不满足 | 已确认的资源、对象或规则阻塞 | 补齐条件、刷新观察、改道或等待 |
| 投递后结果未知 | 尚不能确认原请求是否已有世界结果 | 先查询原请求并等待核对，避免重复行动 |

不为所有失败提供同一个“重连”或“重试”动作。解释使用受支持语言的可访问文本，保留可行下一步；内部错误码、字段和实现诊断按需下钻，不成为玩家必须学习的操作前置。

<a id="preflight-submission-boundary"></a>
### 5.3 检查、确认与提交

对应[提交前检查要求](player-readable-surface-continuity.prd.md#req-agent-surface-005)及[资格与状态变化验收](player-readable-surface-continuity.prd.md#ac-agent-surface-005)。

1. 玩家在当前身份和目标上查看检查结果；检查无法确认权威条件时，停在不可确认或阻塞状态。
2. 检查结果说明当前共享资格条件、主要成本和缺项；玩家可修正、等待或选择提交。显示“当前条件允许提交”不表示已接受或成功，也不保留未来权限与资源。
3. 真实提交重新使用当前权威条件判断；检查后身份绑定、权限或世界条件变化时，解释新的拒绝或重新检查路径，不能静默沿用旧资格。
4. 已受理且适用权威状态确认尚未提交时显示已确认未生效的待决；无法确认是否已提交时显示结果未知、待核对，不能因缺少回执显示确定未生效。先恢复当前读取资格并查询原请求，查询受阻保持未知；只有对应权威结果确认后显示执行或结算结果。断连和重新检查不覆盖原请求结果。

这一顺序减少无效提交，同时保留世界持续变化带来的风险。具体检查范围与资格机制由[专业系统设计](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#collect-data-preflight-consistency)决定；产品层不冻结协议、nonce、错误码或查询频率。

## 6. 非承诺

- 不承诺旧 EGUI 右栏、模块可见性缓存、专用 fullscreen toggle、固定断点或历史 Web 布局。
- 不定义 control profile、动作枚举、seek/step、逻辑时间/事件游标、ack 字段、字体、翻译键、localStorage/JSON 路径、WebSocket callback/backoff、viewport hit test 或 Test API。
- 不以历史截图、旧测试、mock、software-safe 或局部 DOM 存在代签当前产品能力。

## 7. 验证边界与未决

- 验证证据应覆盖目标、blocker、可信反馈和恢复入口在受支持 desktop、窄屏、低高度及双语 fallback 下的可读性，并区分 surface 局部状态与权威世界结果；结构存在、自动刷新或历史截图不能单独证明产品能力。
- 未决：各受支持 surface 的具体控件、断点、翻译资源和恢复动作仍由 Viewer、WASM 与 QA authority 结合当前证据决定；在对应 authority 尚未确认前，产品层保持能力边界与安全返回语义，不把候选布局当作已支持入口。

表现验证沿用[配对 PRD 的视觉证据边界](player-readable-surface-continuity.prd.md#53-表现与视觉证据边界)；既有颜色、框线或范围提示只是表达对应玩家语义的方法。测试和截图需能区分所声明特征的正常与缺失状态；局部画面存在不证明该特征可读，也不证明世界结果。具体确定性比较、实体复用与浏览器证据由[渲染专业设计](../../world-simulator/viewer/viewer-pixel-world-bridge-rendering.design.md#render-reconcile-and-visual-regression-target)维护。

## PRD REQ/AC fragment mapping

This design delegates product requirements and acceptance semantics to its paired PRD. Every row below is a navigable fragment mapping; this design does not create a second requirement authority.

| PRD requirement | PRD acceptance |
| --- | --- |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[REQ-AGENT-SURFACE-001](player-readable-surface-continuity.prd.md#req-agent-surface-001) | [AC-AGENT-SURFACE-001](player-readable-surface-continuity.prd.md#ac-agent-surface-001) |
| Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64；[REQ-AGENT-SURFACE-002](player-readable-surface-continuity.prd.md#req-agent-surface-002) | [AC-AGENT-SURFACE-002](player-readable-surface-continuity.prd.md#ac-agent-surface-002) |
| Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；[REQ-AGENT-SURFACE-003](player-readable-surface-continuity.prd.md#req-agent-surface-003) | [AC-AGENT-SURFACE-003](player-readable-surface-continuity.prd.md#ac-agent-surface-003) |
| Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4；[REQ-AGENT-SURFACE-004](player-readable-surface-continuity.prd.md#req-agent-surface-004) | [AC-AGENT-SURFACE-004](player-readable-surface-continuity.prd.md#ac-agent-surface-004) |
| Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d；[REQ-AGENT-SURFACE-005](player-readable-surface-continuity.prd.md#req-agent-surface-005) | [AC-AGENT-SURFACE-005](player-readable-surface-continuity.prd.md#ac-agent-surface-005) |
