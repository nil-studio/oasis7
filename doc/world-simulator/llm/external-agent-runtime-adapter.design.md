# 外部 Runtime 适配、Skill 与持续执行系统设计

- 设计状态：target contract，待实现与独立评审；不是当前能力或发布报告。
- Canonical repository：`eng-cc/oasis7`；审读基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2`（已合入产品 PR #4493）。
- Owner role：`agent_engineer`；最后审读：2026-10-10；新增合同 revision：`1`。
- 产品入口：[外部 Runtime 游玩](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)；[玩家与 Runtime 协作](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 系统总入口：[LLM / external runtime 系统入口](README.md)；写作规范：[系统设计规范](../../engineering/doc-governance/system-design-writing-standard.design.md)。

本文的新组件、路由、记录和测试场景均为后续实施输入。`accepted` 只表示所属权威已受理，世界效果必须读取既有权威回执；文档合入不代表这些接口已经上线。

## 1. 问题、目标与非目标

把一个模型端点配进旧 Bridge，不等于 OpenClaw 或 Codex 正在游戏。适配需要保留实际 Runtime 的会话、工具编排和恢复能力，又不能把其本机权限带进世界授权。Skill 是说明资产，不是常驻进程；网页发布目标不会自动启动用户电脑上的程序。

首期交付可由用户启动的薄执行器、两个独立 driver、同源 Skill 和可核对的运行状态。非目标：通用 Agent 平台、任意 Runtime 即插即用、自动购买模型额度、强制 MCP、私有记忆无损跨 Runtime 迁移或将模型报告作为世界事实。

## 2. 上游约束与相关角色

Agent owner 维护 Runtime 驱动和版本兼容；Runtime owner 验证 task/executor fence、context 与行动；安全边界包括本机工具授权和游戏授权，二者不能互相代替。发行 owner 发布可信 Skill/worker 资产，QA 分别运行 OpenClaw/Codex，不能用一个 driver 或 mock 证明全部。

### 2.1 需求承接与分配表

| 上游条款 | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 排除与未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-b3e367c3-c72d-4959-b599-613ac652dab8；[AC-EXT-001](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-001) | 声明 Runtime 必须确实执行且兼容方式可复现 | [adapter-drivers](#adapter-drivers) | agent_engineer / qa_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-bd792456-4b98-4891-b5e3-e24b1de4f1a1；[AC-EXT-004](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-004) | 在自身会话和工具能力中自主多轮决策 | [adapter-loop](#adapter-loop) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；[AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007) | 无 GUI 时独立运行，停止 Runtime 不伪装继续 | [adapter-liveness](#adapter-liveness) | agent_engineer / viewer_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008) | 私有上下文无法接续时明确来源、当前限制和可信重规划路径，保留原运行及世界操作 | [adapter-private-context-recovery](#adapter-private-context-recovery) | agent_engineer / runtime_engineer / viewer_engineer；既有任务快照、原操作恢复与报告消费者 | 不承诺私有记忆无损迁移，不重定义世界恢复协议或以新计划证明世界效果 |
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | 计量、预算和超时边界诚实且可执行 | [adapter-budget](#adapter-budget) | agent_engineer / qa_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；[AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | 两个 Runtime 分别通过可信 Skill 完成自助接入 | [adapter-skill](#adapter-skill) | agent_engineer / liveops_community | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018) | 机器冷入场、必要确认停点、owner/executor 隔离和原阶段恢复 | [adapter-autonomous-entry](#adapter-autonomous-entry) | identity / agent_engineer / runtime_engineer | 不扩张默认 GameRequest 权限，不表示公开入口已开放 |
| Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a；[AC-COLLAB-004](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-004) | 问题/回复先持久接收再回答/报告处理，未知原结果不重复启动 | [adapter-question-delivery](#adapter-question-delivery) | agent_engineer / runtime_engineer / viewer_engineer | 不把 worker 收到当作模型理解或世界效果 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | 原反馈验真、持久接收、同代交付、实际 Runtime 输入与后续决策分别成立 | [adapter-result-feedback](#adapter-result-feedback) | Runtime 原反馈/水位、API、两 driver 与 Viewer | 不新立世界事实账，不把接收当语义理解或新的世界效果 |

### 显式 task context 权威模式与恢复

Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；承接 [AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018)。可信本地 executor 配置可显式设置 `authority_mode: "native_executor_v1"`；新冷入场导出必须包含此选择。省略字段维持既有 provider 模式及 schema revision 1 的 wire、签名和 commitment，不把缺少 provider invocation 当作切换依据。native 使用 revision 2，选择写入 prepare/snapshot 请求、context identity 和 commitment；服务端回复必须恰好包含相应一种 typed authority。

native authority 仅证明当前完整 task fence、独立 executor delegation、真实权限 root、业务预算、读取 tick 和有界 lease，不提供 provider module catalog。driver 只能使用所选模式的能力；native 不能借 provider 的 module invocation，provider 也不能消费 native authority 充当其 module 授权。broker 从已固定哈希的 executor 配置读取模式，不接受模型工具参数覆盖。

operation 原件在网络前持久化。重试及重启复用原 schema、mode、nonce、关联和签名；原件与当前配置模式冲突时阻断并要求显式核对，不重签、换 operation 或捕获 provider 错误后降级。已存在的 provider 原件省略 mode，继续按原模式恢复；只有 settled 后才可明确选择新模式创建新 context。模式选择不证明 Runtime 已运行，也不扩大 owner 权限。

## 3. 当前状态、目标状态与差距

| 对象 | 基线证据 | 目标与差距 |
| --- | --- | --- |
| 公开说明 | `site/skills/oasis7.md` 主要是 Local Provider 启动/诊断 | 发布 Runtime-neutral 游戏 Skill，不再要求用户阅读旧桥脚本 |
| 共享 API | `oasis7_agent_api` 当前为 DTO crate | 工具向新 Game API 提案，由世界权威校验 |
| CLI/backend | 原 Provider 专题承载回调 transport，不能证明两个外部 Runtime 的真实适配 | 两 driver 独立 manifest、探测、运行、恢复和失败证据 |
| 上游产品 | 官方文档给出 Codex app-server/非交互、OpenClaw Gateway/agent/skills 接口 | 此处选定接法；具体 runtime 版本与协议须在实施锁定并测试，不宣称当前最新版本天然兼容 |

官方参考（2026-10-10 查阅；只用于驱动选择，不是 oasis7 验收）：[Codex App Server](https://developers.openai.com/codex/app-server)、[Codex non-interactive](https://developers.openai.com/codex/noninteractive)、[Codex Skills](https://developers.openai.com/codex/skills)、[OpenClaw Gateway](https://docs.openclaw.ai/gateway/protocol)、[OpenClaw agent](https://docs.openclaw.ai/cli/agent)、[OpenClaw Skills](https://docs.openclaw.ai/tools/skills)。版本相关 method/schema 以发布时固定源码/生成 schema 为准。

## 4. 边界与结构

```text
用户显式启动 worker / 授权已有会话
    -> RuntimeDriver (OpenClaw 或 Codex)
       -> 实际 Runtime 规划、记忆、工具调用
       -> scoped GameRequest tool -> Game API -> 世界权威
    -> 独立控制循环：拉目标/消息、核对 epoch、心跳、deadline、stop
