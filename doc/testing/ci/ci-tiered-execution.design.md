# CI 分级执行

- 状态：active
- 专业 owner：`repository_health_engineer`；CI 控制、来源与缓存边界由独立评审者审读
- 通用规则：[开发流程规范](../../engineering/workflow/source-of-truth.md)
- 上游要求：[CI 分级执行 PRD](ci-tiered-execution.prd.md)
- 可执行测试命令和平台限制：[testing-manual.md](../../../testing-manual.md)

## 测试范围

普通 CI 是默认验证方式。成功规划后轻量 baseline 始终执行；选择器使用完整 Git name-status diff、base/head 两侧 Cargo 关系及必要目录映射选择 Rust、Web、场景、平台、打包和运维合同测试。

PR 测试合并提交时，选择范围合并两组差异：实际 target/source 的唯一共同祖先到 source HEAD，以及实际 target 第一父提交到 tested merge。main 在事件 base 之后已存在的改动不重复计入；PR 自身变化和合并结果额外变化仍覆盖。校验 tested merge 恰有两个父提交、第二父提交等于 source HEAD、事件 base 是实际 target 的祖先；父提交身份不符、共同祖先不唯一或对象不可读时扩大为普通全量 required。依赖及嵌入输入分析保留事件 base、共同祖先、source、实际 target 与 tested merge 的图。push/source-only 检查继续使用事件 base 到 source 的完整差异。

Rust 变化覆盖自身及需要验证的反向消费者，删除和重命名覆盖两端；保留有效 features、WASM、生成输入和平台组合。workspace、features、锁文件、共享生成器及 CI 控制输入变化采用保守普通 required，图解析失败仍扩大覆盖；未登记路径按下述门禁阻断。

`./scripts/ci-tests.sh required` 是普通必要测试的本地入口；`full` 保留广覆盖回归，nightly 执行适用 full。真实产品测试、WASM 确定性、共识、持久化、安全、升级恢复和跨平台打包均继续保留，不因流程精简删除。

Command Center 的独立 Cargo workspace 由 `command_center` 组承接，工具目录变化选择 baseline 和该组；共享 context、local signer、secure envelope crate 变化也选择该组。Rust workflow 调用可复用的 `command-center.yml`，保留 Core、生成类型、UI、Clippy 和 macOS App 打包，并将调用结果纳入 `required-gate`。专用 workflow 不再独立触发 PR/push，避免重复执行；Cargo manifest、锁文件和 CI 控制变化仍采用保守全量。该平台组的本地验证使用专用 workflow 中的 canonical 命令，不由通用 shell driver 执行。

文档 corpus 对象、索引及 evidence inventory 数据更新由始终执行的 baseline 文档治理检查校验一致性；三平台 corpus/evidence 工具回归只在检查器实现、fixture、Git 路径配置、scope 配置或平台 workflow 变化时触发，数据更新不单独触发三平台回归。

<a id="ci-unknown-inputs-design"></a>
### 未登记路径的失败链路

Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；承接[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs)。

选择器完成 Git/Cargo 与显式规则匹配后收集全部未登记路径，抛出独立的 `UnknownInputsError`，不被现有“输入不确定则全量”的捕获分支吞掉。CLI 返回非零并以转义 JSON 路径列表报告补登记提示，不写成功 plan 或 GitHub 组输出；共享控制输入视为已登记的全量规则，不能因没有 Cargo owner 误判为未知。混合变更中的已知全量输入及 `--add-group` 均不能覆盖未登记路径失败。

所有重任务沿既有 `select` 成功条件启动；轻量 formal 汇总和最终 `required-gate` 保留 `always`。最终 gate 在下载规划产物前先确认 `select` 成功，失败时立即给出范围规划诊断，因此不依赖失败路径上未上传的 artifact。trusted baseline 来源保持不变；新独立目录先通过规则/验证入口登记 PR，再提交使用该目录的功能 PR。新增测试使用真实 Git diff/CLI 验证未知新增、删除、重命名两端、混合全量输入、无成功输出及失败汇总；已登记路径、共享输入和图解析全量回退保留正例。

## 调度和结果

select 先产生明确的非空逻辑组列表，按资源组并行执行。普通组使用原生 job，fleet-health 使用固定 Linux/Windows/macOS 矩阵。最终唯一 `required-gate` 使用可靠的 always 汇总，并显式依赖 select 和所有组。选中组必须 success；未选中组可以 skipped 或 success；失败、取消、缺失、未知及意外 skipped 都必须阻断。fleet 矩阵不吞退出码，采用 fail-fast=false。

记录 source HEAD、base 和实际测试对象及运行链接。普通 PR 权限只读，从同一事件 BASE 提取可信选择器、scope 配置与结果汇总器，候选 checkout 的 driver 执行真实测试；候选控制变化不能缩小基线覆盖。CI 控制、安全或兼容边界变化接受对应能力的独立评审。

