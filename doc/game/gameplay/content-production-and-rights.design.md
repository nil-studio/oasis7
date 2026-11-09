# 游戏内容制作范围、来源与成本设计

- 主责：`producer_system_designer` 维护跨内容类型的制作范围与取舍；各内容作者、视觉设计、Viewer／Runtime 集成、发行维护者与 QA 维护自己的源、消费者和验收。
- 生命周期：`active`。本文是非代码内容的制作与交接设计，不是新的玩法、采购批准或发行授权。
- 源审读基线：`f4f8e1bb1be6cb9746ea767af3bdf71204e347e3`。下列数量是该版本的有界源盘点，实际候选、取得授权和生产成本分别取证。
- 上游：[游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)、[非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)与[规模化制作证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence)。操作方法见[配套手册](content-production-and-rights.manual.md#scope)。

## 1. 问题、目标与范围

首局、工业、场景和程序化图形已有各自的制作说明，但不同类型的“数量”“源文件”“交付完成”并不相同。一个配方可以出现在 native 与 WASM 两条路径中，一张 SVG 可以服务多个交互状态，一个字体名也可能来自系统解析、远程加载或随包嵌入。直接相加文件数，或把根许可证视为所有材料的来源证明，都不能支持生产计划。

本设计补齐 GD-18／GD-20 的共同制作决策：从已有内容入口确定本次生产单元；保留源、导出、权利和实际消费的关系；用可核查的人时、直接费用、依赖与返工计算规模。具体玩法、文案、图形、语言和素材规格仍由原专业条款拥有。本设计不复制全部资产状态，不替换原 manifest，也不新增统一资产服务、采购流程或审批链。

范围覆盖已在库的游戏规则／场景内容、Viewer 内容和图稿、Launcher 字体、官网／小说／传播材料，以及其共享制作依赖。工具样本、历史证据、尚未采用的素材和运行时生成内容分别处理，不能混入同一交付数量。玩法完整性、所有 WASM 模块的可玩内容审查、实际画面品质、完整来源授权与全游戏预算仍须在各自范围成立。

## 2. 上游约束与职责

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 区分内容单元、变体、源文件、导出和实际消费，不漏掉相邻入口 | [制作单元与范围](#production-scope) | 原内容目录、产品范围和实际消费者 | 产品层 N/A；理由：组织既有生产输入，不新增功能；范围：制作单元与来源定位。文件数量不证明游戏内容完整。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 来源、编辑版本、取得材料及复用／署名边界可定位 | [来源与权利](#source-and-rights) | 制作方、依赖维护者、发行维护者 | 产品层 N/A；理由：来源交接不新增玩家功能；范围：本专题来源与权利资料。未取得授权材料不由文档、Git 作者或根许可代签。 |
| [内容生产与交接](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 按内容类型交接可编辑源、规格、转换和实际采用结果 | [类型交接](#production-handoff) | 原内容作者、构建与导入 owner、QA | 产品层 N/A；理由：制作协作不新增玩家功能；范围：内容制作与接收交接。不新增加载器、构建协议或世界写入权限。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 内容替换后仍能核对源、导出、实际消费与受影响证据 | [变更与候选](#change-and-candidate) | 原构建／发行记录、资源清单与加载证明 | 产品层 N/A；理由：交付关联不新增玩家功能；范围：受影响内容与候选绑定。固定文件版本不证明部署、可玩性或已有世界迁移。 |
| [制作范围与维护](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 有明确数量单位、复杂度、角色能力、外包边界与范围取舍 | [生产选择与人员](#production-selection) | 产品与制作 owner、实际投入和报价范围 | 产品层 N/A；理由：生产安排不新增玩家功能；范围：已有内容的批次与角色输入。不决定未采纳内容，不承诺人数或工期。 |
| [规模化制作证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence) | 初次建立、重复制作、集成／验证、返工、维护和等待成本分开核算 | [成本与容量](#cost-and-capacity) | 原制作记录、适用样本和报价 | 产品层 N/A；理由：成本核算不新增玩家功能；范围：本专题制作成本与容量。假设算例不成为实测、市场价格或预算批准。 |

### 2.2 既有专业合同与技术边界

本专题只承接制作信息与证据组织，产品层适用性限于表中说明。玩家体验及规则继续沿各行实际内容的原 authority，不借制作 N/A 排除声音、可访问性、多人或持续世界的玩家义务。

运行内容的身份、加载、安全和恢复分别依赖[工业内容交接](industrial-native-content-delivery.design.md#content-change-handoff)、[WASM 输入闭包](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)、[场景来源与消费](../../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#provenance-and-candidate)、[Viewer 源与构建](../../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#content-source-delivery)及[官网发布与纠正](../../site/github-pages/public-site-discovery-validation.manual.md#内容发布与纠正)。本文不改这些技术合同；需要改变加载、持久状态或权限时，由原 owner 先补相应设计与消费者。

## 3. 制作范围、源与交付关系

<a id="production-scope"></a>
### 3.1 当前可复核的制作单元

以下是源审读基线的分类入口，不是另一份全仓资产状态表。新增或修改内容仍更新原目录／manifest；本文只维护计数口径和跨类型边界。按手册 [CP-SCOPE](content-production-and-rights.manual.md#cp-scope) 可以复核文件集合与目录数量。

| 内容类型与原入口 | 基线可核数量及单位 | 制作真值与实际交付边界 |
| --- | --- | --- |
| [首局内容](../../world-simulator/viewer/first-session-content-delivery.design.md#content-units) | C01～C09，共 9 个语义单元 | 一个单元包含正常、未知／失败和返回表达；不是 9 张图或 9 条已经采用的文案。消费仍绑定原 Next Move、Quote、Receipt 等事实。 |
| [native 工业目录](industrial-native-content-delivery.design.md#native-content-catalog) | 2 个 factory kind；11 个 recipe ID，分别为 4 个 Smelter 和 7 个 Assembler 配方 | 批次不另计配方；native 与同名 WASM 定义是相关实现来源，不能重复计为两套玩法内容。具体数量、成本和副产物仍读原 canonical plan。 |
| `crates/oasis7_builtin_wasm_modules/` 的 [builtin 源与消费合同](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-materialization-contract) | 38 个直属模块的 Cargo.toml：M1 10、M4 factory 3、M4 product 8、M4 recipe 11、M5 5、local_test_provider 1 | 这是源包数；local_test_provider 是测试 provider，不计为正式游戏内容。M4 Miner 在 native 2-kind 目录之外。模块存在不证明已构建、安装、激活或逐项内容已验收。 |
| `crates/oasis7_simulator/scenarios/` 的 [场景文件 authority](../../world-simulator/scenario/scenario-files.prd.md) | 12 份直属 JSON；其中 4 份由[碎片场景交付](../../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#scenario-content-catalog)细化 | 文件是场景变体，不是实际地点／碎片数。程序构造的 formal seed、有效默认配置与当前世界另行绑定，不能只靠 JSON 数量覆盖。 |
| [程序化视觉内容](../../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#visual-content-catalog) | 4 族：材质 10 个键／回退、热点 5 类含回退、模块 4 类及中性回退、depot 3 个状态及未知处理 | 原 Rust 是编辑真值；这些分支不是独立贴图／模型文件。参数变体和程序化实例不能乘成产能。 |
| `assets/ui-ux/` 中的 Viewer 静态图稿及[原映射](../../world-simulator/viewer/assets/ui-ux/full-function/coverage-catalog.json) | 173 个受跟踪 SVG；原 coverage-catalog 包含 107 个源功能条目，catalog 自报来源为 `113868200d1f91bef1008babb689bd257f241428` | SVG 是可编辑参考稿，catalog 是历史源派生映射；两个数字都不是当前全功能完成分母。一个状态可能引用多张稿，稿件也可能被多个状态复用，且不证明 runtime 已使用。 |
| [双语与帮助](../../world-simulator/viewer/viewer-localization-and-context-help.design.md)及[叙事内容](world-narrative-content-delivery.design.md) | 按原术语、消息角色和状态矩阵选单元；zh/en 版本分别核覆盖，不给运行时消息伪造固定总数 | 原消费者中的字符串和对应设计文本各有采用状态。一次 Agent 自由文本不是可预先制作的固定对白资产；其质量验证按原场景进行。 |
| Viewer／Launcher 静态资源与字体 | Viewer 自身受跟踪独立图像为 [favicon.ico](../../../crates/oasis7_viewer/favicon.ico)；原生 Launcher 嵌入 1 个 [ms-yahei.ttf](../../../third_party/ms-yahei.ttf) | 这不是所有运行资源总量；WASM 内嵌依赖、构建输出和机器字体另算。字体的具体来源边界见 §3.2。 |
| `site/assets/images/` 中的官网公共图像 | 23 个受跟踪图像文件：7 SVG、13 WebP、3 PNG | 文件数包括源／导出、概念展示、历史截图和传播图，不能相加为 23 项独立原作或本次发布 payload。实际使用集合从各语言页面和候选生成物追踪。 |
| [小说 V50](../../../site/story/releases/v50/README.md) | 15 章、1 间章和 1 尾声，共 17 个阅读单元 | Markdown 是当前中文阅读源；不是 17 个游戏任务或英文正文。阅读顺序、原文保持和旧版本边界沿原索引。 |
| [传播制作目录](../../../site/social/xiaohongshu/README.md)及[既有八图 manifest](../../readme/production/assets/xhs-ai-economy-2026-06-18/asset-manifest.md) | `site/social/` 有 20 个 HTML 和 66 个 PNG；另一个原制作包有 8 PNG、1 生成脚本及原 storyboard／manifest | 分别按原包追踪源与导出，不把 HTML 与导出逐个当新原作。数量只说明在库文件；实际发布、外部研究来源、字体和图像权利须按原包核实。 |

工具 gallery、历史截图／证据、文档演示和第三方技能素材继续属于各自工具或证据入口。它们只有被明确选入游戏／发行内容后才成为该次制作输入；在仓库内可找到不等于已经采用。六视图机器人仍是[工具原型](../../../tools/voxel_asset_pipeline/DESIGN.md#asset-contract)：没有取得所选正式源图、产物和实际加载链时，不给正式机器人资产计数。

另有[Viewer 音频内容方案](../../world-simulator/viewer/viewer-audio-content-delivery.design.md#audio-content-recipes)定义 S00／S01／S02 三份程序化提示配方；这是新增设计输入，不是上述源码基线已有的三个音频文件。实际生成器、参考声波、运行消费和试听分别交付，制作状态不能相加为三件已完成资产。音乐、空间环境声与配音采用该专题的当前取舍和重评条件。

角色动作／特效、其他世界／模块内容的全量目标没有在上述数字中被消除。本表未提供它们的采纳决定或完整素材集合；扩大范围时先从相应产品／专业条款取得用途、数量和验收。未知总量不能填零，动态生成的无限输出也不能充当已生产数量。

<a id="source-and-rights"></a>
### 3.2 来源、许可、署名与可继续编辑性

每次采用或替换内容，在原制作入口保留可取得的源及版本、作者／取得来源、所用许可或授权记录、要求保留的署名／notice、允许的编辑和复用范围、实际导出及消费者。已有 manifest 字段直接使用；没有等价入口的小包可在原制作说明中共享一组来源信息，不新增全仓逐文件审批表。Git 作者、文件名、生成工具名或根 LICENSE 不能互相替代这些信息。

| 当前来源组 | 已核到的具体材料 | 尚未被材料证明的部分与制作处置 |
| --- | --- | --- |
| 仓库原始规则、代码生成图形、文本和 SVG | 根 [LICENSE](../../../LICENSE)声明 MIT，copyright 为 `2026 scc`；`pixel_world_bridge` 的 [Cargo.toml](../../../crates/pixel_world_bridge/Cargo.toml)也声明 MIT | 这是已定位的仓库声明，不是所有输入逐项取得过程。复用时保留实际适用许可与 notice，若包含导入／生成／第三方元素，继续核其来源；不把根声明扩张到来源不明的外部素材。 |
| Launcher 嵌入字体 | [main.rs](../../../crates/oasis7_client_launcher/src/main.rs)以 `include_bytes!` 使用 `third_party/ms-yahei.ttf`，并允许环境变量覆盖；文件 name 表标示 Microsoft YaHei、Version 5.00、Microsoft Corporation／Founder | 元数据许可提示使用依原产品许可并受嵌入限制约束；相邻目录没有本项目取得／再分发授权材料。字体元数据和文件存在不构成授权结论。发行维护者需核所选分发用途的依据，或按原字体链替换为已核实来源后复验；本设计不修改只读 third_party，也不宣布现有包已审清。 |
| Viewer Bevy 默认字体 | `pixel_world_bridge` 启用 `default_font`；[Cargo.lock](../../../Cargo.lock)固定 Bevy／bevy_text 为 0.19.1 | 依赖来源已定位，本文没有取得该固定依赖实际包含的字体原件和 notice，不猜字体家族或许可证。对实际构建资源核原件、许可及随包保留情况后才能给出分发结论。 |
| Viewer Shell 系统字体栈 | [viewer.html](../../../crates/oasis7_viewer/viewer.html)声明 body／display／mono 字体名称与 fallback | 声明字体名不等于随包分发对应字体，也不证明目标机器字形存在。实际机器解析结果、CJK、fallback 和自托管变更分别验证。 |
| 官网远程字体 | [styles.css](../../../site/assets/styles.css)与 [deck.css](../../../site/deck/deck.css)实际使用 Google Fonts CSS `@import`，包含 Bricolage Grotesque、JetBrains Mono、Manrope；传播包还各有自己的请求 | 当前请求入口不同于系统 fallback，浮动响应没有固定为本地资产。实际发布保留其请求／返回版本和失败 fallback 的验证范围；改为本地打包时核具体字体文件与许可，不从服务名推定取得全部再分发材料。 |
| 官网图像、SVG 与 raster 导出 | §3.1 的具体路径、Git 版本、原 [Site 设计](../../site/github-pages/public-site-discovery-and-showcase.design.md)和公开 provenance 标签 | SVG 可编辑只证明形式；raster 文件不等于原生编辑源、生成输入或完整权利材料。原作者、外部／生成来源未有对应记录时，明确该项缺口，不把概念图写为运行截图。 |
| 原传播包 | 八图包有 [generate_carousel.py](../../readme/production/assets/xhs-ai-economy-2026-06-18/generate_carousel.py)、storyboard、manifest；其 manifest 说明外部研究材料未随仓库提交 | 生成脚本和上传顺序不替代外部资料、图像、字体、署名与复用依据；继续使用原受控来源入口，不在公开文档复制凭据或未授权材料。 |
| Pixel World 原生 CJK fallback | Noto Sans SC Regular 2.004，固定上游 `notofonts/noto-cjk@f8d157532fbfaeda587e826d4cd5b21a49186f7c` 的原样 SC 地区 OTF；[资产及 OFL](../../../crates/pixel_world_bridge/assets/fonts/noto-sans-sc/README.md)记录原始路径、SHA-256、版权、大小与覆盖边界 | OFL 1.1 允许随软件嵌入及再分发并要求附版权与许可；源码目录保留全文，每个 WASM backend 通过 `native_font_notices()`保留全文。未采用 Launcher 微软雅黑，不把此证据扩写为其他资产权利齐备。 |
| 六视图／后续外部或生成素材 | [工具合同](../../../tools/voxel_asset_pipeline/DESIGN.md#source-image-contract)规定实际源图和导出格式 | 采用时绑定真实源图／输出、工具及相关输入来源；不可确定重建的输出保留其固定版本。合成测试长方体不是正式机器人源，生成服务名也不是完整权利证明。 |

权利核对是所声明用途的材料核对，不能把未取得材料改写成“违规已证实”或“可自由复用”。已核范围写明具体文件／版本、用途、限制和证据；有未决输入时保留该内容的采用限制，原 owner 补证、换源或调整对外内容。尚未核实的材料不支持“全资产权利齐备”或正式发行结论。

素材仅有导出时仍可用于其已确认用途，但必须说明继续编辑限制；不得假称 PSD／Blend／原音频工程齐备。下游需要改分辨率、语言、材质、对白或商业用途而超出原资料时，回到原源和适用许可判断，不因旧版曾展示就自动复用。

<a id="production-handoff"></a>
### 3.3 按内容类型完成交接

| 制作类型 | 制作方交付 | 消费方核验与结束条件 |
| --- | --- | --- |
| 配方／工厂／WASM 内容 | 原 canonical ID、用途、内容源、版本、依赖、源解释和原产物身份 | 沿[工业交接](industrial-native-content-delivery.design.md#content-change-handoff)核 native／module-backed 差异、完整输入／主副产物和初态；沿原 WASM pipeline 重建实际依赖闭包。文件交齐不证明世界当前可执行。 |
| 场景与生成输入 | 具体 JSON 或程序构造入口、有效配置、seed／profile、预期用途与生成约束 | 沿[场景导入](../../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#editing-source-and-import)核实际初态和消费者保留；原来源绑定、生成品质与当前玩家开局仍分别验收。 |
| 程序化图形 | 原内容键与表达意图、编辑源、已知／未知对照、语言和字号等组合依赖 | 沿[视觉交接](../../world-simulator/viewer/viewer-world-visual-content-delivery.design.md#content-change-handoff)构建 bridge，核实际加载、标记语义、文本等价与目标环境；不要求伪造独立贴图源。 |
| Viewer 功能提示音 | S00／S01／S02 的数值配方、生成器版本、参考声波及来源资料 | 沿[音频交付](../../world-simulator/viewer/viewer-audio-content-delivery.design.md#audio-delivery-handoff)核实际合成消费者、事件与当前身份、静音等价和目标设备试听；参考 WAV 与运行实时合成字节分别识别，配方本身不证明听感。 |
| 文案、双语、帮助与静态稿 | 原语义／状态、源文本、适用语言和入口、SVG 或原生文件、字体／资源依赖；保留参数与否定句 | 按[本地化手册](../../world-simulator/viewer/viewer-localization-and-context-help.manual.md)与原交互来源核正常／失败／返回、窄屏和可访问性。参考稿、译文和实际集成分别记录；图稿里的示例值不得进入世界事实。 |
| 官网／小说／传播内容 | 原正文、素材源／导出、语言、provenance、署名与引用；新稿与已发布版本可区分 | 按[Site 发布与纠正](../../site/github-pages/public-site-discovery-validation.manual.md#内容发布与纠正)核详情、摘要、列表／RSS、双语与旧链接；小说继续保留原阅读单元，不转成玩法权威。 |
| 外部资产与字体 | 所选文件版本、来源及用途依据、编辑／输出规格、署名／notice、修改限制和实际交接范围 | 原集成 owner 核目标入口、打包、fallback 和受影响内容；需要安装、下载、采购或改造加载器时沿原任务处理，不把收到文件当作已可用。 |

共享模板、palette、术语表和同一二进制源由一个写入者合并；可独立的语义单元、文案包或素材包按原目录拆分。冲突在源解决，再由原工具重新导出；不在生成 WASM、bundle 或 raster 上补丁式消除冲突。文件重命名、拆分、复用和退役均应核对全部真实消费者，而非只核文档链接。

<a id="change-and-candidate"></a>
### 3.4 变更传播与同一候选

候选记录复用原构建、资源清单与发行证据，能关联程序／规则、内容／配置、实际导出、外部固定版本、平台、语言、字体和质量设置。源 commit 说明输入；实际加载／交付证明说明消费者使用了什么，二者不能替代。

| 变更 | 必须重核的相邻内容 | 失败时的处理 |
| --- | --- | --- |
| iron_ingot 配方或 starter 输入改变 | 同名 native／WASM 定义、实际账本／初态、首局文案与 Quote／Receipt、后继依赖；沿原规则决定是否影响已存在行动 | 回原玩法与 Runtime owner 修复；不以更新文案隐藏资源或历史不一致。 |
| `water_ice` 程序化外观改变 | 原组成键、unknown 对照、LOD、实际 bridge、字体／图例及视觉证据 | 回视觉源与集成；保留旧候选结论，不用新图覆盖旧结果。 |
| 字体来源或实际文件替换 | 字体许可／notice、嵌入或远程加载方式、中英混排、图稿与导出尺寸、fallback 及包内容 | 资料或字形不满足原用途时换回合法可用组合或收窄实际发布范围；不把系统字体名当作替代文件已经交付。 |
| 官网主图、传播图或小说版本替换 | 原稿／导出／provenance、语言、摘要／预览和实际静态站点引用；历史材料仍保留其原身份 | 原发布 owner 修正或撤回受影响展示，沿原纠正方法告知；不将新源倒写成旧运行证据。 |

内容替换只有在实际用途和关系确认后才能视为独立交付。涉及进行中行动、原账本或世界状态时，必须沿原兼容／迁移合同处理；本设计没有修改历史、补发资产或跨世界复用授权的能力。

## 4. 制作规模、人员与成本

<a id="production-selection"></a>
### 4.1 选择实际生产批次

制作量来自拟交付体验及已有规则，不能从仓库文件数直接推定。每批在原 Task 或制作记录中确定内容类型、原 ID、增量数量、语言／平台／品质、必要消费者和样本复杂度；本设计不镜像排期与分派。

| 类型 | 可复用的数量单位与复杂度 | 降低成本时可调整什么 |
| --- | --- | --- |
| 规则／场景 | 一个受影响 canonical 内容或场景变体；区分既有模板编辑、增加依赖、改变初态／共享模板 | 先复用已采用类型、延后尚未承诺的变体；不能删副产物、owner／账本或失败条件来使样本变简单。 |
| 表现与交互内容 | 一个语义状态的语言／平台组合，或一个内容族的已采用变体；设计稿和 runtime 采用分别核算 | 复用组件、布局和原几何；保留当前承诺的文本等价、未知处理、键盘／可访问表达，不靠少一张失败稿宣称覆盖。 |
| 官网与传播 | 一份正文或素材包及其必需语言、派生版式和发布入口 | 复用有来源依据的编辑源与样式；未承诺的新渠道或装饰变体可暂缓，不能删除 provenance、关键限制或当前必须的译文。 |
| 模型／音频等新制作方式 | 所选资产的实际规格、变体、输入权利及目标消费者；首次工具／导入建设单列 | 先以已授权的代表性范围验证；不能把工具原型或一个精修样本外推成全类别量产。 |

制作 owner 决定内容取舍，内容作者负责规则／文字／素材，专业工程负责转换和实际接入，QA／专业审阅者核规格与目标环境。角色可以由同一人承担，但每项实际人时只记录一次；职能覆盖不等于必须新增相应人数。人员不足时先看瓶颈职责、可复用输入和依赖，不能单纯按总人数平均分摊。

外包只交接范围确定的包。一个可报价的既有示例是 [first-iron-batch-zh.svg](../../../site/assets/images/social/first-iron-batch-zh.svg)及其[英文源](../../../site/assets/images/social/first-iron-batch-en.svg)对应的双语传播图修订：报价须说清正文事实及来源、两个可编辑 SVG、对应 PNG、原尺寸／字体依赖、允许修改的范围、署名材料、目标入口核对和返修条件。若只报价 PNG 导出，就不能将编辑源、翻译、字体采购或发布集成算作已包。示例不授权实际委托，不改变原内容。外部费用按实际包和报价计，未报价的义务仍由内部负责。

<a id="cost-and-capacity"></a>
### 4.2 角色人时与直接费用

对每一角色 `r`，按一致的人时单位计算选定批次：

```text
H_r = F_r + Σ_c [N_c × (E_c,r + I_c,r + V_c,r)] + J_r + R_r + M_r
C   = Σ_r (H_r × P_r) + C_tool + C_vendor + C_other
```

| 量 | 单位与含义 | 避免重复或失真的规则 |
| --- | --- | --- |
| `N_c` | 类别 c 的增量内容单元数 | 采用 §3.1／§4.1 的单位；同源导出和一次参数实例不自动增加单元。复用仍有实际适配／检查工作。 |
| `F_r` | 人时；首次工具、模板、导入或流程准备 | 与重复制作分开；既有可复用准备不在每个单元重复计算，也不删除本次新适配成本。 |
| `E / I / V` | 人时／单元；分别为内容制作、单元集成、单元验证 | 按角色与复杂度记录。已有复用的收益体现在本次较小的真实增量中，不再另减一遍“复用折扣”。 |
| `J_r` | 人时；批次共同的组合集成与验证 | 只记未包含在单元 I／V 的跨项工作。重复的共享构建或同一次审阅不能多处累加。 |
| `R_r` | 人时；返工、重导出、复核或恢复 | 实际返工与未来风险估算分开；已计入单元原始记录的返工不能再加。 |
| `M_r` | 人时；明确维护窗口内的内容更新、来源／依赖维护和复验 | 固定窗口及次数依据；一次制作与持续维护分别报告，不能用一次性交付预算承诺长期维护。 |
| `P_r` | 货币／人时；实际采用的内部成本口径或报价口径 | 同表采用一致币种与计价范围；没有费率可报告 H，不把缺失价格填零。 |
| `C_tool / C_vendor / C_other` | 同币种的实际直接费用或明确估算 | 区分一次性与按量，写清税费／汇率等采用口径；外包已包含的人时不再计入内部 H，内部接收／返修仍计。 |

等待依赖和日历交付时间单列，不能直接等价为人时。实测样本应注明已覆盖的内容类型、复杂度、源／工具／候选、初次还是重复制作、实际返工与未覆盖风险；样本不足时可以给假设区间，但不标成实测均值或 P95。新类别、新语言、新导入链和共享规范变化不能沿用无关简单样本的速度。

容量按角色核对：在所选窗口中，`可用人时 = 已确认可投入人时 − 其他已承诺工作 − 本窗口保留的维护工作`；如果维护已经通过 M 纳入本批，就不再次扣减同一工时。`max_r(H_r / 该角色每周可用人时)` 仅是资源约束下界，关键依赖、顺序集成、外部交付等待和实际工作日还可能延长。任何必要角色没有可用容量时，不能以其他角色空闲填补其职能，先调整分工、批次或承诺。

运行基础设施、玩家模型调用费用、客服／社区和事故处置属于各自实际服务窗口，继续由[玩家支持与服务生命周期](../../readme/governance/player-support-and-service-lifecycle.design.md)及原运行 authority 维护。项目总预算汇总时分别加入且去重，不把它们混进“每张图制作速度”，也不因为内容制作完成就计为零。

### 4.3 可复核的假设算例

以下只用于说明计算，没有执行制作或取得报价。假设一批包括 4 个已有状态的双语文案单元、2 个既有程序化标记变体、1 个现有场景变体；三个角色分别承担下表对应工作。

| 类别 | N | 制作／单元（作者人时） | 集成／单元（工程人时） | 验证／单元（审阅人时） | 小计人时 |
| --- | --- | --- | --- | --- | --- |
| 已有状态双语文案 | 4 | 0.5 | 0.25 | 0.25 | 4 |
| 程序化标记变体 | 2 | 1 | 1 | 0.5 | 5 |
| 场景变体 | 1 | 1.5 | 2 | 1.5 | 5 |

重复单元合计 14 人时，其中作者 5.5、工程 5、审阅 3.5。再假设 F 分别为 4／3／1，J 为 0／1／2，R 为 2／1／1，M 为 1／0／1，则最终角色人时是 12.5／10／8.5，总计 31 人时。若仅校验费用公式而假设费率为 100／120／100 元每人时、工具费 60 元，外包及其他费用在此算例为 0，则 C 为 3,360 元；这些数字均不是本项目报价、预算或市场费率。

若同一算例中的每周可用人时为 10／5／10，资源下界为 `max(1.25, 2, 0.85) = 2 周`，不是两周交付承诺。删去一个未承诺标记变体最多先减少其 2.5 个重复单元人时，共享准备／组合工作是否减少另核；不能机械从总成本减去七分之一。实际预算需要真实范围、样本、可用容量与报价替换这些假设。

## 11. 验证与验收

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作单元与范围](#production-scope) | 计数可重算，源／内容／导出／消费者单位分开 | [CP-SCOPE](content-production-and-rights.manual.md#cp-scope)；固定源码与原目录，静态盘点 | 原 PR 的版本、集合摘要和语义审读 | 不证明运行状态、全游戏分母或所有模块内容完整。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [来源与权利](#source-and-rights) | 对实际用途定位源与权利材料，未知项保留真实采用边界 | [CP-RIGHTS](content-production-and-rights.manual.md#cp-rights)；所选文件／依赖／来源的固定版本 | 原制作入口的资料定位与审阅结论 | 不补造缺失授权，不以静态扫描或元数据代签法律或发行结论。 |
| [内容生产与交接](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [类型交接](#production-handoff) | 制作方与消费方各自能确认输入和实际采用结果 | [CP-HANDOFF](content-production-and-rights.manual.md#cp-handoff)；选定真实类型与目标入口 | 原 Task／PR 的源、导出、实际接收及缺口 | 文档推演不证明已导入、构建或目标品质。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [变更与候选](#change-and-candidate) | 一项变更能追到实际消费者、重建／重验与旧证据边界 | [CP-CHANGE](content-production-and-rights.manual.md#cp-change)；原变更及新候选 | 原构建／发行与专业验证记录 | 旧截图或同名文件不能证明新的消费组合。 |
| [制作范围与维护](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-design-deliverables)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [生产选择与人员](#production-selection) | 数量、复杂度、角色覆盖和外包包的实际义务可核 | [CP-PLAN](content-production-and-rights.manual.md#cp-plan)；实际范围与可用输入 | 原制作／报价记录和被采纳的范围决定 | 职能划分不证明人员到位、采购授权或排期可行。 |
| [规模化制作证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence) | [成本与容量](#cost-and-capacity) | 成本不重复计入，等待／实测／假设与维护可区分 | [CP-COST](content-production-and-rights.manual.md#cp-cost)；假设算例用于公式审查，生产结论使用真实代表样本 | 原 PR 的算术审查；实际制作记录的输入、费用和容量依据 | 31 人时算例不证明项目产能、预算或任何真实交付。 |

## 12. 决策与剩余风险

采用原内容索引和构建记录作为真值，只补跨类型的计数、来源和生产决策。这样可以沿实际内容开展制作，不要求建立第二套资产数据库；也保留了源内容与实际运行资产不完全相同的事实。

当前仍需补齐的材料包括 Launcher 字体的实际取得／分发依据、Bevy 固定依赖的实际字体与 notice、各外部／生成图像的源与适用授权，以及具体候选加载和目标品质。它们分别由原资产／依赖／发行 owner 在采用或阶段验收时解决。现有声明不足时选择补证、换源或收窄该项实际用途，不能在汇总里将未知项视为通过。

整游戏目标规模、具体人员／外包与货币预算尚无本文所需的全部输入；此处给出了实际内容基线、范围选择、可计算的成本模型和方法，未宣布预算已核定。扩大类别或数量前使用适用的代表性制作、集成、验证和变更记录；仍未覆盖的复杂度保留为风险。局部文档完整、源文件存在及工具可运行，都不能单独得出 GD-18／GD-20 或规模化制作已经完成。
