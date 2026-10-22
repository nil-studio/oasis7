# 外部 Runtime 适配、Skill 与持续执行系统设计

- 设计状态：target contract，待实现与独立评审；不是当前能力或发布报告。
- Canonical repository：`eng-cc/oasis7`；审读基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2`（已合入产品 PR #4493）。
- Owner role：`agent_engineer`；最后审读：2026-10-10；新增合同 revision：`1`。
- 产品入口：[外部 Runtime 游玩](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)；[玩家与 Runtime 协作](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 系统总入口：[LLM / external runtime 系统入口](README.md)；写作规范：[系统设计规范](../../engineering/doc-governance/system-design-writing-standard.design.md)。

本文中的新组件、路由、记录和测试场景均为后续实施输入。`accepted` 只表示所属权威已受理，世界效果必须读取既有权威回执；文档合入不代表这些接口已经上线。

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
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | 计量、预算和超时边界诚实且可执行 | [adapter-budget](#adapter-budget) | agent_engineer / qa_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；[AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | 两个 Runtime 分别通过可信 Skill 完成自助接入 | [adapter-skill](#adapter-skill) | agent_engineer / liveops_community | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018) | 机器冷入场、必要确认停点、owner/executor 隔离和原阶段恢复 | [adapter-autonomous-entry](#adapter-autonomous-entry) | identity / agent_engineer / runtime_engineer | 不扩张默认 GameRequest 权限，不表示公开入口已开放 |

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

控制循环独立于一次模型推理：核对当前 task/fence，增量读取目标、未答问题及授权变更，维持有时效的诊断心跳。没有执行许可、预算或 driver 时保持 awaiting_executor/blocked，不发起模型调用。

决策循环：读取当前 task snapshot 并验证内联 active_goal 正文、revision/digest 和切点 → 获得匹配该激活版本的 Game API context → 把受权观察、完整目标、Skill 和待处理消息交给实际 Runtime → Runtime 按需查询、提案、汇报或提问 → 回读世界结果 → 在仍有效的许可下继续。worker 不固定“移动三次即完成”之类策略，也不把默认 Wait 替代未取得的模型结果。

Runtime 可以在一次本地推理中调用多个只读工具；等待、阻塞或放弃当前轮次时使用 Game API context outcomes 收口，不能一直占用活动 turn。每次副作用都经 Game API 当前 context/fence/goal/grant 校验。目标变化时允许完成旧本地计算，但旧 context 的新 action 必须被阻断；worker 给出新的上下文后才能重新规划。对硬撤销立即停止新的工具出站，并请求 driver 中断，世界侧 fence 才是最终防线。

<a id="adapter-autonomous-entry"></a>
### 5.1.1 自主入场编排与 owner 控制隔离

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

以上 method 和命令来自官方接口，但其实际可用性需要每个目标版本单独验证。驱动 manifest 记录 runtime version、协议/schema hash、模型/profile、sandbox/approval policy、resume/cancel/result-query 支持和已验证平台；不把 README 中的名称作为兼容保证。

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

游戏访问仅允许 `context / outcome / query / action / operation / task-read / goal-receipt / report / question / approval-request / events` 类调用；owner 决定与 goal 写入默认不授予。来自地图文本、报告、Skill 扩展或问答内容的“提升权限”是数据，不能改变工具 allowlist。操作系统 shell、文件、外网和支付等权限继续由实际 Runtime 的本机 sandbox/approval 控制。

<a id="adapter-skill"></a>
### 6.2 Skill 的单一源与分发

新增目标源为 `skills/oasis7-play/`，包含 `SKILL.md`、`references/game-api.md`、`references/collaboration.md`、`references/errors.md` 及支持范围 manifest；`name/description` 使用两 Runtime 可识别的基础格式。模型首先读取简短说明，再按需读 API/状态，不把完整静态动作目录烤进 Prompt。本文仅规定将来交付位置，不在本 PR 安装或执行 Skill。

发行资产包含 source commit、Skill 版本、每文件 SHA-256、API major、driver profile 和所需能力；来源应由受信 release/签名发布链验证，不能只拿同一不可信下载站的 checksum 自证。安装明确选择目标 Runtime 的受支持 skill root，不覆盖用户同名内容；拒绝越界路径、符号链接逃逸和自动高权限安装。升级原子替换、会话下一轮重新确认有效 Skill 版本；不兼容 API 则停止，不静默使用旧说明。

Skill 必须教会：确认目标世界/身份 → 获取当前 task/goal → 动态发现能力 → 保留原 operation ID → 查询结果 → 区分问答/授权 → 处理 stop、版本变化和历史缺口。常驻、后台及付费条件用明确说明表达；Skill 不含 token、系统密码、自动充值或停不下来的无限循环脚本。

## 7. 状态、事务与持久化

worker 本地保存 `task_binding / executor_epoch / current goal revision / event cursor / runtime_session_ref / active RunRef / pending client_operation_ids`。记录存于用户专用权限目录，写入使用临时文件、fsync 和原子替换；凭据用系统安全存储或受限文件，与可导出的诊断分开。用量另外保存受限的 measurement 去重身份、每 meter 最新 sequence/累计值、计量覆盖范围与 final 标记；该状态与 inbox 消费位点原子持久后才 ack。事件压缩不能删除仍可重放窗口内的去重索引；无法恢复计量状态时先与原来源对账并标 usage_recovery_required，不能把余额恢复为未消耗。

worker 运行记录必须先持久保存 invocation_key，再启动 driver；若崩溃发生在上游接受之后、RunRef 回包之前，重启先查原 session/调用记录。无法确认则 run_unknown，不能自动 start 第二次。上游不提供幂等时不声称推理 exactly-once；世界副作用仍由 Game API 幂等/fence 保证。

世界操作返回 `OperationViewV1.status=recovery_required` 与模型 run_unknown 分开处理：保留原 client_operation_id/context/关联，阻断该 Agent 新副作用，不以重新启动模型或轮次绕过；继续按 Game API retry_advice 查询同一操作。只有权威结果已对账且当前许可有效，才决定下一轮。context 到期或 worker stop 不意味着旧世界操作已取消。

重复通知按 event_id 与固定关联去重；本机 inbox 持久后才 ack。恢复先核对世界 binding、当前 executor epoch、最新目标与未决 operation，再考虑 resume 本地 session。允许会话丢失后重新规划，但必须声明 memory unavailable，不能导入其他 Runtime 的私有记忆或复活旧批准。

## 8. 部署、安全与运行约束

<a id="adapter-liveness"></a>
一次安装、登录成功或健康检查不能证明 Runtime 正在推理。worker 显示 active run、最后控制同步时间和最近已确认结果；过期状态变 unknown/offline。heartbeat 初始周期 10 秒、30 秒未见标记通信过期，二者仅是诊断目标，不替代 world epoch/授权判定。

所有远端调用由用户机器向外发起，不要求公网入站端口。只有受信用户显式选择本地 profile 时 worker 可启动对应可执行文件；参数以数组传递，不拼 shell。只能终止自己启动的进程组；不清理其他 Codex/OpenClaw 会话。网页 stop 先影响游戏许可，不偷偷执行本机管理命令。

本机工具审批与游戏授权分开呈现；Game API 批准建设工厂不能批准读取用户文件或购买模型额度。日志默认脱敏，模型账户 secrets 不发给游戏服务器；用户选择任何额外模型路由均需明示费用来源，不自动选择其他账户或套餐。

## 9. 质量与容量

<a id="adapter-budget"></a>
所有网络请求和 driver 调用消费同一任务剩余预算；retry 不重新获得完整 deadline。控制 long-poll 使用 Game API 上限；空闲轮询初值 2 秒并可指数退避，心跳无需调用模型。模型 turn 的初始 deadline 可设 600 秒但必须有有限用户配置；本机中断宽限初值 5 秒，之后只结束自有进程。

可强制限制的 worker 请求数、并发、时间和出站工具调用数与“runtime/provider 报告 token/费用”分开。无法拦截 Runtime 内部子调用时，报告 budget_enforcement=partial，达到外层限额停止新启动，但不能声称下游费用已被硬封顶。严重费用未知或上游 run_unknown 时保留保守预留，先人工/可信状态核对，不重复启动以试探。

预算视图按任务汇总不重叠 meter 的实际已报告用量，并分别呈现估计、保守预留和未知部分；重放通知、流式中间累计值与最终累计值不能重复扣减。现有任务 report 只发布有界、脱敏的计量摘要及来源关联，不上传本机私有 ledger 路径或凭据；driver 报告仍不是世界结算或账单审计证明。未知/未终结用量保留预留，无法拦截的下游消费仍为 partial enforcement。

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
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | [adapter-budget](#adapter-budget) | 计量、预算和超时边界诚实且可执行 | [adapter-budget](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-budget)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a5bbf6e4-3e71-4d48-9eba-68c50867c95c；[AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | [adapter-skill](#adapter-skill) | 两个 Runtime 分别通过可信 Skill 完成自助接入 | [skill-release](../../testing/manual/external-agent-runtime-contract-validation.manual.md#skill-release)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018) | [adapter-autonomous-entry](#adapter-autonomous-entry) | 按真实身份/关系编排入场、确认与恢复 | [api-cold-entry](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-cold-entry) | 两真实 Runtime 各自的阶段与原回执记录 | 不以预置角色或 mock 代签冷入场 |

## 12. 决策、长期风险与未决问题

ADR-ADAPTER-1：薄 worker + 两个显式 driver，拒绝从一个 CLI_BIN 推导通用兼容；API 与 world authority 不依赖 worker 的存在。

ADR-ADAPTER-2：默认 outbound-only 和受限工具代理，网页不直控本机 Runtime。对于无法核对的上游调用，牺牲自动恢复而非重复执行或增加费用。

实施解除项由 agent owner 负责：固定两 runtime 实际版本和 schema、验证 app-server stdio 与 OpenClaw Gateway 的恢复/cancel、证明 Skill 加载与授权 sandbox。若上游接口实验性或行为变化，降为 unsupported profile 并保留已接收任务，不扩大自动化假设。
