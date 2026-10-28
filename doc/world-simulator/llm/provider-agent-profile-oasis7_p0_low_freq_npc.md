# Local Provider 专用玩法 Profile：`oasis7_p0_low_freq_npc`

- 关联 PRD: `PRD-WORLD_SIMULATOR-037`、`PRD-WORLD_SIMULATOR-038`
- owner: `agent_engineer`
- 联审: `producer_system_designer`、`qa_engineer`
- 适用范围: `Local Provider(Local HTTP)` 首期 `P0` 单低频 NPC parity、mock regression、experimental 试点

## 1. 定义
- `oasis7_p0_low_freq_npc` 是当前默认发给 `Local Provider` 的 provider-side 玩法 profile / skill 标识；旧 `legacy_p0_low_freq_npc` 已移除，不再作为兼容别名保留。
- provider 收到该标识后，应加载与 `oasis7` 首期 `P0` 低频 NPC 相匹配的系统提示、动作偏好、恢复策略与禁行动作约束。
- 首期通过 `DecisionRequest.agent_profile` 传输；若 provider 不识别该 profile，必须返回结构化失败，而不是静默退回通用 profile。

## 2. 目标优先级
1. 保持 runtime 权威与动作合法性，绝不假设未在 `action_catalog` 中声明的能力。
2. 让单低频 NPC 在 `P0` 场景内表现出与内置 agent 接近的“能完成目标、不过度犹豫、能解释失败”的体验。
3. 在可见信息不足时优先做低风险动作，不制造高成本或不可恢复行为。
4. 保持 trace / transcript 可诊断，便于 QA 与 producer 做 parity 裁定。

## 3. 行为准则
- 仅在 `wait`、`wait_ticks`、`move_agent`、`speak_to_nearby`、`inspect_target`、`simple_interact` 六类 phase-1 白名单内选动作。
- 若目标地点可见且移动能缩短完成路径，优先 `move_agent`。
- 若目标或上下文不明确，优先 `inspect_target` 获取更多信息；不要凭空构造隐藏状态。
- `speak_to_nearby` 仅用于低成本社交提示、确认、呼喊，不承担复杂谈判或长文本输出。
- `simple_interact` 仅用于 lightweight 交互，不得假定可触发未建模的经济、建造或治理链路。
- `wait` / `wait_ticks` 只应在当前无安全动作、等待外部变化、或收到可恢复错误后短暂退让时使用；不得连续空转掩盖决策失败。

## 4. 禁止事项
- 禁止输出不在 `action_catalog` 内的动作、参数或 schema。
- 禁止把“长期计划”“隐含意图”“未执行成功的语义”伪装成已经完成的 runtime 动作。
- 禁止主动触发高成本、高破坏性或当前未纳入 `P0` 的玩法（如建造、交易、治理、跨 agent 协同编排）。
- 禁止把缺失观察信息脑补成事实；不确定时必须选择 inspect / wait，而不是虚构世界状态。
- 禁止在 provider 不支持该 profile 时静默降级为未知 profile 继续运行。

## 5. 恢复策略
- 收到 `timeout` / `provider_unreachable` / `retryable` 错误时，优先返回可恢复的 `wait` 或 `wait_ticks`，并在 trace 中显式说明原因。
- 收到 runtime / adapter 的非法动作反馈后，下一轮应避免复现同类动作，直到观察上下文发生变化。
- 若连续多个 tick 无法推进，应在 trace 中明确标记“当前 profile 被阻塞/信息不足”，供 parity 采证统计 `completion gap`。

## 6. 对标口径
- `scripts/provider-parity-p0.sh` 与 `oasis7_provider_parity_bench` 默认使用该 profile，确保 `P0` 对标样本具备可重复的玩法口径。
- mock / required regression 也必须显式透传该 profile，防止 provider 侧因 profile 漏配导致“看似通了、实则玩法漂移”。
- 若后续进入 `P1`/`P2`，应新增 profile 标识或版本，而不是复用本 profile 覆盖更高复杂度场景。
