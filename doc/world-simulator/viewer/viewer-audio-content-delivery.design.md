# Viewer 音频内容与交付设计

- 主责：`game_visual_interaction_designer` 维护声音目的、提示配方和等价表达；`viewer_engineer` 维护播放、设置与事件适配；`qa_engineer` 按同候选验证。所属模块为[智能体、世界模拟与交互](../../product/agents-world-simulation/prd.md)，专业入口为 [Viewer](README.md)。
- 生命周期：`active`。本文定义待采用的短功能提示及其完整消费合同，不表示音频素材、播放器、控件或目标平台已经交付。
- 审读基线：`f4f8e1bb1be6cb9746ea767af3bdf71204e347e3` 的产品、Viewer 源码与构建输入；源审计范围见[当前基线](#audio-current-baseline)。新产品输入为[可选声音要求](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006)。
- 交付：三种程序化短提示的编辑配方、事件和设置文案、来源/版本交接，以及[制作与验收手册](viewer-audio-content-delivery.manual.md)。没有声音文件或运行结果的条目不标为素材可用。

## 1. 问题、目标与范围

长时间观察世界与间接指导 Agent 时，持续声景容易掩盖真正需要玩家阅读的反馈。当前声音方案只提供玩家可自行开启的短提醒，让玩家回到已有文字中的原操作、结果与下一步。听到提示不成为世界成功、结算、控制权或新可见信息的证明。

本设计承接 GD-10，并覆盖有关 GD-11、GD-12、GD-18 的文字替代、声音设置、制作与来源。声音从当前已授权的可读反馈产生；它不监听原始世界事件、不新增世界动作或请求状态，不为每个 tick、地图标记、生产动画和 Agent 行为配声。声音缺失、被抑制或播放器失败时，玩家仍有完整的决策路径。

产品范围为 Web/native Viewer。原生目标沿[NativeWorldViewHost](external-agent-world-view.design.md#view-native)加载同一官方 Viewer build，并使用宿主 WebView 的音频能力；macOS、Windows 和其他组合分别验证。`pure_api` 没有播放设备职责，仍保留原结果语义；Launcher 的启动/配置界面、官网和工程控制台不继承 Viewer 音频设置。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006)；Trace refs: O7-REQ-f914fafa-e682-4216-aef2-23354792cf34 | 受支持 Viewer 采用有明确目的的有限声音，不把其他入口或声音类型当作已支持 | [表面与内容取舍](#audio-surface-scope) | 产品 owner；原 Web/native 入口 | 不扩大已验证平台，不承诺音乐、环境声或配音 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006)；Trace refs: O7-REQ-f914fafa-e682-4216-aef2-23354792cf34 | 默认静音，玩家明确开启；只提示符合当前来源和披露条件的事件 | [事件合同](#audio-event-contract) | 当前合法反馈消费者；原请求关联 | 不把 ACK、未知或历史重新读取变成新的结果提示 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006)；Trace refs: O7-REQ-f914fafa-e682-4216-aef2-23354792cf34 | 开关、音量、失活、退出与播放失败有一致的设置和恢复语义 | [设置与生命周期](#audio-settings-lifecycle) | Web/native 播放适配；原 Viewer 设置语境 | 本文不表示现有布局已经提供控件 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006)；Trace refs: O7-REQ-f914fafa-e682-4216-aef2-23354792cf34 | 提示竞争和洪泛不形成持续噪声或延迟补播 | [优先级与混音](#audio-mix-budget) | 本文有界调度；原反馈文本 | 丢弃声音不删除反馈，不延迟世界操作 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 静音、缺设备和不支持音频时，四个决策锚点与两种语言仍可读 | [静音等价表达](#audio-silent-equivalence) | 原布局、双语内容及可访问操作 | 不代签所有屏幕阅读器和设备组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 受理、未知、能力不支持和真实结果保持原含义 | [事件合同](#audio-event-contract) | 原控制面和当前模式 | 音频不从颜色、词语或 HTTP 状态猜结果 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 个人提示绑定当前身份和原操作，旧异步结果不跨身份/世界响起 | [消费身份与接口](#audio-consumer-contract) | 原身份、scope 与反馈 authority | 不创建第二套鉴权或读取资格 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 拒绝有真实原因和下一步；播放失败不改写原请求或触发重试 | [静音等价表达](#audio-silent-equivalence) | typed 原因和原结果查询 | 声音不可用不成为重新提交世界动作的理由 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 三种提示有明确编辑源、制作规格、版本和权利资料 | [声音内容与配方](#audio-content-recipes) | 声音制作方、仓库来源/许可 | 本行产品层 N/A；理由：制作资料不新增玩家功能；范围：本文三种提示。玩家义务仍按以上各行。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 内容与实际消费者能绑定同一候选并支持替换与回退 | [交付与兼容](#audio-delivery-handoff) | 原 Viewer 构建、native build 与实际验证记录 | 本行产品层 N/A；理由：交付追溯不新增玩家功能；范围：声音配方、设置文案和消费适配。 |

### 2.2 原 authority 与一手实践

[个人反馈隔离](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)、[原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)和[客户端等待相关性](viewer-control-plane-split-live-playback.design.md#viewer-client-request-lifecycle-design)继续拥有身份、权限、原请求、迟到结果和世界处置；本设计只消费其合法投影。原[页面模块覆盖](viewer-page-module-design-2026-06-18.design.md#uiux-module-coverage)未设计声音控件；这里新增的是独立声音目标和接入约束，不能据此把旧布局或原型说成已经采用。

W3C 的 [Audio Control 说明](https://www.w3.org/WAI/WCAG22/Understanding/audio-control.html)指出自动声音可能干扰阅读与屏幕阅读器，建议由用户明确启动并可独立控制；[Sensory Characteristics 说明](https://www.w3.org/WAI/WCAG22/Understanding/sensory-characteristics.html)要求操作说明不只依赖声音等感官特征。本文因此选择默认静音和完整可读替代，不由这两条资料宣称全产品符合 WCAG。

[Web Audio Recommendation](https://www.w3.org/TR/2021/REC-webaudio-20210617/)提供声音节点及 suspend/resume/close 的语义；[Chrome 官方自动播放说明](https://developer.chrome.com/blog/autoplay#web_audio)说明 Web Audio 可能受用户手势与浏览器策略限制。本文不假设 API 存在就能发声，也不通过修改浏览器策略获得验收通过。其他浏览器与 native WebView 以实际候选核验。

<a id="audio-current-baseline"></a>
## 3. 当前状态、目标与差距

固定基线的静态检索覆盖：`crates/oasis7_viewer/software_safe_src`、`crates/pixel_world_bridge/src`、其 Cargo 依赖、`crates/oasis7_viewer/package.json`、`crates/oasis7_client_launcher/src` 和 `site/assets/app.js`；另核这些入口目录中的常见音频扩展名。该范围未定位到本专题的播放器、声音设置、事件映射或音频文件。[pixel_world_bridge/Cargo.toml](../../../crates/pixel_world_bridge/Cargo.toml)使用 `default-features = false`，所列 Bevy features 不含音频；[Viewer package.json](../../../crates/oasis7_viewer/package.json)未声明独立声音包。这不是全仓、第三方依赖、运行进程或已部署站点的无音频证明。

| 对象 | 本基线可核实事实 | 本文目标与剩余交付 |
| --- | --- | --- |
| 反馈与身份 | 已有 [viewer_feedback_scope.js](../../../crates/oasis7_viewer/software_safe_src/viewer_feedback_scope.js)、[pixel_world_scope.js](../../../crates/oasis7_viewer/software_safe_src/pixel_world_scope.js)和[原结果投影](../../../crates/oasis7_viewer/software_safe_src/viewer_gameplay_receipt_feedback.js)；它们并非音频接入 | 由同一受信消费者提供声音候选，不复制鉴权/状态判断；缺适配时保持无声 |
| 声音内容 | 没有在所审入口定位到本专题声音文件 | 下文三份配方是新内容设计；生成器实现、实际声波/试听样本和听感未交付 |
| 播放与设置 | 旧页面 authority 明确未覆盖声音设置 | 实现本文开关、音量、试听、生命周期与失败表达；不复用语言存储键或凭证存储 |
| 原生 Viewer | [原生视图设计](external-agent-world-view.design.md#view-native)规定同 build WebView 及逐端验证 | 在实际 native 世界窗口上核音频适配、焦点/最小化/关闭和设备失败；不能以 Launcher 配置窗口代替 |
| 证据 | 本文只读源码和设计，不运行音频、UI、模型或真人试验 | 设计审阅、素材检查、实际播放、静音等价与真人理解分别取证 |

<a id="audio-surface-scope"></a>
## 4. 表面、声音范围与结构

| 表面或内容 | 当前方案 | 理由及重新评估条件 |
| --- | --- | --- |
| Web Viewer 与 native 世界窗口 | 当前页面会话可选的短功能提示，默认关闭 | 辅助阅读当前反馈；native 与 Web 使用同一事件和内容定义，各自验收 |
| 离线回放、Seek、历史和恢复读取 | 始终不播放世界结果提示；设置内的显式试听仍可用 | 回放不能制造新的成果感或声响洪泛；若新增有声回放体验，应先另定播放时间线及可辨识模式 |
| 持续背景音乐 | 本方案不制作、不加载 | 工业指挥以长时间阅读和自定节奏为主；若产品采用音乐体验，先定义曲目/段落、循环/切换、独立音量和静音等价 |
| 环境循环、机械循环、空间声源 | 本方案不制作、不加载，也不按地图距离声像定位 | 地图投影和图形活动不证明真实声源、距离或隐蔽对象；若空间声成为玩法信息，先补声源授权、空间与遮挡合同 |
| 配音、对话朗读、语音聊天 | 本方案不制作；不调用麦克风、TTS 或外部语音服务 | 当前对话内容以可读文本承接；若采用语音，先补台本/字幕、语言与发音、同意/权利、独立设置及费用边界 |
| Launcher、官网、工程控制台 | 不进入本专题声音消费者 | 各入口目标不同；新增声音由其原 authority 决定，不共享 Viewer 开关 |
| `pure_api` | 保持结构化结果，不产生播放义务 | API 不持有玩家听觉设备；不能因此删减既有结果字段或恢复语义 |

声音链的顺序固定为“当前受信反馈消费者完成身份和原操作判断 → 更新原可读反馈 → 提交有限声音候选 → 本地播放门控 → 本地合成/输出”。音频失败不能反向撤销文字、清理原请求或要求世界重发。没有与该提示相同事实的完整可读内容时，不产生声音候选。

<a id="audio-event-contract"></a>
## 5. 事件与运行流程

### 5.1 事件白名单

E01～E03 是本地表现分类，不是 Runtime 事件、wire 枚举或新的要求 ID。当前适配只承接本人在当前 Viewer 明确发起且仍关联到当前反馈的 Gameplay 操作；Agent 自主行动、共享世界活动、Chat、Prompt 配置、模型检查和报价预览保持文字反馈，不自动继承本表。

| 事件 | 必须同时满足的触发条件 | 提示与优先级 | 始终保留的可读结果 | 不可触发的相似情形 |
| --- | --- | --- | --- | --- |
| E01 本地试听 | 玩家点“试听”，或明确执行带试听说明的首次开启；当前窗口可见且有焦点，播放适配已确认可用 | S00；最低优先级 | “本地试听，不会执行游戏操作。” | 页面加载、调整音量、语言切换、登录或窗口恢复 |
| E02 原操作的可信处置 | 同一当前身份/世界、原请求和当前等待实例；原 authority 的新实时结果已被当前消费者接纳并已展示。允许已确认的 committed/applied/completed 语义，但须保留原请求处置与世界效果的区别 | S01；中优先级，含义仅为“查看新反馈” | 原行动、处置语义、仍待同步/后续效果的限制和真实下一步；不得把 committed 一律写成已获得产物 | ACK、received/accepted/pending、仅动画完成、tick 增加、Chat accepted、预检通过、初始快照和查询旧结果 |
| E03 原操作的可信拒绝或阻塞 | 仍为当前反馈的原操作，原 authority 明确拒绝/阻塞/到期，当前用户有权读到其原因和下一步；拒绝来自操作结果而非查询失败 | S02；最高优先级，含义仅为“查看原因” | 原拒绝/阻塞含义和可行的修正、等待或安全停止 | 网络断开、超时、结果未知、查询权限失败、旧请求错误、他人/匿名连接的失败 |

原消费者明确识别的拒绝、阻塞或到期只归 E03，不同时归 E02。没有稳定归属、缺可信结果或无法区分原操作失败与查询失败时，保留原文字并不发声；不能用文本关键词、badge 颜色、HTTP 200、`ok` 布尔值或反馈 ID 变化推定 E02/E03。

### 5.2 禁止补播与原结果关系

首次打开、重新挂载、开声、重新连接、失活后恢复、身份/世界/绑定变化、Seek、重放、查询历史和语言切换都先建立安静基线。它们带来的既有反馈不进入声音白名单。恢复读取出 committed 仍应显示真实结果，但不为这条历史结果再次发声。

同一当前等待实例仅有一次 E02 或 E03 提示机会。拒绝被重新渲染、快照重复、迟到 ACK、组件重排和通知被限频丢弃都不重置机会；已被抑制的声音不加入等待队列。新世界效果或新的明确请求由原 authority 判断，声音层不能靠换本地编号把旧结果变成新事件。

### 5.3 正常、竞争与失败顺序

1. 玩家开启声音并完成本地试听，随后明确发起受支持行动；原请求消费者保存既有操作身份和等待实例。
2. 原消费者先判断到达结果是否仍属于当前身份、世界、操作和反馈范围；旧回调只沿原合同处理，不进入音频候选。
3. 已接纳结果先更新原文字，再生成 E02 或 E03。当前显示已转到后续操作或旧等待实例已销毁时，只保留原有合法历史，不播放旧提示。
4. 本地门控核开启、音量、可见/焦点、播放状态及代次；合格后按优先级和速率调度。失败、限频、已过期或播放中断均结束这次提示机会。
5. 音频异常只更新本地声音状态；原请求继续接受、执行、查询或恢复。任何声音重试仅为玩家明确的本地试听，不重试世界操作。

<a id="audio-consumer-contract"></a>
## 6. 消费身份、内容与接口

### 6.1 身份与调用边界

下面是待实现的进程内职责，不是新服务或必填协议。音频适配不读原始 socket、签名凭证、私有日志或世界状态，不查询“声音是否遗漏”的历史。

| 接口职责 | producer → consumer | 身份与顺序 | 结果与失败 |
| --- | --- | --- | --- |
| 可读反馈到声音候选 | 原 Viewer 受信反馈适配 → 本地门控 | 仅传 E02/E03、当前不透明 scope/生命周期代次、原等待实例和已经展示的事实分类；身份判断仍在原消费者完成 | 缺当前关联或没有完整文字则不提交；不拷贝消息正文、Key、Prompt 或其他玩家标识 |
| 本地设置与试听 | 当前窗口设置 → 门控/播放器 | 本文声音会话与当前窗口代次；一次显式点击对应一次尝试 | 可用、暂停、不可用分别反馈；同一时间至多一个初始化/恢复尝试 |
| 播放 | 门控 → 平台适配 | S00/S01/S02、内容版本、有效音量和当前代次；调度前再核条件 | 返回开始、被抑制或失败；异步返回与旧代次不匹配时只释放旧资源 |
| 停止与销毁 | 设置、焦点/可见性、身份/世界、离页 → 平台适配 | 先使旧代次失效并停止所有节点，再释放/暂停输出 | 即使底层暂停或关闭 Promise 失败，也不能保留可继续播放的节点或旧 callback |

身份沿[当前反馈 scope](../../../crates/oasis7_viewer/software_safe_src/viewer_feedback_scope.js)及[原始等待相关性](viewer-control-plane-split-live-playback.design.md#viewer-client-request-lifecycle-design)接入；scope 字符串只作本地相等性比较，不落日志或独立持久化。绑定/会话/连接代次不同、世界分支或有效读取 fence 改变时，先清当前提示资格；后到结果须重新经过原 authority，不能用旧的“已授权”布尔值通行。

保留一份当前可发声等待实例及已消费标记即可；后续操作替换当前反馈后，旧实例失去发声资格。没有无限去重表或扫描世界历史。重连可继续按原合同恢复原结果，声音资格不从恢复胶囊还原。

<a id="audio-content-recipes"></a>
### 6.2 三种声音配方

声音方向为短、干净、无词语的电子提示。S01 不使用胜利号角、升阶旋律、金币或机器启动拟音；S02 不使用恐吓、尖锐警报或持续催促。声音不能让玩家仅凭音调判断成功/失败细节。

本方案选择程序化单声道正弦脉冲，不采样第三方录音。下表是完整 v1 编辑配方，制作方可按数值实现生成器并导出试听文件；本文没有实际生成或试听结果。所有脉冲相位从零开始，幅度是数字满刻度的 0.10；每个脉冲首尾各 20 ms 线性淡入/淡出，其余段维持该幅度；间隔为绝对静音。频率和时长是本项目的初始设计取值，不是标准规定或听感已验证结论。

| 内容键 / 版本 | 频率与脉冲序列 | 总时长 | 消费与语义 | 源、产物与当前状态 |
| --- | --- | --- | --- | --- |
| S00 / v1 | 660 Hz × 160 ms | 160 ms | E01；仅本地试听 | 编辑源为本表；后续生成器和可试听 PCM 待交付 |
| S01 / v1 | 660 Hz × 100 ms → 静音 60 ms → 同频 100 ms | 260 ms | E02；查看当前新反馈，不表示世界成功 | 编辑源为本表；后续生成器和可试听 PCM 待交付 |
| S02 / v1 | 440 Hz × 180 ms → 静音 60 ms → 同频 180 ms | 420 ms | E03；查看原原因，不代替其文字 | 编辑源为本表；后续生成器和可试听 PCM 待交付 |

制作参考输出使用 48 kHz、单声道、24-bit PCM WAV，S00/S01/S02 分别为 7,680 / 12,480 / 20,160 帧；首尾保持零值，全段样本均值绝对值不超过 0.0001 满刻度，无截波或包络突变。24-bit 量化的单样本幅度允许 1 LSB 舍入差。运行选择 Web Audio 的 Oscillator/Gain 按同配方合成，采用设备上下文实际采样率，不要求设备切到 48 kHz。参考文件供审听和包络比较，不与运行实时合成字节等同。不同采样率的波形字节不同，但频率、时序、幅度和包络仍应符合配方。

S00～S02 只有一个内容版本，不按中文/英文复制声波；相应设置文案有双语正文。实际生成器由 Viewer owner 放入原构建输入，版本修改同时更新本表与监听者；未经审阅不以本地系统提示音或第三方网址代替。

<a id="audio-settings-lifecycle"></a>
## 7. 设置、状态与持久化

### 7.1 设置与双语正文

声音项放入原 Viewer 设置语境，不能新增主导航、抢占世界舞台或遮住原行动。采用语义开关、范围输入和按钮，键盘可到达，有可见焦点和文本状态；不只显示喇叭图标，不增加全局字母快捷键。现有布局没有这些控件，实际采用须核窄屏、低高度及 200% 文字后的可操作性。

| 字段或操作 | 中文 / English | 约定 |
| --- | --- | --- |
| 开关 | 提示音 / Sound cues | 默认关闭。开启说明：“只在当前 Viewer 会话播放短提示。先试听一次；不会执行游戏操作。” / “Play short cues in this Viewer session. Start with a local preview; no game action is sent.” |
| 音量 | 提示音音量 / Cue volume | 0～100 的整数百分比，默认值 25；独立于系统音量。仅调音量不试听、不改变世界或其他应用声音 |
| 试听 | 试听提示音 / Preview cue | 显式开启成功后可再试听 S00；遵守同一音量、优先级与速率限制 |
| 关闭 | 提示音已关闭 / Sound cues off | 停止正在播放及待启动节点；不改变原操作 |
| 零音量 | 提示音已开启，音量为 0（静音） / Sound cues on, volume 0 (muted) | 开关可保持开启意图，但实际输出为零，界面明确；提高音量后仅允许新的合格提示 |
| 可用 | 提示音可用 / Sound cues available | 仅表示适配初始化/恢复已确认，不宣称玩家实际听到或设备音量正常 |
| 暂停 | 提示音已暂停；可在当前窗口恢复 / Sound cues paused; resume in this window | 恢复是显式本地操作，不自动补播 |
| 恢复 | 恢复并试听 / Resume and preview | 本地恢复成功后只试听 S00；没有原反馈补播 |
| 不可用 | 提示音暂不可用，文字与操作仍可用 / Sound cues unavailable; text and actions remain available | 提供“重试试听 / Retry preview”或关闭；原因仅用不含私有资料的设备/适配分类 |
| 试听说明 | 本地试听，不会执行游戏操作 / Local preview; no game action is sent | 与试听控件同时可读，不使用原行动成功徽标 |

以上是新待采用的正文，双语审校与真实布局验证按手册进行。关闭声音、音量为零、失活和不可用状态不能共用一个“播放中”图标。

### 7.2 本地状态转移

声音设置只存当前 Viewer 文档/世界窗口的内存，刷新、重新打开、身份或世界切换恢复默认关闭及音量 25。普通连接恢复不另造身份；若原 scope 因连接代次失效则清提示资格并保持暂停，需玩家显式恢复。设置不写 localStorage、账号、Key 存储、同步服务或世界快照；没有跨入口/设备偏好迁移。

| 状态 | 进入条件 | 可执行操作与转移 | 声音规则 |
| --- | --- | --- | --- |
| 关闭 | 初始状态、显式关闭、身份/世界切换 | 开启 → 初始化；失败 → 不可用 | 无输出；世界流程不等待声音 |
| 初始化 | 前台窗口的显式开启/重试，至多一个尝试 | 当前代次确认可用 → 可用；拒绝/异常或 2 s 未确认 → 不可用；关闭/失活 → 取消本次 | 仅成功后可播一次 S00；晚到成功不得恢复被取消实例 |
| 可用 | 当前代次适配确认可运行 | 显式关闭声音、失活、断连、scope 变化或异常分别进入对应状态 | 只有白名单新候选可响，音量为零时直接消费机会并丢弃 |
| 暂停 | 窗口失活、隐藏/最小化、连接恢复或同身份 scope 失效 | 重新聚焦只恢复设置可操作性；显式“恢复并试听” → 初始化 | 立即停止旧节点；无队列、无自动补播 |
| 不可用 | API/设备拒绝、初始化超时、内容版本未知、输出错误 | 玩家明确重试 → 初始化；关闭 → 关闭 | 不自动重试、不回退到系统警报或外部播放器 |
| 已销毁 | 离开 Viewer、卸载世界窗口或宿主关闭 | 终态；新实例从关闭开始 | 旧代次永久失效，节点断开并释放上下文 |

2 s 是本地设置等待上限，既不是世界请求超时也不是浏览器 Promise 必须在此时结束的保证。超时使本次代次失效；迟到返回只能清理资源。初始化过程中不能让重复点击创建多个 AudioContext；重试须复用仍可恢复的同一上下文，或在旧上下文确认关闭后创建新的一个。关闭结果未知时保持不可用，不为绕过等待另建并存上下文。取消、显式关闭声音、失活和身份变化先将输出增益降为零并停止节点，再请求 suspend/close；底层已送往硬件的极短尾音不能收回，按实际设备记录停止延迟，不宣称绝对零延迟。

音量调为 0 时停止当前与待启动节点，并消费、丢弃期间的提示机会；声音开关可保持开启，也允许保留同一个上下文。提高音量不创建上下文、不试听、不补播，只重新评估此后的新提示；若上下文已因其他原因暂停或不可用，仍须按对应状态执行显式恢复或重试。

### 7.3 播放适配选择

Web 与 native WebView 使用一个当前文档私有的 Web Audio 上下文及单一主增益。首次 create/resume 只在本节明确操作的用户手势内触发；不在页面挂载、认证完成或连接回调内自动开声。API 缺失、被策略拒绝、状态未运行或设备中断时保持可读的暂停/不可用。

暂停停止所有已创建声源，不能仅调用 suspend 后保留计划中的脉冲，因为恢复时不得续响旧提示。关闭/销毁须 disconnect 并 close 旧上下文；迟到的恢复回调检查当前代次，不能把旧节点接到新输出。内容合成只使用本文正弦/增益，不引入 AudioWorklet、网络采样、麦克风权限或 native IPC 播放服务。该选择提供明确的目标实现边界，不证明任一 WebView 已支持或已验证。

<a id="audio-mix-budget"></a>
## 8. 优先级、混音与运行安全

### 8.1 有界调度

S02 优先于 S01，S01 优先于 S00。同一次状态更新中若存在多个候选，只取最高优先级的一条；同优先级取当前原操作，其他丢弃。最多同时播放一种提示，每个提示内部脉冲顺序执行，不叠加不同事件。

任意滚动 5 s 窗口最多启动 2 次提示，包括试听。额度耗尽即丢弃，不排队；更高优先级也不能绕过该上限。正在播放时，低/同优先级候选丢弃；有额度的高优先级可用至多 20 ms 淡出停止旧提示，再启动新提示。等待启动期间仍只有一条候选，保留原代次，最后一次条件重核失败即丢弃。候选形成后超过 200 ms 才能调度时丢弃，避免迟到音效制造错误的事件关联。以上限制只作用于声音，不控制文字更新或世界请求。

主增益为音量百分比除以 100，零值立即无声；音量调整和切换采用至多 20 ms 平滑变化，避免增益突变。任一时刻最大数字幅度不超过 0.10；淡出旧提示完成后再启动新提示，不能借交叉淡化超过单声限制。没有音乐、环境或语音总线，不虚设多通道 ducking。将来增加声音类型时必须重新审视并发、独立音量和替代，不静默挤入当前总线。

### 8.2 安全与资源边界

声音层不获取或写入游戏权限，不使用 credentials、麦克风、外部音频 URL 或第三方语音服务。日志只记有限的本地状态、内容键、代次和失败类别，不记原请求正文、scope 字符串、玩家名字、Key 或私聊。试听不得发送任何世界提交、结果查询或模型请求。

资源上限为一个上下文、一个主增益、一个当前提示及至多两个顺序脉冲的节点，另有一份当前等待实例资格和两个最近启动时刻。异常和停止均回收节点与引用；未开启时不创建音频上下文。播放/资源错误被局部捕获，不能让反馈渲染、输入或世界请求失败。

数字幅度和试听音量不等于真实声压或听力安全证明；设备音量、输出设备与个人体验依实际试听。不能以“短”“低峰值”宣称所有设备上的响度适宜。

## 9. 质量与容量

下表是项目初始验收目标；尚未运行。数值来自本文配方和有界调度选择，不表示行业强制标准或实测产能。

| 场景 | 环境 / 输入 | 预期响应与判据 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 配方一致 | 48 kHz 单声道参考导出；三种 v1 | 帧数、频率、包络、间隔和峰值符合 §6.2；静音处为零、无截波和 DC 偏置 | [AU-ASSET](viewer-audio-content-delivery.manual.md#audio-asset-check) | 配方已定义；素材未制作 |
| 声音洪泛 | 同窗口 1 s 内 100 个合法/重复候选及试听 | 任意 5 s 最多 2 次启动，最多一条在播，无积压补播；原文本全部按原合同保留 | [AU-STATE](viewer-audio-content-delivery.manual.md#audio-state-cases) | 静态模型可推演；实际适配未实现 |
| 迟到与代次 | 初始化未返回、关声/换身份/隐藏，再返回成功 | 无旧声音；当前 UI 和原请求保持；无新增上下文泄漏 | [AU-STATE](viewer-audio-content-delivery.manual.md#audio-state-cases) | 需受控时间/适配测试及实际入口 |
| 响应与停止 | 固定 OS、浏览器/WebView、输出设备；至少 30 次普通提示和 30 次停止 | 调度在 200 ms 内，否则丢弃；记录 audible 起点和尾音分布。相对静音同候选，声音不得造成连续 1 s 交互无响应 | [AU-RUNTIME](viewer-audio-content-delivery.manual.md#audio-runtime-cases) | 真实延迟、CPU 与听感均未验证 |
| 静音等价 | 中英文、desktop/窄屏/低高度、200% 文字、默认关闭/零音量/失败 | 原行动、结果、阻塞和下一步不缺失；开关、试听和状态可键盘读取，不依赖音调 | [AU-EQUIVALENCE](viewer-audio-content-delivery.manual.md#audio-equivalence-cases) | 需实际 UI；不由脚本模拟听感 |
| 长时操作 | 同设备连续 30 min，包含反复开启/关闭/失活和断连 | 实际活跃上下文/节点不持续增长；结束后无未释放本专题资源，无旧提示恢复 | [AU-RUNTIME](viewer-audio-content-delivery.manual.md#audio-runtime-cases) | 需实际播放器及宿主观察 |

<a id="audio-silent-equivalence"></a>
### 9.1 静音等价表达

E02/E03 的原反馈必须在声音之前可读，包含原对象/行动、真实阶段/处置、足够理解的原因及当前可行下一步。音效没有独占信息，故不另加“听到两声就……”的教程、必须听完才能操作的门槛或隐藏音效字幕。S00 的本地试听说明独立显示，不占用行动结果位置。

沿[可访问表达](viewer-localization-and-context-help.design.md#language-accessibility)保留文字/图形表达及原可访问语义；减少动态效果不自动开启声音。音频设置不使用 TTS 重读私聊或原反馈，不与屏幕阅读器争夺唯一播报通路。是否需要修改原反馈的 live region 由其消费者按实际辅助技术验证，不为每次声音另播报一份相同结果。

<a id="audio-delivery-handoff"></a>
## 10. 交付、兼容与回退

| 交付对象 | 可编辑来源与消费者 | 交接要求与当前边界 |
| --- | --- | --- |
| 三种 v1 配方 | 本文 §6.2；后续 Viewer 合成器消费 | 制作方交频率/包络/幅度/时长、实际生成器版本及三份参考 WAV；本次只交配方，没有已完成文件可列 |
| 声音设置和状态正文 | 本文 §7.1；Viewer 设置消费者 | 交中英文审校结果、真实组件绑定及 fallback；不得修改原行动结果措辞来迁就音效 |
| 事件与门控适配 | 本文 §5～8；原反馈、窗口生命周期和平台输出 | 对照固定 source 和已采用内容版本核输入关系、代码路径、事件负例；不得从原消息复制另一状态机 |
| 实际候选 | 原 Viewer 构建和 native 官方 build | 记录本次 source、内容版本、生成器与实际 bundle/安装包身份、OS/浏览器/WebView/输出设备。运行合成无独立声音包下载；参考 WAV 不冒充已运行的声波 |
| 来源与权利 | 仓库当前 [LICENSE](../../../LICENSE)和实际创作记录 | 本配方不含录音采样；实际生成器与导出物仍须记录作者、来源、适用许可和必要署名。根许可不能替第三方样本授权；替换外部素材时沿原资产权利责任先补来源与授权 |

实际采用保持[Viewer 构建真值](viewer-web-build-truth.prd.md#42-generated-artifact-single-source-of-truth)，不手改 dist，不新增外部播放器 fallback。先把配方和播放器、设置、原反馈消费者一起接入同一候选，再按手册验证；只完成声音文件或只有开关，不表示整个目标可用。

既有纯静音客户端保持原文字与动作行为；本设计不改变 wire、Runtime、存储或请求兼容。内容更新按 Sxx 版本定位，未知版本在声音层禁用并给出本地不可用状态，不能取任意旧音效凑齐。回退可移除本专题消费者或保持关闭，以已知静音 Viewer 基线恢复；不回滚世界效果，不删除原请求胶囊或修改反馈真值。已经听到的提示不能撤回，修复错误触发后仍保留原结果解释。

## 11. 验证设计

按[音频制作与验收手册](viewer-audio-content-delivery.manual.md#audio-validation-cases)分层执行：静态事件/配方审读、实际素材检查、受控适配/时序验证、真实 Web/native 播放、静音等价和真人理解。只有实际做过的层级可记录通过。纯文档修改不强制制作音频、启动 Viewer 或发起模型试玩。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、source / scenario / candidate | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006) | [表面与内容取舍](#audio-surface-scope) | 当前声音目的和各表面适用性逐项可核 | [AU-STATIC](viewer-audio-content-delivery.manual.md#audio-static-review)；对固定源码与本设计逐表核内容/入口，记录未审边界 | 当前 PR 的源审阅与范围结论 | 不证明实际平台或内容已采用 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006) | [事件合同](#audio-event-contract) | 仅白名单新反馈提示，不从未知/历史推成功 | [AU-EVENT](viewer-audio-content-delivery.manual.md#audio-event-cases)；固定输入矩阵逐条推演，采用后对实际消费者断言 | 事件结果/零触发负例及实际候选 | 推演不证明真实 UI、身份或听感 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006) | [设置与生命周期](#audio-settings-lifecycle) | 默认、开关、音量、失活、设备失败与恢复正确 | [AU-STATE 与同手册 AU-RUNTIME](viewer-audio-content-delivery.manual.md#audio-state-cases)；同候选受控适配及实际平台分别执行 | 本地状态与真实播放记录、无世界请求旁路 | 单平台不代签 Web/native 其他组合 |
| [REQ-AGENT-SURFACE-006](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-006) | [优先级与混音](#audio-mix-budget) | 竞争有界，无持续噪声或延迟补播 | [AU-STATE](viewer-audio-content-delivery.manual.md#audio-state-cases)；受控时钟、不同优先级和洪泛，另核实际试听 | 启动时刻、峰值、节点上限及抑制原因 | fake clock 不证明设备延迟或响度 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001) | [静音等价表达](#audio-silent-equivalence) | 静音及失败仍能找到四个决策锚点 | [AU-EQUIVALENCE](viewer-audio-content-delivery.manual.md#audio-equivalence-cases)；实际双语、viewport、键盘与关闭/零音量/不可用组合 | 同候选 UI 与辅助技术记录 | 不证明全部语言/设备或真人理解 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002) | [事件合同](#audio-event-contract) | 受理、未知、不支持与真实结果不混同 | [AU-EVENT](viewer-audio-content-delivery.manual.md#audio-event-cases)；AE02～AE07 的独立负例与原文字对照 | 原输入语义、期望及真实接纳路径 | 不改变或代签 Runtime 原语义 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [消费身份与接口](#audio-consumer-contract) | 原操作、当前 scope 与迟到返回不得跨身份 | [AU-EVENT 与同手册 AU-STATE](viewer-audio-content-delivery.manual.md#audio-event-cases)；交错两身份/两世界/旧代次，原 authority 与音频层分别验证 | 零旧提示、原文本隔离与候选身份 | 音频适配测试不代签后端授权 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004) | [静音等价表达](#audio-silent-equivalence) | 拒绝有真实下一步，声音失败不重试原操作 | [AU-EQUIVALENCE](viewer-audio-content-delivery.manual.md#audio-equivalence-cases)；拒绝/未知/无设备分别操作并核提交/查询计数 | 原结果可读与零额外世界操作 | 不证明所有错误类别已由原消费者实现 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage) | [声音内容与配方](#audio-content-recipes) | 三份内容、来源和规格可复核 | [AU-ASSET](viewer-audio-content-delivery.manual.md#audio-asset-check)；核实际配方/生成器/参考文件和独立试听 | 来源、版本、样本分析与原试听记录 | 本次只有配方设计；无素材/权利验收结论 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage) | [交付与兼容](#audio-delivery-handoff) | 实际候选的内容/实现/环境及回退能关联 | [AU-CANDIDATE](viewer-audio-content-delivery.manual.md#audio-candidate-binding)；沿原构建核实际消费者与静音回退 | 原 PR、构建和逐平台证据 | 文档或单份参考音不证明发行就绪 |

## 12. 决策、风险与未覆盖范围

选择少量程序化提示，便于控制来源、响度、包络和体积，并避免当前阶段引入录音/外部语音依赖；代价是听觉表现有限，仍须实际试听才能判断辨识度、疲劳和品牌适配。若 S00～S02 在目标设备上难以区分或使玩家误判成功，声音 owner 修改配方及事件解释后重验，不能靠调大音量或删除静音测试解决。

当前必要的制作配方、消费者、生命周期、负例和验收方法已在本文明确；实际内容生成、控件/播放接入、平台选择锁定、设备试听、辅助技术和真人理解仍是交付/验证缺口。本文不把它们改称不适用，也不代表全游戏 GD-10、GD-12、GD-18 或发行就绪已通过。

产品 owner 拥有音乐/环境/空间/配音范围变化；Viewer owner 拥有真实适配与版本；声音制作方与 QA 共同核声波和听感。新增语音、后台提醒、音频独占信息、跨窗口同步或声音设置持久化时，应先回到相应上游要求，重新评估权限、隐私、内容、状态和成本，不能借用本专题的已审范围直接启用。