```

拟新增 `oasis7_agent_worker` 是现有 Rust 工程里的薄客户端 binary，不启动世界、不充当 Provider callback、不实现另一个任务数据库。职责限于启动/恢复用户允许的 runtime、送达上下文、提供受限工具、记录原请求关联和汇报可观测状态。

`attached` 模式允许一个已启动 Runtime 直接使用同一 Skill/API，自行满足接收目标/恢复合同；只有通过同等验收才能声明该模式支持。`supervised` 模式由用户显式启动 worker，再由 worker 运行指定 driver。关闭 Viewer 不退出 worker；关闭 worker 不能声称云端仍有模型工作。禁止将连接失败静默回退成直接模型推理或另一个 Runtime。

## 5. 关键运行流程

<a id="adapter-loop"></a>
### 5.1 控制循环与决策循环分离

控制循环独立于一次模型推理：核对当前 task/fence，增量读取目标、未完问题线程（含已回答未处理内容）及授权变更，维持有时效的诊断心跳。没有执行许可、预算或 driver 时保持 awaiting_executor/blocked，不发起模型调用。

决策循环：读取当前 task snapshot 并验证内联 active_goal 正文、revision/digest 和切点 → 获得匹配该激活版本的 Game API context → 把受权观察、完整目标、Skill 和待处理消息交给实际 Runtime → Runtime 按需查询、提案、汇报或提问 → 回读世界结果 → 在仍有效的许可下继续。worker 不固定“移动三次即完成”之类策略，也不把默认 Wait 替代未取得的模型结果。

Runtime 可以在一次本地推理中调用多个只读工具；等待、阻塞或放弃当前轮次时使用 Game API context outcomes 收口，不能一直占用活动 turn。每次副作用都经 Game API 当前 context/fence/goal/grant 校验。目标变化时允许完成旧本地计算，但旧 context 的新 action 必须被阻断；worker 给出新的上下文后才能重新规划。对硬撤销立即停止新的工具出站，并请求 driver 中断，世界侧 fence 才是最终防线。

<a id="adapter-result-feedback"></a>
### 5.1.1 原结果进入后续真实 Runtime 输入

Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7。承接 [REQ-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-005) / [AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005)。核对日期与基线：2026-11-05；`cca8cda97cf121ebc1e25e96d6b0204a421323ea`；本节为待实施组合，不改变历史 driver 支持结论。

当前 [NativeTaskRunner](../../../crates/oasis7_agent_harness/src/native_task.rs)将本次工具结果交回 provider，并独立收集已核对的 stale 拒绝；恢复已结算 slot 与计数不单独证明其完整反馈已进入一个重启后的外部 Runtime。目标接续复用 [API 原结果](external-agent-game-api.design.md#api-result-feedback)、Runtime 的 `AuthorizedFeedbackRecord` 和[反馈水位合同](continuous-agent-harness.prd.md#harness-feedback-session-watermark)，扩充既有受保护 inbox/outbox 的交付关联，不另立世界事实、receipt 或反馈序列。

| 阶段与原记录 | 必须完成的检查或提交 | 该阶段能证明什么 |
| --- | --- | --- |
| 原世界操作已知 | 发送前已有稳定 client operation/context/request 与原完整 task fence；查原结果时当前读授权有效，验证 service/world binding、disposition 和 receipt lineage | 原操作身份与真实状态；不证明结果已经送到 Runtime |
| 原反馈可用于投影 | 正向匹配 Agent、世界/branch、原 session/turn/request/digest 与 action/receipt；按原 feedback_seq 消费，gap 有界等待、同序同内容去重、异内容冲突阻断 | 受权、顺序一致的反馈；不是对最新 goal/fence 的自动授权 |
| 本地接收已持久 | 将原 record/digest、来源切点、去重水位与尚待送达目标写入同一受保护 inbox 事务后，才在当前确有 ACK 权限时走原 Runtime FeedbackAck 合同；只读权限不授予 ACK，玩家读取不代发 | 可在进程崩溃后重新取得原反馈；ACK 不代表模型已看到，ACK 被拒绝不抹去原结果 |
| 已准备下一输入 | 将有界结果和其原身份关联到下一 context、invocation_key、runtime_session_ref 及 driver 的实际工具调用/输入项；持久保存此关联后才发送 | 发送意图已记录；没有上游确认时仍是待交付/未知 |
| 实际 Runtime 已接收 | driver 以对应的工具结果接收确认、受信会话历史或协议可核的输入项关联确认原 run/request 收到该内容；回读来源并持久保存交付位点 | 仅证明交给所声明的 Runtime 输入；不证明理解、采纳、记住或成功执行 |
| 后续决策与玩家反馈 | 新决策引用其当前 context 与实际 run；世界新效果另按 admission/receipt 判定。Web/native/pure_api 读同一原结果和有来源的交付报告 | 原结果、输入已接收、下一决策与新世界结果可以分别核查，不能互相代签 |

每次后续决策先核原已提交操作和必须送达的反馈，再核完整当前 task/goal/fence、权限与剩余预算。一个真实 settled 结果不能因报告队列拥挤被永久丢弃；新目标可以读取当前仍获权的旧结果作为带原目标的历史，但不能把旧结果重新标成新目标进度。旧 session 的迟到记录不得注入新 session 的活动请求；可信恢复只按[私有上下文恢复](#adapter-private-context-recovery)把已验证历史作为新基线，保留其原身份，不重编号或伪造一个“新反馈”。换 owner/audience、Agent 身份代际或世界分支时先拒绝旧私有缓存，重新取得当前有权的基线。

实际 driver 有两种交付位置：仍活跃的工具调用接收对应 call ID 的结果；该 run 已明确结束后，新调用的首个受控输入携带尚未交付的受权反馈。它们使用同一持久交付关联，不双送到两个活动 run。崩溃发生在发送后、接收确认前时，以原 invocation/run/input identity 查上游；能证明已接收就恢复位点，能证明尚未接收且原 run 仍允许相同输入才重送原件。上游不能证明时保持 `run_unknown`，不为补送反馈启动第二次付费调用或另一个 Runtime。只读原结果仍可独立查询；界面写“世界结果已确认，Runtime 接收未确认”。

对于明确拒绝或失败，输入包含权威 reason 和已确认的实际成本，下一决策必须重新观察当前条件；不能从 transport error 造失败反馈。普通 pending 可向当前 Runtime 说明等待，但不释放旧活动 turn；`recovery_required` 暂停相关新副作用并查原件，不靠新模型调用“试一次”恢复。无进展、未知或 history_unavailable 都不得生成成功摘要、补零费用或删除原责任。

**容量与同代恢复。** 每条投影沿用 Harness 的 16 KiB 上限；单次待送达批次至多 32 条且总编码不超过 128 KiB，每个任务最多暂存 256 条未交付投影及 4 MiB 正文，身份/水位另计为有界元数据。达到较低上限即在新 turn 准入前 backpressure，继续有界回查和排空；不能淘汰未交付结果、跳过 gap 或无限拼 Prompt。读页、buffer、已 ACK 水位、交付输入和原 invocation 必须从同一已提交本地代际恢复，原始结果继续从世界 authority 验真。分裂代际、缺水位或 outbox 丢失使 projection 暂停；不把缺记录当 sequence=0 或全部已交付。已 ACK 的摘要可按原保留合同压缩，仍有效 session 的水位不能随摘要淘汰；超出可恢复窗口时明确阻塞并走受支持的 session 重建，不重用旧身份。

本节上限是初始目标，不证明当前实现产能。状态 schema 与 driver 输入能力同版本发布；旧本地记录没有交付位点时为未确认，不能升级成已消费。机械持久接收和结果回查不启动模型；交付是否需要新调用仍受既有模型预算和当前执行权限约束。

<a id="adapter-autonomous-entry"></a>
### 5.1.2 自主入场编排与 owner 控制隔离

Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；承接 [REQ-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-018) / [AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018)。可信客户端 `cold-entry` 编排按本节实现，操作入口见正式 Skill 的 `references/cold-entry.md`；executor worker 不承担 owner 操作，真实 Runtime 与公开部署支持仍须按各自证据判定。

SDK/CLI/Skill 从[公开 bootstrap 合同](external-agent-game-api.design.md#api-autonomous-entry)发现实际能力，按阶段完成或恢复认证、准入、角色、canonical owner/必要 starter、task/goal、delegation/executor。机械步骤可连续执行，必要的人类确认、凭据仪式和费用批准返回 action_required 并等待；不把没有批准当默认同意，也不要求 operator 先写好 Agent/task/grant。每阶段首次发送前按 API bootstrap 合同将原 canonical 正文/签名证明/nonce/correlation 写入受保护 outbox，非秘密阶段摘要只存引用；响应未知时恢复原阶段，不重建账号或重新领取资源。

owner 控制是独立受保护的控制通道，不加入默认 GameRequest 工具 allowlist。自身参与主体的 owner 操作只在明确覆盖的资源/用途/费用与确认规则内自动化；长期 owner/登录凭据不可进入模型 Prompt、默认 executor 工具、报告或游戏服务。登录身份 key、owner 控制 key、delegate key 的用途与资格分别验证；不把同一进程或自报“自己的角色”当作权限。owner signer 仅接受独立控制面发起且满足既定 owner policy 的窄命令；普通 executor 工具不能选择 owner 角色、调用该 signer 或以模型输出建立 policy。owner signer 必须核对原业务意图、世界/Agent/task、domain、授权范围和当前资格，拒绝任意签名字节或来自游戏内容的提权指令。

可信 CLI 提供独立的冷入场编排入口，消费本地明确批准的单一参与主体配置，不进入 executor broker 的命令白名单。配置固定 identity 服务/环境/epoch、world/genesis/service key、独立 identity/owner/executor 目录、完整私有目标、starter 选择与独立 executor 标识；首次初始化和联网执行分开。执行前从公开发现核对实际 participant/owner/executor 能力和所有信任 pin；能力缺失时在注册和创建 vault 前停止，不将未发布 Runtime profile 解释成已验收组合。

编排沿用现有身份和 owner SDK 的受保护原操作存储：每阶段使用稳定且不同的 operation ID，配置与私有目标绑定到原编排，重启不能因修改配置或丢失状态而重新创建身份、替换原请求、换 executor 或增加额度。逐阶段确认 identity、participant、必要 starter、task、goal publication、delegation 和 executor binding；pending/unknown/rejected/action_required 立即停止本次推进并保留原阶段，不将 HTTP 成功当 canonical 成功。恢复相同阶段先查询原请求。已有完整角色/任务必须核对原归属与 fence，不自动接管他人资源或重复领取。

完成后重新读取当前 canonical owner、目标和 task fence，只有固定独立 executor 的 Active binding 与原目标一致时，才输出供可信 launcher 使用的 executor 配置；输出不含 owner/登录私钥或私有目标。该配置与后续受限工具/profile 的发布仍遵守独立登录、owner、delegate 隔离及不可重置预算规则。客户端编排完成不等于 Runtime 已启动或公开支持验收完成；discovery 的完整支持声明须有对应真实组合证据，不能仅由开关或自报 profile 推导。

executor 仍使用独立短范围凭据和真实 delegation，复用原控制/决策循环。Runtime 同时处理自有和受托资源时，按主体/world/Agent/task 隔离会话、signer、目标、预算、pending 和私有上下文；切换资源必须显式绑定，不能默认继承最近一次 owner 会话。撤销一份委托只停止其对应工具出站；全局凭据撤销则按实际作用域停止。CLI 的发现、注册/登录、入场、恢复、目标和绑定能力只是同一 API 的客户端，不得获得 operator 或直写世界权限。

当前 private loopback、import_only 与 prepared-world 路径保持原支持边界；未公布安全公开能力时客户端报 unsupported/blocked，不自动将私有服务改为公网或改走旧 Bridge。验证沿 [api-cold-entry](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-cold-entry)和 [api-auth](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-auth)，真实 driver 组合各自留证。

<a id="adapter-drivers"></a>
### 5.2 两个 driver 的具体接入

**Codex driver**：首期使用本机 `codex app-server` 的 stdio 通道，完成 initialize，建立或 resume 专用 thread，通过 turn/start 驱动并持续消费 turn/item 事件，turn/interrupt 仅作为取消请求。明确禁止直接复用或注入用户不相关的聊天 thread。固定协议生成 schema，并把 thread ID、提交响应 ID、实际 started/completed turn ID 分别关联，不能假设所有版本回包字段相同。若所选版本的 stdio 协议或必需能力不满足，拒绝该组合；远程 app-server WebSocket 不作为首期默认依赖。`codex exec --json` 可作为另一个需要独立验收的 driver profile，不自动当作 app-server 的无损 fallback。

**OpenClaw driver**：使用用户已授权 Gateway 的 WS/RPC 会话，固定 agent/session 与调用关联；提交、等待/读取结果和会话历史映射为同一运行记录。Gateway token 只在 worker 本地，网页不直连用户 Gateway。CLI 路径仅作明确选择的 profile：`openclaw agent` 的 timeout 以秒为单位，worker 的整体预算独立控制。Gateway transport timeout 可能意味着任务已经接受，必须先核对原 run/session；禁止自动改跑 `--local` 或重建 session 来“恢复”。没有可靠完成/取消读回能力时标记 run_unknown，暂停新调用而不是重复推理。

以上 method 和命令来自官方接口，但其实际可用性需按目标版本分别验证。驱动 manifest 记录 runtime version、协议/schema hash、模型/profile、sandbox/approval policy、resume/cancel/result-query 支持和已验证平台；不把 README 中的名称作为兼容保证。

## 6. 接口与数据合同

### 6.1 Driver 与工具

```text
RuntimeDriver:
  inspect() -> DriverCapabilities
  open(task_binding, permitted_local_profile, resume_ref?) -> RuntimeSessionRef
  start(session_ref, invocation_key, bounded_input, deadline) -> RunRef
  poll_or_stream(run_ref) -> NormalizedRuntimeEvent
  request_cancel(run_ref) -> CancelRequested | Unsupported
  reconcile(run_ref) -> Completed | Failed | Active | Unknown

