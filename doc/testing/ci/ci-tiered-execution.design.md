# CI 分级执行

Testnet 在同一个不可变 source plan 后并行启动 Web 构建与原生编译，各用独立 runner。原生编译仍只有一个互斥的可信/只读入口；安装包组装等待 Web 成功、且两个原生入口恰有一个成功另一个 skipped，失败、取消或未知状态不能进入组装。

原生中间归档只从同一 run 下载，组装前核对 source SHA、platform、target、profile、ops 范围及每个固定二进制的 SHA-256/权限，缺失、symlink 或篡改拒绝；组装不再调用 Cargo。

最终 SOURCEPLAN/BUILDINFO、入口验证和校验和合同不变。并行收益以实际 hosted 耗时为准，原生 runner 的 Cargo 并发固定为 1，避免多个重型 Thin LTO 构建同时占满内存；这不影响 Web/native 跨 runner 并行。

- 状态：active
- 专业 owner：`repository_health_engineer`；CI 控制、来源与缓存边界由独立评审者审读
- 通用规则：[开发流程规范](../../engineering/workflow/source-of-truth.md)
- 上游要求：[CI 分级执行 PRD](ci-tiered-execution.prd.md)
- 可执行测试命令和平台限制：[testing-manual.md](../../../testing-manual.md)

