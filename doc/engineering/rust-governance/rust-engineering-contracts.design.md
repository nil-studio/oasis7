# Rust 工程契约系统设计

- 状态：`active`；首批 Workspace 继承范围见下文，架构机制按切片实施，signed-read 首个开发期生成入口见[产物合同](#artifact-contract)。
- 主责：engineering 维护者；架构和 wire/profile 语义由原 crate、Runtime 与接口 owner 拥有。
- 专业需求：[Rust 工程契约需求](rust-engineering-contracts.prd.md)（`PRD-ENGINEERING-036`）。
- 实施验收：[验证手册](rust-engineering-contracts.manual.md)，新增工具就绪后按实际候选执行。
- 审读基线：[nil-studio/oasis7@2e21481a95614b42727abaddd1f2bc55008d791a](https://github.com/nil-studio/oasis7/tree/2e21481a95614b42727abaddd1f2bc55008d791a)，Rust `1.96.0`、resolver `2`。
- 通用开发与合入规则继续由[开发流程规范](../workflow/source-of-truth.md)主责；本文不启用新门禁。

## 1. 问题、目标与非目标

目前共享 Cargo 配置只覆盖部分依赖；架构边界分散在专业设计与局部检查中；Rust wire、服务 OpenAPI 和未来跨语言消费者之间还没有统一的产物生命周期。三个问题分别需要配置等价、结构检查和协议证据，不能由一次全 workspace 构建或一次 schema 生成共同代替。

目标是让维护者可追溯地回答：共享设置在哪里改、一个依赖为什么被禁止、一个协议文件从哪里生成，以及当前检查究竟证明了哪些配置和行为。设计要求可渐进实施、可独立回滚，保留现有语义 owner。

本设计不升级工具链、依赖或 resolver，不机械拆 crate，不重新定义 Runtime 授权与持久化，不自动发布 SDK，不实现完整 Game API，也不要求采用 Codex 的全部目录、构建系统和 lint 策略。文件体量继续由[1200 行专题](rust-1200-line-root-cause-governance-2026-03-29.prd.md)维护。

## 2. 上游约束与相关角色

engineering 维护者拥有共性配置与检查工具；crate owner 确认 feature/target 等价和依赖职责；Runtime、Game Client 与接口 owner 确认架构与 wire/profile 边界；评审者核对证据范围。消费者包括开发者、现有 CI、官方客户端和外部协议使用者。此分工沿用现有评审，不新增审批实体。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；[AC-RUST-001](rust-engineering-contracts.prd.md#ac-rust-001) | 共性有唯一来源，迁移保持实际 package/feature/target 语义；产品适用性：N/A；reason=开发配置治理；scope=本次迁移涉及的第一方成员 | [Workspace 合同](rust-engineering-contracts.design.md#workspace-policy) | engineering 与成员 crate owner；固定工具链 | 独立 workspace、依赖升级与性能承诺 |
| Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；[AC-RUST-001](rust-engineering-contracts.prd.md#ac-rust-001) | 本地/CI 共用已采纳 lint 定义，新增强规则独立说明；产品适用性：N/A；reason=诊断配置治理；scope=已迁移成员与最小局部例外 | [Lint 继承](rust-engineering-contracts.design.md#lint-policy) | engineering 与成员 crate owner | 不要求全仓立即新增 lint，不把命令强度当规则相同 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | 按 package identity、kind、target/features 计算声明与激活闭包；产品适用性：N/A；reason=新增结构检查机制；scope=依赖图解析 | [架构规则](rust-engineering-contracts.design.md#architecture-rules) | Cargo 固定工具链与实际支持矩阵 | 不代替权限和运行行为验证 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | 保留原方向、服务组合与合法测试范围；产品适用性：N/A；reason=结构规则表达；scope=首批边界矩阵 | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | 各模块专业设计与 crate owner | 不按包名统一禁止所有 client/server/dev 关系 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | 检查不完整与违规区分，诊断可复现；产品适用性：N/A；reason=检查覆盖与结果合同；scope=选定规则和必需组合 | [检查合同](rust-engineering-contracts.design.md#inspection-contract) | engineering；既有 CI scope/required 选择 | 文档采纳不启用新门禁 |
| [Runtime 状态与投影边界](../../world-runtime/design.md#状态与服务投影的编译边界) | 下层状态/投影不回拉上层执行，保留 Game Client 验证核心复用；产品适用性：N/A；reason=继承专业编译边界；scope=State/Projection/Runtime 结构 | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | Runtime、State、Projection owner | 不改变 patch、准入、恢复资格或 codec 所有权 |
| [宿主与轻量入口](../../world-runtime/design.md#1-设计定位) | 轻量生产与真实宿主/测试职责分别表达；产品适用性：N/A；reason=继承宿主编译边界；scope=Launcher、协议工具与组合入口 | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | Launcher、Protocol Tools 与宿主 owner | 不清除实际执行本地世界的合法依赖 |
| Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；[共享客户端要求](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-003) | Game Client/Harness 不反向拥有主应用或世界实现 | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | Game Client/Harness/Service owner；DES-AGC-001 | 结构检查不证明完整内外执行方行为等价 |
| Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；[共享客户端要求](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-003) | 公共 wire 保持唯一声明，生成物消费原合同 | [协议权威](rust-engineering-contracts.design.md#protocol-authority) | Agent/Client API、Proto 与接口 owner；DES-AGC-002 | 不从 public DTO 推导端点上线或执行权 |
| Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc；[AC-RUST-003](rust-engineering-contracts.prd.md#ac-rust-003) | 生成确定、check 只读、完整集合与冲突恢复明确；产品适用性：N/A；reason=开发期产物治理；scope=已登记 profile | [产物合同](rust-engineering-contracts.design.md#artifact-contract) | 生成工具/profile owner 与真实消费者 | 未登记 profile、自动发布与运行期生成 |
| Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；[游戏 API 要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-012) | 对已实施 bootstrap 提供同版本、编码准确的可发现派生合同 | [协议权威](rust-engineering-contracts.design.md#protocol-authority) | Game API owner；现有 OpenAPI/parser/handler | 仅承接合同一致性子义务，不证明完整行动闭环 |
| Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc；[AC-RUST-003](rust-engineering-contracts.prd.md#ac-rust-003) | 生成不改变整数、字段、canonical JSON/CBOR 或签名语义；产品适用性：N/A；reason=协议产物兼容管理；scope=实际触及的 wire/profile | [Wire 保持](rust-engineering-contracts.design.md#wire-preservation) | 原 wire/codec owner 与新旧消费者 | 未实现 lossless codec 不发布 JS 签名能力 |
| Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；[安全调用要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-013) | 生成/消费不得授予权限、改变原请求或安全恢复 | [运行约束](rust-engineering-contracts.design.md#runtime-constraints) | 原 parser、signer、World Service 与 Runtime | schema/TS 不证明认证、幂等或真实恢复完成 |

<a id="baseline-gap"></a>
## 3. 当前状态、目标状态与差距

下表随当前源码维护首版能力；顶部审读基线仅是历史设计来源。运行结果在实际 PR/CI 记录。

| 对象 | 当前实现与依据 | 目标状态 | 差距 |
| --- | --- | --- | --- |
| Workspace | [根 manifest](../../../Cargo.toml) 集中共性依赖、`workspace.package` / `workspace.lints`，首批 6 个成员显式继承；[工具链](../../../rust-toolchain.toml) 固定为 `1.96.0` | 可继承的共性与显式成员差异 | 其余成员尚未迁移；每批仍须等价检查，不修改 resolver/profiles/patch |
| Lint | [ci-tests.sh](../../../scripts/ci-tests.sh) 使用 `-D warnings`、`-D clippy::correctness`、`-D clippy::suspicious` | 相同规则定义可在本地消费，CI 强度继续明确 | 首批继承保持 correctness=deny、suspicious=warn 的本地级别；CI 继续升级，普通 clippy 不等于所有 CI 组合 |
| 架构 | [Runtime](../../world-runtime/design.md#实现边界与共享值所有权) 与 [Agent/SDK](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001) 已有边界；[Launcher 检查](../../../scripts/check-launcher-p2p-dependency-surface.sh) 使用 host、`--no-default-features` 的局部 `cargo tree` | 有作用域的规则、声明与解析闭包检查 | 首版检查器已有 9 条规则、37 个显式 profile，逐根声明/闭包与编译核查；不证明未登记平台或 Viewer/Service 全量配置 |
| HTTP bootstrap | [OpenAPI](../../../crates/oasis7_agent_api/src/game/read_bootstrap.openapi.json) 是人工维护的 OpenAPI 3.1；API revision 1、read schema revision 5、五条 GET；[handler](../../../crates/oasis7_viewer_runtime/src/game_read_api.rs) 按实际能力收窄 | 首版已从此 profile 生成 TS/JSON Schema，真实 handler fixtures 与 TS consumer 校验 profile | `ReadInfo`、`ReadProjection`、`ReadError` 目前由 `json!` 组装，不能假定已有同名 Rust struct 可直接 derive |
| Rust wire 与测试 | [game.rs](../../../crates/oasis7_agent_api/src/game.rs)、[World Service wire](../../../crates/oasis7_client_api/src/world_service_wire.rs)、[Viewer](../../../crates/oasis7_proto/src/viewer.rs) 分别拥有合同；已有编码/签名测试 | 明确登记根类型和消费者，未来按需生成共享 components | 现有 DTO 不自动代表 HTTP 能力；仅 signed-read bootstrap 已有生成闭环，其他 profile 未登记 |

## 4. 边界与结构

<a id="workspace-policy"></a>
### 4.1 Workspace 共性与成员自治

Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；承接 [REQ-RUST-001](rust-engineering-contracts.prd.md#req-rust-001)。

以 Cargo 解析的 `workspace_members` 为成员边界，不用目录扫描结果代替；被排除的独立 workspace 自行承接规则。根 manifest 只集中确实共享的 `edition`、license、repository 等字段。`version`、`publish`、MSRV、description、lib/bin target、crate-type 与发布文件选择按实际发布语义保留；版本字符串相同不意味着同步发版。继承 `readme`、`license-file` 时核对路径解释。

共享第三方依赖和适合集中维护的第一方 path/version 依赖放在 `workspace.dependencies`，成员显式 `workspace = true`。不能为了统一拼写抹去 `package` 别名、git rev、source 或有意保留的多版本。`optional` 仍由成员声明，normal/dev/build 和 `target.<cfg>` 归属保持原样；根的共享声明不会自动为未引用成员增加依赖。

feature 是叠加关系。共享基线只含所有相关消费者确实共同需要的能力，不取各成员需求并集。对默认 feature 需求不同的消费者，优先采用共享 `default-features = false` 并让原本需要默认能力的成员显式选择；不能依赖仓库固定工具链尚不支持的“根开启、成员关闭”覆盖行为。无法证明同义时保留该成员的显式声明，不通过升级 Rust 或启用额外 feature 消除差异。[Cargo 继承规则](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table)是工具语义来源，实施以仓库固定版本复核。

等价比较对象为：`consumer package × dependency kind × host/target × feature selection → resolved package/source/version + enabled features`。例如 [Model Provider](../../../crates/oasis7_model_provider/Cargo.toml) 的 reqwest 关闭 defaults、[Launcher](../../../crates/oasis7_client_launcher/Cargo.toml) 的 native/wasm eframe 差异、[Net](../../../crates/oasis7_net/Cargo.toml) 的 optional 网络栈及 [WASM SDK](../../../crates/oasis7_wasm_sdk/Cargo.toml) 的可选 `wire` 都必须保留。`Cargo.lock` 不变不能单独证明以上等价。

配置表示也是现有工具的输入。迁移须核对直接读取 manifest 的安全检查、CI 规划、构建和打包消费者；例如 [libp2p 来源检查](../../../scripts/libp2p-security-source.test.py) 当前读取成员的 git/rev。触及这些输入时同步支持显式与继承配置，保留来源锁定、optional/平台条件及原失败断言；不能以 Cargo 图等价为由删除或弱化检查。manifest 变化继续触发现有共享控制覆盖，并可能使相关构建缓存失效，不能将配置去重视为免测或加速证明。

<a id="lint-policy"></a>

首批显式继承成员为 `oasis7_agent_api`、`oasis7_client_api`、`oasis7_proto`、`oasis7_wasm_sdk`、`oasis7_game_client`、`oasis7_model_provider`：

- 只继承原有的 `edition=2024`、`license=MIT`；不集中 version/MSRV/publish，也不补原先缺失的发布属性。Model Provider description、Client API/Proto lib path 和 SDK crate-type 保持成员所有。
- 本批已有的 `hex`、`serde`（含原有 derive）、`serde_json`、`serde_cbor` 继承根版本；SDK 仍在成员声明 optional，`wire` 才激活三个 serde 依赖。Agent API/Client API 的 `sha2=0.11` 继承，Game Client 的 `sha2=0.10` 保留显式多版本。
- `reqwest=0.13.4` 仅共享版本和 `default-features=false`；Game Client 与 Model Provider 分别显式保留 blocking/json/rustls，不把 features 提升到根。其他依赖、kind、target cfg、alias 与 features 不动。
- SDK manifest 同时属于 37 个正式内置模块的本地源码闭包；[构建身份合同](../../p2p/consensus/builtin-wasm-identity-consensus.design.md)按原始 manifest 字节取摘要。共享 hasher 同时纳入各本地包由 Cargo 定位的 owning workspace manifest，避免声明移到虚拟根后漏检根继承配置变化；只覆盖这些 manifest，去重并使用相对路径标签，不增加依赖或扫描整个仓库。配置等价不表示 source identity 不变：通过现有 canonical sync 重建并刷新 m1/m4/m5 identity，核对 WASM 字节摘要与 recipe 的 build_manifest_hash 保持，不放宽 source/receipt 检查，也不发布或改写世界状态。
- `workspace.lints.clippy` 显式承接已采纳组的默认本地级别：correctness 为 deny、suspicious 为 warn；CI 的 `-D warnings -D clippy::correctness -D clippy::suspicious` 保留升级差异。不增加 pedantic/unsafe/unwrap 等规则，也不新增局部例外。

共性 lint 使用 `workspace.lints`，成员通过 `[lints] workspace = true` 加入；独立 workspace 不跨根隐式继承。先承接已经采纳的规则，本地与 CI 共享定义，CI 继续按现有命令升级 warnings。新增禁止 API、锁跨 await 等规则须说明具体风险、生产/测试/平台适用性；不顺带全仓禁止 `unwrap`、`expect` 或 `unsafe`。局部例外采用最小范围和技术理由，不能以关闭继承或 blanket allow 绕过共同规则，也不要求为每个例外另建任务。

<a id="architecture-rules"></a>
### 4.2 架构规则的唯一依据与计算

Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；承接 [REQ-RUST-002](rust-engineering-contracts.prd.md#req-rust-002)。

机器规则是现行专业合同的可执行投影，必须带上游路径和 anchor；专业 owner 仍拥有依赖与能力语义。crate 名称、目录层级和图上位置都不能独立推导禁止关系。公共 facade 只重导出唯一声明，优先用模块可见性、typed port 和最小公开 API 表达能力，检查器补充结构回归保护，不证明授权成立。

检查分两层：全成员 manifest 声明检查识别明确禁止的直接依赖，包括 optional 与平台限定边；真实支持组合的解析闭包检查识别间接引入。实际 package ID/source 用于归一 alias 和 workspace 继承。每条边保留 normal/build/dev、声明 cfg、optional/default/显式 features；build/proc-macro 按 host 语义检查，不能混同 target 运行时代码。

生产 root 的 normal/build 闭包不混入自身或其他 workspace 成员的 dev 图；测试闭包仅加入被选测试根及其所需测试能力，并保留生产与测试的归属。Cargo metadata 的 workspace feature 合并结果不能直接当每个 root 的发行图：实现需以固定工具链、显式选择的 root/target/features 和 `cargo tree` 等实际解析结果验证，声明视图和激活视图分别报告。[Cargo tree](https://doc.rust-lang.org/cargo/commands/cargo-tree.html) 单独的 `-e features` 会包含 dev；显式选择 kind 的图仍需与真实目标构建核对，不宣称等同所有编译单元。

<a id="boundary-matrix"></a>
### 4.3 首批方向矩阵

箭头表示左侧 package 依赖右侧 package；下表省略共同前缀 `oasis7_`。允许关系只列易误判的关键边，不是完整第三方白名单。禁止项同时检查适用的直接声明和传递闭包；原合同更强的能力限制仍由原 owner 验证。

| 受检查根 | 允许的关键关系 | 禁止范围 | 适用与原合同 |
| --- | --- | --- | --- |
| `game_client`、`agent_harness` | API/Proto/纯协议；Game Client → Client Storage；Harness native → Game Client | normal/build 不进入主 `oasis7`、Runtime、State、Projection、Service、Simulator、Viewer 实现 | [DES-AGC-001](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)；Game Client 协议/cache 单测也不通过 backend dev 依赖回拉，真实 World 集成留 Service |
| `world_state` | API、Proto、WASM ABI、CAS/Formal Protocol 及已有平台依赖 | Runtime、Service、Game Client、Simulator、Viewer、Chain host、WASM 执行器；不得经 Projection 绕回上层 | [状态边界](../../world-runtime/design.md#状态与服务投影的编译边界)；下层 codec 测试不靠上层执行实现。并不禁止必要文件系统能力 |
| `world_projection` | World State、API/Formal Protocol、**Game Client 验证核心** | normal/build 不进入 Runtime、Service、Simulator、主 `oasis7`、Viewer/Chain 宿主 | [投影边界](../../world-runtime/design.md#状态与服务投影的编译边界)；活 World 授权构造归 Service，不禁止已允许的 Game Client/HTTP 闭包 |
| `runtime` | World State、API/Proto、CAS/Formal Protocol、WASM Executor/Router/Store/Materializer | normal/build 不反向进入 Simulator、Service、Projection、Game Client、主 `oasis7`、Viewer、Node、DistFS、Chain host | [实现边界](../../world-runtime/design.md#实现边界与共享值所有权)与[宿主边界](../../world-runtime/design.md#1-设计定位)；World State `test-support` dev 边合法，Wasmtime 生产组合单列 |
| `launcher_backend` | Launcher Config/UI、协议、Observability、Hosted Access、Harness、Game Client、Model Provider | normal/build 闭包不包含 Runtime 或 Simulator，包括经主 `oasis7` 中转 | [Launcher 后端边界](../../world-runtime/design.md#1-设计定位)；不能把模型配置与进程编排误当世界执行 |
| `client_launcher` | Launcher Config、Client API、UI 与各 target 界面/HTTP 栈 | 继承现有检查的 `libp2p`、`ring@0.16.20`、`rustls-webpki@0.101.7`、`hickory-proto@0.24.4` | [现有脚本](../../../scripts/check-launcher-p2p-dependency-surface.sh)只证明实际执行的 host/无默认 features 范围；不扩大为禁止所有版本，也不伪称已经分离 dev 图 |
| `protocol_tools` | Client API、Proto、签名/HTTP；**dev → oasis7(default-features=false,test_tier_required)** | normal/build 不包含 Runtime/Wasmtime | [协议工具边界](../../world-runtime/design.md#1-设计定位)；dev 只承载真实 Runtime 签名互操作，不放行生产 |
| `provider_local_bridge` | Agent API、Client API、**Simulator 动作/事件 DTO**、Observability、LetAI adapter、WASM ABI | 不依赖主 `oasis7` 宿主，不转发 Wasmtime/libp2p/Viewer 宿主 features | [Provider 边界](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)；default features 为空，不套用到实际执行 World 的 `provider_tools` |

Service → Runtime/Projection/Game Client/Client Storage 是合法组合；主 `oasis7` 是宿主和兼容 facade。`client_cli` 中执行本地世界或解析权威 Simulator 配置的入口保留显式世界依赖；其**生产默认**不含 Wasmtime/libp2p 的要求不能被扩大为整个 package 禁止 Runtime/Simulator。以上是职责允许关系，不是待清理债务。

测试闭包的 kind 由各专业 owner 明确：Game Client、State、Client Launcher、Protocol Tools 包含 dev（后者仅精确互操作首边允许）；Harness、Projection、Runtime、Launcher Backend、Provider Local Bridge 的禁令限 normal/build。所有测试 profile 的构建及编译影响选择仍包含实际 dev 输入。

规则配置中的例外必须精确到 root、边/kind、target/features、用途和依据；稳定允许关系与临时迁移例外分别表达。临时例外另写复核/移除触发条件，同一 PR 按现有评审处理，不能使用 `allow_all_dev` 或通配全部路径豁免。原专业合同变化时同步改规则与正反例，不能仅为清除报错放松合同。

<a id="protocol-authority"></a>
### 4.4 协议权威与首个实施切片

Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc、O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01、O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；分别承接[产物治理](rust-engineering-contracts.prd.md#req-rust-003)、[唯一共享合同](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-003)和[API 合同一致性](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-012)的适用部分。

| 权威源 | 拥有内容 | 可派生范围 | 不能推断 |
| --- | --- | --- | --- |
| Agent API / Client API / Proto Rust 合同 | 字段、枚举、Serde 编码、纯 codec、签名/摘要原语 | 显式登记的具体根及其共享 components | 路由上线、身份资格、世界权限或全部泛型组合可用 |
| 当前 bootstrap OpenAPI 与服务 profile | 已开放路由、认证、错误、limits、请求收窄和 envelope | 本 profile 的 TS、独立 JSON Schema、发布文档 | 尚未实现的 `/v1/game` 写接口或完整世界对象 |
| 实际 parser/handler/codec 与 fixtures | 编码、拒绝、投影和签名行为 | 验证生成物与已实现语义一致 | 单靠 schema 合法推出授权/签名/世界效果正确 |

首切片使用现有人工维护 [read_bootstrap.openapi.json](../../../crates/oasis7_agent_api/src/game/read_bootstrap.openapi.json) 作为 HTTP profile 输入，生成 TS 类型与独立 JSON Schema；该 OpenAPI 仍是手工源，不被生成器覆盖。只列当前五条 GET 和 `writes_available=false`。`ReadInfo`/`ReadProjection`/`ReadError` 暂无唯一 typed Rust envelope，不能新造一套仅供生成的镜像 struct 来宣称单一真源。

首版消费验证按 fixture 的真实 GET/path 匹配唯一 operation，再确认实际 status 有声明，并使用该 status 的 schema 验证成功和错误 body；缺失、歧义或不匹配均失败。真实 handler fixtures 是独立输入，不能从待检 operation 反向生成期望。删除错误 status、改错 path 或错误 schema 关联须由反例检出。首版消费验证采用独立 TS 协议一致性样例：真实 import 生成类型并执行运行时校验，消费实际 handler 固定下来的成功与错误响应及独立负例。仅编译生成文件不构成消费验证。此样例不要求改造 Viewer、增加产品网络客户端或发布 SDK；真正触及产品调用、请求或重试行为时，再按原产品合同和开发流程选择适用验收。

后续有跨语言复用需要时，可先提取真实 handler 使用的唯一 typed DTO，以既有 JSON fixtures 证明同义，再将该 component 的输入权威迁到 Rust。profile 继续拥有 routes/auth/error/limits 与业务收窄；同一 component 的人工定义和生成定义不能同时成为可编辑真源。

Bootstrap `ReadWorldViewRequest.fixed_commit` 可缺省或为 null，实际 handler 拒绝非 null 的历史读；嵌套未知请求字段由严格 parser/`serde_ignored` 拒绝。响应 `ReadInfo`/`ReadProjection`/`ReadError` 保留原 `additionalProperties=true`，不能把请求严格性扩大为所有响应禁止扩展。底层 World Service DTO 可以表达更宽范围，不能覆盖 profile 收窄。动态投影 `data` 只导出实际已说明的有界 JSON/`unknown`；签名 proof header 在首切片保持 opaque hex string，不附赠未经验证的 JS 签名或 codec。

生成器是独立开发工具，依赖轻量合同/profile 输入；普通运行包不反向依赖它，不在服务启动、请求处理或常规 `build.rs` 自动运行。未来 schema/TS derives 若使用可选 feature，默认关闭且不得进入未选择生成能力的 normal/build 图。具体工具包名属于实施选择，工具职责和以下操作合同必须保持。

## 5. 关键运行流程

<a id="workspace-migration-flow"></a>
### 5.1 Workspace 迁移与结构检查

1. 固定基线、候选、工具链和锁文件，列出受影响成员、依赖边与支持组合；读取真实 Cargo 元数据，不能因缺缓存而默认完成。
2. 将同义共性移入根配置，保留成员的 kind、target、optional、alias 和 features。比较基线/候选的 source/version/feature 图，差异按配置错误或独立功能变更处理。
3. 实施或调用架构边界检查时，先检查声明，再对每个选定 root/host/target/features 检查激活闭包。规则要求的未注册组合报告 `coverage_gap`；解析、离线缓存或工具链失败报告 `inspection_error`。
4. 输出完整范围与可定位违规路径，按现有 CI 选择规则运行受影响验证。未通过的配置不标记迁移完成；取消或失败只留下可审阅工作区差异，不自动改 lockfile 或放宽规则。

纯 Workspace 配置迁移完成配置等价验证与现有必要检查即可，不以新增架构检查器可用为前置。

<a id="generation-flow"></a>
### 5.2 协议生成、检查与中断

1. 选择显式 profile，校验输入根、协议版本、工具/锁定依赖、输出清单与适用配置；冻结输入 digest。默认不访问网络或远端 schema。
2. 在临时目录生成完整输出，检查可解析性、本地 `$ref`、唯一根、精确整数、排序、fixtures、大小与文件集合。首切片对影响导出根语义且无法支持的 OpenAPI/JSON Schema 构造或引用明确失败；custom serde、泛型与 Rust adapter 的支持验证仅在 profile 实际登记 Rust component 后适用，不将未实现的 Rust 导出记为通过。
3. `check` 比较临时结果与已提交产物，报告 `stale`、`missing`、`extra` 或 `invalid`，不改仓库文件。`write` 在发布前重核输入与输出 preimage，冲突则终止，不能覆盖并发人工修改。
4. `write` 只更新明确受管文件；各文件采用临时文件替换，产物 manifest 最后写入。多个文件 rename **不构成多文件原子事务**；检查与打包必须核对完整集合和 digest，混合 generation 不得发布。
5. 中断后不自动重试发布；保留错误和冲突定位。维护者在原输入上重新完整生成，或通过普通 Git 修改恢复已知完整版本，再运行只读检查。临时输出不成为消费者输入，人工 profile 不被覆盖或删除。

## 6. 接口与数据合同

<a id="trusted-local-architecture"></a>
### 5.3 可信局部架构核查

Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84。

完整声明检查及全部登记 profile 的依赖图始终执行。普通源码变化的必需编译集合为：可信 BASE 选择器从 base/source/tested（含实际 merge parents）归属及嵌入输入得到的直接受影响包，与每个 profile 的实际 Cargo 图相交的 root/profile；图包括 normal/build，测试配置另含 root dev，host build/proc-macro 与 target 条件及 features 由 Cargo 解析。编译选择使用未豁免的完整图，精确互操作允许关系只影响违规判定，不能使构建输入漏选。未选中编译明确报告 `not_required`，图通过与编译成功分别记录，不能声称执行过未选项。

Cargo manifest/lock/build script/toolchain、检查器/规则/profile 或 CI 控制变化，显式 full，以及可信计划缺失、旧版或已登记输入解析不确定时，执行完整登记编译矩阵。未登记路径仍由可信 scope 明确拒绝；full 或候选配置不能用于自行登记。候选图解析失败返回 inspection_error，不借缩小集合规避失败。可信 BASE 旧选择器没有局部字段时完整执行，待登记合入后才启用局部收益。排障 `--profile`/`--graphs-only` 仍产生 coverage_gap，不能代替可信必需集合。

报告包含可信输入集合、选择理由、实际图/编译项及两阶段耗时。profile 的整体状态汇总图违规与构建结果；构建成功不能覆盖闭包违规。协议 handler/fixture 的 workspace-support 保留原完整选择，因为同时承担 Rust handler 与其他消费者回归，不仅是 TS 输入。

<a id="inspection-contract"></a>
### 6.1 检查输入、覆盖与结果

规则最小字段包括：稳定 rule key、上游 path/anchor、root package identity、禁止 package/source/version 条件、适用 kind、target/features、允许关系/精确例外、要求覆盖的组合。规则 revision 与工具版本用于复现；不新增任务身份。配置可集中维护，但每条语义只回链一个明确 owner，不能复制一份业务架构说明作第二真源。

支持组合从现有 manifest、平台合同和 CI 选择中承接，至少区分默认生产、轻量无默认 features、显式生产能力、`test_tier_required`、`test_tier_full` 和 `test-support`。`--all-features` 可能启用测试能力，不代表发行配置；不自动宣布所有 crate 支持 wasm。build host 与运行 target 分别记录；不适用组合带原合同依据，未验证组合不能改称不适用。

| 结果 | 判定 | 消费语义 |
| --- | --- | --- |
| `pass` | 指定规则与支持组合完整解析且无违规 | 只证明该范围；整体通过须所有必需组合通过且无缺口 |
| `violation` | 已证明禁入关系可达或直接声明违规 | 定位规则、root、禁止对象及依赖路径，返回非零 |
| `inspection_error` | 元数据、工具链、输入或缓存使检查未完成 | 不能变成 pass；修复环境/输入后重跑，返回非零 |
| `coverage_gap` | 必需组合未注册、未检查或检查范围不足 | 明示缺口；正式启用后的必需检查返回非零 |

每项诊断包含 source HEAD、lockfile digest、规则/工具版本、实际命令、root、host/target、features、edge kind、声明位置、上游条款和采用的例外；违规给出最短或足以定位的具体路径。不把完整巨大 cargo tree 当唯一诊断，不在报告中写凭据。声明检查通过与解析闭包通过分别呈现。

<a id="artifact-contract"></a>
### 6.2 Profile 与受管产物

每个 profile 登记名称、唯一权威输入、导出根及具体泛型实例、target/features、API/schema/协议版本、生成配置和格式版本、生成工具/依赖锁、受管文件相对路径。profile 不以 Cargo 包版本替代 wire 版本；`UnsupportedWireVariant` 等不可构造分支必须表达为不可构造，不降级为任意值。

首切片每个 profile 一个受管目录，输出集合至少为 `types.ts`、`schema.json` 和 `manifest.json`；确需拆分 schema 时由清单穷举。首切片的[生成器与 profile](../../../crates/oasis7_agent_api/protocol_codegen/README.md)及 `generated/` 受管目录位于 `crates/oasis7_agent_api/protocol_codegen/`，由既有 Cargo owner 覆盖；[独立 TS 消费样例](../../../crates/oasis7_agent_api/protocol_conformance/README.md)与生成工具分开维护。新增 profile 仍按[CI 接入约束](#runtime-constraints)核对可信路径归属。手工 OpenAPI 与受管目录分开，所有生成文件标明编辑入口。人工消费者、overlay 和 golden fixtures 不被写模式删除。

Manifest 至少包含格式版本、profile、wire 版本、导出根、相关输入的相对路径与内容摘要、生成工具/配置/锁定依赖摘要及各输出摘要；摘要算法固定为 SHA-256，manifest 不把自身摘要纳入递归计算。生成字节采用 UTF-8/LF、固定排序、稳定命名和确定的格式化工具，不含时间戳、绝对路径或整仓 HEAD。实际执行的源码 SHA 与环境留在 CI/PR 证据，不能让无关文档提交造成协议重生成。

输入摘要只覆盖实际影响该 profile 的输入。首切片覆盖人工 OpenAPI、生成配置/工具与锁定依赖；Rust components 成为输入后再纳入相关源码/Serde 属性及其传递输入。真实 handler/codec 测试负责发现源 profile 与运行行为不一致，不能用输入摘要代替它。修改生成器、工具链或相关依赖后重新验证确定性。

最小操作接口为 `write(profile, output_dir)` 和 `check(profile, output_dir)`，都接受显式支持配置并返回结果摘要与非零失败；可使用 CLI 包装。`check` 只读已有产物，写模式受管路径必须在仓库内，不跟随 symlink 越界，不允许清空未知目录。默认运行不触网；确需安装依赖由正常开发环境准备，失败不得静默改用未锁定工具。

<a id="wire-preservation"></a>
### 6.3 Wire、整数与兼容保持

Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b、O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；承接[API wire 一致性](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-012)与[安全调用](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-013)，不代替完整产品验收。

| 合同 | 必须保持 | 生成/消费约束 |
| --- | --- | --- |
| `DecimalU64` | [现有实现](../../../crates/oasis7_agent_api/src/game.rs)始终 `serialize_str`，包含 CBOR；只接受规范十进制字符串 | TS 可用 branded string，但仍需运行时验证；拒绝前导零、符号、数值 JSON、越界值，不能优化成 CBOR integer |
| Bootstrap 投影 | `world_binding`、`data` 内 i64/u64 递归转字符串，顶层 API revision 保持整数；任务控制中原 `u32` schema revision 也按投影转为字符串 | 不直接把原 `GameTaskControlV1` 的序列化当作 `TaskControlBootstrap`；以真实响应 fixtures 检验 |
| World Service | 裸 `u64`、签名 proof 内 decoded JSON 保持现有 numeric wire | 不默认把裸 u64 生成为可无损使用的 JS `number`；未有 lossless codec/golden 时保持 schema 描述或 opaque proof，不发布可调用签名 SDK |
| 精确 schema 数值 | `u64::MAX` 等边界不能在生成过程中被舍入 | 使用能保持 u64 的 Rust 解析或等效精确整数处理；不能普通 JS `JSON.parse` 后再 stringify 精确边界 |
| 字段与枚举 | `rename`、tag、flatten、default、skip、缺失/null、未知字段/状态处理均保持所属 profile | 不从 Rust `Option` 简化推出“总可省略”；不认识的 custom serde 使用显式 adapter 与 fixture，否则失败 |
| 签名与摘要 | World Service canonical CBOR 的 domain/value 结构、cognition digest、projection canonical JSON 各有原 owner | 不互换 JSON/CBOR canonicalizer，不改 domain、字段表示或原始投影；先验证完整值，再投影窄类型 |

人工 OpenAPI 的 `DecimalU64` 使用标准 JSON Schema pattern 表达规范十进制及精确上界，已知 task-control、GoalReference 和 ExecutorBinding 字段引用同一约束并保留正整数等局部收窄。生成器不从 description 猜测范围，也不维护第二份上界；动态 unknown JSON 不据此推断类型。生成 schema 和 TS 校验须接受 `u64::MAX`、拒绝其加一，并以原 codec/golden 独立核对。相对于原来的 pattern/20 位长度规则，这是机器接受范围的收窄，read schema revision 从 4 升到 5，API revision 保持 1；Rust JSON/CBOR codec 和五条 GET 不变。声明只接受 schema revision 4 的旧消费者应明确拒绝 revision 5，升级后重新验证；本次首次建立 TS 基线，没有旧 TS 版本可执行双向矩阵。

签名与规范编码依据为 [World Service authority](../../../crates/oasis7_client_api/src/world_service/authority.rs)、[canonical CBOR](../../../crates/oasis7_wasm_abi/src/canonical_cbor.rs) 和[投影承诺](../../../crates/oasis7_client_api/src/formal_projection.rs)。生成物只描述合同，不重新实现权限或世界规则。新增字段/枚举并不天然兼容，严格未知字段、版本协商和签名域均须双向验证。

## 7. 状态、事务与持久化

Workspace 和规则的长期状态由 Git 中的 manifest、锁文件、规则配置及原专业合同拥有；一次检查只产生其实际配置的结果。`pass` 不是功能发布、世界提交或历史覆盖证明。进度、失败与实际验证版本记录在 PR/CI，不在正文创建任务状态机。

生成过程具有“输入已核对 → 临时集已生成 → 完整集已验证 → 工作区已更新”的局部状态。只有完整 manifest 与全部文件摘要一致时，该工作区集合才可进入打包；只有正常 Git 提交后才形成可复现的仓库版本，提交不等于对外发布。输入或输出 preimage 变化使本次写入冲突，不重放旧输入覆盖新内容。同一输出目录不允许并发写入；实现使用互斥写入和最后的 preimage 重核，失去互斥或出现冲突即终止。

失败前尚未发布的临时集可删除；多文件更新中断可能留下不完整工作区，不能声称旧工作区字节天然保持。已提交版本仍可通过 Git 恢复；只读检查必须识别缺文件、混合摘要和未完成 manifest。需要运行期多文件原子切换的未来消费者必须采用完整不可变 bundle 与单点激活，不能依赖当前逐文件替换。

世界事务、任务控制、快照、journal、回执和客户端原操作恢复不由此工具持久化或修改，其 authority 继续是 [Runtime](../../world-runtime/design.md#状态与服务投影的编译边界) 与[客户端恢复合同](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-004)。本设计不会把生成/检查成功映射为这些运行状态。

<a id="runtime-constraints"></a>
## 8. 部署、安全与运行约束

工具在本地开发或既有 CI 的最小权限环境运行，只读取受检仓库、锁定依赖与显式配置，不需要服务凭据、签名私钥、玩家资料或真实世界目录。fixture 使用公开测试向量；错误诊断不打印 proof、密钥或私有目标。检查模式不写源码或产物，写模式仅修改已登记文件，拒绝越出仓库、路径穿越和 symlink 重定向。

不创建在线生成服务，不在 Runtime/Viewer 请求热路径执行 Cargo、schema 导出或网络下载。现有 bundled OpenAPI 与 `include_str!` 可继续使用稳定路径；运行包消费已提交或已打包的完整产物，不要求运行机器有仓库或生成器。工具解析/schema 处理必须有显式容量、递归和超时边界，失败返回检查不完整，不静默输出宽泛类型。

Bootstrap 仍只开放其实际 GET profile；生成 TS 不授予连接身份、Agent 资格、委托或世界写权，不修改原请求标识、重试、签名域和未知结果处理。授权、幂等及恢复的产品义务继续按 [REQ-EXT-013](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-013) 与[接口认证设计](../../world-simulator/llm/external-agent-game-api.design.md#api-auth)验证。

CI 接入复用现有 [scope 选择](../../../scripts/ci-required-scope.json) 和 [required/full 入口](../../../scripts/ci-tests.sh)。新增工具、规则、相关 Cargo/lock/toolchain、profile、生成物与 TS 消费样例应有准确触发关系，包含检查器自身和反例；共享控制变化继续遵循原扩大覆盖规则。普通文档修改仍按现行 scope 处理。采用本设计不自动新增 required check 名称、放宽门禁或改变合入权限。

首次接入先以当前可信基线核对上述路径归属。已有 Cargo owner 或可信 scope 规则覆盖的路径可在正常实施 PR 中接入；确需新增未登记独立路径时，依现行[未登记路径合同](../../testing/ci/ci-tiered-execution.design.md#ci-unknown-inputs-design)先合入规则/验证入口登记，再提交使用该路径的实现。候选自登记、混合共享控制变更或全量请求不能替代可信基线登记，不得改用候选选择器绕过。此条件不要求所有切片机械拆分 PR，CI 控制变更仍按现行规范独立评审。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 配置归一 | 固定 Rust/lock，受影响 root、真实 native/wasm 与 feature 组合 | package/source/version/feature/kind 语义一致 | [AC-RUST-001](rust-engineering-contracts.prd.md#ac-rust-001) 的零非预期差异 | 基线/候选 metadata 与 feature 图比较、受影响构建 | 实施后验证；本设计不报告性能收益 |
| 边界检查 | 首批规则、声明全集及登记支持组合；host/target 分开 | 正例通过，直接/间接/build/alias/平台反例命中 | [AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002)，所有必需组合无缺口 | RG-A01～RG-A04 | 首版已接入；范围与缺口按候选 JSON/CI 核验 |
| 生成确定性 | 相同输入、锁定工具，不同临时路径/工作区；首切片全产物 | 输出集合、内容和摘要相同，check 无工作区写入 | [AC-RUST-003](rust-engineering-contracts.prd.md#ac-rust-003)，字节差异为零 | RG-P01、RG-P02 | 首版已实施确定性与故障回归；运行结果留 PR/CI |
| 数值与严格 wire | `0`、`2^53-1`、`2^53`、`u64::MAX`、越界/非规范输入、null/缺省 | 精确保持既有编码与拒绝语义 | 原 wire 与 profile；不得由生成器扩宽 | RG-P03、现有 codec/handler tests | 已有 TS/真实 handler 验证；不代表未登记 profile |
| 容量与故障 | profile 明确登记输入/输出字节、schema 深度、根数量与执行超时上限；上限内最大样例及超限/中断注入 | 有界完成或明确失败，禁止部分集发布 | 实施 owner 以实际 profile 和 CI 资源确定有限上限；首版启用前必须填写并测试 | RG-P02；临时目录/冲突故障注入 | 不虚设无基线吞吐或耗时目标 |

检查开销先记录固定环境的实际耗时、峰值内存与输入规模，必要时再建立回归阈值。配置集中或 crate 分层不直接证明编译变快；如主张收益，沿用现有编译测量方法，固定 toolchain/profile/features/jobs/target/缓存条件分别测量。

<a id="migration-rollback"></a>
## 10. 兼容、迁移与回滚

Workspace 归一、边界检查与 Bootstrap 产物可独立或并行推进；下列条目描述各方向内部的迁移与启用步骤，不建立三项之间的串行前置。检查器须同时支持显式与 workspace 继承声明；协议生成不依赖全仓配置归一。只有具体输入依赖或既有 CI 首次路径登记产生先后关系，不要求新的任务身份或一次完成全仓治理：

1. **同义配置与共性 lint**：成员/依赖迁移和已采纳 lint 继承分别满足其等价与诊断验证后纳入所选切片，不互设前置。保持工具链、resolver、profiles、patch、依赖来源与有效 feature 不变，同步维护受影响的 manifest 消费者；未迁移成员继续显式声明。回滚该切片只恢复配置和对应工具/规则，不夹带协议变化。
2. **结构规则与覆盖**：先实现声明/解析视图、正反例和机器诊断，在开发环境核对真实支持组合与现有合法关系。覆盖未完成的部分保持明确未启用；完成后通过正常 CI 变更接入。新规则达到原 Launcher 检查的实际覆盖并保留其反例前，不移除旧检查；遇到误报应修正规则/解析或撤回新接线，不能放宽上游架构。
3. **Bootstrap 产物**：保留并补齐人工 OpenAPI 的机器约束，增加受管 TS/schema/manifest 与只读漂移检查，接入前述 TS 协议一致性样例的编译及运行时校验。首切片不替换现有 signer/codec；只有完整集合通过才可打包。工具回滚恢复上一已知完整生成集合与匹配的生成配置。
4. **按需求扩展**：只有明确跨语言消费者时才提取 handler 使用的唯一 typed DTO 或登记新的具体 Rust profile。先保持 golden wire，再迁移 component 来源；生产包默认不引入生成工具依赖。不同 target/features 若改变 wire，必须成为显式命名 profile，不能随生成机器改变输出。

生成漂移、协议兼容和部署版本是不同维度。纯产物修正不自动增加 API revision；实际 wire 变化按原接口 owner 的版本协商、旧读新/新读旧、未知枚举和签名兼容合同处理。增字段不被自动认定为兼容，也不把 Cargo package 版本当 API 版本。

本专题的首切片不迁移世界数据或持久化格式，回滚不需要世界状态回退。后续真实协议变化若已被外部消费者或持久数据采用，不能以回滚生成文件恢复兼容；必须按原协议/存储 owner 处理旧消费者与旧数据，再决定可回退范围。已经发布的产物不可通过改写 Git 历史撤销。

## 11. 验证设计与可追溯性

下表是稳定验收设计，`RG-*` 为本专题场景标识，不表示同名测试已存在。实施 PR 必须给出实际测试路径、候选提交、支持配置、执行环境与结果；现有测试文件只证明其原范围。文档 PR 只做文档检查和语义审阅，不宣称以下实现验证已运行。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；[AC-RUST-001](rust-engineering-contracts.prd.md#ac-rust-001) | [Workspace 合同](rust-engineering-contracts.design.md#workspace-policy) | 归一不改变本次受影响配置语义 | [Workspace 等价验证](rust-engineering-contracts.manual.md#workspace-equivalence)；RG-W01（实施后执行）：固定基线/候选与工具链，生产/测试使用分开的 kind、实际 root/target/features 与真实目标构建核对；覆盖变更涉及的 native/wasm 和 required/full | 实施 PR 的逐配置差异、完整命令与真实 CI | lockfile 无 diff、单次 metadata 或显示过滤不证明所有编译单元或性能 |
| Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；[AC-RUST-001](rust-engineering-contracts.prd.md#ac-rust-001) | [Lint 继承](rust-engineering-contracts.design.md#lint-policy) | 共性规则可继承，例外与告警升级范围明确 | [Lint 验证](rust-engineering-contracts.manual.md#workspace-lints)；RG-W02（实施后执行）：核对已迁移成员、相同 target/features 的本地/CI 诊断和测试局部例外 | 实施 PR 的规则选择、诊断和定向结果 | 不把普通 clippy 视为全 CI，不要求无关成员迁移 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | [架构规则](rust-engineering-contracts.design.md#architecture-rules) | 直接、间接、host build、alias 与 feature/target 不能绕过 | [架构正反例](rust-engineering-contracts.manual.md#architecture-regressions)；RG-A01（检查器实施后执行）：真实最小 Cargo workspace 注入 normal/中转/build/optional、Windows/wasm、别名与继承边，固定 Cargo 实际解析 | 检查器真实解析测试与候选 CI artifact | fake Cargo 参数测试不证明实际图；未支持组合单列缺口 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | 合法组合通过，测试互操作不放行生产 | [方向与例外验证](rust-engineering-contracts.manual.md#architecture-regressions)；RG-A02（实施后执行）：核对 Projection/Service/Provider 合法边；Protocol Tools dev 通过而同边 normal 失败；production/test-support 分开 | 规则正反例与真实候选报告 | 不证明服务授权或真实 Runtime 互操作行为 |
| Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；[AC-RUST-002](rust-engineering-contracts.prd.md#ac-rust-002) | [检查合同](rust-engineering-contracts.design.md#inspection-contract) | 不完整检查不能通过；CI 覆盖工具自身和输入 | [检查覆盖与接线验证](rust-engineering-contracts.manual.md#architecture-scope)；RG-A03/A04（实施后执行）：注入缺 lock/root、解析失败和覆盖缺口，验证非零状态及 scope/required 接线 | 负例、selector/gate 测试与实际检查记录 | 接线 fixture 不代替真实依赖图 |
| [Runtime 状态与投影边界](../../world-runtime/design.md#状态与服务投影的编译边界) | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | 下层状态、投影与 Runtime 保持原依赖方向 | [架构 owner 复核](rust-engineering-contracts.manual.md#architecture-regressions)；RG-A02：对候选 rule 逐行核对专业合同、实际 manifest、测试归属和支持组合，审阅误报/漏报反例 | 实施 PR 的 Runtime owner 审阅与规则 diff | 不证明 patch 权限、snapshot 恢复资格或重编收益 |
| [宿主与轻量入口](../../world-runtime/design.md#1-设计定位) | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | 宿主职责与生产/测试例外的范围准确 | [轻量与宿主例外复核](rust-engineering-contracts.manual.md#architecture-regressions)；RG-A02：核对 Launcher、Protocol Tools、Client CLI、Provider tools 的生产/测试图，保留原 Launcher 版本限定 | 实施 PR 的接口/宿主 owner 审阅及依赖报告 | 不声称所有 CLI 都无世界依赖或所有 target 已覆盖 |
| Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；[共享客户端要求](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-003) | [方向矩阵](rust-engineering-contracts.design.md#boundary-matrix) | Game Client/Harness 隔离，真实世界测试留在适当 owner | [客户端隔离验证](rust-engineering-contracts.manual.md#architecture-regressions)；RG-A02：按真实 manifest/target/features 构建 SDK/Harness，核对无世界/Viewer 回流和 backend dev 绕路 | 实际构建/边界报告与代码审阅 | 不证明完整内外执行方等价、玩法闭环或全部平台 |
| Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；[共享客户端要求](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#req-agent-parity-003) | [协议权威](rust-engineering-contracts.design.md#protocol-authority) | 共享 wire 唯一声明，未来 typed component 参与真实 handler | [共享 wire 所有权验证](rust-engineering-contracts.manual.md#wire-compatibility)；RG-P04：核对 DTO/codec 来源、兼容再导出和真实调用点，拒绝只为 schema 复制的镜像类型 | 实施 PR 的源定义/消费者 diff 与相关 golden | 静态复用不证明全部业务行为等价 |
| Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc；[AC-RUST-003](rust-engineering-contracts.prd.md#ac-rust-003) | [产物合同](rust-engineering-contracts.design.md#artifact-contract) | 可重现、check 只读、完整集合与失败恢复 | [产物生成验证](rust-engineering-contracts.manual.md#protocol-generation)；RG-P01/P02（生成器实施后执行）：两临时路径比较全字节；增删改、输入冲突、中断、symlink、未知 serde/ref 与容量边界，核对人工源和混合集保护 | 生成器测试、manifest/diff、候选 CI 与首次 scope 接线 | 不证明 wire 兼容、服务可用或多文件原子发布 |
| Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；[游戏 API 要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-012) | [协议权威](rust-engineering-contracts.design.md#protocol-authority) | Bootstrap 类型、版本、数值与实际服务响应一致 | [Bootstrap 一致性验证](rust-engineering-contracts.manual.md#bootstrap-conformance)；RG-P03：复用现有 game_wire/handler tests，验证五 GET、revision/limits、fixed_commit、未知字段与递归投影；真实 TS 消费者编译和正负响应校验 | profile fixtures、消费者测试与候选 CI | TS 编译不证明网络可信；首切片不证明完整 Game API |
| Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc；[AC-RUST-003](rust-engineering-contracts.prd.md#ac-rust-003) | [Wire 保持](rust-engineering-contracts.design.md#wire-preservation) | 原 JSON/CBOR、字段缺省和签名字节保持 | [Wire 兼容验证](rust-engineering-contracts.manual.md#wire-compatibility)；RG-P04：按受影响 profile 运行原 wire/World Service/signing fixtures，覆盖数值边界、旧读新/新读旧和生成表达；未触及 profile 不扩写覆盖 | 固定 golden 字节、消费者版本与真实测试结果 | schema diff 不证明签名/权限，未有 codec 不声明跨语言签名 |
| Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；[安全调用要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-013) | [运行约束](rust-engineering-contracts.design.md#runtime-constraints) | 生成与消费者不改变 parser、认证和原操作恢复 | [安全语义保持验证](rust-engineering-contracts.manual.md#security-preservation)；RG-P05：复用真实 handler 重复 key、未知字段、深度/字节、历史读和签名负例；触及请求/重试时验证原标识、未知结果与拒绝语义 | 接口/客户端实施 PR 的定向行为测试与审阅 | 不由生成成功推导全部鉴权、远程恢复或世界最终性 |

设计/导航变更使用现有 `scripts/doc-governance-check.sh` 与 `git diff --check`，并人工复核 authority、相对路径/anchor、状态措辞和例外范围。实现变化复用仓库现有 required/full 选择，不把文档检查、fixture 或局部编译扩写为端到端交付证据。

## 12. 决策、长期风险与未决问题

| 决策 | 选择与理由 | 代价、owner 与重新判断触发 |
| --- | --- | --- |
| 共性集中，差异显式 | 只迁移可证明同义的配置，保留独立版本与 feature/target；避免一次全仓功能变化 | 过渡期仍有显式声明；engineering/crate owner 在实际触达或工具链变化时复核 |
| 原合同驱动结构规则 | 声明检查加真实支持组合，保留服务复用与 dev 互操作；不采用统一层级禁入 | 需要维护覆盖和反例；模块 owner 改变边界或支持矩阵时同步复核 |
| Bootstrap profile 先行 | 当前 OpenAPI 拥有真实 HTTP 收窄与动态 envelope；先派生 TS/schema，未来再迁移唯一 typed component | 暂保留人工 OpenAPI 与 handler 的一致性测试；接口 owner 在真实消费者需要复用时决定组件提取 |
| 生成与运行分离 | 开发期工具、显式 profile、提交完整产物，运行时无生成依赖 | 增加开发期工具维护；engineering 与接口 owner 在新增 profile/consumer 时评估输入闭包和依赖成本 |
| 三类证据独立 | 漂移、wire/兼容、服务语义分别验证，禁止以一种成功代替其他 | 测试按受影响合同选择；协议 owner 在编码/消费者变更时复核兼容方向 |

首个 HTTP profile 使用独立 Python 3.12 标准库工具，生成/check 入口及冲突恢复见[生成器说明](../../../crates/oasis7_agent_api/protocol_codegen/README.md)；导出根、有限容量/深度/超时及目标配置由 [profile.json](../../../crates/oasis7_agent_api/protocol_codegen/profile.json)登记，实际输入与输出摘要归生成 manifest。TS 消费者使用独立锁定的 TypeScript/Ajv 工具，验证方法见[手册](rust-engineering-contracts.manual.md#protocol-generation)。本节采纳这些稳定入口与职责，不代签一次运行或部署结果；未登记的 Rust components、Viewer/World Service 导出根仍不属于首版承诺。

长期风险包括规则漂移、feature 合并造成误报、custom serde 与 schema 不一致、精确整数在 JS 处理中丢失，以及严格消费者拒绝新增字段。对应控制是原 owner 的单一依据、按真实配置解析、显式 adapter/fixture、lossless 边界和双向兼容验证；无法证明的部分继续显示缺口，不降级为静默成功。

### 参考与适用边界

设计借鉴 [openai/codex@d6817454d76885e9d0e118bbf82e507aed6f715a](https://github.com/openai/codex/tree/d6817454d76885e9d0e118bbf82e507aed6f715a) 的 [workspace 共性](https://github.com/openai/codex/blob/d6817454d76885e9d0e118bbf82e507aed6f715a/codex-rs/Cargo.toml)、[manifest 检查](https://github.com/openai/codex/blob/d6817454d76885e9d0e118bbf82e507aed6f715a/.github/scripts/verify_cargo_workspace_manifests.py)、[TUI/Core 边界检查](https://github.com/openai/codex/blob/d6817454d76885e9d0e118bbf82e507aed6f715a/.github/scripts/verify_tui_core_boundary.py) 和 [app-server-protocol](https://github.com/openai/codex/tree/d6817454d76885e9d0e118bbf82e507aed6f715a/codex-rs/app-server-protocol) 的协议产物实践。采纳的是共性配置、可执行边界与可重现产物的方法；具体依赖、profile、版本、测试和合入条件以上述 oasis7 主责合同为准。