## 专项条件

缺测试先补普通 CI。只有普通 runner 难以提供的真实长期状态、专用硬件或受控环境才使用专项验证，说明具体不足、环境、版本、通过标准和实际结果。阻塞专项纳入同一 PR 最终汇总；调试 dispatch 不能替代 PR 检查。

CI 结果对应实际 source HEAD。main 前进或分支显示 `BEHIND` 本身不使该结果失效；只有合并冲突或 main 的相关变化带来明确集成风险时，才按需更新分支并验证实际组合。source HEAD 变化后重新运行必要普通 CI，复审实际变化与受影响部分；具体合入规则遵循开发流程规范。产品层的签名、checkpoint 和状态一致性证明仍按专业合同验证。

CodeQL 技术扫描见 [CodeQL 设计](codeql-integration.design.md)，实际 required/advisory 按 GitHub CI 与生效保护执行。

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

<a id="ci-case-tier-design"></a>
## Test case 分级专题的共享设计

本节承接 [testcase-tiering PRD](ci-testcase-tiering.prd.md#case-tier-requirements)。该 PRD 继续
拥有 `test_tier_required` / `test_tier_full` 的标签语义；命令级触发、影响选择和汇总由本设计及通用规范拥有。

- **分配与选择**：新增或迁移用例时核对 cfg/feature 标签、Cargo feature 闭包、driver 参数与实际选中列表。
  标签存在但 runner 未执行不是覆盖；同名 case 在不同 target/features 下不能互相代签。
  required 选择足以检出受影响关键缺陷的用例，full 保留适用 required 及重型回归。
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
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | 离线协议、full/wasmtime、降级目标及失败范围 | [覆盖专题](#ci-coverage-design) | CI 维护者与 QA | 不将未接线 snapshot 或文档合并计为覆盖 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | 独立标签语义到实际选中 case 与结果 | [Case 分级](#ci-case-tier-design) | 测试维护者与 CI driver | 不修改断言或扩展任务准入 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | 保留产品测试并按真实影响选择 | [测试范围](#测试范围) | 选择器与执行器 | 解析不确定扩大普通 required，未登记路径另按门禁阻断 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| Trace refs: O7-REQ-4e7f2e0d-9147-43d9-a593-00a8db9b21df；[未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | 所有未登记路径阻断规划，失败到最终 required 汇总，保持可信规则来源 | [失败链路](#ci-unknown-inputs-design) | 选择器、Rust workflow 与结果回归 | 产品适用性：N/A；reason=CI 范围登记治理，不改变游戏行为；scope=未登记路径的规划失败、可信基线与 required 结果传播；本地失败回归不代签 hosted 运行。 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | 最终汇总真实结果 | [调度和结果](#调度和结果) | workflow 与结果函数 | 本地回归不证明 hosted 运行成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | 普通 CI 优先，专项需具体环境不足 | [专项条件](#专项条件) | 对应产品运行场景 | 不新增通用 PM 集成链 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | Linux x86_64 冷热路径共用摘要与版本验证的预编译安装器 | [安装流程](#ci-install-trunk-design) | Release workflow 与共享安装器 | 工具成功不等于 Web 构建或发布成功 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | 对齐归档身份、可信 main writer 与只读消费者，验证真实 scope | [缓存边界](#ci-install-cache-design) | GitHub cache 与 main full-regression | 静态 key 相同不证明跨 tag 恢复 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | 先量测；需要治理时约束总预算、有限重试及受控子进程清理 | [apt 评估](#ci-install-apt-design) | 临时 CI runner 的依赖准备 | 本次不实现未经数据确认的 apt 策略 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | 分阶段、分冷暖记录可比较的准备成本与失败样本 | [耗时证据](#ci-install-measurement-design) | 实际 Actions 运行与维护者分析 | 不由 Cargo 指标或本地 fixture 推断发布提速 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | full 包含适用 required；真实消费者、wrapper 结果和扫描证据可核对 | [执行清单与结果](#ci-real-coverage-design) | CI driver、构建 wrapper、QA；CodeQL 保持独立主责 | 不把 fake 工具、旧运行或文档当成实际执行 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | 完整 TOML 集合、真实日期、范围棘轮和逐项有界续验 | [例外检查](#ci-advisory-baseline-design) | repository_health_engineer 与安全/兼容审读 | 不修改当前有效期、批准列表或依赖范围 产品 N/A；理由：CI 专业验证要求；范围：本行列明的测试与工具链。 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | 普通/重型分离、一次构建、五分片及失败汇总 | [进程调度](#ci-formal-process-design) | CI 构建与原生进程运行器 | 产品 N/A；理由：CI 专业调度优化；范围：普通检查与真实进程验收，不改变玩家行为。 |

## 3. 依赖准备的基线与目标

2026-10-12 审读基线是 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`。本次采用 Release 复用现有安装器的最小改造；真实 hosted 构建、main 缓存写入、跨 tag 恢复及节省时间分别以实际运行记录确认。apt 保留观察与条件实施要求。

| 对象 | 审读基线事实 | 本设计的变化与限制 |
| --- | --- | --- |
| Release Web 工具准备 | [原工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/release-packages.yml)恢复 `~/.cargo/bin/trunk`，未命中时 `cargo install trunk`。 | 恢复预编译归档并无条件调用共享验证安装器，去掉重复源码编译。 |
| 普通 CI 与 main writer | [Rust 工作流](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/.github/workflows/rust.yml)已使用归档；main schedule/manual-full 的 `full-regression` 可写入。 | 复用已有 writer、路径与 key，不新增预热工作流。 |
| 安装器 | [原安装器](https://github.com/eng-cc/oasis7/blob/fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e/scripts/install-ci-trunk.sh)固定 Trunk 0.21.14、官方归档摘要及 Linux x86_64。 | 继续由该脚本集中拥有版本、来源、摘要、平台与失败规则。 |
| apt | 原工作流直接执行 `apt-get update/install`，本轮没有当前卡死或遗留进程的运行证据。 | 先核实阶段耗时与失败，再决定预算和重试参数。 |

<a id="ci-install-trunk-design"></a>
## 4. Release Trunk 安装流程

消费者是 [Release `build-web-dist`](../../../.github/workflows/release-packages.yml)，支持范围是现有 Linux x86_64 runner。源码身份继续由 `package-source-plan.py` 一次解析，job checkout 固定的计划提交并核对 HEAD；工具优化不改变原有源码、BUILDINFO、构建验证或发布时 tag 复核规则，完整边界见[包来源与缓存安全规则](../../engineering/workflow/package-source-security.md)。

1. 在计划源码校验后确认 `scripts/install-ci-trunk.sh` 是非空普通文件且不是符号链接；缺失、为空或为符号链接时明确失败，不能用空安装器 hash 恢复缓存，也不从其他 ref 临时借用脚本。
2. 使用固定提交的 `actions/cache/restore` 恢复同平台归档。该步骤只恢复，不为 Release 增加 Trunk 缓存保存；不把缓存命中作为跳过安装器的条件。
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

现有 main `full-regression` 的 schedule/manual-full 是共享归档 writer；Release 使用只恢复 action，普通 PR 继续受只读 cache token 约束。其他 Release/Rust 缓存的 key、保存策略和权限保持其现有合同。不得增加宽泛 `restore-keys`、扩大缓存写权限，或把缓存内容当作可执行来源的认证。

缓存是可回收的加速数据，没有发布状态、事务或持久化权威。main 尚未保存该身份、缓存被回收或 reader 不可达时，正确的冷下载仍可完成工具准备。恢复服务异常须保留实际 action 结果；若 action 允许继续且归档不存在，安装器仍执行同一校验冷路径，不用忽略必要安装失败来维持 job 绿色。

跨 tag 复用的验证必须关联一个可确认的 main 保存记录及两个不同 tag 的实际 reader，记录 run/job、event/ref、请求和实际匹配 key、平台与 cache version 兼容条件、恢复及摘要验证结果。仅同 tag rerun 只能证明同 tag 恢复；未取得上述记录时保留“配置已接通、真实跨 tag 恢复待观察”。沿用自然发生且已授权的 main/full 与 Release 执行积累数据，不为本项新建发布事件或 required check。

<a id="ci-install-apt-design"></a>
## 6. apt 的有界安装评估

先从现有 job 日志区分索引更新、下载、安装与锁等待，记录 runner image、阶段耗时、失败类别及实际重试；日志粒度不足时由维护者在对应依赖步骤补量测。当前没有依据直接规定统一超时秒数、尝试次数或重试所有错误。

若观察到足以实施的长尾或取消清理问题，采用一个可单测的 helper 管理整个依赖准备预算：每次执行、终止宽限、退避与后续尝试都计入有限总预算。仅对可确认的瞬态失败重试；缺包、错误参数、权限/配置、签名或完整性失败不靠重复调用掩盖。

超时与取消针对本次启动的受控进程组，先请求终止，必要时在有界宽限后强制终止；确认该组及其受控子进程退出后才可结束或开始下一次允许的尝试。不得删除锁文件、终止无关进程，或把父 shell 退出当作清理完成。若 dpkg 已开始修改且被强制中断，不能盲目重复 install；无法证明状态可继续时让 job 失败，由新 runner 重试。

该后续设计仅面向临时 CI runner，不直接推广到开发机器或部署主机。实现时用假命令与进程 fixture 验证挂起、忽略终止信号、父进程先退出、瞬态/永久错误、预算耗尽及取消，不对真实 apt/dpkg 故障注入。未实施或未执行的场景不得写为已验证。

<a id="ci-install-measurement-design"></a>
## 7. 耗时与运行证据

维护者从实际 Actions 日志分别记录缓存恢复、官方下载、摘要校验/提取/安装/版本校验及总准备耗时；writer 的保存成本另列。当前日志可提供 action/step 级时间，缺少子阶段时间时标明粒度，不把合并步骤的时间伪拆成精确阶段。需要更细比较时再在同一安装路径补计时。

| 样本类型 | 判定依据 | 记录边界 |
| --- | --- | --- |
| 冷启动 | 没有恢复归档且本地归档不存在，实际执行下载 | 包括恢复尝试、下载与验证安装 |
| 暖缓存 | 从 GitHub 恢复对应归档并通过校验，未调用下载 | 包括恢复及验证安装，不跳过校验成本 |
| 本地已有 | 同 job 已存在归档且验证通过 | 单独列出，不充当跨 run 命中 |
| 恢复异常后下载 | action 异常后仍能按冷路径取得正确归档 | 单列异常与额外成本，不混入正常冷启动 |

每组记录 runner OS/image、架构、Trunk 版本及归档摘要、安装器指纹、source、event/ref、run/job 链接、key、样本窗口与数量。失败和取消保留原因及耗时，不能仅挑成功样本。总发布耗时、排队时间、Rust 编译与 Trunk 准备时间分别报告；Cargo cold/no-op 量测不覆盖本节的网络安装与 cache scope。

样本不足时报告单次观测和环境差异；至少三次可比观察后可以报告样本数、中位数及范围，仍不外推稳定性能承诺。当前不设仓库统一 SLO、预写提速比例或新增性能门禁。实现接线正确、本地合同通过、hosted 构建成功与真实缓存收益是分别验证的结论。

## 8. 兼容、启用与回退

实现随普通 PR 合入生效，实际 Release 使用哪个 workflow revision 和 source commit 由原有计划合同确定。计划源必须包含受支持的共享安装器；早于该安装器的历史 tag 在新 workflow 下会明确失败。需要重发历史版本时，先审读相应源码和工具兼容性，不能用移动 tag、任意 ref 脚本或源码安装回退绕过这一边界。

缓存内容无需迁移；旧 Release binary 缓存可以自然回收，首次使用新身份的冷成本单独记录。回退通过正常 PR 恢复已知工作流基线及配套合同测试，仍须独立评审和适用 CI；不在运行中自动选择另一种安装策略。apt 的条件设计不改变当前运行参数。此改造不涉及玩家接口、世界状态或游戏 Runtime，无需 Agent 游戏实玩验证。

<a id="ci-real-coverage-design"></a>
## 9. 执行清单、真实消费者与结果

本节承接[真实覆盖合同](ci-tiered-execution.prd.md#ci-real-coverage)，是后续实现要求；不将清单设计、已有工具接线或历史绿色结果视为全部覆盖已交付。

### 9.1 required 到 full 的可校验包含关系

对固定 source 和环境/平台/features/target/影响范围，生成适用 required 清单 R 与 full 清单 F，验证 R 是 F 的子集。清单来自同一检查注册表或实际执行入口的可核验导出，不能维护一份与执行代码无关的手写“已覆盖”清单。每项至少能核对检查身份、命令与参数、前置依赖、平台/feature 适用条件、退出结果；同名但运行不同 feature/target 的命令不能互相替代。

full 可以按依赖图合并重复构建、并行独立测试，但不删除 Clippy 或独立 Viewer browser smoke 的实际调用。browser smoke 之前必须完成其所需依赖和真实构建，失败不执行使用陈旧产物的验证。单个命令失败沿原 suite/job/required-gate 传播；总测试数、成功构建或后续命令的零退出码不能覆盖它。可信 base 选择器与候选控制输入不缩小基线覆盖的规则保持不变。

对不支持的环境明确标记不适用并指出实际受支持入口；缺工具、缺 target、未执行或无法判定列为阻塞/失败/未知，不能装成不适用或通过。回归故意删去 full 中一项 required、修改其 feature/target 和令各命令返回非零，确认包含关系和失败传播均生效。这些回归进入既有检查，不新增身份门禁或 PM 复验链。

### 9.2 WASM 与 wrapper 分层证明

维护实际 native/WASM 消费者到 stable smoke、确定性构建入口、输入/产物及 hash 核验的映射。fake Docker 只检验 build wrapper 传参和错误传播；真实 builtin 容器构建与 hash 比较独立执行。`build-wasm-module.test.sh` 有测试文件不等于被 CI 调用，应能从实际选中 job 追到该命令和结果。缺必要 target 或把所有 lane 改为 `BUILD_STD=0` 不能压掉真实消费者；明确已退役且无消费者的旧 lane 不重新激活。

构建 wrapper 在启动子命令前冻结一个有效 target-dir，并区分 wrapper/Cargo 参数与 `--` 后的应用参数。保留含空格参数及 sccache 开/关、环境与显式参数的既有边界；冲突输入统一解析或明确拒绝。退出清理使用保存的 child exit status，释放自己持有的 lease；仅更新仍存在且归属有效的元数据，不能在 target 被合法删除后重新 mkdir 或用记账错误遮蔽原退出结果。不得以保退出码为由遗留占用/GC 锁。

原生 Windows 用两个真实进程验证超过平台默认短重试窗口的锁等待、释放后只执行一次、等待可取消、非阻塞模式及非争用 I/O 错误立即失败。WSL、fake lock 或短等待仅证明各自条件；这些合同不授权清理其他 worktree 或未知缓存。

### 9.3 CodeQL 结果是独立证据链

沿用 [CodeQL 专项设计](codeql-integration.design.md) 的语言、查询、预算与启用决定。每个适用语言/查询组合分别核对执行终态、上传/analysis 绑定及 finding 处置，使用准确 source、ref、run、attempt、job、category 和 App。API 分页在总预算内完整读取；后页失败、重复游标或预算耗尽不发布完整结论。缓存结果与不匹配运行不能补齐缺失证据。

修复结果发现/绑定不意味着 CodeQL observe 已启用；扫描超时也不直接授权延长预算、删除查询或新增 required gate。专项执行、依赖兼容复核和 RustSec 到期处置独立推进，不让一个广泛 umbrella 的等待阻止有明确期限的安全维护。

<a id="ci-advisory-baseline-design"></a>
## 10. RustSec 语义解析与例外生命周期

本节承接[RustSec 基线要求](ci-tiered-execution.prd.md#ci-advisory-baseline)。当前值由 `deny.toml` 和 `scripts/rustsec-ignore-direct-dependency-baseline.json` 拥有；本设计不更新日期、advisory、依赖或批准范围。

检查流程为：读取配置并以标准 TOML 解析器取得完整 `advisories.ignore` -> 验证集合类型、每个 ID 和重复项 -> 从受支持的注释布局提取元数据并建立唯一对应 -> 对照批准集合和 direct manifest 基线 -> 日历日期及有效期校验 -> 执行原依赖路径验证 -> 通过或带定位信息失败。不能先从正则抓到的 ID 子集推断真实 ignore 集合。

TOML 中单引号字符串、引号表名和同行元素均以解析后语义为准；注释里的 ID 不进入启用集合。元数据仍可限制为现有清晰布局，但存在多个候选对应、同一元数据覆盖多个条目、缺/重复元数据或不支持布局时明确拒绝。解析成功与治理批准是两步：合法 TOML 中新增未批准 advisory 仍拒绝。集合相等检查不能先去重而丢掉重复条目诊断。

使用日期类型解析所有参与比较的日期，包括测试用 today；拒绝不存在的月/日及不合法闰日，保留现有到期日比较包含关系。`reviewed_at` 与期限先验证合法，再作原有策略检查，不以格式正则或字典序替代日历合法性。测试分别覆盖到期当天与次日，不把修复校验器当作续期。

每项例外到期前的处理记录须明确：当前依赖路径及批准 manifest 是否变化、是否有可行移除/替代、保留理由、负责 owner、实际验证和有界下次期限。仅更改日期、复制旧 cargo tree 或扩大 direct manifest 列表不构成续验；无法取得所需证据时保持未完成并按原检查结果处理。已删除例外不因模板、解析重写或历史测试样本重新进入生产 allowlist。

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计 | 验收内容 | 验证来源 | 实际记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| [Coverage requirements](ci-test-coverage.prd.md#coverage-requirements) | [覆盖专题](#ci-coverage-design) | 同候选/环境/features 下离线消息链、full 包含关系、失败及未接线目标 | [测试手册](../../../testing-manual.md)、实际 driver/选中 job；按本节选择正反例 | 实际命令、参数、case、退出与缺口；本次未执行 | 不由 selector fixture、skip 或旧 PASS 代签真实覆盖 |
| [Case-tier requirements](ci-testcase-tiering.prd.md#case-tier-requirements) | [Case 分级](#ci-case-tier-design) | 同候选标签到实际执行；漏标、错 feature、零匹配和失败传播 | [测试手册](../../../testing-manual.md)、实际 CI driver；结果函数回归 `scripts/ci-required-result.test.py` 仅作补充 | case/feature/target 与运行结果；本次未执行 | 无 hosted 或性能结论，不改 required/full 保护 |
| [测试范围](ci-tiered-execution.prd.md#测试范围) | [测试范围](#测试范围) | 删除、重命名、依赖和未知输入覆盖 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 命令退出码及输出 | 不替代真实产品组运行 |
| [未登记路径门禁](ci-tiered-execution.prd.md#ci-unknown-inputs) | [失败链路](#ci-unknown-inputs-design) | 未登记新增/删除/重命名、混合全量控制、候选豁免不能放行；无成功输出且 gate 明确失败 | [选择器回归](../../../scripts/plan-rust-required-scope.test.py) | 当前 PR 中保留选择器、工作流和结果回归的命令、source HEAD、退出码和 Actions 结果 | 本地 fixture 不证明 hosted 运行；Git/依赖解析不确定仍保留全量 |
| [调度和结果](ci-tiered-execution.prd.md#调度和结果) | [调度和结果](#调度和结果) | 失败、取消、缺失和跳过均正确传播 | [结果函数回归](../../../scripts/ci-required-result.test.py) | 回归输出与 Actions 链接 | hosted 图仍需独立核对 |
| [专项条件](ci-tiered-execution.prd.md#专项条件) | [专项条件](#专项条件) | 适用环境和真实场景执行 | [测试手册](../../../testing-manual.md) | 所测版本和实际环境结果 | 未执行的环境保持未证明 |
| [Release Trunk](ci-tiered-execution.prd.md#ci-install-trunk) | [安装流程](#ci-install-trunk-design) | planned source、只恢复归档、冷热均调用验证安装器、必要失败传播 | [Release 接线回归](../../../scripts/release-packages-trunk-cache-contract.test.sh) | 候选版本、命令退出码及回归输出 | 不替代真实下载、Web 构建或发布 |
| [缓存复用](ci-tiered-execution.prd.md#ci-install-cache-evidence) | [缓存边界](#ci-install-cache-design) | main writer 与不同 tag readers 的真实身份、恢复及验证 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | writer/reader run/job 链接与实际 key | 未 seed 或未执行 reader 时仍未证明跨 tag 命中 |
| [apt 预算](ci-tiered-execution.prd.md#ci-install-apt-budget) | [apt 评估](#ci-install-apt-design) | 量测后选定参数；实施时验证总预算、分类重试、超时/取消无受控遗留进程 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 实际 apt 观察及后续 helper 的 fixture 输出 | 当前未实现，不推断宿主或部署机适用性 |
| [量测与判定](ci-tiered-execution.prd.md#ci-install-measurement) | [耗时证据](#ci-install-measurement-design) | 相同工具/平台下分冷暖计入准备与缓存成本，保留失败样本 | [依赖安装核验](../../../testing-manual.md#ci-dependency-installation-validation) | 原始耗时、样本窗口/数量和条件差异 | 不以本地测试宣称 hosted 性能收益 |
| [真实覆盖](ci-tiered-execution.prd.md#ci-real-coverage) | [执行清单与结果](#ci-real-coverage-design) | 漏项/错误传播、真实 WASM 消费者、wrapper 退出及 Windows 锁边界 | [测试手册](../../../testing-manual.md)；实际 CI driver、构建/wrapper 回归和原生平台运行 | source、计划清单、命令与真实结果 | fake 工具和计划存在不代签执行；CodeQL 启用另验 |
| [RustSec 基线](ci-tiered-execution.prd.md#ci-advisory-baseline) | [例外检查](#ci-advisory-baseline-design) | 完整 ignore、元数据对应、日历/期限与 direct manifest 漂移 | [基线回归](../../../scripts/check-rustsec-ignore-baseline.test.sh)及真实依赖查询 | 正反例、当前依赖路径与逐条例外处理 | 文档不续期；fixture 不证明实际依赖已移除 |
| [共享产物](ci-tiered-execution.prd.md#ci-formal-process) | [进程调度](#ci-formal-process-design) | 独立分片、精确清单、同源产物、旧基线扩大、错误与超时传播 | [分片核验](../../../testing-manual.md#ci-formal-process-validation) | 当前 PR 的真实 runner、测试与评审结果 | fixture 不代签完整产物消费；未完成 hosted 验证保持未证明 |

## 12. 决策与残余风险

历史 [Issue #1808](https://github.com/eng-cc/oasis7/issues/1808) 中的旧 PM 工具问题随 [#4354](https://github.com/eng-cc/oasis7/pull/4354) 退出当前实现；本设计保留可适用于现行 CI 的依赖准备与缓存问题，不恢复旧任务或验收状态机。选择共享预编译安装器，是因为现有实现已集中拥有来源、摘要和版本检查，可以直接去掉 Release 的重复源码安装路径。

`repository_health_engineer` 维护安装器、缓存接线及 apt 观察；`qa_engineer` 协作核对真实运行证据。主要残余风险是外部下载服务可用性、main seed 尚未产生、缓存被回收以及旧 tag 缺少安装器；这些情况分别按既有失败语义或正确冷路径处理。版本、平台、来源策略或 cache action 更新时重新审读受影响合同；没有实际 hosted 数据时保留未知项，不从文档状态推导发布成功。

<a id="ci-formal-process-design"></a>
## 真实进程验收的边界与结构

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7

承接[共享产物要求](ci-tiered-execution.prd.md#ci-formal-process)。`ci-test-artifacts.py` 从 Cargo JSON 的 executable 字段收集可执行文件，并区分 formal、ordinary 与兼容本地入口的 all 构建范围。正式最小 producer 仅构建 required-feature 的 oasis7 library / chain-runtime binary harness、production runtime、release endpoint 及 default-feature distfs library harness；`oasis7-test-build` 保留普通 required 全包、Viewer 和 default-feature snapshot 的原变体。两个构建范围独立依赖 select，正式矩阵不等待普通 producer 完成。

手动 `formal_stop_diagnostic` 模式复用同一 formal producer、产物身份校验和进程清理 runner，
只执行已登记的 endpoint stop/restart 精确用例一次，并打开已有阶段计时标记。
该诊断入口不替代 required suite、不改变普通选择或汇总策略；即使通过也只证明该次诊断范围。
两处 stop 等待仍为 10 秒，单用例外部 watchdog 为 120 秒；构建与整个 job 使用独立有限预算。
`formal_stop_consumer_diagnostic` 使用独立的只读 producer job 构建并发布原 runtime-protocol 闭包，
在另一 fresh consumer job 校验同 run/source/tested 身份和 binary hash 后仅运行该精确用例；
producer/build/consumer 上限分别为 75/45/15 分钟，不在 consumer 编译或运行其他 shard。
两种诊断均不自动重试；fresh consumer 的一次通过不解释历史失败原因。
stop 诊断的同一 owned operation 时钟从 caller stop 入口开始，记录合并的 request-stop 完成/
admission-wait 开始、admission 获锁、spawn 时剩余 deadline 及 worker 阶段；重试等待沿用该时钟与总计 10 条上限。
为容纳入口覆盖，不再单列 runtime-mutex 等待开始和 observer 前后标记；
spawn→mutex-acquired 与 join-end→worker-completed 仅提供对应合并区间，不能拆分归因。
所有标记仍仅 test binary、精确 flag `1` 生效；remaining 为原 deadline 的只读快照，不刷新预算。
成功路径为 entry、request-end/admission-begin、admission-end、spawn、mutex-end、join-begin、
join-end、worker-completed、wait-completed 共 9 条，Pending 后完成最多再加首次 Pending 共 10 条；
worker error 将 worker/wait 的 completed 换为 error，省略未到达阶段，不打印原 error。
固定提交、toolchain、可执行文件哈希、构建耗时、用例 stdout、终态及 cleanup 结果作为 artifact 保留，
不上传私有 fixture/world，不赋予写入权限、生产凭据或 CodeQL/SARIF 能力。

正式 harness 的最小构建显式启用 `oasis7/ci_formal_required_features`，通过 native target 与 host build 的 optional dependency aliases 保留完整 required 根包共同构建时的依赖 feature 集合。该标记不参与业务 `cfg`，不自动启用，也不加入 production runtime、ordinary 或 default-feature snapshot 的原命令；依赖版本复用现有锁文件。验证比较完整构建与正式最小构建保留的完整依赖子树，并分别核对 host/build 与 target context 的实际 feature 组合，不能只比较根包上的 `test_tier_required` 字符串或把同包不同 context 合并后判断等价。

正式 producer 分为互斥的候选只读入口 `formal-test-build` 和可信 main 写缓存入口 `formal-test-build-trusted`，共享相同构建步骤，每轮恰好运行一个。可信入口仅用于默认 main 上的 push、schedule 或 manual-full；其他候选不能取得缓存写权限。矩阵和固定汇总显式依赖两个入口，选中时要求恰有一项 success 且另一项 skipped；任何失败、取消或未知状态均阻断，不能让 skipped 的隐式 needs 传播掩盖应运行的矩阵。

producer 核对完整清单后，为普通组及各物理分片输出独立 tar.gz，gzip level 1；保留 executable 权限，不传输整个 target。每包只带该消费者的 harness、生产子进程和动态库闭包，并报告原始字节、压缩字节和逐 binary 体积分布。consumer 核对原 run/source/tested identity、全局清单及所选闭包的 hash、profile 和实际 `--list`，不能因缺少不属于自己的二进制而补编。producer attempt 记录供审计；同 run、相同源码和构建身份的 consumer 重跑可复用先前 attempt 的产物。保持相同 checkout 布局，恢复编译时嵌入的 Cargo 程序路径，设置包工作目录、动态库路径及原二进制环境。普通集成测试需要的 Cargo binary 和 default-feature snapshot 不得被正式分包移除。

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

表中归属保持为逻辑 suite。物理分片可将 healthy-set 或 runtime-protocol 的完整慢用例放到较轻的分片；执行摘要同时记录逻辑 suite 与物理 shard，并对全集检查恰好一次归属。world-history 的单个 70 次连续操作场景保持完整，不能以切断历史链消除增长成本。上述非 ignored formal 命令未证明与普通环境等价，继续执行；snapshot_progress 与 legacy fields 回填仍使用原 default features。独立 ignored 验收按 `--exact --ignored --test-threads=1 --nocapture` 串行运行，矩阵 `fail-fast: false`、`max-parallel: 5`。world-history job 上限 45 分钟，其他分片初始保护上限 90 分钟；每次子命令绝对上限 40 分钟，均为防挂保护而非性能目标，后续按真实基线调整。

正式 workspace 缓存使用官方 restore/save 的独立步骤，限定正式 target 整体原样保存，不手工拼接 Cargo fingerprint。key 隔离平台/架构、Rust 与 WASM 工具链、Cargo lock/manifests/config、正式构建脚本和 features/profile，source SHA 用于精确条目，同一兼容前缀才可回退。候选只读；可信 producer 完成构建后测量原始 target，最多 8 GiB，超限正常跳过 save。未恢复 workspace 缓存时可使用既有可信依赖缓存，避免命中后重复下载依赖 target。缓存缺失仍走完整冷构建，损坏不得成为合格产物；节省量包含恢复与保存成本。

运行时优化遵循[单次 open 预热复用](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#formal-open-prewarm-reuse)和[健康 Replica 归档安装](../../p2p/design.md#formal-replica-staging-content-reuse)。新请求等待只读取精确请求的 hot result，使用轻量发布切面观察避免每 25ms 克隆完整 World；历史原请求仍保留认证冷查断言。正常成功路径达到目标 publication 后可先停止同一 runtime owner，再做不依赖活服务的材料统计与历史断言，减少测试自身继续产生的空 Tick。生产 Node 的 in-flight stop/join 语义不变，专门的 pending/in-flight 停止场景继续覆盖。阶段诊断记录 open/prewarm/restore、70/40 次操作、冷查及 stop 的单调耗时；不改变原请求绝对期限、签名、fsync 或原验收断言。

链节点入口由独立包 `oasis7_chain_runtime` 拥有，保留 `oasis7_chain_runtime` 可执行文件名、CLI、正式协议与持久化格式。`oasis7` 保留世界执行、状态和读投影；链节点以关闭默认 features 的显式依赖接入，并按需转发 Wasmtime、网络和测试 features。投影承诺树、规范编码和包含证明算法下沉到 `oasis7_client_api::formal_projection`，仅接收序列化投影和显式 audience；世界侧只枚举已授权 audience 并生成投影。通用端点协议、持久化和恢复仍由 `oasis7_distfs` / `oasis7_node` 拥有，链节点只负责世界适配与进程组合，不把这类代码重新放回游戏包。

CI 对依赖影响区分 library 输入与独立目标：包内 `src/bin/`、`tests/`、`examples/`、`benches/` 的变化选择该包自身的检查，不自动沿 library 反向依赖传播；若这些文件被其他目标以静态路径引用，仍按引用者传播。无法确认输入关系时保留保守覆盖。世界 library、状态序列化、正式读投影与共享构建输入变化继续选择 formal 验证，不把玩法代码整体视为与正式运行无关。新增、删除、重命名及测试 fixture 都使用 base/source/tested 的关系并集。

`formal-process` 使用 always 汇总：选中时构建和矩阵均须 success；未选中时矩阵须 skipped，明确输出 not_selected。required-gate 只新增此固定依赖。可信 base 尚无 formal_process 时，仅保守扩大：full 或相关 Rust 组选择全部分片；不修改旧计划使旧结果校验器接受未知组。新 base 通过原 Cargo 反向图映射选择 formal_process。schedule/manual-full 明确选全部重型分片，普通 full-standard 不重复执行重型链路；专用 package dispatch 保持原行为。

进程运行器使用单调绝对期限、20 秒存活心跳及独立进程所有权标识。Linux 扫描同 UID 环境标识并核对进程 starttime，清理包括脱离原进程组的自有后代；macOS 本地兼容使用独立进程组。退出码、超时、实际测试数、清理结果分别保存。摘要产物只含测试名和运行元数据；原输出通过 nocapture 进入 job 日志，不打包临时世界目录。

验证方法见[测试手册](../../../testing-manual.md#ci-formal-process-validation)。具体候选、失败、独立评审及 hosted 结果归当前 PR，未完成真实 runner 验证不得宣称产物复用已验收。
