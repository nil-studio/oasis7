# 客户端启动器引导配置与可用性设计（当前 authority）

> 对应需求：[引导配置与可用性 PRD](game-client-launcher-guided-configuration-and-usability.prd.md)。实现、运行、评审及发行状态由对应 PR 和真实证据维护。

## 1. 问题、目标与范围

让玩家在现有 native/Web Launcher 上看清“正在编辑什么、服务接受了什么、当前运行什么、这次请求得到什么结果”，并按真实 blocker 进入已有修复动作。本文在原配置真值与恢复设计上，落实[专业当前合同](game-client-launcher-guided-configuration-and-usability.prd.md#当前合同)委托的信息层级、具体文案、响应式、键盘操作和可编辑交付稿；不另建设置、账户或运行时产品。

Launcher 是 `viewer` / `pure_api` 路径的交付与控制表面。当前源码的“打开游戏页”进入 Web/UI；本文没有新增 pure_api 启动按钮或模式选择器。展示的目标模式须由该入口的已确认契约提供；未知时显示“入口模式待核实”，不能从 provider、链角色、URL 猜出正式可玩性。`observer` 只有在该入口 authority 明确报告受限路径时才适用；链节点的同名角色不等于玩家仅观察模式。

新增正文和两张 SVG 均为**设计目标 / DESIGN TARGET**。图中端口、请求和状态是布局样例；配置真值、结果关联、焦点与尺寸仍需实现和实际验收。本文不改变当前 primary-mode、权限、启动条件、自动启动策略或世界结果。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [配置真值与恢复](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth) | Restore、dirty 与最近成功画像按实际接受/运行配置归属 | [草稿与确认设计](#launcher-config-draft-design) | Launcher 控制面、native 文件与 Web storage、QA | 不建立新画像产品或改运行中进程 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 配置/接受/运行的真实阶段与已有下一步在 Launcher 上优先可读 | [层级与主操作](#launcher-uiux-hierarchy) | 视觉 owner、Launcher 实现与 QA | 本行是配置与结果的表现子义务，不代整项本地会话或世界结果 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 失败原因与真实恢复/返回动作在适用 native/Web、双语、窄屏及键盘下可读可达 | [响应式与输入](#launcher-uiux-responsive) | 视觉 owner、平台输入与可访问性适配 | 本行是已有失败/恢复的交互子义务；静态稿不证明实际平台或辅助技术通过 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 以可编辑目标稿和具体方法交接配置阶段/本次请求结果的表现 | [交付物](#launcher-uiux-deliverables) | game_visual_interaction_designer；viewer_engineer；qa_engineer | 本行是配置结果界面的制作交接子义务；稿件不替代整项实现或运行证据 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 原承接：Restore 只改草稿，poll 不抹 dirty，成功画像绑定接受配置与实例 | [草稿与确认设计](#launcher-config-draft-design) | Launcher 控制面/本地存储/QA | 仅配置结果子范围，不证明 provider 鉴权或世界结果 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 草稿、接受、运行与画像在界面上分别可辨 | [配置四层](#launcher-uiux-config-states) | 控制面配置与当前运行实例 | 不新增 profile 产品或跨会话保存承诺 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 新请求失败与旧实例运行同时表达，本地成功不代签世界结果 | [请求与旧结果](#launcher-uiux-request-result) | 控制面/当前运行实例；世界结果仍归 Runtime/Viewer | 只接 Launcher 表达，不证明完整玩家旅程 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | 原承接：恢复画像是输入，原接受/运行绑定需重新核实 | [草稿与确认设计](#launcher-config-draft-design) | Launcher 控制面/本地存储/QA | 不复用旧权限或世界恢复结论 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | 不同失败后重新验证并提供真实下一步 | [失败与恢复](#launcher-uiux-recovery) | session 与已有控制动作 owner | 不改旧数据、授权或原行动结果 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | 失效撤销当前快照，旧结果不替代当前请求 | [请求与旧结果](#launcher-uiux-request-result) | 现有控制请求代次与 session 重新验证 | 不定义新自动重试 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | 所交付入口的模式与阶段明确，进程/页面可见不提升能力 | [状态与双语文案](#launcher-uiux-state-copy) | 入口/Runtime/Viewer authority 提供实际模式和能力 | 不接收 pure_api 内部玩法或其他入口证据 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 配置、provider、后端、会话及模式失败分别说明原因 | [状态与双语文案](#launcher-uiux-state-copy) | 原引导/控制面/session authority | 无真实能力时不制造登录或 observer fallback |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 各失败提供已有恢复/返回，修复后验证当前事实 | [失败与恢复](#launcher-uiux-recovery) | 已支持目标入口的恢复动作 | 不增加自动修复/重试或世界权限 |

<a id="launcher-uiux-source-baseline"></a>
## 3. 当前状态、目标状态与差距

固定源码基线为 `4f6f20ece923c5705e26ef879c2b83e0e637b3f9`。以下是静态源码审读，不是行为通过记录。

| 当前接点 | 已核实的源行为 | 本交接处理 |
| --- | --- | --- |
| [main_app_shell.rs](../../../crates/oasis7_client_launcher/src/main_app_shell.rs)；`render_normal_dashboard`、`render_launcher_command_deck` | 状态栏、三张任务卡、预检/健康、快捷命令、More 与日志；正常布局按可用宽度 840/700 分流，底部按 820 分流；专家模式保留下一步而隐藏任务卡 | 保留这些功能与入口，重排主次；本节后的尺寸为目标样本，不声称等于当前断点 |
| [config_ui.rs](../../../crates/oasis7_client_launcher/src/config_ui.rs) | 字段编辑置 dirty；缺项打开 Game/Chain 配置引导；修复完成只关闭窗口再明确启动；≤560 的引导字段纵排，provider token 为 password 输入 | 字段、错误与真实 CTA 同组；检查通过只表明本次检查，不表示已接受/运行 |
| [self_guided.rs](../../../crates/oasis7_client_launcher/src/self_guided.rs)；[error cards](../../../crates/oasis7_client_launcher/src/self_guided_error_cards.rs) | 当前最近成功画像仍按 `game_running` 保存 `self.config`，Restore 未显式置 dirty；Chain 任务卡合并 Ready/Starting；“一键修复”实际只打开配置引导 | 原配置修复合同仍须实现；目标稿分层并准确命名，不把这些缺口写成已修复 |
| [app_process.rs](../../../crates/oasis7_client_launcher/src/app_process.rs)、[app_process_web.rs](../../../crates/oasis7_client_launcher/src/app_process_web.rs) | native/Web 都有控制请求与状态 poll 代次；旧代次/控制在途的 poll 不应用；失败撤销 session 证据；dirty 不等于 snapshot 时保留编辑 | 表达请求结果与当前实例两条事实，保留迟到/失败/未知；不改代次或 HTTP 合同 |
| [preflight](../../../crates/oasis7_client_launcher/src/self_guided_preflight.rs)、[blocked actions](../../../crates/oasis7_client_launcher/src/self_guided_blocked_actions.rs)、[reminder](../../../crates/oasis7_client_launcher/src/self_guided_onboarding_reminder.rs) | 已有重试状态探测、打开配置、显式启用链、继续/重置引导；引导可暂时关闭，普通模式继续提示 | 只编排这些已有动作；重置引导不重置配置、进程或世界 |
| [main_ui_helpers.rs](../../../crates/oasis7_client_launcher/src/main_ui_helpers.rs) | 窗口尺寸存在 360×300 下限；已读配置/主 shell 未定义本次所需完整焦点返回与 Escape 顺序 | 窄屏/低高度与键盘是待实现目标，不能由 egui Window 或静态稿存在推定已通过 |

## 4. 控制与信息边界

<a id="launcher-config-draft-design"></a>
### 草稿、确认配置与成功画像

本节实现[配对 PRD 的配置真值要求](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth)。当前源中的局部状态不足以证明“最近成功”；以下为目标合同。

1. 保留独立编辑态与控制面确认态。输入或 Restore 改变编辑态；normalize 后按同一相等性规则判断未确认差异。普通 poll 只更新确认态，不能抹掉 dirty 草稿。
2. last-successful 绑定已接受配置与对应成功运行实例，不从渲染帧的 `self.config` 或全局 Running 取值。确认不足时保留之前画像。
3. 控制面接受本次提交配置后再合并编辑状态；提交后又编辑出的新草稿不被旧回应清除。native/Web 保持相同语义，各自报告文件/storage 失败。
4. Autofill 的候选来自真实 game binary 位置或显式选择，并复用启动预检；launcher GUI 自身不是 game fallback。

复用 C1 Restore→旧快照、C2 Running A→dirty B→接受 C 和 L5 缺 game 而 GUI 存在的原场景，经真实 handler、poll 与 native/Web storage 验证，具体步骤见[验收方法](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-02)。

<a id="launcher-uiux-hierarchy"></a>
## 5. 信息层级与主要旅程

普通模式先显示当前入口和最影响本次目标的状态，再显示一个主 CTA；既有三张任务卡保留顺序，但只有当前可推进步骤使用强调色。配置、运行信息与日志逐层展开，不能由高密度健康指标压住原因和下一步。专家模式仍保留同一状态、主要原因、当前请求与配置入口，收起的是引导说明。

| 区域，按阅读顺序 | 实际内容 | 展开、收起与返回 |
| --- | --- | --- |
| 顶部身份 | Launcher、已确认目标入口、语言、普通/专家；Game、Chain、Provider 分别标来源和状态 | 长标签换行；不合成“全系统已就绪”总绿灯；语言与配置入口保持可达 |
| 当前下一步 | 当前目标、首要阻塞/未确认原因、一个已有主操作 | 重叠故障先写妨碍本次目标的原因，其余可展开；依赖仅影响对应动作，不把链关闭解释为所有游戏操作都不可用 |
| 任务与当前请求 | 启动链、启动游戏、打开页面各自状态；最近一次本目标请求的真实结果 | 当前请求紧邻对应动作；日志中的旧成功不抢占；启动中显示等待/探测，不再给第二个 Start |
| 配置与运行对照 | 编辑草稿、最近被接受值、当前实例所用值、最近成功画像的可知部分 | 默认只露差异摘要与少量相关字段；未知明确标记，不生成新 profile 名称或版本 |
| 工具与诊断 | 配置、引导、设置和 More；只读状态、日志及现有受控动作 | More 沿原可用性提供 Stop/Restore 等；返回后恢复原焦点；反馈/转账/Explorer 的内部语义沿原专题 |

首次引导保留现有三步：认识主流程→修复配置→启动并打开页面。跳过只收起引导，继续入口仍可达；完成引导不能解除检查。已有演示动作若继续呈现，说明它会按当前实现改配置并发起启动；不能把它包装成“查看示例”或无效果修复。本文不改变其自动编排策略。

<a id="launcher-uiux-config-states"></a>
### 5.1 配置四层与编辑流程

| 层 | 默认标签与内容 | 允许的解释 |
| --- | --- | --- |
| 编辑草稿 | “本地草稿 · 尚未应用 / Draft · Not applied”，显示已编辑字段及当前检查问题 | 只是本地输入；关闭编辑窗口保留现有会话内草稿，不承诺跨会话持久化 |
| 控制面接受 | “服务已接受的配置 / Accepted configuration”，绑定当次真实回应 | 接受不等于进程已启动；若无法关联则标“尚未确认” |
| 当前运行 | “当前运行使用 / Running with”，关联当前实例的可核实配置 | A 运行时编辑 B，A 与 B 并排；B 被拒绝不把 A 改名或宣称 A 已停止 |
| 最近成功画像 | “最近成功配置 / Last successful configuration”，仅原绑定证实且可读取时显示 | Restore 只装入草稿；存储失败保留失败提示，不将未保存值当成可恢复画像 |

配置引导固定顺序为：目标（游戏/链）→问题摘要→对应字段与就近错误→当前检查结果→关闭/高级配置。先聚焦第一个可修复字段；未知字段给“打开高级配置”。错误不会因新 poll、语言切换或打开其他窗口消失。字段修正后更新本地检查，主状态为“配置检查通过；请关闭后明确启动”，不自动提交。

已运行 A、编辑 B 时，主要提示写“当前服务仍使用原配置；草稿尚未应用”。Start 的可用性沿当前控制合同：运行中不提供“热应用”或“重启并应用”新动作；需要停止时使用原 Stop，再由新的 Start 请求重验。Restore、Autofill 和恢复建议分别说明其会修改哪些输入，不静默保存、启动或覆盖其他正在编辑的字段。会自动启动的既有恢复动作在 CTA 旁明确写出启动后果。

secret 字段保持遮挡；配置对照只写“凭据已修改”等安全摘要，不展示 token、key、完整认证 URL 或把 secret 带入错误、截图、日志和导出。native-only 字段不进入 Web required 校验；平台不提供的操作不出现可执行按钮。

<a id="launcher-uiux-state-copy"></a>
### 5.2 状态、文案与真实 CTA

以下是与源状态对应的目标文案，不新增 wire enum。英中文案一一保持阶段、对象和结果含义；纯技术明细放在可达详情中。

| 已核实条件 | 中文 / English 主文案 | 真实动作与边界 |
| --- | --- | --- |
| 本次目标配置不完整 | “需修复游戏配置 / Game configuration needs attention” | “修复游戏配置 / Fix game config”仅打开引导；链目标用相同句式，不写“一键修复成功” |
| 当前草稿检查通过、未开始 | “配置检查通过，尚未启动 / Configuration checked; not started” | “启动游戏 / Start game”沿原准入发送当前草稿；检查不预留资格 |
| 本次启动在途或实例仍 Starting | “正在启动，等待本次状态确认 / Starting; awaiting current status” | 控制请求尚在途时防止同域重复控制；请求已返回但实例仍 Starting 时按原可用性保留 Stop/状态探测。配置与诊断可继续，不能从旧 Ready 补成功 |
| 当前实例确实 Running | “游戏服务运行中 / Game service running” | “打开游戏页 / Open game page”；页面访问和世界行动结果仍由目标入口确认 |
| Chain 当前探测为 Ready | “链状态探测成功 / Chain status check passed” | 仅开放原 authority 支持的相关工具；不等于网络最终性、公开加入或转账结算 |
| 功能由当前配置关闭 | “链功能已关闭 / Chain feature disabled” | 可进入配置；已有“启用链”只改配置，后续启动独立。不得把 disabled 显示为网络故障 |
| 当前状态请求失败/回应丢失 | “本次状态未确认 / Current status unconfirmed”与具体可披露原因 | 当前可用的“重试状态探测 / Retry status check”只查询；旧状态至多标“先前记录”，不能保持当前 Ready |
| Provider 前置失败 | “Provider 未就绪，无法启动此入口 / Provider unavailable; this entry is blocked” | 打开对应配置/既有检查；不自动切 Script 或声称 observer 成功 |
| authority 明确返回仅观察 | “仅观察，当前入口不支持玩家行动 / Observation only; player actions unavailable” | 仅显示该路径真实提供的读取/返回；没有已支持入口时只给说明，不创造 observer 启动按钮 |
| 当前身份或会话不再有效 | “此会话需重新验证 / This session needs verification” | 只链接该目标入口已有的认证/返回动作；Launcher 不恢复旧权限 |

打开浏览器失败显示“游戏页面未打开 / Game page did not open”，保留目标地址的安全可读部分和已有再次打开入口；不能把 `open_browser` 已调用或日志已写视为进入世界。Stop 失败显示“未确认已停止 / Stop not confirmed”，随后按原状态探测核对；不得假定服务已结束。

<a id="launcher-uiux-request-result"></a>
### 5.3 当前请求、旧结果与重连

每个启动/停止目标同时保留“当前请求结果”和“当前可确认运行状态”。例如新 Start 被拒绝但旧 A 仍运行，主反馈说明新请求未被接受，运行区仍显示 A；两条事实都成立。返回 `ok=false` 后携带的状态快照不能覆盖这次失败提示。请求发送后编辑 C，B 的成功回应不能清除 C 的 dirty。

传输失败不能推出服务器未处理请求。保留当前请求关联与未确认原因，先沿已有状态探测确定实例/配置；没有可核实关联时继续未确认，不能自动重发启动、换身份或借旧成功清除错误。迟到 poll 与旧控制响应沿现有请求代次丢弃，界面更新不抢焦点。停止、退出控制面、重连或配置目标变化后，当前 identity/探测证据依 [session 合同](game-client-launcher-runtime-session-continuity.design.md#session-revalidation)失效，新的成功探测才重新呈现为当前。

跨 surface 的 LLM Reload、转账当前 action 和反馈采用各自[受控动作](game-client-launcher-cross-surface-action-parity.design.md#action-result-continuity)与[反馈结果](game-client-launcher-feedback.design.md#feedback-result-boundary)合同。Launcher 的紧凑摘要引用真实结果阶段，不把 submission ID、保存成功、process-static 历史或旧 receipt 升级为结算/完整历史；本交接不重画这些窗口的完整业务。

<a id="launcher-uiux-recovery"></a>
### 5.4 正常、失败与恢复旅程

| 旅程 | 顺序与返回 | 判定时保留的事实 |
| --- | --- | --- |
| 正常首次进入 | 读当前入口→看预检→修复字段→关闭引导→明确 Start→观察当前请求→打开页面 | 配置通过、请求接受、实例运行、页面到达分步出现；未完成一步不跳成世界成功 |
| 运行中改配置 | A 运行→编辑 B→保留差异→原条件允许时显式 Stop→重新 Start | 不支持热更新就不展示；B 拒绝时 A 与 B 原身份仍可辨，之后重验新请求 |
| Restore/Autofill | 选择现有动作→读改动说明→检查草稿→明确后续动作 | Restore 不自动启动；无画像/读取失败有可读结果；未修改的运行实例不变 |
| 配置/二进制失败 | 查看问题→现有引导/高级配置→修正→重新检查→显式重试启动 | 若只存在 GUI，继续缺 game blocker；无须自动下载或虚构可执行文件 |
| 网络/控制面失败 | 保留本次未确认→可用时探测→核对当前实例→给真实下一步 | 探测失败不抹草稿、不显示旧 Ready、不自动再提交；无法核实时允许留在配置/诊断 |
| stale execution world | 读当前尝试的具体错误与已有建议→明确选择受支持的 fresh-node 恢复动作 | 若动作会立即重试，在按钮旁说明；沿原合同保留旧数据，重新验证，不声称修复旧世界或恢复原权限 |
| 语言/专家模式/窗口重排 | 切换表现→继续原编辑或读取→回原控制 | 草稿、目标、请求身份与结果不变，聚焦项被移走时回到同功能的可见入口 |

<a id="launcher-uiux-responsive"></a>
## 6. 响应式、键盘与可访问文本

使用当前浅色背景、白色卡片和蓝色动作色；错误/警告/成功必须同时带文字。主文案 16 px、正文 14–16 px、辅助文字至少 12 px，按 UI 逻辑像素交接；正文行高至少 1.4。主按钮点击区目标 44×44，其他必要控件提供同等可操作区域；完整原因可换行，不以缩小字体或 hover 作为唯一读取方式。

| 目标样本 | 排列与滚动 | 必须可达 |
| --- | --- | --- |
| 1280×800 / 1440×900 | 顶栏→全宽下一步→任务/配置主列与运行/请求副列→工具/折叠诊断 | 当前原因、主 CTA、配置差异和请求结果；正文不依赖三栏均固定宽度 |
| 1024×768 | 优先保留主列；次级事实按内容并列或下移；标签换行 | 主 CTA 与当前结果不被状态 chips 挤到页面末端 |
| 390×844 / 360×800 | 单列；标题与状态换行；主要动作整行；工具折叠到现有 More | 首屏读到目标、主要原因、当前下一步；关闭/高级配置不在不可滚达区域 |
| 844×390 / 640×360 低高度 | 顶栏压缩次级信息；内容纵向滚动；编辑窗按实际可用高度限制 | 字段、错误、关闭与提交前动作可通过顺序滚动到达；不强保 300 px 内容下限 |
| 200% 文本/长 CJK/长英文 | 重排文字与按钮，控件可增高；长路径可断行并有完整安全读面 | 不截断“不/未”、目标名、错误原因或动作后果；不是整张图缩放 |
| 软键盘打开 | 以实际可见高度放置当前字段，相关错误与关闭路径可滚达 | 不让固定底栏盖住字段；收起键盘回同一编辑位置，不触发提交 |

配置引导和高级配置沿现有窗口模型交接，不把普通 `egui::Window` 称作已实现模态。打开时聚焦标题或首个需修复字段；Tab/Shift+Tab 按标题/关闭→字段→错误与说明→现有动作顺序走。非模态窗口允许回到其他可见控件，不能形成无出口焦点陷阱；若未来另有真正模态，须另验证背景隔离，本文不创建它。

Escape 只关闭最上层可关闭编辑/帮助窗口并返回调用入口；入口消失则返回该目标的配置或任务标题，不退出 Launcher、不停止服务。文本输入和 IME 合成期间 Enter/Escape 不触发 Start、Stop 或关闭；按钮获得焦点后才沿平台标准键盘激活。轮询、结果更新、语言切换不得抢走编辑焦点。关闭编辑窗口不等于丢弃草稿或撤销已发请求。

native 和 Web 各用现有框架可支持的可访问名称/角色/状态；图标有文字名，错误与对应字段关联，disabled 原因紧邻且可读。文字对比度目标 4.5:1，焦点/重要边界目标 3:1；每项按实际颜色、字体与缩放核对，不由静态稿宣布符合标准。只对状态变更作克制提示，不逐 poll 重播，减少动态效果时仍保留文字和当前进度。

<a id="launcher-uiux-deliverables"></a>
## 7. 制作与交付

| 可编辑源 | 具体内容与消费者 | 检查边界 |
| --- | --- | --- |
| [桌面稿](assets/ui-ux/guided-entry-desktop.svg) | 1280×800 目标窗口；旧配置运行、新草稿、最近拒绝及唯一下一步；给 Launcher 实现与视觉评审 | 一张目标组合，不是当前截图；源状态变化按 §5 重排 |
| [窄屏稿](assets/ui-ux/guided-entry-narrow.svg) | 三个 390×844 目标帧：缺项首屏、英文配置引导、请求未确认/安全探测 | 展示正常修复入口和故障恢复；低高度、200%/软键盘依 §6 与手册实际核对 |
| [验收方法](../../testing/manual/launcher-guided-uiux-acceptance.manual.md) | 源/稿核对、正常、拒绝、乱序、恢复、双语/尺寸/键盘的步骤与独立预期 | 静态、受控 handler、真实 native/Web、真人理解分别记录，不互相代签 |

SVG 使用可编辑文字和矢量形状；颜色来自本专题目标与原 Launcher 风格，不嵌入外部图片、字体文件、运行数据或凭据。图内固定 `DESIGN TARGET` 标识，保持原始 viewBox 和文字；PNG 只作检查派生物，不作为另一权威源。字体使用已有系统 CJK/sans-serif fallback，导出时记录实际字体；字体替换后重验字宽与换行。未来改字段、动作前置或文案时同时更新正文、稿中对应状态与方法。

## 8. 专业技术依赖

控制动作、输入与 HTTP 兼容沿[控制面设计](game-client-launcher-control-plane-and-machine-interface.design.md)；启动/停止、快照失效和代次沿[会话连续性](game-client-launcher-runtime-session-continuity.design.md)；平台字段、storage 与受控动作沿[跨表面设计](game-client-launcher-cross-surface-action-parity.design.md)。本设计只消费其可核实状态，不新增 endpoint、默认权限、后台轮询、自动重试、跨会话草稿保证或经济操作。

实际可见修改仍须按[主测试手册 S6](../../../testing-manual.md)及专项手册取得适用证据。当前文档与 SVG 的合入只交付目标，不改变已有启动、provider、world 或发布结论。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法、test/manual source 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [配置真值与恢复](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth) | [草稿与确认](#launcher-config-draft-design) | C1/C2/L5 真实 handler/poll/storage | [LG-02](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-02)；按该方法分别固定 native/Web 与输入 | 配置与存储前后值 | planned；不证明世界或 provider 可用 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [层级](#launcher-uiux-hierarchy) | 配置/结果的正常入口、当前原因与已有 CTA | [LG-01](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-01)；按该方法分别固定 native/Web 与输入 | 源/稿/实际操作对照 | planned；静态不证明实际 native/Web 或整项会话 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [响应式](#launcher-uiux-responsive) | 失败原因/恢复动作在双语、尺寸、键盘、IME、焦点下可读可达 | [LG-05](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-05)；按该方法分别固定 native/Web 与输入 | 实际尺寸与操作/焦点序列 | planned；辅助技术按实际环境分别验，不代整项入口恢复 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [交付物](#launcher-uiux-deliverables) | 配置结果表现的可编辑源、渲染及真实功能区分 | [LG-01](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-01)；按该方法分别固定 native/Web 与输入 | SVG/XML/render 与源定位 | planned；不是运行截图或完整状态穷尽 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [草稿与确认](#launcher-config-draft-design) | 原 C1/C2 画像只属于真实成功配置 | [LG-02](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-02)；按该方法分别固定 native/Web 与输入 | 真实 handler、poll、storage | planned；原配置子范围，不代签完整 session |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [配置四层](#launcher-uiux-config-states) | 配置四层及提交后新编辑可辨 | [LG-02](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-02)；按该方法分别固定 native/Web 与输入 | 原配置/当前实例与可见差异 | planned；不创建跨会话保存保证 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [当前请求](#launcher-uiux-request-result) | 旧 A 运行不冒充新 B 请求成功 | [LG-03](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-03)；按该方法分别固定 native/Web 与输入 | 原请求结果与当前状态并列 | planned；不证明世界运行或发行 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [草稿与确认](#launcher-config-draft-design) | 恢复画像只改草稿且重新核实 | [LG-02](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-02)；按该方法分别固定 native/Web 与输入 | 恢复前后配置/存储/实例 | planned；不复用旧授权或世界结果 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [失败与恢复](#launcher-uiux-recovery) | Stop/后端失败、修复后重新验证 | [LG-04](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-04)；按该方法分别固定 native/Web 与输入 | 当前来源、原因与恢复记录 | planned；不改变底层持久性合同 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002 / AC](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [当前请求](#launcher-uiux-request-result) | 旧响应丢弃、未知不隐式重发 | [LG-03](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-03)；按该方法分别固定 native/Web 与输入 | 时序/代次/发送次数 | planned；不以测试 seam 代签真实控制面 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [状态与文案](#launcher-uiux-state-copy) | 模式/阶段来源明确，observer 不由链角色推导 | [LG-04](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-04)；按该方法分别固定 native/Web 与输入 | 模式来源、可达动作和 blocked 原因 | planned；不提供 pure_api 新按钮或玩法 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [状态与文案](#launcher-uiux-state-copy) | 不同失败保持原含义 | [LG-04](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-04)；按该方法分别固定 native/Web 与输入 | 失败输入/文案/实际动作 | planned；不由另一平台或历史 green 代签 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [失败与恢复](#launcher-uiux-recovery) | 恢复/返回可由独立操作者理解 | [LG-06](../../testing/manual/launcher-guided-uiux-acceptance.manual.md#launcher-uiux-06)；按该方法分别固定 native/Web 与输入 | 任务、实际操作与理解回答 | planned；无操作者时保留未执行 |

## 12. 决策、风险与剩余工作

采用当前 shell 和动作，补齐实际层级及文案；配置保持四层、请求与实例保持两条事实。这样可以解释“旧服务还在，但新配置没有生效”，而不创建新的热更新、确认协议或 profile 产品。

主要实施风险是旧 Running 被用于保存新草稿、返回快照抹去本次拒绝、过期 poll 恢复旧 Ready、窗口最小尺寸和键盘焦点不可达。对应方法必须经过真实 handler 与表面，不只构造最终状态。缺配置/实例关联时先显示未确认，不能用展示计算代替技术 owner 修复。

本交付补齐 guided 配置与状态的 UIUX 目标；LLM 设置、转账、反馈、Explorer、Peers 的完整内部界面及 pure_api 正式玩家流程仍沿各自专业设计。实际接线、所有适用平台/辅助技术、真实运行和非作者理解证据尚未取得，状态保持与对应产品卡一致。
