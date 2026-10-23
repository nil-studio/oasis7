# Rust 工程契约需求

- 专业条款：`PRD-ENGINEERING-036`
- 状态：`active`，目标合同；设计覆盖与实现状态分别判断。
- 主责：engineering 维护者；各 crate、Runtime 与对外接口 owner 维护各自语义。
- 上游：[工程域需求](../prd.md#PRD-ENGINEERING-036)
- 技术设计：[Rust 工程契约系统设计](rust-engineering-contracts.design.md)
- 实施验收：[验证手册](rust-engineering-contracts.manual.md)，方法与执行结果分别维护。
- 审读基线：`nil-studio/oasis7@2e21481a95614b42727abaddd1f2bc55008d791a`。

## 1. 目标与适用边界

开发者应能在明确位置维护共享 Cargo 配置，知道哪些依赖关系违反现行架构，并能复现对外协议产物。评审者应能区分配置去重、实际能力变化、生成漂移和协议兼容性，避免把其中一项检查当成全部正确性的证明。

本专题覆盖第一方 workspace 的共性配置、已有架构边界的检查方法，以及已登记协议 profile 的派生产物。独立 workspace、第三方代码、运行时权威、发布权限与 required/full 门禁仍由原主责维护。当前根 workspace 排除的 `tools/oasis7-command-center` 保持独立，不能隐式继承根 workspace 或为统一配置而被并入。

产品适用性：N/A；reason=三项新增要求拥有开发期治理机制，不新增玩家功能或发布承诺；scope=workspace 配置、依赖检查和生成生命周期。机制消费的既有游戏接口、共享客户端与授权合同仍然适用，具体承接见[设计需求分配](rust-engineering-contracts.design.md#21-需求承接与分配表)，不以工程 N/A 排除它们。

### 非目标

本专题不要求一次性迁移所有 crate，不合并独立版本策略，不升级 Rust、resolver 或依赖，不重写现有 Runtime 分层，不将所有客户端包视为同一种能力边界，不新增游戏 API 写路由，也不改变 JSON/CBOR、签名或世界提交语义。不引入新的任务身份、审批链、强制 Issue 或历史整改台账。

## 2. 稳定要求与验收

<a id="req-rust-001"></a>
### REQ-RUST-001：Workspace 共性有唯一维护位置，迁移保持构建语义

Trace ID: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf

共享依赖来源、版本和已采纳的共性 lint 应由 workspace 配置承载，成员显式继承；具有独立语义的包属性、feature 接口、平台依赖和测试能力继续由成员拥有。归一配置前后，受影响构建配置的实际依赖身份、来源、版本、feature 和 target 条件必须等价。配置去重本身不得启用新能力、升级依赖或扩大生产依赖闭包。

产品适用性：N/A；reason=本要求约束 Cargo 维护方式与构建语义；scope=第一方 workspace 及其显式迁移范围。

<a id="ac-rust-001"></a>
#### AC-RUST-001：可验证的同义迁移

Trace refs: O7-REQ-3dae99ae-2b00-4ba4-b5fd-4311965733cf；对应 [REQ-RUST-001](#req-rust-001)。

1. 以 Cargo 识别的 workspace 成员确定归属；对本次迁移涉及的成员和声明说明继承与保留本地配置的理由，独立 workspace、vendor 和第三方代码不被错误纳入。
2. normal、build、dev、target 条件、optional、依赖别名与有意保留的多版本关系不因归一而改变；共享 features 不取所有消费者需求的并集。
3. 在仓库固定工具链上，对受影响 root/target/features 比较基线与候选依赖图；`Cargo.lock` 无变化不是 feature 等价的充分证据。直接读取 manifest 的现有安全检查、CI 规划、构建与打包工具须继续理解本次采用的声明形式，保持原检查语义和失败条件。
4. 本地和 CI 采用同一共性 lint 定义，CI 的目标选择与告警升级策略继续明确。新增更强 lint 必须独立说明适用范围和存量影响，不能借继承配置静默全仓启用。
5. 未验证的成员或构建组合如实保留原声明与覆盖缺口；不得因部分迁移宣称全 workspace 已归一或编译必然更快。

<a id="req-rust-002"></a>
### REQ-RUST-002：现行架构边界可检查、可定位，合法组合有准确范围

Trace ID: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84

依赖规则必须追溯到现行模块合同，按真实 package identity、依赖方向、kind、target 与 feature 检查直接声明和激活的传递闭包。合法的服务组合、DTO/验证核心复用与测试互操作必须保留其准确作用域；不能用目录名或统一禁止 dev 依赖替代职责判断。

产品适用性：N/A；reason=本要求新增依赖检查机制，不重定义世界执行与客户端产品能力；scope=结构规则的表达、覆盖和诊断。被检查的客户端产品合同继续由原要求与专业设计拥有。

<a id="ac-rust-002"></a>
#### AC-RUST-002：结构违规与检查不完整可区分

Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84；对应 [REQ-RUST-002](#req-rust-002)。

1. 每条规则包含上游条款、受检查根、禁入对象、依赖 kind 和支持组合；重命名依赖、workspace 继承、间接边与 build 边不能绕过检查。
2. Game Client/Harness 隔离、Runtime/State/Projection 边界和 Launcher/协议工具轻量边界按[方向矩阵](rust-engineering-contracts.design.md#boundary-matrix)分别表达。Projection/Service 复用 Game Client、Provider bridge 消费 Simulator DTO，以及协议工具 dev 互操作不得误报。
3. 默认生产、无默认 feature 的轻量配置、显式生产能力、required/full 测试能力分别验证；只报告实际支持和执行的组合，不能以 `--all-features` 代替生产证明。
4. 输出至少区分 `pass`、`violation`、`inspection_error`、`coverage_gap`，包含规则、root、host/target、features、可定位依赖路径和上游依据；解析失败不得返回通过。
5. 启用检查前验证真实正例、违规反例及 CI 接线；结构通过不替代运行时权限、持久化、世界结果或跨入口恢复验收。

<a id="req-rust-003"></a>
### REQ-RUST-003：协议产物可重现，权威源、服务范围与兼容性明确

Trace ID: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc

对外类型、schema 和文档产物必须能由明确的 wire/profile 输入重现，并检测遗漏生成、手改生成物和不完整输出。共享 Rust 合同拥有结构与 codec，接口 owner 拥有已开放路由、认证、限制与 profile 收窄；生成器不构成新的业务真值，不能从 public DTO 推导功能已上线。

首个实施切片限定为现有 signed-read bootstrap profile：保留其人工维护 OpenAPI，派生 TypeScript 与独立 JSON Schema，并以真实 parser/handler/codec fixtures 校验。其他 Rust 根类型、Viewer、World Service profile 后续按实际消费者登记，不因本设计存在自动纳入。

产品适用性：N/A；reason=本要求拥有开发期生成与漂移检查机制；scope=已登记 profile 的派生产物管理。既有游戏 API、编码、鉴权和恢复要求通过设计承接，仍须独立验证。

<a id="ac-rust-003"></a>
#### AC-RUST-003：完整产物集与三类验证

Trace refs: O7-REQ-5f1aded7-fa06-40f7-a3ff-7a42a2cbefcc；对应 [REQ-RUST-003](#req-rust-003)。

1. 每个 profile 登记权威输入、导出根、适用 target/features、协议版本、生成工具及输出集合；包版本、API revision、schema revision 与 Viewer protocol version 各守其语义。
2. 同一受支持输入与锁定工具产生字节一致的结果；生成内容不含时间戳、绝对路径或无关提交引起的版本噪声。`check` 不修改仓库，`write` 在临时目录完成验证后更新受管文件。
3. 缺失、额外、过期、混合 generation、无法解析的引用及未支持的输入语义必须明确失败；首切片检查实际采用的 OpenAPI/JSON Schema 构造，custom serde 与泛型验证仅适用于已登记 Rust component 的 profile。不得覆盖人工输入，或用 `any`/宽泛 schema 掩盖本应精确表达的类型。
4. Bootstrap 的五条 GET、只读声明、`fixed_commit` 可缺省或为 null、严格未知请求字段处理与递归整数字符串投影保持现有范围；非 null 历史读仍被拒绝，响应对象保留各自允许扩展的合同。人工 OpenAPI 补齐 `DecimalU64` 的可机读上界后再派生，使 schema 与真实 codec 一致拒绝越界。动态 `data` 的未定义结构如实使用有界 JSON/`unknown`，不声称完整世界 schema。
5. 分别验证生成漂移、wire/消费者兼容性和真实服务语义。`DecimalU64`、裸 `u64`、字段缺省、enum、规范 JSON/CBOR 与签名域保持原行为；未具备 lossless codec 与跨语言 golden 的 profile 不发布可调用的 JS 签名/编解码能力。
6. 中断、并发修改、写入冲突与回滚有明确处理；未形成完整且校验通过的产物集不得打包发布。生成成功不证明实际部署、授权成功或世界效果。

## 3. 设计覆盖与实现状态

| 要求 | 系统设计覆盖 | 实现基线 | 剩余范围与证据入口 |
| --- | --- | --- | --- |
| [REQ-RUST-001](#req-rust-001) | 已覆盖：[Workspace 合同](rust-engineering-contracts.design.md#workspace-policy)、迁移与验证 | 部分基础：根 `Cargo.toml` 已有少量共享依赖，尚无统一 package/lint 继承 | 逐批同义迁移、配置检查与 lint 继承；以实施 PR 的候选图和实际 CI 为证据 |
| [REQ-RUST-002](#req-rust-002) | 已覆盖：[边界规则](rust-engineering-contracts.design.md#architecture-rules)、方向矩阵、覆盖与诊断 | 部分基础：已有专业边界与 Launcher 局部依赖检查，未有本设计的通用检查器 | 规则实现、真实配置矩阵、正反例和门禁接入；不能从现有单一检查推导全平台覆盖 |
| [REQ-RUST-003](#req-rust-003) | 已覆盖：[协议权威与首切片](rust-engineering-contracts.design.md#protocol-authority)、生成与兼容生命周期 | 部分基础：已有 Rust wire、bootstrap OpenAPI 与协议测试，未有本设计的完整 TS/schema 生成闭环 | 首切片生成器、消费者校验、完整集检查与 CI；其他 profile 仍需独立登记和证明 |

上表以[固定基线与源文件](rust-engineering-contracts.design.md#baseline-gap)为实现依据，只说明本专题覆盖范围。工程设计的“已覆盖”不表示实现、CI 或相关产品 AC 已完成；执行进度与验证结果保留在 PR、实际 CI 和评审记录。

## 4. 成功判定与实施原则

每个实施切片应能独立审阅和回滚，并准确列出受影响配置、消费者、验证与缺口。Workspace 归一、边界检查、协议生成可分别推进，不要求完成全仓重构后才能取得收益。

本专题的文档交付以要求、设计、索引和现行文档检查为范围。自动检查只有在实现、反例、作用域和 CI 接线完成后才按现行流程生效；设计采纳不自行改变 required/full 或合入条件。任何性能收益使用既有测量方法和固定配置证明，不设置没有基线的百分比目标。

## 5. 相邻权威与维护触发

- 通用开发、验证与交付：[开发流程规范](../workflow/source-of-truth.md)。
- 文件体量与结构切片：[Rust 1200 行治理](rust-1200-line-root-cause-governance-2026-03-29.prd.md)，本专题不替代其扫描合同。
- Workspace 成员、工具链、共享依赖或 lint 政策改变时，复核 REQ-RUST-001 的适用与等价证明。
- 架构 owner 改变边界、测试归属或支持组合时，同步复核规则及反例；不能仅为通过检查放宽原合同。
- 服务 profile、wire 编码、生成工具、导出根或跨语言消费者变化时，复核产物集合、兼容方向与验证计划。