**实施边界**：[快速反馈与周期回归设计](#ci-fast-pr-periodic-full-design)已在当前候选接线。以下接口描述同一变更内的选择器、driver、workflow 和发布消费者；生效范围仍由可信 BASE、部署的 workflow 与实际保护决定。hosted 完整覆盖、十分钟/P95 和四小时容量分别待实测，不能由本地 fixture 或文档状态代签。

## 测试范围

普通 CI 是默认验证方式。成功规划后轻量 baseline 始终执行；选择器使用完整 Git name-status diff、base/head 两侧 Cargo 关系及必要目录映射选择 Rust、Web、场景、平台、打包和运维合同测试。

PR 测试合并提交时，选择范围合并两组差异：实际 target/source 的唯一共同祖先到 source HEAD，以及实际 target 第一父提交到 tested merge。main 在事件 base 之后已存在的改动不重复计入；PR 自身变化和合并结果额外变化仍覆盖。

校验 tested merge 恰有两个父提交、第二父提交等于 source HEAD、事件 base 是实际 target 的祖先；父提交身份不符、共同祖先不唯一或对象不可读时扩大为普通全量 required。

依赖及嵌入输入分析保留事件 base、共同祖先、source、实际 target 与 tested merge 的图。push/source-only 检查继续使用事件 base 到 source 的完整差异。

同一次选择可按 Git blob OID 复用原始内容和不依赖路径的 Rust 词法解析，避免各版本对相同源码反复扫描。每个版本仍独立建立 manifest、文件路径、模块挂载、嵌入与生成输入、host 和反向消费者关系；相同 blob 移动到新目录不能复用旧路径归属。缓存只在本次选择内存中存在，纯解析结果不得被调用方修改；未知输入、删除/重命名和图解析失败保持原处理。

Rust 变化覆盖自身及需要验证的反向消费者，删除和重命名覆盖两端；保留有效 features、WASM、生成输入和平台组合。workspace、features、锁文件、共享生成器及共享或尚未分类的 CI 控制输入变化采用保守普通 required，图解析失败仍扩大覆盖；未登记路径按下述门禁阻断。

共享控制规则保留 `.github/**` 的保守默认，可信 scope 配置的 `scoped_control_paths` 仅将下表已审读的精确路径映射到现有验证组。键必须是规范化的 `.github/` 相对文件路径，不接受通配符、目录穿越或反斜杠；组名必须有效且不重复。未登记路径或旧配置缺少该字段时仍按原共享控制规则选择全量，不为普通 Cargo、build 或其他目录提供覆盖入口。

| 独立输入 | 精确路径 | 普通验证组 |
| --- | --- | --- |
| GitHub Issue 表单与入口配置 | `.github/ISSUE_TEMPLATE/code-development.yml`、`.github/ISSUE_TEMPLATE/config.yml`、`.github/ISSUE_TEMPLATE/decision.yml`、`.github/ISSUE_TEMPLATE/defect.yml`、`.github/ISSUE_TEMPLATE/document.yml`、`.github/ISSUE_TEMPLATE/test.yml` | baseline、workflow_governance |
| GitHub PR 模板 | `.github/pull_request_template.md` | baseline、workflow_governance |
| GitHub PR 评审说明 | `.github/instructions/pr-review.instructions.md` | baseline、workflow_governance |
| 独立 CodeQL 扫描与诊断 workflow | `.github/workflows/codeql.yml`、`.github/workflows/codeql-rust-capacity.yml`、`.github/workflows/codeql-rust-cargo-check.yml`、`.github/workflows/codeql-rust-cfg-diagnostic.yml` | baseline、workflow_governance |

精确分类只影响共享控制的全量判定，不提前结束该路径的分析，也不撤销 Cargo、嵌入输入、测试消费者或其他规则选择的组；混合修改取并集，删除与重命名两端分别判定。未列入表中的 `.github` 文件仍保守全量，包括新增或改名后的 workflow、共享 Actions 及 `rust.yml`。`scripts/security/**` 继续使用既有 workflow_governance 规则，CodeQL 的独立扫描、运营开关与 required/advisory 边界不变。

Cargo manifest/lock、共享 build 脚本、缓存及其他 CI 控制保持原覆盖。白名单与验证归属均来自同一事件 BASE；候选新增分类不能缩小本轮基线覆盖。

可信 scope 配置的 `workspace_support_profile_paths` 可逐 profile 登记跨 package 的真实测试输入路径；命中时在原 Cargo/目录选择之上追加 `workspace_support` 与该 profile，不把整个 support 组扩大。该映射不登记新输入的 ownership，未知路径仍失败，profile 名称及路径必须通过配置验证。现有非 package support 规则、显式整组请求及共享控制/不确定输入继续保留全部 support profiles。

custody Worker 的真实原生 HTTP/SQLite 集成依赖指定 hosted_identity authority、会话与 fixture 文件，因此这些文件单独变化也执行 Worker 完整平台 profile；其他 backend 功能不因此增加该平台测试。

`./scripts/ci-tests.sh required` 是普通必要测试的本地入口；`full` 保留广覆盖回归。Rust workflow 的四小时完整回归按[当前分层接口](#ci-fast-pr-periodic-full-design)执行；真实产品测试、WASM 确定性、共识、持久化、安全、升级恢复和跨平台打包继续保留。

Command Center 的独立 Cargo workspace 由 `command_center` 组承接，工具目录内的代码、配置、资源和测试变化选择 baseline 和该组；共享 context、local signer、secure envelope crate 变化也选择该组。已核实仅为使用说明的 `tools/oasis7-command-center/README.md` 从该目录规则中排除，保留 baseline 文档治理；排除只作用于这条显式规则，真实嵌入或测试消费者仍可选择相应组，删除和重命名仍检查两端。

Rust workflow 调用可复用的 `command-center.yml`，将 Core、生成类型、UI、workspace host Clippy 和适用的 macOS App 打包结果纳入汇总。可信规则确认只影响已登记 Core/UI 输入时，`command_center_package=false` 仅跳过 App 打包，host 编译仍验证 Core API 消费者；原生打包、平台、共享控制或不确定输入保留打包。完整 profile 始终打包。专用 workflow 不独立重复触发 PR/push，checkout 使用计划固定的 tested SHA。

可复用 Command Center workflow 自身显式声明 `cache-mode: read`，与当前调用方的只读权限一致；后续调用方也不能借此入口写入共享缓存。

该平台组的本地验证使用专用 workflow 中的 canonical 命令，不由通用 shell driver 执行。

文档 corpus 对象、索引及 evidence inventory 数据更新由始终执行的 baseline 文档治理检查校验一致性；三平台 corpus/evidence 工具回归只在检查器实现、fixture、Git 路径配置、scope 配置或平台 workflow 变化时触发，数据更新不单独触发三平台回归。

<a id="ci-generated-inputs-design"></a>
### 生成文件与纯文档选择

嵌入输入扫描先区分 Rust 源码 token、注释及字符串字面量的边界，只把真实代码中的 `include_str!` / `include_bytes!` 调用计入输入分析。普通/byte/raw 字符串、字符字面量及注释中出现的宏文本不能被当作另一次调用；真实宏的字面量参数仍须读取和解析。该规则同样适用于生成测试源码的 fixture，不能通过忽略整个测试文件或 `cfg(test)` 区域规避真实测试消费者。无法可靠确定词法边界或真实动态表达式的输入时继续保守回退。

可信 scope 配置可登记已审读的 `OUT_DIR` 生成文件，逐项绑定 Cargo package、实际 build producer 的仓库路径、producer Git blob 内容的 SHA-256 和固定输出文件名。选择器在每个参与分析的 Git revision 上独立验证 package、manifest 的 build 入口及 producer 摘要，只将完整匹配的 `include_str!` / `include_bytes!` 生成输出计为已解析；不执行候选构建脚本，也不按 `OUT_DIR` 名称统一豁免。

登记适用于已确认输入全部受既有共享构建/CI 控制规则覆盖的独立生成脚本。`oasis7_chain_runtime` 的三份 native 构建元数据分别来自构建环境、编译器版本和根目录 `Cargo.lock`，其 producer、manifest、锁文件、工具链及 CI 配置仍按原规则选择保守全量。

producer 内容或 build 入口变化、输出未登记、路径逃逸及其他动态表达式继续保留未解析回退；更新生成器时须重新核对实际输入再更新登记，不能沿用失效摘要。真正由 Rust 嵌入的文档仍按 reader 及反向消费者选择测试；无关说明文档和 corpus 数据仅保留各自已登记检查。

回归覆盖纯说明文档、corpus 数据的逐路径及组合变化，并同时包含字符串/注释内的伪调用、真实字面量与动态调用、已登记生成输出、producer 在 base/source/tested revision 间漂移、真实文档消费者和共享控制输入。已登记独立 GitHub 路径分别验证单路径、混合修改、真实消费者、删除/重命名和旧基线行为，未知 workflow 与共享控制保留全量正例。还需用当前主仓依赖图重放文档变更，避免仅有微型仓库 fixture 或代码与 README 混合变更掩盖误选。

<a id="ci-unknown-inputs-design"></a>
### 未登记路径的失败链路

Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；承接[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs)。

选择器完成 Git/Cargo 与显式规则匹配后收集全部未登记路径，抛出独立的 `UnknownInputsError`，不被现有“输入不确定则全量”的捕获分支吞掉。CLI 返回非零并以转义 JSON 路径列表报告补登记提示，不写成功 plan 或 GitHub 组输出；共享控制输入视为已登记的全量规则，不能因没有 Cargo owner 误判为未知。混合变更中的已知全量输入及 `--add-group` 均不能覆盖未登记路径失败。

所有重任务沿既有 `select` 成功条件启动；轻量 formal 汇总和最终 `required-gate` 保留 `always`。最终 gate 在下载规划产物前先确认 `select` 成功，失败时立即给出范围规划诊断，因此不依赖失败路径上未上传的 artifact。trusted baseline 来源保持不变；新独立目录先通过规则/验证入口登记 PR，再提交使用该目录的功能 PR。新增测试使用真实 Git diff/CLI 验证未知新增、删除、重命名两端、混合全量输入、无成功输出及失败汇总；已登记路径、共享输入和图解析全量回退保留正例。

<a id="ci-generated-contract-jobs-design"></a>
## 可生成工作流的首个试点

Trace refs: O7-REQ-7d1dab3c-451e-4915-bf76-5bd28c117371；承接[可生成工作流定义](ci-tiered-execution.prd.md#ci-generated-contract-jobs)。设计已定义，生成器及接线尚未实现。产品适用性：N/A；reason=只集中现有 CI job 的结构定义；scope=下列两个 job 的源码生成、漂移检查及行为等价。

### 试点选择与事实归属

当前 [Rust workflow](../../../.github/workflows/rust.yml) 的 `operational-contracts`、`packaging-contracts` 是相邻的两个原生 job，checkout、Python 3.12 准备、文档范围环境和 driver 调用相同，仅 job ID、选择输出及执行组不同。它们均依赖 `select`，没有独立 matrix、缓存写入或平台准备步骤，适合作为最小试点。`workflow-governance` 的可信合同步骤、`compile-metrics` 的 Rust 准备、其余平台及产物 job 继续手工维护。

| 事实 | 唯一维护处 | 生成器的使用方式 |
| --- | --- | --- |
| 两个 job 的公共结构、固定 action revision、Python 版本及 job 顺序 | 拟新增 `scripts/ci-contract-jobs.py` | Python 3.12 标准库的不可变 job 描述和公共渲染函数；首轮只登记 `operational_contracts`、`packaging_contracts` |
| 组集合、输入影响、风险与 profile 选择 | 现有 `scripts/ci-required-scope.json` 和 `plan-rust-required-scope.py` | 只核对两个组已存在；job ID 由组名的下划线转连字符、选择输出由 `run_<group>` 推导，不复制选择策略 |
| 测试命令及其行为 | 现有 `scripts/ci-tests.sh` 与所属合同 | 保留 `required --group <group> --repo-root "$GITHUB_WORKSPACE"` 调用，不把测试清单搬入生成器 |
| GitHub 实际执行内容 | 提交的 `.github/workflows/rust.yml` | GitHub 直接执行 YAML；生成器只在开发与一致性验证时运行，不在 workflow 启动时动态改写 DAG |
| 可信选择、必要结果与发布资格 | 事件 BASE 的 planner、结果汇总及完整结果合同 | 沿用既有来源和定义摘要，不以“生成一致”签认选测或发布 |

选择 Python 是因为现有 planner、合同运行器及 CI 维护环境已使用它；首轮无需增加 Rust 构建前置或 YAML 序列化依赖。公共函数输出有限的固定 YAML 模板，只替换已验证的两个组标识，保留 GitHub `${{ ... }}` 表达式和 shell 原文。该结构不是通用工作流 DSL，不接受任意 step、权限或命令插件。

### 受管区、接口与失败处理

首次实施在两项现有 job 外加入唯一的 `  # BEGIN GENERATED contract-jobs-v1` / `  # END GENERATED contract-jobs-v1` 标记，并注明编辑入口。标记区位于 `jobs:` 内，正好包含两个完整 job；标记外的触发、并发、权限、select、其他 jobs 及最终 gates 仍为手写权威。首轮输出应与原两项 job 的正文逐字一致，允许的额外差异只有来源注释和边界标记。

拟新增接口如下，当前不能当作已可用命令：

```bash
python3 scripts/ci-contract-jobs.py --check
python3 scripts/ci-contract-jobs.py --write
```

两个模式必须显式二选一。入口从脚本位置定位仓库，仅接受固定的 `scripts/ci-required-scope.json` 输入和 `.github/workflows/rust.yml` 输出；不提供任意输出路径、网络输入或按环境扩展 job 的选项。结构描述、组集合、标记区及输出路径全部通过验证后才渲染。

- **确定性：** 输入只取当前工作树中的版本化定义和指定清单；固定 job/step/字段顺序、UTF-8、LF 和结尾换行，不嵌入时间、绝对路径、随机值或机器环境。受管区外字节原样保留，不对整份 YAML 重新序列化。
- **只读检查：** `--check` 在内存生成并比较，返回 `0` 表示一致、`1` 表示漂移、`2` 表示输入/结构/IO 错误。漂移输出限定受管区的 diff 和编辑入口；缺失、重复、逆序或嵌套标记、重复/缺失 job、未知组均返回 `2`。检查不创建临时文件、不触发 Git/GitHub 写入，也不自动运行 `--write`。
- **路径与写入：** 输出及父路径须在真实仓库内且不是 symlink，文件是已有普通文件。`--write` 先完成所有校验，在相同目录写完整临时文件并保留原 mode；替换前重读原文件，若与初读字节不同则报告并发修改并退出，否则原子替换。失败清理自己的临时文件，原文件保持；生成操作由当前工作区的单一写入者执行，不能与其他编辑者并行覆盖。
- **结构错误：** 不猜测缺失标记的位置，不把标记外的同名 job 当作可覆盖目标。修复源码或明确的边界标记后再执行；工具不得为了继续生成删除未知内容。

当前 `validate_candidate_graph` 尚未逐项验证这两个 job 的选择输出和 driver group 映射；不能把现有测试描述为已经覆盖。实施前须先由可信 BASE 合同独立登记并验证这两项映射，再让候选生成器和 workflow 接入；若可信 BASE 尚未包含该合同，沿用可信侧的保守检查及独立前置交付，候选不能声称已完成该语义验收或为自身放宽 bootstrap。

新增输入仍按现有 CI 控制归属执行，拟建 `scripts/ci-contract-jobs.test.py` 在已有 `workflow_governance` 入口运行，并由实施 PR 补齐其真实合同登记。`--check` 也放在该现有组，不增加 GitHub required-check 名称。首次迁移接受当时可信 BASE 的必要范围；新测试在合入并进入可信规则后才能按既有能力被消费，候选不能自行选择更窄的检查集。generator/check 失败向现有组返回非零，最终 gate 按原规则汇总。

### 行为等价与验证

| 必须保持的行为 | 首轮验证 |
| --- | --- |
| 两个 job ID、`needs: select`、成功 select 加原输出/full 条件、Ubuntu runner | 比较迁移前后 job 正文；沿用 `ci-workflow.test.py` 的 job/组对应和普通组非 matrix 断言 |
| 固定 tested SHA、`fetch-depth: 0`、`persist-credentials: false`、Python 3.12、文档 base/head 环境及真实 driver 命令 | 逐字段/表达式核对；执行所属已有合同，不只检查模板变量存在 |
| 顶层 triggers、concurrency、permissions、其他 job、required/explicit-full gate 的 needs 与结果解释 | 受管区外字节比较，并保留现有 DAG、取消、缺失、意外 skipped、空执行及完整库存反例 |
| policy/source/tested 分离、可信 BASE 与候选 checkout 的职责 | 复用现有真实 Git fixture、BASE 合同和结果摘要验证；生成器无权改写计划或伪造结果 |
| 生成一致与语义正确分离 | 实施时先扩展可信 BASE 侧语义合同，分别约束两个 job 的选择输出与 driver group 的精确对应；注入模板和产物同步改错选择输出、driver group 的负例，以及去掉 select 成功条件或改变失败传播的负例。`--check` 可一致，但可信合同必须拒绝；候选不得自行豁免或缩小 bootstrap 检查 |

生成器自身还需覆盖两次生成字节一致、单独改 YAML 后检查只读失败、模板修改而未更新输出、标记损坏、重复 job、未知组、symlink/越界路径、只读文件及写入前并发变化；每个失败检查文件字节未变。测试 fixture 只证明本地生成与行为断言，迁移候选仍需实际 GitHub 必要检查，不能据此宣称 hosted 耗时或覆盖已达标。

### 交付、收益与回退

首个实施 PR 只包含两 job 的公共定义、受管区、生成检查及必要合同登记/回归；planner、业务测试、缓存策略和观测管线保持原职责。评审比较一次公共 checkout/Python/环境变更所需的权威编辑位置、自动产生的 YAML diff、诊断是否指向正确来源，以及工作流语义是否等价。减少手工重复是首轮目标，不能用删除合同测试或放宽检查换取文件变少。

如试点增加维护成本，按一个完整回退提交保留当前正确的两项展开 YAML，删除其生成标记、生成器接线及仅属于该生成器的测试；确认原 job、真实合同、required 名称、DAG 和权限仍在，重新执行适用验证。不能先移除生成器留下必然失败的检查，也不能回退已有选测修复。扩展到其他 job 前分别证明其结构相同且收益明确，不因首轮成功自动生成全仓 CI。

## 调度和结果

select 先产生明确的非空逻辑组列表，按资源组并行执行。普通组使用原生 job，fleet-health 使用固定 Linux/Windows/macOS 矩阵。最终唯一 `required-gate` 使用可靠的 always 汇总，并显式依赖 select 和所有组。选中组必须 success；未选中组可以 skipped 或 success；失败、取消、缺失、未知及意外 skipped 都必须阻断。fleet 矩阵不吞退出码，采用 fail-fast=false。

记录 source HEAD、base 和实际测试对象及运行链接。普通 PR 权限只读，从同一事件 BASE 提取可信选择器、scope 配置与结果汇总器，候选 checkout 的 driver 执行真实测试；候选控制变化不能缩小基线覆盖。CI 控制、安全或兼容边界变化接受对应能力的独立评审。

## 专项条件

缺测试先补普通 CI。只有普通 runner 难以提供的真实长期状态、专用硬件或受控环境才使用专项验证，说明具体不足、环境、版本、通过标准和实际结果。阻塞专项纳入同一 PR 最终汇总；调试 dispatch 不能替代 PR 检查。

CI 结果对应实际 source HEAD。main 前进或分支显示 `BEHIND` 本身不使该结果失效；只有合并冲突或 main 的相关变化带来明确集成风险时，才按需更新分支并验证实际组合。source HEAD 变化后重新运行必要普通 CI，复审实际变化与受影响部分；具体合入规则遵循开发流程规范。产品层的签名、checkpoint 和状态一致性证明仍按专业合同验证。

CodeQL 技术扫描见 [CodeQL 设计](codeql-integration.design.md)，实际 required/advisory 按 GitHub CI 与生效保护执行。

<a id="ci-fast-pr-periodic-full-design"></a>
## PR 快速反馈与周期完整回归设计

Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；承接[快速反馈与四小时完整回归要求](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full)。本节描述当前候选实现及仍待 hosted 验收的目标。

### 执行入口与计划

保留 `ci-tests.sh required/full` 和最终 `required-gate`、`explicit-full-gate` 的职责；`plan-rust-required-scope.py --profile` 支持下列四种执行计划。不传 profile 时保留旧计划接口。`scope=full` 表示普通 required 内的保守扩张，完整 profile 使用 `scope=explicit-full` 并展开非空完整 groups。

| profile | 触发与对象 | 计划及汇总 |
| --- | --- | --- |
| `pr-fast` | PR 当前 source HEAD 及实际 tested merge | 快速基础集合、受影响消费者和命中风险的定向加测；由 `required-gate` 汇总 |
| `main-fast` | main push 的固定提交 | 快速组合验证及对应必要检查；不默认重复完整重回归 |
| `periodic-full` | 四小时 schedule 的固定 main SHA | 全仓适用快速、重型与平台集合；由 `explicit-full-gate` 汇总 |
| `candidate-full` | 手动明确选择的候选 SHA | 对该版本执行完整适用集合；普通候选只读，发布消费者独立核对来源 |

计划沿用同源 scope 配置、Cargo 关系和 driver；当前字段如下。target/features 的真实命令由执行器生成，测试清单与环境身份再写入构建 manifest，不把计划中的组名当作真实执行结果。

| 字段 | 当前含义与消费者 |
| --- | --- |
| `execution_profile`、`fast_plan_version=1`、`check_inventory_version=1` | 区分快速/完整策略及接口、库存版本；旧 BASE 不含 capability 时不得套用候选的窄范围 |
| `source_sha`、`tested_sha`、`policy_sha`、`architecture` | 固定源码、实际 tested commit 和可信策略；PR 保留 source 与 merge 的区别，candidate-full 另记实际 tree，artifact 绑定 commit、工具链与构建输入 |
| `groups`、`oasis7_required_packages`、`oasis7_required_wasm_packages`、`workspace_support_profiles` | 组及实际 native/WASM 包范围；support 沿用版本化 profile，控制/不确定输入保留旧完整库存 |
| `ordinary_artifact_partition_version=1/2`、`ordinary_partitions` | v1 保留 full 完整目标分区；v2 对有 chain-runtime 的 targeted 非 inline 普通库存执行 controls 与四个 native case 分区；保留唯一 producer 的完整 feature 组合，区别于 inline 分包构建 |
| `ordinary_artifact_delivery_version=1` | 仅为最终 artifact v1 计划启用共享辅助产物与四个完整 harness 包；完整父清单、构建与执行选择不变，旧计划继续完整包 |
| `workspace_support_partition_version=1`、`workspace_support_partitions` | 选中 custody worker 时单列 custody，其余 profile 归 other；两路并集保留全部选中 profile，旧 BASE 使用 all |
| `custody_execution_version=1`、`custody_node_shards` | 可信能力且无测试政策变化时允许 custody 三路构建与四个完整 Node 文件分片；缺能力或测试政策变化继续原完整 verifier，稳定 support 汇总核验全部实际结果 |
| `fast_test_inventory_version=1`、`ordinary_fast_targets` | 可信路径合同允许时，固定普通快速检查的 Cargo target、features 和最低 case 身份；构建、Clippy、打包与执行使用同一清单 |
| `fast_execution_version=1`、`ordinary_execution=inline`、`ordinary_clippy_group`、`rust_baseline_checks` | 可信组件快速计划在单 runner 构建并执行测试，由明确的并行组执行相同 target 的 Clippy；显式选择 accounting/独立工具专项，缺少新能力时保留旧执行与 baseline 全集 |
| `architecture.build_policy_version=1`、`build_policy=ordinary-targets` | 全部直接架构输入由快速目标承接时，保留全部声明/依赖图，额外 profile 编译明确延期到 full；缺少能力或混合未覆盖输入保持原编译选择 |
| `full_fast_test_inventory_version=1`、`ordinary_full_fast_targets` | 完整 profile 在原全集上追加全部快速 target 的精确 feature 组合，不替代原 required 库存 |
| `python_control_contract_version=1`、`python_control_paths`、`python_control_tests` | 可信精确路径命中的 CI Python 控制输入及按组归并的非空最低合同，由可信基线执行入口针对候选实现验证 |
| `formal_suites`、`formal_shards`、`command_center_package` | 完整逻辑 case、实际物理分片及 App 打包选择；host Clippy 不随打包开关省略 |
| `formal_full_profiles_only_version=1` | 可信 BASE 启用重型 Formal 仅在完整 profile 执行；快速 profile 的组、逻辑 suite 和物理 shard 均不选择 Formal |
| `formal_deferred_ordinary_cases` | 同一可信计划中按 harness 与完整测试名登记、在普通批次中后移的重型 Formal case；完整 profile 为空，其他普通测试不受影响 |
| `test_policy_changed`、`test_policy_paths`、`fast_plan_exceptions`、`reasons` | 测试选择属性变化、必要扩测与运行前原因；不按实测超时事后改变普通/例外分类 |

快速规划接受 `--base-ref`、`--head-ref`、`--test-ref` 和 `--policy-ref`；完整规划只需固定 source/tested/policy，从可信完整库存展开，不依赖 diff。`--formal-config` 读取同一可信策略的 Formal 清单。ordinary producer 与 `ci-tests.sh required --group oasis7_required --plan <selection.json>` 消费同一文件；support 使用 `--support-plan`，Formal 的 build/pack/run 同时使用 `--plan` 与可信 `--formal-config`。适用环境相同才允许复用，构建成功与测试成功分别记录。

PR/push 继续从事件可信 BASE 取得选择策略、配置及汇总规则；schedule/manual 没有 PR base 时，固定已核准的 main/workflow 策略版本，再解析待测 source，不能以候选自带的策略缩小要求。删除和重命名两端、真实嵌入/生成输入和必要反向消费者取并集。未登记路径明确失败；Git/依赖解析不确定采用保守必要集合并记录超过快速预算的原因。共享控制规则保留，后续更细分类只能按现有独立评审流程进入可信基线。

workflow 先探测可信 planner 的 `--profile` capability；旧 BASE 的 PR/push 调用旧接口，driver 缺少新版本字段时保留原 native、WASM、Formal 和 support 范围。可信 main 尚未具备完整 profile 时，非 main 的手动 full 可用旧完整组库存做首次接线验证，计划标记 `bootstrap_full=true`。该只读运行不能缩减旧覆盖，也不能被发布消费者接受；main 的周期/完整入口不以此 bootstrap 替代可信策略升级。

风险映射由真实路径、消费者和行为合同决定，不依赖作者贴标签或模型猜测。没有已验证的窄用例映射时保留相应普通完整组，不能将缺映射解释为无风险。重型 Formal 由可信 `formal_full_profiles_only_version=1` 单独安排到完整 profile：`pr-fast/main-fast` 无论 targeted、保守 `scope=full`、风险映射或显式追加组，最终均移除 `formal_process`，输出空 `formal_suites/formal_shards` 并记录原因。`periodic-full/candidate-full` 保留全部组和五个逻辑 suite；`scope=full` 只表示普通检查扩测，不能据此恢复重型 Formal。旧 BASE 缺能力时沿用原风险选择，未知能力版本失败。普通 CAS/Formal Protocol 库、单元及控制合同测试继续执行。

Formal producer、合并及物理矩阵继续消费同一可信选择输出；快速层不产生 Formal 产物，固定轻量 `formal-process` 汇总核对 producer/矩阵均 skipped 后报告 `not_selected`。完整层仍要求恰好一个 producer 成功及矩阵成功，`explicit-full-gate` 核齐全部配置组。执行器仅在新能力与快速 profile 同时成立时接受空 Formal；Python 控制迁移也仅在可信配置已启用该能力时，从普通 full 的精确应选组集合移除 `formal_process`，其他组缺失及完整 profile 缺 Formal 仍失败。候选新增能力不能给旧 BASE 的迁移计划授权。

两个已登记的 BFT membership 长场景没有 ignored 或 Formal 环境门控，会在 ordinary `runtime-tests` 批次重复运行。可信 scope 配置以完整 case 名将其写入快速计划的 `formal_deferred_ordinary_cases`；执行器核对 Chain Runtime library 目标身份，以 libtest `--exact --skip` 精确后移并从实际应执行数中扣除，记录延期名称。近邻名称、其他 Formal 命名的单元/合同测试和失败计数保留。两 case 仍由完整 Formal 环境批次及物理分片各覆盖一次；完整 profile 清空延期表，旧 BASE 无能力时不排除任何普通 case。Formal 延期在普通目标、架构编译、Clippy 与 support 转交范围推导完成后应用，不借此进一步缩减其他检查。

### Python 与 CI 控制输入的职责选择

共享 `ci-tests.sh` 仅有一类可证明的范围例外：可信 BASE 支持纯追加诊断测试注册时，唯一 `run_workflow_governance_operational_contract_tests` 函数外字节不变，原命令、顺序、参数与次数完整保留，只增加已登记到 workflow_governance 的 literal Python 测试调用。新增文件必须是实际 Git regular blob；source 与实际 merge 比较均须成立。只有文档及同职责诊断混改可收窄到 baseline/workflow_governance，Rust、其他职责、共享 workflow、策略或解析不确定继续 full。preview 复用同一证明，旧 BASE 保持原范围。

普通 Python 路径沿用显式 `rules` 与 Cargo 输入图，按文档、治理、打包、架构或协议等真实职责选择已有组；直接调用、嵌入数据及反向消费者继续参与选择。无真实合同的工具不能为了缩短 CI 被任意登记为轻量治理组，未知路径仍由既有门禁阻断。

进程监督 helper `ci_process_runner.py` 的直接消费者包括 native/Formal 执行、Viewer 并行分支和 storage acceptance。其精确路径登记到可信 Python 合同，最低执行 `ci-formal-process.test.py`、`ci-support-profiles.test.py` 与 `ci-storage-acceptance.test.py`，使 helper 变更也受到固定 BASE 行为断言保护；只有治理组映射不能替代该验证。单独修改 helper 保持原有 baseline 与 workflow-governance 职责范围，混合共享控制变更仍执行该 profile 的完整必要组。

对原本命中共享控制的 Python 脚本，可信 `scoped_control_paths` 允许登记规范化的精确 `scripts/` 路径，不接受扩展名通配豁免。对应 `python_control_contracts` 必须指定已选中的执行组与非空精确测试清单；未登记或合同不完整时不能取得窄范围。登记的最低测试文件自身变化也通过精确反向关联触发同一合同，不能仅因它原属普通文档检查而绕过可信验证。命中后的计划保留实际源码或测试路径、按组去重的测试清单，并选择 `workflow_governance` 执行独立可信合同。真实 Rust 消费者与混合变更要求的组照常取并集；scope 配置、共享执行器、可信合同入口及 workflow 自身变化继续按共享控制处理。

workflow 从与选择器相同的 BASE 取得可信合同执行入口，在候选 driver 之外执行；测试源码也固定来自该 BASE，受测实现与路径绑定到候选 checkout。执行入口核对非空发现、真实执行、完成状态及失败传播，拒绝跳过与成功退出但未完成合同的情况。候选自身的完整回归仍由所属组运行，不能替代可信最低断言；最低用例身份的删除、空发现或意外跳过须在本轮被发现，不能随候选成为下一轮基线后才暴露。断言语义的保留仍由独立评审核对。可信入口及该固定 revision 配置登记的全部最低测试源码纳入完整 CI 结果的检查定义摘要，固定安全测试也必须属于这份登记；旧结果不能签认已改变的执行合同。

治理 Python 回归由同一个可信协调入口执行。配置中的 `workflow_governance_python_tests` 声明原 baseline 治理函数的完整 Python 清单；具有平台专用 CLI 选择语义的资源清理测试、运维诊断、shell、Node 和 telemetry 合同保留原入口。协调器合并每个 BASE 测试文件的固定子集与本轮最低完整合同；完整合同覆盖其子集，只执行一次。候选清单不得删除 BASE 已登记文件，可以追加；候选治理清单、最低合同与固定安全断言按文件合并后执行一次完整候选 suite。BASE 源码、候选源码和真实 Rust fixture 仍是不同验证责任，不互相代签。

所有受托管协调器执行的 suite 继续由 BASE worker 逐项见证，保持原非空、身份保留、零意外 skip、真实结束及失败传播检查。协调器成功完成后直接执行同一候选 driver 的剩余治理函数，不写可复用成功回执、不接受候选提供的跳过清单，也不跨 run 缓存测试结果。workflow 仅探测下载的 BASE 入口是否支持该完整模式；旧 BASE 仍执行原可信合同加候选完整组，新候选不能自行声明已获得去重复权限。本地完整治理入口沿用各测试文件的原 Python CLI 和平台 skip 语义，不能代签托管的严格完成见证。执行日志记录每文件来源、用例数和单调耗时，实际前后结果归 PR。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

迁移期可通过独立只读 `pr-governance-preview` 比较完整治理入口。两边固定同一实际 tested merge、候选 self-policy 与 planner 生成的已登记 workflow-control 场景，保留完整治理清单；事件 BASE 只提供旧 runner 和 driver 的实现。旧方式重放最低合同、完整 driver 及 workflow 原有的两个 archive 与额外 Node 合同，新方式执行完整 `run --governance`。诊断不提供 BASE 授权，不被 required/full gate 消费；仅双方全部成功后记录耗时差，任一失败则保留不完整证据并失败。同 runner 顺序执行会受到后执行者的缓存预热影响，结果不是独立冷启动或 P95；BASE 已含新协调入口时明确记为不适用，不继续生成旧方式对比。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

隔离执行 shell dispatcher 时，夹具加载受测 driver 的完整函数定义，再覆盖 `run`、Cargo 等明确的记录边界；不手工维护一份易随 helper 演进而遗漏的函数名单。driver 的 source 接口只提供定义，不解析 CLI、不选择解释器、不切换目录或执行检查，也不改变调用方的参数与 shell 选项；正常执行仍经原 CLI 初始化和分派。可信入口仅对明确登记的 support dispatcher 夹具提供这份候选定义闭包，使旧 BASE 测试仍能执行原断言；注入只影响该夹具启动的 shell，不传播到其真实子命令。没有该接口的历史 driver 保留既有夹具路径。缺失 helper、命令失败、空执行及提前成功退出仍判失败，不能将这类兼容问题处理为跳过 BASE 合同；新增 helper、跨版本组合和失败传播须分别回归。

可信合同本身的演进使用明确的完整验证迁移路径：可信入口核对共同祖先到 source 的分支差异与 BASE 到 tested 的实际合并差异，只有精确策略配置或可信入口变更且计划保留该 profile 的完整必要组时，才允许迁移旧的全量策略断言。可信 Formal 新能力仅允许快速 profile 从全组中排除 `formal_process`，其余缺项仍失败。main 单独前进不构成候选迁移。迁移继续执行固定 BASE 安全断言、真实 Rust fixture 及候选完整合同回归，并接受独立评审；固定最低安全身份不能借迁移删除。日志列明迁移原因与实际范围，不宣称完成已迁出的全部旧合同。普通 Cargo/共享输入与 Python 的混合变化不因 `scope=full` 自动取得迁移资格，也不接受候选自报的迁移开关。

planner 的可信最低合同固定验证必要的选择、未知输入和失败边界，另以离线微型 Cargo 项目实际编译并执行 cfg、features 和测试清单，覆盖词法归属及门控变化。该合同不冻结每个历史路径的全量策略断言，允许经过评审的职责细分进入新 BASE；候选完整 planner 回归仍验证新策略。它不构建产品 workspace，也不替代真正受影响的产品 Rust 验证。

完整 profile 保留原全集并执行完整控制合同库存；合同自身迁移仅适用上述明确条件。普通 required 的保守 `scope=full` 执行本次实际命中的控制合同，不混同于完整 profile 的全库存。旧 BASE 未提供此能力时，候选保持旧共享控制范围，不能以自身新规则收窄当前 PR；本轮改变可信配置、执行器和 workflow 的迁移同样接受旧必要检查。纯 Python 具名路径、真实嵌入消费者、混合 Rust 变化、删除/重命名、未知路径、空执行、取消与失败传播分别回归；十分钟效果仍由合入后的真实 hosted 样本验收。

<a id="ci-pr-fast-selection-design"></a>
### 准入规则到实际选择

承接 [快速检查准入规则](ci-tiered-execution.prd.md#ci-pr-fast-selection)。沿用实际 driver 的同源检查定义，关联每项检查的行为合同、适用输入/消费者、默认职责、风险触发、真实命令、构建与环境前置；时延和冷暖缓存样本保留在实际运行记录。无需另一份审批或性能登记账本，也不能把缺少历史样本设为不执行新增测试的条件。

选择按以下顺序进行：

1. **先确定必要覆盖。** 从可信基线规则和完整差异产生基线、受影响快速候选及风险定向集合。修改测试、fixture、生成输入或依赖与修改业务源码一样按真实消费者处理；新增/重命名 case 与实际可执行清单核对，旧枚举、遗漏标签不能让它消失。除明确安排到完整层的重型 Formal 外，关键风险所需的 full case 先加入普通必要集合，再处理其 feature/target 前置。
2. **再形成可运行计划。** 把必要检查映射到最小完整 package/target/features 和平台闭包。对缺少可信窄映射的范围保留相关旧必要组；不确定图及共享控制仍按既有规则保守扩大。只有符合可重复、隔离、有界要求且整体成本适合快速目标的检查才标为已验证快速；其他必要检查继续执行，普通范围的超预算如实暴露。
3. **最后核对实际结果。** 计划记录每项选择原因，执行器核对预期 case/命令/平台与真实结果，汇总拒绝缺项。估算关注共享准备与并行依赖形成的关键路径，不以各 case 耗时小于十分钟或相加各阶段 P95 证明达标；整条 workflow 的实际成功反馈 P95 单独验收。

对照可信 BASE 与候选的标签、cfg 和实际清单。planner 按完整词法 module/函数归属比较选择属性，required 改 full 时标记 `test_policy_changed`，在有实际扩展 driver 且 manifest 声明 feature 的 package 上显式启用 required、full、wasmtime/viewer 等适用组合。条件表达式按实际执行组合求值；例如 `any(test, feature = "test-support")` 在测试 harness 中为真，不能只因另一个分支的 feature 未启用就拒绝。引用的 feature 仍须由真实 owner 声明，声明不等于已经启用；普通组合无法证明时才检查实际扩展 driver。无法证明的嵌套选择门控、未映射的新 ignore 和跨 binary 同名短函数均不能让用例消失。

新增且可证明可达的 item 不等于已有测试门控迁移。普通新增测试及其辅助模块，必须能从实际 Cargo test target 沿静态 `mod`、`#[path]` 或已解析的 `include!` 链确认执行归属，并按该 target 的普通 native Linux Cargo 组合求值，包括实际启用的默认 features。所有引用路由均须闭合到同一真实 package，条件均可判定，且至少一条完整路由及 item 条件成立；同一文件的互斥 production/test 挂载可分别为假/真，不能把两路条件相与，也不能用一条好路遮盖未知或未登记 feature。显式 `--no-default-features` 的 target 仍按其真实组合判断。不能仅凭文件名、函数名或某一层 `cfg(test)` 放行。满足条件时沿用受影响 package、必要反向消费者和风险规则，不追加无关 full features。Windows-only 条件、未知宏、无法解析的外部模块链、禁用 harness/测试 target 或未满足的 required-features 均不能取得普通新增豁免。

生产平台 cfg 的新增 item 只有在真实 ordinary driver 上可达、没有覆盖或替换既有门控时，才保留路径与依赖图确定的必要包及平台范围。既有 item 门控变化、测试删除/改名与新增混合、无法证明的条件或共享控制变化继续保守扩大覆盖；这些检查不把生产 cfg 当作测试退役，也不因新增项可达而减去同一变更中其他输入要求的验证。

`pixel_world_bridge` 的实际 ordinary 根为默认 features 的 library harness；同次选中的 pixel 组还执行 `wasm32-unknown-unknown` 生产检查。新增直接 free function 内的精确 wasm32 / 非 wasm32 语句门控，可按每个分支及全部祖先的同一真实编译组合证明。所有引用路径均须已知并闭合到该库根；生产检查不能证明 WASM-only 测试已执行，既有函数、新增未知谓词、宏生成归属或共享控制变化仍走保守路径。此能力由可信 BASE planner 登记，候选不能自行授权。

自动检查不冻结普通测试名称，也不证明任意 Rust 修改语义等价。普通 case 的删除/改名由既有独立评审核对原行为去留、替代检查或可信规则确认的不适用；未核清时保留相关旧必要组。Formal 的预期 case 另由可信最低库存强制核对，不能仅凭候选清单删除就视为无须执行。

首次切换时逐项比对旧必要集合、新 PR 集合及周期集合。差集中的每个检查均需说明验证对象、所属层和实际完整入口；相关普通行为或缺陷修复回归在本次 PR 执行，重型 Formal 按明确分层由定期或显式候选 full 承接。迁移核对完整库存、真实执行与失败传播，并接受 CI 控制独立评审；旧 BASE 仍保留其原有要求。

### 现有测试的首批归类依据

下表用于解释当前选择及后续细化依据，实际入口见 [CI driver](../../../scripts/ci-tests.sh)、[Formal 清单](../../../scripts/ci-formal-process-suites.json)及 [Command Center workflow](../../../.github/workflows/command-center.yml)。ordinary 对已登记输入按独立 target 合同收窄，其余按 package；support 按 profile，重型 Formal 在完整 profile 执行全部逻辑 suite；表中快速候选尚未取得逐项 hosted 预算验收，现有 `required`、组名或 feature 不能直接改称已达标 fast。

| 现有入口或用例 | PR 快速候选与触发范围 | 合入前必要加测 | 周期完整覆盖 |
| --- | --- | --- | --- |
| `run_required_gate_checks()`、文档/路径/可执行位治理 | 基线入口每次执行，正文、引用、索引和配置按真实输入选测；规划及汇总由既有独立入口保证 | 检查器或规则变化执行其合同与失败路径回归 | 全库一致性及历史报告，保持原阻断/报告级别 |
| `cargo test -p oasis7_consensus --lib`；`proposal_conflict_is_rejected`、`publish_happens_only_after_commit` 等 | 共识及必要消费者变化时保留正常提交、冲突拒绝、发布顺序和快照不变量；按实际 target 成本决定拆分 | 修改真实提交/最终性或身份接续时追加对应进程场景 | 适用共识组合及完整真实进程验收 |
| `run_oasis7_distfs_tests()`、CAS/Formal 协议库测试 | 存储/编码/协议变化时保留哈希、往返、拒绝与状态不变量；不预设整个 lib 都快 | 持久化时序、原子发布、锁、crash/restart、重试变化追加真实恢复 | 完整恢复矩阵、长记录与多副本场景 |
| signed-read codegen/Python/JS 合同；custody/signer 库测试 | API、签名、授权及消费者变化时保留编码、身份绑定、允许与拒绝路径 | Worker 平台、真实 HTTP/CLI/浏览器身份链路变化追加实际消费者 | 全部适用平台与完整身份链路 |
| Viewer `test:frontend-structure`、`test:feedback-contract`、`test:ui` | 前端、接口及消费者变化时执行相关逻辑、反馈合同与必要编译 | auth/会话/权限变化执行 `viewer-auth-browser-security-smoke.mjs`；WASM/runtime 变化验证真实消费者 | 完整浏览器、WASM 构建与跨模块组合 |
| `oasis7_simulator --test oasis7_init_demo --features test_tier_full`，现有过滤 `oasis7_init_demo_runs_` | 场景资产/初始化及必要消费者变化时保留实际场景验证；能否快速须量测 | 初始化、资源守恒和世界配置的相关完整场景属于必要集合，不因 full 标签推迟 | 全部场景及扩展组合 |
| Formal 五个逻辑 suite 的完整 case | 有界状态转换与关键不变量由适用库/合同用例验证 | 需要提前验证重型场景时显式运行 candidate-full，普通 PR 不启动重型 Formal | 定期与候选 full 均保留全部逻辑 suite 与原 70/40 次长场景，物理分片不决定覆盖范围 |
| `workflow_governance`、`packaging_contracts`、Command Center Core/UI/类型合同 | 对应控制、打包、Core/UI 输入变化时执行选测/失败传播/接口等适用合同与编译 | 来源/权限/缓存写边界接受可信旧验证；安装、路径、进程及平台变化验证实际 OS | 完整控制合同、原生 App/打包及运维平台矩阵 |

例如 [共识用例](../../../crates/oasis7_consensus/src/quorum/tests.rs)中的冲突拒绝和提交后发布可作为快速层的真实不变量检查；[成员恢复认证矩阵](../../../crates/oasis7_consensus/src/membership_restore_auth_tests.rs)使用真实签名并检查拒绝后原状态不变，不能用“只保留成功 smoke”替代。Formal 的 `protected_endpoint_children_world_long_history_prunes_reopens_and_retains_original` 和 `protected_endpoint_children_sigkill_runtime_durable_phases_resume_original_once` 均在定期或显式候选 full 执行，历史裁剪、重开、durable 阶段及 crash/retry 修改也不改变其所属层。完整选择依据逻辑合同和实际 case，不按物理 shard 名推断覆盖。

### 快速基础检查与编译范围

#### 独立快速 target 库存

可信 `ci-required-scope.json` 同时登记精确输入合同和组件源码合同，均绑定实际 Cargo 目标及非空最低 case 清单。仅 `pr-fast/main-fast` 的 targeted 计划、没有测试政策迁移且每个相关 native/Formal 输入均命中可信合同，才以 `ordinary_fast_targets` 替代包级 `--tests`。组件路径匹配只是候选条件，必须再证明改动源码沿真实 module/include 路由在所选 target/features 中可达；`src/bin`、未接线模块和所选 lib 之外的 integration 不能借目录匹配进入快速计划。旧 BASE 缺 capability、共享控制、未知或未映射输入、混合风险变化均保留原必要范围。

| 目标 | 完整原始用例 | 选择依据 |
| --- | --- | --- |
| Runtime `pr_fast_runtime` | `src/tests/basic.rs` 的 20 个用例 | 已登记的动作测试和独立 round-robin 调度，包含拒绝、ID 回绕、余额溢出、容量与 tick authority 不变量 |
| World State `pr_fast_reducers` | Runtime `src/tests/apply_domain_event_guards.rs` 的 11 个用例 | 已登记归约测试源，保留幂等、篡改重复、legacy 格式、错误 service 合约及缺失角色的拒绝与状态不变 |
| Runtime `pr_fast_incremental` | `oasis7/tests/incremental_runtime_commit.rs`、`incremental_runtime_allocators.rs`、`incremental_due_module_batch.rs` | Raw/Zstd 实际关闭重开、原始父材料、冷根续跑及有界调度批次；三个原 case 和全部断言不变 |
| Runtime `--lib` | 全部非 ignored 原生 unit tests | 已登记的增量 commit/allocator/clock/body/scheduler 实现或测试变化始终执行，同时以最低清单保留 parent/fence 拒绝、取消无成功前缀、stale clock、重复实例与 allocator 错误等负例 |
| 组件 `--lib` | World State、Simulator、World Service、World Projection、oasis7、Viewer Codec、Observability、Launcher Config 各自全部非 ignored unit tests | 可信源码合同和真实 target 可达性同时成立；World State 的生产及未证明私有测试输入同时执行 Runtime lib 与 `pr_fast_reducers`，保留跨 crate 的真实行为测试 owner；不以函数名过滤减少执行 |
| Viewer Runtime `--lib` | `runtime_live/agent_service_*.rs` 与 `stream_budget_diagnostics.rs` 的行为由全部非 ignored library tests 承接 | 无默认 feature、启用 `test_tier_required`；最低清单保留同阶段精确分派、身份变化拒绝、失败后原请求查询恢复和诊断有界性；其他 Viewer 源码继续使用原范围 |
| Game Client 本地服务 CLI | 完整 library，以及 `identity_cli`、`dev_test_catalog_cli`、`legacy_profile`、`dev_test_service_cli` 四个完整 integration targets | 仅登记私有 Unix CLI 适配文件及上述独立测试入口；真实二进制保护参数分派、私有配置、输入边界、socket 调用、凭据拒绝和子进程回收；共享 SDK、权限校验实现、身份、存储与恢复源码不在该合同内 |

目标使用配置中明确登记的默认 feature 设置与显式 features；声明 `test_tier_required` 的目标启用该 feature。Game Client 使用默认 features，与原 support library 入口相同；不能把该包没有 feature 声明当成永久不变量。增量生产修改同时选择 Runtime lib 和物理 integration，保留正常、拒绝和取消保护；不默认构建或执行 Chain Runtime 等上层完整测试包。精确输入规则优先，已有 module/导出声明限制失败时，组件规则不能再次放行。组件规则使用有界的 owner 源码范围及排除项，并在 base/source/tested 比较中核对真实可达性，不能把整个目录无条件视为已覆盖。

#### 私有 library 单元测试输入

可信 `ordinary_unit_test_contract_version=1` 及 `ordinary_unit_test_targets` 登记可独立承接私有测试源码的完整 library，首批为 Runtime、World State。该能力只适用于所有改动输入均为既有 Rust 文件、全部比较树都证明同一真实 owner 的普通计划。先闭合所有静态 `mod`、`#[path]`、`include!` 引用，再逐路确认唯一 Cargo 根为所选 library，且每路均有明确 `cfg(test)`；同时按该 target 的真实 features 证明执行可达。其他 library、binary、integration、example、bench 或禁用 harness 的根均不能被遗漏。文件名包含 test、只有一条成功路由、Unix 或非 WASM 门控均不足以取得此能力。

该登记合同限定这些私有源码不是其他消费者的运行时数据输入；新增数据消费者必须登记真实输入边，不能以动态路径读取绕过已有影响分析。静态检查不声称证明任意 Rust 程序的文件 I/O 或外部宏行为。它保守拒绝跨包/其他 target 挂载、未知或宏生成模块路径、include 宏别名与参数转发、Git 符号链接、非 NFC 标识符、已知改动源码的数据读者及未解析嵌入。正常嵌套的已解析 include 调用仍按其真实输入检查；读取未改祖先文件的字节不等于读取改动叶子。全部本地 host 生成入口须属于已有可信摘要登记的自包含 producer，且无未登记 build script、proc-macro 或传递本地 build dependency；不能以有限字符串扫描声称证明任意 host 文件访问。BASE、source、实际 target 与 tested merge 所涉及的树分别证明，候选不能改变能力、target 或生成器登记来授权自己。新增/删除文件、测试门控迁移、混合生产或控制输入、路径加测及显式请求均保留原范围。额外词法库存只在可信能力登记的候选源码变更中启用，未扫描不能当作空库存授权；文档和控制输入沿原路径处理。

通过证明时仅去掉这些输入没有实际消费关系的反向测试目标和组；保留完整 affected 集合作为证据，另列每个输入的 owner、比较树和排除的原组。完整 owner library 的全部非 ignored case、原 feature 组合、strict Clippy、基线、格式与完整架构声明图继续执行。World State 的生产及未证明输入仍绑定 Runtime lib、reducers 和适用的物理 incremental target；共享测试按全部真实 owner 执行。旧 BASE 缺少该能力时沿用原选择，周期 full 的完整库存不变。候选 preview 只测量新规则，不替代迁移 PR 的旧可信门禁；hosted 端到端和普通 PR 分布分别验收。

Game Client 的 `dev_test_service_cli.rs` 只承接原 `dev-test-serve`、`dev-test-call` 的参数分派与调用适配。Linux 本地身份代理的 `dev-test-local-serve`、`dev-test-local-call` 由可信公共分派入口按完整命令名提前交给未登记的 `dev_test_local_adapter`，平台条件、模型请求解码和本地身份校验均留在该模块；这些输入与公共分派入口变化继续扩测，不能进入 legacy 私有叶子的快速合同。私有配置的文件类型、权限、UID 校验和原请求解码、65536 字节上限由未登记的 service 实现拥有，语义保持不变。适配文件仅允许审定私有入口的函数体变化，新增 import、导出、impl、trait、模块、unsafe、属性或未知宏不能借此放行。每个比较树都核对私有模块引用库存及实际反向消费者的 commands 路线；新增 SDK 包装、重导出或别名导入均使合同失效。已审定 commands 分派与唯一二进制入口以可信摘要固定，变化后须重新评审登记，不能假定目标分支先合入的新调用边无影响。固定 `wasm32-unknown-unknown` 不执行稳定的 `cfg(unix)` 路由，但仍须核对所有挂载、嵌入与 host 生成路线；新增或改写 production cfg 不享受此豁免。

边界回归同时使用真实 legacy 源码的正例与独立的带平台属性源码负例；后者不能随生产文件拆分而失去保守保护。完整仓库中的普通函数体变化及预览的注释变化都必须得到原五个完整 targets、inline/support 转交和三包 all-targets Clippy；Linux 代理、公共分派、身份或混合输入仍保留原必要范围。本地代理的命令兼容与跨 UID 成功、拒绝、权限和恢复由真实二进制及既有 Linux 进程夹具验证，不以同 UID 模拟替代内核主体校验。

完整 Game Client support 在原 library 与 strict Clippy 之外执行 legacy 服务、本地传输和本地命令三个完整 integration targets；一次性 GitHub-hosted Linux runner 还构建真实 binary，由 root 所有的有限时进程组执行既有跨 UID 夹具。夹具仅复制本次 binary 和合成材料到 root 拥有的临时目录，私有材料另置 `0700` 子目录，保留获准／未获准主体、不可读私有文件、用途／版本、撤销／过期和滴流后重连断言；信号、断言或清理失败均不能完成 support。CI 缺失 Linux／hosted 身份或提权能力直接失败，不降为跳过；本地普通用户只运行可执行的 Cargo 合同，明确报告跨 UID 未执行，完整验收仍需隔离 Linux 证据。该入口仅随实际 Game Client support 执行，legacy 快速合同的五个目标保持不变。

调用库存覆盖全部 workspace 的 Rust 宏来源及实际 include 输入，不能因导出宏没有普通依赖边就忽略它。raw identifier 按真实标识符比较，未知宏参数和模板保持保守；消费者依赖闭包出现未经审定的本地 proc-macro 时回退，不尝试分析任意常量生成的 Rust 源码。独立本地 commands 模块及标准断言中的普通字段、索引和值不作为公共 CLI 路线；本地断言宏覆盖、别名导入和 macro_use 不能借这个表达式例外放行。

可信 `ordinary_fast_support_contracts` 明确登记该 owner 的精确输入和可延后到 full 的反向 support 消费者。只有每项相关输入都命中合同、真实 target 可达、native/test-only 证明成立且无风险扩测时，才把 support 自身的完整 library 测试交给 inline，把自身及被延期消费者原默认 feature 的全部 `--all-targets` strict Clippy 交给 rust-baseline。独立 integration 测试输入也必须经同一证明，不能仅因属于 support 包就跳过 ordinary。仅由同一私有适配输入传递触发、且已登记的消费者完整回归留在 full；计划保留完整 affected 集合和具体转交、延后清单。直接消费者修改、路径加测、显式请求、混合未知输入、测试政策变化和旧能力均保留原必要检查，不把任意 support 包或目录整体变成快速范围。

该合同的性能目标是 workflow 创建到最终 required gate 不超过 600 秒，包含排队、checkout、缓存、编译、Clippy、全部所选测试与汇总。冷、暖缓存均按真实行为改动或新增回归取样，并核对 source/tested/policy、实际目标和完整用例回执。首次 CI 控制变更仍使用可信 BASE 范围；合成差异的诊断只证明候选计划和当前构建成本，不能代签真实 PR、P95 或把超时样本改列例外。

完整反向消费者集合继续写入计划作为影响证据。已证明的组件合同承接普通行为，上层完整集成回归放入 periodic-full；直接规则、手动必要加测、未覆盖输入及平台风险仍取并集。新能力下重型 Formal 始终由完整 profile 承接；原 Formal 风险推导保留旧 BASE 及其他普通覆盖边界，最终延期不能在快速层留下 world-history 等 suite。Launcher release 构建只在其真实输入或必要风险要求时加入快速图。WASM 保留适用消费者编译；仅当源码路由与 cfg 可证明输入不进入 WASM 构建时，才排除纯传递依赖带来的无关 WASM 检查，解析不确定时保留。

不可达证明同时核对实际 library、binary 与 build script 入口、静态模块挂载和嵌入数据消费者。原生源码可能经直接或传递的 Cargo build-dependency、proc-macro 或 build script 在 host 上生成便携目标的输入；这种路线必须保留平台验证，不能仅凭 native cfg 排除。自定义或无法解析的入口同样保守处理；包里存在 build script 本身不等于所有 library 源码都作为 host 生成器执行。

已选完整 library 的实际入口可以按本次差异证明平台不可达：每个 base/source/tested 比较都只能新增顶层静态常规模块声明，逐条带精确的 `#[cfg(not(target_arch = "wasm32"))]`，既有 token 保持不变；被挂载模块必须在同一比较中新建、具有唯一常规路径，并在所选 native target/features 中真实可达。只有这些新增声明及模块的影响可排除，不能因入口既有代码可供 WASM 使用就误算整个文件，也不能凭 native 属性忽略其他改动。已有声明或属性改写、删除、重新挂载既有文件、混合便携改动、未知宏/动态路径，以及 host 生成或其他便携挂载/嵌入消费者均保留原验证。此证明沿用已有可信快速能力，不放宽下述无属性 `static_module_additions` 的独立准入规则。

构建前直接生成 `cargo test -p <package> --lib` 或 `--test <name>`；strict Clippy 使用同一命令列表。独立模式不隐式追加 Viewer/snapshot harness。artifact 核对 package/target、请求及实际 features、默认 feature、最低 case 存在且非 ignored、计划与 source/tested 身份；缺目标、空执行、错误变体或遗漏最低用例均失败。最低名单不限制执行，所选目标的全部非 ignored 用例包括后续新增回归都实际运行。

同一 package、相同默认 feature 设置和相同显式 features 的快速目标可合并到一次 Cargo 构建调用，例如同时指定 `--lib` 与具名 `--test`；不同 package 或 feature 组合不合并。构建清单继续为每个目标分别保存并校验真实 executable、features、摘要及完整用例，不以命令合并消除任何目标或验收项。

修改测试或生产源码时，即使已有 feature gate 没有变化，也核对快速组合可达性；无法执行则扩测。新增共享测试只有证明静态 `mod/include!` 路由实际到达被选 Cargo target 且全部门控成立，才继续精简，未知宏、动态路径和断开入口保守处理。`native.rs/world/mod.rs` 保留已登记声明/导出的精确新增规则；精确 native cfg 也是声明的一部分，删除或替换已有 API、其他代码仍回退。

可信规则可为完整 library 目标登记 `static_module_additions`。它只接纳入口最外层新增的无属性 `mod name;` 及其可见性形式，要求源码实际新增于同一比较范围、使用唯一常规模块路径，且所选 target/features 能执行入口和新模块。此能力用于 Runtime `world/mod.rs` 与 Viewer `runtime_live.rs`，不放宽 `native.rs` 的导出合同。base/source/tested 的每一项实际差异均须通过；已有代码变更、模块删除/替换、`cfg/path` 属性、inline module、宏挂载或歧义路径不能借此准入。新模块内的门控和嵌入输入继续接受原有风险及可达性检查。

periodic-full/candidate-full 保留原 26 组、全部包级测试、Formal、扩展 features 和平台组合，并通过独立 `ordinary_full_fast_targets` **追加全部快速目标的相同默认 feature 设置与显式 features 组合**。默认 feature 的 full 不能抵扣仅在无 Wasmtime 下存在的行为。补充构建、Clippy 和测试均执行，产物按 feature 变体区分；验证补充后仍独立验证原库存，不能用补充 harness 顶替原 required。旧 BASE 缺此能力时仍使用旧入口。

该机制和本地测试不证明 hosted 十分钟/P95 达标。仍按完整排队、准备、编译、Clippy、测试和汇总的真实时长验收；本 CI 控制 PR 自身接受旧可信规则，候选不能先减测使自己通过。

#### 快速执行链路与专项 baseline

可信计划为已选中的组件快速目标声明 inline 执行能力。`oasis7-required` 保持固定逻辑 job 身份，每个执行分区在同一 checkout/工具链中完成真实测试构建与执行，复用现有 build manifest、最低 case、features 和非空执行核验；不再为这些目标启动 ordinary artifact producer，再上传下载到第二台 runner。旧计划缺少 inline 能力字段、包级回退和完整 profile 继续使用原 producer/consumer 路径。汇总仍核对实际选中的执行模式，不能将 selected job 的 skipped、cancelled、missing 或失败视为成功。

可信 `ordinary_partition_version=1` 仅允许目标 owner 恰为 Runtime 与 World State 的有界 native inline 计划按这两个 package 分到并行 runner。父计划保留完整 `ordinary_fast_targets`，`ordinary_partitions` 必须恰好覆盖其全部 package 且无重复；每个目标仍只有一个执行方。单 package、其他 owner 组合、旧能力和非 inline 计划使用 `all`，保持原执行方式；不扩成任意多包 runner 扇出。分区不能修改父计划或其 source/tested/policy 身份，也不能单独宣称整个 ordinary 组已通过。构建与消费入口重算分区的精确目标子集，绑定完整父计划、分区名和目标变体；未知分区、漏项、重复、跨包或 features 不符均失败。矩阵由同一可信计划生成，`fail-fast: false` 保留全部分区结果；最终 required 汇总等待所有选中矩阵成员成功。Clippy、架构声明图和反向消费者继续读取完整父计划，不随测试分区缩减。

有界 inline 计划通过 `ordinary_clippy_group=rust_baseline` 将 strict Clippy 移到已有并行任务，缩短 native 构建和测试前的串行路径；不新增 job，也不改变 target、features 或 lint 参数。两个执行组都读取同一可信计划，核对 source/tested 身份后再判断唯一执行方；迁移要求计划同时选择 `rust_baseline` 与 `oasis7_required`，任一组失败仍由原 required gate 阻断。已知能力中的非法执行组、缺失所需组或不适用 profile 必须失败；缺少该字段时保留 ordinary 组内 Clippy，未知整数能力与旧计划继续使用原执行路径，不能让两个组都跳过。full 保留原 Clippy 全集及精确快速变体补充。

可信配置的独立 `distfs_clippy_contract_version=1` 进一步允许 Runtime 与 World State 的上述 native 双包计划将 DistFS 组原有两条 strict Clippy 调用交给 `rust_baseline`：CAS/Formal Protocol 的默认 feature library，以及 DistFS 的默认 feature library 与 durability endpoint binary，完整参数和三项拒绝 lint 保持不变。只有完整父计划同时选中 `distfs`、`rust_baseline` 和 ordinary，已证明 `ordinary-targets` 反向消费者边界，且没有例外、Formal 或 WASM 扩展时，才能声明 `distfs_clippy_group=rust_baseline`。baseline 将这两条独立 Cargo 调用追加到原 Clippy 清单，DistFS 核对同一可信 selection 的 source/tested 身份和能力后才转交；DistFS 的四个原测试 target 仍全部按原单线程参数执行。版本与 owner 必须成对且合法，缺失必需组或不兼容计划失败。旧 BASE 没有新能力、独立本地组和 full 均保留 DistFS 原位 Clippy；不能借既有 ordinary Clippy 字段取得新转交权限。required gate 继续等待两个组，任一失败、取消、遗漏或跳过均阻断。预览也显式传递同一完整计划，统计包含 baseline 新增工作在内的最后完成时间。

完整构建清单通过核验后，未分区 inline 的独立 harness 可最多两个并行执行，全部 libtest 线程的预算按可用 CPU 分配。已验证的 package 分区使用一个 worker 顺序执行该包全部 harness，将当前可用 CPU 预算分配给正在运行的 libtest；既有较低的 `RUST_TEST_THREADS` 上限仍生效，不提供任意扩线程开关。经完整身份与实际库存核验的 library harness 优先派发，避免短 integration 推迟长库的完成；两类各自保持原顺序，未知或旧 capability 保留原调度。每个 harness 仍执行全部非 ignored 用例并独立核对计数、退出、超时及后代清理；不拆分或截短单个场景。任一失败保留原失败，停止启动待执行项并取消、回收本轮已启动任务；后续成功不能覆盖失败或取消，未执行项不能算通过。进程监督器的可选取消信号只作用于本次拥有的进程树，默认串行调用和完整 profile 保留原行为。分区的额外冷依赖编译、runner 排队和资源竞争仍计入真实端到端预算，不能直接用历史单机耗时推定提速。

调试与测试构建中的密码学热点沿用根 manifest 的精确 package/version 优化：可执行文件校验同时覆盖实际使用的两个 SHA-256 版本，重复私钥封套派生覆盖锁定的 Argon2 实现。仅提高这些依赖的 `opt-level`，保留 debug assertions、overflow checks、固定 KDF 参数、封套格式与完整行为回归；不通过降低安全参数或复用解锁结果缩短测试。Cargo 的 test profile 继承 dev profile，包级配置与继承按 [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#overrides) 执行。比较相同产品源码、features、runner 和冷暖状态下的编译加测试关键路径后判断收益，不能只报告函数速度或忽略新增编译成本。

`rust-baseline` 的格式、架构、RustSec 与文件约束继续执行。NewAPI accounting 仅在其真实 package/依赖输入受影响时执行；独立工具锁文件检查在相应 manifest/lock、检查器或共享构建输入变化时执行。选择来自可信计划及完整差异，不靠候选环境变量自由关闭；没有新能力的旧计划和完整 profile 保留原检查。周期 full 仍执行这些专项全部原命令。

架构编译分层遵循[主责合同](../../engineering/rust-governance/rust-engineering-contracts.design.md#trusted-local-architecture)，Trace refs: O7-REQ-f2a285fe-6e46-4b51-aea7-3ba32e06cf84。可信配置的 `architecture_build_policy_version=1` 仅对完整符合普通快速合同的直接输入启用 `ordinary-targets`；消费者重核目标 owner、source/tested、必需组和干净 checkout。全部登记 profile 的声明/依赖图持续阻断违规，额外编译以 `deferred_to_full` 记录，快速目标的真实构建、Clippy 与测试继续由当轮两个必需组执行。混合非普通输入、必要加测、共享控制、旧能力或 full 保留相应原矩阵，不通过诊断 `--graphs-only` 放行。周期 full 承担额外 feature/平台及上层组合的实际兼容编译。

已覆盖直接输入的反向消费者可以继续选择 `consensus/distfs/net/node`，同时按主责合同提供 `architecture.consumer_groups` 的精确包归属与反向可达证明。组数本身不迫使这些普通输入重做全部架构编译；直接路径命中的额外职责、未覆盖输入和显式加测仍保留原矩阵。消费者检查及其失败汇总继续执行。

Viewer 的完整 UI 测试与 software-safe 构建在输入、输出互不依赖时可并行，前置的共享 fixture/产物清理合同先串行完成。构建内部的 Cargo 命令顺序、目标和 features 保持，不能通过另设 target 目录绕过构建锁。只有 UI 与真实构建都成功且各自进程完成清理后，才执行使用新产物的浏览器安全验证；任何失败、超时、取消或遗留后代均阻断，不使用旧 dist 抵扣构建。并发收益计入同一 runner 的实际资源竞争，不能直接以两个历史耗时相减宣称 hosted 达标。

Viewer 的 Vitest UI 测试保留单 worker、逐文件独立 fork 与 jsdom，只复用 Node 编译字节码，不复用测试文件的可变模块或全局状态。worker 默认使用本工作区 `node_modules/.cache/vitest-node-compile`；调用者显式的 `NODE_COMPILE_CACHE` 路径和 `NODE_DISABLE_COMPILE_CACHE=1` 仍有效。沿用 Node 对源码和运行时版本的缓存校验、Vitest 对 V8 coverage 的禁用保护；缓存不可用时仍执行原测试。该目录不上传为跨 job 缓存，完整文件集合、原断言、Solid 模块身份及失败/清理合同保持。

候选策略可通过独立、只读的性能预览取得合入前数据。现有 Rust workflow 的 `pr-fast-preview-plan` 在 `rust.yml`、`ci-required-scope.json`、`ci-tests.sh`、`cargo-dev-architecture.json` 或实际构建驱动 `ci-test-artifacts.py`、`cargo-dev-architecture.py` 变化时启用产品预览；单独修改 planner 或 preview 编排的 Python 实现及测试时，执行其所属合同与实际 required 消费者，不额外构建产品性能样本。混合修改仍由执行配置或构建驱动启用预览。

预览的触发差异复用 required 选择器的 Git 身份与比较范围：workflow 显式传入事件 BASE、实际 PR source 和 tested commit；在验证真实 target 与 source 父节点、BASE 祖先关系及唯一共同祖先后，取共同祖先到 source、实际 target 到 tested 的差异并集，包含删除和重命名两端。main 单独前进不触发预览，真实 source 改动与 merge resolution 改动仍触发；身份或差异无法确认时报告失败，不能安静跳过。这个范围只决定是否运行诊断，不改变可信 required 计划。

预览以实际 tested tree 为每个样本构造一份仅注释不同的 baseline，benchmark 的 source/tested checkout 保持该 tested commit 不变。`runtime-world` 保留两个既有 Rust 输入，必须包含 baseline、rust-baseline、ordinary 及 Runtime lib、World State lib、reducers、物理 incremental 四个完整 target；额外组仅接受上述 `architecture.consumer_groups` 明确证明的 native 反向消费者。`game-client-private-unix` 只使用已登记的私有 CLI 适配输入，必须包含三个核心组、完整 game-client lib 与四个 CLI integration targets，以及自身运行转交、两项 support 消费者延后和原 all-targets Clippy 保留清单。`executable-image` 使用独立库的私有摘要输入，必须包含三个核心组和关闭默认 features 的完整 library target，用于实测组件编译隔离。各样本期望目标与 feature 组合由预览独立固定，不能从候选选测结果反推应有覆盖。

`ci-fast-preview.py` 按样本分别记录实际 PR BASE/source/target、分支共同祖先和触发比较范围，以及 benchmark 的 policy/source/tested、合成 BASE/tree、固定输入、全部组与 ordinary 分区，下游 runner 按样本与确定性 Git 身份重建比较对象。独立样本最多并行规划两路，各自使用临时 Git index 和独立输出路径，不修改 checkout、真实 index 或 ref。`plan-matrix` 只有在所有样本完整校验成功后才一次发布执行矩阵；不发布半份成功。每行绑定 sample、group、partition 与该样本的合成 BASE，恢复时重核行归属；selection、metadata 和结果 artifact 分别命名，结果名称及时间记录包含 partition，禁止跨样本或分区复用。Runtime 的额外消费者组仍全部执行，样本完成时间取所有分区及其余组的最后完成时间。缺项、未解释的额外需求或错误 feature 策略均不能被静默省略。

预览矩阵直接消费已验证计划输出的完整组列表，所有选中组并行运行真实 driver，不恢复缓存，记录准备、编译、测试与结果；每个样本从 workflow 创建到其全部所选 job 完成的时长独立验收，包含排队、规划和产物上传，不能把单个 job 的 runner_seconds 当成端到端或混用另一份样本的完成时间。不能只运行三个核心组而漏掉计划中的消费者。该预览测量当前 Rust 测试与编译成本，不证明一般差异覆盖、回归质量或 P95；它不是本 CI 控制 PR 的 authoritative required 计划，不生成 release 可接受的完整结果。metadata、日志和状态使用独立名称，不把预览成功混记为当前 PR 已按新策略通过。

baseline 保留必要治理、格式与一致性检查。文档路径按完整差异检查变更正文及受影响的索引、入站引用、删除/重命名和 authority 关系；无相关输入的子检查及时返回。Markdown block/html/link 共享有界、按精确文本缓存的 CommonMark 解析结果，fragment/可见文本查询复用纯解析结果；保留全局 ID/索引等必要不变量，不能只读取修改行。快速 baseline 不再附加 `--full-corpus` 历史内容报告，完整 profile 仍运行，原告警级别不变。

单次系统设计追踪检查可复用本轮 Git 工作树与暂存区的变更映射，避免每读取一个引用目标都重复扫描全仓状态。每次调用重新取得映射，仍按原规则区分 unstaged、staged-only、删除、重命名及未跟踪文件；工作树恢复为 HEAD 而暂存区仍有修改时，读取暂存内容。内容缓存限于本轮首次读取，不跨调用复用，也不把本地可变工作区宣称为原子不可变快照。

未命中独立快速 target 合同时，`oasis7_required` 从可信 26 包库存选择受影响 package，对所选包执行 strict Clippy 和全部非 ignored `--tests`；便携 WASM 编译从原四包库存取实际受影响闭包。Viewer 子进程与默认 feature snapshot 变体只在对应真实消费者被选中时构建。support 的普通 profile 可能只执行 library 或指定 binary，不能据 Cargo target 存在就认定普通入口会执行它；测试政策变化时保留原 profile，并追加所选包的 `--all-targets` 及适用 full features。扩展 feature 集合为空仍执行这些 target，省略空的 `--features` 参数；空 feature 不表示没有新增用例。未知映射或保守 scope 恢复旧库存，不以空 package 列表通过选中的 ordinary 组。

新能力下 Formal 仅在完整 profile 构建其既有共用依赖闭包，并执行全部五个逻辑 suite 的完整 case；旧 BASE 保留原风险选择。逻辑 suite 按可信 `case_shards` 映射物理 shard，跨 shard 的完整 case 必须保留。artifact 把可信 BASE 最低库存与候选新增项合并，既有 case 身份、ignored filters、环境 batch、shell tests 与依赖不能由候选删除；缺预期 case 或新增所需 shard 不在可信计划内均失败。build、pack、run 绑定同一 authority 摘要、同一计划和实际 tested source。

Cargo 的测试名过滤只限制执行，不能当作编译范围缩小的证据。快速集合仍包含实际行为测试，不以 `cargo check` 或空 harness 代替；单个局部编译示例也不能证明所有普通 PR 已达到预算。

兼容的同 run、同 tested commit、同编译输入产物可复用；消费者核对工具链、平台、profile/features、测试清单与摘要，执行被选中的真实用例。缺产物、输入不符、零匹配、错误 feature/target 和意外跳过均失败。长历史与恢复场景保持原断言和操作长度，在周期层或相应定向验证中执行。

保持可信 main producer、只读 PR 和已有仓库缓存预算；新快速 profile 的 main push 同样只读，完整 main profile 保留 workspace 与工具缓存生产，共享原生依赖由独立 main schedule/manual-full 预热任务生产。旧 BASE 回退保留原权限分支，候选始终不能借 full 名称取得写权限。缓存缺失走同一正确冷构建。记录实际 Cargo fresh/rebuilt、恢复与准备成本，不把 cache hit 当作复用或十分钟达标；独立预热的覆盖与写入边界见[缓存设计](#ci-install-cache-design)。

### 完整集合与结果汇总

对相同 source 和环境，令 R 为适用快速及必要加测集合，M 为从旧 PR 后移的检查，F 为完整回归集合，验证 R 与 M 均包含于 F。periodic-full 从全仓适用库存展开，不只选择本轮差异；必需的平台组合由平台矩阵执行。清单包含稳定检查 ID、真实命令/参数、前置、target/features 与结果，同名命令的不同环境不能互相抵扣。

完整计划展开可信 scope 的全部 26 组，包含组级 Clippy、依赖本轮真实构建的 Viewer auth browser smoke、Command Center App、Windows rollout、macOS 包合同及 fleet-health 三平台。普通产物、Formal 产物/物理矩阵、平台组和 `full-extended` 可并行，`explicit-full-gate` 等待所有组、ordinary producer，以及恰好一个成功的可信或候选 full producer。

`full-extended` 只运行 core 扩展 features、support 的完整 target/feature 组合、fixture smoke 与生产 mock guard，不重复整个 required 清单；它单独成功不代表完整回归。support 按真实 manifest 生成 `--all-targets` 调用，启用适用 full features；没有扩展 feature 或该轮没有 `test_policy_changed` 时也执行，保留不在普通 profile 中的 integration 等 target。`full-standard` 保留本地普通加扩展入口，并补齐原遗漏的 Clippy/browser smoke；`full` 再加 Formal，但两者都不能代签 hosted 的多平台集合。

PR 汇总只等待本 PR 计划的必要检查，周期汇总等待本轮完整集合。保留可靠 always 收口：规划失败、选中任务失败/取消/超时/缺失、应执行却 skipped 均失败；未选中可以 skipped。预期矩阵单元须全部终态，不能用其它单元成功或总测试数掩盖漏项。新提交取消的旧运行不为新 source HEAD 放行。

保留稳定 required check 名称，避免对整个必需 workflow 加 paths 过滤造成 Pending；依赖失败也必须进入最终汇总。手动候选验证不替代 PR 必需事件产生的检查。若发布所需平台不在本轮完整集合内，补跑准确候选的相应范围后再消费结果，不能把“不适用”用于隐藏缺环境。

### 调度、并发与主线健康

schedule 使用 `17,37,57 * * * *`（UTC）执行轻量观察。短串行 admission 从实际 Actions run、jobs 与版本化 admission 产物判断：无活动 full 且距最近真实 full 启动达到四小时，才启动本轮完整图；没有历史记录且查询完整时启动首轮。失败和取消仍计入真实启动间隔，不每二十分钟重跑测试；错过多个时隙只补当前一轮。每轮固定事件 main SHA，所有 checkout、产物和汇总引用该值，保留 manual full。

PR 并发组按 PR 编号取消过期运行。schedule 观察使用每 run 独立并发组，admission job 使用固定短并发组且不取消正在执行的判断。判断与上传启动资格在同一短锁内；已发布资格但 select 尚未启动也阻止下一轮重复启动。活动 full 超期只报告，不自动取消或重叠。candidate-full 按触发 ref 与固定候选区分，避免取消主线回归或把不同候选串成一个成功结果。完整扩展 job 的执行上限为 210 分钟，主线健康检查另以四小时观测单轮预算；这不等于整条 workflow 已证明在四小时内完成。超过容量目标时优化分片或资源，不无限重叠。需要硬性隔离 PR 容量时使用独立 runner 池，普通 concurrency 不等于平台优先级。

`ci-full-result.py health` 只读观察最近两天 main schedule 的实际 attempt，区分观察与实际 full，报告最近完整运行、最近终态、`last_successful_sha` 和 `commit_range`，缺数据保留 unknown。为覆盖旧 run 最近重跑，metadata 按两天窗口回看三十七天，并单独查询全部活动轮；范围覆盖 GitHub 的[三十天重跑期限](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs)及[三十五天 workflow 运行上限](https://docs.github.com/en/actions/reference/limits)加两天观察窗，不把创建时间当最新 attempt 时间。读取共享 120 秒、800 次请求上限，最多四个并发只读审计，预算不足保留 unknown。合法空跑必须有唯一、同 run/attempt/SHA/workflow 的 admission=false 产物，admission 实际成功，select 与 full gate 实际 skipped，且无矛盾的 selection/full receipt；缺失或冲突不能算健康。旧版本没有 admission job 的 schedule 保持原 full 语义。四小时 cadence 另留一小时观测余量，默认最近启动窗口为五小时、未结束单轮预算为四小时；缺最近成功、漏跑/过期、失败或任一未完成轮预算超限返回非零。新 pending/running 不清除上一终态失败。该观测是调度与主线健康信号，不代替完整 receipt 的发布 qualification，也不会自动重跑、取消或回退。

admission 成功且上述证据完整证明未执行 full，但 workflow 随后失败或取消时，记录为控制失败：健康和发布资格仍失败，不刷新 full 启动时间或占用活动名额，后续观察可按原四小时条件恢复准入。未知结论、未完成状态、缺失或矛盾证据仍拒绝分类，不能用恢复路径掩盖实际执行。

命令接入既有 [CI 耗时回顾任务](../../../.agents/dot/tasks/ci-duration-review.md)的只读检查说明；文档没有新建外部定时器，也不证明巡检已经运行。调用者独立读取 Actions 事实，不能只靠迟到的 schedule 报告自身漏跑。平台调度可能延迟或丢失，规则依据 [GitHub schedule 语义](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)。

失败记录关联最后成功 SHA 到失败 SHA 的提交区间与失败检查。产品回归、测试不稳定和基础设施故障分别处置；无法判断时保留未知。有限重试保留原失败和 attempt，不自动把曾失败的运行解释为稳定。真实严重回归优先修复或按现有授权回退，限制受影响范围继续扩大；不增加“所有 PR 等最新 full”的全仓门禁。成功/失败区间来自 Actions 与 Git，沿用已有任务承接方式，不新增状态账本或自动回退授权。

### 发布消费与权限

Release/Testnet/Mainnet/ECS 发布或推广入口先固定 package source，再通过 `ci-full-result.py verify --source <sha>` 读取可信完整结果。`explicit-full-gate` 在成功验证完整 needs 后生成 `oasis7.ci-full-result/v1` receipt，绑定 source/tested/policy、检查库存版本、检查定义摘要和实际 run/attempt；产物名为 `ci-full-result-<run_id>-<run_attempt>`，保留 30 天。

消费者只接受 canonical Rust workflow 在 main 的 schedule/manual full，核对实际 API run/jobs、成功的 `explicit-full-gate`、相同 source/tested SHA 与 policy/check-definition 摘要。默认完整查询最近 30 天相关 runs 和必要分页，按最新实际 attempt 顺序判断；`--run-id` 可指定准确运行，但仍须满足其当前 attempt、来源和完整结果。较新的同源运行缺当前 attempt 成功 receipt（包括 pending/running）、失败重跑、过期/歧义产物、API 缺页或未知来源不能回退旧绿色放行。读取结果身份与用于查询的 selection artifact 均绑定实际成功的 producing attempt，失败重跑不能借旧 selection 假称验证了另一个候选。

API 已证明为专用非 full 诊断或经上述严格核验的 schedule 空跑，不会冒充完整结果，也不影响已有完整证据；单纯缺少 selection 不能据此排除未知运行。实际较新的同源 full 失败、取消、未完成或缺当前 attempt 成功 receipt，仍不得回退旧绿色。

普通候选分支自行修改 workflow 产生的绿色和 `bootstrap_full` 均不能取得发布资格。验证尚未覆盖的候选使用可信 main workflow 的 manual full 与 `source_ref`，select 一次解析不可变 source，所有执行 checkout 同一 SHA；变更中的策略/检查定义不等于受信任的新策略。验证代码不获得发布凭据，PR 产物不直接作为特权入口的可执行输入。当前 ECS 包/冷启动 smoke 与其他发布验证继续执行，完整结果不替代它们。

默认发布来源必须属于已 fetch 的受保护 main 历史，可信 policy 同样核对该历史。Testnet 的既有固定候选合同由 `--allow-candidate-source` 保留，该参数只接受 `GITHUB_WORKFLOW_REF=<repo>/.github/workflows/testnet-packages.yml@refs/heads/main` 的调用；它只放宽 source 的 main 祖先要求，仍强制该候选的可信 main 完整结果、相同检查定义和其余验证。其他 workflow 不能借此参数发布候选。校验不要求已验证 source 等于随时前进的 main tip；旧保留版本的显式回滚继续遵守既有恢复合同，不把回滚改称新版本验证成功。

### 接线状态与验收

检查映射、baseline/构建/执行范围、完整库存、四小时触发、汇总与发布消费已在同一候选接线。本实现 PR 自身仍接受可信旧基线的必要验证，不先降门禁使自己通过。新接口随实际合入后的可信 BASE 生效；回退使用正常修复 PR 恢复已知选择与汇总，不在失败时自动跳过检查。

验收至少核对：

- 文档/普通代码/关键协议或持久化/共享依赖四类真实计划，删除与重命名、未知输入、解析失败和控制自改均保持正确边界。
- 快速/完整清单的真实命令和平台包含关系；故意漏掉 Clippy、browser smoke、平台单元或迁出的 case 能被识别。
- 失败、取消、超时、产物缺失、零匹配和意外 skipped 传播到对应汇总；检查全部成功才通过。
- 周期与 main push 并发、PR 连续提交、漏跑/超时、main 前进及候选 SHA 变化；不会重复启动整套重回归或使用旧结果放行。
- 发布正例及错误 SHA、非可信结果、缺平台、缺页/未知状态负例；保留原包 smoke 与明确授权的回滚路径。
- 实际 hosted 数据按[反馈口径](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full)报告完整观察窗口、样本数、普通成功 P95、失败/取消与例外、阶段耗时、冷暖缓存、完整回归时长及重任务占用。零匹配、跳过和早失败不算性能达标。

观察已有 full 失败的处置和修复版本的真实完整结果后再确认迁出的保护。文档、Python 合同及小型真实 Cargo fixture 只证明各自范围；普通 PR 十分钟/P95、四小时内完成的容量和真实平台完整覆盖仍待 hosted 数据，不在本文预写验收成功。

<a id="ci-coverage-design"></a>
## 测试覆盖专题的共享设计

本节承接 [coverage PRD](ci-test-coverage.prd.md#coverage-requirements)，不替代它的独立需求，
也不新增 CI 配置。由本设计的选测、调度和结果链承载离线回放、wasmtime 及 full 深度回归。

- **输入到执行**：从同一候选的实际 driver、features/target 与选中 job 核对覆盖清单；
  离线回放保持 snapshot/journal 输入与 HelloAck/Snapshot/Event 消息断言，端口隔离和超时不能省略。
  full 保留适用 required 和原有重型消费者，不以总用例数代替具体功能被执行。
- **失败与发布影响**：区分清单漏项、消费者未调用、命令非零、超时/端口冲突、环境缺失与有效不适用。
  必要链路缺失或失败沿原 suite/job/gate 传播；一次后续成功不能覆盖原失败。
  发布影响根据实际触发的适用门禁说明，不把文档存在当成 coverage 已交付。
- **历史目标**：PRD 中无 wgpu 的 egui_kittest snapshot 降级仍是历史/目标；未有可运行实现及匹配 gate
  证据前不得计入当前覆盖。未来实现须同时验证可诊断降级与有图形能力环境的真实快照，不能把所有 skip 算 PASS。
- **验证**：按 [测试手册](../../../testing-manual.md) 对同候选 required/full 的实际命令、参数、
  选中 case 与结果逐项核对；选择器/结果函数回归仅证明其自身边界，真实离线回放与 wasmtime 执行结果另记。
  清单故意漏项、失败命令和缺环境的负例应能阻断适用结论；本次文档合并没有运行这些场景。

<a id="local-check-entry-design"></a>
### 显式本地检查的薄入口（目标合同）

Trace refs: O7-REQ-83ee480f-007c-446d-a05d-827c51b403e3；承接 [LOCAL-AC-1–5](ci-test-coverage.prd.md#local-check-entry-requirement)。产品适用性：N/A；reason=开发检查工具合同；scope=本地范围、执行与报告，不改变 CI 准入或玩家行为。

当前已有 `ci-tests.sh` 显式 tier、`cargo-dev.sh`、no-op `pre-commit.sh` 和 Git refs planner；目标 `scripts/check-local.py` 尚未实现，当前命令见[手册](../../../testing-manual.md#local-check-entry)。普通 commit 不调用新入口，`fix-precommit.sh` 的格式化与暂存不进入检查链。

**按用途调用，按能力交付。** 目标接口显式指定 `--profile quick|affected|formal`，接受 `--base-ref`、`--output-dir`、`--format text|json` 与 `--dry-run`；formal 另须显式 `--tier required|full`。三种 profile 不是逐级必跑的流水线。尚未实现的 profile/输入组合明确 blocked，不降级成其他 profile 后报通过。

| Profile | 执行合同 | 成功与未覆盖范围 |
| --- | --- | --- |
| quick | 固定复用 `ci-tests.sh required --group baseline`；沿现有检查读取工作树并传递已解析的文档比较基线，不先求 Rust 影响闭包 | baseline 全部适用检查真实通过；未运行 Rust fmt、编译、业务测试、浏览器或真实服务。业务测试数为零须明示，不声称完成 affected 选测 |
| affected | 同一可信 planner 生成组、case、target/features，调用既有 required driver；保留选中/未选中理由 | 全部选中且适用的检查真实通过；测试组须证明 case 非零且未漏执行。不隐含 full、所有平台或真实世界闭环 |
| formal | 干净源码、已解析 source、显式 tier，执行手册 canonical required/full；required 沿可信 planner，full 沿完整清单 | 必要组、环境或证据缺失均不通过；单机结果不代替 hosted gate 或其他平台 |

quick 报告固定 baseline 的选择理由与全部未运行类别即可，不要求为了列出“受影响但未运行的组”先实现 dirty 精确选测。未调用 planner 时，相关字段明确 `not_used` 及原因，仍记录实际 driver/checker 版本、比较基线和输入范围。affected 不另建路径规则、case 清单或改变断言的轻量命令；按包迭代继续用 `cargo-dev.sh`，只证明其开发配置。不得自动缩小慢组、吞失败重试，或把 required 的 `scope=full` 扩张说成 full tier。

**分阶段接线。**

1. quick：先接固定 baseline、真实执行、共同报告及只读/失败边界。其输入记录与变化检测只覆盖实际检查读取的输入，不需要完整 Cargo 依赖闭包或 dirty planner 适配；必须说明各检查实际支持的工作树形态，不能可靠处理的输入 blocked。
2. affected：先用干净提交证明与现有可信 planner 等价并接通真实 driver，再补完整 dirty/untracked 输入。只有规划适配时仅支持显式 dry-run，输出 planned，不宣称 affected 已可执行；执行接线完成但 dirty 适配未完成时，dirty 输入 blocked。
3. formal：在干净输入、只读 canonical 接线与证据合同具备后独立交付，不依赖 dirty 精确选测，也不要求开发者先跑 quick/affected。原 required/full 必要验证不减项。

各阶段都是包含适用回归的小而完整交付。最终输入范围和 LOCAL-AC-1–5 仍沿 [#5386](https://github.com/nil-studio/oasis7/issues/5386) 收口；不以首片可用关闭整体任务。CLI/help/手册同步随对应能力交付，旧 “used by pre-commit” 描述在实现阶段修正。

**基线与可信选择。** `--base-ref` 优先；缺省仅解析本地 `origin/main` 与 HEAD 的唯一 merge-base，不 fetch。记录原 ref 和解析 SHA；必要基线缺失、歧义、未解决合并或输入不可读为 blocked。affected/formal 的 policy 沿现有可信来源并记录 SHA；候选不能修改选测规则后豁免自己，控制文件变化沿原 planner 扩张/拒绝。quick 的固定 baseline 只提供局部反馈，不授予候选缩减 CI 或绕过可信 policy 的能力。

**输入与运行中变化。** quick 沿 baseline 已支持的输入与比较接口执行，记录 staged/unstaged/untracked 状态及实际读取范围，未做影响分析的部分明确未覆盖；若必要检查不能完整处理该输入，blocked。传入 base/head 后某子检查可能只读提交差异，记录 dirty 状态本身不证明它覆盖工作树；必须逐项核对，不能可靠接线时拒绝该输入。所有 profile 固定起始 HEAD/index 与实际检查输入的内容身份，结束复核；变化时记 `stale_input`，保留局部结果，不宣布当前工作树通过。不能用“运行后没变”替代接线前的只读审计。

affected 的最终 dirty 适配保守合并 base→HEAD、暂存、未暂存和非 ignored 未跟踪输入，以 NUL 安全读取；保留来源、rename 旧/新路径、删除旧路径和同路径抵消变化。显式依赖引用的 ignored 输入不得漏读，不能确定时按既有规则扩张或阻断。复用同一 planner 的基线、清单和依赖图逻辑，在内存或自有临时目录构建不可变输入视图，不创建提交、改 index、stash 或 checkout；HEAD 计划不代表 dirty 覆盖。formal 的源码 dirty/untracked 直接 blocked，ignored 构建产物除外。

**只读与故障。** 仅允许 ignored 构建输出、自有临时目录和显式报告目录；锁定命令的依赖缓存写入须披露。不改源码、锁文件、配置、index 或 ref，不自动安装/升级工具。接线前逐个审计 driver 写集合；必要命令须有真实 check-only 或隔离输出，否则 blocked，不能先写后回滚。缺工具、锁定依赖不可用及禁止联网分别诊断。失败停止依赖步骤，独立检查可继续；取消/超时只清理本次拥有的子进程和临时目录，不删除用户输出。

**共同结果。** text/JSON 来自同一对象，包含 `schema_version`、profile/tier、source/base/policy 或明确的不适用原因、输入/dirty 摘要、实际 planner/driver 版本、选择理由、argv/cwd、非凭据配置、时间、退出码、逐项状态与 reason、预期/实际执行数、日志及未覆盖范围。有效检查数与测试 case 数分列，非测试检查不得虚构测试数量；预期执行测试的项仍须证明实际 case 非零。未选择与必要但 skipped 分开；必要执行数量不可解析为 unknown。运行前确定日志位置，报告写失败不能留下成功摘要。

| 总状态 | 退出语义 | 判定 |
| --- | --- | --- |
| passed | 0 | 至少一项有效检查且本 profile 全部必要项实际通过；quick 无业务测试须单列，不是完整测试通过 |
| planned | 0，仅显式 `--dry-run` | 只有计划，`validated=false`，不输出 pass |
| failed / incomplete | 1 | 任一失败、预期测试零匹配、必要项 skipped、执行证据 unknown 或报告写失败 |
| blocked / stale_input | 2 | 不支持的 profile/输入或参数、基线、路径、工具等前置失败，或运行中输入变化；保留局部结果 |
| no_checks | 3 | 无可执行检查并说明原因，不等同通过 |
| cancelled | 130 | 中断并记录未执行范围；超时按 failed，注明 timeout |

验收按[手册中的阶段范围](../../../testing-manual.md#local-check-entry-validation)执行，每片验证实际开放能力，不提前要求运行尚未开放的 profile。共同底线不能延期；涉及 CI 选择语义的改动仍接受适用独立评审。本地时延先记录实际阶段耗时，不新增 SLA。

<a id="ci-case-tier-design"></a>
## Test case 分级专题的共享设计

本节承接 [testcase-tiering PRD](ci-testcase-tiering.prd.md#case-tier-requirements)。该 PRD 继续
拥有 `test_tier_required` / `test_tier_full` 的标签语义；命令级触发、影响选择和汇总由本设计及通用规范拥有。

- **分配与选择**：新增或迁移用例时核对 cfg/feature 标签、Cargo feature 闭包、driver 参数与实际选中列表。
  标签存在但 runner 未执行不是覆盖；同名 case 在不同 target/features 下不能互相代签。
  按[准入规则到实际选择](#ci-pr-fast-selection-design)形成必要集合；除明确后移的重型 Formal 外，full 标签用例命中风险仍进入 PR，full 保留适用 required 及重型回归。
- **退化和冲突**：错标、漏标、full 回归被移除、预期用例未执行、旧硬编码与 feature 过滤重复都应可定位。
  先澄清对应需求与消费者再修正分配，不能为时延数字删断言、吞失败或把零匹配当通过。
  影响不明时沿现有选测规则扩大普通 required，不新设任务身份或通用审批。
- **性能与回归**：保持缺陷对应的 case、失败原因、实际版本、参数与结果可追溯；用可比较环境的实际运行
  评估反馈时间。文档压缩、feature 名称存在或历史绿色结果都不证明提速或 full 覆盖保持。
- **验证**：使用 [测试手册](../../../testing-manual.md) 和实际 CI driver 核对标签→参数→执行结果；
  漏标、错误 features/target、未选中、重复与失败传播分别构造最小适用检查。
  [选择器回归](../../../scripts/plan-rust-required-scope.test.py)与
  [结果函数回归](../../../scripts/ci-required-result.test.py)不代替真实 case 执行。

两专题共用以上实现与验证分配，不重构整个平台、不修改业务断言、不引入自动 case 学习。
旧同名设计路径暂留兼容跳转；删除必须先核清动态/外部分发、索引、配对与 corpus 消费者。

## 2. 上游与本设计

### 2.1 需求承接与分配表

| 上游要求 | 分配内容 | 本设计 | 负责范围 | 限制 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7d1dab3c-451e-4915-bf76-5bd28c117371；[可生成工作流定义](ci-tiered-execution.prd.md#ci-generated-contract-jobs) | 两个重复 job 的单一结构定义、确定性产物、只读校验及等价迁移 | [首轮生成试点](#ci-generated-contract-jobs-design) | CI 维护者；沿用 planner、driver、可信 BASE 和独立评审 | 产品适用性：N/A；reason=工程工作流定义治理；scope=两个合同 job；生成器尚未实现，不承接十分钟/P95、缓存或遥测验收 |
| Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；[快速反馈与周期回归](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full) | 十分钟目标、快速准入/迁出、风险加测、四小时完整覆盖及同源发布 | [分层实施](#ci-fast-pr-periodic-full-design) | CI 维护者、QA 与发布消费者 | 产品适用性：N/A；reason=开发反馈与验证时点治理；scope=检查选择、构建、调度、汇总、发布消费及实施验收。候选已接线，旧 BASE 保守回退；hosted 覆盖、时延及容量待验证 |
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | 离线协议、full/wasmtime、降级目标及失败范围 | [覆盖专题](#ci-coverage-design) | CI 维护者与 QA | 不将未接线 snapshot 或文档合并计为覆盖 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| Trace refs: O7-REQ-83ee480f-007c-446d-a05d-827c51b403e3；[显式本地检查](ci-test-coverage.prd.md#local-check-entry-requirement) | 分阶段交付三 profile；quick 固定 baseline，affected 最终完整本地输入，保留只读与诚实结果 | [薄入口合同](#local-check-entry-design) | 本地工具维护者复用 planner/driver；QA 验证负例 | 产品适用性：N/A；reason=开发反馈工具；scope=本地选择与执行。当前只校准文档，薄入口未实现；不拥有 hosted 时延或发布准入 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | 独立标签语义到实际选中 case 与结果 | [Case 分级](#ci-case-tier-design) | 测试维护者与 CI driver | 不修改断言或扩展任务准入 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | 保留产品测试并按真实影响选择 | [测试范围](#测试范围) | 选择器与执行器 | 解析不确定扩大普通 required，未登记路径另按门禁阻断 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | 所有未登记路径阻断规划，失败到最终 required 汇总，保持可信规则来源 | [失败链路](#ci-unknown-inputs-design) | 选择器、Rust workflow 与结果回归 | 产品适用性：N/A；reason=CI 范围登记治理，不改变游戏行为；scope=未登记路径的规划失败、可信基线与 required 结果传播；本地失败回归不代签 hosted 运行。 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | 最终汇总真实结果 | [调度和结果](#调度和结果) | workflow 与结果函数 | 本地回归不证明 hosted 运行成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | 普通 CI 优先，专项需具体环境不足 | [专项条件](#专项条件) | 对应产品运行场景 | 不新增通用 PM 集成链 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | Linux x86_64 冷热路径共用摘要与版本验证的预编译安装器 | [安装流程](#ci-install-trunk-design) | Release workflow 与共享安装器 | 工具成功不等于 Web 构建或发布成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | 对齐归档身份、可信 main writer 与只读消费者，验证真实 scope | [缓存边界](#ci-install-cache-design) | GitHub cache 与 main full-regression | 静态 key 相同不证明跨 tag 恢复 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | 约束总预算、有限重试及受控子进程清理 | [apt 有界安装](#ci-install-apt-design) | 临时 CI runner 的依赖准备 | Rust/native cache 共享 helper 与进程 fixture 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | 分阶段、分冷暖记录可比较的准备成本与失败样本 | [耗时证据](#ci-install-measurement-design) | 实际 Actions 运行与维护者分析 | 不由 Cargo 指标或本地 fixture 推断发布提速 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| Trace refs: O7-REQ-7a766dd0-5264-4caf-9dfb-60643ca5faf6；[工程遥测](ci-tiered-execution.prd.md#engineering-telemetry) | 以准确身份关联已有数据，区分执行结论、采集覆盖和诊断 | [共享模型与诊断](#engineering-telemetry-design) | CI 维护者、QA；dot/指挥中心只读消费 | 产品适用性：N/A；reason=开发验证数据治理；scope=CI 与本地构建，不承接产品节点监控或合入审批。 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | full 包含适用 required；真实消费者、wrapper 结果和扫描证据可核对 | [执行清单与结果](#ci-real-coverage-design) | CI driver、构建 wrapper、QA；CodeQL 保持独立主责 | 不把 fake 工具、旧运行或文档当成实际执行 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | 完整 TOML 集合、真实日期、范围棘轮和逐项有界续验 | [例外检查](#ci-advisory-baseline-design) | repository_health_engineer 与安全/兼容审读 | 不修改当前有效期、批准列表或依赖范围 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | 普通/重型分离、一次构建、五分片及失败汇总 | [进程调度](#ci-formal-process-design) | CI 构建与原生进程运行器 | 产品 N/A；理由：CI 专业调度优化；范围：普通检查与真实进程验收，不改变玩家行为。 |
| Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；[共享产物中的缓存容量](ci-tiered-execution.prd.md#ci-formal-process) | 单份 raw 准备预算与远端压缩总量分离，可信写后按族回收 | [容量治理](#ci-cache-budget-design) | 正式 producer、缓存管理 helper 与 CI 维护者 | 产品 N/A；理由：已有共享产物要求的缓存资源约束；范围：可重建缓存，不改变必要测试、发布身份或持久世界。 |

## 3. 依赖准备的基线与目标

2026-10-12 审读基线是 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`。本次采用 Release 复用现有安装器的最小改造；真实 hosted 构建、main 缓存写入、跨 tag 恢复及节省时间分别以实际运行记录确认。apt 在 PR #5477 的长挂证据后实施下述有界安装。

| 对象 | 审读基线事实 | 本设计的变化与限制 |
| --- | --- | --- |
| Release Web 工具准备 | [原工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/release-packages.yml)恢复 `~/.cargo/bin/trunk`，未命中时 `cargo install trunk`。 | 恢复预编译归档并无条件调用共享验证安装器，去掉重复源码编译。 |
| 普通 CI 与 main writer | [Rust 工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/rust.yml)已使用归档；main schedule/manual-full 的 `full-regression` 可写入。 | Trunk 复用已有 writer、路径与 key，不为该工具新增预热工作流。 |
| 安装器 | [原安装器](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/scripts/install-ci-trunk.sh)固定 Trunk 0.21.14、官方归档摘要及 Linux x86_64。 | 继续由该脚本集中拥有版本、来源、摘要、平台与失败规则。 |
| apt | PR #5477 head `d4a7ecbebd3d86f396f03f0d31c1ab69d1a8958c` 的 Rust run `42052064452` 中 `viewer-performance-report` 索引更新等待近六小时后取消。 | 使用 helper 约束依赖准备；不将该现象推断为已证实的镜像源或锁故障。 |

<a id="ci-install-trunk-design"></a>
## 4. Release / Testnet Trunk 安装流程

消费者是 [Release `build-web-dist`](../../../.github/workflows/release-packages.yml) 和 [Testnet `build-web-dist`](../../../.github/workflows/testnet-packages.yml)，支持范围是现有 Linux x86_64 runner。源码身份继续由 `package-source-plan.py` 一次解析，job checkout 固定的计划提交并核对 HEAD；工具优化不改变原有源码、BUILDINFO、构建验证或发布时 tag 复核规则，完整边界见[包来源与缓存安全规则](../../engineering/workflow/package-source-security.md)。

1. 在计划源码校验后确认 `scripts/install-ci-trunk.sh` 是非空普通文件且不是符号链接；缺失、为空或为符号链接时明确失败，不能用空安装器 hash 恢复缓存，也不从其他 ref 临时借用脚本。
2. 使用固定提交的 `actions/cache/restore` 恢复同平台归档。该步骤只恢复，不为 Release / Testnet 增加 Trunk 缓存保存；不把缓存命中作为跳过安装器的条件。
3. 调用 `bash scripts/install-ci-trunk.sh "$RUNNER_TEMP/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz" "$RUNNER_TEMP/ci-tools/bin"`。归档不存在时，安装器从固定官方来源下载并校验后落盘；归档已存在时仍校验固定 SHA-256，校验通过后才解压及执行。
4. 安装器检查提取的 binary、精确版本和支持平台，成功后写入 `GITHUB_PATH`，后续原有 Web 构建使用该工具。任何必要安装失败沿 job 失败传播。

缓存损坏或下载摘要不符时在解压、执行前失败；版本不符、网络失败、提取失败和不支持平台均不得继续构建。保留现有安装器的有界下载重试，不引入任意版本、未审核镜像或 `cargo install` 回退。损坏缓存不静默改写成一次成功的冷启动。

<a id="ci-install-cache-design"></a>
## 5. 缓存身份、作用域与恢复

归档路径与普通 CI/main writer 保持一致：`${{ runner.temp }}/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz`。key 复用现有表达式：

```yaml
trunk-archive-${{ runner.os }}-${{ runner.arch }}-${{ env.TRUNK_VERSION }}-${{ hashFiles('scripts/install-ci-trunk.sh') }}
```

安装器来自已核验的计划 source；key 中的版本、平台、架构和安装器内容指纹共同描述本次工具输入。升级脚本或 pin 会产生正常冷启动。缓存路径、归档格式及 cache action 的兼容实现也必须一致；仅对齐 key 字符串不充分。GitHub 的 cache version 包含路径与压缩方式，tag 间不能直接读取彼此缓存，可恢复默认分支范围内的缓存，见 [GitHub 缓存引用](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)。

现有 main `full-regression` 的 schedule/manual-full 保留 Trunk 归档 writer；Release / Testnet 使用只恢复 action，普通 PR 继续受只读 cache token 约束。Rust 依赖按下述生产者与消费者关系复用，不增加宽泛 `restore-keys`、扩大缓存写权限，或把缓存内容当作可执行来源的认证。

ordinary、formal-debug、formal-endpoint 的 trusted workspace writer 与 `full-regression` 仅在 main、workflow SHA 与事件 SHA 一致、selector 证明 source 等于事件 SHA、且事件与 profile 准入时执行。在这些条件下 tested SHA 与事件 SHA 相同，writer checkout 可直接引用 `github.sha`；candidate 必须继续引用选定的 `tested_sha`，包括 main dispatch 指定另一候选提交的场景。合同测试联合验证 selector 和 job 条件，只允许这四个 writer 首次 checkout 的两种等价 ref 表达；其余 checkout 设置、后续步骤内容与顺序，以及既有缓存保存差异继续逐项核对。复用步骤的 alias 不得掩盖缺失、重复、未知引用或执行差异。

普通原生 Rust CI 与 Release / Mainnet 的 runtime core、support、sync、soak 读取 `ci-shared-native-trusted-v4`。唯一依赖 producer 是独立 `rust-cache.yml` 的 `shared-native-cache`，仅接受 main 的四小时 schedule 或 `run_mode=full` 手动触发；不接受 source 覆盖，也不从候选 workflow、产物或分支取得代码。checkout、workflow SHA、事件 SHA、精确 job 和 REST run 身份须一致。Release / Mainnet gate 仅有只读 cache token，Rust `full-regression` 也只读这份依赖缓存；其原有 Trunk、wasm-bindgen 和 npm 工具缓存合同保留。GitHub cache 是不可变归档，同 key 的并行保存不会合并各自 target，因此由 producer 在同一根 target 内形成依赖并集，全部编译成功后保存一次，禁止失败保存。

新命名空间使旧的不完整快照不能成为精确命中；后续改变生产覆盖时必须更新族版本，不使用每个 source SHA 或 run ID 给大依赖包换代。

独立 producer 的 `cache-native` 入口通过 `ci-shared-native-cache.py` 编译自足的闭合并集：原 `full-extended` 的 core 扩展、Viewer、适用 support features、LLM fixture 和 production mock 所需构建；旧 ordinary 的四个变体；纯 `wasmtime --lib --bins --no-run`、默认 features 的 chain runtime，以及 sync 的 `hydrate_builtin_wasm` 和带 `sync-builtin-wasm-identity` 的 host helper。所有 `cargo test` 都带 `--no-run`，不运行 fixture、mock 断言或测试进程。support features 与现有 driver 使用同一包清单和 feature 解析，保留两个 WASM 构建工具的 `OASIS7_WASM_BUILD_STD=0` 环境。

v4 另外从可信 checkout 的完整 fast target 清单补齐精确 `default_features`、features 和 target；同包且 feature 策略相同的 target 可合并一次 Cargo 调用，不能用更宽 feature 构建替代窄组合。旧 v3 不保证这些依赖 context，因此不能作为 v4 的精确命中；容量维护仅在完整 v4 后继、相同平台/架构/环境 hash 与 cache version、两次库存稳定身份及活动保护条件成立时退役 v3。下载的测试 binary 和其他 runner 的通过结果不算本地 target 预热，缺少单位时 reader 正常补编；任一构建失败使预热 job 失败且不保存。

共享缓存预热与 Rust 验证分属两条 workflow，使用独立并发组且不取消进行中的可信预热。完整回归保留原 26 组、full-extended、210 分钟测试预算、显式 full gate 和整条 Rust run 成功条件，不再在测试后补编译或保存 Rust 依赖。Rust 的 workspace 缓存维护只等待 ordinary/formal producer 保存，不再等待 full 测试收尾；跨 workflow 的正在运行任务仍纳入全仓活动保护，新缓存 writer 不获得删除权限。v4 精确命中已经完整保存的不可变归档时，缓存任务跳过重复编译；前缀命中、未命中或恢复未执行仍编译全部上下文。该条件只属于缓存 producer，不能跳过任何 Rust 验证。预热不生产验证凭证，不给 source 代签测试，也不进入 required/full 的依赖图；失败、取消和容量拒绝分别可见，消费者仍走正确冷路径。四小时内完成及普通 PR 的实际冷暖收益以 hosted 样本验收，单次本地合同或调度优化不证明十分钟/P95 达标。

各 reader 继续原测试、sync 和 soak；没有以预热代替验收。builtin 模块的独立 workspace / Docker target 不属于根 target 缓存，仍走 canonical 构建，不为扩大归档改写其目标目录。

生产者与消费者在缓存步骤前对齐实际已安装的 Rust/WASM 工具链、Cargo 配置及参与 key 的环境值；`shared-key` 相同本身不足以消除工具链或环境 hash 的差异。共享 native 族固定 stable 与 builtin nightly，保留显式 `CARGO CC CFLAGS CXX CMAKE RUST OASIS7_WASM` 指纹。准备 helper 只在 GitHub-hosted 临时 Linux runner 上工作，验证所需的精确工具链与当前 active 工具链后，移除无关预装工具链；缺失、非固定版本或卸载失败均失败，不关闭环境 hash。

Release / Mainnet native gate 在 job 范围对齐 Cargo 重试、registry 协议、debug profile 和四项 WASM 环境。NewAPI 两个入口仍保持各自对齐的身份。

registry 下载内容与 target 编译产物分别核对复用价值。当前固定的 `rust-cache` 将两者一并归档；合并 native 族减少重复库存，但各小 gate 会恢复更大的归档，恢复/解压成本与编译收益必须由实际 hosted 数据分别评估。

`workspace-support-legacy` 在可信计划声明版本化分区时，以 custody / other 两路 matrix 执行完整原 profile 命令。custody worker 的 native 单测、role/WASM 正反编译、真实 World barrier、fixture、TypeScript 和平台测试仍由原 verifier 完整执行；其他支持包不再与其串联。full 在分区前扩展候选完整 profile 库存，缺失、重复、未知分区及任何失败均阻断。本地未指定 `--support-partition` 时，校验完整计划后执行全部选中 profile；显式 custody / other 仍按计划核对并收窄。旧 BASE 保留 all，矩阵 `fail-fast: false`、最多两路，最终 required/full gate 经稳定 support 汇总等待整个选中矩阵的真实终态。

Custody 的可信分阶段执行将 native、wasm-production、wasm-approval 放到三个独立产物 runner，并单列原生合同 runner：完整 core/worker 单测与 native fixture 构建并行，仍是最终 support 汇总的必要条件；它不生产 Node 消费者所需的二进制，因此消费者不等待该合同 job。native 产物承接 Launcher 回归、真实 World commit barrier、contract 和 context-body fixtures；production 构建四个生产角色、Wrangler dry-run 和 native harness；approval 承接 core 与全部有效/无效角色及 harness 编译证明、产生 approval fixture 与 approval harness，并执行可信计划根据真实 manifest 登记的完整 Worker WASM profiles，包括适用的 execution-signer-harness 正反组合。编译证明归到同一 approval runner 复用 dev 依赖；完整十六项证明及动态 profile 都实际执行，负向检查仍核对原错误文本。四路并集与本地完整入口保持相等，manifest 的 build_contract 只声明该 producer 实际完成的证明。`--stage native` 保留原完整语义，拆分入口显式使用 `native-contracts` 和 `native-fixtures`；不能把单独 fixture 成功宣称为原生合同通过。每个 WASM producer 内保持原角色构建锁与顺序，不在共享 build 目录并发。只有计划明确 `test_policy_changed=false` 才启用分阶段执行；测试政策变化或字段不确定时保留 legacy 路径及原有 `--all-targets` 扩展。

固定 `worker-build 0.7.5` 由独立工具目录准备。缓存 key 精确绑定 OS、架构、Rust toolchain、工具版本、安装器和进程监督实现，不使用兼容前缀恢复。PR producer 只恢复；命中后核对普通可执行文件、路径安全和精确版本，缺失或错误命中必须失败，不能自动重装掩盖损坏。未命中时有界执行一次固定版本的 locked Cargo 安装，完成版本校验及自有后代清理后才原子发布目录。缓存唯一 writer 位于 `rust-cache.yml` 的 `worker-build-cache`，只允许 workflow/source/checkout 一致的 main schedule/manual-full；完整仓库库存及既有总预算允许时才保存，容量或观察失败仅跳过保存。缓存不代替实际角色构建、编译证明或平台测试，未 seed 时仍执行真实冷路径；真实跨 runner 命中与耗时由托管记录另验。

产物绑定 source/tested/policy、run 与 producer attempt、平台及工具链、完整测试文件清单和每文件摘要/权限；consumer 仅接受本 run 明确成功的 producer 输出，核验全部文件后恢复到自己的路径，并重建 native/world 二进制指针。Node consumer 不重复 Cargo 构建或恢复 native target 缓存。四个分片按确定性文件清单执行全部 `tests/*.test.mjs`，每个 runner 内文件串行，测试内部操作、断言、超时与 freshness 合同保持。TestSession 按完整场景拆文件，共享 fixture 由每文件独立实例持有可变开关。TypeScript 检查保留一次必要执行。

`workspace-support` 保持稳定汇总 ID：无新能力时执行 legacy 完整 verifier；新能力时独立 other 与 custody 图互不等待，选中的原生合同、全部 producer、分片与完整结果库存必须成功，缺失、重复、取消、错身份及非零均失败。本地 `verify.sh --platform` 和 `npm test` 仍提供完整入口，默认文件进程并发上限为 2。原生崩溃、异常退出和测试失败按真实退出码阻断，不自动重跑。Node 分片使用已测完整文件耗时作排序权重；未测新文件保留确定性的文件大小估计，全部发现的文件恰好分配一次。权重仅决定放在哪个 runner，不决定是否执行；历史 trace 的调度重放与新候选实测分别记录。

`workspace-support` 的只读容量采样在 cache 恢复前后、支持包 profile 边界、custody WASM/native/platform 边界及 job 退出时输出 `SUPPORT_CAPACITY` JSON 行。记录实际 source/tested/policy/range base、runner image、文件系统可用字节与 inode，以及固定目录的 allocated/apparent bytes 和 inode 数；精确 cache key 仍以同一 job 的 restore 日志为准。目录观察可能重叠，不能相加；每个目录内 hardlink 只计一次，不跟随目录符号链接，不读取文件内容。单次采样最多 25 秒，缺失、错误、超时明确标记为未知，不能冒充零占用。证据直接留在 job 日志，避免磁盘耗尽时依赖新建报告文件。采样失败不改变测试退出码，也不清理目录、调整缓存或 runner、减少既有验证；实际容量优化需另有测量依据。

缓存可能被驱逐，保存后的 Cargo 清理也可能丢弃不活跃依赖；缺少产物仍由原命令补齐，不能承诺全部 fresh。不同 target/profile 可以共存；Web WASM、ECS `ecs` profile 与 Formal workspace 保留专用缓存族及完整构建上下文，Web 依赖刷新独立于产品发布和 native 预热的成功状态。

`release-packages-web-wasm-trusted-v3` 的唯一 writer 是现有 `rust-cache.yml` 的 `web-wasm-cache`；Rust CI 的 `viewer-js-required` / `viewer-performance-report`、Release `build-web-dist`、Release Web gate、Testnet / Mainnet / ECS Web 都只读。Release 的产品构建、全部 gates、artifact 和发布条件保持，历史计划源码缺少工具链 helper 时仍走原冷路径。Web cache 前仅保留固定 stable，ECS Web 在 job 范围补齐相同 Cargo 环境。

Web 缓存生产者承接 Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7。admission 的 `release-web` family 只注册新的 workflow/job 元组，旧 Release 身份不能上传。writer 迁移须使 workflow、job、admission 身份、写入权限和预算转发形成完整且互斥的一组，双 writer、零 writer、未知身份或半迁移均失败；必要兼容合同先进入可信 BASE，再原子切换生产接线，不改变 selector 或可信 Python 执行器来避开旧合同。首次 seed 前核对旧版 Release Web 写入任务全部终结，避免跨 revision 的在途 writer 重叠。

producer 沿用四小时 schedule 或 `run_mode=full` 手动事件，只接受当前 main，checkout、事件、workflow 和 REST run 身份一致；不接受 source/tag/version 覆盖。它与 native job 独立，只有 contents/actions read 权限和本 job 的 cache write scope，60 分钟内完成或失败。使用固定 stable、WASM target 和默认 Web 环境指纹，不安装 builtin nightly。完整闭包保留三个独立上下文：默认 features 的 browser WASM、禁用默认 features 且启用 `webgl2_runtime` 的 pixel WASM，以及固定 Trunk 的 launcher `--release` 构建。复用软件安全 Viewer 脚本，并显式强制重新检查两个 WASM 构建，已有 dist 不能抵扣依赖闭包；Trunk archive 只读且始终经过既有安装器摘要/版本校验。不合并 Cargo 调用造成 feature union，不改 target 或 profile。只有明确的 exact hit 可以省略缓存生产；其他恢复结果均完成整个闭包，任一构建失败不保存。最终归档仍受原 step-local Node admission、20 GB 总预算和活动保护约束，不增加缓存族或删除权限，不进入 Rust required/full 或 Release gate 的依赖图。

两个 Viewer reader 的迁移合同只接受原 native-v4 配置，或同一 Web-v3、stable-only 工具链及默认环境指纹的完整配置。当前两个 reader 使用后者：只安装固定 stable 和 `wasm32-unknown-unknown`，缓存前只保留该工具链，环境前缀为 `CARGO CC CFLAGS CXX CMAKE RUST`，明确禁止保存。完整缓存身份与 producer 对齐，原 npm、wasm-bindgen、两个独立 WASM 构建、ABI、安全浏览器及性能验证保持；其他 native reader 不改变。真实 hosted 冷暖收益仍待验收。真实 seed 以上传完成为准，并在同一 source/tested/policy 候选上核对恢复 key、编译复用、准备成本和原验证结果；老归档前缀恢复可能清理过期编译产物，不能仅用下载成功认定暖缓存。同一 run 的 reader 重跑只用于 job 冷暖对照，不能把合入与 seed 等待时间计为新的普通 PR 端到端样本。

ECS 原生独立迁移到 `oasis7-ecs-native-linux-ecs-v2`，仍只允许既有 main 可信 lane 成功保存。计划源码缺少新工具链 helper 的历史包跳过这一 Rust 缓存路径，继续原冷构建；不能把缺失 helper 的空指纹作为新族写入。

Linux 原生发行包仍按工具链、target 和 profile 共享 `oasis7-package-native-…-trusted-v2`；Testnet 的 Linux-only packaging-profile main workflow dispatch 仅在计划源码和 workflow revision 均等于 event SHA 时选择独立的 write-scoped 原生作业。其他分支、tag、旧 SHA 或 workflow/source 不同版本均只读，两条路径互斥，不重复编译。

非 Linux 原生包仅在 OS、target 和 profile 匹配时读取对应 Release 原生包缓存；Storage / Compile 的独立边界不扩展。core/support 的文档 gate 使用计划提交父子范围和完整历史；必要命令通过 `tee` 输出日志时启用 Bash `pipefail`，上游失败必须使 gate 失败。

缓存是可回收的加速数据，没有发布状态、事务或持久化权威。main 尚未保存该身份、缓存被回收或 reader 不可达时，正确的冷下载仍可完成工具准备。恢复服务异常须保留实际 action 结果；若 action 允许继续且归档不存在，安装器仍执行同一校验冷路径，不用忽略必要安装失败来维持 job 绿色。

跨 tag 复用的验证必须关联一个可确认的 main 保存记录及两个不同 tag 的实际 reader，记录 run/job、event/ref、请求和实际匹配 key、平台与 cache version 兼容条件、恢复及摘要验证结果。仅同 tag rerun 只能证明同 tag 恢复；未取得上述记录时保留“配置已接通、真实跨 tag 恢复待观察”。沿用自然发生且已授权的 main/full 与 Release 执行积累数据，不为本项新建发布事件或 required check。

<a id="ci-cache-budget-design"></a>
### 5.1 Workspace 缓存容量、滚动保留与已迁移命名空间的退役

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；承接[共享产物要求](ci-tiered-execution.prd.md#ci-formal-process)中的[正式缓存容量与保留](ci-tiered-execution.prd.md#ci-formal-cache-budget)。

各 workspace writer 共用完整库存、实时额度与已核验配置预算的容量准入，以及预压缩归档和外包装上传上界计算。原始 staging、单份上传、两代常驻和临时第三代写入分别约束，避免把两代目标同时用作第三代写入门槛：

| 缓存族 | 原始逻辑 staging 上限 | 单份上传上界 | 两代常驻目标 | 临时写入族上限 |
| --- | --- | --- | --- | --- |
| ordinary | 6 GiB | 1.25 GiB | 2.5 GiB | 3.75 GiB |
| Formal debug | 6 GiB | 768 MiB | 1.5 GiB | 2.25 GiB |
| Formal endpoint | 2 GiB | 256 MiB | 512 MiB | 768 MiB |
| 历史 Formal v3 | 8 GiB | 1 GiB | 2 GiB | 3 GiB |
| ECS | 6 GiB | 1 GiB | 2 GiB | 3 GiB |

这些上限不是预留额度，各族同时达到最大值再增加一份上传仍须满足有效仓库额度。完整库存包含所有 ref、旧格式、共享原生和 ECS 依赖及其他缓存，实际总量加保守上传上界必须不超过有效仓库额度；不足时仍跳过保存。ordinary 的 6 GiB staging 限制所选完整 library 单元，完整选集超限时按下述有界选择缩减；表中上传上界是容量政策，不是实际压缩体积。Formal 两组件合计沿用原 2 GiB 常驻、3 GiB 临时写入目标；迁移期历史 v3 独立治理，其占用仍计入全仓预算，不以一半组件缓存替代完整历史恢复锚点。

每个族的常驻目标为单份上界的两倍，正常轮换的临时写入上限为三倍；空间充足时按两代目标回收，容量压力下仅减少有稳定完整锚点的额外副本。

仅当新 ECS key 的兼容前缀在库存中尚不存在时，允许 4 GiB 的有界兼容迁移写入峰值，单份上传仍不超过 1 GiB，全仓准入仍使用完整实存总量。新兼容组一旦存在，后续写入恢复 3 GiB 正常轮换上限；保存确认后的通常维护回收未保护的旧兼容组。

[缓存维护 helper](../../../scripts/ci-cache-maintenance.py) 使用 GitHub cache API 的 `size_in_bytes`，按缓存 ID 校验唯一性，以缓存族和 ref 汇总条目数与压缩字节；相同 key 的不同 cache version 或 ref 仍是独立条目。兼容组由去除 source SHA 的完整 key 前缀与 cache version 共同确定。

GitHub 的默认仓库额度与驱逐规则见[官方缓存引用](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy)，它不替代本仓库预算，也不保证旧缓存能保留到下一次复用。

workspace 受管范围限于 `refs/heads/main`：当前格式为 `{ordinary-workspace-v1,formal-debug-workspace-v1,formal-endpoint-workspace-v1,formal-workspace-v3,ecs-workspace-v2}-{64hex compatibility}-{40hex source SHA}`。Formal 历史 `formal-workspace-v2-ubuntu24-{arch}-{rusttoolchain}-{wasmtoolchain}-{64hex compatibility}-{40hex source SHA}` 与 ECS v1 是明确退役的完整格式，仍计入各自族预算。跨兼容组退役使用该族已确认 current key，或下述压力下的不可变源码祖先证据；没有相应证据时只回收完整兼容组内的额外代。

Formal v3 为单文件归档，绝不把旧 v2 目录当作新 payload。identity 包含平台、完整工具链配置以及归档和容量 helper 的内容。ordinary/Formal 改用 `workspace-units.tar.zst`，格式变化产生新的兼容指纹及 actions/cache version，不能回退恢复旧 gzip 身份；ECS v2 继续使用原 gzip 文件。

族预算覆盖所有格式、兼容指纹与 source SHA。source SHA 继续进入不可变精确 key；兼容前缀保持平台、工具链、Cargo 输入、features/profile 与构建脚本边界，不扩大恢复范围。通常维护显式提供已确认的精确 current key；补偿维护使用完整格式白名单，不接受任意前缀。

ordinary/Formal 的 compact 归档使用单线程 zstd level 3、8 MiB window 和 checksum；归档内仍只有 manifest 与扁平 regular-file payload，不接受 tar link 或扩展头。manifest 中有版本的 aliases 映射仅去重 staging 中同 device/inode 的文件；别名只能指向排序靠前的非别名文件，且长度、摘要、权限和纳秒时间戳完全相同。

先校验全部原始逻辑字节、路径和别名关系，解包后仅在受控目录内为已验证 canonical 文件创建硬链接，再由现有 Cargo 单位、摘要和源码校验决定是否恢复。

zstd decoder 使用 64 MiB 窗口/内存参数上限（非进程 RSS 承诺），压缩输入、物理 payload、逻辑展开和 tar 终止均有界；拒绝附加帧、截断、校验失败、路径穿越及多余成员。共享 reader 保留 gzip 支持供 ECS 使用。归档与外层 actions/cache 上传包装分别量测，不能直接把压缩文件大小当上传上界。

额外的退役白名单只接受完整 `v0-rust-{family}-Linux-x64-{8hex environment}-{8hex dependencies}` 与 main ref：旧 `ci-shared-native-trusted-v3`、`ci-full-regression-trusted-v2` 及四个 `release-packages-release-gate-{runtime-core,runtime-support,runtime-sync,soak}-trusted-v2` 由 `ci-shared-native-trusted-v4` 替代；旧 Web `release-packages-web-wasm-trusted-v2` 由同名 v3 替代；ECS `oasis7-ecs-native-linux-{packaging,ecs}-v1` 由 `oasis7-ecs-native-linux-ecs-v2` 替代。只有对应替代族存在非零、合法 main 条目且两次完整清点中身份不变，才允许退役该旧命名空间。该规则不删除替代族；替代族的同环境依赖冗余由下面的独立规则处理。近似命名、其他 ref 和其他族只统计，不按最新时间推断整个环境 hash 可删。

依赖冗余白名单包括 `ci-shared-native-trusted-v4`、`oasis7-ecs-native-linux-ecs-v2`、`release-packages-web-wasm-trusted-v3`、`oasis7-package-native-Linux-native-packaging-trusted-v2`、`release-packages-package-native-Windows-native-trusted-v2` 和 `release-packages-package-native-macOS-x86_64-apple-darwin-trusted-v2`。解析器为每个已知命名空间枚举实际宿主平台/架构（Linux-x64、Windows_NT-x64、Darwin-arm64/x64），不将 macOS 的 x86_64 编译目标误作宿主架构；未知组合不受管。

按去掉最后一个 8hex 依赖 hash 后的完整 restore 前缀与 cache version 分组，与固定 Swatinem action 已有的回退边界一致，不扩大消费者恢复范围。每组保留唯一最新非零条目及活动保护集，只回收更旧的依赖快照；不跨环境、平台或 archive version 回收。

最新创建只是该兼容组的可用恢复锚点，不代表最新 source SHA 或依赖超集；回滚与依赖变更仍由 Cargo 正常验证、补编。此规则同时用于启动和通常维护，在新 workspace 首次保存前即可回收冗余依赖，不以一个尚不存在的 workspace current key 为前提。

共享原生与 ECS 依赖 writer 通过 `ci-rust-cache-admission.py` 在最终归档边界准入。原固定 `Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0` 保留指纹、恢复及 post 清理；仅该 action 的 PATH 使用 runner 临时目录中的 tar 适配器，不修改全 job PATH 或 action 代码。适配器接受固定 Linux cache-create 参数，调用真实 `/usr/bin/tar`，成功生成 `cache.tzst` 或 gzip fallback `cache.tgz` 后检查普通文件身份及真实字节。

固定上游 [GitHub cache 实现](https://github.com/Swatinem/rust-cache/blob/d1527c8f67ef9af357290475d4af4b0fab8f12e0/dist/cache-Dl81DHPs.js) 在 `createTar` 成功后才预留和上传；适配器拒绝会在远端预留前停止该次保存，由原 action 报告 warning 并清理本地归档。未知创建参数、非临时目录、缺失或变化的归档均拒绝；版本查询、列表与恢复命令透传。

更新 action pin 时必须复核此调用及失败传播合同。

Release 的跨平台 writer 使用同一 helper 的归档准入规则，通过仅作用于缓存 action 及其 post 的 Node preload 在固定 provider 的最终归档 stat 边界检查。固定 action 的 restore/save 入口及 provider 内容摘要必须吻合；只有已审读的 save 调用顺序（createTar 完成、同步读取归档大小、远端 reserve/upload）可进入检查。restore 不进行写入准入，未知入口、bundle 或已有未知 NODE_OPTIONS 不得静默绕过。

归档必须位于 runner 临时私有目录且为完整普通文件；同步调用固定 helper 的绝对路径，不经 shell，按同一不可变归档复用一次准入决定并在后续 stat 重查身份。

失败只使可选 cache action 停止，构建走原冷路径；不修改系统 tar、上游 bundle、key 或恢复范围。Windows 的绝对 Git tar 和 macOS gtar 均经过该共同边界。

上传前验证当前可信 main 的仓库、workflow、source、job、事件及 REST run 身份，然后复用完整库存与额度规则；最终归档已经包含 action 的 tar/压缩包装，不再以比例推测，也不重复加一层不存在的包装。清点后重新确认文件身份和长度，只有实际库存加该归档不超过有效预算才允许 tar 调用返回成功。

Rust cache 的 `shared-native-cache` 与 ECS `native-trusted` 仅需 `actions: read`，删除权限仍留在既有维护 job；安装适配器失败则跳过该可选缓存 action，继续冷构建。

Release writer 另外核对 source plan、实际 checkout 与 event SHA 一致，且仅成功构建保存。Release 保留既有 cache token scope；tag、历史 source 和失败构建由这些 action 的保存条件与准入拒绝写入，不将此行为等同于只读 token 隔离；同一 Linux packaging 命名空间的 Testnet 可信 Linux-only/packaging writer 也接入原 Linux tar 适配器，保留原候选只读分流；其他依赖 writer、工具缓存和跨 job 写入没有原子互斥，库存是此次上传前观察值；不为此串行化所有构建。

报告区分实际压缩字节、观察总量、投影总量、实时/配置预算及拒绝原因，不把允许上传当成保存已确认。

Rust 启动维护 `cache-bootstrap-maintenance` 在选中构建的可信 main 完整 profile 使用 `--recover --apply`；旧 BASE 回退保留原 main push 条件，新 `main-fast` 不启动写维护。该步骤无需本轮保存结果；trusted producer 等待其结束，但显式允许维护失败或跳过后继续构建。

Rust 结束维护直接等待普通、Formal debug 与 Formal endpoint 的可信 producer，只要一个成功即可进入，不再等待完整测试、合并分包或独立原生依赖预热，也不要求其保存确认；各族仅传入本轮成功且 API 确认的精确 current key。维护的完整库存和跨 workflow 活动保护仍适用，独立依赖 writer 在上传前检查最终归档与库存，其后续 post 保存不能被当成已预留空间。ECS 结束维护只要求可信 native producer 成功，不依赖 Web。

Release 在可信 main manual dispatch 且计划 source 等于 event/workflow SHA 时增加启动与结束恢复；启动由 plan 后进入，writer 显式允许维护失败或跳过后继续原构建，结束等待 Web/native writer（含 post）完成并使用空 current keys。维护不成为发布成功门槛，tag 或非当前 source 没有新增删除权限。

结束维护同样使用 `--recover --apply`，仅为成功且已确认的 producer 附加 `--current-key`，没有新 key 时仍执行补偿回收。

所有入口保持可信 main 的 source/workflow/event 条件、取消保护、`contents: read`、`actions: write`、`cache-mode: none` 和固定 `ci-cache-maintenance-main` 并发组，不恢复或执行 payload，不改变 required gate 的选择与失败传播。当前运行仍经 REST 验证仓库、source、workflow 与事件，以 run 的 `created_at` 形成活动保护起点。

补偿模式 `--recover` 可接收零个或多个已确认 current key，不与旧 `--bootstrap` 并用。无显式 current key 的当前 workspace 格式按完整兼容前缀与 cache version 独立选择唯一最新非零恢复锚点，空间充足时保留两代；无完整条目、同 key 多 version、非零创建时间并列的整个组保留。明确退役的 Formal v2、ECS v1 与显式白名单依赖冗余沿用每组至少一份锚点的规则。旧 `--bootstrap` 继续只处理原有退役格式与依赖冗余白名单。

存在已确认 current key 的族保留原跨兼容退役规则；current 缺失、空或跨 version 歧义时整轮保守跳过，不能静默改用另一个 key。

恢复锚点和删除候选在两次完整清点中均须保持不可变身份；锚点变化使相应组停止删除，第二次计划不能扩大第一次集合。

保存 action 成功或构建后的精确命中只决定是否核验 exact current；该 key 在同 ref 的完整库存中须对应唯一且非零的合法条目。保存被拒绝或未确认时不能传入它作为 current，但不阻止使用既有锚点进行同兼容组补偿维护。

依赖恢复锚点不能同时成为命名空间退役的删除对象；无 current key 的补偿模式也不能按时间将其他 workspace 兼容组整体淘汰。只有下面的不可变源码祖先证据可以补足跨组替代依据。

跨兼容退休仅在全仓轮换空间或该族常驻压力下启用，限定当前固定 Ubuntu24/Linux X64 writer 的 ordinary v1、Formal v3、ECS v2；同族、同格式、同 cache version 的无歧义非零组锚点才可比较。旧 cache source 必须是替代 source 的严格祖先，替代 source 必须是当前已验证 main run SHA 的祖先或自身；相同 source 的不同配置、不相关分支及未知格式均保留。

使用反向 `GET /compare/{descendant}...{ancestor}?per_page=1`，核对 `base_commit.sha`、`merge_base_commit.sha`、status、ahead/behind/total counts 及空 commits 列表；只接受明确的 behind 关系，同 SHA 只作身份相等处理。比较只使用完整 40hex SHA，GET 路径严格限定，沿用 60 请求/120 秒预算；按 SHA pair 复用不可变证据，并将祖先查询限制为每轮最多 20 次，为库存和删除留出预算。

该局部比较额度用尽或历史对象 404 时只保留未获证明的组；权限、限流、无效响应或全局预算耗尽仍使本轮保守关闭。可用证据不能证明替代时保留对应组，不扩大恢复范围，也不将不同配置 payload 视为可互换。

保留未被后继替代的锚点，已确认 current 优先，任何替代锚点不得同时成为其他规则的删除对象。两轮计划必须保持相同 candidate-to-anchor 关系及双方不可变身份。

保护集包括当前精确条目、未被可信源码关系替代的补偿组完整锚点、零字节条目，以及保护起点之后新建或被访问的全部受管条目。起点取当前可信 run 与完整活跃 run 清单中最早的 `created_at`。默认还保留当前兼容组的第二代；只有明确空间压力时才可回收这个非活动历史副本。较旧 SHA 的慢构建不能删除清点中可见的较新运行结果，已观察到的本轮访问与未完成上传仍受保护。

补偿维护复用 writer 的完整 all-ref 库存、实时额度与已核验配置预算规则，在每轮清点重新读取有效额度。共享常量 `DEPENDENCY_ROTATION_HEADROOM_BYTES = 2.5 GiB` 定义轮换空间目标，常驻目标为有效额度减去该数值。

先生成通常的多余代/已确认兼容退役计划，再按剩余全仓缺口或各族 `max_bytes` 常驻缺口，从旧到新选择对应组的额外 workspace 副本；全仓已满足目标但某族仍超额时继续处理该族，且仅删除达到目标所需的条目，绝不删除未被可信后继替代的唯一完整锚点、current 或活动/歧义保护条目。不同兼容组仅可在上述源码关系、格式及 version 边界内共享替代锚点。

报告保存目标、预计/实际剩余容量和未满足缺口；若容量或身份无法可靠读取则不执行补偿删除。所有族两代同时常驻可能使依赖无法轮换，因此两代是优先目标而不是不可收缩的保留权利。

可信 ordinary/Formal writer 完成身份检查后，对 schedule 和 manual-full 上传额外要求 `observed_repository_bytes + upload_bound_bytes + 2.5 GiB <= effective_repository_budget_bytes`；push 仍使用原准入式。

该保留策略不改变依赖 writer 的实际归档准入，不增加权限或锁，不保证后续归档大小或阻止其他 run 写入；不满足时只跳过 workspace 保存。活动保护可能使一次补偿维护无法达到目标，后续可信启动与结束路径会再次尝试。

先完整读取分页并核对缓存 ID、key、version、ref、字节及时间字段，再生成删除计划；执行前再次完整清点缓存和活跃 runs，重算保护集和计划。只有两次计划都选中、且 ID、key、ref、version、创建时间和字节数保持相同的条目才能删除，第二次计划只能缩小首次删除集合。

当前精确条目的不可变身份变化使整轮跳过；替代项变化或消失使对应退役规则停止。后页失败、重复或冲突数据、分页未结束、字段不合法均不得把部分结果用于删除。

活跃运行查询只对 total_count 漂移、有效列表超出原计数及分页提前结束做最多三次完整状态清点；每次从第一个 status、第一页重新读取并丢弃失败尝试的部分结果。身份冲突、字段错误、未知/截断总量、权限、限流和请求预算耗尽仍立即保守失败；重试共享同一客户端的 60 请求/120 秒预算，不替代删除前所需的两个成功清点。

最终不完整则零删除。删除按准确 cache ID 从创建时间较旧的条目开始，404 视为该条目已不存在，其他删除错误停止并报告已完成部分；不无限重试，不覆盖必要构建结果。

清理后的容量须由再次查询得到；无法回读时仅报告已确认删除的条目和未知剩余量。

两次清点不构成事务锁或缓存租约，之后的访问和平台驱逐仍可能发生；消费者继续依赖正确冷路径恢复。

存储报告与本地 raw/去重体积、下载传输量、实际 Cargo fresh/重编数分列，不从命中次数反推省下的存储或运行时间。旧的未受管缓存仍由平台驱逐或单独核实后处理，本策略不承诺全仓即刻低于默认额度。

<a id="ci-cache-budget-validation"></a>
[容量治理回归](../../../scripts/ci-cache-maintenance.test.py)覆盖保存未确认后的补偿、多页 API 完整性重读、同兼容组两代/压力下一代、Release 平台/架构分组、严格源码祖先退休、current 与稳定恢复锚点、族/ref/version 归属、活动访问保护、两次计划交集、身份变化、歧义和未知项保留、后页失败零删除、部分删除及幂等；[容量准入回归](../../../scripts/ci-cache-policy.test.py)回放高压库存、workspace 和依赖上传先后以及 full 的轮换空间。[workflow 接线回归](../../../scripts/ci-workflow.test.py)核对无新保存仍执行可信补偿维护、候选无删除权限、固定维护并发及必要 CI 结果不变。[发布缓存回归](../../../scripts/package-workflow-cache-reuse-contract.test.sh)核对实际工具链/环境 hash 与 writer/reader 对应关系。真实 hosted 保存、删除后库存和冷热构建收益分别由实际运行记录证明。

<a id="ci-install-apt-design"></a>
## 6. apt 的有界安装

[失败 job](https://github.com/nil-studio/oasis7/actions/runs/42052064452/job/127331913687) 在 Ubuntu 24.04 image `20261025.353.1` 上于 2026-11-05 15:12:46.213Z 开始 update，15:13:23.792Z 后没有 apt 输出，21:12:51.304Z 取消，证明索引更新可占满 job 时间。`scripts/ci-apt-install.py` 分开管理 update 与 install；Rust 和 native cache workflow 的全部 apt 消费者使用 `sudo -n /usr/bin/python3 scripts/ci-apt-install.py --budget-seconds 600 -- <packages>`，保留原软件包、条件及失败传播。lsof 已存在时仍短路，不启动 helper。每步明确 `shell: bash` 与 `timeout-minutes: 11`，外层超时作为异常兜底。

600 秒是上述两个 workflow 的可调整调用参数，不是仓库全局 SLO。helper 用单调时钟约束整个调用，命令执行、退避、重试、终止宽限及等待清理都计入总预算；总预算到期或取消后失败，不进入依赖测试。记录阶段、尝试次数、耗时与退出/清理结果，保留有限诊断输出。

关闭 apt 内部重试并限制网络及锁等待。只有 update 中明确的瞬态网络错误可有限重试，最多三次尝试；未知、混合永久错误、权限、配置、签名或完整性失败直接失败。install 只执行一次，即使发生网络错误也不重跑，避免 dpkg 修改后状态不明时盲目恢复。

临时 GitHub Ubuntu runner 的已知 `/etc/apt/apt-mirrors.txt` 使用 Azure HTTP、Ubuntu archive HTTPS、Ubuntu security HTTPS 三条带优先级的来源；这三个 Ubuntu 域可能同时发生解析失败，调换彼此顺序不足以恢复。依赖准备对精确识别的列表增加独立域的 `https://mirrors.edge.kernel.org/ubuntu/` 为首选，原三个来源作为后备。该端点由 [Linux Kernel Archives](https://mirrors.edge.kernel.org/) 提供，Kernel.org 在 [Ubuntu 官方 archive mirror 名单](https://launchpad.net/ubuntu/+archivemirrors) 中登记；镜像只提供传输，信任仍来自原 Ubuntu archive keyring、签名和文件摘要。保留 noble、updates、backports、security 套件及既有组件、架构与 By-Hash 行为，不新增密钥，不改 Signed-By、其他 APT 来源、DNS/IP 或验签要求，也不接受不安全/过期元数据来换取安装成功。

只接受完整原 runner 列表或本 helper 既有的 Canonical HTTPS 优先列表；未知或自定义内容原样保留。缺失文件保留原安装路径，symlink、非普通文件及写入错误失败。原子替换保留文件权限和所有者，当前目标列表重复准备不再写入。准备计入当前调用预算，未完成或预算耗尽不得开始 update；配置完成不代表依赖安装成功。镜像可用性及包的真实验签/安装必须由 hosted 运行确认，不能用外部目录浏览或诊断探测代替。

native 安装在同一 helper 内复用上述准备；Viewer 安全浏览器和 Deck 安全浏览器在原固定版本 `--with-deps` 命令之前调用同一 helper 的 `--prepare-sources-only` 入口。该入口只准备来源，不调用 apt/dpkg，也不宣称浏览器或其依赖就绪；准备失败阻断后续命令。浏览器原有依赖安装与退出码仍完整保留，Viewer 安装 step 设 11 分钟上限，Deck 保留既有 10 分钟上限；实际网络恢复须由新的 hosted 运行确认，不把来源排序或本地 fixture 当作恢复证据。

helper 以具备清理权限的独立进程管理受控子进程，超时和取消先终止、必要时强制终止并等待回收。父进程先退出、忽略终止信号或脱离原进程组都不能被当作清理完成；不得删除锁文件、终止无关进程或恢复不明的 dpkg 状态。清理无法完成时明确失败，交由临时 runner 销毁，不报告安装成功。

#### 托管 runner 的条件 DNS 恢复

可信 workflow 可通过 `--hosted-dns-recovery` 显式启用临时 hosted Ubuntu 的 DNS 恢复；sudo 仅保留托管身份标识及代理、解析覆写等否决变量的白名单，不保留 token 或其他无关环境。默认调用不改变解析路径。仅在 update 已报告暂时域名解析失败、固定官方域的系统解析也失败时，核对托管环境、默认 stub/NSS、无代理及无自定义或私有软件源，才允许尝试备用 resolver。NXDOMAIN、连接、TLS、签名、完整性和配置错误不能触发此恢复。

备用 resolver 仅作用于 APT 子进程及其维护脚本后代的私有 mount namespace：worker 自己创建并验证 namespace 不同于进入前，完成私有挂载传播设置后才绑定临时 resolver 文件。正常 DHCP search/domain 不依赖具体后缀文本判断，也不复制到备用文件；备用文件固定为 `nameserver 1.1.1.1` 与 `search .`，其他 nameserver/options 和静态覆写仍拒绝。该边界是进程树解析配置隔离，不是四个域名的网络防火墙。不得写入宿主 resolver、systemd DNS、hosts 或软件源；临时文件必须可被 APT 降权后的读取方读取，源目录仍私有。必须在该 namespace 内用实际 NSS/getaddrinfo 证实固定官方域可解析，才可运行原 APT 命令。namespace 或探针不可用时保留失败，不提升 runner 权限、不跳过安装、不改变 TLS 或 APT 签名验证。

探针、namespace 准备、APT 命令、重试和全部清理共用原单调截止及最多三次 update 尝试，install 仍只运行一次。取消、失败和父进程退出后均须清理原 helper 拥有的全部子进程，namespace 随子进程退出撤销；清理不确定不得报告成功。合同测试使用假解析器与假命令保护正常零干预、拒绝边界、总预算和取消；另在独立临时 hosted job 核验真实安装、原 resolver/symlink 不变及无遗留进程。采集诊断成功不能代签安装或本任务 CI 成功。

此实现仅面向临时 Linux CI runner，不推广到开发机器或部署主机。helper 与测试在 `ci-required-scope.json` 的 `scoped_control_paths`、`python_control_contracts` 注册为 `workflow_governance`，同时进入 `ci-tests.sh`；candidate 和未来可信 baseline 都保留合同。使用假命令与临时进程 fixture 验证正常、瞬态/永久错误、重试耗尽、挂起、父进程先退出、忽略终止、预算耗尽及取消，不对真实 apt/dpkg 故障注入。本地合同与实际 hosted 结果分别记录，未执行的场景不得写成已验证。

<a id="ci-install-measurement-design"></a>
## 7. 耗时与运行证据

维护者从实际 Actions 日志分别记录缓存恢复、官方下载、摘要校验/提取/安装/版本校验及总准备耗时；writer 的保存成本另列。当前日志可提供 action/step 级时间，缺少子阶段时间时标明粒度，不把合并步骤的时间伪拆成精确阶段。

需要更细比较时再在同一安装路径补计时。

| 样本类型 | 判定依据 | 记录边界 |
| --- | --- | --- |
| 冷启动 | 没有恢复归档且本地归档不存在，实际执行下载 | 包括恢复尝试、下载与验证安装 |
| 暖缓存 | 从 GitHub 恢复对应归档并通过校验，未调用下载 | 包括恢复及验证安装，不跳过校验成本 |
| 本地已有 | 同 job 已存在归档且验证通过 | 单独列出，不充当跨 run 命中 |
| 恢复异常后下载 | action 异常后仍能按冷路径取得正确归档 | 单列异常与额外成本，不混入正常冷启动 |

每组记录 runner OS/image、架构、Trunk 版本及归档摘要、安装器指纹、source、event/ref、run/job 链接、key、样本窗口与数量。失败和取消保留原因及耗时，不能仅挑成功样本。总发布耗时、排队时间、Rust 编译与 Trunk 准备时间分别报告；Cargo cold/no-op 量测不覆盖本节的网络安装与 cache scope。

样本不足时报告单次观测和环境差异；至少三次可比观察后可以报告样本数、中位数及范围，仍不外推稳定性能承诺。本节工具准备量测不另设性能门禁或预写提速比例；PR 反馈目标遵循[快速反馈设计](#ci-fast-pr-periodic-full-design)。实现接线正确、本地合同通过、hosted 构建成功与真实缓存收益是分别验证的结论。

<a id="engineering-telemetry-design"></a>
### 7.1 共享事件与诊断报告

Trace refs: O7-REQ-7a766dd0-5264-4caf-9dfb-60643ca5faf6；承接[工程遥测要求](ci-tiered-execution.prd.md#engineering-telemetry)。owner 为 `repository_health_engineer`，计算与证据边界由 `qa_engineer` 独立审读。此节定义只读归一化与报告器。当前实现离线 v1 投影、v2 字段来源/身份合同与完整 job DAG 计算，以及显式 run/attempt 的有界只读采集、selection/构建 manifest 和既有 compile metrics 适配；真实来源仍缺数字 job 依赖及生产者/消费者绑定，不输出完整真实 DAG。本地真实 warm 原件及消费者接线仍待核验，不改变 required、缓存保存或发布资格的判定。

#### 当前输入与首版接缝

固定审读基线为 `d4cdabe4c9011ebc66b6376b1dce96f0d8b2cc39`。已有来源各自拥有原始事实，共享层保留原格式，只做版本化适配：

| 来源 | 已有事实 | 适配时的限制 |
| --- | --- | --- |
| Actions run、attempt jobs、steps 与准确 workflow 版本 | GitHub ID、状态、结论、服务端时间戳及 job 依赖定义 | 完整分页并区分 attempts；API 未返回的就绪时间不能推为真实 runner 排队 |
| [受控进程 JSONL](../../../scripts/ci_process_runner.py) | suite/test、started_at、单调 elapsed_seconds、exit_code、timeout/cancelled、count_ok、cleanup_result、passed | 当前记录无 run/job/source 身份；从同一已核验 artifact 的外层绑定，不从测试名猜关联 |
| [构建与产物记录](../../../scripts/ci-test-artifacts.py) | manifest 的 source_sha/tested_sha、producer_run_id/attempt、配置摘要、构建变体、Fresh/rebuilt；打包及恢复报告 | producer 身份不等于当前 consumer；features/profile 从对应 binary、计划与配置读取，缺项保留未知 |
| [本地 compile metrics](../../../scripts/ci-compile-metrics.sh) | current/baseline.metrics.json、comparison.json、固定 commit、命令与日志 | 使用[既有量测设计](../performance/compile-metrics-v2-warm-check.design.md)的隔离 target/环境；不能代表开发 wrapper、远端缓存或全部构建入口 |

真实核验的首条 CI 链选择 PR 中实际命中的 `select → formal-test-build → formal-process-tests → formal-process → required-gate`；展开真实矩阵分片，保留互斥 `formal-test-build-trusted` 的 skipped 事实，并读取 gate 的其他必要上游。main writer 分支还依赖 `cache-bootstrap-maintenance`，不能套用 PR 子图。只采到子链时标 `coverage=partial`，不发布整条 required 的完整关键路径。首个本地入口读取一次 `ci-compile-metrics.sh` 的 `--check-only --measure-warm-check` 输出（实际 package/out-dir 以原命令为准）；适配器不自动执行新的构建。

#### 版本化数据合同

[Python 标准库报告器](../../../scripts/engineering_telemetry.py)提供 `collect INPUT SNAPSHOT` 与 `report SNAPSHOT JSON MARKDOWN`，消费准备好的离线输入。新入口 `collect-github REPOSITORY RUN_ID ATTEMPT SNAPSHOT RECEIPT` 仅读取 Actions，`collect-metrics INPUT SNAPSHOT RECEIPT` 只读取既有指标；所有入口均不触发构建。`collect-source` 保留已有单来源投影。v1 输入、snapshot 与 report 保持原 schema，始终为 partial，DAG analysis 为 unavailable；完整来源合同使用 `oasis7-engineering-telemetry-input/v2`，snapshot/report 同步使用 v2，避免旧消费者误读完整性。

v2 输入的 `sources` 每项包含 `reference / kind / source_schema / payload`。`source_schema` 是本合同支持的适配标签，不宣称准备型 payload 就是真实 GitHub 原始响应。`contexts` 和 `records` 的每个事实字段通过 `field_sources` 说明来源：known 项含 `reference / pointer`，JSON Pointer 必须解析到与该字段同类型、同值的输入；unknown 或 not_applicable 项含 `reason`，对应值必须为 null。缺字段、未知来源 schema、引用不成立或来源冲突，使受影响范围 unavailable。输出仅保留允许字段、`evidence` 中的来源内容 SHA-256 与引用，不复制 raw payload。摘要及字段绑定支持离线复算，不认证来源真实性。

输出 `field_sources` 每个值为来源列表；同身份且同值的事实合并 alias 与来源，不同值保留 conflict，不选取有利值。`sample_kind=synthetic` 显式保留。主要事实字段与后续真实来源映射如下：

| 字段组 | 含义及真实来源 |
| --- | --- |
| `origin / collection_id / collected_at / schema_version` | origin 为 github 或 local；一次采集的 UUID、UTC 查询时间及明确 schema |
| `repository / run_id / run_attempt / job_id / step_number` | GitHub API 原值；`GITHUB_JOB` 是符号键，不能代替数值 job ID。矩阵实例需从可信执行上下文或完整 API 记录精确对应 |
| `source_sha / tested_sha / policy_sha / config_digest` | 从事件、可信计划与实际产物逐项核对；分别表示候选、实际 checkout、规则版本及实际配置。原来源缺 policy 时标 unknown，不能用 source 代填 |
| `local_invocation_id / dirty_inputs_digest / input_capture` | 本地调用身份、固定源码及测量时实际未提交/未跟踪输入的摘要；`input_capture=at_measurement` 明确采集时点；未取得完整输入清单时标身份不完整，事后工作区不能补签。GitHub 专属字段为不适用，本地结果不生成 GitHub check |
| `target / profile / features / default_features / toolchain / runner` | 实际执行命令、计划与 manifest；目标/默认 feature 模式明确，缺字段不根据 job 名反推 |
| `kind / parent_id / depends_on / condition` | context 内 job/step/phase/test 层级；依赖来自该次运行的 workflow 与计划，parent 只表示包含关系。阶段为实际已有 build/restore/verify/test/pack/save 等 |
| `started_at / completed_at / elapsed_seconds / clock` | API 墙钟或本进程单调计时，保留精度和来源；不能将不同进程单调时间相减。仅有开始加耗时得到的结束估计不作跨 job 的精确边界 |
| `execution_result / collection_status / missing_reasons` | 原始结果和采集状态独立；缺失与 unknown 不等于 0，skipped 不等于 pass。退出码、清理失败与首次失败原样保留 |
| `created_at / gate_id / job_ids / pagination_complete / dependencies_complete / conditions_complete` | context 声明创建时间、最终 gate、必要 job 集合及分页/依赖/条件完整性；缺项不推定完整。 |
| `evidence / source_schema / coverage` | run/job/artifact 链接或本地相对路径、内容摘要、适配版本、完整/部分及缺失范围；每个派生指标可回查其输入与公式版本 |

同一来源身份重复且内容相同可以去重，身份相同但值不同须报告冲突；不择取有利值。不同 attempt 与数值矩阵 job ID 独立存储，跨 attempt 复用产物按现有 producer/consumer 规则关联，不能把旧成功拼成新 attempt 的成功。报告只说明原系统的执行结论，不能签发验证资格。

#### 运行时 job 绑定与异构输入

前向 `oasis7-engineering-job/v1` 观察记录由 workflow 的专用非阻断步骤输出。记录 `github.job`、实际 matrix、直接 needs 的结果、workflow SHA、运行目标 source/tested/policy，另列观察时实际 checkout 与允许配置文件摘要。目标 tested 不等于实际 checkout；无 checkout 的汇总节点和未执行分支不补造源码输入。v2 离线合同保持不变，新的绑定报告独立使用 `oasis7-engineering-bindings/v1`，不能把异构输入强填成同一 config。

运行时 `needs` 上下文包含上游 outputs（包括 selection 计划），与只含维度的 matrix 分别限额：JSON 解析前按 UTF-8 字节检查，needs 最多 1 MiB、matrix 最多 64 KiB。解析后仍限制 100 个依赖、32 个矩阵键，校验键名、结果枚举和矩阵值；超额或无效输入使观察失败，不截断、猜测或放宽 gate。观察记录只保留依赖的结果映射，不保留 outputs，source/tested/policy 仍取显式目标字段。此解析额度不保证超过 runner 环境变量传递限制的输入能够启动。

采集器从同一 run/attempt 的数值 job 日志读取专用步骤记录，核对 API 的数值身份、workflow 版本、唯一 job key/matrix 和完整分页。只接受该步骤内唯一标记；缺日志、冲突或错 attempt 保持 unknown。精确 workflow 的静态 needs 仅定义逻辑依赖，未绑定矩阵实例和 skipped 分支时不能发布完整 DAG。观察不是 GitHub 签名证明，也不赋予候选产物执行权限。

Formal 的生产与消费双方观察允许位置的 manifest 原字节 SHA-256；merge 的组件输入与输出、pack 的每分片 manifest、消费者实际读取的 manifest 分开列出。相同摘要仅证明相同 manifest 内容，只有原生产与消费各自的数值 job 观察齐全才建立内容边；它不代替 GitHub artifact ID、下载成功或验证成功。未记录这些事实时分别为 unknown，不按文件名/测试名猜关联。诊断失败不影响测试、gate、清理或发布，日志保留期沿原 Actions 设置，报告建议保留 7 天。 精确非阻断例外在 `ci-required-scope.json.engineering_job_observation` 声明步骤、workflow、超时及 emitter 内容摘要；既有 workflow 合同只豁免匹配的整个独立诊断步骤，任何 required 步骤仍不得吞失败。此策略变化走已有 full/all-groups 迁移路径，不新增 required check；固定基线安全断言与真实 Rust fixture 保留，部分历史完整基线断言交由完整候选 suite 接替，因此不能称为治理规则完全未变。emitter 与声明可一起修改，其摘要不是独立信任根，仍须独立审阅和完整 CI。

#### 时间、成本与样本计算

当前离线分析只对完整 GitHub context 的 job 层进行，step/phase/test 不重复累计资源。所有声明的必要 job 必须具备可核验身份/来源、完整终态、依赖、条件与时间，图无环且最终 gate 覆盖全部必要上游。条件为 false 且 skipped 的节点可保留为零执行节点，但不是 pass。失败或取消的完整终态可计算时间，原结论不变；缺终态则禁止完整指标。

1. workflow 反馈墙钟为创建至该次最终 gate 终态；job 墙钟为 completed_at 减 started_at。只对已终止且时间完整的记录计算；当前离线版本对运行中或缺终态给 unavailable；将来若另报截至查询时刻的等待下界，须明确下界口径。
2. 首次启动延迟是第一个 job 开始减 run 创建。依赖等待依据准确 needs/条件及依赖完成时刻；job 具备执行条件到实际开始之间仅能确认为“未归因等待”，除非有独立 runner 就绪/分配事实。审批、并发限制和服务调度不能默认归到 runner 排队。
3. 完整 DAG 可计算执行时长的最长依赖链，并另列该链的观测等待；不把纯执行链称为完整墙钟。识别观测墙钟链时，给每个 job 增加从实际依赖就绪到启动的等待边，保留其未知成因。缺依赖、条件求值、时刻或必要上游时只报告已知子图，不输出完整关键路径结论。
4. runner-minutes 汇总实际 job 执行区间，单独计资源；并行 job 相加不等于用户等待。共享准备只按实际所属节点计算一次。缓存对照使用同配置的总区间，恢复、校验、构建/链接、打包及保存均计成本；重叠阶段取时间区间并集，无法判断重叠时不伪造净收益。
5. 样本遵循[现行 CI 回顾口径](../../../.agents/dot/tasks/ci-duration-review.md#耗时口径)：固定窗口、同 runner/范围、失败/取消及 carry-in 分开，至少 20 个可比完成样本才报告 P95，少于此数只给样本数、中位数及范围。成功反馈沿用 PRD 的普通成功 cohort；合入阻塞还需 PR 就绪、评审和合入时间线。

可重算的合成例：select 在 0–2 秒，build 在 2–10 秒，两个分片在 10–16 与 12–20 秒，gate 在 20–21 秒。完整反馈为 21 秒；job 执行合计 25 秒；后一个分片有 2 秒未归因等待，执行最长链为 19 秒，含该等待的观测链为 21 秒。删掉该分片终态后，不再输出完整反馈或关键路径，只保留已知记录。首次启动延迟为 0 秒。此例只验证算法，不是实际性能数据。

报告使用 `analysis.formula_version=oasis7-engineering-dag/v1`；`analysis.contexts[].metrics` 每个指标含 `status / value / unit / formula / input_ids`，可回查公式及输入。非空已知子集通过 `known_job_execution_seconds` 列出可证 job 的 input_ids 与 partial 状态；完全没有可证区间时为 status=unavailable、value=null，不冒充完整 DAG；完整图的 `job_timings` 保留未归因等待，图不完整时留空，具体缺口在含 job ID 的 `missing_reasons` 中说明。UTC 时间保留原精度，不从不同进程的单调时钟或开始时间加 elapsed 推导精确跨 job 端点。

#### 采集、保留与故障隔离

离线入口不使用 token 或网络，输入限额为 32 MiB。`collect-github` 使用已有只读权限，通过 `GH_TOKEN` 或 `GITHUB_TOKEN` 读取固定 GitHub API GET；没有 token 时仅尝试公开读取，不调用登录或创建凭据。本地模式不需要 GitHub token。外部服务导出留作可选适配器；新权限、付费服务或产品运行追踪需要各自主责设计。受信任的采集程序不执行候选 artifact 中的脚本，读取归档时拒绝路径越界，并按内容大小限额处理。

每次采集的默认且最大额度为 120 秒、100 次 GET、32 MiB 响应正文及另一个 32 MiB 解码额度。CLI 可以调低这些额度；分页固定每页 100 条，按 total_count 和唯一数字 ID 核对，重复页、变动总数及短页缺项不能标完整。归档最多 256 个成员，投影最多 1000 条记录，每 job 最多 512 steps、每 manifest 最多 512 段构建计时，截断明确保留原因。请求、归档读取、投影和逐条归一化均检查单调时钟截止；HTTP 在自有子进程中执行，父进程超时或取消后终止并回收它，覆盖 DNS 与慢读。截止后的有界校验、序列化及原子落盘属于保存部分结果的收尾，可能超过截止，receipt 的 elapsed_seconds 是采集与归一化到构造 receipt 前的观测耗时，不含随后快照摘要及两文件序列化/写入，全程墙钟需从 CLI 外部另测；这不是硬实时 SLO。任一额度耗尽后保留 partial，不重试或启动额外构建。未知 schema、后页失败、重复页、时间逆序和身份冲突均使相应指标 unavailable，保留可复核的已知输入。

`collect-github` 固定 attempt-specific run/jobs 接口，并完整分页 run 的 artifact 清单。只自动读取唯一的 `ci-selection-RUN-ATTEMPT`，可通过 `--build-artifact ID` 选择同 run 已有构建工件；核对 workflow_run 的 run/head、过期状态及可用的 SHA-256。ZIP 可含 JSON 或一层 gzip/tar；检查所有成员，拒绝越界、重复路径、链接、PAX/特殊成员、损坏内容及超额解码，不落盘解包或执行工件。API 凭据不发送给跳转后的 artifact storage，签名下载 URL 不保留。

现有 selection v1 绑定 context 的 source/tested/policy；仅 pull_request/push 的 head_sha 可作为候选 source，dispatch 不猜。job API 原值单独提供数值 job/step、时间和原结论，不能把 context 的 tested/policy/config 填成各 job 的实测身份，也不按名字推断 needs/条件/矩阵。manifest v2 保留独立 source/tested/config、构建时长和 producer run/attempt；数字 producer/consumer job 缺失则 `producer=null`，在 receipt 的 build_observations 列出已知双方及缺口，不把旧 producer 拼成新 attempt。compile metrics v2 只提供 commit_oid、package、默认 feature 模式及冷/暖/发布时长；不补造命令、环境、invocation、测量时 dirty 输入、退出码或成功结论。

新入口保持 v2 snapshot/report，并另写 `oasis7-engineering-collection/v1` receipt，含预算/实际用量、缺口、来源资源与原字节 SHA-256、原件过期时间及 canonical snapshot SHA-256。contract evidence 的 content_sha256 针对允许字段投影；字段 pointer 保留原路径，receipt 的 raw_sha256 针对实际下载/读取的原件，两者不能混称。`oasis7-readonly-projection/v1` 仅表示已知类型、包含关系、分页事实、默认 feature 布尔转换等确定映射，其 payload 保存在 receipt.projection，不提供外部认证。输出不保存 raw payload。两文件分别原子写入，第二次写入失败返回非零，消费者先核对 receipt 的 snapshot 摘要，不能把旧 receipt 配给新快照。

`--recorded-responses DIRECTORY` 消费显式提供的 run.json、jobs-N.json、artifacts-N.json 和 artifact-ID.zip，经过同一分页/归档/投影路径；receipt.transport=recorded，collected_at 是回放时刻，不是原 API 取证时间。输出不得写回该输入目录；默认 live 与离线回放均不修改原测试、原件或工作区。

只收集允许字段；不采集 token、环境变量全集、原始命令参数中的密钥、个人绝对路径、世界目录或模型请求。已有产物保留期保持不变；新增 CI 归一化报告建议 7 天并遵守仓库更短上限，本地输出由调用者指定目录与保留策略。正文引用的原 artifact 过期时显式显示证据不可读取，不能凭报告摘要补出完整证明。

`collect` 在失败或 partial 时返回 1；`report` 成功渲染返回 0，只说明报告生成成功，不等同执行通过。采集器错误单独输出非零采集状态。接入现有诊断上传时由该步骤记录失败并保留原测试结果，不改写 gate、吞原测试退出码或让遥测控制测试清理；取消时也不要求重任务为上传继续运行。离线报告不读活动写入中的半份文件；生产者完成后原子发布，缺失/截断文件标 partial。

<a id="engineering-telemetry-validation"></a>
#### 最小交付与验证

当前交付包含兼容 v1 的单来源投影、v2 字段来源与身份核验、完整 DAG 算法及有界真实来源适配。真实 Formal 运行已通过导出回放核对，不能把回放耗时当作 live 网络采集成本；本地适配器有固定正反例，但已有真实 warm 测量原件未取得时仍保留验收缺口。随后再让既有 dot 回顾和指挥中心只读消费。共享生成配置、缓存实现和产品节点监控分别留在原主责，报告器不新增日程或自动优化动作。

具体验收按[报告核验手册](../../../testing-manual.md#engineering-telemetry-validation)执行，包含并行计算、身份与重跑、缺失/取消、采集预算和原结论保留的正反例。每项预期具体到报告字段或失败分类，故意改变依赖/等待计算应使同一断言失败。

真实核验对照 API 原值、命令、manifest、JSONL 和文件摘要，确认选中链完整程度及每个派生值。采集与报告的墙钟/字节另列，未测开销不宣称可忽略。无收益时可移除新增诊断接线并保留原产物和检查；schema 不兼容升级保留版本适配，未知版本拒绝完整聚合。输出不足以证明缓存净收益、十分钟/P95 或真实合入卡点时，由原任务继续取证。

## 8. 兼容、启用与回退

实现随普通 PR 合入生效，实际 Release 使用哪个 workflow revision 和 source commit 由原有计划合同确定。计划源必须包含受支持的共享安装器；早于该安装器的历史 tag 在新 workflow 下会明确失败。需要重发历史版本时，先审读相应源码和工具兼容性，不能用移动 tag、任意 ref 脚本或源码安装回退绕过这一边界。

缓存内容无需迁移；旧 Release binary 缓存可以自然回收，首次使用新身份的冷成本单独记录。回退通过正常 PR 恢复已知工作流基线及配套合同测试，仍须独立评审和适用 CI；不在运行中自动选择另一种安装策略。apt 回退须同步 helper、workflow 接线、planner 合同与测试，不能只移除超时或错误传播。此改造不涉及玩家接口、世界状态或游戏 Runtime，无需 Agent 游戏实玩验证。

<a id="ci-real-coverage-design"></a>
## 9. 执行清单、真实消费者与结果

本节承接[真实覆盖合同](ci-tiered-execution.prd.md#ci-real-coverage)，是后续实现要求；不将清单设计、已有工具接线或历史绿色结果视为全部覆盖已交付。

### 9.1 required 到 full 的可校验包含关系

对固定 source 和环境/平台/features/target/影响范围，生成适用 required 清单 R 与 full 清单 F，验证 R 是 F 的子集。清单来自同一检查注册表或实际执行入口的可核验导出，不能维护一份与执行代码无关的手写“已覆盖”清单。每项至少能核对检查身份、命令与参数、前置依赖、平台/feature 适用条件、退出结果；同名但运行不同 feature/target 的命令不能互相替代。

full 可以按依赖图合并重复构建、并行独立测试，但不删除 Clippy 或独立 Viewer browser smoke 的实际调用。browser smoke 之前必须完成其所需依赖和真实构建，失败不执行使用陈旧产物的验证。单个命令失败沿原 suite/job/required-gate 传播；总测试数、成功构建或后续命令的零退出码不能覆盖它。可信 base 选择器与候选控制输入不缩小基线覆盖的规则保持不变。

对不支持的环境明确标记不适用并指出实际受支持入口；缺工具、缺 target、未执行或无法判定列为阻塞/失败/未知，不能装成不适用或通过。回归故意删去 full 中一项 required、修改其 feature/target 和令各命令返回非零，确认包含关系和失败传播均生效。这些回归进入既有检查，不新增身份门禁或 PM 复验链。

### 9.2 WASM 与 wrapper 分层证明

维护实际 native/WASM 消费者到 stable smoke、确定性构建入口、输入/产物及 hash 核验的映射。fake Docker 只检验 build wrapper 传参和错误传播；真实 builtin 容器构建与 hash 比较独立执行。`build-wasm-module.test.sh` 由 `ci-tests.sh required --group packaging_contracts` 执行；本地 provider bridge 回归由 `operational_contracts` 执行。两条链路都须保留实际调用与失败传播，不能仅凭测试文件存在宣称 CI 覆盖。缺必要 target 或把所有 lane 改为 `BUILD_STD=0` 不能压掉真实消费者；明确已退役且无消费者的旧 lane 不重新激活。

本地 `cargo-dev.sh` 只设置开发 profile 与可用的 sccache，不设置或覆盖 target-dir。Cargo 参数及 `--` 后的应用参数原样透传，包括含空格路径；显式参数、环境与配置的优先级由 Cargo 决定。默认产物位于各 worktree 的 `target/`。入口不维护外部缓存、租约或退出记账，直接传递 Cargo 的退出码。

原生 Windows 保留 Git Bash 的工具链发现与执行验证；本地入口不再要求符号链接权限或实现额外的缓存锁。并发构建使用 Cargo 自身的锁；不得清理使用中的 target 或其他 worktree 的资料。

### 9.3 CodeQL 结果是独立证据链

沿用 [CodeQL 专项设计](codeql-integration.design.md) 的语言、查询、预算与启用决定。每个适用语言/查询组合分别核对执行终态、上传/analysis 绑定及 finding 处置，使用准确 source、ref、run、attempt、job、category 和 App。API 分页在总预算内完整读取；后页失败、重复游标或预算耗尽不发布完整结论。缓存结果与不匹配运行不能补齐缺失证据。

修复结果发现/绑定不意味着 CodeQL observe 已启用；扫描超时也不直接授权延长预算、删除查询或新增 required gate。专项执行、依赖兼容复核和 RustSec 到期处置独立推进，不让一个广泛 umbrella 的等待阻止有明确期限的安全维护。

<a id="ci-advisory-baseline-design"></a>
## 10. RustSec 语义解析与例外生命周期

本节承接[RustSec 基线要求](ci-tiered-execution.prd.md#ci-advisory-baseline)。当前策略由 `deny.toml` 拥有：空 `advisories.ignore`，以及对原三个停止维护包的无版本限制禁止项。没有实际例外时不再维护日期或直接依赖名单。

[配置检查器](../../../scripts/check-rustsec-ignore-baseline.sh) 使用标准 TOML 解析器读取完整配置，要求 ignore 是空数组，并核对每个禁止包都有无条件 `bans.deny` 条目。版本限制、wrapper 豁免、缺失条目和错误类型均拒绝。解析失败沿既有治理入口传播；合法 TOML 中的非空 ignore 仍拒绝。注释中的 advisory 不算启用项，字符串和表名的合法写法不影响语义判断。

随后由既有 Rust 基线入口执行 `cargo deny --locked --all-features check advisories bans`，按解析后的真实 package 名称检查依赖图，涵盖别名和传递路径。锁文件必须与 manifest 一致；配置检查器同时扫描仓库内受版本管理或新加入的全部 Cargo.lock，按 package.name 拒绝三个旧包，涵盖独立 WASM 模块及新旧网络兼容 fixture；锁文件扫描不能代替真实构建和平台运行。既有直接依赖基线随最后三个例外删除，避免保留无实际消费者的期限与允许名单。未来重新引入例外或禁止包需要显式策略变更和独立安全评审。网络组在 Linux 上运行本地 if-watch 补丁的全部 feature 测试，直接验证接口枚举；macOS 交叉编译不能代替这一运行验证。

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计 | 验收内容 | 验证来源 | 实际记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-7d1dab3c-451e-4915-bf76-5bd28c117371；[可生成工作流定义](ci-tiered-execution.prd.md#ci-generated-contract-jobs) | [首轮生成试点](#ci-generated-contract-jobs-design) | 确定性、漂移/标记/路径错误、只读失败、原子写入及 job/DAG/权限/身份/失败语义等价 | [workflow 合同](../../../scripts/ci-workflow.test.py) 为现有入口；实施时按本节正反例扩展生成器回归，并执行既有 `ci-required-result.test.py` | 实施 PR 记录固定 source/tested/policy、迁移 diff、实际命令与 CI；当前仅设计 | 模板与输出一致不能代替语义合同，不证明 hosted 性能或扩展范围 |
| Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；[快速反馈与周期回归](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full) | [分层实施](#ci-fast-pr-periodic-full-design) | 准入/迁出、真实包与 feature 闭包、Formal 最低库存、完整组/平台、旧 BASE/候选 bootstrap、失败重跑及固定候选发布 | [分层验收](../../../testing-manual.md#ci-fast-pr-periodic-full-validation)、实际 workflow/driver、selector/result/receipt 与真实 Cargo 合同 | 实施 PR 的准确候选、计划、运行链接、观察窗口与耗时分布 | 当前候选已接线；本地合同不代签 hosted 完整覆盖、十分钟/P95 或四小时容量 |
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | [覆盖专题](#ci-coverage-design) | 同候选/环境/features 下离线消息链、full 包含关系、失败及未接线目标 | [测试手册](../../../testing-manual.md)、实际 driver/选中 job；按本节选择正反例 | 实际命令、参数、case、退出与缺口；本次未执行 | 不由 selector fixture、skip 或旧 PASS 代签真实覆盖 |
| Trace refs: O7-REQ-83ee480f-007c-446d-a05d-827c51b403e3；[显式本地检查](ci-test-coverage.prd.md#local-check-entry-requirement) | [薄入口合同](#local-check-entry-design) | LOCAL-AC-1–5：阶段实际范围、最终完整输入、只读与非成功语义 | [本地入口核验](../../../testing-manual.md#local-check-entry-validation)，实施候选的真实 driver 与临时仓库正反例 | 实施 PR 的输入/命令、前后文件与 index 身份、报告和退出码 | 文档检查不证明 thin entry 已实现；本地通过不代签 PR/平台/真实环境 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | [Case 分级](#ci-case-tier-design) | 同候选标签到实际执行；漏标、错 feature、零匹配和失败传播 | [测试手册](../../../testing-manual.md)、实际 CI driver；结果函数回归 `scripts/ci-required-result.test.py` 仅作补充 | case/feature/target 与运行结果；本次未执行 | 无 hosted 或性能结论，不改 required/full 保护 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | [测试范围](#测试范围) | 删除、重命名、依赖和未知输入覆盖 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 命令退出码及输出 | 不替代真实产品组运行 |
| [未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | [失败链路](#ci-unknown-inputs-design) | 未登记新增/删除/重命名、混合全量控制、候选豁免不能放行；无成功输出且 gate 明确失败 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 当前 PR 中保留选择器、工作流和结果回归的命令、source HEAD、退出码和 Actions 结果 | 本地 fixture 不证明 hosted 运行；Git/依赖解析不确定仍保留全量 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | [调度和结果](#调度和结果) | 失败、取消、缺失和跳过均正确传播 | [结果函数回归](../../../scripts/ci-required-result.test.py) | 回归输出与 Actions 链接 | hosted 图仍需独立核对 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | [专项条件](#专项条件) | 适用环境和真实场景执行 | [测试手册](../../../testing-manual.md) | 所测版本和实际环境结果 | 未执行的环境保持未证明 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | [安装流程](#ci-install-trunk-design) | planned source、只恢复归档、冷热均调用验证安装器、必要失败传播 | [Release 接线回归](../../../scripts/release-packages-trunk-cache-contract.test.sh) | 候选版本、命令退出码及回归输出 | 不替代真实下载、Web 构建或发布 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | [缓存边界](#ci-install-cache-design) | main writer 与不同 tag readers 的真实身份、恢复及验证 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | writer/reader run/job 链接与实际 key | 未 seed 或未执行 reader 时仍未证明跨 tag 命中 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | [apt 有界安装](#ci-install-apt-design) | 验证总预算、分类重试、超时/取消及受控进程清理 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 实际 apt 观察与 helper 的 fixture 输出 | 本地合同与 hosted 分别验收，仅限临时 Linux CI runner |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | [耗时证据](#ci-install-measurement-design) | 相同工具/平台下分冷暖计入准备与缓存成本，保留失败样本 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 原始耗时、样本窗口/数量和条件差异 | 不以本地测试宣称 hosted 性能收益 |
| Trace refs: O7-REQ-7a766dd0-5264-4caf-9dfb-60643ca5faf6；[工程遥测](ci-tiered-execution.prd.md#engineering-telemetry) | [共享模型与诊断](#engineering-telemetry-design) | 身份精确关联、并行时间计算、缺失与执行结论分离、有界只读采集 | [报告核验手册](../../../testing-manual.md#engineering-telemetry-validation)，固定合成快照与一条真实 CI 链、一个本地 compile metrics 输出 | 实施 PR 的 schema、输入摘要、实际报告、命令和独立复核 | 离线 v2 来源合同与 DAG 已实现；live collector 与真实 CI/本地核验未完成，合成输入不证明性能收益、真实合入阻塞或产品节点健康 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | [执行清单与结果](#ci-real-coverage-design) | 漏项/错误传播、真实 WASM 消费者、wrapper 退出及 Windows 锁边界 | [测试手册](../../../testing-manual.md)；实际 CI driver、构建/wrapper 回归和原生平台运行 | source、计划清单、命令与真实结果 | fake 工具和计划存在不代签执行；CodeQL 启用另验 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | [例外检查](#ci-advisory-baseline-design) | 空 ignore、无条件禁止包和真实依赖图 | [基线回归](../../../scripts/check-rustsec-ignore-baseline.test.sh)及真实依赖查询 | 配置正反例、cargo-deny 真实依赖图 | fixture 不证明实际依赖已移除 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | [进程调度](#ci-formal-process-design) | 独立分片、精确清单、同源产物、旧基线扩大、错误与超时传播 | [分片核验](../../../testing-manual.md#ci-formal-process-validation) | 当前 PR 的真实 runner、测试与评审结果 | fixture 不代签完整产物消费；未完成 hosted 验证保持未证明 |
| [共享产物中的缓存容量](ci-tiered-execution.prd.md#ci-formal-process) | [容量治理](#ci-cache-budget-design) | 已确认 current 或稳定兼容锚点回收、轮换空间、族/ref/version 与压缩预算、分页及并发/访问保护、失败不影响必要 CI | [容量治理回归](../../../scripts/ci-cache-maintenance.test.py) | 当前 PR 的回归输出、准确 source、hosted 保存/回收和库存记录 | 本地 fixture 不证明实际释放空间或缓存收益 |

## 12. 决策与残余风险

历史 [Issue #1808](https://github.com/eng-cc/oasis7/issues/1808) 中的旧 PM 工具问题随 [#4354](https://github.com/eng-cc/oasis7/pull/4354) 退出当前实现；本设计保留可适用于现行 CI 的依赖准备与缓存问题，不恢复旧任务或验收状态机。选择共享预编译安装器，是因为现有实现已集中拥有来源、摘要和版本检查，可以直接去掉 Release 的重复源码安装路径。

`repository_health_engineer` 维护安装器、缓存接线及 apt 有界安装；`qa_engineer` 协作核对真实运行证据。主要残余风险是外部下载服务可用性、main seed 尚未产生、缓存被回收以及旧 tag 缺少安装器；这些情况分别按既有失败语义或正确冷路径处理。版本、平台、来源策略或 cache action 更新时重新审读受影响合同；没有实际 hosted 数据时保留未知项，不从文档状态推导发布成功。

<a id="ci-formal-process-design"></a>
## 真实进程验收的边界与结构

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7

承接[共享产物要求](ci-tiered-execution.prd.md#ci-formal-process)。可信 BASE 启用完整层策略后，以下正式 producer/矩阵只由 `periodic-full/candidate-full` 选择，普通快速层保留轻量未选中汇总；旧 BASE 继续原选择。`ci-test-artifacts.py` 从 Cargo JSON 的 executable 字段收集可执行文件，并区分 formal、ordinary 与兼容本地入口的 all 构建范围。正式最小 producer 仅构建 required-feature 的 oasis7 library / chain-runtime binary harness、production runtime、release endpoint 及 default-feature distfs library harness；`oasis7-test-build` 按可信计划选择包级或独立 target 库存，完整 profile 保留原全包、Viewer、default-feature snapshot 并追加快速 feature 组合。两个构建范围独立依赖 select，正式矩阵不等待普通 producer 完成。

正式 harness 的最小构建显式启用 `oasis7/ci_formal_required_features`，通过 native target 与 host build 的 optional dependency aliases 保留完整 required 根包共同构建时的依赖 feature 集合。该标记不参与业务 `cfg`，不自动启用，也不加入 production runtime、ordinary 或 default-feature snapshot 的原命令；依赖版本复用现有锁文件。验证比较完整构建与正式最小构建保留的完整依赖子树，并分别核对 host/build 与 target context 的实际 feature 组合，不能只比较根包上的 `test_tier_required` 字符串或把同包不同 context 合并后判断等价。

正式构建按 profile 拆成并行的两路。`formal-debug-build` / `formal-debug-build-trusted` 构建原有 required core、production runtime 与 default-feature distfs harness；`formal-endpoint-build` / `formal-endpoint-build-trusted` 只构建原有 release endpoint。每一路只读与可信入口互斥。Formal 的 DEV/TEST 显式使用原有 opt-level 0，保留 debug assertions 和 overflow checks；已有少数 package 的独立优化保持原配置，storage 专项继续使用自己的 opt-level 2，endpoint 保持 release opt-level 3 及原校验。构建、缓存身份、组件合并与消费校验使用同一配置，拒绝混入原全局 opt-level 2 的 Formal 产物。扩大优化范围必须先比较相同源码、features、runner 和缓存状态下的构建加真实用例关键路径。新 profile 只有可信 main 的完整运行使用 writer，`pr-fast`/`main-fast` 与其他候选保持只读；旧 BASE 回退保留原 main push 条件。

两路构建为每个 Cargo 上下文输出开始、持续进度和结束状态，并启用稳定的 `--timings` HTML 报告。进度包含已耗时长和可取得的编译进程 CPU/RSS、主机内存及 swap 摘要，不读取或输出进程环境；未知指标保留为空。诊断按本轮独立目录保存，已产生的摘要与 timing 在成功或失败后上传，不能把旧缓存中的报告冒充本轮数据。观察器退出或缺少指标不改写 Cargo 的实际退出结果；心跳只表示观察器与进程状态，不证明编译有进展。缓存仍按实际 profile 隔离，不以新建缓存族或 rerun 次数作为预热证据。

原 `formal-test-build` / `formal-test-build-trusted` job 名保留，职责改为下载两路精确输出 artifact、执行 `merge-formal` 完整核验并分包，不编译或写 workspace 缓存。选中的合并入口须在对应两组件均 success、另一对均 skipped 后才可运行；任一必选组件失败、取消、缺失或跳过均阻断。矩阵和固定汇总继续要求两个合并入口恰有一项 success 且另一项 skipped，保持可信 BASE 汇总合同。

组件 manifest 明确区分 `formal-debug` 与 `formal-endpoint`，绑定同一 run/source/tested/checkout、计划、Formal 配置及 authority、工具链、平台和构建身份；`merge-formal` 按真实 Cargo target 库存核对 variant/package/kind/name、profile/features、文件摘要与实际测试列表，拒绝漏项、重复逻辑目标、跨角色或部分组件。相同字节的不同真实目标保留各自身份。组件不能直接进入完整 Formal 的 pack/run。全部核验通过才生成 `formal` 完整 bundle，合并不补编。

组件 artifact 名包含实际 producer run/attempt，并通过 job outputs 传给合并入口。同 run、同源码失败重跑可组合较早 attempt 的成功组件；producer attempt 必须为正整数且不晚于当前 attempt，记录两路来源，拒绝跨 run、未来 attempt 或身份不一致。组件成功后可独立完成可信缓存维护，维护不等待分包、测试或 full-regression，也不影响 required/full 结论。

新构建 scope 同时决定缓存 profile 与族容量，可信 Python 合同登记因此要求 `ci-test-artifacts.py` 的修改执行容量策略及共享 profile 的存储验收回归，`ci-workspace-cache.py` 的修改执行维护回归，覆盖 profile 断言、两组件预算、writer 身份与不跨族退休。迁移沿 full 的精确完整组校验，仅在可信新策略的快速 profile 中排除已后移的 Formal；固定 BASE 安全断言、最低测试清单和真实 Rust fixture 保留。

重型 `formal-process-tests` job 在既有 `always()` 和两构建入口状态判断上同时要求 `!cancelled()`。同一 PR 的并发替换或显式取消使该条件变为 false，停止旧分片；保留 `always()` 处理互斥构建入口中合法的 skipped。固定 `formal-process` 与 `required-gate` 继续 always 汇总，取消、失败或意外跳过仍阻断；分片故障的 `fail-fast: false` 与诊断上传步骤的 `always()` 保持原语义。条件回归须覆盖取消前后、合法/非法构建结果组合及未选中路径，不以只匹配一段 YAML 文本代替实际条件判断。

producer 核对完整清单后，为普通组及各物理分片输出独立 tar.gz，gzip level 1；保留 executable 权限，不传输整个 target。每包只带该消费者的 harness、生产子进程和动态库闭包，并报告原始字节、压缩字节和逐 binary 体积分布。consumer 核对原 run/source/tested identity、全局清单及所选闭包的 hash、profile 和实际 `--list`，不能因缺少不属于自己的二进制而补编。producer attempt 记录供审计；同 run、相同源码和构建身份的 consumer 重跑可复用先前 attempt 的产物。保持相同 checkout 布局，恢复编译时嵌入的 Cargo 程序路径，设置包工作目录、动态库路径及原二进制环境。普通集成测试需要的 Cargo binary 和 default-feature snapshot 不得被正式分包移除。

完整 ordinary 由可信计划的 `ordinary_artifact_partition_version=1` 启用物理执行分片，保留唯一 producer 的完整 Cargo 命令、feature resolution 和完整 archive。`controls` 单独执行原 Clippy/WASM 检查；`runtime`、`component-libs` 分别执行主 runtime harness、其余 library harness；其余 harness 及两个 default-feature snapshot 检查按完整 binary/filter 身份排序后交替归属 `integrations-0`、`integrations-1`。每个消费者先核验完整产物，再按实际 variant/package/target 身份分配完整执行项；并集必须等于全部原执行项且各有唯一归属，新增 target 进入对应剩余类别。消费者内部串行，不重复 Cargo 构建或减少测试。本地未指定 `--ordinary-partition` 时，保留一次构建完整 bundle 后执行全部 ordinary 项的入口，并执行原 Clippy/WASM 检查。旧 BASE 没有该能力时保留 `all`，版本或分片缺失、重复、未知均拒绝。矩阵保持 `fail-fast: false`，任何必选分片失败、取消或跳过都阻断汇总。

可信 BASE 的 `ordinary_artifact_delivery_version=1` 在最终计划为 artifact v1 时，进一步将文件运输分为一个 common 包与四个 harness 包。producer 保留原完整构建、feature 组合及库存验证；父 `manifest.json` 与旧 ordinary 完整包同义，保留全部 binaries、完整 `included_binaries`、`selection={suite: ordinary}`、原计划及全部身份。从既有 `ordinary_artifact_entries` 重算四个分区的完整 harness 集合，要求各有唯一 owner、并集等于原全部执行项；剩余共同二进制、全部既有动态库及 provider/builtin 资源只打包一次。每个 native consumer 下载 common 和自己的 harness 包；controls 保留原 Clippy/WASM 检查与 required 依赖，但不下载无用 native 包。此版本不改变执行分桶、case filters、默认 snapshot、full 补充 feature、helper 的原 Cargo 路径恢复或资源校验，不细化 helper 归属。

父清单只序列化一次；common 与各 harness receipt 绑定其原字节 SHA-256、分区及重算的精确二进制集合。摘要用于防止混包，不替代可信计划授权。受控解包仅接受父清单推导的角色与文件集合，拒绝重复路径、越界、symlink、未知文件及跨包 hardlink；分区包不得覆盖父清单、common receipt、helper 或资源。每个 archive 内独立进行相同字节的 hardlink 去重。消费时保留父清单原貌，另算本地共同文件与当前完整 harness 的闭包，沿用 source/tested/plan/run/toolchain/config、hash、profile、实际 tests/ignored inventory 和资源核验。缺失或损坏失败，不补编。相同 run 中 consumer 重跑可使用较早成功 producer；仅要求本次消费的 common 与 payload 具有相同父摘要，不要求所有历史已成功矩阵行重跑或 producer attempt 等于当前 attempt。

新 delivery 字段只有可信配置授权且最终 plan 为非 inline 的 artifact v1 才输出。旧 BASE、旧 plan、artifact v2、inline 及本地完整 bundle 保留原路径；字段一旦存在，bool、未知版本或不兼容组合直接拒绝，不静默降级。实际当前完整父库存的分发完整性不等于另建了所有 Cargo target 的独立权威库存。矩阵成员、实际测试计数、退出/取消/清理和最终 required/full 汇总仍沿用原合同。

能力尚未进入 BASE 的迁移 PR 可运行独立只读诊断 job：下载正式 producer 的旧完整包，验证原父身份和实际库存，再对相同二进制执行新打包、受控解包及四分区实际库存/资源核验。诊断不改父计划、不生成新的选择权限，receipt 明确标识 diagnostic-only，正式 run/load 拒绝该材料；它不属于 required/full gate。记录完整包与新包的压缩字节、各消费闭包、打包/解包/校验时间及父摘要，不将本地重复读取当作真实网络或端到端提速。可信 BASE 已启用后，正式 producer/consumer 自身提供新分发的真实运行证据，不再为该迁移重复启动诊断。十分钟 P95 和四小时完整回归仍分别按托管实际记录验收。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

可信配置支持 v2 后，只有 `pr-fast/main-fast`、`scope=targeted`、非 inline、明确 `test_policy_changed=false` 且选中 chain-runtime required 包时，才输出 `controls/native-0/native-1/native-2/native-3`。full/explicit-full 继续原 v1；其它 targeted、inline 和旧 BASE 保持原路。producer 的完整 targets、默认 wasmtime/node-libp2p 与 test_tier_required features 不变。每个消费者先校验同一完整 bundle 的 source/tested/policy、内容和实际 `--list` 库存，再以 `required/oasis7_chain_runtime/lib/oasis7_chain_runtime` 完整身份寻找唯一 runtime library。先展开原 exclusive singleton，剩余原 ordinary case 按完整名称 SHA-256 排序后轮转四片，以 exact skip 补集执行完整 case；原 ignored/deferred 语义保持。所有分片非空，并集与原普通库存相等、交集为空，其余完整 harness、snapshot 和 exclusive singleton 各有唯一 owner。未知版本/owner、错身份、空执行、缺失、重复、少测、异常、取消、超时或进程遗留都失败；原断言、超时与清理不变。数量估计仅用于确定性分配，不作为历史时长数据或十分钟收益证明。

可信配置可为 targeted v2 下发 `ordinary_harness_costs` version 1，按 package、target kind/name、variant 和既有 batch/filter/exact 执行身份保存成功进程回执的固定毫秒估计，并记录来源 run/attempt 及 source/tested/policy。它是逻辑目标的粗调度提示，不证明不同 features 或构建 profile 等价，也不用于选择或跳过测试。Chain 原 case 分配保持不变；其余完整执行项按估计成本降序、稳定身份打破平局，再分配到累计成本最小的片。完整项的累计成本从零开始，不与 Chain 的 case 数混合。没有匹配历史的新增目标、变体或执行项仍完整运行，使用固定正数 fallback；未知版本、重复身份、非整数、非正数或超过一小时的成本拒绝。profile 不携带可执行过滤指令，实际 filters、库存、计数和失败传播仍来自原父计划及产物。旧计划没有该字段时保持原数量分配；v1、full 和 inline 不接受该能力。日志分别保留真实应执行数、命中/回退数量与估计成本，实际耗时及十分钟目标仍由 hosted 回执验收，不以同样本静态重排代签。

v2 中仅当包集合精确为 `[oasis7_chain_runtime]` 且 owner 为上述四个 native 分片时，consumer 省去 Rust toolchain 安装、Rust cache 稳定化和依赖 cache 恢复；产物执行及当前测试所调用脚本不需要编译器，真实小型 Cargo fixture 用失败哨兵验证制品消费没有补编。controls、含其它包的 v2、v1、inline 和旧路径保留原准备，避免省略 Game Client 等测试实际调用的 rustc；APT 原依赖仍完整安装。日志记录 runtime 总库存及当前分片摘要、预期执行数与附带完整 harness，真实产品四片耗时、producer 成本及端到端 P95 仍由托管运行验收。

分片清单由 `scripts/ci-formal-process-suites.json` 拥有，实际名称以下载 harness 的 `--list`/`--ignored --list` 核对。清单中的旧过滤器用于检出重命名、遗漏和新增未分配验收；辅助入口单独登记，不运行整个 ignored 集合。

| 旧入口覆盖 | 新归属 |
| --- | --- |
| protected endpoint 协议、崩溃、effect、handoff、availability、survivor | runtime-protocol |
| protected endpoint 长历史 | world-history |
| formal healthy durable set、owner 故障 | healthy-set |
| distfs endpoint qualification、recovered children、C10 长记录 | storage-recovery |
| world initialization、full state sync、local-node-lifecycle、legacy migration | lifecycle-cli |
| oasis7 lib / runtime bin required-feature 的非 ignored formal | runtime-protocol，保留原二进制环境 |
| distfs default-feature lib 的非 ignored formal | storage-recovery，保留原二进制环境 |

表中归属保持为逻辑 suite。物理分片可将 healthy-set 或 runtime-protocol 的完整慢用例放到较轻的分片；执行摘要同时记录逻辑 suite 与物理 shard，并对全集检查恰好一次归属。world-history 的单个 70 次连续操作场景保持完整，不能以切断历史链消除增长成本。上述非 ignored formal 命令未证明与普通环境等价，继续执行；snapshot_progress 与 legacy fields 回填仍使用原 default features。每个精确 case 仍使用 `--test-threads=1`，ignored case 仍带 `--exact --ignored --nocapture`。

runtime 的两个完整 membership 场景分别放到 healthy-set（actual world noise follower）与 lifecycle-cli（confirmation proposal timeout recovery）；逻辑 suite 仍为 runtime-protocol，继续独占各 runner 的 CPU 预算。预计算物理分片同时考虑显式 case 和所选 environment batch 的 override；构建后仍须证明 override 命中真实库存，不能因为 case 来自非 ignored batch 而漏选目的分片。

普通执行同样隔离这两个四 Node 时序敏感场景：按 runtime library 的真实 package/target 身份和完整测试名，将实际非 ignored 库存分为剩余 batch 与每项独立 exact 执行，三者不重叠且并集保持原库存。剩余 batch 使用 exact skip，不能以子串过滤误删相似名称；各项仍保留原计数、环境、超时和失败判定。完整物理分片仍归 runtime，inline 池中的独立场景占用全部 CPU 预算，不与其他 harness 同时运行。未包含这些场景的 feature 组合不新增测试，新增或其他用例继续按原库存执行。该隔离不放宽协议期限，也不以 Formal 通过代替普通环境验收。

新完整 manifest 的 `formal_execution_version=1` 允许分片内采用有界完整用例执行池。只有可信 Formal 配置 `parallel_cases` 中按 binary 身份、完整测试名登记并审读隔离性的 case 可占 1 或 2 个 CPU 槽；总槽数不超过 `native_cpu_budget()`，worker 最多 4 个。未知、新增未登记 case 与 shell script 独占全部预算。候选运行只接受 BASE 的并发许可，不因候选配置扩大并发；旧或未知 execution version 保留串行。每个 case 拥有独立进程 owner、绝对期限、结果身份、计数和清理，取消后不再启动排队项；任一失败使本次选定执行失败，已完成成功的结果不被其他 worker 后续失败改写。结果核验通过后才追加逻辑 suite / 物理 shard 元数据，不能以并发掩盖缺项或重复。

初始许可覆盖 18 个精确 runtime case：局部内存 snapshot、私有 Scratch/CAS 的 multivalue 与 prefix ledger、完整 SIGKILL 恢复、10 高度存储验收、单 Node publication 恢复，以及独立 endpoint 的 authenticated native pair。私有根使用 PID/序号或随机材料区分，故障配置通过各子进程环境传递，TCP listener 持有零端口绑定直到关闭，子进程有独立清理；有真实 Node/子进程的条目占 2 槽。先释放临时端口再重启固定地址的 protected runtime、四 Node 时序敏感 membership 和 70 步世界历史仍独占。此许可基于源代码隔离性审读，首次主线并发仍须核验实际期限、清理和资源，不能据此声称已有 hosted 加速。

矩阵 `fail-fast: false`、`max-parallel: 5` 保持。world-history job 上限 45 分钟，其他分片初始保护上限 90 分钟；每次子命令绝对上限 40 分钟，均为防挂保护而非性能目标，后续按真实基线调整。

正式 workspace 缓存使用官方 restore/save 的独立步骤。先恢复既有可信依赖缓存，再恢复本次正式构建实际使用的仓库内 path package 编译单位；不重复保存整个第三方依赖 target。

Formal debug 和 release endpoint 使用独立依赖 namespace `ci-formal-debug-deps-trusted-v1` / `ci-formal-endpoint-deps-trusted-v1`，由各自真实组件构建自然准备其精确 Cargo context。shared-native 的命中不能代签这两份独立缓存已准备；是否复用必须依据本组件真实恢复结果与 Cargo fresh/rebuilt。两路 reader/writer 使用相同 namespace、工具链与显式 profile 环境；候选只恢复，可信 main schedule/manual-full 的对应 component writer 仅在 job 成功后保存。上传前复用现有实际压缩归档准入，校验 writer/job/key/run/platform 和完整仓库额度；准入安装或容量检查失败时跳过保存，完整构建与测试照常执行。维护按各 namespace/environment/cache version 保留完整恢复锚点，不把 debug、endpoint 或旧 shared-native 互相视为替代。

构建从 Cargo 的 compiler-artifact 与 build-script-executed 消息确定具体产物 hash，保存对应 deps、完整 .fingerprint、build-script 编译与执行输出目录以及公共可执行文件。内容和纳秒时间戳原样保留，不改写 Cargo fingerprint 来强制命中。缓存目录位于 RUNNER_TEMP，恢复前校验绝对 checkout/target 身份、路径、文件类型、长度、权限和摘要，完整校验通过才叠加到依赖 target。

新 checkout 的源码时间戳会使相同内容的 workspace 产物失效。producer 因此记录构建前后完整源码文件清单，包含未跟踪和 ignored 的普通文件，排除 .git 与实际 Cargo target；只有始末路径、内容、权限和时间戳均一致才允许保存。

恢复时逐路径核对实际长度、权限和 SHA-256，内容相同才还原旧 mtime；内容或权限改变的文件设置严格晚于缓存 Cargo 参考的时间戳，交由 Cargo 正常重编。

新增、删除、重命名输入、symlink、越界路径、不完整或未来时间戳的缓存均保守视为 miss，不回退目录时间戳以隐藏 build.rs 的目录观察。缺失缓存仍完成冷构建，缓存不是产物身份或测试通过的依据。

key 隔离平台/架构、Rust 与 WASM 工具链、Cargo lock/manifests/config、构建与缓存脚本、实际 build variants 和 features/profile，source SHA 用于精确条目，同一兼容前缀才可回退。候选只读，可信 main producer 才可保存。

两路使用独立的 `formal-debug-workspace-v1` 与 `formal-endpoint-workspace-v1` 兼容前缀，可信 writer 分别固定为 `formal-debug-build-trusted` 与 `formal-endpoint-build-trusted`，避免一种 profile 的命中掩盖另一种冷编译。debug 原始 staging 上限 6 GiB、单份上传上界 768 MiB；endpoint 分别为 2 GiB、256 MiB。各保留两代、允许临时第三代，新两族合计常驻目标 2 GiB、轮换上限 3 GiB。历史 `formal-workspace-v3` 仍按旧族的 8 GiB staging、1 GiB 单份上传、两代/临时三代治理，不根据任一新组件的存在跨族删除；迁移期间旧族占用与新两族一并计入既有全仓预算及依赖轮换预留，容量不足跳过保存，不能提高额度或伪报缓存收益。

所有 producer 仅用 stat 报告完整 target、所选单位、硬链接去重字节、目录归因和预算 eligible，不为测量计算 payload 摘要、复制文件或改写源码 mtime。

只有允许写入且未精确命中的 producer 才校验并准备缓存，超限正常跳过 save。远端代数与实际压缩总字节按[容量治理](#ci-cache-budget-design)独立约束，不能把单份 staging 预算当成仓库总量保护。

复用收益必须包含依赖与 workspace 两次恢复、源码验证、保存与清理成本和 Cargo 实际 fresh/重编数量，不能仅凭 cache action 命中宣称免编译。

运行时优化遵循[单次 open 预热复用](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#formal-open-prewarm-reuse)和[健康 Replica 归档安装](../../p2p/design.md#formal-replica-staging-content-reuse)。新请求等待只读取精确请求的 hot result，使用轻量发布切面观察避免每 25ms 克隆完整 World；历史原请求仍保留认证冷查断言。

正常成功路径达到目标 publication 后可先停止同一 runtime owner，再做不依赖活服务的材料统计与历史断言，减少测试自身继续产生的空 Tick。生产 Node 的 in-flight stop/join 语义不变，专门的 pending/in-flight 停止场景继续覆盖。

阶段诊断记录 open/prewarm/restore、70/40 次操作、冷查及 stop 的单调耗时；不改变原请求绝对期限、签名、fsync 或原验收断言。

链节点入口由独立包 `oasis7_chain_runtime` 拥有，保留 `oasis7_chain_runtime` 可执行文件名、CLI、正式协议与持久化格式。`oasis7` 保留世界执行、状态和读投影；链节点以关闭默认 features 的显式依赖接入，并按需转发 Wasmtime、网络和测试 features。

<a id="loaded-executable-component"></a>
已加载可执行文件的描述符、文件元数据稳定性和 BLAKE3 身份由独立生产库 `oasis7_executable_image` 拥有。该库仅依赖哈希与目标平台的系统接口，不依赖 Chain Runtime、World、网络或 WASM 执行器，测试也不借 dev 依赖拉回这些宿主。Chain 的 Frozen policy、原生环境捕获、package preimage 和运行时兼容校验共同调用该唯一实现；可执行文件名、协议、摘要格式、每进程 OnceLock 语义和平台拒绝行为保持。它表达本进程加载的文件身份，不构成远程证明或权限凭据。

公开 facade 固定在 `src/lib.rs`，使用 `File`、`Metadata` 和字符串结果；私有摘要实现及完整替换文件回归按组件运行真实 library target。当前快速登记仅包含 `src/digest.rs`；该实现改动无需编译 Chain 宿主，facade、manifest、操作系统实现、工具链、平台 cfg 或测试政策变化仍扩展验证实际消费者。可信配置登记最低行为用例并执行该库全部非 ignored 测试；被父用例以独立复制进程调用的既有 ignored helper 保留原功能。首次迁移保留平台源码与测试的原 Chain 路径，由新库的显式 `#[path]` 唯一编译；Chain 仅调用新库，不再编译第二份实现。这样旧 BASE 仍能核对原平台和 ignored 合同。可信 BASE 纳入新库的实际测试入口后才能迁移这些源码路径；过渡期不为原操作系统文件登记窄路径或新增词法豁免。完整回归同时包含独立库与实际宿主组合，迁移 PR 本身接受旧 BASE 的完整必要检查。编译与测试收益分别记录实际 Cargo 闭包、构建和执行耗时，不以目录迁移推定十分钟目标已经达到。Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。

投影承诺树、规范编码和包含证明算法下沉到 `oasis7_client_api::formal_projection`，仅接收序列化投影和显式 audience；世界侧只枚举已授权 audience 并生成投影。通用端点协议、持久化和恢复仍由 `oasis7_distfs` / `oasis7_node` 拥有，链节点只负责世界适配与进程组合，不把这类代码重新放回游戏包。

CI 对依赖影响区分 library 输入与独立目标：包内 `src/bin/`、`tests/`、`examples/`、`benches/` 的变化选择该包自身的检查，不自动沿 library 反向依赖传播；若这些文件被其他目标以静态路径引用，仍按引用者传播。无法确认输入关系时保留保守覆盖。

世界 library、状态序列化、正式读投影与共享构建输入变化继续选择适用普通库/合同验证，不把玩法代码整体视为与正式运行无关；重型 Formal 统一由完整层承接。新增、删除、重命名及测试 fixture 都使用 base/source/tested 的关系并集。

`formal-process` 使用 always 汇总：选中时构建和计划内矩阵均须 success；未选中时矩阵须 skipped，明确输出 not_selected，required-gate 等待此固定依赖。可信 `formal_full_profiles_only_version=1` 启用后，`pr-fast/main-fast` 输出空 Formal 选择，`periodic-full/candidate-full` 保留全部五个逻辑 suite 及跨物理 shard 的完整 case。缺能力的旧 BASE 继续按风险定向选择，控制输入或缺窄映射时保留其原覆盖；可信 base 尚无 formal_process 时仅保守扩大，不改旧计划使旧校验器接受未知组。schedule/manual-full 始终包含全部重型 suite，full-extended 不重复执行其独立链路；专用 package dispatch 的独立用途保持。

进程运行器使用单调绝对期限、20 秒存活心跳及独立进程所有权标识。Linux 扫描同 UID 环境标识并核对进程 starttime，清理包括脱离原进程组的自有后代；macOS 本地兼容使用独立进程组。退出码、超时、实际测试数、清理结果分别保存。摘要产物只含测试名和运行元数据；原输出通过 nocapture 进入 job 日志，不打包临时世界目录。

live 日志背压不得阻塞监督期限、结果落盘或自有后代清理。每个运行器进程仅有一个有界后台转发队列，不修改共享 stdout 的 open-description flags；队列满时只抑制 live 转发，独立 reader 仍以有界 chunk/候选摘要行读取完整子进程输出，仅保留最后一个有效标准 libtest 计数。摘要记录实际读取的 `output_bytes`、被队列明确抑制或丢弃的 `output_suppressed_bytes`、结果生成时尚未确认写出的 `output_pending_bytes` 及读取错误类型，不新增 raw 日志产物，也不以 live 日志是否送达改变测试选择或成功条件。未完整读取的摘要不能证明执行数量。退出成功若到绝对期限后才被监督者观察到，仍记 timeout/failed；结果文件先于最后的 live 结果通知保存。受控背压回归证明这一机制，不代表某次 hosted 停滞的根因已确定。 每次调用独立快照当前输出描述符，排队片段持有独立副本并在转发或抑制后关闭，防止跨调用重定向失配或描述符复用误投。显式普通文件重定向保留完整输出；storage acceptance 通过可选普通文件日志入口保存原有逐 case 日志，独立于可抑制的控制台转发，不经 Tee 回调。该显式日志必须完整读取且无写入错误才可接受，结构化容量及全节点报告仍从完整日志核验；普通调用不新增默认 raw 日志或 spool。

验证方法见[测试手册](../../../testing-manual.md#ci-formal-process-validation)。具体候选、失败、独立评审及 hosted 结果归当前 PR，未完成真实 runner 验证不得宣称产物复用已验收。

### Ordinary workspace library cache

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；沿用[容量要求](ci-tiered-execution.prd.md#ci-formal-cache-budget)。

普通构建使用 `ordinary-workspace-v1`，先恢复共享依赖 target，再按上述内容、完整路径集合和纳秒 mtime 校验叠加本地单位。旧完整范围保留四个 ordinary build variants；新快速计划按 package、Viewer 消费者和 snapshot owner 选择真实变体，并保留对应测试 artifact 语义。缓存只选择非测试 library（包括 proc-macro）及完整 custom-build 编译/执行单位，不把 test harness 或普通 binary 放进这份缓存。缓存记录的默认选集仍是完整单位，Formal 与 ECS 不受 library 过滤影响。每个 variant 分别记录本地与第三方 compiler-artifact 的总数、fresh 和 rebuilt；这些是单位观测数，不是唯一 crate 数。

只读与可信 main producer 共用构建步骤，分别使用只读和可写 cache token；新 profile 仅可信 main 完整运行可保存，旧 BASE 回退保留原 main push 条件。`oasis7-test-build` 汇总两入口：选中时必须恰好一个成功且另一个 skipped，失败、取消或未知结果不能通过；ordinary 消费者和最终完整汇总依赖此结果，`full-extended` 不依赖普通 bundle、可独立并行。显式状态条件避免互斥入口的 skipped 祖先传播，取消后不启动消费者。

保存前测量完整库选集；超过 6 GiB raw 上限时，按 profile/fingerprint 组织完整 Cargo 单元，优先保留最近构建的单元，同时间戳按稳定路径排序，跳过无法容纳的超大单元。必须保留 build-script 支持目录及其 fingerprint、已解析的混合无哈希库相关单元和跨组硬链接涉及的单元；缺失输出、未绑定路径、歧义或不安全状态拒绝保存。保留完整输入快照和包归属，未缓存单元由 Cargo 正常重建并传播失效，不修改 target 或编译、测试选择。该排序是有界打包启发式，不承诺最优复用率。

raw 大小不能代替压缩结果。ordinary 最多执行三次本地完整单元选集压缩试算，按实际 compact 归档和外包装保守上传上界缩小选集，不重试远端上传。最终重新读取完整远端库存，核对 1.25 GiB 单份上传、2.5 GiB 常驻目标、3.75 GiB 临时写入上限、依赖周转预留和全仓预算；仍不满足预算时跳过保存并完成原构建和测试。诊断分别记录完整选集、选中大小、单元裁剪和各次压缩上界；重选阶段的 omitted 数仅描述该阶段输入，不作为原始选集累计数。

所有状态、归档和报告位于 runner 临时目录，禁止 Python bytecode 污染源码扫描。

普通和 Formal 使用共同 workspace 入口及与 ECS 相同的归档、容量策略。准备成本、archive 恢复校验、实际 Cargo fresh/rebuilt 与远端保存确认分开记录。候选可以做只读容量预检和 stat-only 测量，不获得写入或删除权限。首次可信保存、兼容 reader 的实际复用及第三代回收仍须真实 hosted 运行证明；本地 fixture 仅验证机制。

### ECS workspace artifact cache

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7; [cache capacity requirement](ci-tiered-execution.prd.md#ci-formal-cache-budget). Product applicability: N/A; this is build-cache capacity policy, not a player-facing requirement.

The fixed Linux native `ecs` profile uses a separate `ecs-workspace-v2` family. Reuse the complete Cargo unit/fingerprint and source content/mtime checks in `ci-test-artifacts.py`, without its Formal package list. Identity covers toolchain, target, profile, features, flags, Cargo configuration and manifest/lock/helper inputs. Immutable source keys have a separate compatible restore prefix.

Only the current main workflow native writer saves after native success; consumers are read-only and saving does not depend on Web success. The v2 archive has its own restore prefix; a v1 directory is never interpreted as a v2 payload.

The staging logical-byte limit is 6 GiB. A single conservative upload bound is limited to 1 GiB, the two-generation resident target is 2 GiB, and the temporary third-generation write ceiling is 3 GiB.

When no entry has the new compatible prefix, a bounded 4 GiB migration write ceiling permits adoption from the observed three-entry old compatibility inventory; the 1 GiB single-upload and complete repository limits still apply. Once the compatible prefix exists, the normal 3 GiB ceiling applies. Inventory accounts for every ECS format generation under the same family budget.

This entry point never deletes caches. Unavailable full inventory or a conservative projected family or repository size over budget skips saving.

Two generations are the retention target: a temporary third generation may be saved within both capacity budgets, then a separate trusted maintenance job verifies the exact complete current entry and removes only unprotected older generations. Insufficient headroom skips the new save.

The separate trusted recovery path can reclaim extra current-format generations within a verified compatibility/version group, preserving its complete anchor and all activity protections; cross-compatibility retirement requires an explicit confirmed current key or the pressure-only immutable-source ancestry proof above, retaining a stable permanent successor.

Before installing the toolchain or building, `preflight` reads the complete repository inventory and attempts `GET /repos/{repository}/actions/cache/storage-limit`. `stage` repeats both reads immediately before authorizing an upload; the early report is not a capacity reservation.

A valid positive integer `max_cache_size_gb` is conservatively converted using `1,000,000,000` bytes per GB. An explicitly configured `ECS_CACHE_REPOSITORY_BUDGET_BYTES` is a policy ceiling; when the live limit is available the effective budget is the smaller value.

Only a recognized access-denied response from the quota endpoint, excluding rate limits, may use this verified configured budget while leaving the live limit unknown. Invalid responses, incomplete inventory, unrelated API failures and an absent configured budget fail closed.

Diagnostics retain only bounded, allowlisted status/request/permission information; response bodies and credentials are not logged.

The repository's [Actions cache page](https://github.com/nil-studio/oasis7/actions/caches) showed a 10 GB eviction limit when the configured budget was verified on 2026-10-21; the historical numerical fixtures retain that 10,000,000,000-byte policy. The trusted workflows now configure a 20,000,000,000-byte repository budget, unchanged by the Ordinary single-upload increase. These configured values do not claim that every run read the live quota; a lower valid live limit still wins. Maintainers must reverify the setting when the repository cache limit changes.

Reports separately identify the live limit, configured and effective budgets, capacity basis, observed inventory and projected usage. The entry point never changes repository limits or grants administration permissions.

Stage the validated units outside the checkout, preserving internal hard links, then produce the single `workspace-units.tar.gz` cache payload with flat regular-file entries. Keep the logical and unique sizes for staging diagnostics.

Capacity checks use the measured compressed payload plus a conservative bound for the actual outer tar/compression wrapper used by `actions/cache`, not raw logical bytes or an assumed compression ratio.

The wrapper guard validates GNU tar and GNU gzip and rejects inherited TAR_OPTIONS/GZIP overrides. It measures the actual outer tar records, then takes the larger safe zstd streaming or GNU gzip CLI bound (including block/header slack), rather than a generic zlib bound for unrelated deflate parameters.

The payload must remain the same single-link regular file before and after measurement. Upload ceilings are 1.25 GiB for ordinary, 768 MiB for Formal debug, 256 MiB for Formal endpoint and 1 GiB for legacy Formal; all repository/family budgets are checked independently; real gzip and zstd wrapper regressions check the bound independently.

The complete all-ref/all-family inventory plus this upload bound must fit the effective repository budget. Separately, the main ECS family inventory plus the bound must fit the ECS family budget.

Restore rejects unsafe paths, links, duplicate entries and excessive expanded data before invoking the existing manifest/hash/source-mtime validation. A rejected archive falls back to the normal full build. Record pack/unpack time independently from Cargo time.

Capacity remains a point-in-time check: concurrent writers and later dependency-cache post-job saves may still affect eviction; this guard does not reserve capacity.

During source scanning all native staging, state, monitoring and reports live in runner temporary storage. Disable Python bytecode generation before the package helper imports any repository module, so a producer cannot add `__pycache__` inputs absent from a clean consumer. Do not broadly exclude untracked or ignored inputs.

Misses, corruption and unavailable cache services fall back to the original full build. Candidate native jobs run `stage --preview` to measure the same archive and capacity policy while always emitting `save=false`; the actual upload remains restricted to the trusted main writer.

Record the requested/matched key, cache-hit, validated restoration, Cargo unit Fresh/rebuilt, restore/save duration, bytes and final package identity independently.

After the save action, `observe-save` confirms a unique, positive-size exact main entry through a new complete inventory; a successful action alone is not a confirmed save. Upload diagnostics after this observation.

Successful trusted main producer and compatible consumer samples remain required remote evidence, obtained only after actual runs. Candidate workflows retain read-only cache access throughout validation.

### Controlled graceful-stop deadline regression

The runtime-protocol suite registers one real-owner controlled regression for admission and Node-worker budget consumption. With a short test-only timeout, a held admission gate must return retained Pending before release without creating a late worker; a later retry keeps the same owner and exactly one worker until real completion. Join-only Pending, unstarted/already-stopped stop(0), poison and cleanup failure remain distinct. This regression does not increase any existing caller or production stop timeout. Artificial blocking proves this mechanism rather than any historical CI root cause; measured return overruns include OS scheduling. No diagnostic workflow, observer or production timeout is enabled by this registration.

Controlled crash-publication shutdown must distinguish a zero-budget Pending before worker creation from a Pending while a worker owns the Node. It advances that same owner with a positive interval bounded by the existing scenario/10-second observation deadline before asserting worker ownership; zero-budget Pending alone never authorizes release or successful completion. No retry creates a replacement worker or renews the scenario budget.

## Worker 生产编译的精确平台组合

Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；沿用快速准入、完整 feature/平台覆盖和可信基线要求，不新增游戏权限。

生产模块名含 `test_session` 不等于 Rust 测试，但其新增 cfg 必须证明实际可编译。可信 planner 与可信 CI driver 共用封闭 Worker wasm 编译组合：四个角色分别单独编译，另分别编译 Owner+approval-harness、Identity+native-runtime-harness。每条均为 `wasm32-unknown-unknown`、`--no-default-features`、`--locked` 的独立命令；不合并互斥角色。六项已有主线平台 driver 先例，可信 driver 继续执行原 `verify.sh --platform` 并直接执行同一精确编译命令库存，使新候选不能仅凭声明一个 feature 或脚本名取得证明。

该证据只用于同属 Worker 的静态、无环、无 opaque 生产模块路径，要求父路径与局部 cfg 在同一个实际组合全部成立。未声明 feature、未知谓词、矛盾角色、跨包路径及不可证明的 mount 保持拒绝。编译时 `test=false`；该库存不执行或授权 wasm `#[test]`、ignored 用例，不改变原 native/expanded 测试组合和忽略用例 runner 登记要求。新增平台组合须独立审查并先进入可信 main；候选自行修改 planner/config 不成为该候选的授权来源。

第七项作为新引入的封闭条件合同预登记，不能称为既有主线编译证据：仅当 Worker manifest 声明空、默认关闭的 `execution-signer-harness=[]` 时，可信 driver 才追加 Executor+execution-signer-harness 的成功编译，并分别验证 Identity、Owner、RecoveryFence+该 feature 的失败编译及精确 guard 消息。feature 缺失时不执行这四项，也不声称已编译。每个 tuple 都以实际 manifest 的本地 feature 传递闭包，同时核对父路径和局部 cfg；不注入 default，不跨 tuple 合并。非空声明、default 闭包开启，或六个原组合通过角色或 harness 别名隐式开启该 feature 均直接拒绝；无法建模的 optional/weak/implicit 依赖传播不能推定为已编译。失败编译仅证明精确字面 `compile_error!` guard，不能证明其后的任意生产函数、模块或测试可执行；真实成功组合仍逐实际 parent/cfg 核对。此前所有新增 ignored 用例的 runner 缺口保持独立阻断。

### Preview provider 测试附属产物

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；沿用共享产物合同。本能力不登记新的 ignored 测试，也不改变可信 BASE 的选择权限。ordinary expanded／full producer 在实际 native 库存包含启用 `test_tier_full` 的 `oasis7_chain_runtime` 测试消费者时准备材料；不为仅含其他包的隔离编排夹具启动模块构建。准备仍使用官方 `scripts/build-wasm-module.sh --candidate-build`，按同 checkout 构建 `module.runtime.local-test-provider`；原 WASM、官方 metadata 与 native bundle 的 tested/source SHA、两个文件及官方 build receipt、同 candidate 只读 checker 的 hash 一并绑定。checker 复用既有 `sync_builtin_wasm_identity --verify-provider-artifact` 入口，使用官方 SourceRule2 完整闭包、builder recipe／execution 和 receipt 验证 API；不修改 producer recipe，不同步身份或生成发布材料。允许现 API 的 locked Cargo metadata 解析，不启动编译或模块执行。pack／load 验证完整材料、当前 source-input v2 和身份后，才向实际测试进程显式注入 `OASIS7_PREVIEW_TEST_PROVIDER_WASM` 与 `OASIS7_PREVIEW_TEST_PROVIDER_METADATA`。缺失、损坏、跨 source 或路径越界均失败，不回退 placeholder、其他 checkout 或本机缓存材料。`ci-tests.sh full-extended` 使用同一官方准备及验证入口；不新增 secret 或外部写权限。纯 Python 合同不代签真实 WASM 构建或 consumer 执行；后续实际 full-tier consumer 必须在材料缺失时失败而非 skip。

Viewer performance 固定浏览器安装仍保留 `agent-browser@0.37.1 install --with-deps` 的完整依赖。该第三方入口自行执行 APT update，不能继承 helper 的命令预算；在临时 runner 中为 APT HTTP/HTTPS 传输设置 15 秒超时、禁止内层重试，并用 15 分钟步骤期限约束整个安装。来源、签名和摘要校验保持原约定，安装失败不得开始测试。
