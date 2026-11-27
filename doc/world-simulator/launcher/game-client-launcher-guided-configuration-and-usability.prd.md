# 客户端启动器引导配置与可用性（当前 authority）

> **退役与承接边界（[#7321](https://github.com/nil-studio/oasis7/issues/7321)）**：原 `egui`/`eframe` Native 与 Trunk `web-launcher` 客户端启动器 GUI（`oasis7_client_launcher`、`oasis7_launcher_ui`、`oasis7_launcher_config` 及 `.oasis7_launcher_ux_state.json`）已决定退役下线（代码清理由 [#7325](https://github.com/nil-studio/oasis7/issues/7325)、[#7326](https://github.com/nil-studio/oasis7/issues/7326) 执行），玩家唯一 GUI 收敛至 SolidJS Web `viewer`。本文保留配置草稿/确认态/运行态分离、真实 `blocked`/`observer` 表达与恢复语义供无头控制面（`oasis7_launcher_backend`）及适用入口消费，不再维护或要求独立 `egui` 启动器界面。

- 对应设计: `doc/world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md`
- 历史迁移、验证与 task 状态：GitHub task issue evidence。

## 目标

- 让操作者在受支持控制面或入口理解当前配置问题、可执行动作和受阻原因，而不是从原始日志猜测状态。
- 以当前事实呈现 blocked、未就绪或 observer 等状态；状态可见不等同于游戏、网络、公开加入或发布 readiness。
- 把配置修复和下一步提示限定在已有受支持动作内，不创造新的玩家权能或世界规则。

## 范围

- 受支持本地启动与控制面入口的启动前配置检查、问题分类、草稿/已接受/运行态区分以及真实受阻提示（原 `egui` Native/Trunk 窗口与自引导 UI 已按 #7321 退役）。
- 本文只记录配置与请求结果边界；不收敛 runtime、chain execution-world、stale-world recovery、数据持久化或网络语义。

## 当前合同

| 范围 | 当前 launcher 表现 | 不得外推为 |
| --- | --- | --- |
| 语言与配置 | 中英文文案、必填/格式问题和预检阻断在既有表面可见；配置问题应有当前可达的修复入口 | 所有语言、字段或历史配置规则永久不变 |
| 状态与受阻反馈 | 启动、链、provider 或功能入口的当前 `ready`、未就绪、disabled、blocked/error 与 observer 语义必须按现有状态呈现 | 操作成功、runtime 健康、网络连通、公开玩家资格或发布 readiness |
| 引导与下一步 | 当前 onboarding、任务提示、错误卡或 CTA 只能编排现有动作；引导可跳过/重置，并必须保留专家配置入口 | 自动修复、自动启动、强制流程、远端运营承诺或新的控制能力 |
| 响应式可用性 | 状态、配置问题和关键操作在当前 native/Web 表面中保持可读、可访问；具体视觉验收以视觉角色规格为准 | 固定布局、所有设备适配或逐像素 native/Web 一致 |
| 编辑与请求状态 | 本地配置编辑态和当前请求/错误状态应保持可解释，不能以无关快照或空白覆盖用户可见问题 | 后台持续监控、任意并发保证、持久化恢复或数据完整性证明 |

## 接口 / 数据

<a id="launcher-config-draft-truth"></a>
### 配置草稿、最近成功画像与恢复

Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1, O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；产品上游为 [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) 的配置阶段/本地成功不得代签世界结果，以及 [REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) 的旧上下文不直接复用、恢复后重新验证。本节只接收草稿、确认配置和成功画像子义务，不承担全部会话、凭据注入或世界恢复。

既有 Restore Config 的本地画像是配置输入，不是已运行状态。本节承接 [#4406](https://github.com/eng-cc/oasis7/issues/4406)
C1/C2 与 L5，细化[本地启动产品要求](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#23-设置与结果边界)，不新增画像系统或自动续跑。

- 本地草稿、控制面接受的配置、实际对应运行实例的配置和 last-successful 画像必须可区分。
  某进程仍 Running，不能证明当前可编辑草稿已被使用。
- Restore 只把已存画像 normalize 后放入本地编辑态；若与当前已确认配置不同，dirty 必须保留。
  后续旧配置 snapshot/poll 不得覆盖该编辑态；对应配置真正被控制面接受并确认后才可清 dirty。
  Restore 不自动提交、不启动进程，也不改变仍在运行的 A 配置。
- last-successful 只记录已确认用于成功运行的配置；native 文件和 Web storage 都不能在
  A Running 时把未应用的 B 草稿存为成功画像。新配置 C 只有在实际被接受并进入相应运行状态后
  才能替代 A；没有可靠关联时保留原画像，不猜成功。
- 缺失 game binary 的修复只能指向已确认的 game 可执行文件或构建/安装/显式选择入口。
  Autofill 不得把 launcher GUI 的 `current_exe` 当 game 程序，也不能据此解除缺失阻断。

验收：A 已存、重开 idle B、Restore A、下一 poll B 后仍显示 A 草稿；A Running→编辑无效 B
跨多帧仍只保存 A；C 实际接受后才保存 C。另测缺 game 而 GUI 存在时的 Autofill，以及有效
game 的正常路径。按钮 handler→状态刷新→保存/展示分别贯通，单测赋值不代替这条链。

### 接口与数据来源

- 行为事实来自 `oasis7_client_launcher` 的语言状态、配置问题收集、launcher 状态、self-guided 组件及其既有 native/Web 表现层；具体字段、CTA 和存储细节保持现有兼容性，缺陷修复须满足上面的草稿/确认值边界。
- 角色值中的 `observer` 仅表示当前配置/状态语义，不构成 node admission、验证者资格、公开链访问、结算或数据完整性证明。
- 本文不新增 API、GUI Agent 动作、CLI 参数、遥测、DOM、轮询、数据保留或 runtime 接口。

## 里程碑

- 已完成：四组历史专题分别记录了语言与必填配置、可用性修补、全量体验收口和自引导工作。
- 本轮：将可验证且仍适合默认入口的共同表现合同收敛为 stable authority，修复本 scope 的路由并退役十二个 source 文件。
- 后续：任何新的配置规则、引导动作、状态语义、可见交互或浏览器表面变更，均须在独立任务中重新定界和取证。

## 风险

- 状态标签或引导文案若脱离当前实现，可能把 blocked/observer 误读为可用或可加入；本页不以历史任务完成记录支撑当前结论。
- 过度引导可能妨碍专家操作，因此引导只组织既有动作，并保留当前高级配置入口。
- 不同窄屏、native 与 Web 表面可能有视觉差异；可见变更需由 game_visual_interaction_designer 定义验收并按 `testing-manual.md` S6 取证。

## 不作出的承诺

- 除本文明确承接的 Restore/最近成功画像外，不从日期化来源推导任务完成、测试、截图、默认值、脚本、端点、演示、其他画像或持久化仍是当前事实。
- 不主张玩家可玩性、hosted login、strong-auth、public join、node admission、chain/runtime readiness、mainnet、结算、最终性、公开服务或 release readiness。
- 本轮不修改代码、UI、DOM、协议、runtime 或测试；S6 截图不适用。未来触达可见表面时必须重新取得 desktop/mobile 浏览器和视觉验收证据。

## 验收与追溯

- 文档迁移验收：`./scripts/doc-governance-check.sh`、`./scripts/readme-link-check.sh`、`python3 scripts/product-doc-governance-check.py`、`git diff --check`。
- 下列四组 source triplet 已退役删除：`i18n-required-config-2026-03-02`、`availability-ux-hardening-2026-03-08`、`full-usability-remediation-2026-03-08`、`self-guided-experience-2026-03-08`。历史需求、设计、测试和完成记录仅通过 Git 与 GitHub task issue evidence 追溯。
