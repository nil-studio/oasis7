# Viewer Pixel World Player Leverage & Production Readability Brainstorm（2026-05-28）

- 上游设计文档: `doc/world-simulator/viewer/viewer-pixel-world-player-readable-rendering.design.md`
- 上游 PRD: `doc/world-simulator/viewer/viewer-pixel-world-player-readable-rendering.prd.md`
- 当前性质: 2026-05-28 的历史 bounded brainstorming / next-slice design brief；下文候选、理由、任务身份及当时下一步建议保留原语境。
- 当前适用性：现行设计入口是[配套设计](viewer-pixel-world-player-readable-rendering.design.md)；后续动作遵循[开发流程规范](../../engineering/workflow/source-of-truth.md)。本文的 “Current implementation task” 与新建 `.pm` task 建议仅是历史流程记录，不恢复退役本地任务义务，也不声明 S1–S4 已实施或验收。

## Decision
- Problem: pixel-world 的商业化目标不是把模拟器数据全部画出来，而是把玩家能理解、能选择、能复盘的因果关系画出来。
- Why now: commercial rendering loop 已把首屏从 renderer diagnostics 收回游戏棋盘；下一步需要定义“玩家为什么要盯着它、为什么想点下一步、点完如何知道自己改变了世界”。
- Recommendation: 按“指挥棋盘 -> 生产生态 -> 像素剧场”的顺序推进，先压实 player leverage，再扩生产可读性，最后叠表现与可分享瞬间。

## Scope
- Ready now: 作为下一轮 viewer/runtime 协议设计输入，不直接进入代码实现。
- Needs split: 生产设施、资源流、行动预览和结果回执需要分阶段落地，不能塞进当前 host-only `commercial_surface` 一次完成。
- Proposed slices:
  - S1 action receipt surface: 用现有 accepted intent / recent world change 显示“你刚刚改变了什么”。
  - S2 action preview lens: 选择 agent 或目标时，高亮路线、阻塞、预期收益和风险。
  - S3 production readability DTO: runtime snapshot 增加设施、资源点、生产边、吞吐和瓶颈摘要。
  - S4 pixel-world moment polish: 在因果语义稳定后再加微动效、状态脉冲、里程碑演出和分享截图感。

## Core Principle
- Pixel-world 应渲染因果，不追求全量数据。
- Agent/action/player leverage 的视觉权重大于 Location 和 Fragment block。
- Fragment 是世界身体和空间背景；在 Agent 可读视角下，fragment block 默认是背景信息。
- 任何主视野信息必须回答至少一个问题：我能做什么、我会影响谁、世界哪里变了、哪里卡住了、下一步为什么值得继续。
- 世界活跃不等于玩家进展；如果没有 accepted intent 或玩家造成的 world change，UI 应明确显示尚无 player-facing leverage。

## Player Mental Model
玩家进入 pixel-world 时，应依次理解：

1. 当前目标是什么。
2. 哪个 agent 或设施是我现在最该关注的对象。
3. 它和哪个 fragment / location / route / resource 有关系。
4. 下一步行动会带来什么结果。
5. 当前最大阻塞是什么。
6. 我刚才的操作让世界发生了什么变化。
7. 继续推进会解锁什么长期成长。

## Data Priority
| Priority | Data | Player Question | Visual Treatment | Not Allowed |
| --- | --- | --- | --- | --- |
| P0 Command | objective, next action, blocker, active agent | 我现在该做什么？ | HUD + selected target + route highlight | 被 renderer/runtime badges 挤掉 |
| P1 Causality | accepted intent, affected target, world change, effect detail | 我刚才改变了什么？ | result receipt, pulse, changed tile/route highlight | 用 recent event 噪音冒充玩家反馈 |
| P2 Production | facility, resource source/sink, throughput, queue, bottleneck | 我的系统哪里产出、哪里卡住？ | production lanes, bottleneck badges, resource pulse | 变成 spreadsheet 或调试表 |
| P3 Ambient World | fragments, unselected locations, terrain variation, background agents | 世界在哪里、背景状态如何？ | subdued terrain/body layer, low-contrast context | 抢占 agent/action/readability |
| P4 Diagnostics | renderer status, raw DTO, camera, test controls | 为什么渲染不对？ | collapsed diagnostics | 默认进入首屏主视觉 |

