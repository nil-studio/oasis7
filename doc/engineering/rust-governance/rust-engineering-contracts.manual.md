# Rust 工程契约验证手册

- 适用需求：[Rust 工程契约需求](rust-engineering-contracts.prd.md)。
- 技术依据：[Rust 工程契约系统设计](rust-engineering-contracts.design.md)。
- 本文维护各实施切片的验收方法；Workspace 首批继承范围见系统设计，新架构检查器和协议生成器仍需实施。下列操作不表示均已有自动入口，也不记录一次运行通过。
- 通用验证范围、开发缓存与正式命令沿用 [testing-manual.md](../../../testing-manual.md) 和[开发流程规范](../workflow/source-of-truth.md)，不新增 gate、审批或强制任务。

## 执行前置与结果归属

执行者先在实施 PR 确认候选工具已就绪，记录基线/候选提交、固定 Rust、锁文件、host、target、root、features 和实际命令。尚无工具或所需环境时记录未执行范围，不填 PASS，也不将其写成不适用。
只选择本次受影响的配置与消费者；不同 target/features 的结果分别记录。生产、测试、fixture、真实服务与部署证据不得互相替代。
本地日常反馈使用 `scripts/cargo-dev.sh`；首次本地编译按现有约定准备 target，保留 worktree 隔离和缓存规则，不另改 `CARGO_TARGET_DIR`。正式验收使用现有 canonical 命令与 CI 选择。
以下 `ROOT`、`TARGET` 是必须替换的占位符；原配置启用的 `--no-default-features`、`--features` 和 target 选择必须一致保留。新工具命令由实施 PR 提供，不在本手册虚构可执行名称。
实际输出、失败、未覆盖组合及恢复结果存于 PR、CI 和适用验证记录；本手册只维护方法。配置迁移不以新架构检查器可用为前置。

<a id="workspace-equivalence"></a>
## RG-W01：Workspace 配置等价

1. 在基线与候选的对应 checkout 读取 `cargo metadata --offline --locked --format-version 1`，核对真实 workspace 成员、package/source/version、依赖别名和声明 kind。独立 workspace 单独处理，保留并记录两侧相同的 Cargo 环境与 wrapper 配置。
2. 按实际支持组合逐个选择 root；metadata 的 workspace 合并结果不能代替单个 root 的发行依赖图。生产图与测试图分别使用下面的 Cargo 命令模板。

```bash
cargo tree --offline --locked --package ROOT --target TARGET --edges normal,build,features
cargo tree --offline --locked --package ROOT --target TARGET --edges normal,build,dev,features
```

3. 生产比较包含 normal/build/features，测试比较包含所选测试根的 normal/build/dev/features；同时核对 host build/proc-macro 与运行 target。不能把其他 workspace 根的 dev features 混入生产结论。
4. 对默认、无默认、显式生产 feature 及受影响 required/full/test-support 组合，比较实际启用 features、source/version、optional 激活和 target 条件。别名及有意多版本按 package identity 归一，不按显示名猜测相等。
5. 使用相同 root/target/features 的真实 check/build 和测试构建交叉核对解析结果；生产选择实际 lib/bin，测试按既有入口构建并执行受影响用例。不可运行的跨 target 测试只报告已完成的构建范围。
6. 核对 Model Provider 的默认 feature、Launcher native/wasm 差异、WASM SDK 的可选 wire，以及本次触及的 formal producer/host feature 别名；不能仅用 lockfile 无 diff 判定等价。
7. 核对受影响的原始 manifest 消费者，包括安全来源检查、CI 规划、构建与打包脚本；显式和继承配置均保留原断言与失败条件。若迁移 libp2p，验证[现有来源检查](../../../scripts/libp2p-security-source.test.py)仍检查有效 git/rev、成员 optional/平台条件。保留 manifest 变化的现有共享控制覆盖，不以纯去重规避 CI 或保证缓存命中。

