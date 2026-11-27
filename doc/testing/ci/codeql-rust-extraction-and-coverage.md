# CodeQL 查询、提取、缓存与扫描覆盖（分册）

- 主设计文档：[`codeql-integration.design.md`](codeql-integration.design.md)
- 本分册按 #6507 原样承接 `codeql-integration.design.md` 第 7 节（查询、提取、缓存与扫描覆盖），不改变既有需求映射、安全约束或验证边界。

## 7. 查询、提取、缓存与扫描覆盖

### 7.1 两档查询

PR 和定时默认基线保留官方默认查询集合，不加入 `security-and-quality`、`security-extended` 或实验查询。Rust default 的可信默认分支维护使用 §7.2.6 的固定库修复和原始默认查询源码；其他场景继续使用 Action 内置默认选择。尽可能保留官方对 Python、JS/TS 的默认增量路径。[E3、E4]

每日扩展档使用 `security-extended`。对当前查询包明确支持的语言，可以在扩展档验证 `local` threat model，以纳入 CLI 参数、环境和本地文件等来源；支持范围、额外告警和耗时必须单独确认，不能默认它覆盖所有项目 CLI 威胁。PR 默认档不因该扩展而改变。[E10]

不首批启用自定义查询平台或海量质量规则。扫描“全面”指四语言、维护中源码、关键上下文、定时更广查询和明确缺口，不要求每次 PR 运行所有可能的规则。

### 7.2 Rust

固定 `build-mode: none`。不要为每个 crate 执行 `cargo build -p`，不要以 `ci-tests.sh required/full` 为 CodeQL 初始化步骤，不跑发行版、WASM、Trunk、GUI、GPU 或链网启动。

使用与仓库兼容的明确稳定 Rust toolchain；清理对提取无关或有干扰的 `RUSTC_WRAPPER` 等环境继承。build script 所需工具只按提取诊断补充最小集合，不照搬业务编译 apt 安装表。

对非默认 features、target cfg、宏展开、独立 manifest、build.rs 生成源、WASM/nightly 路径逐项记录覆盖。完整源码文件存在不是所有条件编译路径均获得有效语义的证明。覆盖缺口不能用移除相关目录来隐藏。

经用户明确批准，Tauri 上下文生成是一个单文件覆盖例外：仅将 `tauri::generate_context!()` 隔离到 `tools/oasis7-command-center/src-tauri/src/generated_context.rs`，并在 Rust 的无构建提取配置中精确忽略该文件。该文件只允许真实的上下文生成包装函数，不放入命令、权限判断或其他业务逻辑；普通构建始终调用同一真实宏，不使用 CodeQL 专用 stub、条件编译或禁用全部宏。其余入口、命令、features、targets 与依赖上下文继续提取。此例外承接 CQ-R03，结果记录须注明未分析生成的上下文，不能宣称无例外完整覆盖；是否减少实际提取/求值成本仍需新源码数据库验证，旧冻结源码测量不随配置自动更新。

同样经用户批准，将测试夹具 `cognition_projection` 原样隔离到 `crates/oasis7_runtime/src/tests/agent_cognition_live_persistence_projection_fixture.rs`，三个入口仅增加该精确路径的覆盖例外。排除整个测试数据构造函数（含前置状态选择及大型 `json!`），不排除调用它的持久化测试或生产代码，不按宏名或整个测试目录排除。普通测试继续调用真实实现；该文件不得承载业务逻辑或断言。此例外承接 CQ-R03，结果必须披露测试数据构造未分析；实际数据库排除及性能收益须用包含拆分的新源码身份另验；新增路径配置不能改变旧冻结源码中的内联构造，不可冒称已验证。

维护中的多版本兼容测试应有明确的 Cargo 源码归属：每个版本拥有包内入口和依赖该版本的适配器，共用协议与完成条件通过正常 path dependency crate 复用，不让两个独立 manifest 的外部 `[[bin]] path` 指向同一份实现。公共协议 crate 不选择或包装具体 libp2p 版本；版本适配器保留各自的依赖、patch、原生身份和通道类型。依赖锁文件除新增本地公共包外保持各版本的解析结果，真实双向传输测试继续验证协议兼容性。