NormalizedRuntimeEvent = {invocation_key, runtime_session_ref, run_ref,
  provider_submission_ref?, event_id, kind, observed_at,
  summary?, usage: UsageReportV1, result_ref?}
UsageReportV1 = {status: unknown, reason}
  | {status: reported, coverage: complete|partial,
     measurements: UsageMeasurementV1[], missing_metrics: string[]}
UsageMeasurementV1 = {measurement_id, meter_id, metric,
  amount, unit: tokens|calls|currency, currency?, source_ref,
  sequence, is_final}
```

`invocation_key` 为 worker 本地固定调用身份，不是 Game API 的 action 幂等键或世界 turn ID。外部 Runtime 的 thought/reasoning stream 不进入玩家报告；只投影必要的计划摘要、工具状态与错误。缺少 usage 保留 unknown，不算零。

**实际用量合同**：`UsageMeasurementV1` 保存 runtime/provider 确实报告的数值，而非根据定价估算。metric 首期为 input_tokens、output_tokens、cached_input_tokens、tool_calls、cost；相应 unit 为 tokens、calls、currency。amount 使用非负十进制字符串：tokens/calls 是 u64 整数，cost 最多 18 位整数与 9 位小数；cost 必填明确币种，非 cost 不允许币种，禁止浮点、负值、静默截断和自动汇率换算。source_ref 是有界、不含秘密的原计量记录关联，不是任意外部 URL；measurement_id、meter_id、source_ref 各最多 128 UTF-8 bytes，sequence 是 u64 十进制字符串。

同一 `(invocation_key, meter_id, metric, unit, currency)` 的 amount 始终是该计量作用域的**累计值**；driver 若只收到 delta，先按稳定原事件 ID 在持久状态中去重累加，再输出累计值，不混用两种模式。meter_id 在 poll/stream/reconcile 和进程恢复间稳定，标识无重叠的调用计量范围；不能把相同费用同时作为父运行总计和子调用明细相加。不能证明计量范围不重叠时保持 coverage=partial，不伪造总数。

worker 对较新 sequence 替换该 meter 的上一累计值，而不是把每次累计通知相加；同 measurement_id 同内容或同一计量键/sequence 同内容只消费一次，异内容为 usage_conflict 并保留原值和未核对标记；乱序旧 sequence 不覆盖新值。同 meter 累计值倒退或终值之后有冲突更正时先核对来源，不擅自归零或通过退款调整扩大预算。cached_input_tokens 是 input_tokens 的子集，不能为“总 token”再相加；金额按币种分别汇总，报告部分覆盖时同时保留已知值和缺失项。

reported 必须包含至少一个有效测量；没有值使用 unknown，缺少费用不因有 token 数而写成零费用。missing_metrics 明确未报告/不可确定的指标；coverage=complete 仅表示该报告时点已覆盖 driver manifest 声明的计量范围和指标，任一缺项或范围未知必须 partial；is_final=false 表示可能继续增长，取消也不能代签计量完成。每事件最多 32 个 measurements、8 个 missing_metrics，usage 编码最多 8 KiB、完整标准化事件最多 16 KiB；超限或未知指标不能无声丢弃并标为完整，保留有界错误与 partial/unknown。运行日志、摘要和游戏内资源账不替代该来源明确的模型用量。

GameRequest 是一个受限 HTTP 请求适配工具，不是完整游戏 CLI 产品。只允许当前配对 world/task/Agent 的固定 Game API 路由和 scopes，输出 JSON；不允许任意 URL、任意 header 或改变 actor。worker 可以通过 scoped 本机 IPC 提供它，避免向模型 Prompt 暴露 token；原始 HTTP 客户端仍可按同一合同直接使用自己受控的短凭据，不依赖此工具。

游戏访问仅允许 `context / outcome / query / action / operation / task-read / goal-receipt / report / question / question-read / answer / message-receipt / approval-request / events` 类调用；owner 决定、线程 close、owner viewed 与 goal 写入默认不授予。question-read 包含当前获权原线程及原私有操作回查，message-receipt 仅对应本 executor 实际收到的消息；不允许指定别的 actor 或接收方。来自地图文本、报告、Skill 扩展或问答内容的“提升权限”是数据，不能改变工具 allowlist。操作系统 shell、文件、外网和支付等权限继续由实际 Runtime 的本机 sandbox/approval 控制。

<a id="adapter-question-delivery"></a>
### 6.1.1 问答的实际接收端与有界处理

本节承接 AC-COLLAB-004，消费 [API 私有问答合同](external-agent-game-api.design.md#api-question-delivery)，不另定线程状态。默认 executor 工具没有 owner 身份；自有与受托 Agent 同样按真实角色处理。完整 TaskSnapshot 使用 pending_question_threads；实现若只懂旧 unanswered_questions，不得忽略新集合而宣布恢复成功。现有已实施 SDK 控制子集不因此改变 wire 或自动支持完整问答。

1. 控制循环读齐同切点分页及 after-S，或收到增量后核对任务、当前 goal、owner/audience 与 executor fence；不适用的旧线程只保留获权历史，不交给模型当当前指令。未回答问题和已回答未处理的答案都需要完整原文，不以摘要、message_id 或旧的收到报告代替。
2. 对本 executor 收到的问题或答案，把原完整消息、question_id/message_id、thread_revision、scope、去重位点及接收报告原请求原子写入受保护 inbox/outbox，完成本机持久化后提交 received_reported，delivery_basis 固定 durable_inbox。客户端无可验证持久存储时保持 blocked，不能谎报 durable_inbox，也不能改报仅允许玩家 surface 的 volatile_client。
3. 问题接收报告已持久确认，才允许 Runtime 的答案通过 answers 出站；答案接收报告已持久确认，才允许其处理结果通过 receipts 出站。原接收报告响应丢失时，先查原 QuestionWriteResultV1 或原线程报告，确认后继续；不因已经生成了回答就自动补 ACK。控制循环可在没有模型运行时完成接收，但不能代模型制造 considered、cannot_use 或其他处理结论。
4. Runtime 的真实回答或有界处理结论持久写入原 outbox，再按原键提交。executor 处理收到的答案只允许 considered、needs_goal_change、needs_authorization、cannot_use，并可关联实际 runtime session/run/result 的受保护来源；driver 的完成、heartbeat、工具返回或默认 Wait 都不能自动生成该报告。接收方未作处理时保留 answered_pending_processing。答案本身证明对问题的后续回答，不再给问题发送 processed_reported。
5. 回读原私有结果与当前线程后更新显示；persisted 只表示服务已保存报告。处理报告不签发 grant、不修改目标、不释放原 world operation 的未知状态，也不证明模型语义服从。需要新目标、授权、追问或世界行动时调用各自已有入口并重验当前权限，不把消息文字转成这些命令。

本机保护复用 §7 的专用权限目录、fsync/原子替换和受限诊断，不新增第二份游戏权限账本。收到撤权、目标变化或 Stop 后立即停止新的相关问答/工具出站；已经提交但响应未知的私有写入仅在当前仍获权时查询原件。新 epoch 不继承旧问答阶段，原终态/迟到成功回执保持 stale。清理或丢失本地 inbox 后，先从完整服务端当前线程恢复正文；服务器已有 durable_inbox 报告不能证明这个新进程仍有原件。

本节的持久 inbox 要求只针对 executor；玩家 Web/native 按协作合同使用 volatile_client 内存接收，重开后重读服务端，不隐式引入浏览器私有正文持久缓存。接收声明不等于人已看；owner 显式 viewed 仍在原玩家入口。线程、消息和阶段都有固定身份，两个窗口/运行的竞答或不同处理结论必须回读原胜者，不能靠新键覆盖或无限积累阶段。

每个完整线程最多一问一答与三条阶段报告，消息/线程/页大小沿协作合同限制；5 次最小串行私有写入及额外回查计入同一任务剩余请求/时间预算，机械接收与回查不调用付费模型。没有预算处理时只报告 blocked/等待，不虚构处理终态；不能因消息恢复而重启原 run_unknown 或重投未结算世界操作。Stop 的控制通道独立于这 5 个阶段，真实取消和费用边界仍按 driver 原合同，不作即时停费承诺。

<a id="adapter-skill"></a>
### 6.2 Skill 的单一源与分发

新增目标源为 `skills/oasis7-play/`，包含 `SKILL.md`、`references/game-api.md`、`references/collaboration.md`、`references/errors.md` 及支持范围 manifest；`name/description` 使用两 Runtime 可识别的基础格式。模型首先读取简短说明，再按需读 API/状态，不把完整静态动作目录烤进 Prompt。本文仅规定将来交付位置，不在本 PR 安装或执行 Skill。

发行资产包含 source commit、Skill 版本、每文件 SHA-256、API major、driver profile 和所需能力；来源应由受信 release/签名发布链验证，不能只拿同一不可信下载站的 checksum 自证。安装明确选择目标 Runtime 的受支持 skill root，不覆盖用户同名内容；拒绝越界路径、符号链接逃逸和自动高权限安装。升级原子替换、会话下一轮重新确认有效 Skill 版本；不兼容 API 则停止，不静默使用旧说明。

Skill 必须教会：确认目标世界/身份 → 获取当前 task/goal → 动态发现能力 → 保留原 operation ID → 查询结果 → 区分问答/授权 → 处理 stop、版本变化和历史缺口。问答说明须包含完整未完线程、问题/答案先接收后续处理、原键回查、单答案竞态和更正路径，不能只给发送问题的示例。明确说明常驻、后台及付费条件；Skill 不含 token、系统密码、自动充值或停不下来的无限循环脚本。

## 7. 状态、事务与持久化

worker 本地保存 `task_binding / executor_epoch / current goal revision / event cursor / runtime_session_ref / active RunRef / pending client_operation_ids`。记录存于用户专用权限目录，写入使用临时文件、fsync 和原子替换；凭据用系统安全存储或受限文件，与可导出的诊断分开。用量另外保存受限的 measurement 去重身份、每 meter 最新 sequence/累计值、计量覆盖范围与 final 标记；该状态与 inbox 消费位点原子持久后才 ack。事件压缩不能删除仍可重放窗口内的去重索引；无法恢复计量状态时先与原来源对账并标 usage_recovery_required，不能把余额恢复为未消耗。

worker 运行记录必须先持久保存 invocation_key，再启动 driver；若崩溃发生在上游接受之后、RunRef 回包之前，重启先查原 session/调用记录。无法确认则 run_unknown，不能自动 start 第二次。上游不提供幂等时不声称推理 exactly-once；世界副作用仍由 Game API 幂等/fence 保证。

世界操作返回 `OperationViewV1.status=recovery_required` 与模型 run_unknown 分开处理：保留原 client_operation_id/context/关联，阻断该 Agent 新副作用，不以重新启动模型或轮次绕过；继续按 Game API retry_advice 查询同一操作。只有权威结果已对账且当前许可有效，才决定下一轮。context 到期或 worker stop 不意味着旧世界操作已取消。

重复通知按 event_id 与固定关联去重；本机 inbox 持久后才 ack。恢复先核对世界 binding、当前 executor epoch、最新目标与未决 operation，再按[私有上下文恢复](#adapter-private-context-recovery)决定接续本地 session 或依据当前事实重新规划。

问答另保存完整当前线程及 received/processed 的原操作身份和待发正文，按 §6.1.1 对账；有限事件窗口压缩不能删掉已保存未处理答案。若模型已经接受调用但处理报告尚未持久形成，先 reconcile 原 invocation_key/run_ref，仍未知则保留 run_unknown；不由接收报告推导模型已处理，也不为补一条处理报告自动再次运行。服务端私有写入 recovery_required 与模型 run_unknown、世界操作 recovery_required 分开显示并各查原件。

<a id="adapter-private-context-recovery"></a>
### 7.1 私有上下文状态与可信重规划

本节设计补核：2026-11-04；固定输入 `c3862d479ba835bddde3539e8e8e97c5cb563166`。新增合同与验证方法不更新本文其他专题的历史审读范围，不代表实现或真实 Runtime 已通过。

Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；承接 [AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008)。本节适用于声明支持的 supervised 或 attached Runtime/profile，消费既有[任务恢复切点](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-recovery)、[同切点分页](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-snapshot-pages)和原操作恢复合同，不增加世界 journal、授权或会话迁移协议。这里的外部 driver 运行核对不改写[认知生命周期](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#53-commit-and-delivery-guarantees)对同 provider_invocation_key 的既有 at-least-once 投递、journal 恢复及 profile 语义。

**先分清丢失的对象。** 私有上下文指实际 Runtime 自己保存的会话、记忆和本地计划；服务端完整目标正文、当前授权、未完协作义务及原世界结果仍是任务恢复的必需输入。私有上下文不可接续不等于任务或世界历史丢失；缺少必需目标正文也不能降格为“只丢了记忆，可以继续”。世界服务没有读取外部私有记忆的 authority，心跳、保存目标或打开旧 session 都不证明记忆内容已恢复。

**有来源的恢复摘要。** driver 依据实际 inspect/open/reconcile 结果报告，worker 保留受保护来源记录和观察顺序；模型自述不能替代这些记录。摘要经既有 [Report](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-content) 的 phase、summary、blocker、next_actor 和 evidence_refs 承载，沿原 task、目标版本、来源执行方及私有受众投递，不新增协议枚举。绑定必须能核对到 world/genesis/branch、Agent/task、owner revision、executor/epoch、goal revision、当前 audience/读权限版本及实际 Runtime/profile；本机另保存原 session/run/invocation 的关联。来源执行方取自认证绑定，不能由模型在报告正文里切换。

| 摘要内容 | 来源与可表达的事实 | 不能推导的结论 |
| --- | --- | --- |
| 私有会话可接续、不可接续或待核对 | driver 已确认的专用会话接续结果及限制；未找到、格式不兼容、无 resume 能力等原因与暂时无法查询分开 | 可接续不保证所有旧记忆完整；暂时超时、缺报告或观战断连不等于记忆已丢失 |
| 当前任务基线与阻塞 | 当前获权快照的切点、目标及控制版本，完整度和原操作核对状态 | 旧计划、缓存摘要、收到 ACK 或空的未决列表不能替代完整当前态 |
| 重规划进展 | 尚未开始、实际 Runtime 已开始、已产生关联的新计划/明确阻塞；说明旧私有上下文是否参与 | 新计划不证明旧记忆恢复、模型理解、授权增加或世界效果 |

报告保留原观察时间、reported_at 和服务端 received_at；重试复用原稳定消息身份与原内容，不能把重发时间当重新核验时间。worker 不把旧 session/run 的迟到输出改标为当前恢复结果；消费者结合当前 scope 与可核对的来源先后展示，不能只取最后到达的文字覆盖现状。无法判断来源先后时保留“最近已知/待核对”。通信过期沿 [adapter-liveness](#adapter-liveness) 降为未知；owner、goal、executor、branch 或读权限变化后，旧报告仅按当前资格保留历史，不成为新范围的恢复证明。

**从原任务继续。** 控制同步独立于模型调用，先核对身份和原件，再闭合以下恢复依赖。完整基线不足只阻断依赖它的新决策；当前获权的原操作核对和 Stop 仍走原通道。

1. 核对当前身份、世界绑定与读取资格，恢复本机原 invocation/run、operation、inbox/outbox 及预算记录。只查询本案受权专用会话；缺原件或无法判断原调用是否已接受时保留未知，不以新 session、目录、nonce 或 operation ID 消除它。无需模型参与的状态核对不启动付费调用。
2. 读齐同一 S 的全部 snapshot 页并接续 after-S，验证完整 active_goal 正文、激活版本、当前执行许可及预算，取得当前能力和受权观察。恢复未答及已答未处理线程、待决申请、未结算 operation 和当前报告；旧批准、已处理答复及历史报告不按事件顺序重演。缺正文、坏摘要、缺页、过期游标或无法接续的历史缺口按原合同重新同步或 recovery_required，不能以空目标/摘要继续。
3. 分别核对原模型调用和世界操作。原 run 仍 active 时追读同一次调用；run_unknown 时暂停新调用。已完成/失败的 run 先回收原结果，不能据此认定它提交的世界操作成功或失败。世界 pending/recovery_required 仍按原键查询，并保持原合同的新副作用限制；当前获权的只读查询、任务沟通和原结果核对可以继续。无法恢复计量状态或开销仍未知时保留原预留及相应预算限制。
4. 原运行已核清、当前世界操作允许继续且基线完整后，把当前事实交给实际 Runtime。可接续的会话也须接收新目标、授权和已确认结果；旧记忆只能提供非权威背景。旧私有上下文不可接续，或仅其内容仍待核对时，可以不使用它，以当前受支持 profile 建立隔离的新会话重新规划；报告明确“旧私有上下文未接续”。现有委托允许同设备续接且权限/预算有效时无需统一增加人工确认；失权、到期、换设备或超范围则走原授权路径，不能复活旧批准或自动切换 Runtime/费用路由。
5. 新模型调用前持久保存这次调用的 invocation 原件和基线关联；输出与同次实际 session/run/result 对账后，才报告新计划或明确阻塞。保留基线切点、goal/fence、输入送达和真实输出的受保护关联，不保存推理链作为证据。每次新的受限工具出站继续重验当前 context/goal/fence/grant；恢复中发生目标、权限或分支变化时重新同步，旧输入和迟到结果不能换标签后使用。模型未运行、无输出或只有 heartbeat 时仍显示相应阻塞，不生成默认计划或成功。

摘要原件与关联先写入原受保护 outbox，再经现有 reports 发送；断连或保存响应未知保留原消息身份和待核对状态，不假称玩家已收到。报告投递不产生新执行许可，接收确认也不授予额外权限。Viewer/pure_api 消费者分别展示执行方说明、当前限制和可继续的下一步；只有实际新输出关联闭合才可说明重规划已有结果，原私有上下文未接续的限制仍在恢复历史可查。后续成果独立读世界 receipt，计划出现不替代它。

**隔离与有限恢复。** 不自动复制另一个 Runtime、Agent、owner 或 executor 的私有记忆；同一进程中的 A/B 也分别绑定来源、目标、预算和原操作。新 owner 不默认取得旧私有历史，切身份或失去读权清除玩家 surface 的受限内存，沿原协作合同重读，不新增私有正文持久缓存。当前 audience/读权收窄后，不能继续使用含已失权资料的缓存或旧 session；停止向其注入新输入和受限工具出站，重新取得合法基线后才可在隔离会话继续，不把旧正文或摘要搬入新会话。停止复用不承诺删除第三方 Runtime 已持有的历史。

报告只含有界脱敏摘要和获准关联，不上传完整会话/Prompt、私有记忆片段、推理链、凭据、本机路径、可枚举私有 digest、可重放 cursor/签名或任意结果 URL；原来源证据只留受保护记录。沿现有 report 8 KiB 正文和完整编码限制，不把快照/历史打包进报告。读取、对账、报告和重规划均消费同一任务剩余请求、时间与模型预算，不清零累计用量或预留。

旧 reader 或 driver 只能提供无来源的一般文字时，保留为执行方说明并标明无法核对，不能转换成“恢复完成”或完整能力声明。两 Runtime 各自的版本、resume/result-query 能力、私有会话不可接续与可信重规划实际链路，按 [adapter-private-context-recovery](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-private-context-recovery) 分别验证。

## 8. 部署、安全与运行约束

<a id="adapter-liveness"></a>
一次安装、登录成功或健康检查不能证明 Runtime 正在推理。worker 显示 active run、最后控制同步时间和最近已确认结果；过期状态变 unknown/offline。heartbeat 初始周期 10 秒、30 秒未见标记通信过期，二者仅是诊断目标，不替代 world epoch/授权判定。

所有远端调用由用户机器向外发起，不要求公网入站端口。只有受信用户显式选择本地 profile 时 worker 可启动对应可执行文件；参数以数组传递，不拼 shell。只能终止自己启动的进程组；不清理其他 Codex/OpenClaw 会话。网页 stop 先影响游戏许可，不偷偷执行本机管理命令。

本机工具审批与游戏授权分开呈现；Game API 批准建设工厂不能批准读取用户文件或购买模型额度。日志默认脱敏，模型账户 secrets 不发给游戏服务器；用户选择任何额外模型路由均需明示费用来源，不自动选择其他账户或套餐。

Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e。以下四层组成 [AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007) 的同一状态解释，分别有来源和新鲜度；不能合成为一个“在线”布尔值。

| 独立层 | 唯一来源与有效事实 | Web/native / pure_api 的共同消费 |
| --- | --- | --- |
| 玩家观察连接 | 对应 observer 的真实连接/订阅及最近成功读取；不同窗口分别计 | 只说明这个观察端可否取得当前数据；关窗/断开只释放自有视图资源 |
| Runtime 执行 | driver 的实际 run/session、started/completed/failed/unknown 及最后可核事件；控制 sync/heartbeat 另外标时 | 正在推理须有匹配活动 run 的证据；只同步控制、安装成功或健康检查不能写 thinking；来源过期写未知 |
| Agent 执行资格 | Runtime task 状态、当前 owner/delegation/fence、期限、额度及必须的本机批准 | 只说明目前可否开始对应工作；执行诊断不能续期，历史 grant/本机缓存不能代表当前权力 |
| 世界原结果 | 原 operation 的持久 disposition、receipt/finality 和相应工业结果 | 原已受理工作由世界继续处理；网络/renderer/模型状态不能把它改为取消、失败或成功 |

状态投影使用既有 task snapshot/events、受限 report、driver 诊断和原 operation 读取；报告保留 world/Agent/task、原/当前 fence、来源时间与序号，不新设 liveness authority。相互来源尚未同步时各层标明时点/未知，不声称形成同一原子世界快照；权限先于详细诊断展示。pure_api 返回同样四类可消费事实和下一步，Viewer 用玩家语言展示，无一端的截图或 ACK 能替另一端证明。

无 GUI 的持续条件是：原 worker/attached Runtime 的实际宿主仍运行、网络可达、独立于观察会话的合法绑定有效、本机审批与预算允许。`viewer` 关闭窗口、`pure_api` 断开观察客户端都不改变这些条件；如果实际部署的执行器寄生于该页面，必须如实标为不具备此独立组合，而不能宣称首期无 GUI 已满足。网页不能暗中启动后台进程以补齐条件。

| 变化 | 允许继续的范围与必须显示的结果 |
| --- | --- |
| 只关闭观察端，独立条件仍成立 | worker 可继续真实决策；重新观察核当前快照和离开期间原结果，不重启任务或重做动作 |
| worker/driver 停止或 run 状态未知 | 停止新启动；已受理世界操作继续可查；控制 heartbeat 不能填补一个消失的推理进程 |
| 执行资格到期/撤销或模型预算耗尽 | 停止对应新模型/工具/行动，best-effort 取消仅属本机请求；不能保证云端已取消或费用立即停止 |
| 世界/授权读取不可用或响应过期 | 不发新副作用，分别保留最后可信状态与未知；恢复后重核原件与当前资格，不能把 world 断连视作 worker 已退出 |
| 读取资格也消失 | 按 [当前读权](external-agent-game-api.design.md#api-result-feedback)隐藏旧私有投影、停止无权回查，提供有权 owner 的合法路径或明确阻塞；不自动接管 owner 身份 |

回访和重启都先恢复原 operation、反馈水位与实际 run，再按完整当前绑定决定是否恢复执行；仅 observer 重连不会增加预算、创建新 executor 或重新授权。已授权同设备可按既有 CAS 取得新执行代际，旧迟到请求仍拒绝。正常无 GUI、关闭观察、停止执行、失权、重启分别是独立验收，不以其中一种替代其他。

## 9. 质量与容量

<a id="adapter-budget"></a>
新执行的网络请求和 driver 调用消费同一任务剩余执行预算；retry 不重新获得完整 deadline。Stop、当前控制读取和原结果回查使用独立有界的控制/读取配额，不消耗新的行动或模型调用额度，仍受当前读权、有限超时与服务限流。控制 long-poll 使用 Game API 上限；空闲轮询初值 2 秒并可指数退避，心跳无需调用模型。模型 turn 的初始 deadline 可设 600 秒但必须有有限用户配置；本机中断宽限初值 5 秒，之后只结束自有进程。

可强制限制的 worker 请求数、并发、时间和出站工具调用数与“runtime/provider 报告 token/费用”分开。无法拦截 Runtime 内部子调用时，报告 budget_enforcement=partial，达到外层限额停止新启动，但不能声称下游费用已被硬封顶。严重费用未知或上游 run_unknown 时保留保守预留，先人工/可信状态核对，不重复启动以试探。

预算视图按任务汇总不重叠 meter 的实际已报告用量，并分别呈现估计、保守预留和未知部分；重放通知、流式中间累计值与最终累计值不能重复扣减。现有任务 report 只发布有界、脱敏的计量摘要及来源关联，不上传本机私有 ledger 路径或凭据；driver 报告仍不是世界结算或账单审计证明。未知/未终结用量保留预留，无法拦截的下游消费仍为 partial enforcement。

Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43。承接 [AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) 的两类成本及停权后结果读取。下表只是现有来源的联合读面，不创建统一扣费账或把世界电力换算成货币。

| 成本/限制 | 实际来源与归因 | 未知、拒绝和停止边界 |
| --- | --- | --- |
| 外部模型/工具用量 | 原 invocation/run 的 UsageReportV1、稳定 meter、单位/币种和报告覆盖；实际值、估计、预留分别显示 | token 不推导实际费用；取消不证明 final；同币种无重叠来源才可相加，未知不记零 |
| worker 可执行限制 | 当前用户配置与已持久预留的调用次数/并发/时间/出站工具数；原任务恢复不重置 | 内部不可拦截调用保持 partial enforcement；没有新的批准不加额、不换账号或另开 run |
| 世界资源投入与占用 | 原 operation/request 与业务 job/root、实际 payer/owner/ledger、资源单位、当前授权、匹配生产/费用 receipt；报价、hold/reserved、consumed、released 各保留身份 | cognition accepted/committed 不代替业务生产或成本；一个拒绝可能已有另一个合法费用/预留结果，须查原 authority，不能自动退款或报零 |
| 未来维护及继续成本 | 当前玩法/设施合同的条件化报价、维护义务与有效期，明确为未来估计 | 不与已消费值相加成实际总额；新条件/期限变化重新报价，不靠旧预算展示授权新效果 |

世界成本读取沿 Game API 的受权观察/工业 query 和原 operation 结果，以原业务 receipt 的行项或稳定 effect identity 去重；同一批生产的排程、completion 和显示摘要不能各算一次。原生资源、模块调用 Data/Electricity、材料与外部货币分别按真实 authority 归因，不从全局余额差估算“这个任务花了多少”。缺 payer、lineage、金额或完整成本范围时保留已知分项并标未知，不能宣称总成本已齐。一次读面只使用一个可核世界切点；另有晚到费用时在原身份下追加当前事实，不逆写历史消费，也不因跨切点重读重复加总。

达到限制先持久记录已用/预留及原运行关联，再关闭新启动和对应受限工具出站；仍按当前读权保留原请求和费用回查。后续收据到达只完善原成本与结果，不自动释放上游未知预留或触发新认知。若执行权限撤销但 owner 仍可读取，由原受保护 owner 路径查询；executor 不借这一需求选择 owner signer。读取也失权、配额到限或服务不可用时明确相应阻塞及 retry_after，保留原件，不强行保证“仍可读到”。

控制/原结果的后台读取初始共用至多 1 个在途调用、2 秒最小轮询间隔和 30 秒单请求 deadline，可按服务 retry_after 延长间隔。合法 Stop 使用独立、至多 1 个在途的优先控制槽，单请求 deadline 10 秒；它不等待后台 long-poll、退避计时或模型/行动额度，可取消本客户端自有只读 long-poll 释放资源，但不能因此丢弃原结果定位。Stop 重试保留同一稳定请求身份；收到不等于已失效，仍回读权威 task/fence，网络失败明确保持停止待确认。服务器的控制优先队列与有界配额同样独立于新 action 和历史读取队列，不能让普通请求排满后永远无法受理合法 Stop。

任务控制授权持续有效时可继续有界观察，执行预算不会因此恢复。每次实际请求均在发送前再验权限，本机请求数/字节仍计诊断；断网后不得无限并发重试。控制恢复不调用模型，未知模型 run 的取消/核对仍使用原身份。世界、控制和本地预算代际分别复核；旧备份不能复活已耗调用、旧 fence 或已释放的世界占用。

首期一个 Agent 一个决策执行器，stdout/stderr/event 缓冲必须有界；超出时停止该 run 并保留操作关联，不截掉最后错误后误报成功。计划报告可以合并，授权/停止/结果确认不得被降采样丢失。

## 10. 兼容、迁移与回滚

两个 driver 与 API/Skill 单独版本化；受支持矩阵记录实际 runtime/driver/schema/平台而非写一个任意二进制路径。新 Skill 替代旧 `site/skills/oasis7.md` 的推荐 Bridge 路径之前，两个 runtime 的代表性首局分别通过。

切换 driver 是显式 owner 操作：撤销旧 fence、核对 pending、激活新执行方，私有上下文默认不迁移。回滚 worker/Skill 只在协议和本地状态 schema 可读时进行；不兼容保持暂停并提供明确恢复，不删本地 ledger 以重新获得预算或新键。旧 Bridge 不自动成为 failover。

本次 UsageReportV1 替换尚未发布草案的 reported/unknown 字符串；driver、worker ledger 与测试 schema 同批更新，不从旧 reported 标记猜测金额。旧 ledger 无量值时保留 unknown 并核对，不把缺值迁移成零。

## 11. 验证设计与可追溯性

普通 CI 先用 scripted fake drivers 验证 unknown/cancel/restart/版本冲突和预算；真实模型仅在明确授权的组合验收中使用。CI 不能为文档验证启动付费模型。必须分别展示真实 OpenClaw 与 Codex 承担认知，而非直连模型返回一个 JSON。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法与场景 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-b3e367c3-c72d-4959-b599-613ac652dab8；[AC-EXT-001](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-001) | [adapter-drivers](#adapter-drivers) | 声明 Runtime 必须确实执行且兼容方式可复现 | [adapter-connect](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-connect)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-bd792456-4b98-4891-b5e3-e24b1de4f1a1；[AC-EXT-004](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-004) | [adapter-loop](#adapter-loop) | 在自身会话和工具能力中自主多轮决策 | [adapter-loop](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-loop)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；[AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007) | [adapter-liveness](#adapter-liveness) | 无 GUI 时独立运行，停止 Runtime 不伪装继续 | [adapter-liveness](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-liveness)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；[AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008) | [adapter-private-context-recovery](#adapter-private-context-recovery) | 私有上下文有来源告知、原运行及世界操作核对后可信重规划，当前权限与跨 Agent 隔离 | [adapter-private-context-recovery](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-private-context-recovery)；按该场景区分确定性故障与每个真实 Runtime/profile 的同案恢复 | 候选、原请求/基线/实际输出/报告关联、调用及世界效果次数与权限负例 | fake 不证明真实会话；新计划不证明旧记忆恢复、模型理解或世界效果 |
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | [adapter-budget](#adapter-budget) | 计量、预算和超时边界诚实且可执行 | [adapter-budget](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-budget)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；[AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | [adapter-skill](#adapter-skill) | 两个 Runtime 分别通过可信 Skill 完成自助接入 | [skill-release](../../testing/manual/external-agent-runtime-contract-validation.manual.md#skill-release)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018) | [adapter-autonomous-entry](#adapter-autonomous-entry) | 按真实身份/关系编排入场、确认与恢复 | [api-cold-entry](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-cold-entry) | 两真实 Runtime 各自的阶段与原回执记录 | 不以预置角色或 mock 代签冷入场 |
| Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a；[AC-COLLAB-004](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-004) | [adapter-question-delivery](#adapter-question-delivery) | 执行端持久接收、真实输出关联、响应丢失及未知运行恢复 | [collab-question-delivery](../../testing/manual/external-agent-runtime-contract-validation.manual.md#collab-question-delivery) 的 inbox/outbox/新进程/未知操作用例 | scripted driver 与实际持久存储故障记录；真实组合另按 collab-session | fake 不证明真实 Runtime、玩家已读或模型理解 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | [adapter-result-feedback](#adapter-result-feedback) | 原结果到实际输入的关联、去重、水位、同代恢复与容量完整性 | [adapter-result-feedback](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-result-feedback)；RF01–RF09，required 持久故障与 scripted driver，full 两真实 Runtime/声明表面 | 原反馈/ACK/输入/实际运行关联分层证据及未知/失败；静态方法不是运行通过 | 不证明模型理解、首局全部义务或另一 Runtime |

## 12. 决策、长期风险与未决问题

ADR-ADAPTER-1：薄 worker + 两个显式 driver，拒绝从一个 CLI_BIN 推导通用兼容；API 与 world authority 不依赖 worker 的存在。

ADR-ADAPTER-2：默认 outbound-only 和受限工具代理，网页不直控本机 Runtime。对于无法核对的上游调用，牺牲自动恢复而非重复执行或增加费用。

实施解除项由 agent owner 负责：固定两 runtime 实际版本和 schema、验证 app-server stdio 与 OpenClaw Gateway 的恢复/cancel、证明 Skill 加载与授权 sandbox。若上游接口实验性或行为变化，降为 unsupported profile 并保留已接收任务，不扩大自动化假设。
