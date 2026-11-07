# Viewer 世界视觉内容制作与验收手册

- 主责：`game_visual_interaction_designer`；协作：`viewer_engineer`、`qa_engineer`及内容／依赖的原 owner。
- 适用设计：[世界视觉内容与交付](viewer-world-visual-content-delivery.design.md)。本手册覆盖当前程序化材质、热点、模块标记、Micro Depot 内容及其构建依赖；不增加模型导入器或生产素材。
- 本文是方法，执行结果保留在实际 PR／原证据记录。文档审读、源码测试、软件 raster、WASM 构建、真实 WebGL、真人理解和发行分别记录；未运行不填通过。

## 1. 准备与验证选择

先固定内容源 HEAD、实际改动文件／键／状态、上游条款和本次交付目标。纯文档变更执行 §2 的只读核对与文档治理检查即可；实际内容变更继续 §3，再按影响范围选择 §4。不得为了补文档运行游戏、生成机器人或把合成 fixture 说成真实素材。

实际候选遵循[像素世界验证准备](../../testing/manual/pixel-world-visualization-validation.manual.md#1-执行准备与证据边界)：记录 source、构建／资源版本、profile、输入、OS／浏览器／GPU、viewport／framebuffer／DPR、语言／字体和 reduced motion。只保存允许披露的输入与证据；生成物和截图按原目录／提交规则维护。

<a id="visual-content-source-review"></a>
## 2. 内容源与交付关系的静态核对

V1～V6 仅为本手册审读步骤，不是新要求 ID 或已存在的自动测试名。按下表打开原文件并逐项比较，不从文件名或目录存在推定实现／运行通过。

| 步骤 | 实际输入与操作 | 通过判据与失败去向 |
| --- | --- | --- |
| V1 材质 | 读取 [host_state.rs](../../../crates/pixel_world_bridge/src/host_state.rs) 的 palette、dominant_compound 和 fallback；再读 [fragment visuals](../../../crates/pixel_world_bridge/src/render_fragment_visuals.rs) 的 shading／inset | 设计十个键／RGB 与源码一致；unknown、同值排序、无有效组成边界明确；不把奇偶 inset 叫作十种独特纹理。差异由视觉／Viewer owner 修正文档或原源。 |
| V2 事件 | 读取 [hotspot cues](../../../crates/pixel_world_bridge/src/render_hotspot_cues.rs) 的 kind 分支、通用分支和几何；联读原 hotspot 身份／命中处理 | blocker、goal、resource_transfer、build_queue、通用事件的形状与含义逐项对应；转运折线不表示实际方向，装饰不增加命中或成果。 |
| V3 模块 | 读取 [module glyph](../../../crates/pixel_world_bridge/src/render_module_visual_entities.rs) 的四种 chip 与未知分支，核原 label／anchor 接口 | 四种已知 kind 与中性回退可定位；不把 chip 当能力、权限或新世界位置。新类别未由原合同定义时交回模块 owner。 |
| V4 设施 | 读取 [depot glyph／读数](../../../crates/pixel_world_bridge/src/render_micro_depot_facilities.rs) 的状态分支、ratio、text、附属内容清理 | 三种状态、未知不绘、正 limit、已知零库存、缺 limit 分别解释；runway 不叫通用库存百分比；吞吐文字与库存保护分开。 |
| V5 事实负例 | 用 V1～V4 的未知键、未知 status、缺输入逐项追控制分支；对照[事实与未知](viewer-world-visual-content-delivery.design.md#visual-fact-boundaries)和原来源／回执合同 | 文档没有把未知补零、未绘制写成删除、排队写成完成或近邻写成关系。当前文本解释若未被核实，保留未验证，不编造 UI 能力。 |
| V6 源与权利 | 顺着[源与交付表](viewer-world-visual-content-delivery.design.md#content-source-delivery)打开源、Cargo 配置／锁、LICENSE、finalize 和 loader；对外部样本检查实际源／输出／来源资料 | 可编辑源和生成物不混同；有无实际消费者分别记录；仓库 MIT、字体依赖和外部素材依据分开。缺源图／notice／授权不写已齐备，不把工具测试替代素材。 |

可使用以下只读定位命令辅助审读；匹配成功只证明找到文本，不是内容或运行验收：

```bash
rg -n 'FRAGMENT_TERRAIN_PALETTE|fn dominant_compound|fn fragment_terrain_color' crates/pixel_world_bridge/src/host_state.rs
rg -n 'HotspotCuePart|resource_transfer|build_queue|RecentEventTick' crates/pixel_world_bridge/src/render_hotspot_cues.rs
rg -n 'module_identity_chip_specs|BeaconStem|RelayBar|ArtifactDot|SensorLead' crates/pixel_world_bridge/src/render_module_visual_entities.rs
rg -n 'facility_glyph|stock_runway_ratio|stock_throughput_text' crates/pixel_world_bridge/src/render_micro_depot_facilities.rs
rg -n 'buildPixelWorldRuntimeVariant|pixel_world_bridge_bindgen' crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs
rg -n 'optional-payloads.json|size_bytes|sha256|resolvePixelWorldWasmDescriptor' crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.js
```

核对文档中的路径、锚点、Trace 与承接／验证表，运行 `./scripts/doc-governance-check.sh`。结构检查只负责其已实现范围；语义由 V1～V6 审读。报告分别写“静态一致”“实际内容可取得”“候选已消费”“目标环境已验证”，不要合成一个未经证明的完成状态。

<a id="visual-content-production-handoff"></a>
## 3. 制作、替换与交接

### 3.1 现有程序化内容的操作步骤

1. **选择真实单元。** 例如 `water_ice` 材质或 `resource_transfer` 热点；记录要改善的表达、当前源、已知／未知对照与原设计依赖。若需新资源、业务状态或数据字段，先由原专业 owner 承接，不能用美术编辑绕过。
2. **固定输入与比较。** 复用已有 fixture／测试场景，记录当前形状／颜色和预期变化。材质包含相近色及 unknown；事件包含其他已知 kind 与通用事件。只写文档时沿源码推演路径，不生成“前后图”冒充实测。
3. **在编辑源修改。** 程序化内容改原 Rust，Shell 依赖改其原 source；不手改 WASM、bindings、dist 或 generated viewer.js。多人涉及同函数或 palette 时由明确写入者合并，关联分支与未知处理同次审读。
4. **检查关联内容。** V1～V5 核变体、默认值、文字含义、原命中区及回收。设施变更还核 runway、读数、底板与范围圈；来源／许可或字体变化同时执行 V6。
5. **交给实际消费者。** 使用原构建入口生成同候选 bridge／bundle；交付原 source、实际输出版本和适用输入。按 §4 核载入资源与显示结果，不以文件收到或 build 成功直接判内容可用。
6. **回写原任务证据。** 保存实际命令、输入／输出、环境、预期／实际、失败与未覆盖范围。视觉表达失败回到视觉／源实现；输入缺失回 Runtime／模块 owner；加载或 hash 问题回构建／Viewer owner；权利资料缺失由内容来源方补证。

以下路径供审读制作关系，不表示本专题执行了改色或试产：`water_ice` 原键 → host palette → fragment shading／LOD → bridge WASM → loader／host → 实际地形与 Actor 同屏检查；`resource_transfer` 原 kind → hotspot 类别细节 → 原 hotspot 组合／身份 → 同候选 bridge → 类别区分与无虚构路线检查。

共享地形／设施材质候选可在完整 visual-test 构建后执行 `AGENT_VISUAL_SOURCE_HEAD=<构建固定提交> npm --prefix crates/oasis7_viewer run test:pixel-world:materials:gpu`。可用 `AGENT_VISUAL_ARTIFACT_ROOT` 指定同源产物，`AGENT_VISUAL_CASE=desktop|narrow|dpr2` 与 `MATERIAL_VISUAL_SCOPE=factory|terrain` 缩小范围；默认覆盖三种尺寸与两类材质。需要 agent-browser、真实 WebGL2 与 Pillow。runner 保存实际 JS/WASM 和源 PNG hash、受控输入改动、DTO、截图及离线 PNG oracle；工厂校验已知／未知／删除／恢复，地形仅校验可见局部变化和精确恢复；原 base 组成色同时变化，不能单凭 delta 证明 ore PNG 纹理已被 GPU 消费，独立纹理特征／受控启停仍留原 P3。它不证明真实世界授权、depot PNG、全组合恢复或性能预算，缺口保留原 Issue。

### 3.2 替换、退役和外部素材

替换内容保留仍被消费的身份与旧版本证据，明确受影响输出、引用、缓存、署名和消费者；不能用新源覆盖旧截图的候选身份。依赖或生成配置变化时重新判断哪些结果可复用，未变的内容键不等于字形、颜色、像素或加载行为未变。

六视图工具采用前，制作方须能提供实际源图、固定版本、来源／使用依据及原工具要求的六视图规格；选择机器人或其他资产类别的 cleanup policy 时保留理由。输出 JSON.gz／OBJ／MTL／元数据与评估图分别定位，预览／评估仍遵循工具不提交规则。现有 `tests/test_six_view_to_voxel_model.py` 的长方体 fixture 只检工具行为。

当前未定位到本工具成品进入正式 Viewer 的加载链。若决定采用，Viewer owner 先补实际消费者、坐标／材质／资源预算和失败恢复承接，再做导入与目标环境验收；不要把“应消费 JSON”写成已集成。素材不可取得或授权未核实则保留缺口，不能把导入验证写 N/A。

<a id="visual-content-target-validation"></a>
## 4. 实际内容变更的分层验证

本节在实际内容、依赖、构建或目标候选变化时按影响执行，不是纯文档 PR 的必跑集合。复用[测试选择树](../../../testing-manual.md#testing-default-selection-tree)与[像素世界验证手册](../../testing/manual/pixel-world-visualization-validation.manual.md)，不新增 CI 门禁。

### 4.1 已有自动验证的选择

| 改动范围 | 真实测试源与准确入口 | 能证明与不能证明 |
| --- | --- | --- |
| 材质与背景层级 | [material hierarchy tests](../../../crates/pixel_world_bridge/src/render_material_hierarchy_tests.rs)：`compound_shading_preserves_channels_without_saturation_or_overflow`、`same_color_compound_update_reconciles_inset_without_resetting_camera`、`bevy_pixel_regression_exports_material_route_hierarchy` | 色彩运算、更新／清理与软件 raster；不能证明真实 GPU 或人能认出全部材质。 |
| 热点类别细节 | [hotspot core tests](../../../crates/pixel_world_bridge/src/render_hotspot_core_tests.rs)：`recent_event_kinds_have_distinct_low_density_glyph_parts_without_changing_hotspot_bases_or_hits` | 既有类别形状与 base／命中合同；不能证明真实视口可读性或世界事件真值。 |
| 模块 chip | [module visual tests](../../../crates/pixel_world_bridge/src/render_module_visual_entities_tests.rs)：`known_module_kinds_have_distinct_identity_glyphs_while_unknown_kinds_stay_neutral` | 已知／未知细节与软件样本；不能证明所有模块能力或当前部署。 |
| Depot 与附属内容 | [depot tests](../../../crates/pixel_world_bridge/src/render_micro_depot_facilities_tests.rs)：`micro_depot_glyphs_are_status_distinct_non_interactive_and_cleanup_stale_entities`、`micro_depot_stock_runway_is_deterministic_across_healthy_low_and_zero_states`、`micro_depot_stock_throughput_texts_use_compact_neutral_fallback_and_reconcile`；范围圈改动选 `micro_depot_service_radius_outline_is_noninteractive_and_cleans_up_when_invalid` | 原 glyph／读数／回收与非交互约束；不能把吞吐 ratio 当库存业务正确性。 |
| 动效依赖 | [selected raster tests](../../../crates/pixel_world_bridge/src/render_selected_raster_tests.rs)：`reduced_motion_freezes_pixels_across_reconcile_and_resumes_on_preference_change` | 固定相位及偏好变化下的软件像素；不能代签真实浏览器与用户感受。 |
| Loader／资源依赖 | [WASM loader tests](../../../crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.test.js)：manifest、payload 字节数／SHA-256、缺失资源、低动态偏好等现有用例 | 受控输入下的加载与生命周期合同；mock 模块不能证明实际 Bevy 输出。 |

Rust 的 canonical 局部命令形式为 `env -u RUSTC_WRAPPER cargo test -p pixel_world_bridge --lib <已选测试名>`；所选过滤器必须实际命中用例，零测试不计通过。Rust／WASM 改动按原测试手册核对应 wasm target；日常反馈可用仓库 wrapper，不把未执行检查写成已通过。

Loader 变更可执行 `npm --prefix crates/oasis7_viewer run test:ui -- pixel_world_runtime_module_wasm`。未改相关行为不为本专题另增镜像测试；已有覆盖不足时说明具体风险，交给实际实现 PR 补充。

### 4.2 构建、加载与真实画面

1. 按原构建合同执行适用的 `npm --prefix crates/oasis7_viewer run build:viewer`；视觉 fixture 使用 `build:viewer:visual-test`，不得将其 profile 当 release。记录实际 source／profile／输出；finalize 同时构建其他模块，不能把其中任一步跳过后仍称完整构建。
2. 核该候选实际 `viewer.js`、bindings 和 bridge payload；采用 optional payload 时核原 `optional-payloads.json`、所引用实际字节数／hash 和 loader 结果。检查发生在同一候选，不从 checkout 里的 dist 推断线上资源。
3. 按[PW-PROJECTION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-projection)与[PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)选择受影响场景；实际桌面／窄屏、DPR、字体和动效组合遵循原手册，保留未测组合。
   统一像素渲染候选同时按[生产坐标合同](viewer-pixel-world-player-readable-rendering.design.md#production-pixel-coordinate-contract)记录 C/F/DPR/s/L/o、相机与实际 CSS 命中中心；复核表中桌面、窄屏、奇数边缘和 zoom 手算，以及 Rust/Web 对同一最终坐标的消费。手算只核合同，另以真实 GPU 检查 nearest、source-pixel 边缘和独立中文层。
4. 复用 `test:pixel-world:visual`、`test:pixel-world:module-visual` 或 `test:pixel-world:location-frame:visual` 的适用入口。先检查其 fixture 是否包含本次变体；已有脚本没覆盖的材质／设施用例不能因名称含 visual 就计入。缺实际 fixture 或环境时记录 blocked／未覆盖，由该实现负责人补齐。
5. 保存实际尺度截图／crop 和输入身份，沿[视觉评审 SOP](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md#模型评审-rubric)审读；材质不压过 Actor，未知不变已知，热点类别不冒充路线／成功，设施状态和读数含义一致。构图／层级／动效直接引用原 B01～B13，不另建一套状态矩阵。

实际视觉改动应分别检查正常样本、未知／缺输入、删除／替换、同位置密集和 reduced motion 中受影响的组合。只读审图不能证明命中区或真实操作；实际交互按原 S6 方法执行。声称首读／因果理解有效时再执行原[PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability)，模型视觉意见不替代真人结果。

## 5. 记录与失败处理

在原 PR／Task 记录选定内容族／键、来源版本、方法／命令、候选／环境、预期与实际、原始证据位置及未覆盖范围。使用原 PASS／FAIL／BLOCKED／NOT RUN 语义；文档检查通过、素材可取得、build 通过、真实画面通过和权利资料核实分别表达。

本专题不新建全仓资产状态表。缺源／导出、缺消费者、缺授权资料、缺实现与缺验证各自保留；纯文档交付可完成设计与方法，但不能关闭尚未执行的素材／运行义务。扩大生产的成本、返工与代表性批次证据留在原制作任务，不从一次合成工具测试估算产能。