平台相关回归将资源生命周期、可复用观测和平台判据分别组织成普通模块；预期消息在遍历输出前构造，平台 `cfg` 只选择对应判据，不删除原来的失败条件。结构重构和运行测试通过均不等于 CodeQL 覆盖通过：仍须在真实依赖上下文中取得新源码的宏展开、调用解析及数据流证据，旧行号或旧数据库不能代签重构后的源码。

#### 有界公共模型性能诊断

Rust 公共模型的有界诊断承接 [CQ-R03](codeql-integration.design.md#cq-r03)、[CQ-R05](codeql-integration.design.md#cq-r05) 与 [CQ-R09](codeql-integration.design.md#cq-r09)，独立于安全扫描、SARIF 上传及批次启用。固定业务源码、真实 manifest/lockfile/path dependency/vendor 身份及官方完整 bundle；初始收集文件少不表示依赖建模成本小。最后一个活跃谓词、文件过滤或编译耗时均不能单独证明瓶颈、裁减前置成本或运行收益。

分区按原见证的互斥标签集合投影唯一完整期望，保留原公式、零行、阳性与负例；缺项、重复项、额外项和非法值均失败。每个成功分区立即解码并保存独立结果及终态，后续失败不能抹去已完成结果。端点存在性仅解释负例，不能替代原数据流见证。完整验收另核全部规定结果及同一源码、数据库和官方库身份；局部结果不得相加冒充原完整批次在同一预算内通过。

每轮只选择能区分既有假设的探针，先从公共 Node/调用解析/局部流等前置边界定位，再决定是否运行完整分区。复用同一数据库内官方缓存时，预计算、预热及所有分区累计成本均计入预先固定的总预算和模型阶段预算，不通过逐分区重置时间获得表面提速。全轮墙钟最多 1080 秒、单模型阶段最多 180 秒；线程、内存请求及各阶段 deadline 在启动前冻结，外部 watchdog 负责进程整体上限，不以 CLI 内部阶段 timeout 替代。

诊断保留独立结构化 evaluator 日志、tuple counting、官方 `generate log-summary` 结果与资源采样。阶段汇总按共享 stage 去重，重叠 wall 区间不得相加当作 CPU；采样记录 owned 进程组及 PID/start 身份、CPU/RSS/I/O 的有效性和缺口，不能据末样本推完整消耗。原始日志、摘要、遥测和结果合计不超过 64 MiB，启动前预留终态空间；超限、写失败、进程身份或清理不确定时停止后续执行，保留失败，不以裁掉原始失败证据满足上限。

官方版本对照各自使用完整固定版本 bundle、解析包版本及摘要，分别重新提取数据库；业务源码、工具链、依赖身份、提取配置和资源边界一致。速度变快必须同时核对宏展开、提取诊断及公共正负见证，不能把缺失模型解释为优化。若公共 Node 探针仍承担相同昂贵共享前置且官方新版对照未完成，停止无新机制的查询改写和同版重跑，整理可重建的真实上下文、QL、原始/官方摘要日志、资源样本和终态交专家评估；公开提交或上传仍受单独授权约束。性能诊断解决不代替 legacy、平台相关源码及其余 Rust 完整覆盖和运营验收。

#### 7.2.6 Rust default 生产库接入与首次验收

复用候选库的固定组成与身份合同，生产执行及当前源码验收仍按本节独立核对。

本路径承接 CQ-R02/R03/R05/R06/R09，只用于 `codeql.yml` 中可信默认分支的 Rust default 维护扫描。生产输入是本次计划与实际 checkout 一致的提交，不沿用研究入口的冻结业务 SHA、manifest 数量或历史覆盖结论。现有批次变量仍控制是否执行；首次配置为 baseline 时按 §4.2.1 保持 pending acceptance，接线合入和候选研究通过均不代替运营验收。PR default、extended 和其他语言继续各自原有执行路径及预算。

正式 native 前置仅在 default 且非 `pull_request` 的维护调用中显式选择已验证的 `ubuntu-archive-https` profile，沿用私有源副本、完整四包与模块探针及前后身份合同。共享 helper 默认仍为 `runner-sources`，PR、extended、固定源码观察、容量研究及独立 Cargo check 不随之切换。选中的 profile 写入 native budget 与 terminal；准备仍受原 180 秒窗口、270 秒父截止与 90 秒收尾保留约束，update 的 60 秒上限、20 秒探针空间和 5 秒特权收尾差额不变。源准备、核验和清理均计入原截止，不失败回退重跑。

正式调用沿用普通 native stdout/stderr 与独立 `oasis7-codeql-native` artifact，不开启诊断 Worker 观察器或 archive 采样。`native-source-profile.json` 与四条实际命令收据分别验收，不能放入分析截止内的 production 查询收据集合。终态只投影异常类型、真实命令 exit/failure_kind 和源收据保存错误的固定字段，保留原失败并阻止后续提取、查询与上传；源身份或清理不完整不能记 PASS。native 成功只表示本次依赖就绪，原 37 条结果、适用 Linux 覆盖、官方上传、唯一平台关联及每日 baseline 验收继续独立完成。

固定 Rust-only CodeQL bundle 2.27.4，下载后核对 SHA-256 `1c2f867094b80e43c9a0e511090e57e0ea3bf107600b4b3763c52b9020b4ec4b`，通过官方 init 的 `tools` 输入使用该本地包。生产最终库版本独立固定为本节的 `qualifier-lines-negative-projection-v8`，直接基线为 `qualifier-lines-constraint-target-qualified-v7`，其原 v6 基线保持。官方库保持只读，在任务私有目录依次复用 v3 qualifier/arity/lines、shared helper、两处 trait 注解、ConstraintMatching 和 target-qualified 的实际准备函数，再复用本节负向投影的严格准备函数。保留真实 v3/v5 中间快照、v6/v7 全树和各层收据，从已验证的 `candidate-target-typeinference` 独立复制出 `candidate-negative-typeinference`；所选最终完整库集合相对官方仍恰为三个 Rust 文件与一个 shared 文件变化，固定字节和 mode 继续核验。`str::lines` 使用通用 taint summary，不添加业务专用流边。版本或补丁不匹配时失败，不回退到旧库继续上传。

生产准备终态与恢复检查同时绑定 v8 最终身份和 v7 直接基线。进入实际准备或恢复前，分别核对共享 helper 的 `WORKLOAD_VARIANT` 与固定 v6 组件、target helper 的 `RESEARCH_VARIANT` 与固定 v7 组件；这些组件常量与生产最终版本分开，研究版本变化不能自动改变生产选择。原 v6/v7 准备、验证和 closure 接口保持原义，生产仅在已验证的 v7 闭包中显式选择 negative shared 根。`arity-patch-identity.json`、`lines-model-identity.json` 保持 v3，`shared-helper-identity.json`、`trait-visibility-identity.json` 保持 v5，`constraint-matching-identity.json` 单独绑定 v6 及两份 v5 收据，`target-qualified-identity.json` 继续绑定真实 v6 全树和收据摘要、固定 target 补丁及完整 v7 树。`negative-projection-identity.json` 再绑定真实 v7 全树、target 收据摘要、固定补丁及完整 v8 树。恢复及查询后从当次官方库重建全部验证链；不能只比较保存的最终快照或将旧收据改标签。七份收据摘要和全部私有库实际字节纳入执行前后检查，原官方/v6/v7 文件权限检查保留，并核对官方/v7/v8 的完整目录集合与目录、文件权限。旧 v2/v3/v5/v6/v7 production preparation、研究或观测 preparation、缺失或伪造收据、额外文件或 mode 变化、任一私有库被替换均拒绝；即使 production 与 negative observation 使用相同 v8 字节，也不能互相恢复。

正式 helper 只封装工具准备和完整默认查询执行，不调用强绑定研究源码的容量主入口，也不加入研究七项见证或研究专用 evaluator capture/summary 观察器。复制官方 rust-queries 0.1.46 的查询与支持源码，保持原始 37 项默认查询的 ID、字节和集合指纹；移除发布包的预编译 QLX、嵌入库及旧依赖绑定，以独立源码 pack 包装。逐查询解析实际 14 项依赖，确认选择 `candidate-rust-all` 与 `candidate-negative-typeinference`，拒绝 v7、v6 或官方 shared 根替代，并保留 `suite-helpers` 的 query 角色校验，使用独立编译缓存和 `--no-use-precompiled`。全部官方依赖、四棵私有库树及源码 query pack 在执行前后保持一致；仅添加 search path 或设置文件存在不足以证明实际依赖正确。开发阶段复用原 600 秒窗口的固定 CLI 37 项真实解析测试，不运行数据库、编译或求值；该测试只证明生产消费者接线，不能代签当前 main 的真实生产验收。

仅默认查询 batch 在现有进程 runtime 中开启低频 Linux 资源时间序列，原 0.2 秒聚合采样及其他调用方保持原行为。每 15 秒至多读取一次，最多 320 条、每条编码不超过 12 KiB、合计不超过 4 MiB，仍计入原证据总限。每次最多检查 1024 个进程目录项、8 个同 PGID 进程和合计 64 个线程；单个 proc 文件接纳上限为 4 KiB，额外读取 1 字节探测超限，读取检查点共用 `min(原 payload 截止, 本次起点+50ms)` 软截止，不能据此声明单次系统调用或 RSS 的硬限制。只保存固定字段的数值与状态，包含接收 wall/monotonic 区间、进程 CPU/I/O/minor-major fault、线程 CPU 和 system PSI 累计值及差分，不保存进程名、命令行、路径或 proc 原文。进程统计与线程集合在前后 PID/start_ticks/PGID 一致时接纳；线程差分还须匹配同一父身份及 TID/start_ticks。首次、缺测或计数倒退的差分为 null，读取失败、身份变化及上限须显式保留，不补零。I/O 包含内核计入的已等待子进程，不能跨进程相加当设备流量；PSI 明确为 system 范围，不能独占归因到本任务，CPU full 不作为系统压力值使用。采样读取/解析故障或额外空间不足只停止附加观察并保留原因，不改变查询首失败、取消、截止、清理或上传判定；所有证据文件的创建、写入或 flush 错误仍按原规则处理。该时间序列用于与迭代及缓存逐出日志对齐，不是独占谓词成本或覆盖验收。

init 后从实际 GitHub job 起点计算一次分析绝对截止：最多 75 分钟，且不晚于 90 分钟总截止前 600 秒。私有库准备、官方 Action 提取、37 项查询与结果校验共用该截止；提取前重新计算剩余整分钟作为 Action 外层步骤上限，CLI 子进程只消费原截止的剩余时间。通过 Action main 监督核心，以独立 production mode 绑定当前源码和预算，在真实进程启动及清理时执行绝对截止；官方 post 仍由 runner 执行。Action 使用公开的 `skip-queries: true` 完成数据库；helper 核验监督结果、finalized DB、实际源码、资源及库身份后运行默认查询，并生成待上传 SARIF。失败、超时或身份不符阻止上传，不重新开始分析预算。

失败采证最多消费分析截止后的 90 秒，属于原 600 秒预留，仍为上传及 post 保留至少 510 秒；该阶段不启动查询或解码子进程。超时后不能完成 guard、身份或日志核对时保留失败和未知，不将采证成功改写为分析成功。

覆盖采证动态枚举当前受维护 manifest 与独立 workspace 锁文件，复用有界源码/锁观察和 Cargo guard，在实际提取的 metadata/解析调用中保留 locked 约束。提取日志和 `coverage_report` 保留原始 COMPLETE/PARTIAL、REVIEW/UNKNOWN；发现清单、成员集合对账或查询完成均不代签实际提取与宏/feature/target 语义。WASM 仍需其真实 CodeQL 上下文验证；未接线源码的归属须明确收口。沿用既定支持范围及两条精确覆盖例外，不扩大忽略路径。

原 37 项结果逐项执行 `bqrs info` 和 `bqrs decode --format=json --entities=id,string,url`，保留所有结果集和行，核对 info 与 decoded 的集合及行数。显式保留 BQRS 已提供的实体 ID、文本和 URL，供原 diagnostics 按实际位置与源码归属对照；实体文本可能重复，不能作为宏位置的唯一键。URL 取决于查询编译时保存的信息，缺失时保持未知，不由文本推算位置，也不把整文件 URL 当作宏坐标或 owner/展开关系。解码沿用原 30 秒单命令上限、分析绝对截止和证据总限，不在收尾阶段追加解码；该参数不改变查询集合、字节或覆盖验收条件。

执行状态由提取和正式 helper 联合决定，提取成功但查询失败不能记录为 execution success。通过原 `upload-sarif` 上传，保留 `oasis7/rust-repo/default` category、实际 SHA/ref 及唯一 `sarif_id` 关联；上传关联 evidence v1 结构保持。独立生产 evidence artifact 保存工具、查询库、源码、结果与覆盖收据及有界日志，不包含数据库或完整源码包。查询执行、平台上传、findings 和覆盖验收分别呈现。

首次验收核对实际执行/容量、唯一上传来源、支持范围与控制样本、Security findings 及运营设置。试运行允许收集尚缺的覆盖事实；持续启用声明须完成适用验收，不能把 REVIEW/UNKNOWN 改记为通过。失败仅回退本次 Rust 批次键，保留其他语言、已有告警、required policy 和历史结果。通过后按[日频目标](codeql-integration.design.md#rust-daily-baseline-goal)持续运行，不追加 PR observe 验收。现有 PR/extended 的 45 分钟路径未被本次生产结果验证；未来若另行启用这些范围，须分别处理执行路径、容量及适用行为。

### 7.3 其他语言

Python 与 JS/TS 默认只建立静态分析所需环境，不执行项目安装脚本和构建脚本。确实依赖生成源码时，单独批准最小生成步骤，隔离凭据，并评估放到定时档而非所有 PR。

Actions 单元包含工作流与 local action metadata；它不替代所有被调用 Shell/Python 代码的语言级分析。

### 7.4 缓存

优先使用官方支持的数据库/提取增量机制，不自造旧 SARIF 上传或跨提交数据库替换。缓存 miss 只能变慢，不能改变扫描通过条件、身份和覆盖。

Rust 依赖缓存可保存包下载数据，如 Cargo registry/cache、index 和 git dependency 数据；不保存凭据、`~/.cargo/bin`、可执行 target、发布目录或全局缓存根。cache key 至少区分系统、工具链和相关 lockfile 摘要。只有受信默认分支维护执行负责写入共享依赖缓存；PR 只恢复可信缓存。不得与发布、required CI 的可执行编译产物共享缓存。

不假定 CodeQL 的内置 dependency caching 自动支持所有 Rust 场景；实现需确认当前 Action 的实际能力，不支持时采用受控的下载缓存或不用缓存。

### 7.5 度量提取而非只数绿灯

每个单元记录选中原因、预期源码/manifest 集合、提取诊断、实际执行配置、CodeQL/查询包版本、完成时间及上传状态。首次基线核查所有维护中的 manifest 是否出现于预期覆盖。

覆盖检查可以使用临时数据库查询或提取元数据辅助，但不把有文件记录等同于可用数据流。已知控制样本在临时 fixture 中验证，不能为了证明检出能力把真实可触发漏洞植入 main。