通过条件：所选配置没有非预期 package/source/version/kind/feature 变化，真实构建与适用测试一致。离线缓存、工具链或解析失败属于未完成；通过正常环境准备补齐后重跑，不静默去掉 `--offline`/`--locked` 或更新锁文件。
本场景不证明性能提升。若主张编译收益，另复用现有 compile metrics，并固定 profile、jobs、target 与缓存条件。

<a id="workspace-lints"></a>
## RG-W02：Lint 继承与局部例外

1. 核对本次迁移成员显式采用 `[lints] workspace = true`，独立 workspace 保持自身配置；比较基线和候选共性规则，不把新增强规则当配置去重。
2. 在相同 root/target/features 下分别运行本地 clippy 和 [CI 入口](../../../scripts/ci-tests.sh) 的实际 flags；当前 CI 使用 `-D warnings -D clippy::correctness -D clippy::suspicious`。记录诊断集合及级别差异，区分相同规则与 CI 告警升级。
3. 在隔离 checkout 的临时 example 中引入真实代码，使用成员的实际 `[lints] workspace=true`，不要复制一份规则冒充继承。可用 `Add` 实现内做减法触发 `clippy::suspicious_arithmetic_impl`（本地 warning、成功；CI error、失败），用变量自比较触发 `clippy::eq_op`（本地/CI 均 error、失败）。分别固定基线与候选的诊断 code/level/退出码；修正代码后两种入口均须成功，并删除临时 example。不得只用 stub 参数记录证明 lint 生效。
4. 对确需例外的测试或平台范围，验证例外命中该范围，相同问题移入生产代码或例外外仍能检出；不采用关闭继承、全 crate blanket allow 或放宽现有 CI flags 的方式消除失败。

通过条件：已迁移成员获得预期规则；局部例外有技术理由且不扩大范围。未迁移成员和未执行组合不记为完成；普通 clippy 通过不代表全 CI 已执行。

<a id="architecture-regressions"></a>
## RG-A01 / RG-A02：真实依赖正反例

