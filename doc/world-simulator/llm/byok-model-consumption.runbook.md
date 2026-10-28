# 会话内 BYOK 模型消费层运行手册

- 生命周期：`active`
- Owner role：`agent_engineer`
- 适用设计：[网页自带模型连接](self-supplied-model-connection.design.md)。
- 验收方法：[BYOK-01～08](../../testing/manual/self-supplied-model-connection.manual.md)。

## 当前交付边界

适用范围：本文记录现有 Launcher 托管临时凭证实现，不是浏览器直连的目标合同。新的 [浏览器模型连接设计](self-supplied-model-connection.design.md#des-byok-001)采用 localStorage 保存 Key、页面内直接调用供应商；尚未实施，不能按本文启用步骤宣称该路径已可用。迁移时须停止旧模型循环并核对原件，不自动回退托管或将旧 Key 导出到浏览器。

`oasis7_model_provider` 提供仅内存凭证、应用会话权威回调、连接 CAS、有限租约、跨连接共享的调用/token 预算、两阶段幂等检查、固定供应商传输和 Harness 适配。Launcher 的受控网页接口复用 hosted identity 的精确 Origin、CSRF、JSON 与数据库撤销校验；现有 HTML CSP/no-store 继续生效。未配置默认关闭，不显示 Key 表单。

当前支持受部署配置限定的 **非流式实验组合**：DeepSeek Chat Completions / Responses、OpenRouter Chat Completions。OpenRouter Chat 必须指定路由允许集合；OpenRouter Responses 与流式输入明确拒绝，不静默删参数或切协议。协议 fixture 不证明真实模型兼容；两家真实供应商验收仍延期。协议来源：[DeepSeek Chat](https://api-docs.deepseek.com/api/create-chat-completion/)、[DeepSeek Responses](https://api-docs.deepseek.com/api/create-response/)、[OpenRouter Responses](https://openrouter.ai/docs/api/api-reference/responses/create-responses)。

内置 Harness 通过受控 Launcher 进程运行，复用同一消费层和已授权 SDK。部署需显式登记既有设备配置及所属账号；执行端核对当前登录、PlayerBinding、服务签名任务视图、完整 Runtime fence 与设备执行器身份。无合法登记时 `executor_available=false`，activate 返回 `game_executor_unavailable`。模型 Key 和登录 cookie 均不能替代 signer、任务/grant 或 Runtime fence，也不传入 Runtime 子进程。

## 启用前提与非敏感配置

首版仅 Unix 受控执行端：启用时进程禁用 core dump；部署还须禁请求体/APM 采样、会话重放、secret 备份及第三方页面脚本。配置中的确认字段是 operator 的部署责任声明，不是自动审计或公开发行批准。反向代理必须保留 HTTPS、精确公开 Origin 与实例亲和，并沿用 hosted identity 的可信代理配置。实例重启后 Key 全部丢失，不迁移、不从宿主环境恢复。

通过 `OASIS7_AGENT_CLIENT_CONFIG` 指向绝对路径的 TOML；该文件 **不含 Key**。`public_origin` 必须为 hosted identity 已批准的规范 HTTPS origin。示例默认关闭，实际启用前填写已审核的模型、协议与预设 revision：

```toml
enabled = false
public_origin = "https://game.example"
display_name = "受控第一方执行端"
location = "经用户确认的数据处理位置"
memory_only_controls_confirmed = false

[limits]
max_calls = 20
max_tokens = 100000
max_input_tokens = 8192
max_output_tokens = 2048
max_concurrent = 1
lease_ms = 120000
absolute_ms = 7200000
renew_interval_ms = 30000

[[presets]]
id = "reviewed-deepseek-chat-v1"
provider = "deepseek"
protocol = "chat_completions"
model = "REPLACE_WITH_REVIEWED_MODEL"
routes = []
revision = 1

[presets.limits]
max_input_bytes = 8192
max_response_bytes = 2097152
max_output_tokens = 2048
timeout_ms = 30000

# 可选：部署前已完成正常 SDK 配对和任务授权的设备，不含模型 Key。
[[executors]]
account_id = "REPLACE_WITH_HOSTED_ACCOUNT_ID"
device_config_path = "/absolute/private/device.json"
```

配置与 Web capability 的 provider 标识分别为 `deepseek` / `openrouter`。OpenRouter 的 `routes` 必须为已审核的非空供应商允许集合；不接受客户端提交任意 URL、header、owner 或路由。

## 运行、限制和恢复

先确认接收方和会话隐私，再提交推理 Key；输入随提交立即清空。保存不计模型调用，不自动 probe 或运行。每次显式检查最多执行两次固定、无世界副作用的请求：工具 echo 与原 call ID 结果往返；同一个 operation ID 不重新发送。出错或结果未知时，保留原检查身份，先刷新状态。新的检查需要明确操作，不能把旧失败变成透明重试。

预算在同一应用会话的所有连接间共享，换 Key/新建连接不清零。发送前原子预留调用次数及完整输入/输出上界；输入按编码字节数及固定 envelope 余量作保守 token 准入。未知、部分、溢出或自相矛盾的 usage 不按零返还；该限制不是供应商账户费用硬封顶。认证、余额、限流和兼容失败不自动充值、重试或改用公共 Key。

续期有版本 CAS、滑动期限及绝对期限，不能复活已过期连接。续期改变版本并保守终结旧版 pending 检查；网页在检查进行中不续期。管理版本用于网页 CAS；激活时冻结运行版本，正常续租不改变运行版本。

执行端在发送、接收结果及每个工具之前重新检查会话、运行版本、运行代、租约与绑定；新上下文准备前也检查。Stop 永久撤销当前运行代但保留尚有效凭证；Forget/退出/撤销/到期清除凭证。已经受理的世界作业依 Runtime 原合同，不由模型连接层回滚。

诊断 `input_limit` 时，先核对历史模式与实际编码大小。SessionModelProvider 自持结构化历史，同一 Context 的后续输入不得再次嵌入累计 transcript，工具结果仅沿原 supplier call ID 交付一次；新 Context 首轮仍需完整当前输入。完整候选请求的无副作用预检先于持久物理调用预留，纯本地拒绝不增加物理请求数或 unknown usage。已预留后的竞态/结果不明仍保守计入，不能据此退款。不得记录完整 prompt/Key 来排障。

目标恢复合同：必要输入真实超限时保持明确阻塞，保留 Context、运行身份、工具结果及原 operation；不自动 retry、新 slot、清空必要历史、扩大限额或重置预算。先查未知原件，独立确认 Stop；重启不能作为绕过容量或预算的恢复方式。修复重复编码后仍须重验当前授权与 fence，普通离线多轮通过不代签真实模型续玩。

当前实现边界：`SessionModelProvider` 的结构化 conversation 只保存在该适配器实例内；`NativeTaskRunner::run(self)` 消耗 runner/provider，Launcher 后续重新开始会构造新的 provider（`conversation: None`）。因此同一存活适配器的 input-limit/preflight 测试只证明实例内历史保持，不证明 runner 退出、Stop/重新开始后所需 conversation/工具历史可恢复。

跨此生命周期的必要历史恢复仍有实现与验证缺口，不能把重新开始当成容量恢复办法或宣称上述目标已全量满足。原世界 operation 的持久恢复与稳定 run 预算是独立合同，仍按下段执行；此缺口不意味着预算可绕过，也不允许暴露私有历史。

内置工业循环使用有限成功效果数和持久物理调用预算；同一设备和完整任务 fence 的稳定 run 身份用于恢复，重启不追加预算。未知原件先查原请求，不替换或重签原件。SDK 校验的 canonical `stale_base` 终结反馈可按既有合同进入新 slot；另在完整任务 fence 与当前执行许可仍有效、全部原件已扫描、原 prefix 已 committed 且无竞争 action/outcome、Context 严格过期时，可按[过期 Context 恢复合同](agent-game-client-boundary.design.md#des-agc-004)提交原身份 `context_expired`。只有该终止 outcome committed 后才取得新 Context，继续使用原 run 身份与剩余持久预算，且不计成功业务 turn。其他 aborted、Unknown/Pending、清理 Rejected、Stop、撤权或 fence 变化均不因此允许继续。

设备登记固定账号和 SDK 设备边界，不把启动时的活动任务永久缓存为权限。任务暂时停止、尚无有效目标或该设备只是备用执行器时，Launcher 仍可启动，但当前执行能力关闭。

任务/目标或执行器 fence 更新后，旧运行代继续失效；玩家须重新确认开始，由服务端读取当前签名任务视图并核对账号玩家归属，创建携带完整 fence 摘要的新代绑定，无需重启 Launcher。该摘要只由受控执行端生成，网页仅提交 Agent 引用；不能用新代复活旧输出或跳过仍占用设备的旧 worker。相同完整 fence 继续复用原 run 身份与预算；不同合法 fence 才形成新的 run 身份，历史原件仍保留。

回滚先关闭配置/重启受控进程，清除内存凭证并阻止新请求。不得恢复旧 Key、旧运行代或重放未知世界操作。

## 验证与未覆盖

局部自动化：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_model_provider --lib
env -u RUSTC_WRAPPER cargo clippy -p oasis7_model_provider -p oasis7_agent_harness --all-targets -- -D warnings
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher hosted_model_client
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher model_session
npm --prefix crates/oasis7_viewer run test:ui -- software_safe_src/viewer_model_connection_client.test.js software_safe_src/model_connection_panel.test.jsx
npm --prefix crates/oasis7_viewer run build:viewer
```

自动化分别验证协议、预算、身份权威与 HTTP 路由；Chrome 桌面/窄屏使用无价值 canary 和受控 UI fixture，不能代签真实供应商、生产部署、整段 TCP 登录流程或自主游戏。实际 source/CI、独立安全评审及每项结果由实现 PR 记录，未覆盖的 BYOK-04/08 不预填 PASS。


模型失败诊断：执行状态中的可选 `model_failure` 只承载本地固定枚举；旧报告缺字段仍可读取，未知代码投影为 `other`。`output_limit` 表示生成触顶、整份回复未获准执行，不等同于不支持模型；模型身份不符、未支持工具、工具ID错误和意外协议类型分别诊断。不得从分类反推已遗失旧回复的具体原因，也不记录原始回复/推理。

停止后先核对原世界操作；若证实是容量或组合配置问题，由部署方在已确认的有限预算内修订配置并重新验证，不能以自动重试、切协议或默认提高预算恢复。

执行器重启后若尚未发生新的模型决策、原 operation 查询即失败，先检查 World Service 原结果及历史材料，不把它归因于模型能力或通过提高 token 上限解决。`HistoryUnavailable` 保留原身份和预算、阻塞新副作用；记录头存在不等于 snapshot／journal／proof 仍可用。宿主先恢复可验证材料，再按原任务／围栏继续核对，不能重置任务或用新世界代签原世界恢复。