## Screen States
| State | Trigger | Must Show | Player Value |
| --- | --- | --- | --- |
| Idle Command | no selected target | objective, next action, blocker, world read | 5 秒内知道当前局面 |
| Agent Selected | player selects agent | agent role, route, current assignment, reachable targets | 知道这个 agent 能做什么 |
| Action Preview | player hovers/chooses available action | expected target, cost/time, risk, expected effect | 行动前有可判断的预期 |
| Executing | action accepted / tick advancing | route pulse, acting agent, affected target | 看到“命令正在发生” |
| Result Receipt | world change arrives | changed resource/facility/blocker, before/after summary | 明确“这是我造成的” |
| Bottleneck | blocker or stalled production | bottleneck object, missing dependency, recovery hint | 不迷路，知道卡点 |
| Milestone | capability/resource chain improves | unlock label, new route/facility/option | 产生继续玩的奖励感 |

## Visual Directions
### Option A: Command Lens
- Description: 选择 agent/目标后，地图只强化当前行动相关路线、目标、阻塞和结果，其余内容降噪。
- Fit: 最贴近当前 `commercial_surface`，可以先用 host-only 数据落地。
- Tradeoff: 世界生命感较弱，但可玩性解释最强。

### Option B: Production Pulse
- Description: 把设施、资源点和物流线做成持续流动的生产网络，瓶颈用热区或断流表达。
- Fit: 最适合长期商业化留存，能支撑成长、优化和策略循环。
- Tradeoff: 需要 runtime snapshot 正式提供 facilities/resources/throughput，不适合继续靠前端猜。

### Option C: Pixel Moment Theater
- Description: 用 agent 微动作、局部演出、里程碑动效和前后变化截图感强化“世界活着”。
- Fit: 最适合第一眼吸引力、视频传播和玩家情绪记忆。
- Tradeoff: 如果先于因果/生产语义，会漂亮但不可玩。

## Recommendation
- Chosen direction: Option A first, Option B second, Option C after causality is stable.
- Why: 当前技术面已经能派生 objective / action / leverage / route；先把“选择-预期-执行-结果”闭环做清楚，后续设施和资源协议才有明确承载点。
- Product thesis: pixel-world 的商业价值来自可读的玩家因果，而不是模拟器完整性展示。

## Runtime Contract Candidates
后续若进入实现，应优先讨论以下 snapshot/DTO 字段，而不是继续扩大 host-side guessing：

- `player_action_receipts[]`: action id、source agent、target、before/after summary、effect kind、tick。
- `action_previews[]`: action id、target, route ids、expected cost/time、risk、predicted effect。
- `production_nodes[]`: facility/resource/source/sink id、kind、state、owner/agent binding、fragment anchor。
- `production_edges[]`: source、target、resource kind、throughput、queue pressure、blocked reason。
- `bottlenecks[]`: target id、missing dependency、severity、recommended recovery action。
- `milestones[]`: unlocked capability、new visible option、changed strategic layer。

## Implementation Slice Proposal
| Slice | Owner Bias | Scope | Verification |
| --- | --- | --- | --- |
| S1 action receipt surface | viewer_engineer | Extend host `commercial_surface` with explicit receipt fields from existing gameplay summary/events. Current implementation task: `.pm/tasks/task_cc47f34ea897420cb20a44c7a77c5424.yaml`. | `pixel_world_host` UI tests + browser smoke |
| S2 action preview lens | viewer_engineer + runtime_engineer | Bind available action target/route/cost preview into pixel-world selection state. | host tests + selected-agent browser smoke |
| S3 production readability DTO | runtime_engineer + viewer_engineer | Add facilities/resources/edges/bottlenecks to snapshot and render state. | runtime contract tests + viewer DTO tests |
| S4 pixel moment polish | viewer_engineer + producer_system_designer | Add motion/visual hierarchy once causality and production data are real. | screenshot comparison + mobile/desktop browser smoke |

## Non-Goals
- 不把 Fragment block 变成默认 hover/select target。
- 不把生产网络做成调试表格或工程 dashboard。
- 不用 recent event 数量、runtime tick 或 renderer health 冒充玩家进展。
- 不在没有 runtime contract 的情况下前端猜设施、库存和吞吐。
- 不为了“世界热闹”牺牲 agent/action 的可读性。

## Repo Truth Writeback
- This brainstorm should be cited by the next PRD if it changes runtime snapshot or viewer interaction scope.
- Current commercial rendering PR remains the first-slice UI baseline.
- Any production readability implementation should create a new `.pm` task rather than reopening the completed commercial HUD task.