1. 检查器就绪后，用真实最小 Cargo workspace 构造夹具，固定依赖和 lock；分别测试直接依赖、中转传递依赖、build 依赖、optional feature、平台限定边、`package=` 别名及 workspace 继承。
2. 先运行合法图，再仅引入一个违规边；以实际 Cargo 解析结果检查 rule key、root、kind、host/target、features、source 和可定位路径。声明违规与激活闭包违规分开核对。
3. 生产图不混入其他成员 dev 图；测试图只含所选测试根。Windows-only/wasm-only 与 host build 反例按已支持矩阵执行，未具备条件的组合报告覆盖缺口。
4. 按[方向矩阵](rust-engineering-contracts.design.md#boundary-matrix)核对真实正例：Projection→Game Client、Service→Client Storage、Provider bridge→Simulator DTO；不得因通用分层猜测而拒绝这些已允许关系。
5. Protocol Tools 的现有 dev 互操作应通过；将同一边改为 normal、扩展例外 root/target/features，或让禁止对象经中间包进入生产闭包时，应命中原规则。
6. 对 Game Client/Harness、State、Runtime、Launcher 等实际禁止边分别保留最小反例；取消错误边后恢复通过。现有 Launcher 检查覆盖和反例尚未被承接时保留原入口。

通过条件：正例通过、反例命中准确规则、例外仅作用于登记范围。模块 owner 按现行评审核对原合同、候选 manifests 与误报/漏报，不新增独立审批身份。
依赖图通过不证明授权、World 执行结果、持久化恢复或产品行为等价；这些继续使用原专业验收。

<a id="architecture-scope"></a>
## RG-A03 / RG-A04：不完整检查与 CI 选择

1. 注入缺失 lock/root、损坏元数据、离线依赖不足、工具链不可用与规则解析错误，确认返回非零 `inspection_error`，没有伪造 `pass` 或静默换配置。
2. 对规则要求而尚未注册/未执行的组合验证 `coverage_gap`；只有正式启用的必需检查按设计阻断，未启用范围明确报告，不用扩大 N/A 消除缺口。
3. 分别改变规则、检查器、反例、相关 Cargo/lock/toolchain、profile、受管产物与 TS 消费样例，使用既有 [scope 规划测试](../../../scripts/plan-rust-required-scope.test.py) 和实际候选 CI 核对选择及 required 汇总。首次新增独立路径时核对[可信基线登记规则](../../testing/ci/ci-tiered-execution.design.md#ci-unknown-inputs-design)：旧基线须拒绝未知路径，登记合入后的基线须选中实际验证；已被 Cargo owner/可信规则覆盖的路径不强制另拆登记 PR。生成工具首次接入同样适用。
4. 核对普通文档改动仍按原 scope 处理；共享控制变化保留原扩大覆盖策略。选择器 fixture 通过不能替代实际运行的依赖检查。

通过条件：失败和缺口分类准确、既有选择与汇总正确消费新检查结果；不新增同名成功状态、跳过必要任务或自行改变合入权限。

<a id="protocol-generation"></a>
## RG-P01 / RG-P02：确定生成、只读检查与完整集合

1. 生成器就绪后，固定 profile、输入和工具/依赖锁，在两个不同临时路径生成完整集合，比较所有文件字节、清单与摘要；使用已登记容量上限的边界样例。工具准备失败时停止，不改用未锁定版本。
2. 核对产物没有时间戳、绝对路径或无关 HEAD；修改实际输入/生成配置应被识别，无关文档变更不应造成协议输出变化。首切片人工 OpenAPI 始终作为输入保留。
3. 执行候选工具的 `check` 前后比较全部受管文件和人工源，确认检查无工作区写入；分别修改、删除、增加、混合不同 generation 的产物，验证 `stale`、`missing`、`extra` 或 `invalid` 与非零结果。
4. 在隔离副本中注入无法解析的 `$ref`、影响导出语义但未支持的 OpenAPI/JSON Schema 构造、精确整数舍入、超限输入和 symlink/path 越界；拒绝不完整或过度宽泛输出，不影响人工消费者与 golden fixtures。custom serde/泛型负例仅在 profile 引入 Rust component 后执行，首版不虚报 Rust 导出覆盖；首次 CI 接入按 [RG-A03/RG-A04](#architecture-scope)验证真实路径选择。
5. 在生成与写入之间改变输入或输出 preimage，验证写入冲突停止；在多文件替换过程中中断，验证 manifest/摘要能识别混合集，检查与打包均拒绝发布。
6. 恢复固定输入后完整重生成，或用普通 Git 操作恢复已知完整版本，再运行只读检查。不得把逐文件 rename 称为多文件原子事务，临时输出不交给运行消费者。

通过条件：同一输入可重现完整集合；检查只读；冲突、超限和中断不被写成成功。所有故障注入在专用夹具/副本进行，不修改真实服务、凭据或世界目录。

<a id="bootstrap-conformance"></a>
## RG-P03：Bootstrap profile 与真实消费者

1. 对照[人工 OpenAPI](../../../crates/oasis7_agent_api/src/game/read_bootstrap.openapi.json)、[实际 handler](../../../crates/oasis7_viewer_runtime/src/game_read_api.rs)和[wire 测试](../../../crates/oasis7_agent_api/tests/game_wire.rs)，核对当前五条 GET、API/schema revision、limits 及 `writes_available=false`。源 OpenAPI 须补齐 `DecimalU64` 的标准 schema 上界约束；原 pattern/20 位长度与 description 不足以拒绝上界加一，不从自然语言猜测范围。
2. 复用并按差距扩展 [handler 测试](../../../crates/oasis7_viewer_runtime/src/game_read_api_tests.rs)，核对 `fixed_commit` 缺省/null 允许与非 null 历史读拒绝、严格未知请求字段和递归整数投影；不能用较宽 World Service DTO 放宽 HTTP profile。
3. 编译独立 TS 协议一致性样例，真实 import 生成类型并运行校验，消费实际 handler 的成功/错误响应 fixtures 及独立负例；覆盖 `0`、`2^53-1`、`2^53`、`u64::MAX`、上界加一、前导零、尾随换行、null/缺省和未知字段。标准 schema 校验与 TS 校验均应拒绝越界，包含已知 task-control 嵌套字段的上界加一反例和字段正整数收窄；请求拒绝未知字段与响应允许扩展分别测正反例。仅 `tsc` 编译生成文件不算消费验证，此场景不要求改造 Viewer 或新增产品网络客户端。
4. 确认 `world_binding`/`data` 的实际字符串投影与顶层数值版本字段没有混淆；动态 `data` 保持有界 JSON/`unknown`，不声称获得完整世界 schema。proof 仍为 opaque hex。

通过条件：生成表达、真实响应和消费者行为一致；TS 编译本身不证明网络输入可信、鉴权完成或完整 `/v1/game` 行动闭环。

<a id="wire-compatibility"></a>
## RG-P04：Wire、golden 与新旧方向

1. 核对共享 DTO、codec 的唯一声明、兼容再导出及实际消费者；若从人工 component 迁到 Rust，确认 typed DTO 被真实 handler 使用，未复制只为生成 schema 的镜像类型，旧人工定义不再成为可编辑真源。
2. 按实际触及 profile 选择现有 [Agent wire tests](../../../crates/oasis7_agent_api/tests/wire_contracts.rs)、[World Service contract tests](../../../crates/oasis7_client_api/tests/world_service_contract.rs) 和[签名兼容测试](../../../crates/oasis7_client_api/tests/signing_compatibility.rs)；记录实际版本、features、测试过滤与结果，不为未触及 profile 宣称覆盖。
3. 保留原始 JSON/CBOR golden 字节和独立预期值，核对 DecimalU64 字符串、裸 u64 数值、字段省略/null、enum/tag/rename、canonicalizer 与 domain；生成工具不得自行重写签名字节。
4. 对实际受影响且已有旧版本的消费者执行旧读新和新读旧，包含新增字段/枚举、严格未知字段及版本拒绝；明确预期兼容或明确拒绝，不自动将新增字段视为兼容。首次 TS 产物没有旧 TS 版本，以现有 Rust wire golden、真实 handler fixtures 和现行 profile 建立首个消费基线，不伪造双向 TS 结果；后续版本再按真实消费版本验证双向兼容。
5. 精确整数通过真实解析/编码往返验证；没有 lossless codec 与跨语言 golden 的 profile 只提供已验证描述或 opaque proof，不发布可调用的 JS 签名/编解码能力。

通过条件：声明范围内 wire 与签名语义保持，兼容方向有实际证据。schema diff 或字节生成一致不替代该验证。

<a id="security-preservation"></a>
## RG-P05：真实拒绝、授权与恢复语义

1. 复用真实 parser/handler 的重复 key、未知字段、深度/字节上限、历史读拒绝与签名错误负例；确认产物或消费者变更未绕过原拒绝行为，不能只验证 schema 拒绝。
2. 消费者触及请求或重试时，按[安全调用验收](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-013)保留原请求身份，验证未知结果不自动变成新请求、权限拒绝不升级为成功、恢复沿原操作继续。
3. 使用公开测试身份和明确的失效签名/权限夹具，核对原 Runtime/接口 owner 的实际结果；不访问生产密钥或以生成器返回成功代替授权判断。

通过条件：所触及行为的正负例符合原接口合同；未执行的远程恢复、权限或世界最终性范围明确标为未验证，不能由本专题检查代签。
