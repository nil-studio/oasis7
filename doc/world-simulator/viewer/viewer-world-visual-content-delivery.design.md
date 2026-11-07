# Viewer 世界视觉内容与交付设计

- 主责：`game_visual_interaction_designer`；所属模块为[智能体、世界模拟与交互](../../product/agents-world-simulation/prd.md)，专业入口为 [Viewer](README.md)。`viewer_engineer` 维护编辑源、构建与消费者；`qa_engineer` 承接分层验证。
- 生命周期：`active`。适用于当前 Viewer Web 程序化世界图形的内容制作及本轮像素设计源交接，不表示实际候选、来源权利或视觉品质已经验收。
- 审读基线：`d21019414c46d7e8ba61ab2c59b1f217b144ba88` 的源码、专业设计与仓库素材。除 §4.1.1 明示新增设计目标外，下文“当前”均指该基线的静态观察；实际消费版本和运行结果留在原 PR、构建与证据记录。
- 上游：[世界首读与因果](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)、[来源与未知](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)、[游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)。具体 Trace refs 见 §2.1。

## 1. 问题、目标与范围

当前世界画面中的地形材质、事件符号、模块标记与设施状态主要由 Rust 程序生成。它们已有可编辑内容和运行消费者，但仅引用渲染源码或模型制作工具，不能说明制作方应交付哪些变体、哪些值表达事实、怎样关联生成物与来源。本专题把这些现有内容整理成可直接制作、替换和验收的交付规格。

玩家应能区分组成语境、事件类别、对象类别与设施状态，并继续通过原有文本和上下文读取完整含义。视觉内容不能凭颜色、装饰或活动增加资源、路线、控制权或已结算成果。本设计补齐 GD-09／GD-18 在上述程序化内容范围内的内容目录、制作边界、来源和交接，不宣称覆盖全游戏美术、动画、特效或资产权利。

