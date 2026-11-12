# 外部 Agent 大世界 Web 与桌面可视化系统设计

- 设计状态：`target contract`；当前实现和运行证据不得由本文推定。
- Canonical repository：`nil-studio/oasis7`；审读基线：`a2389f437e6cedc3b870a91d33a1cdf40a00a8da`。
- Owner role：`viewer_engineer`；协作：runtime_engineer、agent_engineer、game_visual_interaction_designer、qa_engineer、Launcher owner。
- 最后审读：2026-10-16；文档合同revision：2，不是已发布API major。
- 产品入口：[世界舞台](../../product/agents-world-simulation/player-readable-world-stage.prd.md)、[外部Runtime游玩](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)、[玩家与Runtime协作](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 依赖authority：[Game API](../llm/external-agent-game-api.design.md)、[Runtime协作](../../world-runtime/runtime/player-runtime-collaboration.design.md)、[Viewer手册](viewer-manual.manual.md)。

## 1. 问题、目标与非目标

让玩家在Web/native界面观察同一持久世界，从总览进入局部，理解Agent/设施/路线/事件，连接合法高层指导并读取原行动结果。需要连接现有snapshot→Rust render state链路与未来授权、分页、流式大世界读模型。

目标不仅是画出对象，还包括主体、权限、来源、时效、部分性、交互与恢复可验证。非目标：另建世界引擎/第二Viewer、地图直接采集/建造/移动、全图全知、修改共识或世界schema、自动调用模型、把所有实体套Agent控制。

§9是待测的初始质量目标，不能写成当前实测成绩。

## 2. 上游约束与相关角色

Runtime拥有世界、控制版本、观察资格、admission及持久结果；API/身份owner提供授权查询、Context、撤销与原操作恢复；Viewer只派生显示/本地交互；Launcher交付受限原生容器；QA分开核验数据合同、GPU、各平台与真实世界。

### 2.1 需求承接与分配表

| 上游 requirement / product AC | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251；[REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001) | WorldView负载下本地可见接受、selected确认和网络完成分别可核，原预算不变。 | [本地反馈测量](#view-local-feedback-measurement) | Viewer反馈所有者/QA、真实意图及可见帧观测。 | DOM状态/设计不代签可见时延、网络完成或P6。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001) | 世界/目的/blocker/下一步与原结果可读，选择不改归属。 | [共享舞台](#view-scene) | Viewer/visual；Runtime因果。 | 局部HUD/截图不证明真人理解。 |
| Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002) | 来源/时效、冲突、旧响应、撤权与生效重验闭环。 | [观察到生效](#view-admission) | Runtime/API Context、fence、授权与原操作。 | Viewer不另建真值裁决/admission。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003) | 多尺度探索、partial适配、跟随/返回与有界密度。 | [探索查询](#view-query) | 授权空间投影、Rust/renderer、QA。 | 地形LOD不等于完整semantic zoom或已测容量。 |
| Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；[AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004) | Web/native自身交互、同义结果、重连和图形恢复。 | [原生与逐端合同](#view-native) | Launcher/WebView、发行资产、QA。 | Web成功/外部浏览器不代签原生。 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005) | 只读选择进入合法高层指导，原操作不重复世界效果。 | [指导边界](#view-guidance) | owner API、Runtime task/goal/receipt。 | 不新增地图动作或模型权限。 |
| Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；[AC-EXT-015](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-015) | 两表面各自实际操作同一世界。 | [原生与逐端合同](#view-native) | Viewer/Launcher/QA。 | 不证明真实Runtime或完整首局。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[AC-EXT-016](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-016) | 概览至局部、搜索并返回目标，未知不伪造。 | [探索查询](#view-query) | 授权索引及位置/关系。 | 不授予全图情报。 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[AC-EXT-017](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-017) | 世界现场连接合法高层指导和原回执。 | [指导边界](#view-guidance) | Agent/Runtime协作。 | accepted/动画不是world effect。 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | 原结果、原主体/目标及实际 Runtime 接收分别可读 | [view-runtime-results](#view-runtime-results) | API 原 operation、adapter 输入来源 | 不把收到报告或用户看到当模型理解 |
| Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；[AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007) | 四层事实各有来源/时效，回访不复活或重做执行 | [view-runtime-results](#view-runtime-results) | task/fence、driver、observer 与原结果 authority | 不以页面或健康检查代签独立 Runtime |
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | 外部用量与世界投入/占用按原来源显示，当前读权失效清私有投影 | [view-runtime-results](#view-runtime-results) | adapter cost、业务 receipt/ledger、读取权限 | 不另算事实总账或承诺完整账单 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[AC-EXT-017](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-017) | renderer 失败保留获准文本/原操作，安全恢复不重放指导或效果 | [view-recovery](#view-recovery) | 可信资源、当前 audience、原 operation 与逐端 host | 不从 renderer 恢复推导后端/Runtime/另一端就绪 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[AC-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | 通用 pending/未知及拒绝、过期、重规划和已生效分别可读，安全下一步不重提原件 | [view-intent-results](#view-intent-results) | API SR2 目标映射、Runtime decision/proof 与当前读权 | 不把当前五态 UI 或 Chat 替代升级为通用实现，不新增撤回动作 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[AC-DWE-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | 各原成员与获权前驱/胜者关系分别可读，旧终态不被后来的 winner 改写 | [view-intent-results](#view-intent-results) | API SR2 目标映射、Runtime decision/proof 与当前读权 | 不把当前五态 UI 或 Chat 替代升级为通用实现，不新增撤回动作 |

## 3. 当前状态、目标状态与差距

| 当前基线 | 目标适配 | 实现边界 |
| --- | --- | --- |
| viewer.html/viewer.js及oasis7_viewer_live，software_safe仅alias。 | 共用同一Viewer build与应用状态。 | 不建立第二入口。 |
| host_state.rs::build_render_state从有界snapshot派生DTO；JS消费Rust。 | 数据客户端先组装明确授权范围/版本，再交Rust和renderer。 | 分页WorldView不是完整snapshot的直接替身。 |
| 按类型reconcile、位置来源、地形LOD和宿主选择事件。 | 保留ABI，补partial/裁剪/来源侧带状态。 | 不把旧每帧全量重建当当前事实。 |
| 显示焦点仅取明确选中；下一动作与原回执各取其原绑定。 | 保持原操作关联，不从列表补主体。 | 缺原绑定保留null；真实Runtime组合仍须验收。 |
| Viewer WS `request_world_view`提供当前连接获准对象/设施/路线的有界查询、空间索引、同cut分页与可选增量、工业聚合和事件锚点。 | HTTP WorldView、远端独立观察binding、完整产业/事件空间投影。 | 新协议能力协商；来源不足标unknown，冻结续页标last-known；不证明完整大世界容量。 |
| Launcher控制面不证明内嵌世界。 | 同build的受限WebView世界窗口。 | 平台支持/运行证据待独立验证。 |

## 4. 边界与结构

<a id="view-scene"></a>
### 4.1 共享舞台与四层职责

```text
Runtime checkpoint / authorized events
 -> WorldProjectionReader (server auth, source cut, spatial projection)
 -> WorldDataClient (pages, generations, cache, recovery)
 -> WorldSceneAdapter -> Rust host_state -> WASM renderer + accessible host UI
                    -> TaskContextPanel (authorized task/goal/original receipt)
Owner guidance -> existing owner API -> Runtime admission -> original receipt
```

Viewer不拥有第二资源账本、任务完成规则或admission。外部executor的私有observation不能广播给Viewer；按audience投影。图形输入只含绘制所需获准字段，token/模型Key/私有prompt/其他主体内容不进render DTO、DOM probe或日志。

观察焦点只控制浏览；原操作固定world/Agent/task/goal与操作身份；回执按原身份关联，缺绑定不借active_agent_id。具体映射见[主体绑定](viewer-pixel-world-player-readable-rendering.design.md#subject-binding)。目的/blocker/下一步与原结果的可读性按[质量预算](#view-quality)及专项首读验收，TaskContextPanel不能取代世界舞台成为诊断面。

<a id="local-terrain-source-proof"></a>
### 本地生成地形来源证明

#5558 的本地生产路径仅在未连接 world service 且未启用 chain link 时，使用启动阶段已加载的 seed_model 作为本地生成地形 authority。它与 Runtime state 的空间证明分开：只提取每个 Location 的 id、name、pos、profile.radius_cm 与 fragment_profile.blocks，不包含资源账本、私有字段或新能力。获准 current 地形记录不附带未被该来源证明的 resources；缺资源保持未知，不填零，也不借地形时效提升库存事实。原始地形来自实际生成源，不在 Viewer 中合成或猜测。

SourceAuthority 私有保存各地点上述字段的摘要，摘要集合进入 source cut key；来源种类/版本进入 scope，世界和 audience 仍沿原 cache 身份。投影候选必须同时通过原 Runtime snapshot 验证，并与该地点的生成源摘要完全相等，才以 local_generated_terrain_cut/current 发出获准 fragment 字段。缺源、位置或组成被改写、来源失效、world/epoch/audience 切换均不得借旧 current 结果；切换或拒绝后消费者沿原清理合同回收图块。摘要仅为同进程本地来源绑定，不是远端签名、用户授权凭据或公开资源承诺。

远端 verified Runtime cut 不包含 simulator fragment，不能把运行态签名或本地 sidecar 旁路当成远端地形证明；没有服务明确交付且验证过的 terrain 来源时保持 unknown，不输出可绘制 fragment。新增远端地形证明不在本次实现范围。位置精度与组成仅为观察，不声明可采量、库存、道路或建造能力。

验收覆盖同源正常、单字段位置/组成改写、缺源、seed 地形变更后的 cut 失效，以及 remote/unknown 调用不能获取本地证明；真实素材/GPU与组合仍由 #5558/#5555 后续承担。Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3。

<a id="view-adapter"></a>
### 4.2 当前链路到WorldView的适配

| 输入语义 | 适配与消费者 | 禁止行为 |
| --- | --- | --- |
| 世界坐标基准 | world_bounds是world基准；viewport bounds独立。复用既有单位/轴/world-to-canvas。 | 用当前页包围盒重定义世界尺度或轴向。 |
| binding/cut/projection revision | 同cut页组装一个只读视图，范围/版本/完整性侧带保留；先验完整签名投影再裁剪显示。 | 混多个cut；裁未知字段后验签。 |
| 身份/权限 | cache key含部署/world/主体/audience/projection/query generation；应用前重验。 | 跨世界/账号按裸entity ID复用缓存。 |
| 几何与时效 | 几何：权威/地点派生/关系布局/缺失；时效：current/last-known/unknown/conflict/expired。 | 旧权威位置转当前snapshot，刚刷新派生位置变精确。 |
| 旧position_source | 当前权威坐标→snapshot；地点派生→location_derived；缺失→missing；其他组合保留侧带并有陈旧/抽象样式。 | 不能表达时效却绘成current；应抑制该动态图形并给文本。 |
| partial/页 | 同query scope合并并保留coverage/completeness。 | 一页缺项就触发全世界删除。 |
| 删除/淘汰 | 完整同cut scope替换或明确获准tombstone才说明相应删除；viewport/LRU只是本地不绘制。 | 过滤/裁剪/配额淘汰说成对象不存在。 |
| 关系/事件 | 独立authority、有效身份/端点/时效；无anchor事件留Feed。 | location_id/邻近自动生成关系，最近Agent作为事件位置。 |
| 目标/回执 | 独立协作流的绑定状态，不一致标待同步。 | 世界帧/当前选择改写task/原receipt。 |

适配层不合成完整Runtime model。现有Rust函数需要集合时，仅提供明确的授权可见集，不能丢partial或把本地reconcile删除解释为世界删除。新增读模型字段属于查询目标合同，不修改世界状态、既有WASM ABI或旧公开snapshot schema。

坐标有限且符合现有范围/单位，签名整数不经JS Number丢精度；只在验签/校验后转成局部显示数值，不反写权威坐标。camera、DPR、CSS hit region与renderer用同一转换来源，以角点/中心/缩放/高DPR fixture验证。

## 5. 关键运行流程

<a id="view-query"></a>
### 5.1 进入、探索、选择和跟随

先取得部署/world身份与许可范围，再查询summary/初始viewport；授权先于聚合/计数，unknown不泄漏隐藏数量或位置。按[适配合同](#view-adapter)组装页/来源，按[质量合同](#view-quality)处理密度和输入预算，不能只实现拖动画布就宣称多尺度探索完成。

平移/缩放立即改本地camera，稳定后合并有界只读query；取消仅节流，正确性由generation保证。失败保留仍获准的旧范围并标最近已知，不给空世界。搜索区分当前Targets过滤和授权空间搜索，结果只提供稳定ID/来源/定位线索，不自动提交。

<a id="view-explicit-search-intent"></a>
**后台读取期间的显式搜索。** 正常挂载且仍具备当前scope的WorldView搜索表单不因后台query loading禁用。一次真实提交立即给出本地等待／读取反馈，接受只表示本地意图，不是网络结果。controller保持single-flight，至多保留一个最新显式查询意图（包括清空搜索）；新意图替换旧意图，旧调用明确结束为未完成结果，不累积then链或并行transport。已有读取立即invalidate其发布代，晚页／错误不得覆盖新意图；等原读取结束后只派发最新槽。槽以原30秒上限有界结束，不重发原输入。

显式意图等待或读取时，后台snapshot/poll/相机/pin触发只合并dirty，不invalidate或抢占显式结果；结果结束后恢复原调度。总览当前查询不改变查询，保留当前显式查询并等待同绑定结果；其他新显式搜索、导航／选择、scope/world/connection/authority失效或卸载取消旧槽，所有调用promise均结束；旧finally不得清除新槽或发布成功。搜索仍沿既有授权查询、分页／字节／实体限额，不新增grant/pin能力，不冻poll或放宽生产权限。scope失效不能借旧输入恢复读取。完整／partial／错误仍按原读模型呈现，不因本地接受升级事实。此条不将搜索完成套用选择ACK预算，也不改变P5/P6或原性能测量口径。

选择/跟随绑定world/entity/selection generation。显式跟随只消费当前可信位置；手动浏览退出跟随，失效暂停，恢复数据不自动抢回camera。重叠候选列表、聚合下钻、返回总览/主目标都只改读状态；目标无位置则返回合法文本，不用fallback坐标。

<a id="overview-query-fit"></a>
**显式总览当前查询结果。** WorldView按钮明确命名“总览当前查询结果 / Overview query results”，只对当前获准查询的结果范围操作，不宣称已看见全世界。按钮不清空搜索、不同时启动另一查询；修改搜索/范围仍走独立查询操作。`world_bounds`提供坐标域，不证明当前查询覆盖或隐藏对象范围，不能把它冒作完整结果集合。

一次显式总览有独立用户导航代，和world/当前scope及authority、来源cut、查询代分别绑定。当前结果为可信、未冻结的complete且具有可拟合位置时（含已验证的聚合中心或事件锚点，历史锚点仍为历史，不转成跟随位置；不让装饰热点改变查询范围），消费一次并拟合该完整集合；导航不借旧snapshot、旧位置或另一查询补全。partial仍在读取时，最多保留一份有界待检查总览，保持原相机并明确显示“查询尚未完成”；同一查询的完整合法结果到达后才一次拟合。原查询终止仍partial（包括累计cap/字节预算）、完整性unknown、错误/陈旧/冻结或完整集合无可拟合位置或显示预算已省略几何marker时，明确总览不可用并保持相机，不无限pending；等待上限沿原查询30秒。该按钮不额外提供或暗含“仅总览当前局部页”的行为。

新导航/显式选择/查询变更、用户手动相机、scope/authority/cut/reorg变化或卸载使旧总览意图失效，晚页不能抢回相机。自动分页、相机状态/viewport回报不是新的用户导航或手动相机代；自动回报不得被误报为手动取消。完成总览所产生的正常viewport查询仍可按原调度更新，但后台partial/full、增量或相同导航代不得重新拟合或形成查询循环。

新合同由 navigation.overview_query.version=1 显式启用，记录公开查询绑定；该字段已声明但为null、版本未知或缺必要绑定时拒绝，不降级legacy。对应RenderState的overview_query_context保留同一绑定、completeness、source_frozen及独立display_limited（输入/输出显示marker省略，不冒作查询partial）；本地导航ID按page单调且重挂载不复用。Host到Rust的现代WorldView导航必须保留查询完整性与该导航所绑定的查询身份，异步derive/update不能把完成时的最新view补到旧导航结果。Rust只在匹配绑定且complete的输入上消费该用户导航一次；完整结果派生提交尚不等于Rust已消费：同意图提交到Rust后，原已调度后台刷新可短暂合并等待实际navigation_result（request_id/status/render_version）主Update回报或原30秒期限；该回报不是GPU/呈现fence，不新增读请求。已计划刷新随后按原调度恢复，不能静默让下一自动read覆盖尚未消费的导航；其间新用户操作仍可立即取消。当前render版本、已处理/抑制fit版本不等于实际导航消费/拟合输入版本。调试证据若记录拟合，须区分这些版本及实际输入集合，不能由版本字段反推完成。真正legacy snapshot或旧的无该查询导航元数据输入保留原显式导航行为，不擅自等待不存在的complete字段；已声明现代WorldView查询元数据但缺失/非法完整性则明确unknown/不可用，不能降级冒作legacy。Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。

#5605：同scope已呈现目标在新查询partial暂缺，不等于删除。Host可为该呈现绑定下的只读点击登记至多一份待检查选择意图，仅保留typed identity、world/epoch/audience及用户选择代际；不复制旧实体、坐标或权限事实。等待明确可见，不改变业务主体、原回执或导航。目标经同一有效请求/cut的后页重新证明后才消费一次；不借旧snapshot补事实，不为任意未知ref发pin探测。

完整结果仍缺、错误/不可用、明确删除、换scope/authority/cut/request、更新选择或导航、卸载均终止该意图；等待最多30秒，与现有查询期限相同。自动分页不创建新用户意图，晚包不得覆盖更新操作。本切片只等待原有查询的后页，不新增重读、授权或世界命令；原查询结束仍缺则明确不可用。Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。

### 5.2 分页、增量与并发

初始snapshot与resume cursor来自同source cut；page token绑定world/audience/query/projection/cut，失效reset，不能用下一cut补旧页。可渐进显示合法页但始终partial；服务器明确完整且全部页验证完才complete。单页200项/实际编码2 MiB，超大单项明确不可用/资源限制，不截字段假装完整。

增量按服务分区顺序，用world/epoch/stream/event身份去重；同身份异内容、缺口、cursor失效、reorg或投影不兼容时reset，不按客户端时间排序。world/task分别维护cursor/binding/revision，overlay不一致显示待同步，不拼成原子事务。

request_generation是客户端请求代际，不证明世界新旧；current需可验证cut/binding，不可比较cut不能以较晚HTTP响应覆盖。撤权/换账号先清私有缓存、图形、详情、搜索与在途结果，再初始化新范围，不先绘制后鉴权。

<a id="view-guidance"></a>
### 5.3 从对象到高层指导

只对当前合法owner开放目标/Prompt/审批/stop。地图、terrain、设施、路线及其他Agent点击不构造action:submit。客户端提示不代替服务器逐次授权。选择不固定业务主体；明确提交后原操作身份不可被新选择改写。

目标saved、activation pending、active、executor read/adoption/context-used与world result分开。审批需明确操作及精确方案摘要，普通Chat不是批准。提交沿原owner API expected revision/client_operation_id，效果只由匹配原操作/Agent/task的受权receipt确认；[权威重验](#view-admission)是完整链路的生效边界。关窗口不停止独立执行器，renderer健康不代表executor状态。

<a id="view-runtime-results"></a>
#### 原结果、执行事实与费用的共同投影

Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7, O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e, O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43。分别承接 [AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005)、[AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007)、[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) 的玩家消费范围。核对日期与基线：2026-11-05；`cca8cda97cf121ebc1e25e96d6b0204a421323ea`；下列为目标接线与显示合同，不是当前 Web/native 验收结论。

世界结果来自 [Game API 原结果与反馈](../llm/external-agent-game-api.design.md#api-result-feedback)，Runtime 输入交付来自 [adapter 的原运行关联](../llm/external-agent-runtime-adapter.design.md#adapter-result-feedback)。每个结果保持原 world/Agent/task/goal/executor/operation 身份；切换选中对象只改变浏览焦点，晚到结果不能归入新 Agent 或新目标。受权已结算结果可以先显示“世界已确认，Runtime 接收未确认”，并保留真实业务阶段；只有 driver 的实际输入证据才显示该输入已接收，不能写成模型已理解或已执行下一步。

观察连接、Runtime run、当前委托和世界结果分别消费[四层事实](../llm/external-agent-runtime-adapter.design.md#adapter-liveness)，各自注明来源新鲜度。有效心跳不能把无 run 的执行器显示为正在思考，执行器停止也不能把 pending 世界工作显示为已取消。回访先读取完整当前任务与原 operation，旧未知保留为“结果待恢复确认”；观察订阅重建不创建 context、不启动 Runtime 或续期委托。pure_api 使用同一事实投影，仍单独验证其可消费结果。

费用显示消费[预算与成本来源](../llm/external-agent-runtime-adapter.design.md#adapter-budget)：外部实际 token/工具/币种、估计及未知分开；世界资源按原业务身份、payer、单位和 reserved/consumed/released 阶段显示。Viewer 不从余额差或模型摘要另算事实总账，不叠加重放行或把控制 ACK 当生产消耗。未确认总额如实标未知；外部费用或资源余量不形成继续执行许可。

执行失权后按当前读取权限保留可读原结果。读取也失权时清除私有图形、文本、费用、搜索及在途旧响应；原结果在权威历史保留不等于当前页面仍有权展示。owner 可沿原受保护路径核对，不能由 Viewer 借新 token 或最新选中 Agent 自动接管。超限、分页未完、受众/切点变化和来源缺口按[分页与代际](#view-query)及[恢复](#view-recovery)处理；关键结果不可静默裁掉后宣称完整。

<a id="view-intent-results"></a>
#### 通用 intent 的独立处置与关联

本节是 [API SR2 结果映射](../llm/external-agent-game-api.design.md#api-intent-results) 的待实施消费者，只在该真实能力和 schema 已协商时使用。现有原生任务/Gameplay/Chat/Prompt 等原结果仍按各自合同，不能把当前控件或五态 registry 当作已接入这组新态。

当前原操作保持自身 world/Agent/task/goal 与 operation/intent 的来源关联，分别表达“等待权威结果”“结果待恢复确认”“已提交的原效果”“被拒绝”“已过期”“需要重新规划”“本请求无效果终止”及真实原因。已提交仍保留业务后续阶段；无效果终止不写成全部关联请求取消。过期须来自权威决定，不能由页面倒计时产生；查询/读取失败也不能替原请求生成终态。

原成员始终保留自己的结果卡/结构化项。linked replacement 或 withdrawal 以只读“关联原请求”入口连接其直接前驱；exclusive loser 以只读“生效的关联请求”入口连接已验证 winner，不覆盖原卡。对 available 关系才显示其真实原对象与状态；not_readable 只说明当前无法读取关联详情，history_unavailable 说明原关联资料不可得，两者不泄漏 ID/hash/私有原因，也不推断没有关联。单次不展开整条 lineage；打开下一原件仍执行当前读权，跨主体、分支和迟到响应按原 scope 隔离。

查看、等待、返回目标与原结果回查都是原合法只读路径；需要新计划时回到当前目标/能力，只有 domain 已支持且当前有权时才展示其明确撤回/替代入口。状态变更、关闭页面、停止 Runtime 或关系卡点击不自动重提、换 nonce、启模型或释放费用/资源。API 能力未支持或原件不足时显示具体阻塞，保留原请求身份与安全返回，不用旧 pending 徽标掩盖新态。pure_api 使用同一原语义；Web/native 的文本、关系导航和恢复各自按 [IR01–06](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-intent-results) 验证。

<a id="view-admission"></a>
### 5.4 从观察到生效的权威重验

Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；承接[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002)。

[查询代际](#view-query)和[来源适配](#view-adapter)保证显示不伪造当前观察；下表连接到实际权威生效，不新增Viewer自己的admission。

| 观察/操作 | Viewer表达 | 权威边界 |
| --- | --- | --- |
| 当前授权只读 | 展示/定位/检查。 | 读取授权和cut，不授予写权。 |
| 最近已知/冲突/未知 | 获准内容可只读，纯建议/草稿标待核验。 | View DTO不能铸造Context/grant/成功，预览不生效。 |
| owner目标保存/激活 | 固定主体/目标/expected revision，区分active与awaiting_executor。 | 原owner API重验owner权限、task/goal版本及控制提交；可在executor未绑定或离线时激活，不以executor Context为前置。 |
| owner审批/高后果授权 | 固定主体/精确方案/expected revision，披露旧观察。 | 沿适用审批合同重验当前身份、scope、版本及所需条件；离线目标激活不意味着任意审批都可离线通过，也不直接产生世界效果。 |
| executor实际世界动作 | guidance状态与执行/原回执分开。 | 原Game API Context/action合同重验执行主体、权限、控制/目标版本、fence及适用世界条件；Runtime最终裁决资源/规则。 |
| 预览后条件变化 | stale/conflict/重新授权；改方案必须重新确认。 | 校验与实际接受/生效权威事务关联，不是Viewer独立GET即获许可。 |
| stop/撤销/原结果查询 | 不因图形失败或观察陈旧统一置灰。 | 各自owner授权和控制合同，不能绕过撤权。 |
| 丢reply/结果unknown | 保留原operation，显示recovery_required并回查。 | 不新键/换Agent/重新报价替代已提交请求，按原幂等恢复。 |

owner指导沿固定主体与方案→核验owner权限及适用控制/审批条件→纯预览→明确确认→原owner API版本约束提交→pending/拒绝/待确认→原结果回读；目标激活与executor接入、Context取得和行动admission分开。

实际executor动作另按读取既有权威Context/能力→提案→Runtime重验→原结果回读执行。消费[Game API Context](../llm/external-agent-game-api.design.md#api-context)与[原操作合同](../llm/external-agent-game-api.design.md#api-actions)，不发明新的UI许可字段；observed_at_tick、badge和WorldView cursor都不是admission许可。执行世界动作的Agent仍须自己的Context/fence通过Runtime，owner guidance不绕过。

记录可脱敏关联的原操作、显示cut/分类、expected/实际控制版本及权威原因供对账，不公开token、观察正文或私有推理。

### 5.5 初始实现范围

#### 增量、工业聚合与事件空间层

Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。

三项查询扩展分别协商 `world_view_delta_v1`、`world_view_aggregate_v1`、`world_view_event_anchor_v1`，复用原 `request_world_view`，旧服务端不收到新增字段。分页 token 与增量 resume cursor 分开；只有完整、非冻结且满足累计预算的查询最终页提供增量基线。`resume_cursor` 绑定会话、world/epoch/audience、来源权限和完整查询（含视口、搜索、pin、层级、事件开关），不是世界写权限或 Feed cursor。更新以十进制序号、base/target cut 和 query ID 原子应用；重放冲突、缺口、过期、撤权、reorg、不兼容、超出单批 200 项/2 MiB 时 reset 并重建基线。未改变的获准记录仅按明确 carry-forward 确认到新切点，不提升未知几何。持续活动合并为单个在途读取，可见页面有界补读，切换范围立即使旧更新失效。

增量移除区分 `projection_removed`/离开查询范围与世界删除。视口、搜索、聚合、权限投影和事件保留窗口的移除仅撤下本地记录，不声称世界对象消失。只有同授权来源的明确回收事实才能发布 `world_deleted:true` 的设施删除通知，且只通知此会话此前已见的对象；远端投影未提供回收事实时保持未知，不从缺项推断。路线 unavailable 不等于删除。scope 改变先清缓存，不通过旧范围 tombstone 泄漏身份。

`detail_level` 为 `auto/overview/site/entity`；工业层级在授权、空间与搜索筛选后，对可信当前设施按有界网格聚合。聚合 DTO 独立于业务实体，含显示中心、格网范围、设施数量与类型计数、来源和下钻层级；无可信位置的设施不进入空间计数。pin 保留原对象并排除重复统计；明确路线可将端点映射到聚合节点汇总，不能按邻近生成产业关系。展开聚合只用 Rust `fit_bounds` 调整相机和更细查询，不设置业务行动主体；聚合中心不是任何设施的精确坐标。全部记录共同使用分页与字节预算，部分查询明确不完整。

事件空间层只从已获准 typed journal payload 提取显式位置，首批支持 AgentRegistered 与 absolute ModuleVisualUpsert。远端只消费 verified projection 自带的获准事件；公开投影无事件时不补造。事件按 `(world_id,reorg_epoch,event_seq)` 保留原身份并与 Feed 精确关联；当前实体仅用于验证可见资格，绝不拿其现位置填补历史事件坐标。无锚点、无权限、冲突或未知来源事件不落地图，仍可在原 Feed 表达。事件点击仅检查、定位已验证锚点或查看原 Feed，不跟随事件、不补写行动或回执主体。事件集合进入切点与缓存身份，旧 epoch/旧 scope 不再命中；历史锚点明确为历史位置，不冒充当前设施/Agent 状态。

#### 绑定来源与空间查询扩展

远端空间观察复用服务端已验证的 WorldService 投影，不接受客户端自报 current。当前性须同时匹配读取配置、world/branch/权限/投影版本、普通证据策略、Runtime 的空间依赖字段与兼容 snapshot；未知、observer、回放或不匹配时降级。出站仅含非秘密来源绑定，不发布 read key 或其身份摘要。分页保留同一切点；仍能验证为当前同一来源切点时保留当前，来源推进或无法比较时冻结并降为最近已知；权限、来源范围或 epoch 改变立即使旧 cursor 失效。

设施从获准 snapshot 的 factories 投影，路线从同源 runtime_snapshot 的 logistics_routes 投影。路线端点只引用同一可见投影中的 Agent、设施或地点，不从邻近或 Agent 所在位置推断设施位置、归属或路线。设施明确绝对空间锚点可保留权威位置；只有地点身份或来源不足的派生坐标保持未知，可搜索和只读检查，不能伪装为可定位实体。路线 unavailable 与 active 分开，只有同切点、当前且端点可信的 active 路线绘制连线。

客户端和服务端仅在共同协商 `world_view_pins_v1` 后使用新增请求字段；旧 `world_view` 服务端继续使用原 bounds/search/cursor，不发送其不认识的字段。

`request_world_view.query.pinned_entities` 可携带最多三个 `{kind,id}`，仅在原授权可见集合内合并选择、目标和跟随对象并优先分页；不存在或越权 ID 不增加存在性信息。实体与关系共同消耗每页 200 项、累计 1000 项和字节上限；客户端达到累计上限即保留 partial 和现有可信对象，提示缩小范围，不伪装读取失败或完整结果。关系的两端点可跨页到达，未齐不绘制。范围外对象和聚合淘汰仍不表示世界删除。

相机事件增加十进制字符串 `viewport_bounds` 与逻辑像素尺寸，使用 Rust 绘制转换的逆变换，向外取整并裁至世界边界；宿主不重算另一套坐标。连续平移/缩放立即使旧查询代际失效，尾部防抖发出最新 bounds；应用查询结果不触发自动总览或查询循环。服务端在完整授权投影建立有界二维包围盒索引，查询索引后再筛选文本、稳定分页并合并获准 pin/路线端点；索引和来源切点一起重建，不能跨权限复用。

本查询扩展不包含原生容器或独立推送订阅；工业聚合和事件空间层按上文合同提供。索引正确性与候选裁剪可用确定性测试证明，50k 世界、P95 与长期驻留内存仍须按质量条款独立实测。

当前空间读模型使用已协商 `world_view` 的 Viewer WebSocket：`request_world_view` 请求带 `request_generation`、可选 `search`/`bounds`/`cursor` 和 `page_size`，响应为 `world_view`。它只裁剪同一连接本来可读的兼容 snapshot，不新增 Agent 私有观察权限。展示字段经 allowlist；world、reorg、audience 和查询范围隔离分页。每会话只保留一份有界短期只读投影，不能用旧 audience 的 cursor读取。

初次查询只在匹配本地权威 Runtime 或已验证远端来源的 Agent/显式设施位置上声明current；回放、observer、来源不匹配和缺几何证明的地点/设施位置保留unknown。仍能证明来源切点未变的续页保留current；冻结历史续页的全部显示集合降为last-known，不以分页完成升级时效。客户端按整个查询范围保留partial，不把 viewport/搜索淘汰解释为删除；明确删除事实按增量合同发布，其他移除不表示世界删除。请求代际、输入导航、world/task流分别处理。

图形context loss停止原会话和命中，保留仍获准的文本/原回执并提供显式重试；授权身份或world变化先清显示，旧异步挂载和事件不再生效，新scope通过带请求身份、socket和作用域代际校验的 `request_scope_snapshot` 定向重读后才可解锁；普通迟到snapshot不能解除锁定。旧本地反馈按提交scope隐藏，但原pending操作身份保留，不重新提交。导航明确区分选择、定位、跟随、停止和总览；手动平移/缩放退出跟随，失去可信位置后不会自动恢复。

本切片不交付 HTTP 目标路由或 native 窗口；投影只覆盖现有获准对象、明确路线及最近有界 typed 事件候选，不声明完整历史、所有实体类型、§9 数值预算和端到端验收已经通过。

<a id="public-feed-history-source"></a>

### 5.6 公共 Feed 历史来源与重连就绪

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a。承接 [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)、[AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002) 与 [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)。

公共 Feed 的历史分页与实时流复用同一获准公共投影和既有 `world_feed/v1` 事件身份/顺序；WorldService 模式从该世界的权威公共历史读取，不用本地 disposable Runtime 的空历史冒充远端历史。历史不可用、超出保留窗口、分页未完和已确认空窗口分别表达；不得由空的本地缓存推断世界从未发生事件。cursor 绑定 world、epoch、audience 与来源切点，换源或权限变化使旧 cursor 失效，不能拼接另一世界的历史。Feed 不补造私人操作回执或行动完成状态。

投影在服务端以允许列表选择公共事件类型和字段，历史与实时走同一隔离边界。未列入的私有上下文、供应商原始响应及其他主体诊断不进入公共 DTO、搜索、缓存、日志或错误详情；未知事件默认不公开，不以内容字符串过滤代替类型边界。负例仅使用合成 canary 验证匿名与不同账号均不可见，不记录或公开真实私有内容。

普通断连后，旧画面只可作为仍获准的 stale/last-known 来源；新 socket 的 hello 不代表世界 ready。只有当前连接代、当前 world/audience 和当前读取作用域匹配的首个有效 snapshot（或符合原合同的增量恢复基线）被接纳后，才解除连接恢复锁定并重新评估只读交互；控制能力仍由独立 authority 判定。首读失败保留明确失败/重试入口，不无限等待，不用旧 socket 的迟到 snapshot 解锁。重试是有界只读恢复，不重发原 pending 操作；scope 切换继续遵守 5.5 的定向快照约束，报价继续按各自相关输入/版本合同失效，而非任意 snapshot 一律清空。

## 6. 接口与数据合同

目标路由为Game API `/v1/game/worlds/{W}/views`，复用鉴权。以下为实施输入，不声称端点已部署；wire schema由API owner与实现锁定，未知版本明确blocked/升级。

```text
WorldViewQueryV1 = {api_revision, request_generation, scope_ref,
  scale: world|region|local, bounds?, focus_entity_id?, filters, page_size, cursor?}
WorldViewSnapshotV1 = {world_binding, snapshot_id, projection_revision,
  audience_revision, source_cut, request_generation, coverage,
  entities, relations, events, next_cursor?, resume_cursor,
  completeness: complete|partial|unknown, unavailable_reasons}
EntityViewV1 = {entity_id, kind, parent_ref?, label,
  position?, position_source: authoritative|relationship_layout|last_known|unknown,
  observed_at_tick?, valid_at_source_cut, visibility, activity?,
  task_refs, receipt_refs, authorized_guidance_capabilities, provenance?}
provenance = {geometry_source: authoritative|location_derived|relationship_layout|missing,
  freshness: current|last_known|unknown|conflict|expired,
  source_ref, observed_at_tick?, valid_at_source_cut, reason?}
```

provenance区分几何和时效，旧position_source仅兼容表达。缺provenance时只保留旧输入可证含义，不能凭authoritative升级current；没有独立观察authority/binding证明则unknown/last-known，并阻止不适用的生效。新增必要语义须版本协商，不以optional默认值让旧consumer假装支持。

实体类型、单位/合法关系沿Runtime与语义定位；tick/revision为精确十进制。activity/Intent/receipt独立，unknown不填零坐标，不可见对象不回私有label/数量。

POST /query只读；GET /entities/{id}重验主体且绑定cut；GET /search的q/filter/cursor和条数有界；GET /events只投影获准事件。opaque同cut cursor不能用offset替代；无anchor事件留Feed。

错误沿API envelope：身份/权限不泄漏存在性；cursor失效重snapshot；cut/revision冲突重新确认；资源超限明确partial或拒绝；服务不可用不伪装空集合。只读重试有抖动、次数和总deadline，副作用失败先回查，不无限刷新。

## 7. 状态、事务与持久化

camera/selection/filters/drawer是客户端展示偏好，可按账号/world保存轻量非秘密设置；任务/审批/原操作/结果不以localStorage为共享真值。换账号/撤权清旧私有缓存和恢复键，不恢复旧selection后泄漏详情。

空间索引可重建，index generation与cut一致；缺checkpoint/root、落后或不可验证不给current。先授权再聚合/计数，读投影不改世界历史。network/renderer/query/task/executor/world operation各自状态独立；已committed不因重载改pending。按字节/对象数有界LRU，pin不能绕授权或无限增长。

<a id="view-recovery"></a>
### 7.1 降级与恢复状态机

Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096。除逐端恢复外，本节直接承接 [AC-EXT-017](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-017) 的 renderer 失败可信文本与安全恢复：文本/目标/原结果先通过当前 audience 与[原归属投影](#view-runtime-results)核验，不能为保留信息绕过撤权。图形失败只影响观察能力；仍获权的高层指导、Stop 和原结果查询保持各自可用条件，恢复图形不重放它们。

| 触发 | 清理/保留 | 恢复条件 |
| --- | --- | --- |
| boot/loading | 稳定舞台框、获准目标与文本。 | 正确资源hash、当前读模型、实际renderer ready。 |
| 初始GPU/资源失败 | unavailable，无第二JS世界。 | 原可信资源显式重试或文本/Targets降级。 |
| ready后context loss/fatal | 旧命中/动画停止，旧帧非实时，合法文本/原操作保留。 | 重建会话，重应用授权视图/camera/selection并验证命中，不能只看probe。 |
| disconnected | 来源stale，可披露历史保留，控制按authority。 | cursor/binding/audience有效才增量，否则snapshot。 |
| query failed/partial | 标受影响范围，不误删仍有效旧范围。 | 重查scope，完成前不complete。 |
| reorg/gap | 清失效cursor/索引，结果按权威重组状态。 | 新checkpoint和配套cursor，不重放为收益。 |
| 账号/world/audience变化 | 先清图形/私有缓存/详情/hover/搜索/旧promise结果。 | 新授权后初始化。 |
| 原结果未知 | 原operation和recovery_required保留。 | 查证原结果，无证据不产生第二效果。 |

自动图形恢复最多一次有界尝试，失败后显式重试/文本，不无限reload。关页面/窗口只释放自有视图订阅/GPU/临时缓存，不停止独立worker或其他进程。

## 8. 部署、安全与运行约束

<a id="view-native"></a>
### 8.1 NativeWorldViewHost与逐端合同

现有Launcher增加受限世界窗口，加载同一有hash的官方Viewer build，不弹系统浏览器。Web与native共同执行[探索](#view-query)、[原结果与指导](#view-guidance)及[恢复状态机](#view-recovery)，逐平台取证，不因共享代码免验。

目标选择系统WebView：macOS WKWebView、Windows WebView2；Linux等另外候选，未验证不声明支持。本表只规定新增世界窗口的实施，不重写其他组件支持范围。

| 组合 | 设计选择 | 上线前锁定与验证 |
| --- | --- | --- |
| Web桌面 | canonical Viewer和Game API。 | OS/浏览器/GPU/版本、viewport/DPR、release交互。 |
| macOS native | 受限WKWebView窗口。 | Rust binding/锁文件、OS/架构、WASM/WebGL、签名安装/升级/恢复。 |
| Windows native | 受限WebView2窗口。 | binding/runtime分发、OS/架构、GPU恢复、安装/升级/回滚。 |
| 其他浏览器/移动/Linux | 独立通过后扩大支持。 | 同义旅程、安全、质量和发行资产，不继承green。 |

最低版本和依赖由Launcher owner在实施候选锁定；这是明确交付项，不用已有纯数据Launcher代替内嵌内核。未通过的组合未验证/不支持，不静默打开系统浏览器计通过。

### 8.2 来源、认证与IPC

远程用受信HTTPS同源，本地用显式启动loopback服务；导航/redirect按allowlist，第三方页不能获native bridge，不禁TLS。IPC仅生命周期、用户明确文件保存和安全存储，无通用shell/任意文件读/world action后门。

复用device/session proof；一次性bootstrap绑origin/nonce/窗口，secret不进URL。cookie沿既有Secure/HttpOnly/SameSite/CSRF；native不共享模型token缓存。深链只含world/entity/task，重鉴权后只读定位，不携bearer或自动批准。

### 8.3 资产与可观测性

页面/模块/字体/optional WASM沿同manifest、SHA-256和build provenance；缺失/错配明确失败，不下载任意代码补救。图形payload缺失不清合法task/receipt/control。发布区分核心UI可操作、renderer ready、完整探索，安装包存在不是可玩。

分段记录读/解码/派生/reconcile/首帧/输入、payload字节、cache、reset和renderer失败阶段；标签有界，用脱敏关联和原因，不记token/prompt/私有label/坐标/其他账号数据。未测量不宣称达标。

## 9. 质量与容量

<a id="view-quality"></a>
### 9.1 容量和密度

单页最多200项、实际编码响应2 MiB；单视点累计1000实体投影、突出200labels。目标/选中/blocker/相关路线优先；超限授权聚合/分页并partial，不无声吞掉关键对象。详情可独立查但画布不无限扩容；关系端点和聚合成员负载分别记录。

<a id="world-view-render-density-profile"></a>
#### 授权投影显示 profile

#5596 将上述查询累计容量落实为独立的 `world_view` 显示 profile，而非扩大旧 snapshot profile。桥接字段 `display_profile` 仅允许 `legacy_snapshot_v1` 与 `world_view_v1`；缺省仍为旧profile，Host仅在既有 `worldView` 对象投影入口写入 `world_view_v1`，Rust只在显式 `world_view_v1` 时要求有效worldView对象；省略或 `legacy_snapshot_v1` 即使带旧worldView仍走500/100/256 KiB，不自动扩容。未知或null profile拒绝，新world profile缺少worldView对象拒绝；profile 不是能力协商、授权凭证、完整性或 freshness 证明。既有 scope/cut/分页、授权过滤、unknown/stale/revoked 规则先执行，不能借旧 snapshot 填满显示数量。没有 worldView 的旧输入仍受 500 marker、100 visible label、每方向256 KiB约束。

`world_view` profile 的 Host→WASM 与 WASM→Host 各自实际 UTF-8 JSON 上限为2 MiB（独立的显示传输上限，不将查询单页2 MiB视为全部累计结果可无界复制），最多1000个普通可见 marker、200个可见canvas label、每label128 Unicode标量。集合最多1000项、深度16、字符串8192个UTF-16 code unit、复制节点131072；普通对象字段数仍最多500，关系集合独立有界且不能借端点依赖突破查询累计1000项。JS白名单复制与Rust输入/输出独立复核profile和各自边界，未知profile或与入口不一致拒绝；不接受自由数值cap。先完成受限JSON解析/结构校验，再派生场景，超大输入不得先无界派生。

容量是上限，不是为凑数补充未知事实。保留选中/目标/blocker及位置依赖的既有确定性顺序；字节或节点不足时缩减可选marker与装饰，记录实际source/projected/omitted数量及原因。必需语义无法容纳则明确unavailable，不截断JSON、不静默丢关键结果。label先遵守各生产者zoom/碰撞/权限可见性，再按同一优先顺序实施200全局预算；不能强行把隐藏label计作已显示。切回旧入口、删除、撤权、清空和重建必须同时恢复相应预算并清理旧命中/背板。

此profile不改变世界尺度、LOD、授权、写命令、Targets完整授权列表或性能预算；实际1000 marker/200 label仍需可观察计数与真实绘制验证，cap常量不能代签负载或可读性。正式目标机A/B、网络/查询、输入与世界帧间隔仍按下节和父项验收，缺外部设备不阻塞profile的边界实现及局部实测。

服务端有界空间索引、先授权后聚合；客户端合并读请求、有限cache、静态reconcile与动画分离。50000对象是测试世界规模，不是每帧传/画50000。洪泛不能阻塞输入、退出跟随、stop、原结果查询。

密度先退次级标签/背景；确定性散布不证明重叠可用，还需候选选择。关键状态有非颜色信号；派生位置不作可执行距离依据；5秒首读与误归因测试按专项手册，模型审读不代替真人/L5。

### 9.2 初始验收预算

以下是待测目标，不是当前性能报告。A：桌面8 GiB内存、集成GPU、100 Mbps/40 ms RTT；B：同设备10 Mbps/150 ms。实际CPU/GPU/OS/浏览器、电源、刷新率、viewport/DPR必须记录，缺身份不可比较。

| 指标 | 初始目标 | 边界 |
| --- | --- | --- |
| 核心目标/反馈/恢复UI可操作 | A冷P95≤3 s；B≤5 s。 | 从认证已完成的导航起，不等待可选renderer；登录另计。 |
| 首个真实可交互世界帧 | A冷P95≤20 s、热≤5 s；B冷≤90 s、热≤10 s。 | 含payload下载/编译、snapshot、派生、首帧；实际字节冷/热分别报，不把占位/DOM算世界帧。 |
| 有界query服务耗时 | P95≤300 ms。 | 50000对象授权scope；网络/响应体与e2e另报，不以热缓存替冷索引。 |
| 本地选择/退出跟随/stopUI反馈 | P95≤100 ms；无连续1 s输入无响应。 | 只是UI接受，不等于网络/Runtime完成；端点分层见[本地反馈测量](#view-local-feedback-measurement)。 |
| 连续平移/缩放帧间隔 | P95≤33.3 ms，P99≤100 ms。 | 1000实体/200labels、前台交互；静止按需和后台节流分开。 |
| 应用可归因缓存 | ≤64 MiB且有对象上限。 | renderer/进程/WASM/GPU另测；LRU不删权威结果或留旧权限。 |
| 30分钟长稳 | 无崩溃/旧命中；稳定同负载末窗口内存较初窗口增长≤10%。 | GC和测量方法明确，未知分量不以JS heap代替。 |

<a id="view-retention-observation"></a>
客户端保留观测只读当前实例/本地代际：分别统计已发布view与changes、仍活跃分页的entities/relations/aggregates/events Maps和cursor集合，不能假称已有LRU。统计只返回计数、逻辑JSON UTF-8字节和采集成本，不返回ID、文本、scope凭据或DTO副本；旧代际异步查询不计作当前scope数据；清除统计引用不代表已取消异步闭包或释放其Maps，transport完成前的旧inflight残余内存记unknown，不据active=false宣称实际缓存清空。逻辑编码字节不是实际堆分配，重复引用的逻辑编码与对象去重计数分列。JS heap、WASM线性内存、renderer/GPU和owned进程RSS各自采集或unknown，不求和冒充精确总量。长期工具流式写有界数字收据，不积累全DTO/大trace；预热后初末同负载同阶段比较，不在末端GC、清缓存或reload制造下降。30分钟/M3/16GiB结果仍不代签正式A/B或P5/P6。 可用本地实例/范围序号、当前query匹配和已知source_cut比对布尔校验观测阶段，序号不是权威epoch；便宜stamp不遍历DTO或触发新query。heap/DOM读取前后应括在相同阶段：同scope/cut正常轮询跨读则原值保留并标unknown，不重新抽样挑低值，初末窗口有该项不得只取已知子集求中位数；实例/范围/页面、身份或实际负载丢失则停止。逻辑编码/对象/保留容量只作趋势，已知内存分量超过既有10%须保存raw后非零，缺项不能签整体预算。

实测失败先报告/修复或缩小支持范围；改预算须同fixture证据和owner决定，不临时放宽。方法见[专项性能验证](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-performance)。这些不是新通用CI gate，文档PR不需GPU/付费模型。

<a id="view-local-feedback-measurement"></a>
### 9.3 本地接受与选择确认的测量

本节解释9.2既有本地UI预算，不改阈值或授权范围。上游是 [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001) / AC（Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251）；[5.1](#view-query)的待检查选择仍沿 O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf。通用发行的[连续交互预算](viewer-gameplay-release-experience-overhaul.design.md#viewer-performance-verification)与本专项按profile和端点分别报告，不用通用预算替代WorldView预算。

当前观测切片仅定义只读选择。退出跟随及stop继续保留9.2预算，但分别沿原导航和原operation身份单独设计验证；选择pending不代表stop已接受或世界已停止。

| 端点 | 可表达的事实 | 判据与报告边界 |
| --- | --- | --- |
| 本地接受状态 | 对本次输入显示pending、已选择或明确不可用。 | 只是DOM状态接受时间；单独不足以证明反馈可见或达到9.2预算。 |
| 首个本地可见响应（ACK） | 同一输入意图已被UI处理，玩家获得可辨的等待、选择或拒绝反馈。 | 9.2的本地UI接受预算适用于此列；结果类型分别报告，拒绝不写成选择成功。必须有匹配身份与可见帧证据。 |
| 选择确认 | 对象由当前获准读模型重新证明并成为实际选择。 | 继续保留原strict selected的DOM、同目标EventTiming及额外rAF列、阈值和unknown；pending不算selected，不能把ACK列的通过复制到本列。 |
| 网络/世界完成 | 后页、真实操作/回执或Runtime结果完成。 | 沿原请求/主体/操作身份另报，不把UI接受当作完成或改写原结果。 |

只从下一轮事先注册的测量计划使用ACK列；旧样本和旧strict selected结论原样保留，不能事后挑较快的端点重算。ACK合格与strict selected unknown可以并列，不由此宣布P6整体通过。所有计划输入保留，未送达、错误目标、关联缺失、时钟/可见性未知分别记录，不补零、不静默丢弃。

ACK单项结论不改变既有selected runner的退出码；若新探针只验ACK，必须显式声明组件scope，不能冒充完整P6 gate。已经选中的对象被再次点击也不能因状态原已为true记零时延；该类幂等输入要么在计划中单列处理证明，要么明确未测。

ACK必须绑定实际可信输入的typed ref、world/当前scope代及显式用户选择代际，沿同一page/timeOrigin。分页请求代不能充当用户意图代；新选择、导航、换scope或卸载终止旧关联。若需补只读观测字段，只能由真实输入/状态所有者发布本地关联，不接受工具写入的token，不保留旧实体/坐标/权限事实，不新增读取、pin、grant或世界命令；不输出私有authority key。现有泛pending文案或role=status本身不提供此完整证据。

可见响应必须相对输入前有可辨的反馈变化。新的局部代号或DOM属性、仍在显示的旧pending徽标、无变化的Selected旧对象均不能单独给新输入记成功。连续两个pending意图若只有不可见metadata变化而画面相同，后一输入的可见ACK为unknown；需要新的目标提示时作为后续独立UI实现，不用测试字段伪造可见变化。反馈的目标来源仍限于5.1已呈现证明。

观测分两步。先以冻结候选实际shell核对反馈状态、关联、文本范围、viewport/clip、祖先display/visibility/opacity及遮挡；另在非计时校准中以实际字形正控、A/A稳定对照和透明/遮挡负控固定字形区域、噪声与覆盖判据，不能拿背景变化或一个变化像素证明语义可见。计时路径不复制状态节点、不改文本/样式、不隐藏目标或产品HUD、不暂停rAF/renderer、不强制绘制。

随后只做有界可见帧能力核对：保存当前浏览器协议/trace字段和实际合成图像，用可核验的同帧关系及前后时钟锚将图像时间限定到输入的performance时基。不能把Paint、DrawFrame、截图API返回或rAF自动等同物理呈现；未验证图像/正常产生的帧/时钟关联时，可见时延保持unknown。图像证据还须落在同意图、同scope反馈状态未改变的完整时间区间；区间跨意图或不同scope不借用。采集开销单列，不从时延扣除，trace诊断不自动代签无trace预算窗口。

稀疏图像只能给出已经可见的上界，首个捕获帧不自动是首次可见帧；缺帧区间不能用来推迟下界或证明此前没有响应。时钟或覆盖不足的下界保留unknown。

对新ACK列保存逐样本可见时间上下界及未知原因；仅在全部必需样本可关联、可见且P95上界及连续无响应情况满足9.2时报告该列合格。P95下界已超过预算才称已证超标；区间跨阈值或任一样本未知均不签通过。原strict selected的判法不因本规则改变；EventTiming target=null仍按原判据unknown，禁止仅凭时间戳补目标。具体短场景和实施前置见[专项手册](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-local-feedback-measurement)。

## 10. 兼容、迁移与回滚

先修当前HUD主体/时效并补Rust/host负例；再接一致snapshot/task overlay、partial/分页/generation；之后扩空间查询、聚合、探索/性能；最后同build原生容器逐OS验收。保留旧有界snapshot时明确范围，不同时拥有两套世界真值。

回滚只用满足主体/来源/权限的兼容Viewer或文本降级，不改世界历史、不丢pending、不恢复旧授权、不切debug世界/旧executor。不支持projection major时blocked/升级，旧构建不能正确表达时效/原操作时仅文本。P1-A/B/C前置与Feed/receipt隔离不变。

## 11. 验证设计与可追溯性

普通CI测精度/类型、来源、partial/删除、乱序/撤权/selection/原主体与观察无副作用；fake Context不调用模型。实际S6/GPU/native/真人另验。保留[原组合手册](../../testing/manual/external-agent-runtime-contract-validation.manual.md)全部viewer-*义务，专项手册补可重复判据，不取消历史AC。

### 11.1 验证映射表

| 上游 requirement / AC | 本设计条款 | 独立 obligation 与条件 | 验证方法 / source / scenario / environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251；[REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001) | [本地反馈测量](#view-local-feedback-measurement) | 可见ACK、selected确认及网络完成分列，原预算/unknown不改 | [PW-PERFORMANCE](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-performance)与事先注册的短关联/可见帧能力核对 | 原始输入/反馈代际/图像与时钟区间、旧样本独立保留 | 设计/DOM属性不代签可见帧、P5或P6 |
| [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001) | [共享舞台](#view-scene) | 首读/原归属和ambient分离。 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)及PW-READABILITY，Rust/host与真实S6同候选。 | 原操作/DTO/图像/回答分层记录。 | DOM/模型不证明真人理解。 |
| [AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002) | [观察到生效](#view-admission) | 来源/冲突/过期/撤权与原结果恢复。 | [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)，确定性竞争+真实API/Runtime临时世界。 | 显示cut、原operation、权威原因和前后结果。 | fake authority不证明接线。 |
| [AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003) | [探索查询](#view-query) | partial/cut/空间适配、完整旅程与密度质量。 | [PW-PROJECTION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-projection)、PW-EXPLORATION、PW-PERFORMANCE，固定世界/build/设备。 | 页集合、删除/淘汰、交互及实测帧/内存。 | helper阈值不证明像素/容量。 |
| [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004) | [原生与逐端合同](#view-native) | 各端自身交互与失效恢复。 | [PW-PLATFORM](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-platform)和PW-RECOVERY，实际release/WebView/OS。 | 资产hash、操作、原结果、恢复前后。 | 另一端/无GPU不代签正常ready。 |
| [AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005) | [指导边界](#view-guidance) | 观察无副作用、合法指导/原结果。 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)、PW-FRESHNESS，双主体/原操作临时世界。 | 请求、前后root、原receipt/权限。 | UI disabled不证明授权。 |
| [AC-EXT-015](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-015) | [原生与逐端合同](#view-native) | 两表面实际同世界交互。 | [viewer-dual](../../testing/manual/external-agent-runtime-contract-validation.manual.md#viewer-dual)，真实候选/平台。 | 双端操作与权威结果。 | 不单独证明模型/完整首局。 |
| [AC-EXT-016](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-016) | [探索查询](#view-query) | 概览至局部再返回任务。 | [viewer-explore](../../testing/manual/external-agent-runtime-contract-validation.manual.md#viewer-explore)，实际授权世界。 | 对象、来源、定位与返回。 | 不证明全图可见。 |
| [AC-EXT-017](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-017) | [指导边界](#view-guidance) | 世界现场合法指导/回执。 | [viewer-guidance](../../testing/manual/external-agent-runtime-contract-validation.manual.md#viewer-guidance)，实际owner操作。 | 原operation/receipt关联。 | accepted/animation不代签效果。 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | [view-runtime-results](#view-runtime-results) | 原结果、原主体/目标及实际 Runtime 接收分别可读 | [adapter-result-feedback](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-result-feedback)；RF01/RF02/RF07/RF08；双主体与 late/unknown/失权，逐表面核文本和来源 | 同候选来源/当前权限、文本/真实交互和原请求结果分层记录；静态审读不代签实际界面 | 不把收到报告或用户看到当模型理解 |
| Trace refs: O7-REQ-c5c90e09-ae74-4d77-9858-a4769562b01e；[AC-EXT-007](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-007) | [view-runtime-results](#view-runtime-results) | 四层事实各有来源/时效，回访不复活或重做执行 | [adapter-liveness](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-liveness)；LV01–LV06；声明 viewer/pure_api，Web/native 独立环境 | 同候选来源/当前权限、文本/真实交互和原请求结果分层记录；静态审读不代签实际界面 | 不以页面或健康检查代签独立 Runtime |
| Trace refs: O7-REQ-cb69c2a7-f13a-42ea-a929-214fa7ff9d43；[AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | [view-runtime-results](#view-runtime-results) | 外部用量与世界投入/占用按原来源显示，当前读权失效清私有投影 | [adapter-budget](../../testing/manual/external-agent-runtime-contract-validation.manual.md#adapter-budget)；BG01–BG06；原成本去重、未知和执行/读取资格各自变化 | 同候选来源/当前权限、文本/真实交互和原请求结果分层记录；静态审读不代签实际界面 | 不另算事实总账或承诺完整账单 |
| Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；[AC-EXT-017](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-017) | [view-recovery](#view-recovery) | renderer 失败保留获准文本/原操作，安全恢复不重放指导或效果 | [viewer-guidance](../../testing/manual/external-agent-runtime-contract-validation.manual.md#viewer-guidance)；boot/context-loss/断连、显式重试、换主体/撤权与迟到响应；正式 Web/native 分开 | 同候选来源/当前权限、文本/真实交互和原请求结果分层记录；静态审读不代签实际界面 | 不从 renderer 恢复推导后端/Runtime/另一端就绪 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[AC-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) | [view-intent-results](#view-intent-results) | 原请求真实处置、未知与未支持分别显示，导航和重连无新效果 | [api-intent-results](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-intent-results) IR01–03/06；同候选实际 Web/native 与原 API，双主体/双序/重启/旧响应 | 原结果 proof/当前读权、各表面文本和关系导航、零隐式重提/取消记录 | 目标未实施；API fixture 不代签真实界面，关系显示不证明底层仲裁 |
| Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；[AC-DWE-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003) | [view-intent-results](#view-intent-results) | 原成员/前驱/胜者的真实关系和当前读权分别成立，关联导航无新效果 | [api-intent-results](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-intent-results) IR02–06；同候选实际 Web/native 与原 API，双主体/双序/重启/旧响应 | 原结果 proof/当前读权、各表面文本和关系导航、零隐式重提/取消记录 | 目标未实施；API fixture 不代签真实界面，关系显示不证明底层仲裁 |

补充验证 [5.6](#public-feed-history-source)：固定同一 world/epoch，比较历史与实时公共事件身份、顺序和允许字段；以历史不可用、真实空窗口、分页未完及换源测试不同展示；用合成 canary 检查匿名/A/B 的公共 DTO 与错误均无私有内容。重连覆盖旧 socket 迟到、新 hello 后首读失败/重试、当前首 snapshot 接纳和 scope 变化，断言 stale 到 ready 的条件及零副作用重发。此映射尚待确定性回归与真实 Viewer 同候选验证，不证明实现完成。

## 12. 决策、长期风险与未决问题

ADR-VIEW-1：共享Viewer/读模型，native受限系统WebView，代价是逐平台资产与图形验证。

ADR-VIEW-2：观察与owner指导分开，服务器裁决，缺来源/权限诚实降级。

ADR-VIEW-3：几何、时效、请求代际、世界切点分开；partial不是全量，显示默认值不是业务主体。增加适配复杂度换取权限/因果连续性。

ADR-VIEW-4：保留renderer/Shell，按预算做局部索引/聚合/reconcile，不先重写引擎/shader。

待实施锁定：平台依赖/发行身份、API Context/fence真实接线、空间索引来源及实测质量。这些是交付缺口，不可跳过；独立跨域评审和运行证据在实际PR记录，不由本文自报完成。
