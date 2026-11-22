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

#### 生产资源下的完整容量评估

容量评估承接 [CQ-R02](codeql-integration.design.md#cq-r02)、[CQ-R03](codeql-integration.design.md#cq-r03)、[CQ-R05](codeql-integration.design.md#cq-r05) 与 [CQ-R09](codeql-integration.design.md#cq-r09)，回答固定配置是否具有计算与覆盖可行性，与前述 180 秒性能诊断分别记录。入口默认关闭，仅经明确手动确认执行一次；完成或失败即结束，不自动重试、不修改运营批次或 required policy，不向平台上传 SARIF。旧性能问题与未达到的覆盖结论不因容量任务成功改判。

冻结一个真实 main 提交和完整官方 bundle，保留生产 `rust-repo/default` 的全仓源码上下文、独立 manifest、本地维护 vendor、features、targets、模板与既有排除规则；不得换成单 crate、旧 NodeOnly 或伪造投影。记录 bundle/解析包、实际项目工具链、分析工具链和归档标准库来源。线程与内存采用生产所固定 CodeQL Action 的选择逻辑及实际环境值，记录 runner、CPU/cgroup/内存来源与实际参数；不能把原诊断的 2 threads/3072 MiB 或数据文件限制当生产配置。

经单独授权可在 CFG 手动入口使用默认关闭的 `callee=true`，固定 f9ab 容量失败业务快照，保持官方2.27.3、生产自动资源、单业务采集根和45分钟总截止。该模式与 profile 互斥，只提取/finalize及定向调用目标诊断，不运行安全suite或原CFG图测量。保留原 Syntax、ResolutionCounts、TargetFunctions、OriginalCalls 四表原文与顺序，按第42行调用AST、语法路径、静态目标、目标名、目标位置分别计数，另保留目标声明及原CallsCandidate结果；零值与无位置必须显式可见，不改变原容量验收断言。

随后追加 InternalPaths、InternalTypes 两表，区分调用路径与目标impl self的解析及类型关系。仅正常导入固定官方2.27.3中导出的 PathResolution、TypeMention/TypeInference 内部接口，不修改官方QLL、可见性或解析闭包；接口不兼容在提取前的check-only失败。内部API不稳定，结果只用于原因诊断；AST文本、resolvePath结果、TypeMention.getTypeAt的根与参数映射、公共静态调用结果分别报告。完整关联函数路径的resolvePath空值、调用处省略泛型参数的TypeMention空值都不能单独判缺陷；这些关系也不是调用表达式的完整类型推断，不能代替公共验收。原公共阶段摘要与OriginalCalls行数在解码后立即保留，后续表失败仍标整体INCOMPLETE并保留已完成结果。

所有表共用每命令最多900秒、原总截止及180秒采证收尾预留；每表最多100行、每行最多8192字符、解码文件8MiB、总artifact256MiB，不因追加表而续期。查询未执行或失败保持NOT_REACHED/INCOMPLETE；诊断完成不等于容量通过或根因确认，不跨数据库关联内部ID。

固定业务 checkout 必须占据 `GITHUB_WORKSPACE` 根，使 Action 的实际进程目录、Rust `trace-command` / `index-files` 起点与数据库 source root 一致；`source-root` 或 `checkout_path` 配置正确本身不证明采集隔离。工具 checkout 先在同一根读取，将有摘要的最小 Python 工具闭包复制到 `RUNNER_TEMP`，再切换到固定业务提交；不得在采集树内保留第二个 checkout。初始化前保存真实 cwd、workspace、Git 根/HEAD/状态、工具位置与摘要的独立校验。预检记录下一步 Action 的预期采集起点，实际 trace 命令仍须以执行日志核对。

当前固定容量配置的 job 最多 90 分钟，分析阶段上限 75 分钟，并按当时剩余时间减 600 秒公共见证、结果校验和收尾预留动态缩短。它与 Rust default 默认分支维护采用相同总预算，但容量入口另做公共见证且不上传 SARIF，仍须独立完成 hosted 验收。整个评估包含动作准备、下载、提取、官方默认安全查询、全部规定公共正负见证、解码、结果和证据落盘。起点绑定真实 job 启动身份，内部截止为收尾及 artifact 保存预留时间；排队与运行分别记录。手动容量、CFG 和 Cargo 诊断用同一次 GitHub 响应的服务器时间计算 job 已运行时长，再扣除请求耗时与时间精度余量，换算为 runner 时钟的剩余截止；缺少、过期或矛盾的服务器时间拒绝启动，不能把 runner 与 GitHub 的时钟偏移当作运行耗时。成功既要求默认查询集完整完成，也要求公共 DF、closure、真实 callee 来源、legacy、Linux 平台判据和源码/manifest 覆盖校验通过，不能用默认查询完成代签缺失的语义见证。所有阶段共用一个全局剩余预算，缓存预计算和额外见证成本不得隐去。

结果分别记录执行、覆盖、资源、预算余量与证据完整性。全部结果通过且有预声明的余量，才得到该冻结配置下的容量可行证据；接近上限时评估持续成本、频率和资源，不据此进入 PR 扫描运营。查询完成但覆盖不全时处理具体缺口并保持对应 HOLD。超时、OOM 或计算结果不完整表示该实际配置未通过容量评估；若先触及观测或证据收集限制，须单列测量不完整，不能将工具自身限制归因生产不可用。

公共查询批次已完成后，在原剩余预算内逐表解码并保存真实验收结论；单表见证不符合时保留 FAIL，继续收集其余已计算结果，最终任一见证失败仍使整个批次失败。解码、子进程、预算或证据保存错误继续立即停止，不追加查询或重试，也不把未到达项补为通过。

失败采证独立于成功验收：初始化、准备或分析失败后，仍在原截止及 artifact 上限内保存已有数据库日志、各步骤 outcome 和逐项 Git 校验的原始 stdout/stderr/exit；资源或身份 gate 失败不能短路采证。日志按读取起点长度截取，增长、截断、不安全路径、预算不足和缺失分别标记，已保存的部分不因后续失败丢弃。公共见证及默认结果校验未执行时明确保持 `NOT_REACHED`，不启动失败后的 CodeQL 子进程。终态保留最初的失败事实并另记采证/收尾错误；helper 的进程组清理不代表官方 Action 已清理，后者需 runner 终态日志独立确认。总 job 截止、分析 step 实际分配和收尾预留分别记录，内部 step 限时不能单独证明完整评估预算不足。

超时后的日志采证可在原日志读取截止内先观察有界稳定窗口：最多 5 秒、连续至少 1 秒元数据与日志集合不变，保留至少 1 秒给实际复制；不足稳定窗口时直接标记未稳定并尽剩余预算保存字节。每次观察重新 nofollow 打开路径，累计扫描最多 8192 项、每次最多 128 个日志，沿用深度和路径长度上限；替换、路径不安全、持续变化、预算或扫描上限均保留 PARTIAL。窗口稳定后仍执行原初始长度、文件与路径前后身份、压缩及摘要校验；复制期间再变化同样拒绝完整快照。该窗口只减少末尾写入与复制竞争，不证明写入者退出、终态日志齐全或 Action 清理；不发送终止信号，不扩展原采证/总 job 预算，不修改旧运行的 PARTIAL 结论。

日志采集总落盘上限仍为 128 MiB，整个 artifact 仍为 256 MiB。小日志保持原字节；超过 16 MiB、初始长度不超过 512 MiB 且总原始读取不超过 1 GiB 的日志，可用 level-1 gzip 流式完整保存，单个压缩文件最多 64 MiB。分别记录原始和压缩字节数、SHA、编码及原始坐标；只有读满初始长度、gzip trailer 完成且打开文件与路径的前后身份一致，才为 COMPLETE。超出读取上限的日志继续在原 16 MiB 单文件上限内保存首尾片段，记录偏移与省略区间；任何超时、容量、写入、源变化或 trailer 错误保留已有部分并标记 PARTIAL，不能将部分压缩流当作完整日志。

消费者以有界块和行流式解压，校验压缩及原始 SHA、长度、EOF/CRC/trailer 和原截止，不整份展开到内存。查询进度只汇总完整行，不跨省略区间拼接；覆盖解析保持同一连续流中的列表状态。缩略、未闭合列表或部分日志的缺失集合仍为未知。

工作区 clean gate 与 tracked 状态分开呈现。准备开始根据已维护 manifest 清单，直接记录各候选 Cargo.lock 的初始存在性及有界 nofollow 摘要，包括 ignored 文件；候选最多 256 个、每文件读取最多 1 MiB、每次观察总读取最多 16 MiB，缺失父目录或不安全路径不记为不存在；每次既有 metadata 调用后和分析前保存身份及锁文件观察，并关联该次既有 metadata 原始输出与解析图摘要，不额外重解图。发现输入变脏时准备失败，分析不得启动。首次观察阶段只能缩小来源区间，不能证明精确写入者或解析影响；没有同一上下文的可比后图时解析影响保持 UNKNOWN。

运行中新出现的未跟踪 Cargo.lock 记录精确路径、有限内容摘要、初始是否存在及可证来源；不能仅凭文件名认定由提取器生成或不影响依赖解析。未经验证的来源及解析影响保持 UNKNOWN，清洁检查仍失败，不白名单忽略 vendor，也不删除或改写该文件。分析后再次比较初始锁文件状态与摘要，ignored 锁文件新增或变化同样拒绝验收；缺少完整可比观察也不得放行。tracked 状态没有变化不代替冻结输入内容复核。

独立 workspace 的 Cargo.lock 是冻结输入，不因它同时被主 workspace 作为 path dependency 使用而共享主解析上下文。容量入口在分析前解析维护 manifest 的 `[workspace]` 声明，要求每个独立 workspace 的锁均已存在且纳入原始输入观察；缺失、未知或无法解析时列出原因并停止，不现场生成锁、不忽略 vendor、不把新锁加入旧 source SHA 的白名单。正式补锁与依赖解析复核属于业务输入变更，使用新的显式业务版本。

固定官方提取器可能复制已有锁并更新临时副本，因此 clean gate 或准备阶段的 `--locked` 不能单独证明实际模型解析被锁定。容量分析使用独立 Cargo 代理：仅在采集树外的私有 `CARGO_HOME/bin/cargo` 安装入口，以匹配固定 rust-analyzer 的代理选择顺序；执行真实 Cargo 前恢复原 Cargo home，保持原缓存、配置及提取器传入的分析工具链。代理对实际 metadata/check/build/rustc 解析调用（包括临时锁路径）加入 `--locked`，保留原参数，限定只读命令并拒绝未知调用。普通 `cargo_extra_args` 不作为 metadata 已受约束的证明。代理与真实工具身份、原 home、固定截止和调用记录绑定；不修改官方 bundle 或用户 Cargo 安装。

代理记录有界、无完整参数或环境敏感值的调用开始和终态（操作、是否完整 metadata、manifest 身份及参数摘要），命令输出仍交官方 Action。任何拒绝、非零退出、超时、未完成记录、记录失败、身份变化或缺少任一要求独立 workspace 的完整 metadata 记录，都使结果准入失败，不能被 rust-analyzer 的 no-deps fallback 隐藏。记录总量最多 8 MiB、单条最多 4 KiB、调用最多 2048 次，仍计入原 artifact 上限与原截止；不为代理续预算。子进程沿 Action 原进程组，由最终 Action 清理事实解释，不据此声称整个 runner 无泄漏。该机制只约束解析可更新性，不代替 features/targets、宏与完整覆盖验收。

容量分析仅对官方 `ra_ap_load_cargo` 模块启用 debug，保留提取器原 info 级别；该模块的既有失败分支提供 build-script/check 的编译诊断与 stderr，避免只剩代理退出码。不开启全局 debug、Cargo 配置或命令环境 dump，不新增编译、提取或查询调用。该输出仍进入现有数据库日志采集、读取/压缩上限和固定截止；缺失、截断或无明确错误时原因保持 UNKNOWN，不从退出 101 推定缺系统库或锁拒绝。

封存清单内的所有普通文件都须运输，包括代理的隐藏计数器、失败标记和锁文件；artifact 上传仅限既有有界 evidence 目录并显式包含隐藏文件，不扩大到业务树、Cargo home 或凭据目录。上传成功不代替消费者逐项校验封存摘要；漏件保留原不完整结论，不补造历史控制文件。


经单独授权，可在同一默认关闭入口选择 `official-default-research`：固定同一业务源码、完整官方 bundle、默认查询及生产资源选择，仅研究官方工作负载的终态成本，不生成、编译或运行自定义见证。该模式整个 job 最多 360 分钟，截止绑定真实 job 启动；分析按当时剩余时间减去 600 秒证据/结果验证与收尾预留分配，不套用容量模式的内部分析上限。当前 `capacity` 为整个评估 job 90 分钟、分析最多 75 分钟，完整公共见证合同与至少 270 秒最终余量要求不变。历史 source 的 helper 分配保留原 60/45 分钟身份，不改变 CFG/profile 入口的实际截止。未知模式拒绝。

研究终态使用 `RESEARCH_*`，单列官方 Action 分析 outcome、逐结果校验、输入/覆盖与证据状态；分析成功不自动通过 dirty、部分日志或结果验证门槛。研究完成也只证明冻结配置下的官方工作负载，生产容量与完整 Rust 覆盖继续 UNKNOWN/HOLD，不使用容量余量判据放行。运行一次即停止，不自动重试；无 SARIF/数据库上传、批次启用、外部设置或资源清理。若需向上游共享数据库，必须另获覆盖该披露动作的授权。

单独定位已记录的 Cargo check 失败时，使用默认关闭的手动 `CodeQL Rust Cargo Check` 入口：固定原业务提交、原分析 Cargo 1.99 的 release/commit/host 身份，以及从旧调用摘要核对的 compile-time-deps、features/targets 和 locked 参数。整个 job 从真实启动计最多 15 分钟，唯一 check 最多 10 分钟；复用外置 helper 闭包、源身份检查与有界进程收集，保留独立 stdout/stderr、退出码、进程组清理结果和摘要清单，artifact 上限 16 MiB。Rust >=1.97 的原解析方式使用现有锁的私有副本与 `CARGO_RESOLVER_LOCKFILE_PATH`，源与副本变动均须记录。使用新 runner 的私有 target，明确不复现原多 manifest 的共享 target 历史；成功、不同失败或未完成不能追认旧错误原因。禁止 CodeQL 提取、查询、解码、SARIF/数据库上传；一次运行即停，具体错误形成修复依据后再决定验证，不盲重跑。

针对已确认的原生库发现失败，可在同一总预算内为一次修复验证安装 GTK 3、WebKitGTK 4.1 与 D-Bus 开发依赖：原生准备（安装及版本探针）共用最多 180 秒且早于总截止 90 秒的截止，使用发行版包源，不改变 Cargo features、targets 或锁约束；记录包版本与原失败所需 pkg-config 模块版本。准备失败则不启动 check，保留独立安装 stdout/stderr 与终态；发现通过仅说明该 runner 的前置就绪，不代替真实 check 或 CodeQL 覆盖。特权包管理命令使用自身超时；普通 helper 的进程组清理观测不代签 root apt 子树已空。

容量入口的 prepare 阶段复用单 check 已验证的原生依赖准备，在冻结源码、独立锁及资源记录通过之后、metadata 与官方提取之前安装相同发行版包并检查相同 pkg-config 模块。四条命令共用最多 180 秒且早于容量总截止 120 秒的准备截止，仍计入该固定配置的整个容量评估预算与原 256 MiB artifact 上限；不另执行 Cargo check。准备非零、超时或证据失败即阻止 metadata 和分析，已有安装/模块输出及 prepare 终态由原失败采证路径封存。安装实际版本保留，普通 helper 清理不代签 root 子树已空；此环境准备不改变 features、targets、业务 pin、官方 bundle、查询集或覆盖要求。

正式 Rust default/extended 扫描在 CodeQL init 前复用同一原生准备函数，只准备发行版依赖与检查模块，不执行独立 Cargo check。安装和探针仍共用 180 秒，外层步骤最多 5 分钟并计入该场景的总 job 预算；失败阻止后续提取、查询及上传。独立 native artifact 保留有界安装日志、实际包与模块版本和终态，不混入上传关联 evidence；准备成功不代签宏、目标或语义覆盖。

本地完整安全结果与解码见证可以作为容量证据保存，但无上传结果不证明 SARIF 关联、平台 provider、批次启用、并发运营或 hosted baseline 已验收。原始失败保留，一次容量评估未完成后停止追加运行；后续若需调整资源、实现或范围，先根据具体结果另定候选和适用验收。

#### 7.2.1 省略泛型限定符的私有补丁对照

容量入口提供默认关闭的 `compare_qualifier_patch`，只适用于当前固定 `capacity` 配置。官方分析、七表公共见证、独立终态、证据封存及上传先完成；随后才在同一 finalized 数据库上运行官方库与私有候选库的 `CallsCandidate`、`CallsLegacy` 原公共查询。该开关只增加研究证据，不改写官方失败、覆盖 HOLD 或批次配置，也不向上游发布或上传数据库。

候选从同一官方 `codeql/rust-all` 0.2.25 复制，仅应用固定摘要的行为补丁；应用前后核对 `TypeInference.qll` 摘要及私有库的全部文件差异。两侧逐查询核验实际 library closure，公共查询字节相同，结果与编译缓存分离。数据库的 evaluator 中间关系缓存仍共享，顺序与单次耗时如实记录；不能据此声称冷缓存性能、补丁提速或完整安全查询对照。原最小复现与 18 行边界样例不在该数据库中，真实对照只检验两个实际调用及各自 libp2p 版本来源。

研究最多 900 秒，截止取开始后 900 秒与原 job 截止前 450 秒的较早值；450 秒包含原最终 270 秒余量及 180 秒研究收尾/上传空间。少于 60 秒不启动查询，记 `NOT_REACHED`。源码、manifest/锁文件、官方库和已封存官方 evidence 的身份均需保持；官方分析未完成、身份或资源不可信时拒绝研究。研究结果与 artifact 独立，失败保持可见；实际完整 job 时长包含研究和上传，须与官方阶段时间分列，不能用研究前的终态时间冒充整个 job 的成本。

#### 7.2.2 独立真实调用对照研究

Trace ID: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89

本条是调用补丁研究的专业要求；产品层不适用，不新增玩家功能承诺。执行隔离、准入和预算由现有 capacity/comparison 回归验证，真实提取、四次调用结果与成本由对应单次 hosted 研究验证，离线替身测试不能代签。

`mode=qualifier-research` 为同一调用解析问题提供独立、显式的研究入口，须设置 `execute=true`；它不使用容量模式的附加开关。输入仍固定为完整业务源码 `7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef`、官方 2.27.4、`codeql/rust-all` 0.2.25 和 §7.2.1 的同一行为补丁。沿用原生准备、全部 manifest/锁文件、Cargo guard、Action 实际资源、完整源码提取和闭包核验。固定 Action 设置 `skip-queries: true`，先完成数据库 finalization；本模式不运行默认 37 项查询或七项公共见证求值，候选完整工作负载另按 §7.2.4 执行。

提取 checkpoint 必须确认源码、锁、guard、资源与官方库身份可信，数据库为预期源码根的 finalized Rust 数据库，五处规定源码归档内容一致，且尚无查询 BQRS。checkpoint 独立封存并上传后，才在同一数据库上依次运行官方、私有候选两个批次，每批包含原字节的 `CallsCandidate`、`CallsLegacy`。两个批次采用不同 pack 名称和独立编译/common cache、结果路径；官方两项完成后立即解码、校验、保存，再开始候选。超时或失败时保存已产生的有界 BQRS 与阶段收据，不追加解码、重试或续期，未校验结果不计通过。目标来源与已有目标不回退的 oracle 保持原样。共享数据库 evaluator 缓存的限制与 §7.2.1 相同。

此研究的整个 job 预算为 150 分钟，从实际 hosted job 起点绑定；提取分配不超过 25 分钟，并为后续阶段保留至少 600 秒。比较总窗口为 `min(7200, 剩余秒数−450)`，每侧批次最多 3600 秒且受同一比较截止约束；比较内部另留 60 秒进行最终身份审计。所有准备、复制、解析、编译、求值、解码、封存及上传均计入原总预算，不因进入新阶段重置时钟。预算用于一次有界研究，不是完成时长保证；不足 60 秒不启动比较。旧 `capacity` 的 90/75 分钟、完整七表及 270 秒最终余量合同，和其附加比较的 900 秒上限保持不变。

仅此研究模式的原生依赖准备分配最多 360 秒，外层 prepare 步骤从 8 分钟等量增加到 11 分钟；其他容量、官方默认研究、独立 Cargo check 与正式扫描继续使用原 180 秒分配。原生准备仍受总截止前 120 秒的较早截止约束，update 最多 60 秒，install 可使用同一窗口中剩余时间；保留 20 秒包/模块探针空间和特权 timeout 的 5 秒收尾差额。启动前及各命令前写入 `native-budget.json`，记录固定分配、父截止、实际准备截止和各命令 timeout；真实退出状态仍取各自进程收据。依赖、探针、包源和不重试行为保持原样，失败即阻止 metadata 与提取。新增时间用于容忍已观察到的下载耗时变化，不保证包管理完成，也不把未确定的下载原因认定为镜像故障；消耗仍从本次 150 分钟中扣除，减少后续可用研究时间。

提取成功只记 `EXTRACTION_READY`，研究另记 `COMPARISON_COMPLETED`、`INCOMPLETE` 或 `NOT_REACHED`；四个实际调用观察及其源码/官方库/数据库 metadata/已封存 checkpoint 的最终身份分别保存。默认结果与完整公共语义为 `NOT_REQUESTED`，`production_capacity=UNKNOWN`、`full_rust_coverage=HOLD` 始终保留。该模式只验证真实调用补丁，不代签完整安全结果、覆盖、容量、上游官方回归或运营启用；不上传 SARIF/数据库、不修改批次设置、不自动发布上游补丁。

#### 7.2.3 短原生准备诊断

原生安装在查询之前耗尽时，可单独授权一次默认关闭的 `CodeQL Rust Native Diagnostic`。仅可信默认分支手动设置 `execute=true` 才执行，整个 job 从实际启动计最多 10 分钟，共用现有扫描槽位。默认 `source_profile=runner-sources` 复用同一 `prepare_native` 的 360 秒窗口、四个包、模块探针、update/install 顺序、发行版源与 `Acquire::Retries=0`、`Acquire::http::Timeout=20`；不改变镜像优先级、HTTPS 参数或失败即停行为。总截止前保留至少 90 秒收尾/上传，不续期、不重试。该入口只识别实际 helper 提交和 runner，不装 Rust/CodeQL、不提取源码数据库，也不以业务源码 pin 代签已分析该源码。

当实际 Worker 与 partial 时间线已将耗时定位到获取路径时，可显式选择固定候选 `source_profile=ubuntu-archive-https`。候选只验证现有官方 HTTPS archive 路径能否完成准备，不预设镜像、协议或代理是根因。其 native 窗口固定为正式生产相同的 180 秒，源副本准备也计入此窗口；job 总计 600 秒、父级 90 秒保留、20 秒探针空间和 5 秒特权收尾差额不变。两种 profile 与固定预算由代码绑定，并在 job-clock、terminal 和源收据中一致记录；不接受任意 URL、APT 参数或预算输入。

候选只接受有界、完整、普通文件的已支持 Ubuntu Noble 源布局。现场读取源原字节，在源码及 artifact 目录之外新建临时副本；仅将 `ubuntu.sources` 中唯一的 `URIs: mirror+file:/etc/apt/apt-mirrors.txt` 字段改为镜像列表已包含的精确 `https://archive.ubuntu.com/ubuntu/`。该 stanza 的 suites/components/签名字段、security stanza、其他来源保持原字节；不从受限投影反造源文件，不修改 `/etc/apt`、信任、认证、HTTPS timeout、pipeline 或重试策略。重复字段、续行、混合 URI、缺失候选或未知布局拒绝启动 APT。update/install 只通过命令行指定副本 sourcelist/sourceparts；包与模块集合不变，失败即停。

`native-source-profile.json` 保存固定 profile/目标、原输入与有效副本的文件集合及 SHA-256、唯一替换计数和最终前后核验；私有源正文不进入 artifact 或异常文本。所有退出路径清理副本，身份或清理失败保持未就绪，原 APT 失败仍保留实际退出。默认诊断与研究调用保持原行为，正式采用范围见 §7.2.6。候选成功只证明本次四步准备在该路径与预算下完成；它同时改变 host 与 scheme，不能单独归因 HTTPS，也不能替代真实生产、37 条结果、Linux 覆盖、官方上传或平台关联。

只为本入口开启 `Debug::pkgAcquire::Worker=1`。运行时的可选观测器在任何 stdout/stderr 字节落盘前投影白名单：URI 状态/重定向/完成/失败、允许的数值/布尔获取配置、包与模块版本；配置帧仅保留必要键，URI 去除认证、查询和片段。未知格式、过长行及不在白名单内的内容只记长度、摘要与丢弃原因，绝不原样回退。普通调用不传观测器，沿用原字节采集。正常读取、无末尾换行与失败后 drain 均经过同一处理和共用 16 MiB 证据上限，预留 1 MiB 保存失败收据与摘要。

每条投影记录接收端 wall/monotonic 时间、原进程流 byte offset/长度和投影文件 offset；这是接收时序，不是服务端时钟或精确网络耗时。记录实际 APT 版本、源/镜像配置的受限字段与文件摘要，实际命令的 601 配置投影是有效值观察，缺项保持未观察。对固定 archives/partial 目录执行有自身短 timeout 的只读元数据采样，最多每 10 秒一次，只读普通文件名称/大小/mtime，不读取包内容、不更改权限；不可读、超限或变化均显式保留。观测、采样及清理时间都计入原截止，不能据普通进程组清理推定 root apt 子树已空。

仅在四条原生命令全部成功、证据正常封存时记 `NATIVE_READY`；失败保留原命令退出/超时以及已收集投影，观测缺项单列，不由丢弃条数推断网络原因。开发测试覆盖分块/尾部过滤、配置白名单、限额、父截止及失败停止，真实下载行为由单次 hosted 诊断核实。任何终态均不改变 Rust 覆盖 HOLD、生产容量 UNKNOWN 或批次配置；成功也不自动启动完整研究。

#### 7.2.4 候选库完整工作负载研究

Trace refs: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89；承接同一调用补丁研究的完整查询、身份和失败采证要求，不新增玩家功能或生产库选择。

`mode=candidate-workload-research` 是默认关闭的独立手动分支，仅在 `execute=true` 时运行，不接受附加比较开关。保持 §7.2.2 的完整业务 pin、官方 CLI/bundle、原生准备、锁/guard、features/targets、归档与 Action 资源；官方 Action 仅提取并 finalize 数据库。提取 checkpoint 沿原门槛检查并单独封存，候选结果不得冒称官方默认查询结果。

整个 job 从实际 hosted 起点绑定 90 分钟。在提取前只分配一次至多 75 分钟的绝对分析截止，并按当时剩余时间减 600 秒结果验证/收尾预留向下缩短。提取、私有布局、解析、编译和原 37 默认查询加原 7 公共见证的单批求值共用该截止；checkpoint 后不重置。所有 helper 子进程仍受总截止前 120 秒的证据边界约束。最终 job 成本包括上传与 post，独立读回实际完成时间核验至少 270 秒余量；阶段完成时间不能代签该余量。

提取的绝对截止由候选分支专用入口监督器执行。保留固定官方 `uses`、Node executable/参数、cwd、输入与 `GITHUB_STATE`/`GITHUB_OUTPUT` 等命令文件；显式 step env 加载受信 preload，同步等待监督器并阻止外层再次执行 main。监督器在真实启动时复核原预算，以独立进程组执行一次未修改的官方 main；子 Node 在 main 前再次检查原截止。沿现有 owned-process runtime 处理采样、TERM/KILL 与失败收据，收尾时段计入分析窗口，启动延迟不能获得新窗口。原 `timeout-minutes` 仅为外层保护。post 保持官方入口且不受过期分析截止阻挡；旧模式不采用监督路径。

Action stdout/stderr 继承 runner 通道以保留命令处理与 masking，不把未遮蔽流另外写入 artifact。监督样本、入口身份和终态写入尚未封存的官方 256 MiB 目录；不能修改已封存的私有 prepare 证据。只有监督成功、真实 main 退出 0、进程组收尾可信且原截止内完成，提取 checkpoint 才可准入。超时、外层取消/消失、监控或收据失败均保持不完整；不能因 Action step 或后续采证单独成功而放行。

本模式使用独立研究版本 `qualifier-lines-constraint-target-qualified-v7`，先完成并验证原 v3/v5/v6 准备链，再从真实 v6 shared 树的独立副本形成研究最终候选。共享 helper 的 `WORKLOAD_VARIANT` 继续固定为 `qualifier-lines-constraint-matching-v6`，供 §7.2.6 生产路径复用为真实基线。基础组件仅在采集树外复制官方 `rust-all`，先按 §7.2.1 核验原 qualifier 补丁及唯一 `TypeInference.qll` 差异，再应用独立固定摘要的 arity 补丁。它仅把原参数个数等式封装为 `bindingset[ce, target]`、`pragma[inline_late]` 检查，使调用及路径解析目标绑定后再展开比较；目标唯一性仍量化同一调用的全部 `ItemNode` 解析结果，原资格条件、类型路径及返回集合不变。该中间 v3 `TypeInference.qll` SHA256 仍为 `d502d88bafabca4c3b41e173c362ce80660bd8c096ee95e53a47f124deaf33fb`。原 qualifier 与 arity 的前后摘要分别保存，旧 capacity/qualifier 对照仍使用原单文件补丁。

随后向 `codeql/rust/frameworks/stdlib/core.model.yml` 增加原有唯一通用 summary：`["<core::str>::lines", "Argument[self].Reference", "ReturnValue.Element.Reference", "taint", "manual"]`。固定原文件 SHA256 为 `756cb0231b80f70c944cef16ed8c7db449580629b70328ec1e776d4c3f206400`，新文件仍为 `4a2534bf0f566206409fa9a9f24eca3b2744c8f79b33409689f680bb4f504734`；中间 v3 `rust-all` 的完整文件集合及前后摘要仍恰有 TypeInference/core.model 两项差异。该 summary 表示 `&str` 内容到 iterator 元素所引用内容的派生 taint，不证明字符串值相等、实际行数或运行时一定产生元素；它不引用业务路径或见证名称。

v5 中间层沿用 v4 的 shared helper：在采集树外复制固定 `codeql/typeinference` 0.0.42，仅允许 `codeql/typeinference/internal/TypeInference.qll` 一处差异：原 SHA256 为 `2a6d0a6be96444593e6294cb848633256f228f0ff137d42357f1ee4b92724378`，结果为 `4a65a578b82c8fa739c2a802b2cc6763cebc34cb67be9008fd396d5dbb5bd47b`，补丁为 `6b5a4cd3443caca8e994f05aa3c57e34b23c6fe3dfdc27851eb5f5c6ebf3c36c`。来源是上游 [34c3f5b](https://github.com/github/codeql/commit/34c3f5b74d3acee241d57c596044de3f1a0c8918) 的两个 helper hunk；这一层不包含 ConstraintMatching 父重构、ParenExpr 删除或测试预期变化，不修改 extractor、dbscheme 或原官方库。

该改写保持 `satisfiesConstraint0` 的量化域、歧义分支及原约束匹配条件；新增 helper 只先绑定本次 term/constraint 与同一抽象的候选集合。对原 `forall` 域内任一行，外层同一 constraintMention 已是新增存在条件的见证，因此右侧与原条件等价，空域及非歧义分支不变。计划验收须检查真实连接键及递归依赖，不能仅依据 helper 名称；原官方 type-inference fixture 与未改 expected 必须对官方及实际新组合做回归，代表计划与小 fixture 均不证明全仓收敛或性能改善。

v5 只在上述完整基础上应用固定两文件注解补丁（SHA256 `15d67391f614b0ef8ef5cc6ff4a2b404bb98b4656b33ff41a4f6292db8ec1c21`）：关联函数的 `assocFunctionCallTraitCandidate` 从 `nomagic` 改为 `inline`，`PathResolution.qll` 的 `relevantTraitVisibleLift` 从 `nomagic` 改为 `bindingset[trait, element]` 与 `inline_late`。全部谓词体、名称与参数数关联、显式 trait 排除、端点域、图边及最终配对不变；不为前者添加双端 bindingset，因为端点域仍需投影存在性。最终 Rust TypeInference SHA256 为 `db190947441166d6a24217f4dd3bdeafdf8965ca4e11998eba54080bbf91016b`；PathResolution 原 SHA256 为 `c83ebb64d9a10628ec37afaac3543ad0e2ac36f1297cffd4e85e29ea66f9fa8d`，新 SHA256 为 `222a5a43252a4055a66b38d70b2177668e3ebdbd918de67eeb7e9c206cc9ebbf`。当前生产按 §7.2.6 复用包含该注解的 v6 准备链；v3 中间层准备与验证、旧 qualifier 对照及 WASM helper 保持各自固定合同。

v6 在真实 v5 通过验证后，仅对同一私有 shared 文件叠加上游 [1c4fea90](https://github.com/github/codeql/commit/1c4fea90af949111bc1c625f1e72de56a12c7712) 的 ConstraintMatching 父重构，将共有约束匹配逻辑集中到同一模块。最终 shared SHA256 为 `8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33`，其余 Rust/shared 文件与 v5 完全一致。原 API patch SHA256 为 `ba47e346672cd8281b3a6a3d94647b0d00d733a273311799b0533dc4b0fd37db`；补充私有包路径头的原 8-hunk patch SHA256 为 `3880e57052c74d8af7f155a7abc8b721eef0725463897647809fd10b28d3a0ba`。仓库应用补丁 `codeql-rust-constraint-matching.patch` SHA256 为 `e6563cf895df0f662444795cf5c8f2358804784d667ccce4b11fa336ffde7db8`：只拆分不变上下文并按固定 v5 输入重算行号，保留原全部增删行的字节与顺序，相同输入产生逐字相同的最终文件。应用前验证补丁与 v5 基线，使用 `--forward --no-backup-if-mismatch --fuzz=0`；最终完整集合和 mode 校验不接受备份文件或其他变化。已有 RA/fixture 结论只能随相同最终库字节复用，实际新 helper 的补丁应用和原 44 条解析仍须验证消费路径。

v6 基线的准备、恢复与结果记录继续绑定其整体身份，`shared-helper-identity.json` 和 `trait-visibility-identity.json` 仍明确标识 `qualifier-lines-shared-trait-v5` 中间层。恢复先用当次官方 Rust 快照重验保存的 v3 两文件基线及实际 trait 注解树，再用当次官方 shared 快照和保存的 v5 shared 快照重建旧 helper 身份。旧 shared 路径验证器保持原签名、路径检查、固定摘要及返回结构；只将快照比较体抽为纯函数，不能把实际 v6 目录冒充 v5。`constraint-matching-identity.json` 绑定这两份 v5 收据摘要、上游/补丁来源、shared 前后完整快照和唯一 `4a65… → 8b32…` 差异。原 `prepare_constraint_matching`、`verify_workload_identity` 与 `workload_closure` 保持真实 v6 的准备、校验和选根合同，`candidate-typeinference` 始终保留通过该验证的 v6 字节。

研究在真实 v6 树和原 ConstraintMatching 收据验证通过后，另复制完整 `candidate-typeinference` 到 `candidate-target-typeinference`；复制结果须先与 v6 全树一致，再应用固定 2878 字节、SHA256 为 `96dcb5da3818ce129924cc9f80df366cf1517613acc19145df27ea2a65eaed2d` 的 target-qualified 补丁。只允许 `codeql/typeinference/internal/TypeInference.qll` 从 `8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33` 变为 `8f8ac05fb73efc3d7ebfd3d9ab452e90c13d61c7e651379f57f88b4400080ca9`，文件集合和所有 mode 不变；应用仍使用 `--forward --no-backup-if-mismatch --fuzz=0`。原 v6 树不被覆盖，`candidate-rust-all` 保留既有三文件变化；研究实际选择的两包相对官方仍恰为三个 Rust 文件与一个 shared 文件变化。

新增 `target-qualified-identity.json` 绑定 v7 最终版本、真实 v6 全树、原 `constraint-matching-identity.json` 摘要、固定补丁及 target 全树。旧 v6 快照的 `mode` 字段仍表示 Git 执行位（`0o111`）；target 身份层另为官方 shared、v6 和 target 的每个普通库文件记录完整 POSIX 权限位（`st_mode & 0o7777`）并要求三方一致，不扩展为目录权限或 uid/gid 校验。研究 prepare、run、恢复和 `finally` 均复核 v6 与 target 两树及这条收据关系，`private_inputs` 同时纳入两棵 shared 树；恢复必须重新调用原 v6 验证后再验证 target，不能仅以已保存的 v6 快照或改标签的收据证明基线。研究 closure 将 `codeql/typeinference` 指向 `candidate-target-typeinference`；§7.2.6 的生产消费者独立固定同一 target 最终版本，原 `workload_closure` 继续表示真实 v6 基线。44 条研究实际解析必须选择 target 根和既有私有 Rust 根；旧研究 variant、伪造或改写的基线、target 回退 v6/官方、额外差异或缺失收据均拒绝，即使重新封存也不能放行。

旧 v5 完整研究运行已超时，默认结果与公共语义未完成，详见 [#4989](https://github.com/nil-studio/oasis7/issues/4989)。旧 v2/v3/v4/v5/v6 收据和结果保留原身份与终态，不代签 v7 研究；本研究不直接启用生产批次，生产接入按 §7.2.6 验收，现有 WASM shared helper 的固定契约保持。

执行计划核对必须使用对应 CLI 和实际编译选项，检查真实目标连接先于仅按 arity 的宽连接，并核对两处 TraitIsVisible 实例：完整 candidate/Lift 不再独立物化，最终由可达关系绑定端点后按原配对过滤，端点存在域与图边保持。TC 起点包装的消去或反向求解须分别证明等价，不能只以谓词名称消失判定优化成立。ConstraintMatching 重构另需核对实际约束位置、连接/投影和递归直接反馈；`inferType` 块未变不能被表述为热点已消除。原官方 type-inference/path-resolution fixture、未改 expected 与实际候选组合需覆盖相关正反例，分别记录结果多重集和原 CLI 结果，不将既有 RESULT 失败改写为通过。计划或小 fixture 均不证明全仓耗时、内存或收敛改善，单查询编译通过不代签原预算下的 37+7 执行、语义或生产容量。

默认查询从冻结的官方 37 项 suite 取得，保留全部原 QL 字节、相对支持文件及 suite；私有源码 pack 明确列出包装差异，排除 `.qlx`、发布编译缓存、嵌入依赖和发布锁定绑定。默认查询的 14 项依赖包含 `suite-helpers` 1.0.61，其 `library:false` 角色单独严格核验，不放宽原 13 个 library 闭包解析器。44 条查询逐项验证实际候选库、完整依赖和 dbscheme；源码清单、命令参数或解析成功不能代替实际编译与求值。执行显式使用 `--no-use-precompiled`、独立编译/common cache 和 Action 实际资源。

七表中的 Image 使用显式 `image-v2` 合同，其他六表保持原字节和 oracle。参数经公开 `Param`/`IdentPat` 名称接口选择，不把参数或比较式的 `toString()` 展示文本当名称。完整冻结 `expectations.rs` 中，同一 `contains_line` 函数的原 `output`/`expected` 参数、`output.lines()` 的循环绑定与原 `EqualsOperation` 左右操作数必须共享 AST 所属关系；LHS 是该循环的 `line`，RHS 是该函数的 `expected`。保留两处真实 format 展开、两个 digest 到各自展开的 localFlow、两处精确 `contains_line` callee 和跨函数负例。原 `expected` 到同一 RHS 用 localFlow，原 `output` 到同一 LHS 用公开 `TaintTracking::Global<Config>`；不能把起点改为循环变量或给查询添加业务流边。每次 `flow(a,b)` 绑定具体源/汇，Config 同时纳入需检查的负例端点，不能用配置之外的节点产生天然零值。新增 expected→LHS、output→RHS 及两个参数→整个 bool 比较结果的零负例；结构见证与新端点标签显式带 v2。prepare、私有布局、query-inputs、oracle 和终态记录同一合同及 QL/标签预期身份，旧 v1 查询和历史失败保留在原版本及旧模式。

该模型的开发回归使用真实固定源码小样本及独立双 receiver、同名非 stdlib `lines` 控制；后者须先证明来源到实际 receiver 的传播，避免把没有来源的零值当模型隔离通过。小样本只验证候选模型及查询，不代签完整原 crate/workspace、显式 `into_iter`/`next` 或整个 iterator 家族。模型改变影响全部默认查询，后续完整结果必须由同一新候选的一次37+7运行取得，不能拼接旧37结果。

同一 finalized 数据库以一次 `run-queries` 执行44项。37项均须保存并核对 BQRS 身份、完整 info/decode；七表按上述版本的完整正反例、行数和来源 oracle 逐项验证，Image v2 不覆盖或追认 v1 的失败。仅对默认37项在 runner 本地 interpret-results 并调用原 SARIF 校验；SARIF 留在不上传的数据目录，artifact 只保存其 hash、size、受限规则/计数摘要及验证收据。数据库与源码 archive 同样不上传。任一结果不符合时保留原失败和其余已计算观测；解码、进程、预算或证据错误停止后续执行，未执行、超时、真实零行与 oracle FAIL 分开记录。

本候选模式的 `candidate-evaluator-v1` 观测只接入上述唯一批次，固定官方 2.27.4、`--evaluator-log-minify`、日志级别 3 和禁用 tuple counting；不额外运行查询或预热。固定 2.27.4 的 summary collector 会读取 stage-result usages；level 1/2 缺此字段，实测 level 1 summary 出现 NullPointerException，因此使用最低提供 usages 的 level 3，不开启 level 4 cache-miss 或 level 5 RA。候选专用 capture 将 owned FIFO 的原始 evaluator 字节流无损编码为 gzip，固定 zlib level 6、windowBits 31、memLevel 8；实际 encoder 和收据使用同一个候选专用常量。Linux memfd 中的 encoded payload 至多 47 MiB，codec 和传输有界缓冲合计保守预留 1 MiB。原流和编码内容均不写普通文件、artifact 或控制台。默认 raw-only `Capture()`、WASM 与共享 runtime 保持原行为。

level 6 用更高编码 CPU 换取压缩容量，不能承诺总运行更快或任意完整流都能装入。windowBits 31 是有效窗口 15 加 gzip 封装；固定窗口和 memLevel 决定主要 codec 状态分配，参见 [zlib 参数说明](https://zlib.net/manual.html)与[内存说明](https://zlib.net/zlib_tech.html)。每次输入仍至多 64 KiB，feed 与 finish 返回的编码块继续受 128 KiB 检查；返回后检查不是 Python 临时分配或 RSS 的硬上限，也不能用单次整流的 deflateBound 代签 streaming 返回峰值。encoder 在结束后释放，decoder 仅在后续重放阶段拥有状态。容量、输出块或 finish 超界保持原拒绝与清理规则，禁止截断、raw fallback 或扩大预算。`gzip-capture-v1` 的身份含义不变，历史 level 1 原件不重解释；公开固定输入的容量回归须冻结输入生成规则、原始长度/SHA 和测试上限，验证同一流在旧 level 1 超限、新 level 6 完整编码并独立解码，不能在测试执行时按输出重算上限或外推私有全量流与 20 秒摘要容量。

`gzip-capture-v1` 将原流的实际观察字节数、SHA256、header/footer 和传输 EOF，与编码存储的字节数、SHA256、gzip finish 及 memfd seals 分别记录。`encoding_input_bytes` 及其摘要只表示成功交给编码存储路径的原输入前缀，编码未完成时不证明该前缀可解码；`encoded_bytes` 才是 memfd 的实际长度，不沿用 raw `retained_bytes` 的含义。停止生产并清理 owned 进程组后，关闭父写端、有限 drain 到真实 EOF，再完成 gzip 编码并封住 memfd 的写入/增长/截断。gzip finish 的输出也受 47 MiB 上限约束；编码长度不是原流长度，编码结束或 seals 成功也不能代替原流完整与查询成功。

固定 2.27.4 的真实 query/STAGE_RESULT 小日志已验证：非 seekable proc-pipe 输入以及 gzip 解压重放与 regular/sealed memfd 输入的官方摘要逐字相同，修正原“摘要器必须 seek”的静态假设，依据见 [#4989 的续接记录](https://github.com/nil-studio/oasis7/issues/4989#issuecomment-6874145775)。候选仅从已 finish、完整封存的 gzip memfd 有界解压，通过 `/proc/父pid/fd/readfd` 向官方摘要器重放原字节；不将 evaluator 的实时 FIFO 接给摘要器，不落地解压副本。解压须核对 gzip 完整结束、没有额外编码内容，并将重放的原流字节数和 SHA256 与 capture 相等绑定。小样本只证明该输入路径兼容，不代签所有 44 项的日志形状、311 MB 完整流的压缩比或 20 秒全量摘要容量。

临时原始流存储及处理、持久化私有证据和原终态预留共同计入 64 MiB。memfd 存活期间仍保守预留完整 48 MiB，其中 47 MiB 给编码 payload、1 MiB 给 codec/传输；进度、摘要、资源样本及观测收据另受合计 8 MiB 子限，不能挤占终态保留区。候选模式从此子限中预留 4 MiB 给数值投影及随后官方摘要消费的 framing、受限解析和固定关联状态，两者须交接所有权；其余持久化观测证据仍不超过 4 MiB。这是有界输入和保留状态的保守分账，不是 Python 分配器或进程 RSS 的硬上限。只有编码容量将超 47 MiB 时，停止存储新增编码内容，继续排空 FIFO、累计原流实际字节数和摘要，并继续下述白名单数值投影；已有编码存储的字节数、摘要和完成状态另行冻结。该状态记 `MEMORY_LIMIT`、capture/观测 `INCOMPLETE`，不据此中止原唯一 batch；其计算、BQRS 校验和七表 oracle 仍受原截止、证据空间及失败规则约束。不得对不完整编码调用官方摘要器，不伪造丢弃部分的 footer、事件、gzip 尾部或 summary；即使计算及 oracle 通过，研究总体仍为 `CANDIDATE_RESEARCH_INCOMPLETE`。写入、codec 异常、身份、证据空间及清理不可信等其他失败仍按原规则处理，不能套用仅容量不足时的继续排空例外。

数值投影只在上述候选批次的 `Observation.start()` 显式接入同一 `Capture`，从第一字节沿既有 FIFO/framer 观察，不另读数据库原生日志，也不增加查询、摘要进程或截止。默认 `Capture()` 与 WASM 的 `Progress` 保持原行为。固定 CLI 2.27.4/raw log 0.5.0 下只选择一个源码 family：`appearsAs` 的完整 alias 在有界编译器 suffix 之前须含精确 `TypeInference` namespace 分量，末成员须精确为 `inferType/2`；泛型参数内部的同名字样、相似 namespace/member、其他 arity 或成员之后额外 namespace 均不准入。suffix 只作为 opaque 身份，不解释为成本或关系语义；准入 alias 立即转为 SHA-256，构成本次运行的精确映射，不把小 fixture 的 alias SHA 当作完整 v4 批次已经观测的身份，也不使用 `predicateName` 后备匹配。

同一显式数值观察路径在 `Capture.feed` 的既有处理体外计量 wall 与当前线程 CPU，将整次调用及输入字节归入固定三个桶。候选 gzip 路径按实际 encoded 存储是否越界归类：进入调用前已越界为 `AFTER_LIMIT`，本次编码写入首次越界为 `CAP_CROSSING`，其余为 `BEFORE_LIMIT`；恰好填满仍属 `BEFORE_LIMIT`，跨限调用不拆分。不按原流字节数推测编码容量，也不因 gzip 缓冲延迟输出而提前计入越界桶。仅在 gzip finish 时越界须在独立的编码完成状态中记录，不虚构一次 feed 或回改此前桶。默认 raw 路径的原有分桶行为保留。仅保存各桶次数、字节数和时钟差值累计；时钟不可用、读值或差值无效时保留未测次数及已测小计，完整总量为未知，不写异常原文，不改变原 capture、查询失败或验收状态。无调用的桶记 `NOT_OBSERVED`。计量采用固定数量的计数器，并写入原 observation 控制收据，不新增逐调用明细、文件、解析或预算。

`Observation.run_batch` 另在原唯一 `runtime.run` 调用外以父进程时钟计量，`finally` 覆盖成功、超时、异常及原清理。该 CPU 包含调用期间父进程全部线程的监督、采样、feed、drain 和进程收据写入，排除子进程/PGID CPU；不包含整个 job，也不能与 feed CPU 相加或相减当作其他组件独占成本。回调 wall 包含处理期间的等待与调度，时钟读取和累计本身也有观测开销；这些数据不能还原未插桩运行或证明旧运行的因果。计量完整性不成为新的研究或启用门禁，默认 raw-only `Capture`、WASM 和共享 runtime 保持原合同。

原始记录的 `PREDICATE_STARTED.eventId`、`PIPELINE_STARTED.predicateStartEvent` 与完成记录的 `startEvent` 建立同一次流内关联；每个 predicate event 独立记录，多 alias 共享一个事件只投影一次。数值收据使用 `candidate-numeric-v2`，完成和未完成 predicate 样本增加 `ra_identity_sha256` 与 `predicate_start_offset_ns`；前者对原 `raHash` 的 UTF-8 字节使用与官方 summary 投影相同的 SHA-256，缺失或空值为 `null`。同 RA 的不同事件不合并，alias 摘要也不等同于 summary 的 predicate-name 摘要。pipeline 在启动时复制父实例的这两项字段，完成尾样本和未完成样本据此在 predicate 样本裁剪后仍可关联；沿用原活动表、尾样本和收据上限，不增加历史身份表。旧 v1 收据没有这些字段，不能回填或推定旧执行计划相同；外层观测与 capture/summary 合同不变。

`predicate_start_offset_ns` 以同一原生流首个通过既有校验的 `LOG_HEADER.nanoTime` 为原点；零和负的原生 `nanoTime` 都是合法值，重复 header 不重置原点。顶层 `last_observed_offset_ns` 只取流中最后一条未触发现有数值校验错误的事件，包含字段、枚举、完成状态、关联和容量检查；不用最大时间，也不由报错记录推进。未完成 predicate 的 `observed_open_span_ns` 只到该末次有效事件；它不是完成耗时、CPU 或到父进程超时的时长。缺少原生端点、负差或差值超出有符号 64 位非负范围时，仅相应字段为 `null`；多线程事件可按到达顺序交错，后续合法端点仍可恢复可计算信息。末次事件晚于 header 但早于某 predicate 起点时，顶层偏移仍可有效，该 predicate 的观察跨度未知。不得钳制、取绝对值、改用最大时间或补父进程时钟。新增字段缺失或无法计算不增加校验错误或完成门槛；旧 schema 原本拒绝的缺 `nanoTime`、错误类型和非法关联仍保持原错误，未闭合或缺 footer 仍保持 `PARTIAL`。

其余持久化白名单仍只有固定 probe 编号、alias 摘要、event/layer/generation 数字、配对的非负 `nanoTime` 差、分别命名的 predicate/pipeline `result_size`、固定完成/异常存在状态及有界计数和尾部样本；不保存 QName、路径、RA 原文、异常文本或 `queryCausingWork`，不按 query 分摊整批共享关系。省略 `resultSize`、generation 或计时不能补零；成功时可省略的 `completionType` 与显式失败/不完整状态分开保留。单个 pipeline 的零结果之后，同一 predicate 更晚的 generation 仍可有正值，因此 `result_size` 不是通用 delta 或收敛判定；较大 generation 也不直接证明慢。pipeline 与 predicate 区间可能重叠，不相加当作 CPU、全批次耗时或完整排名。

framer 仍最多保留 2 MiB 未完成行；新增数值解析每条最多 64 KiB，并分别固定 alias、活动 predicate/pipeline、包括已关闭事件的目标身份、完成尾样本及拒绝计数的上限。超长记录有界跳过至下一换行，未知 schema、重复/不完整关联、表饱和、解析异常或缺少真实尾部均使数值观察 `PARTIAL`，只保留固定原因和已有允许字段，不抛异常遮盖原 batch 首失败，不回退原文。未命中记 `NOT_OBSERVED`，不能称该源码 family 未执行。数值状态独立单列，是否命中或投影完整不成为研究完成的新门禁；原 capture 与官方 summary 的准入不变，部分投影也不能提升它们或研究、生产验收。数值详情使用原私有 evidence 下单独至多 32 KiB 的控制收据，包含流摘要/字节字段的最终编码、原进程/控制收据及子限一并核验，序列化与清理仍受原截止约束。

`candidate-numeric-v2` 可附加独立版本的 `non_success_global_v1` 诊断，仅解释全流原有 `NON_SUCCESS` 计数增量；此作用域不受外层目标 family 的样本范围限制。固定 23 槽：索引 0–9 为 `PREDICATE_COMPLETED`，10–19 为 `PIPELINE_COMPLETED`，两组均按 `MISSING / IMPLICIT_SUCCESS / SUCCESS / FAILURE / INCOMPLETE` 的顺序、每类先 exception 键不存在再存在；20–22 为 `QUERY_COMPLETED` 的 termination 缺失、`COMPLETED_SUCCESSFULLY`、其他已通过旧类型校验的字符串。前两类 termination 不适用，query 不接纳 completion 或 exception 字段。仅在原两个 NON_SUCCESS 增量点同步计数，失败 completion 与 exception 同时存在仍只计一次；空 exception 字符串仍是键存在，错误类型和未知 completion 仍按原 validator 拒绝。不得按记录有效性、目标命中或是否已触 capture cap 重新筛选，也不改变原时间端点、关联、错误或完整性判定。

每槽与原总计均限制在有符号 64 位非负范围内，饱和须显式标记；完整分账满足 `min(2**63-1, sum(counts)) == errors["NON_SUCCESS"]`。内部只保留固定数量计数器，报告仅有固定 schema/status、数字数组和饱和标记，不保存异常名、termination 原文、事件/query/RA/路径身份或逐事件明细。旧报告先按原 31 KiB 算法完成裁剪与状态判定，再在原 32 KiB 控制收据内附加该诊断及原流身份；不得因新增字段挤掉旧样本或新增 RECEIPT_LIMIT。最终编码仍须有界，诊断不可用或余量不足时仅用固定 UNAVAILABLE 原因，不覆盖原错误、补零或建立新门禁。旧原件缺少该字段表示未采集，不反推历史原因；计数增量不等于唯一事件数或失败查询数。

持久化证据空间不足、写入、组身份或有限 drain 不可信时，仍记观测不完整并停止后续子进程；编码存储上限耗尽不能解释成查询 OOM 或语义 FAIL。先冻结原 batch 退出及首失败、保存已有 BQRS 和核心身份，再按完整 capture 的准入条件，在原证据截止内尝试至多一次官方 `generate log-summary --format=predicates --minify-output --utc`。整个摘要路径共用一个至多 20 秒的绝对截止，包含有界解压、pipe 写入、官方进程及 worker/pipe/进程组清理；剩余时间/空间不足记 `NOT_REACHED`，不重置分析截止或增加收尾预算。

摘要启动前只准备 owned pipe 和 sealed 输入；重放 writer 必须在现有 runtime 的 `Popen` 已返回后，由 `Summary.poll` 首次启动，避免活跃 writer 线程跨越 fork/preexec，不修改共享 runtime。正常完成、错误、超时和取消均须在同一截止内停止并收齐官方进程组与 writer，关闭其写端，再对父进程保留的 readfd 做有界读回检查：必须真实 EOF 且无残余字节。writer 完整写入、重放摘要匹配或 CLI 提前 exit 0 都不能单独证明完整消费；残余、未到 EOF、断管、writer 未退出或组清理不可信均不得记摘要完整。官方摘要原有版本/schema、输出边界及成功条件继续同时满足。

官方 summary 的 stdout 与 stderr 均在首次持久化前受限处理：明确核对 2.27.4 及实测版本的 schema，拒绝重复键、未知字段/类型、非有限数、截断和非法编码；错误只写固定类别、字节数及摘要。名称、路径、RA、源码位置和异常自由文本不原样保存；只输出受限数值/状态、hash 及冻结查询身份。literal `millis` 与非负迭代小计分列，缺失或 `-1` 保留未知，不补零、不把 recursive layer/member 重叠成本相加，不跨运行连接 opaque identity，也不宣称独占 CPU。未知形状保留部分观察，不能由小 fixture 的已知形状推断完整批次兼容。

通过原字段、类型、枚举和嵌套数值校验，但未匹配冻结完整形状签名的记录，仍计入 `unknown_shapes` 并保持 `PARTIAL`，不进入 RA 索引或排名。只为首次此类记录保留一个至多 2 KiB 的诊断：原记录序号、原行 SHA256、已验证的顶层字段类型集合，以及 `evaluationStrategy`、`completionType`、`isCached` 中实际存在的固定枚举值。没有通过基础校验的记录不进入该诊断；不保存字段值、嵌套键、名称、路径、RA、查询或异常文本。诊断使用原 512 KiB 控制分账，包含构造和编码暂存，不增加 4 MiB 活跃额度、证据上限或 20 秒截止；容量或构造失败只留固定 `UNAVAILABLE` 原因，不能覆盖原未知计数、原始首错或建立新门禁。后续未知记录不覆盖首条，终态释放旧状态后再解码该受限诊断。旧运行仅有拒绝 SHA 而无此诊断时，不能反推形状、补造签名或追认成功；新增签名仍需实际来源及独立核对。

摘要累计 stdout/stderr 字节数与 SHA256 覆盖整个实际观察流，不受 48 MiB capture 存储上限限制。消费者逐记录处理并丢弃原文，保持 2 MiB 单条、100000 条记录及原 20 秒共同截止；累计流超过 48 MiB 后仍逐项执行完整 schema、失败状态和尾部验证，不能只校验前缀。两流 pending 共用活跃额度，framer 在解析前移交并清空当前 pending，不保留整段 split 副本；当前行转换为 bytes 时的 pending/bytes 同时存活计入 framing 峰值。每次 runtime 读块最多 64 KiB。

JSON 准入先在原 bytes 上识别字符串边界、转义和真实结构，不先整行 decode，不建 token 列表，也不使用为长字符串保留逐字符回溯状态的正则。长字面片段由固定字符类 search、find/count 跳过；字符串内的冒号、逗号和括号不作为容器收费。词法检查仅决定保守表示上界，后续仍由原 JSON/schema 校验拒绝重复键、未知字段、非法 UTF-8/转义、非有限数、类型/范围和截断，不能以预检通过替代字段验证。

设当前源字节数为 N，整行解码表示宽度上界为 K（1、2、4），各字符串按字面片段字节数与反转义单位数上界、自己的宽度及 UTF-8 编码长度上界分别计量。ASCII 源中的 Unicode 转义和代理对可产生较宽的值，不能沿用源的 K；一处字面宽字符也可能扩宽整行 decode。解析 tree 分账为每个字符串的内容表示加 128 字节、非字符串标量 token 每个源字节 4 字节，再加 `512 × (对象数 + 实际键值分隔数) + 128 × 数组数 + 64 × 实际元素分隔数`；空白和结构符不重复算作标量。对象/键值费用包含 object_pairs_hook 的 pairs list/tuple 与最终 dict 同时存活，以及解析器键缓存；后续校验阶段的 associations set 可包含44项之外的唯一 query，仍计入同一结构峰，不能按44个已知 query 免计。

按所有权分三阶段取最大值，再加另一流 pending：decode 峰为 `N + 2 × K × N`，为旧/新解码 writer 同时存活留额；parse 峰为 `N + K × N + tree + 2 × 最大单字符串内容表示`，包含反转义 writer/chunks 的暂存；校验/投影峰为 `N + tree + 最大单字符串反转义后 UTF-8 编码长度上界`。整行 decode 在 parse 返回后不再与后续编码机械累加，text_value 临时编码却不能漏算。固定映射的排序、行对象及其 JSON/UTF-8 编码同时存活继续计入下表控制分账。该模型是有界输入/保留表示的保守分账，不声明 Python allocator/RSS 硬上限；没有以未知私有失败行反推新系数。公开合法大 ASCII、标点字符串、转义等价、BMP/nonBMP、dense 容器与 late-wide 空白样例须验证不同表示和阶段峰；原3744项迭代及对应 delta、累计超过48MiB后的完整尾部验证继续保留。2MiB仍为外层记录硬上限，并不保证任意较小对象都能在状态额度内准入。

摘要接手同一 4 MiB 活跃额度，固定分账如下；不增加第二份活跃状态预留。

| 保留内容 | 字节上限 |
| --- | ---: |
| 65536 个两字节索引槽，至多 49152 个完整 32 字节 RA 摘要及四字节首次序号 | 1900544 |
| 冻结 schema/query 映射、两榜各 50 条受限编码行、至多 16 条复用/拒绝样本、首条未知形状诊断及控制状态 | 524288 |
| 当前 runtime 输入块 | 65536 |
| 两流 framing、逐记录源字节及解析暂存 | 1703936 |

索引槽仅保存无符号 16 位条目编号：0 表示空槽，1..49152 指向完整摘要条目；首次记录序号仍用 32 位，不能随槽位压缩截断超过 65535 的原记录序号。相较四字节槽，同一账本回收 131072 字节给解析暂存，增加的是内部解析份额，4 MiB 活跃总额、distinct 容量、槽数和探测上限不变。

schema 编码至多 64 KiB、query 映射至多 128 KiB，排名每行至多 2 KiB，临时第 51 项也计入分账；启动 Summary 前释放已经结束的 batch `Progress.queries`，其原 streams/counts 收据继续保留。RA 索引用完整摘要判等，每条最多探测 256 个槽；达到 distinct 上限后仍能查到旧 RA，新 RA 或探测饱和须以 `SUMMARY_STATE_LIMIT` 拒绝，不消耗额外截止。不得清表、截断摘要或静默省略后宣称 `PROJECTED`。状态或输入边界失败须保持 sticky `PARTIAL`，继续现有 runtime 的有限排空与全观察字节/摘要计量，不能恢复完整状态或覆盖原批次首失败。

状态限只增加一个至多2KiB的 sticky first-limit 对象，计入原控制分账及收据上限。固定枚举区分 schema/query、chunk/framing、decode/parse/projection、RA容量/探测与排名行分支；有界整数仅记录流行号、已接纳数量、原字节/另一流pending、真实结构/字符串计数、阶段峰及适用额度，或该分支的RA/probe/行编码大小。没有完整记录时不补长度，不保存原文、名称、路径或异常自由文本；预检拒绝不能标记schema已验证。诊断失败不得覆盖最早状态限或原查询首错，不新增验收门禁，也不回填旧运行的失败记录。

`Summary.summary()` 只承接 runtime 已结束进程与有限 drain 后的幂等终态快照；先保留实际 EOF、未完成 pending 长度及原错误，再释放 RA 表、framing 和不再使用的映射/编码状态后构造受限摘要。没有真实 EOF 的路径仍为 `PARTIAL`，不补 EOF；终态快照后禁止继续 feed，只允许重复读取同一结果。此交接覆盖取消、超时和 drain 未闭合路径，不能只对正常 EOF 释放状态。

任一 `Summary` 构造或摘要子进程启动前，先将已结束数值投影按原至多 32 KiB 收据写入并核摘要，绑定其原流字节数/SHA256，然后释放 `capture.projection` 的关联表与尾样本；保存失败时保留原对象并停止摘要准入。若写入成功但首次核验失败，`finish()` 仅核验已有同一内容，不覆盖重写：使用不跟随符号链接、非阻塞方式读取长度精确匹配的普通文件，至多读取 32 KiB 加一字节，核对读前后文件及路径身份和完整字节摘要；特殊文件、替换、长度或内容不同均拒绝。已冻结引用不重新写入或降为 `NOT_REACHED`。原数值投影的 `PARTIAL`、`NOT_OBSERVED` 仍单列，不新增完成门槛；默认 raw Capture、Progress/WASM 与共享 runtime 保持原合同。

每条观察由本次官方摘要流 SHA256 与 header 后的原记录序号共同标识。`raHash` 是内容身份，不能当执行事件 ID；官方允许其复用，名称也不唯一。所有合法记录按原序号保留各自 literal 计时并参与有界排名，复用仅计数和留受限示例，不据此拒绝、去重或合计成本，也不将同一记录复制归因到多个查询。

batch stdout/stderr 的受限查询进度保留 wall/monotonic 接收时刻、流完整性和冻结查询身份。该时刻含缓冲和观察误差，不是 CLI 内部阶段边界，不能用每 query 标签相加推断整体编译 wall。终态分别记录计算、oracle、capture、summary 与研究完整性；摘要成功不会提升原 timeout 或 `NOT_REACHED`，原首失败不会被后续观测错误覆盖。观测明确标记 `instrumented=true`，资源采样仍是观察下界；部分摘要只描述已观察记录，不能称为全批次完整排名或最后活跃谓词。

开发验收以新 600 秒 D0、独立小 fixture/DB/cache、2 threads/3072 MiB 验证真实 2.27.4 的 gzip memfd/有界解压/proc-pipe/官方摘要链，再覆盖原 candidate 接线及成功、未知 schema、encoded cap 与 finish 越界、重放长度/摘要不匹配、提前 exit 0 未消费、内存/证据边界、取消/超时、尾部 drain、异常隐私和 writer/FD/组清理。它只验收观测适配，不代签完整37+7、全量压缩容量、20 秒摘要完成或性能根因；旧 raw `MEMORY_LIMIT`/`INCOMPLETE` 不以新表示追认为成功，旧 2.27.3 profile 与其他 runtime 调用保持原行为。

执行前后检查完整业务输入、官方 bundle、候选整个库、私有 query pack、DB metadata 和封存 checkpoint。缓存/结果与受保护源码隔离。失败仍在原截止内保存已有有界日志、BQRS/解码和退出收据；官方证据保持 256 MiB 上限，私有证据为 64 MiB 且保留终态空间，部分、超限、身份不明或封存失败不得报告完整研究。研究不追加查询、不自动重试、不改写旧结果。

终态使用明确的 `CANDIDATE_RESEARCH_*`，分别记录计算、默认结果、公共语义、证据、资源、身份和余量；始终保留 `production_capacity=UNKNOWN`、`full_rust_coverage=HOLD`、`production_acceptance=UNCHANGED`。完整研究仅证明本次私有配置的实际结果，不等于官方容量、全 Rust 覆盖、上游发布或运营启用。旧 capacity、official-default-research、qualifier-research 的预算与行为保持不变。

#### 7.2.5 合法 WASM 单角色 metadata 诊断

本入口承接 CQ-R03/R04/R05/R06 的上下文、预算、失败可见和信任边界，仅观察 Cargo 项目归属与依赖图，不运行 CodeQL 或 Rust 编译。独立 `CodeQL Rust WASM Metadata` workflow 默认关闭；仅可信默认分支的 `workflow_dispatch` 且精确 `execute=true` 执行，权限限 `contents: read` 和读取实际 job 时钟所需的 `actions: read`，复用 scan-slot-1 队列，不取消已有运行。它不改变生产批次、required policy 或其他诊断预算。

固定完整业务源码 `7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef`、根虚拟 workspace 的 53 个不同成员及 `oasis7_custody_worker/recovery-fence` 角色。实际 Cargo 固定为 1.99.0、commit `5f94df4789f005f9a352888e8355ffc645b7ed0e`、host `x86_64-unknown-linux-gnu`；本轮首次采集 analysis rustc 完整 commit 并前后比较，不从 Cargo commit 或历史 sysroot 路径推断。项目声明的 1.96.0、源码、manifest 与锁文件不改。

首步用实际 hosted job start 与 GitHub 时钟绑定一次 1200 秒绝对截止；绑定失败拒绝，不退回当前时间加预算。所有 helper/源码获取、工具链安装、联网依赖准备、探针和身份核对都计时，准备最迟在总截止前 420 秒停止。仅剩余至少 420 秒才准入完整 300 秒观察窗口，其截止取准入后 300 秒与总截止前 120 秒的较早者；D1/D2 最晚在观察截止前 60 秒停止，尾部用于清理、after、guard 审计和封存。最后独立 120 秒留给 artifact upload/post，仍受原 job 截止约束。D1 最多 10 秒，D2 使用同一窗口剩余量；不重置、不自动重跑。helper 终态不代签完整 job，上传及 post 后须读回实际平台终态。

启动链使用仅此任务的闭合 Python bootstrap：先绑定时钟，再按固定工具提交获取白名单 helper blobs，核验对象身份后交接；业务 archive 按固定 commit/tree 与全部 blob 对账。它不调用会自行打印 git 命令的 checkout Action。网络错误、重定向和异常只记录受限类别、字节数与摘要，不保存 token、临时 URL 或异常原文。宿主真实 Cargo/rustup proxy 留在原位置并核摘要，以任务 CARGO_HOME/RUSTUP_HOME/target 隔离工具与缓存，HOME/CODEX_HOME 不改；不绕过 guard 对 real_cargo 位于 guard work 下的拒绝。准备与最终 offline metadata 使用同一 cache/proxy。联网准备仅在专用动作表中，原 guard/runtime/layout 的共享命令权限不扩展。

版本审计使用本任务 helper 内的投影 adapter，原共享 guard 源码、真实 Cargo 路径/摘要检查、参数约束及 inspect 均保留。仅在新 guard 的 counter=0、failed=0 且尚无调用记录时，对 owned regular wrapper/config 一次重绑；保存原/新 wrapper 摘要及 adapter helper 固定 SHA。wrapper 必须先核 helper 字节再执行，版本回调消费当次真实输出，在写入 actual_cargo_version 前严格校验版本、commit、header/date 一致性并规范化允许字段；未知字段/错身份先拒绝。finish_record 只追加白名单 bytes/hash/status，不删原字段、不改原错误或成功状态。原版本函数在返回前因超限、退出、UTF-8 或 timeout 失败时记 NOT_RETURNED、摘要未知，不伪造完整输出观察。

D1 从 custody cwd 对其 manifest 执行 `locate-project --workspace --message-format=json`；D2 对根 manifest 执行一次完整 `metadata --locked --offline --format-version=1 --no-default-features --features oasis7_custody_worker/recovery-fence --filter-platform wasm32-unknown-unknown`，两者均经过本轮安装的原 guard。根成员按完整目录集合核验，虚拟根 `resolve.root=null` 是预期；不把子目录当独立 crate。核对 custody 的实际 resolve features 与四角色交集恰为 recovery-fence、无两种 harness，并保存 normal/build/dev 边及 target 表达式。六个本地身份为 crates 下同名 manifest 的 oasis7_custody_worker、oasis7_custody_core、oasis7_agent_api、oasis7_client_api、oasis7_proto、oasis7_wasm_abi；它们不替代完整 53 成员。filter-platform 只过滤 resolve 图，packages 清单和每编译单元 features、宏或语义分别解释。

所有准备及执行路径从第一次写入起只保留允许的 manifest/cwd/toolchain/target/features、参数数量和摘要；异常、取消、临时输出和控制台同样适用，不能先落原文再删。合法有界 D1/D2 JSON 经批准图 schema 验证后保存完整依赖图；合法图的角色/成员不符另记 ROLE_CONTEXT_MISMATCH，保留实际 features 与固定失败项后使终态 INCOMPLETE，不把可诊断差异丢成只有摘要；未批准自由文本、非法或超限输出只留类别、字节与摘要，不回退原文。证据合计 64 MiB，含 guard audit/隐藏控制文件、元数据、前后身份、终态与清单；初始按普通 52 MiB、audit 8 MiB、配置/after/终态/hash 4 MiB 预留，启动前证明固定收尾结构够用，必要调整分账须保持总额。guard 原有 256 MiB 检查不代替本项合计上限。

仅两命令、身份、图校验、清理和封存均成立时记 `ROOT_WORKSPACE_ROLE_METADATA_OBSERVED`；准备失败、范围不符、超时或证据不完整分别保留。开发回归覆盖真实启动交接、固定身份、首写投影、取消/TERM/KILL、残留进程、写入失败、预算和 metadata 独立 fixture。该观察不证明 WASM 编译、proc-macro、extractor 选择或 CodeQL 模型，始终保留 production_capacity=UNKNOWN、full_rust_coverage=HOLD；真实 hosted 结果另由父目标验收。

#### 7.2.6 Rust default 生产库接入与首次验收

Trace refs: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89；复用候选库的固定组成与身份合同，生产执行及当前源码验收仍按本节独立核对。

本路径承接 CQ-R02/R03/R05/R06/R09，只用于 `codeql.yml` 中可信默认分支的 Rust default 维护扫描。生产输入是本次计划与实际 checkout 一致的提交，不沿用研究入口的冻结业务 SHA、manifest 数量或历史覆盖结论。现有批次变量仍控制是否执行；首次配置为 baseline 时按 §4.2.1 保持 pending acceptance，接线合入和候选研究通过均不代替运营验收。PR default、extended 和其他语言继续各自原有执行路径及预算。

正式 native 前置仅在 default 且非 `pull_request` 的维护调用中显式选择已验证的 `ubuntu-archive-https` profile，复用 §7.2.3 的私有源副本、完整四包与模块探针及前后身份合同。共享 helper 默认仍为 `runner-sources`，PR、extended、固定源码观察、容量研究及独立 Cargo check 不随之切换。选中的 profile 写入 native budget 与 terminal；准备仍受原 180 秒窗口、270 秒父截止与 90 秒收尾保留约束，update 的 60 秒上限、20 秒探针空间和 5 秒特权收尾差额不变。源准备、核验和清理均计入原截止，不失败回退重跑。

正式调用沿用普通 native stdout/stderr 与独立 `oasis7-codeql-native` artifact，不开启诊断 Worker 观察器或 archive 采样。`native-source-profile.json` 与四条实际命令收据分别验收，不能放入分析截止内的 production 查询收据集合。终态只投影异常类型、真实命令 exit/failure_kind 和源收据保存错误的固定字段，保留原失败并阻止后续提取、查询与上传；源身份或清理不完整不能记 PASS。native 成功只表示本次依赖就绪，原 37 条结果、适用 Linux 覆盖、官方上传、唯一平台关联及每日 baseline 验收继续独立完成。

固定 Rust-only CodeQL bundle 2.27.4，下载后核对 SHA-256 `1c2f867094b80e43c9a0e511090e57e0ea3bf107600b4b3763c52b9020b4ec4b`，通过官方 init 的 `tools` 输入使用该本地包。生产最终库版本独立固定为本节的 `qualifier-lines-negative-projection-v8`，直接基线为 §7.2.4 的 `qualifier-lines-constraint-target-qualified-v7`，其原 v6 基线保持。官方库保持只读，在任务私有目录依次复用 v3 qualifier/arity/lines、shared helper、两处 trait 注解、ConstraintMatching 和 target-qualified 的实际准备函数，再复用本节负向投影的严格准备函数。保留真实 v3/v5 中间快照、v6/v7 全树和各层收据，从已验证的 `candidate-target-typeinference` 独立复制出 `candidate-negative-typeinference`；所选最终完整库集合相对官方仍恰为三个 Rust 文件与一个 shared 文件变化，固定字节和 mode 继续核验。`str::lines` 使用通用 taint summary，不添加业务专用流边。版本或补丁不匹配时失败，不回退到旧库继续上传。

生产准备终态与恢复检查同时绑定 v8 最终身份和 v7 直接基线。进入实际准备或恢复前，分别核对共享 helper 的 `WORKLOAD_VARIANT` 与固定 v6 组件、target helper 的 `RESEARCH_VARIANT` 与固定 v7 组件；这些组件常量与生产最终版本分开，研究版本变化不能自动改变生产选择。原 v6/v7 准备、验证和 closure 接口保持原义，生产仅在已验证的 v7 闭包中显式选择 negative shared 根。`arity-patch-identity.json`、`lines-model-identity.json` 保持 v3，`shared-helper-identity.json`、`trait-visibility-identity.json` 保持 v5，`constraint-matching-identity.json` 单独绑定 v6 及两份 v5 收据，`target-qualified-identity.json` 继续绑定真实 v6 全树和收据摘要、固定 target 补丁及完整 v7 树。`negative-projection-identity.json` 再绑定真实 v7 全树、target 收据摘要、固定补丁及完整 v8 树。恢复及查询后从当次官方库重建全部验证链；不能只比较保存的最终快照或将旧收据改标签。七份收据摘要和全部私有库实际字节纳入执行前后检查，原官方/v6/v7 文件权限检查保留，并核对官方/v7/v8 的完整目录集合与目录、文件权限。旧 v2/v3/v5/v6/v7 production preparation、研究或观测 preparation、缺失或伪造收据、额外文件或 mode 变化、任一私有库被替换均拒绝；即使 production 与 negative observation 使用相同 v8 字节，也不能互相恢复。

正式 helper 只封装工具准备和完整默认查询执行，不调用强绑定研究源码的容量主入口，也不加入研究七项见证或研究专用 evaluator capture/summary 观察器。复制官方 rust-queries 0.1.46 的查询与支持源码，保持原始 37 项默认查询的 ID、字节和集合指纹；移除发布包的预编译 QLX、嵌入库及旧依赖绑定，以独立源码 pack 包装。逐查询解析实际 14 项依赖，确认选择 `candidate-rust-all` 与 `candidate-negative-typeinference`，拒绝 v7、v6 或官方 shared 根替代，并保留 `suite-helpers` 的 query 角色校验，使用独立编译缓存和 `--no-use-precompiled`。全部官方依赖、四棵私有库树及源码 query pack 在执行前后保持一致；仅添加 search path 或设置文件存在不足以证明实际依赖正确。开发阶段复用原 600 秒窗口的固定 CLI 37 项真实解析测试，不运行数据库、编译或求值；该测试只证明生产消费者接线，不能代签当前 main 的真实生产验收。

仅默认查询 batch 在现有进程 runtime 中开启低频 Linux 资源时间序列，原 0.2 秒聚合采样及其他调用方保持原行为。每 15 秒至多读取一次，最多 320 条、每条编码不超过 12 KiB、合计不超过 4 MiB，仍计入原证据总限。每次最多检查 1024 个进程目录项、8 个同 PGID 进程和合计 64 个线程；单个 proc 文件接纳上限为 4 KiB，额外读取 1 字节探测超限，读取检查点共用 `min(原 payload 截止, 本次起点+50ms)` 软截止，不能据此声明单次系统调用或 RSS 的硬限制。只保存固定字段的数值与状态，包含接收 wall/monotonic 区间、进程 CPU/I/O/minor-major fault、线程 CPU 和 system PSI 累计值及差分，不保存进程名、命令行、路径或 proc 原文。进程统计与线程集合在前后 PID/start_ticks/PGID 一致时接纳；线程差分还须匹配同一父身份及 TID/start_ticks。首次、缺测或计数倒退的差分为 null，读取失败、身份变化及上限须显式保留，不补零。I/O 包含内核计入的已等待子进程，不能跨进程相加当设备流量；PSI 明确为 system 范围，不能独占归因到本任务，CPU full 不作为系统压力值使用。采样读取/解析故障或额外空间不足只停止附加观察并保留原因，不改变查询首失败、取消、截止、清理或上传判定；所有证据文件的创建、写入或 flush 错误仍按原规则处理。该时间序列用于与迭代及缓存逐出日志对齐，不是独占谓词成本或覆盖验收。

init 后从实际 GitHub job 起点计算一次分析绝对截止：最多 75 分钟，且不晚于 90 分钟总截止前 600 秒。私有库准备、官方 Action 提取、37 项查询与结果校验共用该截止；提取前重新计算剩余整分钟作为 Action 外层步骤上限，CLI 子进程只消费原截止的剩余时间。复用 §7.2.4 的 Action main 监督核心，以独立 production mode 绑定当前源码和预算，在真实进程启动及清理时执行绝对截止；官方 post 仍由 runner 执行，原研究模式保留固定源码约束。Action 使用公开的 `skip-queries: true` 完成数据库；helper 核验监督结果、finalized DB、实际源码、资源及库身份后运行默认查询，并生成待上传 SARIF。失败、超时或身份不符阻止上传，不重新开始分析预算。

失败采证最多消费分析截止后的 90 秒，属于原 600 秒预留，仍为上传及 post 保留至少 510 秒；该阶段不启动查询或解码子进程。超时后不能完成 guard、身份或日志核对时保留失败和未知，不将采证成功改写为分析成功。

覆盖采证动态枚举当前受维护 manifest 与独立 workspace 锁文件，复用有界源码/锁观察和 Cargo guard，在实际提取的 metadata/解析调用中保留 locked 约束。提取日志和 `coverage_report` 保留原始 COMPLETE/PARTIAL、REVIEW/UNKNOWN；发现清单、成员集合对账或查询完成均不代签实际提取与宏/feature/target 语义。WASM 仍需其真实 CodeQL 上下文验证；未接线源码的归属须明确收口。沿用既定支持范围及两条精确覆盖例外，不扩大忽略路径。

原 37 项结果逐项执行 `bqrs info` 和 `bqrs decode --format=json --entities=id,string,url`，保留所有结果集和行，核对 info 与 decoded 的集合及行数。显式保留 BQRS 已提供的实体 ID、文本和 URL，供原 diagnostics 按实际位置与源码归属对照；实体文本可能重复，不能作为宏位置的唯一键。URL 取决于查询编译时保存的信息，缺失时保持未知，不由文本推算位置，也不把整文件 URL 当作宏坐标或 owner/展开关系。解码沿用原 30 秒单命令上限、分析绝对截止和证据总限，不在收尾阶段追加解码；该参数不改变查询集合、字节或覆盖验收条件。

执行状态由提取和正式 helper 联合决定，提取成功但查询失败不能记录为 execution success。通过原 `upload-sarif` 上传，保留 `oasis7/rust-repo/default` category、实际 SHA/ref 及唯一 `sarif_id` 关联；上传关联 evidence v1 结构保持。独立生产 evidence artifact 保存工具、查询库、源码、结果与覆盖收据及有界日志，不包含数据库或完整源码包。查询执行、平台上传、findings 和覆盖验收分别呈现。

首次验收核对实际执行/容量、唯一上传来源、支持范围与控制样本、Security findings 及运营设置。试运行允许收集尚缺的覆盖事实；持续启用声明须完成适用验收，不能把 REVIEW/UNKNOWN 改记为通过。失败仅回退本次 Rust 批次键，保留其他语言、已有告警、required policy 和历史结果。通过后按[日频目标](codeql-integration.design.md#rust-daily-baseline-goal)持续运行，不追加 PR observe 验收。现有 PR/extended 的 45 分钟路径未被本次生产结果验证；未来若另行启用这些范围，须分别处理执行路径、容量及适用行为。

##### 固定源码的同配置性能观察

沿用本节 Trace ID，`codeql-rust-capacity.yml` 的独立 `observe` job 为 #4989 提供封闭观测入口：仅可信 main 手动 `execute=true` 且选择本节显式观测 mode 才执行。原默认 mode 仍为 capacity，execute 仍默认 false；原 capacity job 排除两种观测 mode，保留四种原工作负载。观测拒绝 `compare_qualifier_patch=true`。业务源码固定为 `2bcc4edafdc615f395c9138a0b39ede366726d34`；工具来自本次真实 `GITHUB_SHA`，并要求与 `GITHUB_WORKFLOW_SHA` 及该 workflow 的 main 身份一致。先从工具 checkout 冻结既有完整 helper 闭包及 production helper 到 `RUNNER_TEMP`，逐文件绑定 Git blob 和 SHA-256；随后在同一 workspace 根切换到固定业务源码。工具不得进入采集根，不覆写 `GITHUB_SHA`，不接受任意 ref、bundle、命令或资源输入。

新增 `rust-default-fixed-source-observation` mode 只复用本节 production 的 prepare/run 查询核心，固定原 2.27.4/v7、默认 37 条和原 diagnostics，不加入研究七项见证。官方 init 显式请求 4 threads、14570 MiB；prepare、恢复和实际 Action 准入校验导出资源与固定值一致，记录 runner 资源，差异保留失败。CLI、Python supervisor、JavaScript preload 分别识别该 mode，同时绑定业务 SHA、工具 SHA、布局快照与原预算；生产仍要求本次 `GITHUB_SHA == source_sha`，候选研究仍要求其原固定源码，模式、预算或准备收据不能互换。

job 仍为 90 分钟并共用 scan-slot-1。init 后使用原 production 的 GitHub Date 与实际唯一 job started_at 分配一次至多 75 分钟分析，前置 checkout、工具冻结、native 与下载耗时全部计入 job age；保留原 600 秒余量及最多 90 秒失败采证，不在工具冻结、提取或查询前重开窗口。官方 post 保持 runner 所有。默认 batch 复用上述有界资源观察，其他调用保持原行为。

独立 artifact 只包含布局、native 和 production 核心证据；保留原 `queries.json`、`official-default-suite.json`、`info-00..36.log`、`decoded-00..36.log`、`default-results.json` 及 prepare/run 终态。固定源码终态显式标识 mode、tool_sha 和观测用途，查询成功仍需审查实际 diagnostics 与适用 Linux 覆盖；本入口不上传 SARIF/数据库、不调用 collector、不改变批次配置，不产生可用于平台验收的上传关联。官方上传和唯一平台 analysis 的验收仍由本节真实生产入口完成，既有 #38 研究验收保持原范围。

同一 observe job 另提供显式 `mode=rust-default-negative-projection-observation`，用于 [#4989 的负向实例化候选](https://github.com/nil-studio/oasis7/issues/4989#issuecomment-6942772467)。原 `rust-default-fixed-source-observation` 继续严格使用 v7/v6，negative observation 使用 `qualifier-lines-negative-projection-v8`/v7；两者都固定业务 2bcc，不继承 production 的当前事件源码。生产按本节独立固定 v8/v7，仍使用 `rust-default-production` 及当前事件 SHA，不能改为观测 mode 来选库。业务、可信工具身份、37 条查询与原 diagnostics、资源、采样、绝对截止和上传限制完全沿用上述观测合同；两种观测的 prepare、预算和 Action 收据必须按精确 mode 绑定，不能互相或与生产恢复。

负向投影准备先完成并保留原 v3/v5/v6/v7 全部树及收据，再从经过验证的 `candidate-target-typeinference` 独立复制 `candidate-negative-typeinference`。只应用固定 `codeql-rust-negative-projection.patch`，用固定 v7 输入 SHA、补丁 SHA 和候选结果 SHA 检查唯一 shared 文件差异；文件集合、所有其余字节及完整权限位相同，目录集合及权限还与真实官方 shared 树对照。`negative-projection-identity.json` 绑定真实 v7 全树、原 target 收据摘要和候选完整树。该补丁属于观测外置工具及开发解析工具的快照闭包。prepare、恢复和查询后均重建原 v7 验证链与候选身份，不能仅信任保存的新收据。生产和 negative observation 在原 14 项实际 resolver 闭包中选择候选 shared 根；v6、v7、官方或其他根替代该根均拒绝。原 fixed-source observation 仍选择 v7 target 根，不生成 negative 树或收据。

唯一 QL 改写是在两个负向消费者中使用独立 `getTypeAtForNonPseudoApp`：它从原 `potentialInstantiationOf` 与同一 `app/path` 的非伪类型存在性出发，再关联约束类型树，不包装原完整 `getTypeAt` 后过滤。共享 `getTypeAt`、正向 `getNthTypeAt`、rank/max、约束根、lookup-trait 和 PseudoType 规则均保持。私有 H=`hasTypeParameterAt` 的唯一消费者 K 原本为 `H ⋈ N`；新 H′=`H ⋉ N`，故展开后 `H′ ⋈ N = H ⋈ N`。具体类型冲突分支也已有相同 N。不能把 guard 的类型 witness 与约束类型强行相等，也不能收窄共享正向关系；正向一次出现的自由参数允许 app 子路径尚未知。

固定点证明先在所有消费者中按定义消去旧 `getTypeAt`/H 与新 helper/H′，包括不变的正向消费者。消元后，`Q(app,path)=exists N(app,path,_)` 与已经相连的 N 满足 `N ⇒ Q`；等式对其余递归关系的每种解释成立，因而保留谓词的递归方程和最小固定点投影相同。这里不假定求值某一轮的独立辅助关系已等于其展开定义；辅助关系的迭代传播延迟、编译计划和物化成本可能不同。Run39 已完成的类型推断固定点中，完整 H 为 2,284,212 行、K 为 208,550 行，只能推出 `H′=πH(K)` 至多 208,550 个逻辑 tuple，不能把该界外推到共享关系或整体耗时。

开发回归覆盖真实私有补丁及两棵树的身份/权限、实际 resolver 选根、模式混用和收据篡改拒绝；语义 oracle 包含正向未知自由参数子路径、负向不同具体类型 witness 和重复类型参数。固定 CLI 不可用时当次真实 QL 编译/执行明确未验证，不能用 mock 或代数证明代签。源码证明、固定源码受控运行和当前生产验收分别记录；观察的全部结果与性能仅适用于该次真实输入，不代签生产容量或上传关联。日志中间行数的消除界不是内存或墙钟收益承诺。

#### 7.2.7 合法 WASM 单角色 CodeQL 语义诊断

本入口继续承接 CQ-R03/R04/R05/R06/R09，在 §7.2.5 的同一冻结源码 `7c2f99a0ff2fbc3a3523dee7a1cc06fa5bf187ef` 上独立采集真实 CodeQL 上下文，不复用旧 metadata 的通过标签。新增默认关闭的 `CodeQL Rust WASM Context` workflow，仅可信默认分支精确 `execute=true` 的手动请求执行，权限、scan-slot-1 排队和不取消已有运行与 metadata 入口一致。该入口不改生产变量、SARIF 上传、数据库上传、既有研究或生产预算。

完整根 workspace、53 个不同根成员、全部源码与锁文件保持原样；固定 Rust 1.99.0、`wasm32-unknown-unknown` 和唯一 `oasis7_custody_worker/recovery-fence`，安装该工具链的 rust-src 与 WASM 标准库。重新执行受 guard 约束的根 metadata，对实际 resolve 图验证角色与六个本地包，拒绝其余角色及两种 harness。六包身份沿用 §7.2.5；完整根仍是实际项目，不称为独立 crate。工具 helper 在业务根之外，CLI 的 cwd、`--source-root` 和 index 起点均为完整业务根，`database index-files` 只选六包下 `.rs`，记录精确输入集合及摘要。

首次步骤按实际 hosted job start 绑定一次 1800 秒截止，准备最迟在原起点后 780 秒结束。仅剩余至少 1020 秒才准入最多 900 秒的共享提取与小查询窗口，其截止取准入后 900 秒与原总截止前 120 秒的较早值；子进程在该窗口结束前 60 秒停止，余下用于 guard、身份与封存。末尾 120 秒只供 artifact/post，不逐阶段重置、不自动重跑。工具获取、完整源码获取、固定 bundle 校验、工具链安装和依赖 fetch 均计入准备；只用 2 threads、3072 MiB 请求，不将采样资源数当硬限制。

时钟准入使用进程初始的同一 20 秒绝对窗口，HTTP、响应校验与等待均计时。只有唯一 job 的名称、整数 ID、run、attempt 和工具 HEAD 全部匹配后，才允许对 `queued` 或 `in_progress` 且 `started_at` 明确为 null 的状态间隔至多 1 秒重读，重读必须保持同一 job ID；缺 job、重复 job、身份不符、未知状态、缺失时间字段或 HTTP Date 立即拒绝。已有时间必须带时区且合法，开始时间晚于服务器时间或计算年龄达到 120 秒也立即拒绝。超时前未取得实际开始时间则不准入，不以本机当前时间补开始时间，不在重读后重新分配准备或总预算。

既有 `bootstrap-http.json` 保留每次请求的实际状态、字节数与 SHA-256；`bootstrap-terminal.json` 增加固定错误类别、读取/重读次数以及经类型、枚举和时间解析后的白名单时钟字段。未知自由文本、原始响应、token 和异常消息从首次写入起均不保存，原 512 KiB bootstrap 总限不变。证据写失败不得交接 helper。仅有旧 `Rejected` 和响应摘要而没有字段投影的证据，不能事后指定其拒绝子因。

固定官方 Rust-only CodeQL 2.27.4 bundle 及 §7.2.6 的 SHA-256，原包只读。本入口使用独立研究身份 `wasm-shared-helper-v1`，在业务根与官方 bundle 外复制 `codeql/typeinference` 0.0.42，仅应用 §7.2.4 已固定的两个 shared helper hunk 及其补丁、官方和结果 SHA-256；完整文件集合与 mode 必须相同，唯一差异为 `codeql/typeinference/internal/TypeInference.qll`。其余 12 项库保持官方原件，含 stock `rust-all` 0.2.25，不应用 qualifier、arity 或 lines-model 修改。九项见证的 QL、计数和 oracle 不变；已有官方库运行保留原身份和终态。

候选批次与本入口通过既有轻依赖 `codeql-rust-model-diagnostic.py` 共用固定 shared 补丁身份与准备逻辑，各自保留原执行监督、预算和收据包装。补丁纳入 bootstrap 与 executor 的精确 helper 冻结集合，复制、核验及至多 20 秒的受监督应用均计入原准备截止。保存官方与选择后的 13 库 closure、完整 shared 前后文件身份及实际 resolver 结果；实际 library path 必须精确选择本次私有 shared，拒绝官方 shared 回退、额外库或错误 dbscheme，独立编译缓存与查询身份继续核对。查询前复核 shared 身份；成功后重验实际 closure，失败与成功的 finally 均核对官方 bundle、query pack 和已准备 shared 的前后身份。缺少完整 shared 准备或身份变化时不能报告身份通过。

使用 `database init --build-mode=none`、`database index-files`、`database finalize` 后运行有界见证，不运行生产默认 37 项查询。实际 extractor 指定 `cargo_target=wasm32-unknown-unknown`、`cargo_features=oasis7_custody_worker/recovery-fence`，默认 features 不自动开启。官方 `build_script_command` 以 JSON argv 指向本轮绝对 guard cargo，执行根 manifest 的 `check --locked --offline -p oasis7_custody_worker --lib --target wasm32-unknown-unknown --no-default-features --features oasis7_custody_worker/recovery-fence --message-format=json`，target 在业务根之外。根 metadata 仍可读取完整 workspace；不得把默认 `check --workspace` 或只通过预检 metadata 的 guard 收据当作本次提取事实。

见证限定于 `platform.rs`、`recovery_fence_control.rs`、`recovery_fence_store.rs`。AST 绑定原始文件、声明名称和冻结行号；被 attribute expansion 替换而隐藏 Name 的原 Item 使用冻结起始 span，名称可见时仍必须匹配。分别观察 `event(fetch)`、`durable_object`、`wasm_bindgen` attribute expansion 与 `Serialize` derive expansion，要求存在真实生成 AST 后代；普通函数与无 derive 的结构体作为零展开负例。不猜宏生成的符号名称。另对 activation 内的 `call` 和 platform 内的 `control_enabled` 调用，分别计数语法节点、正确静态目标及明确不同的错误目标；正例必须实际出现，错误目标必须为零。另通过公开 `Crate.getACfgOption()` 观察 custody crate 的所有实际上下文，要求 WASM arch/family/os、recovery-fence 阳性和其余角色/harness 阴性；不能仅按请求参数判定 cfg 已生效。返回缺行、重复标签、非整数、只有零值或未知 schema 时不通过；文件被 index、AST 存在、metadata 图合法各自都不等于宏或调用模型通过。该有限语义见证不证明全数据流、所有维护中的 WASM 上下文或生产容量。

原 guard 的真实 Cargo 身份、locked 约束、版本首写 adapter 和审计保持；新验收按实际根 workspace 校验本轮新增提取调用及精确 build-script argv、cwd、工具链，而非要求容量入口的 47 roots 或旧 D1/D2 两条调用。源码、锁、helper、bundle/library 与工具链前后身份独立核对；命令失败、上下文不符、超时、遗留进程或证据不足均保留 INCOMPLETE。持久化 evidence 和控制台从首次写入即受原诊断隐私合同约束，允许 JSON 只有 schema 校验后的投影，原始进程输出只在有界内存中使用，不先写证据再清理。真实提取必需的数据库、其原生日志、TRAP 和源码归档是独立 RUNNER_TEMP 私有工作产物，位于业务采集根及 evidence 目录之外，不作为允许上传的日志或证据。证据合计 64 MiB 并复用分账、终态预留与哈希封存，不包含 DB、源码、原始编译日志或 SARIF。

同一次 `witness-query` 显式使用 `--verbosity=progress` 的 console progress，保留 `--loglevel=ERROR`，该阶段不设置会改走详细日志 stderr 的 `--log-to-stderr`；原生详细日志仍限于私有 DB。直接复用固定 2.27.4 的 `candidate_profile.Progress` 白名单解析器，该依赖一并纳入 bootstrap 冻结闭包。`witness-query.result.json` 的 `query_progress` 只记录本次冻结查询摘要、`COMPILE_START`、`COMPILE_DONE`、`EVALUATE_START`、`EVALUATE_DONE` 的计数及每类首末接收时间，另保留有界流分帧状态。时间是接收时刻，不是 CLI 内部阶段边界；未知行、非法编码与未换行尾部不成为进度事件，缺事件保持未知，不能靠时长或资源采样指定瓶颈。原 stdout/stderr 的实际字节数、SHA-256 和 EOF/前缀状态继续完整记录，原文不持久化。

进度投影覆盖正常读取、TERM/KILL 后有限尾部 drain 和退出收据路径，不增加独立编译、查询、预热或摘要进程，不延长原截止或扩大证据分账。投影错误单列 INCOMPLETE 并保留原命令首失败；原命令成功但进度采证失败时仍拒绝完成。事件计数与实际查询/BQRS/oracle 结果分别判断，不由进度标记提前报告语义通过。

最终分列 metadata、提取、宏见证、静态目标、guard、身份与证据结果；仅本次全部成立可报告 `WASM_CONTEXT_SEMANTICS_OBSERVED`。始终保留 `production_capacity=UNKNOWN`、`full_rust_coverage=HOLD`、平台完成待独立读回。开发阶段可对固定 bundle 解析并编译小查询以核对 API；未实际提取和执行之前必须保持 NOT_RUN，不能把本地编译或历史 release build 当成真实 WASM CodeQL 通过。

#### 7.2.8 候选与生产解析准备的 CLI server 生命周期

Trace refs: O7-REQ-351dbec8-3110-4ccd-aadc-49d0ba84bd89；本节只改变 §7.2.4 与 §7.2.6 的解析进程复用方式，保留原查询、库身份、资源、时限和隐私合同。

候选 `resolve_layout` 的 44 条与生产 `resolve_queries` 的 37 条 `resolve library-path` 使用同一个受限 adapter。各入口仍先独立验证实际默认 suite，保持原 30 秒上限；候选仍逐项解析原 37+7，生产仍逐项解析原 37。不得以 `resolve queries` 的文件列表、代表查询、历史响应或共享缓存推测其余实际 library path；查询准备、编译、求值、结果验证与上传的其他调用不迁入此 adapter。

父执行器冻结本次记录顺序、每条 query 的路径/相对名/pack/SHA、按种类选择的完整 closure、独立 cache 和原绝对截止，保存有界请求身份。adapter 只接受该固定结构并构造 `resolve library-path` 参数，不提供任意 CLI 命令转发接口。复用可信 helper 同目录 `codeql-rust-model-diagnostic.py` 的五字段校验器，保持 `libraryPath`、`dbscheme`、`compilationCache`、`relativeName`、`qlPackName` 的完整检查；候选默认 14 项、见证 13 项依赖及生产 14 项依赖分别核对，两个私有库不得回退官方根。新增 helper 纳入 capacity layout 的复制、Git blob、mode、字节数与 SHA 冻结集合；不能从随后切换的业务 checkout 导入同名模块。

一次既有 `runtime.run` 拥有一个 Python adapter 进程组，adapter 内只启动一次固定 2.27.4 `execute cli-server`，server 与其 launcher/Java 后代继承同一 PGID。进程不跨 `runtime.run` 保活，失败不重启、不切回逐条独立 CLI 补跑。命令是 UTF-8 JSON argv 数组加 NUL；同时最多一条未完成命令。每个实际响应收到 NUL 后，adapter 先核对完整 JSON、五字段、冻结 query 身份与指定 cache，再将该 index 的有界响应交给父执行器。NUL 是协议完成标记，不是 OS exit 0；收据分别保存命令完成标记、实际 server launcher 退出和 adapter 退出。

父 wrapper 在 spawn 前读取、保存并启用自身的 Linux child-subreaper 状态，outer `runtime.run` 显式使用 `reap_orphans=True`，只 wait/reap 本次 owned PGID；该选项本身不启用孤儿收养。adapter 也启用 child-subreaper，并在 TERM 后停止发命令、按原截止收齐 launcher/Java 和输出；若 adapter 在途中被终止，父 wrapper 负责收养并回收该组后代。正常、异常、取消与 spawn 失败都执行有界收尾，并最终恢复父 wrapper 原有 subreaper 状态。状态设置或清理不可信时停止执行；成功退出小样本不能代替在途 TERM/KILL 与孤儿回收的负例。

共享 runtime 的 owned-PGID waitpid 清理必须保留 `(pid, status)`；若回收当前 leader，使用 `os.waitstatus_to_exitcode(status)` 回填其唯一拥有的 `Popen.returncode`，group wait 及回填遵循 Popen 等待锁协议，防止后续 ECHILD 伪造 0。只修正真实退出收据，不改变首失败、deadline 或清理范围；以实际子进程和 WNOWAIT 同步固定 poll→group-wait 竞态，覆盖非零及 signal 状态。

父进程是 evidence 的唯一持久写入者。父观察器再次校验当前 index、query 身份和实际响应，完成该条原始 resolver 响应及 `library-resolution.json` 的保存后，才允许 ACK 当前 index；adapter 收到恰好匹配的 ACK 才能发送下一条。保留已有逐条实际结果与候选/生产 closure 状态含义，不伪造 44 或 37 份独立进程退出收据。运行终态另外绑定唯一 adapter/server 生命周期、已完成前缀和首个失败 index。写失败、超限或任何校验失败均不发送 ACK，保留已保存前缀及当前失败项，尚未发送的项保持未执行。

ACK 只使用一个本次 owned 非阻塞 pipe，承载有界 index 确认，不扩为通用 RPC。共享 runtime 的现有 `Popen` 调用不传入 `pass_fds`；父端保留 pipe 读端，以 `/proc/父pid/fd/readfd` 交给 adapter 显式打开并核对 pipe 身份，写端始终只归父进程。该传递方式沿用已有 proc-pipe 能力，不修改共享 runtime 的 stdin 或继承描述符规则。`observer.feed` 只处理和保存当前响应，不能发 ACK；现有 runtime 在 `feed` 返回后完成本轮输出写入，下一轮先检查取消与 payload 截止，再调用 `observer.poll`，ACK 在这个回调中发出。spawn/输出登记失败、有限尾部 drain 或 `summary()` 路径均不发 ACK。ACK 的 EOF、错序、重复、非法长度、写失败或超时均停止序列；父读端、写端及 adapter 打开的读端在成功、异常和取消时都须关闭。

每条解析仍至多 20 秒，第一条计入 server 启动，响应读取、完整校验、父写入和等待 ACK 都消费该条与原共享绝对截止。一次 adapter 的执行、shutdown、EOF 和 owned-group 清理均在原截止内完成；沿用 runtime 的有界清理预留，不能给 shutdown、ACK 或重试另开窗口。候选仍消费准备/提取/查询共享的分析时间，生产仍消费其原分析时间；不改变 75 分钟分析、90 分钟 job、原收尾预留及资源选择。发下一条前确认取消、剩余时间和输出容量，不能仅按剩余 query 数推定还有预算。

输入、命令和输出全部有界：记录数严格等于本入口冻结的 44 或 37；每条 argv 沿用 16 KiB 上限，请求文件及每条完整 JSON 响应沿用现有读取器的 16 MiB 上限。只保留当前原始响应、一个待确认 index、固定控制状态及受冻结条数和总空间限制的已验证结构化结果，不在内存累积历史原始 stdout/stderr；控制封装、分帧、序列化和双流尾部也要计入实际容量。父端所有请求、响应、会话 stderr、资源样本、控制收据和封口均通过现有 evidence 写入器，计入候选 64 MiB 或生产 256 MiB 总额及原终态预留，不另设可绕开总额的 adapter 目录。单条上限不保证剩余总空间足够，空间不足必须在继续命令前失败。

stderr 只归属整个 server 会话，沿各入口原隐私和证据策略保存；无明确协议边界时不得将异步 stderr 冒认成某条 query 的独立输出。命令 stdout 按实际 NUL 边界保存，未知或多余响应、未归属尾部、截断和非法编码不得当作下一条成功结果。所有已发命令及 ACK 完成后发送一次 `shutdown`，不等待不存在的 shutdown 成功 NUL；实际 launcher 退出、stdout/stderr EOF、adapter wait/reap 和 owned-group 清理共同决定本次完成。stdin EOF 或提前 exit 0 不能代替缺失命令/ACK。超时或取消在同一截止内执行 TERM/KILL 和有限 drain，保留首失败，清理不确定不得报告准备完成。

开发验证直接经过上述真实 44/37 消费入口及同一 adapter，使用固定 CLI、实际私有库补丁、独立 work/cache 和原查询身份；两个入口在受控解析 D0 中共用一次冻结的 600 秒总窗。唯一 candidate 测试入口在导入项目 helper 前固定起点，顺序执行研究 44 条和生产 37 条；前 540 秒用于准备与解析，末 60 秒用于最终核验、清理和封存，不为第二个入口重置时钟。可复跑命令、固定 CLI 与全新输出目录要求见[操作说明](codeql-operations.md#候选库完整工作负载研究)。44 条研究解析须选择 §7.2.4 的 v7 target shared 根，37 条生产解析须选择 §7.2.6 独立固定的 v8 negative 根；两个消费者分别复核原 v6/v7 全链，生产再复核 v7→v8 身份与权限。negative patch 必须纳入真实工具前后快照，生产完整 negative 树须与原私有树一同封存后才能清理工作目录。本地解析子进程固定 `CODEQL_THREADS=2`、`CODEQL_RAM=3072` 和 `JAVA_TOOL_OPTIONS=-Xmx3072m -XX:ActiveProcessorCount=2`；`resolve` 不接受 `--threads`/`--ram` CLI 参数，这些环境请求和 JVM 堆配置不构成进程 RSS 硬上限。保持预先固定的证据限额，不创建数据库、不编译、不求值。回归覆盖跨 chunk 的 NUL、错 closure/query/cache、父写失败不 ACK、ACK 错序/重复/EOF、命令与 ACK 超时、取消、spawn 失败、非零退出、提前 exit 0、输出饱和、TERM 尾部及 owned-group/FD 清理。真实小样本和解析 D0 只证明协议与消费者接线；原官方/target 双 fixture 的语义结果只覆盖相应固定字节，均不代签 44 项计算、七表 oracle、全仓性能、生产容量或启用验收。

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