阅读顺序、现有尺寸、图层、选择／回执标记、LOD、动效与 B01～B13 场景复用[Bevy 详细视觉交接](viewer-pixel-world-player-readable-rendering.design.md#bevy-detailed-visual-handoff)；品牌 token 复用[品牌系统](viewer-brand-system-2026-06-05.design.md#3-token-taxonomy)。新增像素方向由[视觉规范](viewer-visual-design-spec-2026-06-05.design.md#pixel-world-art-direction)拥有，本文只登记共享设计源及消费边界，不另建性能规范，不改变 Shell、DTO、渲染器、世界规则或权限，也不重写首局页面、译文或帮助内容。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 具体视觉变体有可读类别与状态，不以装饰冒充原行动结果 | [内容与变体](#visual-content-catalog) | 视觉 owner、Rust 投影与原世界状态 | 承接四类程序化内容；不证明完整首读、因果或真人理解 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | 像素设计源可复用，图形及上层状态分别交接 | [像素设计源](#pixel-world-design-assets) | 视觉 owner、后续 Bevy 实施 | 新增设计参考不证明生产素材已接入、完整首读或真人理解 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | 未知类别、缺失输入与显示细节保留真实事实边界 | [事实与未知](#visual-fact-boundaries) | Runtime 来源、授权投影、原降级合同 | 不新增新鲜度机制，不由外观核实权限或世界结果 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 可定位内容编辑源、产物、消费者、来源与权利资料 | [源与交付](#content-source-delivery) | 内容制作方、Viewer 构建 owner、依赖维护方 | 本行产品层 N/A；理由：制作信息不新增玩家功能；范围：本专题内容来源与交付定位。其他玩家义务仍适用。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 同一候选能关联实际消费的内容版本与环境 | [候选组合](#visual-content-candidate) | 原构建、资源清单和实际加载记录 | 本行产品层 N/A；理由：候选证据不新增玩家功能；范围：本专题消费关系。不新增 manifest 或发布许可。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 变体修改、来源替换与依赖变化有具体交接和重验去向 | [制作交接](#content-change-handoff) | 视觉制作、Viewer 集成、QA；新内容键由原专业 owner 决定 | 本行产品层 N/A；理由：生产协作不新增玩家功能；范围：本专题变更传播。方法存在不证明批量产能。 |

### 2.2 已有专业合同与本设计的依赖

| 依赖 | 唯一主责及本专题使用方式 |
| --- | --- |
| 视觉语言与品牌 | [视觉语言](viewer-visual-design-spec-2026-06-05.design.md#7-visual-language)和[资产语言](viewer-brand-system-2026-06-05.design.md#5-asset-language)拥有工业风格、语义色和文字等价表达。下文 RGB 是源码观察，不是新增品牌 palette，也不证明 Rust 已消费 CSS token。 |
| 几何、LOD、密度与动效 | [图层与标记](viewer-pixel-world-player-readable-rendering.design.md#bevy-layer-mark-reference)、[密度](viewer-pixel-world-player-readable-rendering.design.md#bevy-density-handoff)及原 §13.4 拥有当前参数和交接边界；本专题只说明内容单元如何使用它们。 |
| 模块数据与设施事实 | [模块实体合同](viewer-module-visual-entities.prd.md#module-visual-data-contract)拥有 kind、label 和 anchor；设施输入来自已有 Rust RenderState。内容制作不能新增业务状态或把视觉 fallback 写回世界。 |
| 构建与恢复 | [生成物单一来源](viewer-web-build-truth.prd.md#42-generated-artifact-single-source-of-truth)、[脚本与 bundle](viewer-web-build-truth.prd.md#43-script-and-bundle-contract)拥有输出方式；[恢复合同](viewer-pixel-world-player-readable-rendering.design.md#bevy-recovery-handoff)拥有图形失效后的合法文本和重试。 |

## 3. 内容与表现设计条款

<a id="visual-content-catalog"></a>
### 3.1 当前内容族与具体变体

本节是一组可维护内容，不为每个素材增加要求 ID。表内形状与颜色是源码已有样本；“表达与制作约束”规定如何使用和修改这些样本。实际像素、可辨识度和状态理解仍按 §11 验证。

#### 地形组成材质

编辑源为 [host_state.rs](../../../crates/pixel_world_bridge/src/host_state.rs) 的 `FRAGMENT_TERRAIN_PALETTE`、`dominant_compound`，表现源为 [render_fragment_visuals.rs](../../../crates/pixel_world_bridge/src/render_fragment_visuals.rs)。

| 组成键 | 当前 RGB | 内容用途 |
| --- | --- | --- |
| `silicate_matrix` | 126, 144, 99 | 硅酸盐基底组成提示 |
| `iron_nickel_alloy` | 176, 184, 196 | 铁镍合金组成提示 |
| `water_ice` | 125, 211, 252 | 水冰组成提示 |
| `hydrated_mineral` | 96, 165, 250 | 含水矿物组成提示 |
| `carbonaceous_organic` | 120, 113, 108 | 碳质有机组成提示 |
| `sulfide_ore` | 202, 138, 4 | 硫化矿组成提示 |
| `rare_earth_oxide` | 167, 139, 250 | 稀土氧化物组成提示 |
| `uranium_bearing_ore` | 132, 204, 22 | 含铀矿组成提示 |
| `thorium_bearing_ore` | 244, 114, 182 | 含钍矿组成提示 |
| `unknown`／未匹配键 | 148, 163, 184 | 未知或尚未映射组成的中性外观 |

当前从正值 `compounds.ppm` 选择占比最高的键，同值按键稳定排序；没有有效项时使用 unknown。它不表示完整矿物配方、纯度分级、可采数量或危害等级。中文用途说明是本专题释义，不新增运行字符串。

材质由 shadow、base、inset、fleck 组合；inset 变暗，fleck 提亮。当前 inset 形状只按组成键字节和的奇偶区分，不能宣称十类均有独特纹理。近似颜色和相同细节仍依赖原有名称／详情；不把材质颜色作为唯一识别渠道。制作时保持背景地位、保留未知色和原 LOD，不为强调稀有度而压过 Agent、目标或回执。

#### 热点事件符号

编辑源为 [render_hotspot_cues.rs](../../../crates/pixel_world_bridge/src/render_hotspot_cues.rs)、[render_hotspot_core.rs](../../../crates/pixel_world_bridge/src/render_hotspot_core.rs)，由 [render.rs](../../../crates/pixel_world_bridge/src/render.rs) 组合共享 diamond、核心与类别细节。

| kind／回退 | 当前类别细节 | 表达与制作约束 |
| --- | --- | --- |
| `blocker` | 两条交叉线 | 表示已发布的阻塞；活动或脉冲不表示已解决。 |
| `goal` | 上方与右侧角标 | 表示已发布的目标，不表示已完成。 |
| `resource_transfer` | 向右的折线符号 | 表示转运类事件；符号朝向不是世界路线方向，不随此符号推导两端、移动或交付。 |
| `build_queue` | 两条不等长堆叠横线 | 表示排队建造类事件，不表示施工百分比或建成。 |
| 其他 kind | 单短刻线 | 保留通用近期事件提示；未制作专用细节时不借用已知类别的含义。 |

类别细节是非交互几何，保留原 hotspot 身份与命中区域；neutral core、highlight 和 shadow 提供可读结构，不承担新的状态。类别的文字解释沿原上下文，不把这一组符号称为完整动画或 VFX 库。

#### 模块实体类别标记

编辑源为 [render_module_visual_entities.rs](../../../crates/pixel_world_bridge/src/render_module_visual_entities.rs) 的 `module_identity_chip_specs`。统一 base marker 与现有 label／anchor 合同继续有效。

| kind | 当前中性细节 | 修改时保留的边界 |
| --- | --- | --- |
| `beacon` | 竖短杆 | 只区分类别，不表示信号已接通或可操作。 |
| `relay` | 横短杆 | 不把相邻模块自动连成有效中继关系。 |
| `artifact` | 中心点 | 不由外观推导价值、可拾取或所有权。 |
| `sensor` | 两个错位点 | 不表示已探测到隐藏对象或扩大观察权限。 |
| 未知 kind | 只有统一 base marker | 无专用细节仍保留已获准的对象身份与原文字路径，不伪造专用能力。 |

新增细节须使用已由原模块合同发布的 kind；新数据类别、anchor 或交互需求回到其专业 owner。细节不新增点击区。共锚点偏移与 label 预算仍由现有 renderer 处理，制作方不能移动世界坐标来避让图形。

#### Micro Depot 设施状态与附属读数

编辑源为 [render_micro_depot_facilities.rs](../../../crates/pixel_world_bridge/src/render_micro_depot_facilities.rs) 的 `facility_glyph`、`stock_runway_ratio`、`stock_throughput_text`。

| 状态／输入 | 当前内容 | 表达与制作约束 |
| --- | --- | --- |
| `active` | 绿色正方形＋横向细节 | 设施状态为 active，不直接证明库存、吞吐或当前可执行交易。 |
| `suspended` | 琥珀菱形＋方形细节 | 暂停；具体原因由原事实解释，不从颜色猜原因。 |
| `depleted` | 红色菱形＋斜线细节 | 耗尽；不能解释为设施或历史成果已删除。 |
| 未知 status | 当前函数不产生设施 glyph | 不回退成 active；不得由未绘制推定对象不存在，合法文本按原入口承接。 |
| 正 throughput limit | 四段 runway 和 remaining／limit 文字 | 主要表达吞吐剩余比例；不是通用库存百分比。remaining 的显示值限制在 0～limit。 |
| 库存 map 非空、正库存总量为零，且 limit 为正 | runway 为零并出现交叉形 | 这是已知零库存的显示保护；吞吐文字仍按自己的字段读取，不能改写成库存数量。 |
| limit 缺失／非正 | 无 runway 与吞吐读数 | 保留未知，不制造百分比；不抹去仍有效的设施状态。 |

有效正服务半径的范围圈、runway、文字底板均为被动内容，不新增命中区或扩展设施实际服务资格。更换设施外观时一起核对附属内容清理，避免状态消失而范围圈或文字底板留在画面。

Agent、Location、selection、receipt、recommended target 及 derived／missing position 标记直接使用[原标记表](viewer-pixel-world-player-readable-rendering.design.md#bevy-layer-mark-reference)，不在此复制制作参数。现有图稿只作原交接的参照；本专题不以图稿数量代替上述具体变体。

<a id="visual-fact-boundaries"></a>
### 3.2 事实与未知的内容处理

| 情况 | 内容处置 | 不能据此宣称 |
| --- | --- | --- |
| 已知组成但未有专用 palette／纹理 | 保留原组成身份，使用当前中性 fallback；缺少独特美术表达作为内容边界交接 | 未知世界资源、资源不存在或十种组成已可仅凭纹理分辨 |
| 新热点 kind 或未知模块 kind | 分别保留通用事件细节或统一模块 base；需要专用表达时回到内容族增加变体 | 借另一个已知 kind 的语义使其看起来完整 |
| 未知设施 status | 记录当前不绘 glyph 的行为和合法文本依赖；若目标入口无可读解释，保留待验证／待实现缺口 | 未绘制即空世界，或用 active 颜色代填未知 |
| 缺可信位置、关系来源或有效观察 | 按[原来源与恢复](viewer-pixel-world-player-readable-rendering.design.md#display-provenance)保留可披露内容，抑制误导性图形 | 图形刚刷新即事实当前，邻近即关系或控制权 |
| 只有活动、排队或事件效果 | 只表达实际类别／阶段，原操作结果仍读原回执 | 玩家成功、已交付或新收益 |

源代码中的固定颜色、形状和动画只是表现输入。原授权、来源、时效、主体与结算分别核对，内容制作不从图形反推它们。缺少正式素材、真实目标环境或专业审阅时记录具体缺口，不能把验证改为 N/A。

## 4. 制作规格与交付物

<a id="content-source-delivery"></a>
### 4.1 编辑源、生成物与实际消费者

| 内容类型 | 可编辑源与依赖 | 生成／导出与消费者 | 当前可用边界 |
| --- | --- | --- | --- |
| 四类程序化世界内容 | §3 的 Rust 源；[crate 配置](../../../crates/pixel_world_bridge/Cargo.toml)、[依赖锁定](../../../Cargo.lock)；同候选授权 RenderState 输入 | Rust 编译为 WASM，wasm-bindgen 生成 bindings；[finalize](../../../crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs)输出 `dist/pixel-world-bridge/webgl2/`；[WASM loader](../../../crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.js)与[host](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx)消费 | 有实际可编辑实现；本轮未构建或证明该版本已在部署候选加载。没有独立贴图／模型编辑源是其程序化实现形式，不免除内容验证。 |
| 相邻 Shell 样式与文字标记 | [viewer.html](../../../crates/oasis7_viewer/viewer.html)的 CSS token／font stack、[terminal CSS](../../../crates/oasis7_viewer/viewer_terminal_shell.css)、原 `software_safe_src/` | Vite／finalize 生成 canonical `viewer.js` 与 dist 页面／compat aliases；世界图形仍由 WASM 绘制 | 是本专题组合依赖，不在此重新制作 Shell、译文或图标库。 |
| Bevy 文本与系统字体依赖 | crate 启用 Bevy `default_font`；Shell 声明 body／display／mono 字体栈 | Bevy 构建与浏览器字体选择各自消费；实际随包文件、字体 fallback 和字形需看候选 | 本轮只核到依赖与字体名，未核完整字体文件、notice 或真实 CJK 渲染。 |
| 六视图机器人制作原型 | [原工具合同](../../../tools/voxel_asset_pipeline/DESIGN.md#asset-contract)、[制作说明](../../../tools/voxel_asset_pipeline/README.md)、`six_view_to_voxel_model.py`及其 requirements | 设计输出 voxel JSON.gz、OBJ／MTL 与评估图；当前审读没有定位到所选正式源图、固定成品或上述 Viewer 对这些成品的加载链 | 工具和合成测试存在；不是正式机器人交付。结论只限该工具到当前 Viewer 的链路，不推导全游戏没有模型或动画。 |

程序化内容的编辑真值是 Rust；无需为它虚构 PSD、Blend 或动画剪辑。生成 WASM、bindings、dist 和 canonical generated bundle 不作为手工编辑源。实际制作复用原[构建交付设计](viewer-web-build-truth.design.md#dist-copy-contract)；源码、生成物与截图各自保留版本身份。

六视图输入顺序、1 cm 体素、默认 200 cm 尺度和坐标合同全部引用原工具。预览／评估产物继续按工具约定留在本地，不因本专题新增提交义务。开发说明中的 Image 2 robot sample 没有可取得的固定源图、权利材料与正式输出时，保留“未核实正式资产”；不能用测试生成的长方体 fixture 补成机器人样本。

<a id="pixel-world-design-assets"></a>
#### 4.1.1 本轮像素设计源与消费边界

#5527 / #5543 新增的二维俯视像素设计沿[视觉方向](viewer-visual-design-spec-2026-06-05.design.md#pixel-world-art-direction)，不更新上述生产源码审读基线。可编辑原件为 [atlas.svg](assets/ui-ux/pixel-world/atlas.svg) 与 [scene.svg](assets/ui-ux/pixel-world/scene.svg)：场景 `320×180` 逻辑像素、地块 `16×16`、Agent `16×16` 且局部脚点 `(8,15)`；Smelter `48×48`、场景左上角 `(144,64)`。两份原件共用固定 16 色与整数矩形轮廓，具体颜色及图形键由原件 metadata 拥有；它们不是新增品牌 token、Rust 材质常量或世界单位。地块、道路、建筑布局与脚点均为设计坐标，不表示合法世界位置、关系、可建设 tile 或点击执行能力。

`atlas.svg` 拥有图形和色板；`scene.svg` 拥有 `<use>` 实例布局，其内联符号及 metadata 由 [build.py](assets/ui-ux/pixel-world/build.py) 同步。该脚本从同源整数矩形导出 Agent、ore、smelter、depot 的独立 PNG；`--sprites-only` 只需 Python 3 与 Pillow，完整 scene／页面稿导出仍需 Inkscape。既有 Agent 导出在相同源下保持字节一致。该脚本导出 [scene.png](assets/ui-ux/pixel-world/scene.png) 与 [agent.png](assets/ui-ux/pixel-world/agent.png)，供[离线交互原型](assets/ui-ux/interactive/index.html)加载；同时将同一源的整数几何嵌入图 16、17 与工业方案／结果四张 SVG 的 `pixel-world:*` 标记区，使这些[页面稿](viewer-page-module-design-2026-06-18.design.md#uiux-design-sheets)可以独立查看。不要手工编辑生成区或在各稿另画一套世界。场景底图不包含 Agent、姓名、权限、库存、任务状态或选择标记；Agent 按各 fixture 单独放置，HUD／中文／原回执保持独立。源文件、导出成功与设计操作均不证明正式 Viewer 已加载这些资产。

| 图形键／层 | 原始尺寸与局部锚点 | 当前设计用途 |
| --- | --- | --- |
| `ground`、`ground-alt`、`grass`、`path` | 16×16；左上角 `(0,0)` | 安静地面及正交道路；道路不声明获准移动、分配关系或建设能力 |
| `ore`、`rock`、`tree`、`scrub` | 16×16；左上角 `(0,0)` | 环境外观，不从矿床图形推导组成、库存或可用输入 |
| `smelter` | 48×48；中心 `(24,24)`、入口 `(24,48)` | 场景实例 `(144,64)`；金色为外壳涂装，不表示开工或成功 |
| `depot` | 32×32；中心 `(16,16)`、入口 `(16,32)` | 场景实例 `(96,64)`；不编码 Micro Depot 业务状态 |
| `agent` | 16×16；脚点 `(8,15)` | 独立静态精灵；身份／选择／权限由上层文字和边框表达，同位候选仍用集合标记 |

在仓库根执行 `python3 doc/world-simulator/viewer/assets/ui-ux/pixel-world/build.py` 重建；需要 Python 3、Pillow 与 PATH 中的 Inkscape。导出在原始逻辑尺寸完成，颜色和透明度经过脚本检查；不把旧平滑插画缩成马赛克。更改色板／图形时编辑 atlas，再重新生成；更改设施布局时编辑 scene 的实例并同步消费者锚点。尺寸、Smelter 相机中心和 Agent 脚点属于本次固定规格，若改变还须同步生成脚本与原型布局常量；不会由 SVG metadata 自动改变交互相机。

**原型显示规则。** 以 `.world-art` 实际整数 CSS 尺寸 `W×H` 计算 `s=max(1,ceil(max(W/320,H/180)))`；图像为 `320s×180s`，围绕 `(168,88)` 居中后将整数偏移夹到 `[W−320s,0]`、`[H−180s,0]`，超出舞台的部分裁切。Agent 脚点与设施标签使用同一偏移和倍率，标签字号不乘 `s`。`ResizeObserver` 监听舞台，包含窄屏文档流和文字 200% 的高度变化；没有新增玩家相机或缩放操作。四张静态稿按其未被侧面板占据的世界区域构图：图 16 为 484×390、3×，图 17 为 1048×900、5×，工业稿为 944×844、5×；原型面板是覆盖层，按完整舞台裁切。这些是设计组合范围，不能把静态稿裁切当成已实现的响应式结果。源像素与整数 CSS 缩放也不证明任意 DPR、浏览器 zoom 或 GPU 采样均已逐物理像素对齐。

生产仍沿 [Rust/Bevy `Camera2d`](../../../crates/pixel_world_bridge/src/lib.rs)／WASM；[canonical canvas](../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx)已有 [CSS pixelated／crisp-edges](../../../crates/oasis7_viewer/viewer.html)。固定低分辨率 RenderTarget、纹理 nearest、整数倍缩放、camera pixel snap 与这些设计资产的 loader 均是后续接入和验证目标，本轮不宣称已经实现。继续复用原程序化图元和[恢复边界](viewer-pixel-world-player-readable-rendering.design.md#bevy-recovery-handoff)，不新增 JS 世界 fallback 或 3D 模型生产路线；六视图 voxel 工具也不是本次素材消费者。

沿[内容源审读](viewer-world-visual-content-delivery.manual.md#visual-content-source-review)核对源尺寸、16 色、整数像素、导出一致性及 desktop／narrow 设计稿中的轮廓与独立文字；实际接入另按[候选组合](#visual-content-candidate)和[目标环境验证](viewer-world-visual-content-delivery.manual.md#visual-content-target-validation)记录实际像素、DPR、相机、加载与命中结果。静态稿可读性、浏览器原型和真实 Bevy／GPU 验证分别记录，未执行的层级保持未验证。

#### 来源、权利与可继续编辑的边界

| 来源组 | 当前可定位依据 | 交付时还需核对什么 |
| --- | --- | --- |
| 仓库程序化几何／材质／样式 | 原路径、固定源码修订；根 [LICENSE](../../../LICENSE)为 MIT、copyright `2026 scc`，pixel_world_bridge 的 crate 也声明 MIT | 原文件及贡献来源、适用 notice 随实际交付保留情况；仓库声明不是对所有外部输入权利的逐项证明。 |
| Bevy 默认字体与其他构建依赖 | crate features 与 Cargo.lock | 实际打包资源及其许可／署名资料由依赖与发行维护方核实；本轮不从依赖名推定字体资料齐备。 |
| Shell 字体栈 | viewer.html 声明字体名称和 fallback | 区分系统解析与随包再分发；仅有字体名不能当作已交付字体文件或取得再分发依据。 |
| 六视图或其他未来外部／生成素材 | 实际采用时的源图／编辑源、作者或生成来源、固定输出、许可／授权记录 | 采用的素材版本、允许用途、署名／复用限制、工具及关联输入；来源不明时不进入“权利已核实”的交付结论。 |

制作方说明源可取得性与编辑限制，消费方记录实际采用版本；必要资料按原权限与稳定存储入口关联，不在公开文件写凭据。本专题不新立许可，不将仓库 MIT 扩张为未核实外部素材的授权。

<a id="visual-content-candidate"></a>
### 4.2 绑定实际消费候选

程序化内容随同一 Rust／Viewer 构建进入候选，组合记录复用原[同来源构建与产物新鲜度](../../engineering/workflow/package-source-security.md#final-artifact-freshness)，不另建视觉 manifest。实施方在原构建／运行记录关联 source HEAD、构建 profile、实际 bridge／bundle 产物、输入 fixture 或授权投影版本、平台／viewport／DPR／语言／字体和动效偏好。

finalize 当前生成 `dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm`、对应 bindings 和 loader。正式打包可能通过 [copy-viewer-web-dist.sh](../../../scripts/copy-viewer-web-dist.sh)生成的 `optional-payloads.json` 引用拆分 payload；实际采用以该候选清单与 loader 的读取、字节数／hash 校验为准。开发入口的相邻 WASM 路径不代替正式交付记录，磁盘有同名文件也不证明浏览器加载了它。

内容仍未集成、只有源码、只有静态预览或只有合成测试时，分别写明事实。本设计和工具可执行不等于构建就绪、正式部署通过、视觉已验收或产能已建立。

<a id="content-change-handoff"></a>
### 4.3 制作、替换与变更传播

制作方先按 §3 选实际内容键和变体，保留原数据含义；跨类别改名、新增 kind／业务状态或改变权威事实先回原专业 owner。Viewer 工程维护生成物与集成，视觉 owner 核对表达，QA 按受影响范围分层验证。协作复用独立分支与文件写入边界；同一 palette 或 glyph 函数由明确写入者合并，不手改生成产物解决冲突。

| 代表性变更 | 一起交接的内容 | 传播与失败处理 |
| --- | --- | --- |
| 调整 `water_ice` 的现有材质表达 | 原组成键、改动前后视觉意图、palette／shading 源、unknown 对照和原 LOD 依赖 | 重建 bridge；核色彩运算、同色／未知与实际背景层级。若不能辨识或压过 Actor，回视觉／渲染源修正，不改资源事实或验收阈值。 |
| 调整 `resource_transfer` 类别细节 | 原 kind、折线表达、通用事件对照、原热点身份／命中区 | 核同类／通用变体、复用／清理与目标环境可读性；禁止为表现方向补造世界路线。 |
| 修改模块 chip 或 depot glyph | 现有类别／状态、未知回退、labels／附属读数／范围圈及实际源文件 | 核关联内容一起更新与删除；缺业务输入交回原 owner，不能用 active 或另一个 kind 代填。 |
| Rust／Bevy／wasm-bindgen／字体依赖变化 | 原锁定版本、受影响构建与实际字体／资源资料 | 重建受影响候选并复核像素／文字和加载；旧截图只能证明旧组合，不能仅因内容键未改就沿用通过结论。 |
| 采用、替换或退役外部模型／源图 | 可取得的固定源与输出、来源／许可、编辑限制、目标消费者 | 先补真实导入方案；同次处理引用／导出／署名／缓存与旧版本去向。当前六视图工具不能直接接到不存在的 loader。 |

详细操作与判据见[制作交接手册](viewer-world-visual-content-delivery.manual.md#visual-content-production-handoff)。上表是既有内容的制作方案，未声称发生了一次实际改色、导入或试产。

本范围规模是十个材质键／回退、五类热点细节／回退、四种模块 chip／中性回退及三种 depot 状态／未知处理，不据此推算生产速度。更丰富模型、动作动画、粒子特效、外包数量和品质／成本尚无本专题的采纳决定。需要扩大生产时由原产品与制作 owner 明确批次、复杂度、预算和实际试产证据；工具测试不能当作批量产能。

## 11. 验证与验收

文档审读、素材／源检查、构建与目标环境分别判断。原生产内容审读只核内容、真实来源与可执行方法，不提供新的运行候选、GPU 或游戏验证结果；§4.1.1 的像素源、导出及预览只证明各自设计层。未来实际变体、构建或来源变化按配套手册选择所需验证，不要求纯文档 PR 执行全部运行场景。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [内容与变体](#visual-content-catalog) | 四族已有变体与设计含义一致，关键区别不只靠颜色／动效 | [内容源审读 V1～V4](viewer-world-visual-content-delivery.manual.md#visual-content-source-review)；固定本次源码，实际视觉变化另按手册选择目标环境 | 当前 PR 的逐项静态核对；实际候选另存原证据 | 静态表和源码不证明真实 GPU、完整首读或真人理解 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616 | [像素设计源](#pixel-world-design-assets) | 像素稿主体与独立文字可读，设计消费者共用同一源 | [内容源审读](viewer-world-visual-content-delivery.manual.md#visual-content-source-review)及 §4.1.1，核源像素／导出一致性／视口；实际接入另按目标环境验证 | 当前 PR 的像素源、派生图层和设计预览 | 静态像素稿不证明真实 GPU、生产接入或真人理解 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)；Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3 | [事实与未知](#visual-fact-boundaries) | 未知／缺输入不被改写成世界事实或完成 | [内容源审读 V5](viewer-world-visual-content-delivery.manual.md#visual-content-source-review)；同候选源码／输入负例 | 原 PR 的预期、实际与缺口 | 不代签授权链、来源新鲜度或运行恢复 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [源与交付](#content-source-delivery) | 编辑源、导出、消费者及来源资料可追，未核实项不计入完成 | [内容源审读 V6](viewer-world-visual-content-delivery.manual.md#visual-content-source-review)；固定原文件与适用权利材料 | 源／产物／依赖定位与未核实记录 | 未取得的源图、第三方字体资料或外部授权保持缺口 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [候选组合](#visual-content-candidate) | 声称实际可用时能证明候选消费了所选内容 | [目标环境验证](viewer-world-visual-content-delivery.manual.md#visual-content-target-validation)；实际 bridge／bundle／payload 与环境 | 原构建、加载、截图及分层结果；未执行时保留计划去向 | 源码 HEAD、同名文件、hash 或构建单独通过不证明可读性／部署 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作交接](#content-change-handoff) | 代表性变更识别关联产物、消费者、检查与失败 owner | [制作交接步骤](viewer-world-visual-content-delivery.manual.md#visual-content-production-handoff)；文档审读使用 water_ice／resource_transfer 路径，真实变更使用实际输入 | 当前 PR 的路径审读；真实制作／返工另留原记录 | 不证明发生了试产、批量制作或规模化产能 |

## 12. 决策、风险与未决问题

采用程序化源作为现有四族的编辑真值，复用已有构建与专业验证。它允许维护当前可用内容，不要求先采购模型包；也不意味着未来模型、动作动画或特效义务不适用。

| 未完成事项 | 主责与解决触发 | 当前交付边界 |
| --- | --- | --- |
| 近似材质、未知状态、密集重叠与真实字体是否可读 | 视觉 owner／QA；实际内容或候选变化及阶段验收时执行原 B01～B13 相关场景 | 源码与方法存在；实际品质未由本 PR 证明。 |
| Bevy 默认字体及实际发行资源的完整来源／notice | 依赖维护方与发行 owner；候选资源打包、替换或发行核对时补证 | 本轮只确认依赖和仓库许可声明，保留未核实资料。 |
| 六视图机器人是否作为正式 Viewer 内容 | 产品／视觉／Viewer owner；确定用途、目标品质、源图与导入需求时决定 | 工具原型有合同，所选正式资产和加载链未由本专题确认。 |
| 更丰富模型、动作动画与效果的种类、数量、制作成本 | 产品与制作 owner；扩大该专业或对外承诺前定范围并试产 | 待范围决定，不以缺素材写 N/A，不宣称全 GD-09／GD-18 完成。 |

源码、图稿或素材存在只证明其实际范围。替换内容或依赖后保留旧证据的版本，不以新设计覆盖旧失败，也不把本专题的 active 状态作为发布或权利审核通过。
